use std::env;
use std::fs;
use std::path::PathBuf;

use batch_nomos_engine::batch::{
    BatchConfiguration, BatchOutcomeReporting, OfflineBatchConfiguration, OfflineBatchGeneration,
};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

fn main() -> Result<(), AnyError> {
    println!("cargo:rerun-if-changed=tests/fixtures/batch-config.json");
    println!("cargo:rerun-if-changed=tests/fixtures/nexus.ethos");

    let manifest = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .ok_or("Cargo did not supply CARGO_MANIFEST_DIR to the batch witness build script")?,
    );
    let output = PathBuf::from(
        env::var_os("OUT_DIR")
            .ok_or("Cargo did not supply OUT_DIR to the batch witness build script")?,
    );
    let configuration = fs::read_to_string(manifest.join("tests/fixtures/batch-config.json"))?;
    let source = fs::read_to_string(manifest.join("tests/fixtures/nexus.ethos"))?;
    let generator = BatchConfiguration::from_json(&configuration)?.prepare()?;
    let outcome = generator.generate(&source)?;
    if !outcome.deferred().is_empty() {
        return Err("Nexus build-script generation unexpectedly deferred constructs".into());
    }
    fs::write(output.join("build-script-nexus.rs"), outcome.rust())?;
    fs::write(output.join("build-script-nexus.outcome"), outcome.report())?;
    Ok(())
}
