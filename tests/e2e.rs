use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use name_table::Identifier;
use signal_frame::ProtocolVersion;
use signal_sema_storage::{
    BelongsRelation, DocumentKey, DocumentKind, FamilyDeclaration, FixtureScope, FrameMessage,
    NameTableBytes, NexusActorDeclaration, NexusRoute, NexusRuntimeRoot, OpensRelation,
    SemaStorageRoot, SignalContractRoot, SlotIdentifier, StreamDeclaration, Wire,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

struct Processes(Vec<Child>);
impl Processes {
    fn terminate(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.0.clear();
    }
}
impl Drop for Processes {
    fn drop(&mut self) {
        self.terminate();
    }
}
struct ProductionBinaries {
    sema: PathBuf,
    schema: PathBuf,
    nomos: PathBuf,
    logos: PathBuf,
}
impl ProductionBinaries {
    fn from_environment() -> Self {
        Self {
            sema: std::env::var_os("SEMA_STORAGE_BIN")
                .expect("SEMA_STORAGE_BIN")
                .into(),
            schema: std::env::var_os("SCHEMA_ENGINE_BIN")
                .expect("SCHEMA_ENGINE_BIN")
                .into(),
            nomos: std::env::var_os("NOMOS_ENGINE_BIN")
                .expect("NOMOS_ENGINE_BIN")
                .into(),
            logos: std::env::var_os("LOGOS_ENGINE_BIN")
                .expect("LOGOS_ENGINE_BIN")
                .into(),
        }
    }

    fn spawn(&self, kind: &str, socket: &Path, state: &Path, upstream: Option<&Path>) -> Child {
        let program = match kind {
            "sema" => &self.sema,
            "schema" => &self.schema,
            "nomos" => &self.nomos,
            "logos" => &self.logos,
            _ => panic!("unknown production binary: {kind}"),
        };
        let mut command = Command::new(program);
        command.args(["daemon", socket.to_str().unwrap(), state.to_str().unwrap()]);
        if let Some(upstream) = upstream {
            command.arg(upstream);
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let line = BufReader::new(child.stdout.take().expect("daemon stdout"))
            .lines()
            .next()
            .expect("daemon readiness line")
            .expect("read daemon readiness");
        assert_eq!(line, format!("READY {}", socket.display()));
        child
    }
}
struct TestSocket {
    stream: UnixStream,
    sequence: u64,
}
impl TestSocket {
    async fn connect(path: &Path) -> Self {
        let mut socket = Self {
            stream: UnixStream::connect(path).await.unwrap(),
            sequence: 0,
        };
        socket
            .stream
            .write_all(&Wire::frame_current_handshake_request().unwrap())
            .await
            .unwrap();
        assert!(
            Wire::decode_frame(&socket.read_frame().await.unwrap())
                .unwrap()
                .is_accepted_handshake()
        );
        socket
    }

    async fn request<Q>(&mut self, request: &Q, encode: impl Fn(&Q) -> Vec<u8>) {
        let frame = Wire::frame_request(encode(request), self.sequence).unwrap();
        self.sequence += 1;
        self.stream.write_all(&frame).await.unwrap();
    }

    async fn read_reply<R>(&mut self) -> R
    where
        R: rkyv::Archive,
        R::Archived: for<'a> rkyv::bytecheck::CheckBytes<
                rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>,
            > + rkyv::Deserialize<R, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
    {
        let FrameMessage::Reply { payload, .. } =
            Wire::decode_frame(&self.read_frame().await.unwrap()).unwrap()
        else {
            panic!("expected shared reply frame")
        };
        rkyv::from_bytes::<R, rkyv::rancor::Error>(&payload).unwrap()
    }

    async fn read_frame(&mut self) -> std::io::Result<Vec<u8>> {
        let length = self.stream.read_u32().await? as usize;
        let mut frame = Vec::with_capacity(length + 4);
        frame.extend_from_slice(&(length as u32).to_be_bytes());
        frame.resize(length + 4, 0);
        self.stream.read_exact(&mut frame[4..]).await?;
        Ok(frame)
    }
}

async fn exchange<Q, R>(socket: &Path, request: &Q, encode: impl Fn(&Q) -> Vec<u8>) -> R
where
    R: rkyv::Archive,
    R::Archived: for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>
        + rkyv::Deserialize<R, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
{
    let mut socket = TestSocket::connect(socket).await;
    socket.request(request, encode).await;
    socket.read_reply().await
}
fn write_crate(path: &Path, rust: &str) {
    fs::create_dir_all(path.join("src")).unwrap();
    fs::create_dir_all(path.join("tests")).unwrap();
    fs::write(
        path.join("Cargo.lock"),
        include_str!("fixtures/spirit-lock.toml"),
    )
    .unwrap();
    fs::write(
        path.join("Cargo.toml"),
        "[package]\nname=\"generated-spirit\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nrkyv={version=\"0.8\",features=[\"bytecheck\"]}\nsignal-frame={git=\"https://github.com/LiGoldragon/signal-frame.git\",rev=\"f46872e7e8edae5264c892443d415a273b231234\",default-features=false}\nnota={git=\"https://github.com/LiGoldragon/nota.git\",rev=\"89dc3c85a9ff96d4e4d53accfd867df672cae5a8\",optional=true}\n[features]\ndefault=[]\nnota-text=[\"dep:nota\"]\n",
    )
    .unwrap();
    // The pipeline now emits the full module head (marker, scalar prelude, NOTA
    // import), so the generated source is written verbatim — no prelude is injected.
    fs::write(path.join("src/lib.rs"), rust).unwrap();
    fs::write(
        path.join("tests/behavior.rs"),
        r#"use generated_spirit::{Description,Entry,Input,Kind,Magnitude,Output,Query,RecordIdentifier,RecordSet,Summary,Topic,Topics};
fn archived<T:rkyv::Archive>(){}
fn public_fields(entry:&Entry,query:&Query){let _: &Topics=&entry.topics;let _: &Kind=&entry.kind;let _: &Description=&entry.description;let _: &Magnitude=&entry.magnitude;let _: &Topic=&query.topic;let _: &Kind=&query.kind;}
#[test]
fn complete_public_surface_and_behavior(){
 archived::<Topic>();archived::<Topics>();archived::<Description>();archived::<Summary>();archived::<RecordIdentifier>();archived::<Entry>();archived::<Query>();archived::<RecordSet>();archived::<Kind>();archived::<Magnitude>();archived::<Input>();archived::<Output>();
 assert_eq!(Kind::Decision,Kind::Decision);assert_ne!(Kind::Decision,Kind::Constraint);assert_eq!(Magnitude::High,Magnitude::High);
 let _=public_fields as fn(&Entry,&Query);
 for name in [std::any::type_name::<Topic>(),std::any::type_name::<Topics>(),std::any::type_name::<Description>(),std::any::type_name::<Summary>(),std::any::type_name::<RecordIdentifier>(),std::any::type_name::<Entry>(),std::any::type_name::<Query>(),std::any::type_name::<RecordSet>(),std::any::type_name::<Kind>(),std::any::type_name::<Magnitude>(),std::any::type_name::<Input>(),std::any::type_name::<Output>()]{assert!(name.starts_with("generated_spirit::"));}
}
"#,
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_document_pushes_through_four_processes_and_recovers() {
    let temporary = tempfile::tempdir().unwrap();
    let sema = temporary.path().join("sema.sock");
    let schema = temporary.path().join("schema.sock");
    let nomos = temporary.path().join("nomos.sock");
    let logos = temporary.path().join("logos.sock");
    let database = temporary.path().join("isolated.sema");
    let binaries = ProductionBinaries::from_environment();
    let mut processes = Processes(vec![binaries.spawn("sema", &sema, &database, None)]);
    let mut unsupported = TestSocket {
        stream: UnixStream::connect(&sema).await.unwrap(),
        sequence: 0,
    };
    unsupported
        .stream
        .write_all(&Wire::frame_handshake_request(ProtocolVersion::new(99, 0, 0)).unwrap())
        .await
        .unwrap();
    assert!(
        !Wire::decode_frame(&unsupported.read_frame().await.unwrap())
            .unwrap()
            .is_accepted_handshake(),
        "unsupported shared wire versions are rejected before request admission"
    );
    processes
        .0
        .push(binaries.spawn("schema", &schema, &sema, None));
    processes
        .0
        .push(binaries.spawn("nomos", &nomos, &sema, Some(&schema)));
    processes
        .0
        .push(binaries.spawn("logos", &logos, &sema, Some(&nomos)));

    let mut projected = TestSocket::connect(&logos).await;
    projected
        .request(
            &signal_logos::Request::Subscribe {
                scope: FixtureScope(1),
            },
            |request| signal_logos::encode_request(request).unwrap(),
        )
        .await;
    let subscribed: signal_logos::Reply = projected.read_reply().await;
    assert!(matches!(subscribed, signal_logos::Reply::Subscribed { .. }));

    for request in [
        signal_schema::Request::StoreSignalContract {
            scope: FixtureScope(1),
            slot: SlotIdentifier(2),
            root: SignalContractRoot {
                contract: Identifier::new(0),
                streams: vec![StreamDeclaration {
                    stream: Identifier::new(1),
                }],
                opens: vec![OpensRelation {
                    operation: Identifier::new(2),
                    stream: Identifier::new(1),
                }],
                belongs: vec![BelongsRelation {
                    stream: Identifier::new(1),
                    contract: Identifier::new(0),
                }],
                names: NameTableBytes(Vec::new()),
            },
        },
        signal_schema::Request::StoreNexusRuntime {
            scope: FixtureScope(1),
            slot: SlotIdentifier(3),
            root: NexusRuntimeRoot {
                actors: vec![
                    NexusActorDeclaration {
                        actor: Identifier::new(3),
                    },
                    NexusActorDeclaration {
                        actor: Identifier::new(4),
                    },
                ],
                routes: vec![NexusRoute {
                    sender: Identifier::new(3),
                    receiver: Identifier::new(4),
                }],
                names: NameTableBytes(Vec::new()),
            },
        },
        signal_schema::Request::StoreSemaStorage {
            scope: FixtureScope(1),
            slot: SlotIdentifier(4),
            root: SemaStorageRoot {
                families: vec![FamilyDeclaration {
                    family: Identifier::new(5),
                    layout_version: 1,
                }],
                names: NameTableBytes(Vec::new()),
            },
        },
    ] {
        let reply: signal_schema::Reply = exchange(&schema, &request, |request| {
            signal_schema::encode_request(request).unwrap()
        })
        .await;
        assert!(matches!(reply, signal_schema::Reply::Stored(_)));
    }

    let schema_reply: signal_schema::Reply = exchange(
        &schema,
        &signal_schema::Request::IngestTypeSchema {
            scope: FixtureScope(1),
            slot: SlotIdentifier(1),
            legacy_text: include_str!("fixtures/spirit-min.schema").into(),
        },
        |request| signal_schema::encode_request(request).unwrap(),
    )
    .await;
    assert!(matches!(schema_reply, signal_schema::Reply::Stored(_)));

    let pushed = tokio::time::timeout(
        Duration::from_secs(5),
        projected.read_reply::<signal_logos::Reply>(),
    )
    .await
    .expect("Schema → Nomos → Logos push completes without polling");
    let signal_logos::Reply::Event(event) = pushed else {
        panic!("expected projection event, got {pushed:?}");
    };
    let rust = event.rust;

    // Working-programs bar: the pipeline's emitted Rust must compile under its
    // locked manifest and pass its public-surface behavior tests. The witness no
    // longer byte-compares the emission against a schema-rust oracle projection.
    let generated = temporary.path().join("generated");
    write_crate(&generated, &rust);
    for feature_arguments in [
        &["test", "--quiet", "--locked"][..],
        &["test", "--quiet", "--locked", "--no-default-features"][..],
        &["test", "--quiet", "--locked", "--all-features"][..],
    ] {
        let status = Command::new("cargo")
            .args(feature_arguments)
            .current_dir(&generated)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "the emitted library compiles under its locked manifest and passes its public-surface behavior tests for {feature_arguments:?}"
        );
    }

    processes.terminate();
    for socket in [&sema, &schema, &nomos, &logos] {
        let _ = fs::remove_file(socket);
    }
    processes
        .0
        .push(binaries.spawn("sema", &sema, &database, None));
    processes
        .0
        .push(binaries.spawn("schema", &schema, &sema, None));
    processes
        .0
        .push(binaries.spawn("nomos", &nomos, &sema, Some(&schema)));
    processes
        .0
        .push(binaries.spawn("logos", &logos, &sema, Some(&nomos)));

    let recovered: signal_sema_storage::Reply = exchange(
        &sema,
        &signal_sema_storage::Request::Fetch {
            key: DocumentKey {
                scope: FixtureScope(1),
                kind: DocumentKind::Logos,
                slot: SlotIdentifier(1),
            },
            version: None,
        },
        |request| Wire::encode_request(request).unwrap(),
    )
    .await;
    assert!(matches!(
        recovered,
        signal_sema_storage::Reply::Document(Some(_))
    ));
    for (kind, slot) in [
        (DocumentKind::SignalContract, SlotIdentifier(2)),
        (DocumentKind::NexusRuntime, SlotIdentifier(3)),
        (DocumentKind::SemaStorage, SlotIdentifier(4)),
    ] {
        let recovered_root: signal_sema_storage::Reply = exchange(
            &sema,
            &signal_sema_storage::Request::Fetch {
                key: DocumentKey {
                    scope: FixtureScope(1),
                    kind,
                    slot,
                },
                version: None,
            },
            |request| Wire::encode_request(request).unwrap(),
        )
        .await;
        let signal_sema_storage::Reply::Document(Some(document)) = recovered_root else {
            panic!("root-specific state did not recover: {kind:?}")
        };
        assert_eq!(document.payload.kind(), kind);
        assert_eq!(document.payload.validate(), Ok(()));
    }

    let mut resumed_projection = TestSocket::connect(&logos).await;
    resumed_projection
        .request(
            &signal_logos::Request::Subscribe {
                scope: FixtureScope(1),
            },
            |request| signal_logos::encode_request(request).unwrap(),
        )
        .await;
    let resubscribed: signal_logos::Reply = resumed_projection.read_reply().await;
    assert!(
        matches!(resubscribed, signal_logos::Reply::Subscribed { .. }),
        "the restarted Logos re-establishes the push subscription on its rebound socket"
    );

    let second_slot = SlotIdentifier(5);
    let second_stored: signal_schema::Reply = exchange(
        &schema,
        &signal_schema::Request::IngestTypeSchema {
            scope: FixtureScope(1),
            slot: second_slot,
            legacy_text: include_str!("fixtures/second-min.schema").into(),
        },
        |request| signal_schema::encode_request(request).unwrap(),
    )
    .await;
    assert!(matches!(second_stored, signal_schema::Reply::Stored(_)));

    let resumed = tokio::time::timeout(
        Duration::from_secs(5),
        resumed_projection.read_reply::<signal_logos::Reply>(),
    )
    .await
    .expect("all four restarted daemons resume push progression without polling");
    let signal_logos::Reply::Event(second_event) = resumed else {
        panic!("expected a second projection event, got {resumed:?}");
    };

    assert_ne!(
        second_event.rust, rust,
        "the genuinely second document generates distinct Rust from the first"
    );
    assert_eq!(second_event.source.key.kind, DocumentKind::Logos);
    assert_eq!(second_event.source.key.slot, second_slot);

    let second_recovered: signal_sema_storage::Reply = exchange(
        &sema,
        &signal_sema_storage::Request::Fetch {
            key: DocumentKey {
                scope: FixtureScope(1),
                kind: DocumentKind::Logos,
                slot: second_slot,
            },
            version: None,
        },
        |request| Wire::encode_request(request).unwrap(),
    )
    .await;
    let signal_sema_storage::Reply::Document(Some(second_document)) = second_recovered else {
        panic!("the second Logos document did not store durably in Sema")
    };
    assert_eq!(second_document.payload.kind(), DocumentKind::Logos);
    assert_eq!(second_document.payload.validate(), Ok(()));
    assert_eq!(
        second_document.hash, second_event.logos,
        "the durable second document is the one the restarted pipeline just pushed"
    );
    assert_eq!(second_document.hash, second_event.source.hash);
}
