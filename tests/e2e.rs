use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

use name_table::Identifier;
use signal_sema_storage::{
    BelongsRelation, DocumentKey, DocumentKind, FamilyDeclaration, FixtureScope, NameTableBytes,
    NexusActorDeclaration, NexusRoute, NexusRuntimeRoot, OpensRelation, SemaStorageRoot,
    SignalContractRoot, SlotIdentifier, StreamDeclaration,
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
fn spawn(kind: &str, socket: &Path, state: &Path, upstream: Option<&Path>) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engine-process"));
    command.args([kind, socket.to_str().unwrap(), state.to_str().unwrap()]);
    if let Some(upstream) = upstream {
        command.arg(upstream);
    }
    command
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
}
fn wait(path: &Path) {
    for _ in 0..100 {
        if path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("socket did not appear: {}", path.display());
}
async fn send<Q>(socket: &Path, request: &Q, encode: impl Fn(&Q) -> Vec<u8>) -> UnixStream {
    let mut stream = UnixStream::connect(socket).await.unwrap();
    let bytes = encode(request);
    stream.write_u32_le(bytes.len() as u32).await.unwrap();
    stream.write_all(&bytes).await.unwrap();
    stream
}
async fn exchange<Q, R>(socket: &Path, request: &Q, encode: impl Fn(&Q) -> Vec<u8>) -> R
where
    R: rkyv::Archive,
    R::Archived: for<'a> rkyv::bytecheck::CheckBytes<
            rkyv::rancor::Strategy<
                rkyv::validation::Validator<
                    rkyv::validation::archive::ArchiveValidator<'a>,
                    rkyv::validation::shared::SharedValidator,
                >,
                rkyv::rancor::Error,
            >,
        > + rkyv::Deserialize<R, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
{
    let mut stream = send(socket, request, encode).await;
    read_value(&mut stream).await
}
async fn read_value<R>(stream: &mut UnixStream) -> R
where
    R: rkyv::Archive,
    R::Archived: for<'a> rkyv::bytecheck::CheckBytes<
            rkyv::rancor::Strategy<
                rkyv::validation::Validator<
                    rkyv::validation::archive::ArchiveValidator<'a>,
                    rkyv::validation::shared::SharedValidator,
                >,
                rkyv::rancor::Error,
            >,
        > + rkyv::Deserialize<R, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
{
    let length = stream.read_u32_le().await.unwrap() as usize;
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await.unwrap();
    rkyv::from_bytes::<R, rkyv::rancor::Error>(&bytes).unwrap()
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
fn expected_generated_rust() -> String {
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
    ];
    let golden = include_str!("fixtures/spirit_generated.rs");
    let mut output = String::new();
    for paragraph in golden.split("\n\n") {
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
fn write_crate(path: &Path, rust: &str) {
    fs::create_dir_all(path.join("src")).unwrap();
    fs::create_dir_all(path.join("tests")).unwrap();
    fs::write(
        path.join("Cargo.toml"),
        "[package]\nname=\"generated-spirit\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nrkyv={version=\"0.8\",features=[\"bytecheck\"]}\n[features]\nnota-text=[]\n",
    )
    .unwrap();
    fs::write(
        path.join("src/lib.rs"),
        format!("{}{}", scalar_prelude(), rust),
    )
    .unwrap();
    fs::write(
        path.join("tests/behavior.rs"),
        r#"use generated_spirit::{Description,Entry,Kind,Magnitude,Query,RecordIdentifier,RecordSet,Summary,Topic,Topics};
fn archived<T:rkyv::Archive>(){}
fn public_fields(entry:&Entry,query:&Query){let _: &Topics=&entry.topics;let _: &Kind=&entry.kind;let _: &Description=&entry.description;let _: &Magnitude=&entry.magnitude;let _: &Topic=&query.topic;let _: &Kind=&query.kind;}
#[test]
fn complete_public_surface_and_behavior(){
 archived::<Topic>();archived::<Topics>();archived::<Description>();archived::<Summary>();archived::<RecordIdentifier>();archived::<Entry>();archived::<Query>();archived::<RecordSet>();archived::<Kind>();archived::<Magnitude>();
 assert_eq!(Kind::Decision,Kind::Decision);assert_ne!(Kind::Decision,Kind::Constraint);assert_eq!(Magnitude::High,Magnitude::High);
 let _=public_fields as fn(&Entry,&Query);
 for name in [std::any::type_name::<Topic>(),std::any::type_name::<Topics>(),std::any::type_name::<Description>(),std::any::type_name::<Summary>(),std::any::type_name::<RecordIdentifier>(),std::any::type_name::<Entry>(),std::any::type_name::<Query>(),std::any::type_name::<RecordSet>(),std::any::type_name::<Kind>(),std::any::type_name::<Magnitude>()]{assert!(name.starts_with("generated_spirit::"));}
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
    let mut processes = Processes(vec![spawn("sema", &sema, &database, None)]);
    wait(&sema);
    processes.0.push(spawn("schema", &schema, &sema, None));
    wait(&schema);
    processes
        .0
        .push(spawn("nomos", &nomos, &sema, Some(&schema)));
    wait(&nomos);
    processes
        .0
        .push(spawn("logos", &logos, &sema, Some(&nomos)));
    wait(&logos);

    let mut projected = send(
        &logos,
        &signal_logos::Request::Subscribe {
            scope: FixtureScope(1),
        },
        |request| signal_logos::encode_request(request).unwrap(),
    )
    .await;
    let subscribed: signal_logos::Reply = read_value(&mut projected).await;
    assert!(matches!(subscribed, signal_logos::Reply::Subscribed { .. }));
    tokio::time::sleep(Duration::from_millis(100)).await;

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
        read_value::<signal_logos::Reply>(&mut projected),
    )
    .await
    .expect("Schema → Nomos → Logos push completes without polling");
    let signal_logos::Reply::Event(event) = pushed else {
        panic!("expected projection event, got {pushed:?}");
    };
    let rust = event.rust;
    assert_eq!(
        rust,
        expected_generated_rust(),
        "all ten declarations preserve the free byte-exact witness"
    );

    let generated = temporary.path().join("generated");
    let reference = temporary.path().join("reference");
    write_crate(&generated, &rust);
    write_crate(&reference, &expected_generated_rust());
    for crate_path in [&generated, &reference] {
        let status = Command::new("cargo")
            .args(["test", "--quiet"])
            .current_dir(crate_path)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "generated and reference libraries share manifest, public surface, and behavior"
        );
    }

    processes.terminate();
    let _ = fs::remove_file(&sema);
    processes.0.push(spawn("sema", &sema, &database, None));
    wait(&sema);
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
        |request| signal_sema_storage::Wire::encode_request(request).unwrap(),
    )
    .await;
    assert!(matches!(
        recovered,
        signal_sema_storage::Reply::Document(Some(_))
    ));
}
