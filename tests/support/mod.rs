pub use signal_domain::bootstrap_authority::DomainRustTypePaths;

use slice_rust_logos::RustLogos;
use slice_signal_sema_translator::VocabularyEncodedId;

pub const SOURCE: &str = signal_domain::DOMAIN_INTERFACE_SOURCE;
pub const SIGNAL_DOMAIN_REVISION: &str = "ee00352781a9af10c60675fc562c378a70fec77b";

/// The witness observes the component's production authority path; it does
/// not recreate the component's catalog or authority state.
pub fn assembly() -> signal_domain::bootstrap_authority::AuthorizedBootstrap {
    signal_domain::bootstrap_authority::domain_bootstrap()
}

pub fn universal(local: u16) -> VocabularyEncodedId {
    signal_domain::bootstrap_authority::universal(local)
}

pub fn declaration(spelling: &str) -> VocabularyEncodedId {
    signal_domain::bootstrap_authority::declaration(spelling)
}

pub fn rust_logos() -> RustLogos {
    signal_domain::bootstrap_authority::rust_logos()
}
