use signal_sema_storage::{
    DeclarationRoot, DocumentKind, FixtureScope, NameTableBytes, SlotIdentifier,
};
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
struct Processes(Vec<Child>);
impl Drop for Processes {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
fn spawn(kind: &str, socket: &Path, state: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_engine-process"))
        .args([kind, socket.to_str().unwrap(), state.to_str().unwrap()])
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
        thread::sleep(Duration::from_millis(25))
    }
    panic!("socket did not appear: {}", path.display())
}
async fn exchange<Q, R>(socket: &Path, q: &Q, encode: impl Fn(&Q) -> Vec<u8>) -> R
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
    let mut s = UnixStream::connect(socket).await.unwrap();
    let b = encode(q);
    s.write_u32_le(b.len() as u32).await.unwrap();
    s.write_all(&b).await.unwrap();
    let n = s.read_u32_le().await.unwrap() as usize;
    let mut b = vec![0; n];
    s.read_exact(&mut b).await.unwrap();
    rkyv::from_bytes::<R, rkyv::rancor::Error>(&b).unwrap()
}
fn declared_name(block: &str) -> Option<String> {
    for line in block.lines() {
        for head in ["pub struct ", "pub enum "] {
            if let Some(rest) = line.strip_prefix(head) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }
    None
}
fn expected_migrated_rust() -> String {
    let names = [
        "Topic",
        "Description",
        "Summary",
        "RecordIdentifier",
        "Entry",
        "Query",
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
fn support() -> &'static str {
    "pub type String = std::string::String;\npub type Integer = u64;\n#[derive(rkyv::Archive,rkyv::Serialize,rkyv::Deserialize,Clone,Debug,PartialEq,Eq)] pub struct Topics(pub Vec<Topic>);\n#[derive(rkyv::Archive,rkyv::Serialize,rkyv::Deserialize,Clone,Copy,Debug,PartialEq,Eq)] pub enum Kind { Decision, Principle, Correction, Clarification, Constraint }\n#[derive(rkyv::Archive,rkyv::Serialize,rkyv::Deserialize,Clone,Copy,Debug,PartialEq,Eq)] pub enum Magnitude { Minimum, VeryLow, Low, Medium, High, VeryHigh, Maximum }\n"
}
fn write_crate(path: &Path, rust: &str) {
    fs::create_dir_all(path.join("src")).unwrap();
    fs::create_dir_all(path.join("tests")).unwrap();
    fs::write(path.join("Cargo.toml"),"[package]\nname=\"generated-spirit-subset\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nrkyv={version=\"0.8\",features=[\"bytecheck\"]}\n[features]\nnota-text=[]\n").unwrap();
    fs::write(path.join("src/lib.rs"), format!("{}{}", support(), rust)).unwrap();
    fs::write(path.join("tests/behavior.rs"),"use generated_spirit_subset::{Description,Entry,Query,RecordIdentifier,Summary,Topic};\nfn archived<T:rkyv::Archive>(){}\n#[test] fn shared_public_surface_and_archive_behavior(){archived::<Topic>();archived::<Description>();archived::<Summary>();archived::<RecordIdentifier>();archived::<Entry>();archived::<Query>();for name in [std::any::type_name::<Topic>(),std::any::type_name::<Description>(),std::any::type_name::<Summary>(),std::any::type_name::<RecordIdentifier>(),std::any::type_name::<Entry>(),std::any::type_name::<Query>()]{assert!(name.starts_with(\"generated_spirit_subset::\"));}}\n").unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_document_crosses_four_processes_and_compiles_independently() {
    let tmp = tempfile::tempdir().unwrap();
    let sema = tmp.path().join("sema.sock");
    let schema = tmp.path().join("schema.sock");
    let nomos = tmp.path().join("nomos.sock");
    let logos = tmp.path().join("logos.sock");
    let db = tmp.path().join("isolated.sema");
    let mut children = vec![spawn("sema", &sema, &db)];
    wait(&sema);
    children.push(spawn("schema", &schema, &sema));
    children.push(spawn("nomos", &nomos, &sema));
    children.push(spawn("logos", &logos, &sema));
    wait(&schema);
    wait(&nomos);
    wait(&logos);
    let _guard = Processes(children);
    let schema_reply: signal_schema::Reply = exchange(
        &schema,
        &signal_schema::Request::IngestTypeSchema {
            scope: FixtureScope(1),
            slot: SlotIdentifier(1),
            legacy_text: include_str!("fixtures/spirit-min.schema").into(),
        },
        |q| signal_schema::encode_request(q).unwrap(),
    )
    .await;
    let signal_schema::Reply::Stored(schema_stored) = schema_reply else {
        panic!("{schema_reply:?}")
    };
    for (kind, slot) in [
        (DocumentKind::SignalContract, 2),
        (DocumentKind::NexusRuntime, 3),
        (DocumentKind::SemaStorage, 4),
    ] {
        let reply: signal_schema::Reply = exchange(
            &schema,
            &signal_schema::Request::StoreDocumentRoot {
                scope: FixtureScope(1),
                slot: SlotIdentifier(slot),
                root: DeclarationRoot {
                    kind,
                    declarations: vec![],
                    names: NameTableBytes(vec![]),
                },
            },
            |q| signal_schema::encode_request(q).unwrap(),
        )
        .await;
        match reply {
            signal_schema::Reply::Stored(summary) => assert_eq!(summary.key.kind, kind),
            other => panic!("{other:?}"),
        }
    }
    let nomos_reply: signal_nomos::Reply = exchange(
        &nomos,
        &signal_nomos::Request::Transform {
            scope: FixtureScope(1),
            schema: schema_stored.hash,
            output_slot: SlotIdentifier(1),
        },
        |q| signal_nomos::encode_request(q).unwrap(),
    )
    .await;
    let signal_nomos::Reply::Transformed(logos_stored) = nomos_reply else {
        panic!("{nomos_reply:?}")
    };
    let logos_reply: signal_logos::Reply = exchange(
        &logos,
        &signal_logos::Request::ProjectRust {
            scope: FixtureScope(1),
            logos: logos_stored.hash,
        },
        |q| signal_logos::encode_request(q).unwrap(),
    )
    .await;
    let signal_logos::Reply::RustProjected { rust, .. } = logos_reply else {
        panic!("{logos_reply:?}")
    };
    for name in [
        "Topic",
        "Description",
        "Summary",
        "RecordIdentifier",
        "Entry",
        "Query",
    ] {
        assert!(
            rust.contains(&format!("pub struct {name}")),
            "missing {name}"
        )
    }
    assert_eq!(
        rust,
        expected_migrated_rust(),
        "the already-free six-item byte-exact witness stays green"
    );
    let generated = tmp.path().join("generated");
    let reference = tmp.path().join("reference");
    write_crate(&generated, &rust);
    write_crate(&reference, &expected_migrated_rust());
    for crate_path in [&generated, &reference] {
        let status = Command::new("cargo")
            .args(["test", "--quiet"])
            .current_dir(crate_path)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "independent crate did not compile and pass the shared public-surface test"
        );
    }
    let state = fs::metadata(&db).unwrap();
    assert!(
        state.len() > 0,
        "central Sema state is durable and isolated"
    );
}
