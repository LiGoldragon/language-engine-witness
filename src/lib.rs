//! Process-level witness support only; production behavior lives in the component crates.

/// Nexus Rust emitted by the socket-free Nomos API from this crate's build script.
pub const BUILD_SCRIPT_NEXUS_RUST: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-nexus.rs"));

/// Typed-success receipt emitted alongside the build-script artifact.
pub const BUILD_SCRIPT_NEXUS_OUTCOME: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-nexus.outcome"));

/// Interface Rust emitted by the same socket-free build-script generator.
pub const BUILD_SCRIPT_INTERFACE_RUST: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-interface.rs"));

/// Typed-success receipt for build-script Interface generation.
pub const BUILD_SCRIPT_INTERFACE_OUTCOME: &str =
    include_str!(concat!(env!("OUT_DIR"), "/build-script-interface.outcome"));

/// Observable results from exercising the exact generated Interface module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceBuildWitness {
    /// Number of generated positional memberships proven by generic bounds.
    pub membership_assertions: usize,
    /// Whether an emitted output survived a portable archive round trip.
    pub archive_round_trip: bool,
    /// Whether Refusal's generated Display delegates to its structural Debug.
    pub refusal_display_matches_debug: bool,
}

/// Exercise the exact Interface artifact included from the build-script output.
pub fn exercise_build_script_interface() -> InterfaceBuildWitness {
    generated_interface::exercise()
}

#[allow(dead_code, non_camel_case_types)]
mod generated_interface {
    use interface_protos::{Input as z2VL5p, Output as z2VL5q, Refusal as z2VL5r};

    type z2VL4p = String;
    type z2VL4r<Payload> = Vec<Payload>;
    type z2VL58 = u64;
    type z2VL5H = u64;

    include!(concat!(env!("OUT_DIR"), "/build-script-interface.rs"));

    pub(super) fn exercise() -> super::InterfaceBuildWitness {
        fn assert_input<Value: z2VL5p>() {}
        fn assert_output<Value: z2VL5q>() {}
        fn assert_refusal<Value: z2VL5r>() {}

        assert_input::<z2VL4X>();
        assert_input::<z2VL4Z>();
        assert_input::<z2VL4b>();
        assert_output::<z2VL4d>();
        assert_output::<z2VL4f>();
        assert_output::<z2VL4h>();
        assert_refusal::<z2VL4j>();
        assert_refusal::<z2VL4n>();

        let output = z2VL4d(z2VL4e(19));
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&output)
            .expect("archive generated Interface output");
        let restored = rkyv::from_bytes::<z2VL4d, rkyv::rancor::Error>(&bytes)
            .expect("restore generated Interface output");

        let refusal = z2VL4j {
            field_0: z2VL4k::z2VL5B,
            field_1: z2VL4m("guardian rejected the record".to_owned()),
        };
        let debug = format!("{refusal:?}");
        let error: &dyn std::error::Error = &refusal;
        let refusal_display_matches_debug = error.to_string() == debug && error.source().is_none();

        super::InterfaceBuildWitness {
            membership_assertions: 8,
            archive_round_trip: restored == output,
            refusal_display_matches_debug,
        }
    }
}
