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
fn declared_name(block: &str) -> Option<String> {
    for line in block.lines() {
        for head in ["pub struct ", "pub enum "] {
            if let Some(rest) = line.strip_prefix(head) {
                let name: String = rest
                    .chars()
                    .take_while(|character| character.is_alphanumeric() || *character == '_')
                    .collect();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }
    None
}
fn reference_generated_rust() -> String {
    let schema = schema_language::SchemaEngine::default()
        .lower_source(
            include_str!("fixtures/spirit-min.schema"),
            schema_language::SchemaIdentity::new("spirit:lib", "0.1.0"),
        )
        .expect("legacy reference schema lowering");
    schema_rust::RustEmitter::default()
        .emit_code_from_true_schema(&schema)
        .as_str()
        .to_owned()
}

fn expected_generated_rust(reference: &str) -> String {
    let names = [
        "Topic",
        "Topics",
        "Description",
        "Summary",
        "RecordIdentifier",
        "Entry",
        "Query",
        "RecordSet",
        "Kind",
        "Magnitude",
        "Input",
        "Output",
    ];
    let mut output = String::new();
    for paragraph in reference.split("\n\n") {
        let block = paragraph.trim_matches('\n');
        if declared_name(block).is_some_and(|name| names.contains(&name.as_str())) {
            output.push_str(block);
            output.push_str("\n\n");
        }
    }
    output
}
fn scalar_prelude() -> &'static str {
    "pub type String = std::string::String;\npub type Integer = u64;\n"
}
fn write_crate(path: &Path, rust: &str, scalar_prelude_required: bool) {
    fs::create_dir_all(path.join("src")).unwrap();
    fs::create_dir_all(path.join("tests")).unwrap();
    fs::write(
        path.join("Cargo.lock"),
        include_str!("fixtures/spirit-lock.toml"),
    )
    .unwrap();
    fs::write(
        path.join("Cargo.toml"),
        "[package]\nname=\"generated-spirit\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nrkyv={version=\"0.8\",features=[\"bytecheck\"]}\nsignal-frame={git=\"https://github.com/LiGoldragon/signal-frame.git\",rev=\"f46872e7e8edae5264c892443d415a273b231234\",default-features=false}\nnota={git=\"https://github.com/LiGoldragon/nota.git\",rev=\"7d0651a0e098efea5fe2578cb06d88e009d40ff0\",optional=true}\n[features]\ndefault=[]\nnota-text=[\"dep:nota\"]\n",
    )
    .unwrap();
    fs::write(
        path.join("src/lib.rs"),
        if scalar_prelude_required {
            format!("{}{}", scalar_prelude(), rust)
        } else {
            rust.to_owned()
        },
    )
    .unwrap();
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
    let reference_rust = reference_generated_rust();
    assert_eq!(
        rust,
        expected_generated_rust(&reference_rust),
        "all ten declarations preserve the free byte-exact witness"
    );

    let generated = temporary.path().join("generated");
    let reference = temporary.path().join("reference");
    write_crate(&generated, &rust, true);
    write_crate(&reference, &reference_rust, false);
    for crate_path in [&generated, &reference] {
        for feature_arguments in [
            &["test", "--quiet", "--locked"][..],
            &["test", "--quiet", "--locked", "--no-default-features"][..],
            &["test", "--quiet", "--locked", "--all-features"][..],
        ] {
            let status = Command::new("cargo")
                .args(feature_arguments)
                .current_dir(crate_path)
                .status()
                .unwrap();
            assert!(
                status.success(),
                "generated and reference libraries share their locked manifest, public surface, and behavior for {feature_arguments:?}"
            );
        }
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
    let _: signal_logos::Reply = resumed_projection.read_reply().await;
    let stored: signal_schema::Reply = exchange(
        &schema,
        &signal_schema::Request::IngestTypeSchema {
            scope: FixtureScope(1),
            slot: SlotIdentifier(1),
            legacy_text: include_str!("fixtures/spirit-min.schema").into(),
        },
        |request| signal_schema::encode_request(request).unwrap(),
    )
    .await;
    assert!(matches!(stored, signal_schema::Reply::Stored(summary) if summary.version.0 == 2));
    let resumed = tokio::time::timeout(
        Duration::from_secs(5),
        resumed_projection.read_reply::<signal_logos::Reply>(),
    )
    .await
    .expect("all four restarted daemons resume push progression");
    assert!(matches!(resumed, signal_logos::Reply::Event(_)));
}
