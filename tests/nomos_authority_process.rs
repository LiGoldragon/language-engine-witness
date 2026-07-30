use std::collections::BTreeMap;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use nomos_engine::{EngineEthosNameTree, encode_ethos_population};
use signal_nomos::{
    DeployOutcome, GenerationSelection, NomosDeploymentArtifacts, NomosProjectionArchive,
    NomosSlotId, ProjectionOutcome, Rejection as NomosRejection, Reply as NomosReply,
    Request as NomosRequest, SlotExpectation, SlotGeneration, TransformSelector, encode_request,
};
use slice_core_ethos::{
    WholeEthos, WholeEthosAttributes, WholeEthosItem, WholeEthosNewtype, WholeEthosTypeReference,
    WholeEthosVisibility, WholeEthosWrappedField,
};
use slice_core_logos::{LogosLanguage, LogosLanguageTypeIds, LogosLanguageWords};
use slice_core_nomos::{
    MetaType, NameTreeProjectionVersion, NomosFileManifest, NomosLoadError, NomosManifestFile,
    NomosManifestLoadError, NomosModulePath, NomosSourcePath, TemplateFutureOutput,
    TemplateLandingShape, TemplateLanguage, TextualNomos, TextualNomosMetaType,
    TextualNomosTypeIds, TextualNomosWords,
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
const PROCESS_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const PROCESS_IO_TIMEOUT: Duration = Duration::from_secs(10);

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
        wait_for_readiness(&mut child, socket, "sema-translator");
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

struct NomosDaemon {
    child: Child,
    socket: PathBuf,
}

impl NomosDaemon {
    fn start(socket: &Path, database: &Path) -> Self {
        let program = std::env::var_os("NOMOS_ENGINE_BIN").expect("NOMOS_ENGINE_BIN");
        let mut child = Command::new(program)
            .arg("daemon")
            .arg(socket)
            .arg(database)
            .arg(process_uid().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn nomos-engine daemon");
        wait_for_readiness(&mut child, socket, "nomos-engine");
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
            std::fs::remove_file(&self.socket).expect("remove stale Nomos socket");
        }
    }
}

impl Drop for NomosDaemon {
    fn drop(&mut self) {
        self.stop();
    }
}

fn process_uid() -> u32 {
    std::fs::metadata(".")
        .expect("current directory metadata")
        .uid()
}

fn wait_for_readiness(child: &mut Child, socket: &Path, daemon: &str) {
    let stdout = child
        .stdout
        .take()
        .unwrap_or_else(|| panic!("{daemon} readiness stdout was not captured"));
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let readiness = BufReader::new(stdout).lines().next().transpose();
        let _ = sender.send(readiness);
    });
    let received = receiver.recv_timeout(PROCESS_STARTUP_TIMEOUT);
    let timed_out = matches!(&received, Err(RecvTimeoutError::Timeout));
    if timed_out {
        terminate_child(child);
    }
    let reader_panicked = reader.join().is_err();
    let expected = format!("READY {}", socket.display());
    let outcome = match received {
        Ok(Ok(Some(line))) if line == expected => Ok(()),
        Ok(Ok(Some(line))) => Err(format!(
            "{daemon} emitted an unexpected readiness line: expected {expected:?}, got {line:?}"
        )),
        Ok(Ok(None)) => Err(format!("{daemon} closed stdout before readiness")),
        Ok(Err(error)) => Err(format!("{daemon} readiness read failed: {error}")),
        Err(RecvTimeoutError::Timeout) => Err(format!(
            "{daemon} did not become ready within {} seconds",
            PROCESS_STARTUP_TIMEOUT.as_secs()
        )),
        Err(RecvTimeoutError::Disconnected) if reader_panicked => {
            Err(format!("{daemon} readiness reader panicked"))
        }
        Err(RecvTimeoutError::Disconnected) => {
            Err(format!("{daemon} readiness reader disconnected"))
        }
    };
    if let Err(detail) = outcome {
        if !timed_out {
            terminate_child(child);
        }
        remove_stale_socket(socket, daemon);
        panic!("{detail}");
    }
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn remove_stale_socket(socket: &Path, daemon: &str) {
    if socket.exists() {
        std::fs::remove_file(socket).unwrap_or_else(|error| {
            panic!("remove stale {daemon} socket after startup failure: {error}")
        });
    }
}

fn connect_process_socket(socket: &Path, daemon: &str) -> UnixStream {
    let stream = UnixStream::connect(socket)
        .unwrap_or_else(|error| panic!("connect {daemon} process socket: {error}"));
    stream
        .set_read_timeout(Some(PROCESS_IO_TIMEOUT))
        .unwrap_or_else(|error| panic!("set {daemon} read timeout: {error}"));
    stream
        .set_write_timeout(Some(PROCESS_IO_TIMEOUT))
        .unwrap_or_else(|error| panic!("set {daemon} write timeout: {error}"));
    stream
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
    let mut stream = connect_process_socket(socket, "sema-translator");
    stream
        .write_all(&frame.encode_length_prefixed().expect("encode request"))
        .unwrap_or_else(|error| panic!("write sema-translator request within timeout: {error}"));
    let reply =
        TranslatorFrame::decode_length_prefixed(&read_frame(&mut stream, "sema-translator reply"))
            .expect("decode reply");
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
        match TranslatorFrame::decode_length_prefixed(&read_frame(
            &mut stream,
            "sema-translator post-commit event",
        ))
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

fn read_frame(stream: &mut UnixStream, phase: &str) -> Vec<u8> {
    let mut length = [0; 4];
    stream
        .read_exact(&mut length)
        .unwrap_or_else(|error| panic!("read {phase} length within timeout: {error}"));
    let length = u32::from_be_bytes(length) as usize;
    let mut bytes = Vec::with_capacity(length + 4);
    bytes.extend_from_slice(&(length as u32).to_be_bytes());
    bytes.resize(length + 4, 0);
    stream
        .read_exact(&mut bytes[4..])
        .unwrap_or_else(|error| panic!("read {phase} body within timeout: {error}"));
    bytes
}

fn nomos_exchange(socket: &Path, request: &NomosRequest) -> NomosReply {
    let request = encode_request(request).expect("encode Nomos request");
    let mut stream = connect_process_socket(socket, "nomos-engine");
    stream
        .write_all(
            &u32::try_from(request.len())
                .expect("bounded request")
                .to_be_bytes(),
        )
        .unwrap_or_else(|error| panic!("write Nomos request length within timeout: {error}"));
    stream
        .write_all(&request)
        .unwrap_or_else(|error| panic!("write Nomos request within timeout: {error}"));
    let mut length = [0; 4];
    stream
        .read_exact(&mut length)
        .unwrap_or_else(|error| panic!("read Nomos reply length within timeout: {error}"));
    let mut reply = vec![0; u32::from_be_bytes(length) as usize];
    stream
        .read_exact(&mut reply)
        .unwrap_or_else(|error| panic!("read complete Nomos reply within timeout: {error}"));
    let reply =
        rkyv::from_bytes::<NomosReply, rkyv::rancor::Error>(&reply).expect("decode Nomos reply");
    reply.validate().expect("validate Nomos reply");
    reply
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
            structure: encoded(&[13]),
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
            field: encoded(&[14]),
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
            struct_body: encoded(&[100, 12]),
            enumeration_body: encoded(&[100, 10]),
            attributes_body: encoded(&[100, 11]),
        },
        TextualNomosWords {
            named: encoded(&[101, 1]),
            structural: encoded(&[101, 2]),
            recursive: encoded(&[101, 9]),
            newtype: encoded(&[101, 3]),
            structure: encoded(&[101, 8]),
            enumeration: encoded(&[101, 4]),
            realize: encoded(&[101, 5]),
            splice: encoded(&[101, 6]),
            invoke: encoded(&[101, 7]),
            insert_at: encoded(&[101, 10]),
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
                (encoded(&[101, 9]), "Recursive"),
                (encoded(&[101, 10]), "InsertAt"),
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
    assert!(MANIFEST.contains("e4230f62b55fcf8543477a26d272862a63aa1fc3"));
    assert!(MANIFEST.contains("1af71a9d0625a6404f81cd6fe8b6393ac0c9040f"));
    assert!(MANIFEST.contains("58fd8036bffcb3cff6e27af4db25690764ecc768"));
    assert!(MANIFEST.contains("6df830ab1ec9f315a5b50e40ffc393b48ea3d412"));
    assert!(MANIFEST.contains("51c02c4a7b6f67d9dad095f11986085d7d65785b"));
    assert!(MANIFEST.contains("0786fbe8caf27552afcdd5deb85bc82ec6088337"));
}

#[test]
fn authored_nomos_deploys_transforms_advances_and_recovers_through_the_process() {
    let directory = tempfile::tempdir().expect("isolated process directory");
    let authority_socket = directory.path().join("sema-translator.sock");
    let authority_database = directory.path().join("sema-translator.sema");
    let mut authority = Daemon::start(&authority_socket, &authority_database);
    let textual = textual(&logos());
    let fixed = FixedNames::new();
    let planned = textual
        .plan_load(
            SOURCE,
            &fixed,
            fixture_module(),
            [44; 32],
            expected(current(&authority_socket)),
        )
        .expect("allocation-free authored Nomos plan");
    let sealed = exchange(
        &authority_socket,
        authority_request(AuthorityOperation::SealUniversal(planned.request().clone())),
        true,
    )
    .0;
    match sealed {
        AuthorityReply::Committed(CommittedReceipt::SealUniversal(_)) => {}
        other => panic!("expected committed authored Nomos seal, got {other:?}"),
    }
    let durable = exchange(
        &authority_socket,
        authority_request(AuthorityOperation::Read(ReadOperation::CommittedReceipt {
            operation_key: OperationKey::new([44; 32]),
        })),
        false,
    )
    .0;
    let loaded = textual
        .complete_load(&planned, &durable, &fixed)
        .expect("materialize from durable authority receipt");
    authority.stop();

    let population = loaded.population();
    let initial = population
        .seal(NameTreeProjectionVersion::initial())
        .expect("initial sealed population");
    let successor = population
        .advance_projection(&initial)
        .expect("successor projection");
    let identity = initial.capsule().content_identity();
    let initial_artifacts =
        NomosDeploymentArtifacts::from_population(&initial).expect("initial deployment artifacts");
    let successor_artifacts = NomosDeploymentArtifacts::from_population(&successor)
        .expect("successor deployment artifacts");

    let nomos_socket = directory.path().join("nomos.sock");
    let nomos_database = directory.path().join("nomos.sema");
    let slot = NomosSlotId::new(44);
    let mut nomos = NomosDaemon::start(&nomos_socket, &nomos_database);
    let deployed = nomos_exchange(
        &nomos_socket,
        &NomosRequest::Deploy {
            slot,
            expected: SlotExpectation::Empty,
            artifacts: initial_artifacts.clone(),
            selection: GenerationSelection::enriched(),
        },
    );
    assert!(matches!(
        deployed,
        NomosReply::Deployed(DeployOutcome::FreshDeployed {
            identity: deployed_identity,
            generation,
            committed_at,
            ..
        }) if deployed_identity == identity
            && generation == SlotGeneration::initial()
            && committed_at.commit_sequence() == 1
    ));

    let item = encoded(&[500, 1]);
    let reference = encoded(&[600, 4, 1]);
    let ethos = encode_ethos_population(
        WholeEthos::new(vec![WholeEthosItem::Newtype(WholeEthosNewtype::new(
            item.clone(),
            WholeEthosVisibility::Public,
            WholeEthosAttributes::empty(),
            WholeEthosWrappedField::new(
                WholeEthosVisibility::Private,
                WholeEthosTypeReference::Identity(reference.clone()),
            ),
        ))]),
        EngineEthosNameTree::try_new(
            vec![item.clone()],
            Vec::new(),
            Vec::new(),
            vec![item],
            vec![reference],
        )
        .expect("complete direct NameTree plan"),
    )
    .expect("direct Ethos population");
    let transformed = nomos_exchange(
        &nomos_socket,
        &NomosRequest::Transform {
            selector: TransformSelector::Live(slot),
            ethos: ethos.clone(),
        },
    );
    assert!(matches!(
        transformed,
        NomosReply::Transformed(ref outcome)
            if outcome.snapshot().identity() == identity
                && outcome.snapshot().generation() == SlotGeneration::initial()
                && outcome.snapshot().projection_version()
                    == NameTreeProjectionVersion::initial()
                && !outcome.logos_population().is_empty()
    ));

    let advanced = nomos_exchange(
        &nomos_socket,
        &NomosRequest::AdvanceProjection {
            capsule: identity,
            expected_previous_version: NameTreeProjectionVersion::initial(),
            projection: NomosProjectionArchive::from_projection(successor.projection())
                .expect("successor projection archive"),
            translator_receipt: None,
        },
    );
    assert!(matches!(
        advanced,
        NomosReply::ProjectionAdvanced(ProjectionOutcome::Advanced {
            previous_version,
            version,
            committed_at,
            ..
        }) if previous_version == NameTreeProjectionVersion::initial()
            && version == NameTreeProjectionVersion::new(1)
            && committed_at.commit_sequence() == 2
    ));

    nomos.stop();
    let mut recovered = NomosDaemon::start(&nomos_socket, &nomos_database);
    let recovered_transform = nomos_exchange(
        &nomos_socket,
        &NomosRequest::Transform {
            selector: TransformSelector::Live(slot),
            ethos,
        },
    );
    assert!(matches!(
        recovered_transform,
        NomosReply::Transformed(ref outcome)
            if outcome.snapshot().identity() == identity
                && outcome.snapshot().generation() == SlotGeneration::initial()
                && outcome.snapshot().projection_version() == NameTreeProjectionVersion::new(1)
    ));

    let stale = nomos_exchange(
        &nomos_socket,
        &NomosRequest::Deploy {
            slot,
            expected: SlotExpectation::Generation(SlotGeneration::new(999)),
            artifacts: initial_artifacts,
            selection: GenerationSelection::enriched(),
        },
    );
    assert_eq!(stale, NomosReply::Rejected(NomosRejection::ProjectionStale));
    let current = nomos_exchange(
        &nomos_socket,
        &NomosRequest::Deploy {
            slot,
            expected: SlotExpectation::Generation(SlotGeneration::new(999)),
            artifacts: successor_artifacts,
            selection: GenerationSelection::enriched(),
        },
    );
    assert!(matches!(
        current,
        NomosReply::Deployed(DeployOutcome::AlreadyCurrent {
            identity: current_identity,
            generation,
            observed_at,
            ..
        }) if current_identity == identity
            && generation == SlotGeneration::initial()
            && observed_at.commit_sequence() == 2
    ));
    recovered.stop();
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
    let content = loaded
        .population()
        .content_identity()
        .expect("Nomos content identity");

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
        loaded
            .population()
            .content_identity()
            .expect("content identity"),
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
