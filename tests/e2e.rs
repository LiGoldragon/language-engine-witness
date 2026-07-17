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
fn reference_generated_rust(fixture: &str, component: &str) -> String {
    let schema = schema_language::SchemaEngine::default()
        .lower_source(
            fixture,
            schema_language::SchemaIdentity::new(component, "0.1.0"),
        )
        .expect("legacy reference schema lowering");
    schema_rust::RustEmitter::default()
        .emit_code_from_true_schema(&schema)
        .as_str()
        .to_owned()
}

/// The class of one reference item. The pipeline now emits the full enriched surface
/// — the module head (`// @generated` marker, scalar-alias prelude, NOTA import), the
/// schema declarations, and generation classes A (newtype ergonomics), B (interface
/// ergonomics), C-stub (the two route enums, the `short_header` const module, and the
/// `SignalOperationHeads` impl), and D (trace support). Every other reference block is
/// out of scope: the signal-frame codec bodies (the `route`/`encode`/`decode` fns,
/// `SignalFrameError`, `RequestPayload`/`LogVariant`, the `Frame` type aliases, and
/// `into_frame`) and the relocation-bound message planes (classes E/F). The selection
/// must reproduce exactly what the pipeline emits, no more.
///
/// The daemon assembles its output as the fixed module head followed by each lowered
/// item's projection plus a newline (see `logos-engine`), so every item is
/// blank-line-separated even where the legacy reference groups items without a blank
/// (class-A inherent/`From` pairs, the interleaved class-C region). The selection
/// therefore segments the reference at `#[rustfmt::skip]` item boundaries — merging
/// the scalar-alias prelude, whose four aliases the head renders as one block — rather
/// than at blank lines, and joins the landed items with a blank line each.
enum ReferenceBlock {
    GeneratedMarker,
    ScalarPrelude,
    NotaImport,
    ShortHeaderModule,
    Declaration(String),
    ImplItem,
    Other,
}

impl ReferenceBlock {
    /// The four scalar aliases the prelude fixes (`String`/`Integer`/`Boolean`/`Path`).
    const SCALARS: [&'static str; 4] = ["String", "Integer", "Boolean", "Path"];
    /// The class-C route enums and class-D trace types the pipeline emits under fixed
    /// names. The route enums are `<Root>Route`, recognized from the landed root; the
    /// trace types carry these fixed generated names, independent of the schema.
    const GENERATED_LANDED: [&'static str; 3] = ["SignalObjectName", "ObjectName", "TraceEvent"];
    /// Method and trait tokens that mark a signal-frame codec body or a relocation
    /// plane — the impl blocks the enriched stub does not generate, so an impl on an
    /// otherwise-landed type (`Input`/`Output`) that carries one of these is excluded.
    const OUT_OF_SCOPE_IMPL_MARKERS: [&'static str; 6] = [
        "encode_signal_frame",
        "into_frame",
        "into_reply_frame",
        "with_origin_route",
        "RequestPayload",
        "LogVariant",
    ];

    fn classify(item: &str) -> Self {
        if item.starts_with("// @generated") {
            return Self::GeneratedMarker;
        }
        if item.lines().any(|line| line.starts_with("pub use nota::")) {
            return Self::NotaImport;
        }
        if item
            .lines()
            .any(|line| line.starts_with("pub mod short_header"))
        {
            return Self::ShortHeaderModule;
        }
        // Any other module (`schema`, `signal`) is out of scope; guard before the
        // declared-name check so an inner `pub enum`/`pub struct` cannot leak a class.
        if Self::signature_line(item).is_some_and(|line| line.starts_with("pub mod ")) {
            return Self::Other;
        }
        if Self::is_scalar_prelude(item) {
            return Self::ScalarPrelude;
        }
        if let Some(name) = declared_name(item) {
            return Self::Declaration(name);
        }
        if Self::signature_line(item).is_some_and(|line| line.starts_with("impl")) {
            return Self::ImplItem;
        }
        Self::Other
    }

    /// Whether the pipeline emits this reference item, given the schema's landed
    /// declaration names.
    fn is_landed(item: &str, declarations: &[&str]) -> bool {
        match Self::classify(item) {
            Self::GeneratedMarker
            | Self::ScalarPrelude
            | Self::NotaImport
            | Self::ShortHeaderModule => true,
            Self::Declaration(name) => Self::name_is_landed(&name, declarations),
            Self::ImplItem => Self::impl_is_landed(item, declarations),
            Self::Other => false,
        }
    }

    /// A declared name is landed if it is a schema declaration, a `<Root>Route` route
    /// enum over a landed root, or one of the fixed generated trace types.
    fn name_is_landed(name: &str, declarations: &[&str]) -> bool {
        declarations.contains(&name)
            || name
                .strip_suffix("Route")
                .is_some_and(|root| declarations.contains(&root))
            || Self::GENERATED_LANDED.contains(&name)
    }

    /// An impl is landed if its subject type is landed and it carries no signal-frame
    /// codec or relocation-plane marker.
    fn impl_is_landed(item: &str, declarations: &[&str]) -> bool {
        if Self::OUT_OF_SCOPE_IMPL_MARKERS
            .iter()
            .any(|marker| item.contains(marker))
        {
            return false;
        }
        Self::impl_subject(item).is_some_and(|subject| Self::name_is_landed(&subject, declarations))
    }

    /// The subject type an impl is for: the type after `for` for a trait impl, else
    /// the type after `impl` past any generic parameters.
    fn impl_subject(item: &str) -> Option<String> {
        let signature = Self::signature_line(item)?;
        let after_impl = signature.strip_prefix("impl")?.trim_start();
        let after_generics = if after_impl.starts_with('<') {
            Self::skip_angle_group(after_impl)
        } else {
            after_impl
        }
        .trim_start();
        let subject = match after_generics.rfind(" for ") {
            Some(index) => &after_generics[index + " for ".len()..],
            None => after_generics,
        };
        let name: String = subject
            .trim_start()
            .chars()
            .take_while(|character| {
                character.is_alphanumeric() || *character == '_' || *character == ':'
            })
            .collect();
        (!name.is_empty()).then_some(name)
    }

    /// The remainder after a balanced `<…>` generic group at the start of `text`.
    fn skip_angle_group(text: &str) -> &str {
        let mut depth = 0usize;
        for (index, character) in text.char_indices() {
            match character {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        return &text[index + character.len_utf8()..];
                    }
                }
                _ => {}
            }
        }
        text
    }

    /// The first non-attribute line — the item's own signature (`impl …`, `pub … `).
    fn signature_line(item: &str) -> Option<&str> {
        item.lines()
            .map(str::trim_start)
            .find(|line| !line.starts_with('#'))
    }

    /// A block is the scalar prelude iff every line is an attribute or a
    /// `pub type <scalar> = …;` alias over the fixed scalar set — so the signal-frame
    /// alias block (`pub type Frame = …`) is excluded.
    fn is_scalar_prelude(item: &str) -> bool {
        let mut saw_alias = false;
        for line in item.lines() {
            if line.starts_with("#[") {
                continue;
            }
            let Some(rest) = line.strip_prefix("pub type ") else {
                return false;
            };
            let name: String = rest
                .chars()
                .take_while(|character| character.is_alphanumeric() || *character == '_')
                .collect();
            if !Self::SCALARS.contains(&name.as_str()) {
                return false;
            }
            saw_alias = true;
        }
        saw_alias
    }
}

/// The reference split into individual `#[rustfmt::skip]`-delimited items, with the
/// scalar-alias prelude kept whole (the pipeline renders its four aliases as one head
/// block, not four items). Everything else that shares a blank-line paragraph in the
/// legacy reference — the class-A pairs, the interleaved class-C region — is split so
/// each item can be selected on its own.
fn reference_items(reference: &str) -> Vec<String> {
    let mut items = Vec::new();
    for paragraph in reference.split("\n\n") {
        let block = paragraph.trim_matches('\n');
        if block.is_empty() {
            continue;
        }
        if ReferenceBlock::is_scalar_prelude(block) {
            items.push(block.to_owned());
            continue;
        }
        let mut current = String::new();
        for line in block.lines() {
            if line == "#[rustfmt::skip]" && !current.is_empty() {
                items.push(current.trim_end_matches('\n').to_owned());
                current.clear();
            }
            current.push_str(line);
            current.push('\n');
        }
        let tail = current.trim_end_matches('\n');
        if !tail.is_empty() {
            items.push(tail.to_owned());
        }
    }
    items
}

/// The expected pipeline output: the freshly generated legacy reference segmented at
/// item granularity and filtered to exactly the landed classes (module head, schema
/// declarations, generation classes A/B/C-stub/D), each item blank-line-separated to
/// match the daemon's own assembly.
fn select_landed_classes(reference: &str, declarations: &[&str]) -> String {
    let mut output = String::new();
    for item in reference_items(reference) {
        if ReferenceBlock::is_landed(&item, declarations) {
            output.push_str(&item);
            output.push_str("\n\n");
        }
    }
    output
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
        "[package]\nname=\"generated-spirit\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nrkyv={version=\"0.8\",features=[\"bytecheck\"]}\nsignal-frame={git=\"https://github.com/LiGoldragon/signal-frame.git\",rev=\"f46872e7e8edae5264c892443d415a273b231234\",default-features=false}\nnota={git=\"https://github.com/LiGoldragon/nota.git\",rev=\"7d0651a0e098efea5fe2578cb06d88e009d40ff0\",optional=true}\n[features]\ndefault=[]\nnota-text=[\"dep:nota\"]\n",
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
    let reference_rust =
        reference_generated_rust(include_str!("fixtures/spirit-min.schema"), "spirit:lib");
    assert_eq!(
        rust,
        select_landed_classes(
            &reference_rust,
            &[
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
            ],
        ),
        "the module head, the twelve declarations, and generation classes \
         A/B/C-stub/D preserve the byte-exact witness over the full landed surface"
    );

    let generated = temporary.path().join("generated");
    let reference = temporary.path().join("reference");
    write_crate(&generated, &rust);
    write_crate(&reference, &reference_rust);
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

    let second_reference =
        reference_generated_rust(include_str!("fixtures/second-min.schema"), "second:lib");
    assert_eq!(
        second_event.rust,
        select_landed_classes(
            &second_reference,
            &[
                "Weight", "Note", "Priority", "Parcel", "Ticket", "Input", "Output"
            ],
        ),
        "the restarted pipeline emits the second document's module head, \
         declarations, and generation classes A/B/C-stub/D byte-exact"
    );
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
