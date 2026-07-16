use std::{env, path::PathBuf};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut a = env::args().skip(1);
    let kind = a.next().ok_or("kind")?;
    let socket = PathBuf::from(a.next().ok_or("socket")?);
    let state = PathBuf::from(a.next().ok_or("state/sema socket")?);
    if let Some(p) = socket.parent() {
        std::fs::create_dir_all(p)?
    }
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(socket)?;
    match kind.as_str() {
        "sema" => {
            let r = sema_storage::Runtime::open(&state).await?;
            loop {
                let (s, _) = listener.accept().await?;
                let r = r.clone();
                tokio::spawn(async move {
                    let _ = sema(s, r).await;
                });
            }
        }
        "schema" => {
            let r = schema_engine::Runtime::new(state);
            loop {
                let (s, _) = listener.accept().await?;
                let r = r.clone();
                tokio::spawn(async move {
                    let _ = schema(s, r).await;
                });
            }
        }
        "nomos" => {
            let r = nomos_engine::Runtime::new(state);
            loop {
                let (s, _) = listener.accept().await?;
                let r = r.clone();
                tokio::spawn(async move {
                    let _ = nomos(s, r).await;
                });
            }
        }
        "logos" => {
            let r = logos_engine::Runtime::new(state);
            loop {
                let (s, _) = listener.accept().await?;
                let r = r.clone();
                tokio::spawn(async move {
                    let _ = logos(s, r).await;
                });
            }
        }
        _ => Err("unknown kind".into()),
    }
}
async fn bytes(s: &mut UnixStream) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let n = s.read_u32_le().await? as usize;
    let mut b = vec![0; n];
    s.read_exact(&mut b).await?;
    Ok(b)
}
async fn write(s: &mut UnixStream, b: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    s.write_u32_le(b.len() as u32).await?;
    s.write_all(b).await?;
    Ok(())
}
async fn sema(
    mut s: UnixStream,
    r: sema_storage::Runtime,
) -> Result<(), Box<dyn std::error::Error>> {
    let q = rkyv::from_bytes::<signal_sema_storage::Request, rkyv::rancor::Error>(
        &bytes(&mut s).await?,
    )?;
    write(
        &mut s,
        &signal_sema_storage::Wire::encode_reply(&r.request(q).await?)?,
    )
    .await
}
async fn schema(
    mut s: UnixStream,
    r: schema_engine::Runtime,
) -> Result<(), Box<dyn std::error::Error>> {
    let q = rkyv::from_bytes::<signal_schema::Request, rkyv::rancor::Error>(&bytes(&mut s).await?)?;
    write(
        &mut s,
        &signal_schema::encode_reply(&r.request(q).await?).map_err(|e| format!("encode: {e}"))?,
    )
    .await
}
async fn nomos(
    mut s: UnixStream,
    r: nomos_engine::Runtime,
) -> Result<(), Box<dyn std::error::Error>> {
    let q = rkyv::from_bytes::<signal_nomos::Request, rkyv::rancor::Error>(&bytes(&mut s).await?)?;
    write(
        &mut s,
        &signal_nomos::encode_reply(&r.request(q).await?).map_err(|e| format!("encode: {e}"))?,
    )
    .await
}
async fn logos(
    mut s: UnixStream,
    r: logos_engine::Runtime,
) -> Result<(), Box<dyn std::error::Error>> {
    let q = rkyv::from_bytes::<signal_logos::Request, rkyv::rancor::Error>(&bytes(&mut s).await?)?;
    write(
        &mut s,
        &signal_logos::encode_reply(&r.request(q).await?).map_err(|e| format!("encode: {e}"))?,
    )
    .await
}
