use std::collections::BTreeMap;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use slice_core_logos::{LogosLanguage, LogosLanguageTypeIds, LogosLanguageWords};
use slice_core_nomos::{
    MetaType, NomosFileManifest, NomosLoadError, NomosManifestFile, NomosManifestLoadError,
    NomosModulePath, NomosSourcePath, TemplateFutureOutput, TemplateLandingShape, TemplateLanguage,
    TextualNomos, TextualNomosMetaType, TextualNomosTypeIds, TextualNomosWords,
};
use slice_name_table::{LocalEncodedId, Name, OperationKey};
use slice_sema_translator::{AUTHORITY_ROUTE, principal_for_unix_uid};
use slice_signal_frame::{
    ExchangeIdentifier, ExchangeLane, LaneSequence, Reply, Request, SessionEpoch,
    StreamingFrameBody, SubReply,
};
use slice_signal_sema_translator::{
    AuthorityCapability, AuthorityOperation, AuthorityReply, AuthorityRequest, AuthorityRole,
    AuthorizationClaim, CommittedReceipt, DatabaseMarker, NoWriteFailure, PostCommitEvent,
    PrincipalId, ReadOperation, Rename, RenameCommitReceipt, SealCommitReceipt, TranslatorFrame,
    VocabularyEncodedId, VocabularyRoot, WritePrecondition,
};
use slice_structural_codec::{EncodedNameResolver, LandingShape};

const SOURCE: &str = r#"{1}
[]
[]
{
WireAttributes.Named {
()
[]
}
WireNewtype.Structural.Newtype {
(name.Name wrapped.Type)
Public Invoke.WireAttributes Realize.name Private Realize.wrapped
}
}
{}
{}"#;
const ATTRIBUTES_SOURCE: &str = r#"{1}
[]
[]
{
WireAttributes.Named {
()
[]
}
}
{}
{}"#;
const NEWTYPE_SOURCE: &str = r#"{1}
[]
[]
{
WireNewtype.Structural.Newtype {
(name.Name wrapped.Type)
Public Invoke.WireAttributes Realize.name Private Realize.wrapped
}
}
{}
{}"#;
const EXTERNAL_INVOKE_SOURCE: &str = r#"{1}
[]
[]
{
WireExternal.Structural.Newtype {
(name.Name wrapped.Type)
Public Invoke.WireAttributes Realize.name Private Realize.wrapped
}
}
{}
{}"#;
const MANIFEST: &str = include_str!("../Cargo.toml");

struct Daemon {
    child: Child,
    socket: PathBuf,
}

impl Daemon {
    fn start(socket: &Path, database: &Path) -> Self {
        let program = std::env::var_os("SEMA_TRANSLATOR_BIN").expect("SEMA_TRANSLATOR_BIN");
        let mut child = Command::new(program)
            .arg("daemon")
            .arg("--socket")
            .arg(socket)
            .arg("--database")
            .arg(database)
            .arg("--authorized-uid")
            .arg(process_uid().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn sema-translator daemon");
        let line = BufReader::new(child.stdout.take().expect("captured daemon stdout"))
            .lines()
            .next()
            .expect("daemon readiness line")
            .expect("read daemon readiness");
        assert_eq!(line, format!("READY {}", socket.display()));
        Self {
            child,
            socket: socket.to_path_buf(),
        }
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.socket.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.socket.exists() {
            std::fs::remove_file(&self.socket).expect("remove stale witness socket");
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.stop();
    }
}

fn process_uid() -> u32 {
    std::fs::metadata(".")
        .expect("current directory metadata")
        .uid()
}

fn process_principal() -> PrincipalId {
    principal_for_unix_uid(process_uid())
}

fn authority_request(operation: AuthorityOperation) -> AuthorityRequest {
    let (role, capability) = match &operation {
        AuthorityOperation::SealUniversal(_) => (
            AuthorityRole::UniversalAuthor,
            AuthorityCapability::SealUniversal,
        ),
        AuthorityOperation::Rename(_) => (
            AuthorityRole::UniversalMaintainer,
            AuthorityCapability::Rename,
        ),
        AuthorityOperation::PublishRustVocabulary(_) => (
            AuthorityRole::RustVocabularyPublisher,
            AuthorityCapability::PublishRustVocabulary,
        ),
        AuthorityOperation::Read(_) => (AuthorityRole::Reader, AuthorityCapability::Read),
    };
    AuthorityRequest {
        authorization: AuthorizationClaim {
            principal: process_principal(),
            role,
            capability,
        },
        operation,
    }
}

fn exchange(
    socket: &Path,
    request: AuthorityRequest,
    expect_event: bool,
) -> (AuthorityReply, Option<PostCommitEvent>) {
    let exchange = ExchangeIdentifier::new(
        SessionEpoch::new(1),
        ExchangeLane::Connector,
        LaneSequence::first(),
    );
    let frame = TranslatorFrame::new(
        AUTHORITY_ROUTE,
        StreamingFrameBody::Request {
            exchange,
            request: Request::from_payload(request),
        },
    );
    let mut stream = UnixStream::connect(socket).expect("connect sema-translator daemon");
    stream
        .write_all(&frame.encode_length_prefixed().expect("encode request"))
        .expect("write request");
    let reply =
        TranslatorFrame::decode_length_prefixed(&read_frame(&mut stream)).expect("decode reply");
    let reply = match reply.into_body() {
        StreamingFrameBody::Reply {
            reply: Reply::Accepted { per_operation, .. },
            ..
        } => match per_operation.head() {
            SubReply::Ok(reply) => reply.clone(),
            SubReply::Failed {
                detail: Some(reply),
                ..
            } => reply.clone(),
            other => panic!("unexpected authority subreply: {other:?}"),
        },
        other => panic!("expected authority reply frame, got {other:?}"),
    };
    let event = if expect_event {
        match TranslatorFrame::decode_length_prefixed(&read_frame(&mut stream))
            .expect("decode post-commit event")
            .into_body()
        {
            StreamingFrameBody::SubscriptionEvent { event, .. } => Some(event),
            other => panic!("expected authority event frame, got {other:?}"),
        }
    } else {
        None
    };
    (reply, event)
}

fn read_frame(stream: &mut UnixStream) -> Vec<u8> {
    let mut length = [0; 4];
    stream.read_exact(&mut length).expect("read frame length");
    let length = u32::from_be_bytes(length) as usize;
    let mut bytes = Vec::with_capacity(length + 4);
    bytes.extend_from_slice(&(length as u32).to_be_bytes());
    bytes.resize(length + 4, 0);
    stream.read_exact(&mut bytes[4..]).expect("read frame body");
    bytes
}

fn current(socket: &Path) -> DatabaseMarker {
    match exchange(
        socket,
        authority_request(AuthorityOperation::Read(ReadOperation::Current)),
        false,
    )
    .0
    {
        AuthorityReply::Current(current) => current.database_marker,
        other => panic!("expected current authority marker, got {other:?}"),
    }
}

fn expected(marker: DatabaseMarker) -> WritePrecondition {
    WritePrecondition {
        database_marker: marker,
        table_generations: Vec::new(),
    }
}

fn encoded(chain: &[u16]) -> VocabularyEncodedId {
    VocabularyEncodedId::new(
        VocabularyRoot::Universal,
        chain.iter().copied().map(LocalEncodedId::new).collect(),
    )
    .expect("fixture identity is non-empty")
}

fn logos() -> LogosLanguage {
    LogosLanguage::seal(
        LogosLanguageTypeIds {
            newtype: encoded(&[1]),
            enumeration: encoded(&[2]),
            visibility: encoded(&[3]),
            attributes: encoded(&[4]),
            attribute: encoded(&[5]),
            path: encoded(&[6]),
            configuration_predicate: encoded(&[7]),
            derive_group: encoded(&[8]),
            generics: encoded(&[9]),
            generic_parameter: encoded(&[10]),
            type_reference: encoded(&[11]),
            variant: encoded(&[12]),
        },
        LogosLanguageWords {
            public: encoded(&[20]),
            private: encoded(&[21]),
        },
    )
    .expect("canonical Logos language")
}

fn literal_landing(shape: &TemplateLandingShape<VocabularyRoot>) -> LandingShape<VocabularyRoot> {
    match shape {
        TemplateLandingShape::Fixed(landing)
        | TemplateLandingShape::ValueOrFuture { value: landing, .. } => landing.clone(),
        TemplateLandingShape::Nested(target) => LandingShape::Type(target.clone()),
        TemplateLandingShape::Sequence {
            minimum,
            maximum,
            element,
            ..
        } => LandingShape::sequence(*minimum, *maximum, literal_landing(element)),
    }
}

fn textual(logos: &LogosLanguage) -> TextualNomos {
    let newtype = TemplateLanguage::derive(logos.grammar(), logos.landing(), logos.newtype_type())
        .expect("derive Template(Logos)");
    let constructor = newtype
        .type_declaration(newtype.root())
        .and_then(|declaration| declaration.constructors().first())
        .expect("fixture newtype constructor");
    let field_output = |index: usize| {
        TemplateFutureOutput::new(literal_landing(
            constructor
                .landing_fields()
                .get(index)
                .expect("fixture landing field")
                .shape(),
        ))
    };
    TextualNomos::seal(
        logos,
        TextualNomosTypeIds {
            document: encoded(&[100, 1]),
            revision: encoded(&[100, 2]),
            empty_braces: encoded(&[100, 3]),
            empty_square: encoded(&[100, 4]),
            transformers: encoded(&[100, 5]),
            transformer: encoded(&[100, 6]),
            input_signature: encoded(&[100, 7]),
            input_parameter: encoded(&[100, 8]),
            newtype_body: encoded(&[100, 9]),
            enumeration_body: encoded(&[100, 10]),
            attributes_body: encoded(&[100, 11]),
        },
        TextualNomosWords {
            named: encoded(&[101, 1]),
            structural: encoded(&[101, 2]),
            newtype: encoded(&[101, 3]),
            enumeration: encoded(&[101, 4]),
            realize: encoded(&[101, 5]),
            splice: encoded(&[101, 6]),
            invoke: encoded(&[101, 7]),
        },
        vec![
            TextualNomosMetaType {
                word: encoded(&[102, 1]),
                meta: MetaType::Name,
                output: field_output(2),
            },
            TextualNomosMetaType {
                word: encoded(&[102, 2]),
                meta: MetaType::Type,
                output: field_output(4),
            },
        ],
    )
    .expect("seal TextualNomos")
}

struct FixedNames(BTreeMap<VocabularyEncodedId, Name>);

impl FixedNames {
    fn new() -> Self {
        Self(
            [
                (encoded(&[20]), "Public"),
                (encoded(&[21]), "Private"),
                (encoded(&[101, 1]), "Named"),
                (encoded(&[101, 2]), "Structural"),
                (encoded(&[101, 3]), "Newtype"),
                (encoded(&[101, 4]), "Enumeration"),
                (encoded(&[101, 5]), "Realize"),
                (encoded(&[101, 6]), "Splice"),
                (encoded(&[101, 7]), "Invoke"),
                (encoded(&[102, 1]), "Name"),
                (encoded(&[102, 2]), "Type"),
            ]
            .into_iter()
            .map(|(identity, spelling)| (identity, Name::new(spelling)))
            .collect(),
        )
    }
}

impl EncodedNameResolver<VocabularyRoot> for FixedNames {
    fn resolve(&self, encoded_id: &VocabularyEncodedId) -> Option<&Name> {
        self.0.get(encoded_id)
    }
}

fn resolved_id(
    receipt: &SealCommitReceipt,
    modules: &[&str],
    spelling: &str,
) -> VocabularyEncodedId {
    receipt
        .name_table
        .declarations()
        .iter()
        .find(|resolved| {
            resolved.path().spelling().as_str() == spelling
                && resolved
                    .path()
                    .table()
                    .modules()
                    .iter()
                    .map(Name::as_str)
                    .eq(modules.iter().copied())
        })
        .unwrap_or_else(|| panic!("missing declaration {modules:?}/{spelling}"))
        .encoded_id()
        .clone()
}

fn source_path(path: &str) -> NomosSourcePath {
    NomosSourcePath::try_new(path).expect("valid relative .nomos path")
}

fn fixture_module() -> NomosModulePath {
    NomosModulePath::try_from_spellings(["fixture"]).expect("fixture module")
}

fn file_manifest() -> NomosFileManifest {
    NomosFileManifest(
        source_path("entry.nomos"),
        vec![
            NomosManifestFile(source_path("attributes.nomos"), fixture_module(), vec![]),
            NomosManifestFile(
                source_path("entry.nomos"),
                fixture_module(),
                vec![source_path("attributes.nomos")],
            ),
        ],
    )
}

fn assert_no_committed_receipt(socket: &Path, operation_key: [u8; 32]) {
    let operation_key = OperationKey::new(operation_key);
    let reply = exchange(
        socket,
        authority_request(AuthorityOperation::Read(ReadOperation::CommittedReceipt {
            operation_key,
        })),
        false,
    )
    .0;
    assert!(matches!(
        reply,
        AuthorityReply::Rejected(NoWriteFailure::UnknownCommittedReceipt {
            operation_key: missing
        }) if missing == operation_key
    ));
}

#[test]
fn authored_nomos_process_dependencies_pin_the_approved_producers() {
    assert!(MANIFEST.contains("7cd62205a19938cb3921a4cad56d79a596c662f0"));
    assert!(MANIFEST.contains("6df830ab1ec9f315a5b50e40ffc393b48ea3d412"));
    assert!(MANIFEST.contains("51c02c4a7b6f67d9dad095f11986085d7d65785b"));
    assert!(MANIFEST.contains("0786fbe8caf27552afcdd5deb85bc82ec6088337"));
}

#[test]
fn authored_nomos_seals_recovers_and_renames_through_the_authority_process() {
    let directory = tempfile::tempdir().expect("isolated authority directory");
    let socket = directory.path().join("sema-translator.sock");
    let database = directory.path().join("sema-translator.sema");
    let mut daemon = Daemon::start(&socket, &database);
    let logos = logos();
    let textual = textual(&logos);
    let fixed = FixedNames::new();
    let planned = textual
        .plan_load(
            SOURCE,
            &fixed,
            NomosModulePath::try_from_spellings(["fixture"]).expect("module path"),
            [41; 32],
            expected(current(&socket)),
        )
        .expect("allocation-free authored Nomos plan");

    let (sealed, event) = exchange(
        &socket,
        authority_request(AuthorityOperation::SealUniversal(planned.request().clone())),
        true,
    );
    assert!(matches!(
        textual.complete_load(&planned, &sealed, &fixed),
        Err(NomosLoadError::ReceiptNotDurableSeal)
    ));
    let receipt = match &sealed {
        AuthorityReply::Committed(CommittedReceipt::SealUniversal(receipt)) => receipt.clone(),
        other => panic!("expected committed authored Nomos seal, got {other:?}"),
    };
    assert!(matches!(
        event,
        Some(PostCommitEvent::UniversalSealed(ref committed)) if committed == &receipt
    ));
    let durable = exchange(
        &socket,
        authority_request(AuthorityOperation::Read(ReadOperation::CommittedReceipt {
            operation_key: OperationKey::new([41; 32]),
        })),
        false,
    )
    .0;
    let durable_receipt = match &durable {
        AuthorityReply::Receipt(CommittedReceipt::SealUniversal(receipt)) => receipt,
        other => panic!("expected durable authored Nomos receipt, got {other:?}"),
    };
    assert_eq!(durable_receipt, &receipt);
    let mut loaded = textual
        .complete_load(&planned, &durable, &fixed)
        .expect("materialize only from the durable authority receipt");
    let transformer = resolved_id(&receipt, &["fixture"], "WireNewtype");
    let wrapped = resolved_id(&receipt, &["fixture", "WireNewtype"], "wrapped");
    assert_eq!(transformer.chain().len(), 2);
    assert_eq!(wrapped.chain().len(), 3);
    assert_eq!(wrapped.chain()[..2], transformer.chain()[..]);
    let content = loaded.content_identity().expect("Nomos content identity");

    daemon.stop();
    assert!(UnixStream::connect(&socket).is_err());
    let mut recovered_daemon = Daemon::start(&socket, &database);
    let recovered = exchange(
        &socket,
        authority_request(AuthorityOperation::Read(ReadOperation::CommittedReceipt {
            operation_key: OperationKey::new([41; 32]),
        })),
        false,
    )
    .0;
    let recovered_receipt = match &recovered {
        AuthorityReply::Receipt(CommittedReceipt::SealUniversal(receipt)) => receipt,
        other => panic!("expected recovered Nomos receipt, got {other:?}"),
    };
    assert_eq!(recovered_receipt, &receipt);
    let recovered_load = textual
        .complete_load(&planned, &recovered, &fixed)
        .expect("recovered receipt rematerializes the same encoded document");
    assert_eq!(recovered_load.transformers(), loaded.transformers());

    let (renamed, rename_event) = exchange(
        &socket,
        authority_request(AuthorityOperation::Rename(Rename {
            operation_key: OperationKey::new([42; 32]),
            expected: expected(current(&socket)),
            target: wrapped.clone(),
            new_spelling: Name::new("inner"),
        })),
        true,
    );
    let rename: RenameCommitReceipt = match renamed {
        AuthorityReply::Committed(CommittedReceipt::Rename(receipt)) => receipt,
        other => panic!("expected committed operational rename, got {other:?}"),
    };
    assert!(matches!(
        rename_event,
        Some(PostCommitEvent::Renamed(ref committed)) if committed == &rename
    ));
    loaded
        .apply_rename(&rename)
        .expect("committed spelling-only rename applies to the sibling");
    assert_eq!(loaded.names().spelling(&wrapped), Some("inner"));
    assert_eq!(
        loaded.content_identity().expect("content identity"),
        content
    );
    let viewed = textual
        .view(loaded.decoded(), loaded.names())
        .expect("render renamed sibling");
    assert!(viewed.contains("(name.Name inner.Type)"));
    assert!(viewed.contains("Realize.inner"));

    let invalid_source = SOURCE.replace("Invoke.WireAttributes", "Invoke.Missing");
    let before = current(&socket);
    let invalid = textual
        .plan_load(
            &invalid_source,
            &fixed,
            NomosModulePath::try_from_spellings(["fixture"]).expect("module path"),
            [43; 32],
            expected(before),
        )
        .expect("lookup-only missing target remains a valid structural plan");
    let refused = exchange(
        &socket,
        authority_request(AuthorityOperation::SealUniversal(invalid.request().clone())),
        false,
    )
    .0;
    assert!(matches!(
        refused,
        AuthorityReply::Rejected(NoWriteFailure::UnresolvedReference { .. })
    ));
    assert_eq!(current(&socket), before);

    recovered_daemon.stop();
}

// [not-understood-by-psyche, Entry 7, NomosTrainAddendum-2026-07-30]
// The resolved files populate one self-contained v1 package namespace; this
// process witness does not introduce cross-package Invoke lookup.
#[test]
fn authored_nomos_manifest_is_one_process_request_and_graph_failures_leave_no_receipt() {
    let directory = tempfile::tempdir().expect("isolated authority directory");
    let source_root = directory.path().join("sources");
    std::fs::create_dir(&source_root).expect("create source root");
    std::fs::write(source_root.join("attributes.nomos"), ATTRIBUTES_SOURCE)
        .expect("write attributes source");
    std::fs::write(source_root.join("entry.nomos"), NEWTYPE_SOURCE).expect("write entry source");
    let socket = directory.path().join("sema-translator.sock");
    let database = directory.path().join("sema-translator.sema");
    let mut daemon = Daemon::start(&socket, &database);
    let logos = logos();
    let textual = textual(&logos);
    let fixed = FixedNames::new();

    let planned = textual
        .plan_file_population(
            &source_root,
            &file_manifest(),
            &fixed,
            [51; 32],
            expected(current(&socket)),
        )
        .expect("all files and graph edges plan before one request exists");
    assert_eq!(planned.request().declarations.len(), 1);
    assert_eq!(planned.request().references.len(), 3);
    let (committed, event) = exchange(
        &socket,
        authority_request(AuthorityOperation::SealUniversal(planned.request().clone())),
        true,
    );
    let receipt = match &committed {
        AuthorityReply::Committed(CommittedReceipt::SealUniversal(receipt)) => receipt,
        other => panic!("expected committed manifest allocation, got {other:?}"),
    };
    assert!(matches!(
        event,
        Some(PostCommitEvent::UniversalSealed(ref committed)) if committed == receipt
    ));
    let durable = exchange(
        &socket,
        authority_request(AuthorityOperation::Read(ReadOperation::CommittedReceipt {
            operation_key: OperationKey::new([51; 32]),
        })),
        false,
    )
    .0;
    let population = textual
        .complete_file_population(&planned, &durable, &fixed)
        .expect("durable receipt materializes both source files");
    assert_eq!(population.transformers().declarations().len(), 2);

    let before_refusals = current(&socket);
    std::fs::write(
        source_root.join("external-invoke.nomos"),
        EXTERNAL_INVOKE_SOURCE,
    )
    .expect("write external Invoke source");
    let external_invoke = NomosFileManifest(
        source_path("external-invoke.nomos"),
        vec![NomosManifestFile(
            source_path("external-invoke.nomos"),
            fixture_module(),
            vec![],
        )],
    );
    // [not-understood-by-psyche, Entry 7, NomosTrainAddendum-2026-07-30]
    // Authority already contains fixture/WireAttributes, but the later manifest
    // does not select that declaration into its self-contained v1 package.
    assert!(matches!(
        textual.plan_file_population(
            &source_root,
            &external_invoke,
            &fixed,
            [55; 32],
            expected(before_refusals)
        ),
        Err(NomosManifestLoadError::ExternalInvoke(modules, spelling))
            if modules == vec![Name::new("fixture")]
                && spelling == Name::new("WireAttributes")
    ));
    assert_eq!(current(&socket), before_refusals);
    assert_no_committed_receipt(&socket, [55; 32]);

    let missing = NomosFileManifest(
        source_path("entry.nomos"),
        vec![NomosManifestFile(
            source_path("entry.nomos"),
            fixture_module(),
            vec![source_path("missing.nomos")],
        )],
    );
    assert!(matches!(
        textual.plan_file_population(
            &source_root,
            &missing,
            &fixed,
            [52; 32],
            expected(before_refusals)
        ),
        Err(NomosManifestLoadError::MissingDependency(_, _))
    ));
    assert_eq!(current(&socket), before_refusals);
    assert_no_committed_receipt(&socket, [52; 32]);

    let cyclic = NomosFileManifest(
        source_path("entry.nomos"),
        vec![
            NomosManifestFile(
                source_path("entry.nomos"),
                fixture_module(),
                vec![source_path("attributes.nomos")],
            ),
            NomosManifestFile(
                source_path("attributes.nomos"),
                fixture_module(),
                vec![source_path("entry.nomos")],
            ),
        ],
    );
    assert!(matches!(
        textual.plan_file_population(
            &source_root,
            &cyclic,
            &fixed,
            [53; 32],
            expected(before_refusals)
        ),
        Err(NomosManifestLoadError::DependencyCycle(_))
    ));
    assert_eq!(current(&socket), before_refusals);
    assert_no_committed_receipt(&socket, [53; 32]);

    std::fs::write(directory.path().join("outside.nomos"), ATTRIBUTES_SOURCE)
        .expect("write outside source");
    symlink(
        directory.path().join("outside.nomos"),
        source_root.join("escape.nomos"),
    )
    .expect("create source-root escape");
    let escaping = NomosFileManifest(
        source_path("escape.nomos"),
        vec![NomosManifestFile(
            source_path("escape.nomos"),
            fixture_module(),
            vec![],
        )],
    );
    assert!(matches!(
        textual.plan_file_population(
            &source_root,
            &escaping,
            &fixed,
            [54; 32],
            expected(before_refusals)
        ),
        Err(NomosManifestLoadError::SourceEscapesRoot(_))
    ));
    assert_eq!(current(&socket), before_refusals);
    assert_no_committed_receipt(&socket, [54; 32]);

    daemon.stop();
}
