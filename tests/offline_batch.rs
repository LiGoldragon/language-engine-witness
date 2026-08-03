//! Process and build-script witness for the socket-free Nomos batch entry point.

use std::fs;
use std::path::Path;
use std::process::Command;

use batch_nomos_engine::batch::{
    BatchConfiguration, BatchGenerationError, DeferredBatchConstruct, OfflineBatchConfiguration,
    OfflineBatchGeneration, PreparedBatchGenerator,
};
use language_engine_witness::{
    BUILD_SCRIPT_INTERFACE_OUTCOME, BUILD_SCRIPT_INTERFACE_RUST, BUILD_SCRIPT_NEXUS_OUTCOME,
    BUILD_SCRIPT_NEXUS_RUST, exercise_build_script_interface,
};
use nexus_core_ethos::{EthosDecodeError, WholeEthosFileKind};
use nexus_rust_logos::RustEncodedIdCodec;
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
    assert_eq!(sema.logos().items().len(), 6);
    assert_eq!(sema.deferred().len(), 3);
    assert!(
        sema.deferred()
            .iter()
            .all(|item| matches!(item, DeferredBatchConstruct::SemaTable { .. }))
    );
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
            "pub struct {entry};\npub struct {record_set};\npub struct {guardian_reason};\npub struct {referent};\npub type {unit} = ();\n{emitted}"
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
