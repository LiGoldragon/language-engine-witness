//! Process-level witness support only; production behavior lives in component crates.

/// Interface Rust emitted from the sealed Spirit Ethos bundle during this
/// crate's build.  It is compiled below in the same module layout consumed by
/// Nexus and Sema.
pub const BUILD_SCRIPT_INTERFACE_RUST: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-interface.rs"));

/// Nexus Rust emitted from that same complete bundle.
pub const BUILD_SCRIPT_NEXUS_RUST: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-nexus.rs"));

/// Sema Rust emitted from that same complete bundle.
pub const BUILD_SCRIPT_SEMA_RUST: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-sema.rs"));

/// Typed Interface generation receipt.
pub const BUILD_SCRIPT_INTERFACE_OUTCOME: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-interface.outcome"));

/// Typed Nexus generation receipt.
pub const BUILD_SCRIPT_NEXUS_OUTCOME: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-nexus.outcome"));

/// Typed Sema generation receipt.
pub const BUILD_SCRIPT_SEMA_OUTCOME: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-sema.outcome"));

#[allow(dead_code, non_camel_case_types)]
pub mod generated_interface {
    include!(concat!(env!("OUT_DIR"), "/build-script-interface.rs"));
}

#[allow(dead_code, non_camel_case_types)]
pub mod generated_nexus {
    include!(concat!(env!("OUT_DIR"), "/build-script-nexus.rs"));
}

#[allow(dead_code, non_camel_case_types)]
pub mod generated_sema {
    include!(concat!(env!("OUT_DIR"), "/build-script-sema.rs"));
}
