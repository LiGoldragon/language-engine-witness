use std::env;
use std::fs;
use std::path::PathBuf;

use batch_nomos_engine::batch::{
    BatchComponent, BatchConfiguration, BatchOutcomeReporting, OfflineBatchConfiguration,
    OfflineBatchGeneration,
};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

fn source_root() -> Result<PathBuf, AnyError> {
    env::var_os("SPIRIT_ETHOS_SOURCE")
        .map(PathBuf::from)
        .ok_or_else(|| "SPIRIT_ETHOS_SOURCE must name the sealed spirit-ethos revision".into())
}

fn main() -> Result<(), AnyError> {
    println!("cargo:rerun-if-env-changed=SPIRIT_ETHOS_SOURCE");
    let source = source_root()?;
    for file in [
        "batch-config.json",
        "allocation-manifest.nota",
        "allocation-receipt.nota",
        "interface.ethos",
        "nexus.ethos",
        "sema.ethos",
    ] {
        println!("cargo:rerun-if-changed={}", source.join(file).display());
    }

    let output = PathBuf::from(
        env::var_os("OUT_DIR")
            .ok_or("Cargo did not supply OUT_DIR to the strict batch witness build script")?,
    );
    let configuration = fs::read_to_string(source.join("batch-config.json"))?;
    let interface = fs::read_to_string(source.join("interface.ethos"))?;
    let nexus = fs::read_to_string(source.join("nexus.ethos"))?;
    let sema = fs::read_to_string(source.join("sema.ethos"))?;
    let generator = BatchConfiguration::from_json(&configuration)?.prepare()?;
    let outcomes = generator.generate_bundle(&[
        BatchComponent::named("interface", &interface),
        BatchComponent::named("nexus", &nexus),
        BatchComponent::named("sema", &sema),
    ])?;

    for outcome in outcomes {
        let (rust_name, report_name) = match outcome.kind().spelling() {
            "Interface" => (
                "build-script-interface.rs",
                "build-script-interface.outcome",
            ),
            "Nexus" => ("build-script-nexus.rs", "build-script-nexus.outcome"),
            "Sema" => ("build-script-sema.rs", "build-script-sema.outcome"),
            unexpected => return Err(format!("unexpected generated file kind {unexpected}").into()),
        };
        fs::write(output.join(rust_name), outcome.rust())?;
        fs::write(output.join(report_name), outcome.report())?;
    }
    Ok(())
}
