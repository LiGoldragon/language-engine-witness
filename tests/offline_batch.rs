//! Process and build-script witness for the socket-free Nomos batch entry point.

use std::fs;
use std::path::Path;
use std::process::Command;

use batch_core_ethos::{EthosDecodeError, WholeEthosFileKind};
use batch_core_logos::WholeLogosItem;
use batch_nomos_engine::batch::{
    BatchConfiguration, BatchGenerationError, DeferredBatchConstruct, OfflineBatchConfiguration,
    OfflineBatchGeneration, PreparedBatchGenerator,
};
use language_engine_witness::{
    BUILD_SCRIPT_INTERFACE_OUTCOME, BUILD_SCRIPT_INTERFACE_RUST, BUILD_SCRIPT_NEXUS_OUTCOME,
    BUILD_SCRIPT_NEXUS_RUST, BUILD_SCRIPT_SEMA_OUTCOME, BUILD_SCRIPT_SEMA_RUST,
    exercise_build_script_interface, exercise_build_script_sema,
};
use nexus_rust_logos::RustEncodedIdCodec;
use sema_engine::{
    Assertion, Engine, EngineOpen, EngineRecord, Error as SemaError, FamilyName, RecordKey,
    SchemaHash, SchemaVersion, TableDescriptor, TableName,
};
use slice_name_table::LocalEncodedId;
use slice_signal_sema_translator::{VocabularyEncodedId, VocabularyRoot};

const CONFIGURATION: &str = include_str!("fixtures/batch-config.json");
const INTERFACE: &str = include_str!("fixtures/interface.ethos");
const NEXUS: &str = include_str!("fixtures/nexus.ethos");
const SEMA: &str = include_str!("fixtures/sema.ethos");

fn generator() -> PreparedBatchGenerator {
    BatchConfiguration::from_json(CONFIGURATION)
        .expect("caller-authored batch configuration JSON")
        .prepare()
        .expect("caller-authored identities seat without allocation")
}

fn universal(local: u16) -> VocabularyEncodedId {
    VocabularyEncodedId::new(VocabularyRoot::Universal, vec![LocalEncodedId::new(local)])
        .expect("fixture identity chain is complete")
}

#[test]
fn library_projects_all_goldens_and_reports_exactly_deferred_semantics() {
    let generator = generator();

    let nexus = generator.generate(NEXUS).expect("Nexus batch generation");
    assert_eq!(nexus.kind(), WholeEthosFileKind::Nexus);
    assert_eq!(nexus.logos().items().len(), 4);
    assert!(nexus.deferred().is_empty());
    assert_eq!(nexus.rust(), BUILD_SCRIPT_NEXUS_RUST);
    assert!(BUILD_SCRIPT_NEXUS_OUTCOME.contains("deferred 0"));

    let interface = generator
        .generate(INTERFACE)
        .expect("supported Interface declarations generate");
    assert_eq!(interface.kind(), WholeEthosFileKind::Interface);
    assert_eq!(interface.deferred().len(), 2);
    assert_eq!(
        interface
            .deferred()
            .iter()
            .filter(|item| matches!(
                item,
                DeferredBatchConstruct::InterfaceOperatorApplication { .. }
            ))
            .count(),
        2
    );
    assert_eq!(interface.rust(), BUILD_SCRIPT_INTERFACE_RUST);
    assert!(BUILD_SCRIPT_INTERFACE_OUTCOME.contains("deferred 2"));
    assert!(interface.rust().contains("#[derive(rkyv::Archive"));
    assert_eq!(interface.rust().matches("impl z2VL5p for").count(), 3);
    assert_eq!(interface.rust().matches("impl z2VL5q for").count(), 3);
    assert_eq!(interface.rust().matches("impl z2VL5r for").count(), 2);
    assert_eq!(
        interface
            .rust()
            .matches("impl std::error::Error for")
            .count(),
        2
    );
    assert!(!interface.rust().contains("impl From<"));
    let behavior = exercise_build_script_interface();
    assert_eq!(behavior.membership_assertions, 8);
    assert!(behavior.archive_round_trip);
    assert!(behavior.refusal_display_matches_debug);

    let sema = generator
        .generate(SEMA)
        .expect("supported Sema record declarations generate");
    assert_eq!(sema.kind(), WholeEthosFileKind::Sema);
    assert_eq!(sema.logos().items().len(), 9);
    assert!(sema.deferred().is_empty());
    assert_eq!(sema.rust(), BUILD_SCRIPT_SEMA_RUST);
    assert!(BUILD_SCRIPT_SEMA_OUTCOME.contains("deferred 0"));
    assert_eq!(
        sema.rust()
            .matches("impl sema_engine::TableSpecification for")
            .count(),
        3
    );
    assert_eq!(sema.rust().matches("type Record =").count(), 3);
    assert_eq!(sema.rust().matches("type Key =").count(), 3);
    assert!(sema.rust().contains("type Key = signal_domain::Domain"));
    assert!(!sema.rust().contains("pub struct z2VL5m"));

    let migration = RustEncodedIdCodec::encode(&universal(1070));
    let migrations = RustEncodedIdCodec::encode(&universal(1074));
    assert!(sema.rust().contains(&format!("pub struct {migration}")));
    assert!(sema.rust().contains(&format!("pub struct {migrations};")));
    assert!(!sema.rust().contains("EvolutionStep"));
    assert!(!sema.rust().contains("with_prior("));

    let temporary = tempfile::tempdir().expect("fresh generated Sema store");
    let behavior = exercise_build_script_sema(temporary.path().join("generated.sema"))
        .expect("generated Sema tables execute through sema-engine");
    assert_eq!(behavior.registered_tables, 3);
    assert_eq!(
        behavior.records_table,
        RustEncodedIdCodec::encode(&universal(1071))
    );
    assert!(behavior.durable_round_trip);
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, Eq, PartialEq)]
struct StoredShapeV1 {
    key: String,
    value: u64,
}

impl EngineRecord for StoredShapeV1 {
    fn record_key(&self) -> RecordKey {
        RecordKey::new(self.key.clone())
    }
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, Eq, PartialEq)]
struct StoredShapeV2 {
    key: String,
    value: u64,
    extra: u64,
}

#[test]
fn same_ids_changed_layout_moves_hash_and_existing_store_refuses_registration() {
    let generator = generator();
    let original = generator.generate(SEMA).expect("original Sema generation");
    let changed_source = SEMA.replace(
        "StoredRecord.{RecordIdentifier Entry}",
        "StoredRecord.{RecordIdentifier Entry Entry}",
    );
    let changed = generator
        .generate(&changed_source)
        .expect("same identities with changed StoredRecord layout generate");
    let original_hash = records_schema_hash(&original);
    let changed_hash = records_schema_hash(&changed);
    assert_ne!(original_hash, changed_hash);

    let temporary = tempfile::tempdir().expect("fresh incompatibility store");
    let path = temporary.path().join("shape-mismatch.sema");
    {
        let mut engine = Engine::open(EngineOpen::new(&path, SchemaVersion::new(1)))
            .expect("original store opens");
        let records = engine
            .register_table(TableDescriptor::<StoredShapeV1>::new(
                TableName::new("z2VL5k"),
                FamilyName::new("z2VL5k"),
                SchemaHash::new(original_hash),
            ))
            .expect("original generated identity registers");
        engine
            .assert(Assertion::new(
                records,
                StoredShapeV1 {
                    key: "witness".to_owned(),
                    value: 17,
                },
            ))
            .expect("original-shape bytes persist");
    }

    let mut reopened = Engine::open(EngineOpen::new(&path, SchemaVersion::new(1)))
        .expect("existing store reopens");
    match reopened.register_table(TableDescriptor::<StoredShapeV2>::new(
        TableName::new("z2VL5k"),
        FamilyName::new("z2VL5k"),
        SchemaHash::new(changed_hash),
    )) {
        Err(SemaError::FamilyIdentityMismatch { .. }) => {}
        Err(error) => panic!("unexpected changed-layout registration error: {error}"),
        Ok(_) => panic!("changed layout unexpectedly registered over existing bytes"),
    }
}

fn records_schema_hash(outcome: &batch_nomos_engine::batch::BatchGenerationOutcome) -> [u8; 32] {
    outcome
        .logos()
        .items()
        .iter()
        .find_map(|item| match item {
            WholeLogosItem::Table(table) if table.name() == &universal(1071) => {
                Some(table.schema_hash().expect("table schema hashes"))
            }
            _ => None,
        })
        .expect("records table projects")
}

#[test]
fn library_preserves_typed_header_refusals() {
    let generator = generator();
    match generator.generate("Unknown.1\n[]\n{ [] [] }\n") {
        Err(BatchGenerationError::Decode(EthosDecodeError::UnknownFileKind { .. })) => {}
        Err(error) => panic!("unexpected unknown-kind error: {error}"),
        Ok(_) => panic!("unknown file kind unexpectedly generated"),
    }
    match generator.generate("Nexus.2\n[]\n{ [] [] }\n") {
        Err(BatchGenerationError::Decode(EthosDecodeError::UnsupportedVersion { .. })) => {}
        Err(error) => panic!("unexpected version error: {error}"),
        Ok(_) => panic!("unsupported version unexpectedly generated"),
    }
}

#[test]
fn installed_cli_generates_all_goldens_and_nexus_output_compiles() {
    let program = std::env::var_os("NOMOS_GENERATOR_BIN").expect("NOMOS_GENERATOR_BIN");
    let temporary = tempfile::tempdir().expect("batch CLI scratch directory");
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for fixture in ["interface", "nexus", "sema"] {
        let output = temporary.path().join(format!("{fixture}.rs"));
        let receipt = temporary.path().join(format!("{fixture}.outcome"));
        let run = Command::new(&program)
            .arg(fixture_root.join("batch-config.json"))
            .arg(fixture_root.join(format!("{fixture}.ethos")))
            .arg(&output)
            .arg(&receipt)
            .output()
            .expect("run installed nomos-generate");
        assert!(
            run.status.success(),
            "{fixture} CLI generation failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(
            fs::metadata(&output)
                .expect("generated Rust metadata")
                .len()
                > 0
        );
        assert!(fs::metadata(&receipt).expect("outcome metadata").len() > 0);
    }

    compile_nexus_artifact(
        &temporary.path().join("nexus.rs"),
        temporary.path().join("scratch-crate").as_path(),
    );
    assert_eq!(
        fs::read_to_string(temporary.path().join("interface.rs")).expect("CLI Interface artifact"),
        BUILD_SCRIPT_INTERFACE_RUST
    );
    assert_eq!(
        fs::read_to_string(temporary.path().join("sema.rs")).expect("CLI Sema artifact"),
        BUILD_SCRIPT_SEMA_RUST
    );
    assert!(
        fs::read_to_string(temporary.path().join("sema.outcome"))
            .expect("CLI Sema receipt")
            .contains("deferred 0")
    );

    let bad_source = temporary.path().join("unknown.ethos");
    let bad_output = temporary.path().join("unknown.rs");
    let bad_receipt = temporary.path().join("unknown.outcome");
    fs::write(&bad_source, "Unknown.1\n[]\n{ [] [] }\n").expect("bad source fixture");
    let refusal = Command::new(program)
        .arg(fixture_root.join("batch-config.json"))
        .arg(&bad_source)
        .arg(&bad_output)
        .arg(&bad_receipt)
        .output()
        .expect("run installed nomos-generate refusal");
    assert!(!refusal.status.success());
    assert!(String::from_utf8_lossy(&refusal.stderr).contains("UnknownFileKind"));
    assert!(!bad_output.exists());
    assert!(!bad_receipt.exists());
}

fn compile_nexus_artifact(generated: &Path, scratch: &Path) {
    fs::create_dir_all(scratch.join("src")).expect("scratch source directory");
    fs::write(
        scratch.join("Cargo.toml"),
        "[package]\nname = \"offline-batch-nexus\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .expect("scratch manifest");
    let entry = RustEncodedIdCodec::encode(&universal(1001));
    let record_set = RustEncodedIdCodec::encode(&universal(1009));
    let guardian_reason = RustEncodedIdCodec::encode(&universal(1013));
    let referent = RustEncodedIdCodec::encode(&universal(1036));
    let unit = RustEncodedIdCodec::encode(&universal(1061));
    let emitted = fs::read_to_string(generated).expect("CLI Nexus artifact");
    fs::write(
        scratch.join("src/lib.rs"),
        format!(
            "pub mod generated_interface {{\n    pub struct {entry};\n    pub struct {record_set};\n    pub struct {guardian_reason};\n    pub struct {referent};\n}}\npub type {unit} = ();\n{emitted}"
        ),
    )
    .expect("scratch Nexus source");
    let check = Command::new("cargo")
        .args(["check", "--quiet", "--jobs", "2"])
        .current_dir(scratch)
        .env("CARGO_TARGET_DIR", scratch.join("target"))
        .output()
        .expect("compile generated Nexus crate");
    assert!(
        check.status.success(),
        "scratch Nexus compile failed:\n{}\ngenerated:\n{emitted}",
        String::from_utf8_lossy(&check.stderr)
    );
}
