use std::{env, path::PathBuf, time::Duration};

use signal_sema_storage::{DocumentKind, FixtureScope, FrameMessage, Wire};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

struct FramedSocket {
    stream: UnixStream,
    sequence: u64,
}
impl FramedSocket {
    async fn connect(path: &PathBuf) -> Result<Self, AnyError> {
        let stream = UnixStream::connect(path).await?;
        let mut socket = Self {
            stream,
            sequence: 0,
        };
        socket
            .write_frame(&Wire::frame_current_handshake_request()?)
            .await?;
        let handshake = Wire::decode_frame(&socket.read_frame().await?)?;
        if !handshake.is_accepted_handshake() {
            return Err("daemon rejected shared frame protocol".into());
        }
        Ok(socket)
    }

    async fn accept(stream: UnixStream) -> Result<Self, AnyError> {
        let mut socket = Self {
            stream,
            sequence: 0,
        };
        let FrameMessage::HandshakeRequest(peer) = Wire::decode_frame(&socket.read_frame().await?)?
        else {
            return Err("first frame was not a protocol handshake".into());
        };
        socket
            .write_frame(&Wire::frame_handshake_reply(Wire::handshake_reply(peer))?)
            .await?;
        Ok(socket)
    }

    async fn request(&mut self, payload: Vec<u8>) -> Result<(), AnyError> {
        let frame = Wire::frame_request(payload, self.sequence)?;
        self.sequence += 1;
        self.write_frame(&frame).await
    }

    async fn read_request(
        &mut self,
    ) -> Result<(signal_frame::ExchangeIdentifier, Vec<u8>), AnyError> {
        let FrameMessage::Request { exchange, payload } =
            Wire::decode_frame(&self.read_frame().await?)?
        else {
            return Err("expected shared request frame".into());
        };
        Ok((exchange, payload))
    }

    async fn reply(
        &mut self,
        exchange: signal_frame::ExchangeIdentifier,
        payload: Vec<u8>,
    ) -> Result<(), AnyError> {
        self.write_frame(&Wire::frame_reply(exchange, payload)?)
            .await
    }

    async fn read_reply(&mut self) -> Result<Vec<u8>, AnyError> {
        let FrameMessage::Reply { payload, .. } = Wire::decode_frame(&self.read_frame().await?)?
        else {
            return Err("expected shared reply frame".into());
        };
        Ok(payload)
    }

    async fn write_frame(&mut self, bytes: &[u8]) -> Result<(), AnyError> {
        self.stream.write_all(bytes).await?;
        Ok(())
    }

    async fn read_frame(&mut self) -> Result<Vec<u8>, AnyError> {
        let length = self.stream.read_u32().await? as usize;
        let mut frame = Vec::with_capacity(length + 4);
        frame.extend_from_slice(&(length as u32).to_be_bytes());
        frame.resize(length + 4, 0);
        self.stream.read_exact(&mut frame[4..]).await?;
        Ok(frame)
    }
}

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
                    let _ = Server::serve_sema(stream, runtime).await;
                });
            }
        }
        "schema" => {
            let runtime = schema_engine::Runtime::new(sema_or_database);
            loop {
                let (stream, _) = listener.accept().await?;
                let runtime = runtime.clone();
                tokio::spawn(async move {
                    let _ = Server::serve_schema(stream, runtime).await;
                });
            }
        }
        "nomos" => {
            let runtime = nomos_engine::Runtime::new(sema_or_database);
            let schema = upstream.ok_or("schema upstream")?;
            let relay_runtime = runtime.clone();
            tokio::spawn(async move { Relay::schema(schema, relay_runtime).await });
            loop {
                let (stream, _) = listener.accept().await?;
                let runtime = runtime.clone();
                tokio::spawn(async move {
                    let _ = Server::serve_nomos(stream, runtime).await;
                });
            }
        }
        "logos" => {
            let runtime = logos_engine::Runtime::new(sema_or_database);
            let nomos = upstream.ok_or("nomos upstream")?;
            let relay_runtime = runtime.clone();
            tokio::spawn(async move { Relay::nomos(nomos, relay_runtime).await });
            loop {
                let (stream, _) = listener.accept().await?;
                let runtime = runtime.clone();
                tokio::spawn(async move {
                    let _ = Server::serve_logos(stream, runtime).await;
                });
            }
        }
        _ => return Err("unknown kind".into()),
    }
}

struct Relay;
impl Relay {
    async fn schema(path: PathBuf, runtime: nomos_engine::Runtime) {
        loop {
            if Self::schema_connection(&path, &runtime).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn schema_connection(
        path: &PathBuf,
        runtime: &nomos_engine::Runtime,
    ) -> Result<(), AnyError> {
        let mut socket = FramedSocket::connect(path).await?;
        socket
            .request(signal_schema::encode_request(
                &signal_schema::Request::Subscribe {
                    scope: FixtureScope(1),
                    kind: Some(DocumentKind::TypeSchema),
                },
            )?)
            .await?;
        let _: signal_schema::Reply = Self::decode(&socket.read_reply().await?)?;
        loop {
            if let signal_schema::Reply::Event(event) =
                Self::decode::<signal_schema::Reply>(&socket.read_reply().await?)?
            {
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

    async fn nomos(path: PathBuf, runtime: logos_engine::Runtime) {
        loop {
            if Self::nomos_connection(&path, &runtime).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn nomos_connection(
        path: &PathBuf,
        runtime: &logos_engine::Runtime,
    ) -> Result<(), AnyError> {
        let mut socket = FramedSocket::connect(path).await?;
        socket
            .request(signal_nomos::encode_request(
                &signal_nomos::Request::Subscribe {
                    scope: FixtureScope(1),
                },
            )?)
            .await?;
        let _: signal_nomos::Reply = Self::decode(&socket.read_reply().await?)?;
        loop {
            if let signal_nomos::Reply::Event(event) =
                Self::decode::<signal_nomos::Reply>(&socket.read_reply().await?)?
            {
                runtime
                    .request(signal_logos::Request::ProjectRust {
                        scope: event.logos.key.scope,
                        logos: event.logos.hash,
                    })
                    .await?;
            }
        }
    }

    fn decode<Value>(bytes: &[u8]) -> Result<Value, AnyError>
    where
        Value: rkyv::Archive,
        Value::Archived: for<'a> rkyv::bytecheck::CheckBytes<
                rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>,
            > + rkyv::Deserialize<Value, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
    {
        Ok(rkyv::from_bytes::<Value, rkyv::rancor::Error>(bytes)?)
    }
}

struct Server;
impl Server {
    async fn serve_sema(
        stream: UnixStream,
        runtime: sema_storage::Runtime,
    ) -> Result<(), AnyError> {
        let mut socket = FramedSocket::accept(stream).await?;
        let (exchange, payload) = socket.read_request().await?;
        let request = Relay::decode::<signal_sema_storage::Request>(&payload)?;
        let reply = Wire::encode_reply(&runtime.request(request).await?)?;
        socket.reply(exchange, reply).await
    }

    async fn serve_schema(
        stream: UnixStream,
        runtime: schema_engine::Runtime,
    ) -> Result<(), AnyError> {
        let mut socket = FramedSocket::accept(stream).await?;
        let (exchange, payload) = socket.read_request().await?;
        let request = Relay::decode::<signal_schema::Request>(&payload)?;
        let subscription = matches!(request, signal_schema::Request::Subscribe { .. });
        let mut events = runtime.subscribe();
        socket
            .reply(
                exchange,
                signal_schema::encode_reply(&runtime.request(request).await?)?,
            )
            .await?;
        if subscription {
            while let Ok(event) = events.recv().await {
                socket
                    .reply(
                        exchange,
                        signal_schema::encode_reply(&signal_schema::Reply::Event(event))?,
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn serve_nomos(
        stream: UnixStream,
        runtime: nomos_engine::Runtime,
    ) -> Result<(), AnyError> {
        let mut socket = FramedSocket::accept(stream).await?;
        let (exchange, payload) = socket.read_request().await?;
        let request = Relay::decode::<signal_nomos::Request>(&payload)?;
        let subscription = matches!(request, signal_nomos::Request::Subscribe { .. });
        let mut events = runtime.subscribe();
        socket
            .reply(
                exchange,
                signal_nomos::encode_reply(&runtime.request(request).await?)?,
            )
            .await?;
        if subscription {
            while let Ok(event) = events.recv().await {
                socket
                    .reply(
                        exchange,
                        signal_nomos::encode_reply(&signal_nomos::Reply::Event(event))?,
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn serve_logos(
        stream: UnixStream,
        runtime: logos_engine::Runtime,
    ) -> Result<(), AnyError> {
        let mut socket = FramedSocket::accept(stream).await?;
        let (exchange, payload) = socket.read_request().await?;
        let request = Relay::decode::<signal_logos::Request>(&payload)?;
        let subscription = matches!(request, signal_logos::Request::Subscribe { .. });
        let mut events = runtime.subscribe();
        socket
            .reply(
                exchange,
                signal_logos::encode_reply(&runtime.request(request).await?)?,
            )
            .await?;
        if subscription {
            while let Ok(event) = events.recv().await {
                socket
                    .reply(
                        exchange,
                        signal_logos::encode_reply(&signal_logos::Reply::Event(event))?,
                    )
                    .await?;
            }
        }
        Ok(())
    }
}
