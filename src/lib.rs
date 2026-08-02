//! Process-level witness support only; production behavior lives in the component crates.

/// Nexus Rust emitted by the socket-free Nomos API from this crate's build script.
pub const BUILD_SCRIPT_NEXUS_RUST: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-nexus.rs"));

/// Typed-success receipt emitted alongside the build-script artifact.
pub const BUILD_SCRIPT_NEXUS_OUTCOME: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-nexus.outcome"));
