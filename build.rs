use std::env;
use std::fs;
use std::path::PathBuf;

use batch_nomos_engine::batch::{
    BatchConfiguration, BatchOutcomeReporting, DeferredBatchConstruct, OfflineBatchConfiguration,
    OfflineBatchGeneration,
};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

fn main() -> Result<(), AnyError> {
    println!("cargo:rerun-if-changed=tests/fixtures/batch-config.json");
    println!("cargo:rerun-if-changed=tests/fixtures/nexus.ethos");
    println!("cargo:rerun-if-changed=tests/fixtures/interface.ethos");
    println!("cargo:rerun-if-changed=tests/fixtures/sema.ethos");

    let manifest = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .ok_or("Cargo did not supply CARGO_MANIFEST_DIR to the batch witness build script")?,
    );
    let output = PathBuf::from(
        env::var_os("OUT_DIR")
            .ok_or("Cargo did not supply OUT_DIR to the batch witness build script")?,
    );
    let configuration = fs::read_to_string(manifest.join("tests/fixtures/batch-config.json"))?;
    let generator = BatchConfiguration::from_json(&configuration)?.prepare()?;
    let nexus = generator.generate(&fs::read_to_string(
        manifest.join("tests/fixtures/nexus.ethos"),
    )?)?;
    if !nexus.deferred().is_empty() {
        return Err("Nexus build-script generation unexpectedly deferred constructs".into());
    }
    fs::write(output.join("build-script-nexus.rs"), nexus.rust())?;
    fs::write(output.join("build-script-nexus.outcome"), nexus.report())?;

    let interface = generator.generate(&fs::read_to_string(
        manifest.join("tests/fixtures/interface.ethos"),
    )?)?;
    if interface.deferred().len() != 2
        || !interface.deferred().iter().all(|construct| {
            matches!(
                construct,
                DeferredBatchConstruct::InterfaceOperatorApplication { .. }
            )
        })
    {
        return Err(
            "Interface build-script generation did not defer exactly two operator applications"
                .into(),
        );
    }
    fs::write(output.join("build-script-interface.rs"), interface.rust())?;
    fs::write(
        output.join("build-script-interface.outcome"),
        interface.report(),
    )?;

    let sema = generator.generate(&fs::read_to_string(
        manifest.join("tests/fixtures/sema.ethos"),
    )?)?;
    if !sema.deferred().is_empty() {
        return Err("Sema build-script generation unexpectedly deferred constructs".into());
    }
    fs::write(output.join("build-script-sema.rs"), sema.rust())?;
    fs::write(output.join("build-script-sema.outcome"), sema.report())?;
    Ok(())
}
