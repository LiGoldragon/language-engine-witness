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

    /// A concrete current-v14-shaped entry used solely by the compatibility
    /// witness.  It lives beside generated types because their newtype fields
    /// are intentionally private to the generated module.
    pub fn preserved_v14_entry() -> z2VKmn {
        z2VKmn {
            field_0: z2VKmj(vec![signal_domain::Domain::Information(
                signal_domain::Information::Documentation,
            )]),
            field_1: z2VKnW::z2VKme,
            field_2: z2VKmf("preserved current-v14 record".to_owned()),
            field_3: z2VKnH(z2VKnc::z2VKnh),
        }
    }

    /// The key paired with [`preserved_v14_entry`].
    pub fn preserved_v14_record_identifier() -> z2VKoA {
        z2VKoA("preserved-current-v14-record".to_owned())
    }

    /// A distinct generated key used to prove a reopened adopted store accepts
    /// new records as well as its frozen current-v14 row.
    pub fn generated_after_v14_reopen_record_identifier() -> z2VKoA {
        z2VKoA("generated-after-current-v14-reopen".to_owned())
    }
}

#[allow(dead_code, non_camel_case_types)]
pub mod generated_nexus {
    include!(concat!(env!("OUT_DIR"), "/build-script-nexus.rs"));
}

#[allow(dead_code, non_camel_case_types)]
pub mod generated_sema {
    include!(concat!(env!("OUT_DIR"), "/build-script-sema.rs"));

    /// A current-v14-shaped stored record assembled within the generated
    /// module, where its generated newtypes remain constructible.
    pub fn preserved_v14_stored_record() -> z2VKoT {
        z2VKoT {
            field_0: crate::generated_interface::preserved_v14_record_identifier(),
            field_1: crate::generated_interface::preserved_v14_entry(),
        }
    }

    /// A fresh generated record with a key not present in the frozen v14
    /// fixture, written after the adopted store has been reopened.
    pub fn generated_after_v14_reopen_stored_record() -> z2VKoT {
        z2VKoT {
            field_0: crate::generated_interface::generated_after_v14_reopen_record_identifier(),
            field_1: crate::generated_interface::preserved_v14_entry(),
        }
    }

    /// The current-v14 migration key.
    pub fn preserved_v14_source_schema_version() -> z2VKoN {
        z2VKoN(14)
    }

    /// A current-v14-shaped migration record.
    pub fn preserved_v14_migration() -> z2VKnj {
        z2VKnj {
            field_0: preserved_v14_source_schema_version(),
            field_1: z2VKni(1),
        }
    }
}
