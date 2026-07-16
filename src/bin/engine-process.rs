use std::{env, path::PathBuf, time::Duration};

use signal_sema_storage::{DocumentKind, FixtureScope};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), AnyError> {
    let mut arguments = env::args().skip(1);
    let kind = arguments.next().ok_or("kind")?;
    let socket = PathBuf::from(arguments.next().ok_or("socket")?);
    let sema_or_database = PathBuf::from(arguments.next().ok_or("state/sema socket")?);
    let upstream = arguments.next().map(PathBuf::from);
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(socket)?;
    match kind.as_str() {
        "sema" => {
            let runtime = sema_storage::Runtime::open(&sema_or_database).await?;
            loop {
                let (stream, _) = listener.accept().await?;
                let runtime = runtime.clone();
                tokio::spawn(async move {
                    let _ = serve_sema(stream, runtime).await;
                });
            }
        }
        "schema" => {
            let runtime = schema_engine::Runtime::new(sema_or_database);
            loop {
                let (stream, _) = listener.accept().await?;
                let runtime = runtime.clone();
                tokio::spawn(async move {
                    let _ = serve_schema(stream, runtime).await;
                });
            }
        }
        "nomos" => {
            let runtime = nomos_engine::Runtime::new(sema_or_database);
            let schema = upstream.ok_or("schema upstream")?;
            let relay_runtime = runtime.clone();
            tokio::spawn(async move { relay_schema(schema, relay_runtime).await });
            loop {
                let (stream, _) = listener.accept().await?;
                let runtime = runtime.clone();
                tokio::spawn(async move {
                    let _ = serve_nomos(stream, runtime).await;
                });
            }
        }
        "logos" => {
            let runtime = logos_engine::Runtime::new(sema_or_database);
            let nomos = upstream.ok_or("nomos upstream")?;
            let relay_runtime = runtime.clone();
            tokio::spawn(async move { relay_nomos(nomos, relay_runtime).await });
            loop {
                let (stream, _) = listener.accept().await?;
                let runtime = runtime.clone();
                tokio::spawn(async move {
                    let _ = serve_logos(stream, runtime).await;
                });
            }
        }
        _ => return Err("unknown kind".into()),
    }
}

async fn relay_schema(path: PathBuf, runtime: nomos_engine::Runtime) {
    loop {
        if relay_schema_connection(&path, &runtime).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
async fn relay_schema_connection(
    path: &PathBuf,
    runtime: &nomos_engine::Runtime,
) -> Result<(), AnyError> {
    let mut stream = UnixStream::connect(path).await?;
    write_value(
        &mut stream,
        &signal_schema::encode_request(&signal_schema::Request::Subscribe {
            scope: FixtureScope(1),
            kind: Some(DocumentKind::TypeSchema),
        })
        .map_err(|error| error.to_string())?,
    )
    .await?;
    let _: signal_schema::Reply = read_value(&mut stream).await?;
    loop {
        if let signal_schema::Reply::Event(event) = read_value(&mut stream).await? {
            runtime
                .request(signal_nomos::Request::Transform {
                    scope: event.document.key.scope,
                    schema: event.document.hash,
                    output_slot: event.document.key.slot,
                })
                .await?;
        }
    }
}

async fn relay_nomos(path: PathBuf, runtime: logos_engine::Runtime) {
    loop {
        if relay_nomos_connection(&path, &runtime).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
async fn relay_nomos_connection(
    path: &PathBuf,
    runtime: &logos_engine::Runtime,
) -> Result<(), AnyError> {
    let mut stream = UnixStream::connect(path).await?;
    write_value(
        &mut stream,
        &signal_nomos::encode_request(&signal_nomos::Request::Subscribe {
            scope: FixtureScope(1),
        })
        .map_err(|error| error.to_string())?,
    )
    .await?;
    let _: signal_nomos::Reply = read_value(&mut stream).await?;
    loop {
        if let signal_nomos::Reply::Event(event) = read_value(&mut stream).await? {
            runtime
                .request(signal_logos::Request::ProjectRust {
                    scope: event.logos.key.scope,
                    logos: event.logos.hash,
                })
                .await?;
        }
    }
}

async fn serve_sema(
    mut stream: UnixStream,
    runtime: sema_storage::Runtime,
) -> Result<(), AnyError> {
    let request: signal_sema_storage::Request = read_value(&mut stream).await?;
    let bytes = signal_sema_storage::Wire::encode_reply(&runtime.request(request).await?)?;
    write_value(&mut stream, &bytes).await
}
async fn serve_schema(
    mut stream: UnixStream,
    runtime: schema_engine::Runtime,
) -> Result<(), AnyError> {
    let request: signal_schema::Request = read_value(&mut stream).await?;
    let subscription = matches!(request, signal_schema::Request::Subscribe { .. });
    let mut events = runtime.subscribe();
    let bytes = signal_schema::encode_reply(&runtime.request(request).await?)
        .map_err(|error| error.to_string())?;
    write_value(&mut stream, &bytes).await?;
    if subscription {
        while let Ok(event) = events.recv().await {
            let bytes = signal_schema::encode_reply(&signal_schema::Reply::Event(event))
                .map_err(|error| error.to_string())?;
            write_value(&mut stream, &bytes).await?;
        }
    }
    Ok(())
}
async fn serve_nomos(
    mut stream: UnixStream,
    runtime: nomos_engine::Runtime,
) -> Result<(), AnyError> {
    let request: signal_nomos::Request = read_value(&mut stream).await?;
    let subscription = matches!(request, signal_nomos::Request::Subscribe { .. });
    let mut events = runtime.subscribe();
    let bytes = signal_nomos::encode_reply(&runtime.request(request).await?)
        .map_err(|error| error.to_string())?;
    write_value(&mut stream, &bytes).await?;
    if subscription {
        while let Ok(event) = events.recv().await {
            let bytes = signal_nomos::encode_reply(&signal_nomos::Reply::Event(event))
                .map_err(|error| error.to_string())?;
            write_value(&mut stream, &bytes).await?;
        }
    }
    Ok(())
}
async fn serve_logos(
    mut stream: UnixStream,
    runtime: logos_engine::Runtime,
) -> Result<(), AnyError> {
    let request: signal_logos::Request = read_value(&mut stream).await?;
    let subscription = matches!(request, signal_logos::Request::Subscribe { .. });
    let mut events = runtime.subscribe();
    let bytes = signal_logos::encode_reply(&runtime.request(request).await?)
        .map_err(|error| error.to_string())?;
    write_value(&mut stream, &bytes).await?;
    if subscription {
        while let Ok(event) = events.recv().await {
            let bytes = signal_logos::encode_reply(&signal_logos::Reply::Event(event))
                .map_err(|error| error.to_string())?;
            write_value(&mut stream, &bytes).await?;
        }
    }
    Ok(())
}

async fn write_value(stream: &mut UnixStream, bytes: &[u8]) -> Result<(), AnyError> {
    stream.write_u32_le(bytes.len() as u32).await?;
    stream.write_all(bytes).await?;
    Ok(())
}
async fn read_value<T>(stream: &mut UnixStream) -> Result<T, AnyError>
where
    T: rkyv::Archive,
    T::Archived: for<'a> rkyv::bytecheck::CheckBytes<
            rkyv::rancor::Strategy<
                rkyv::validation::Validator<
                    rkyv::validation::archive::ArchiveValidator<'a>,
                    rkyv::validation::shared::SharedValidator,
                >,
                rkyv::rancor::Error,
            >,
        > + rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
{
    let length = stream.read_u32_le().await? as usize;
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok(rkyv::from_bytes::<T, rkyv::rancor::Error>(&bytes)?)
}
