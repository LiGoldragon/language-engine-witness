//! Acceptance of the sealed Spirit Ethos bundle through the strict batch API.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use batch_nomos_engine::batch::{
    BatchComponent, BatchConfiguration, OfflineBatchConfiguration, OfflineBatchGeneration,
    PreparedBatchGenerator,
};
use language_engine_witness::{
    BUILD_SCRIPT_INTERFACE_OUTCOME, BUILD_SCRIPT_INTERFACE_RUST, BUILD_SCRIPT_NEXUS_OUTCOME,
    BUILD_SCRIPT_NEXUS_RUST, BUILD_SCRIPT_SEMA_OUTCOME, BUILD_SCRIPT_SEMA_RUST,
};
use nexus_core_ethos::{
    EthosCodec, EthosGrammarIdentities, EthosGrammarIds, WholeEthos, WholeEthosBuiltinPriors,
    WholeEthosFileKind,
};
use nexus_structural_codec::{
    DeclarationAssignment, DecodeNameBindings, EncodedNameResolver, NameOccurrence,
    ResolvedReference,
};
use slice_name_table::{LocalEncodedId, Name};
use slice_signal_sema_translator::{VocabularyEncodedId, VocabularyRoot};

fn source_root() -> PathBuf {
    std::env::var_os("SPIRIT_ETHOS_SOURCE")
        .map(PathBuf::from)
        .expect("SPIRIT_ETHOS_SOURCE names the sealed source revision")
}

fn source(name: &str) -> String {
    fs::read_to_string(source_root().join(name)).expect("sealed source file")
}

fn configuration() -> serde_json::Value {
    serde_json::from_str(&source("batch-config.json")).expect("sealed batch configuration JSON")
}

fn generator() -> PreparedBatchGenerator {
    BatchConfiguration::from_json(&source("batch-config.json"))
        .expect("sealed batch configuration JSON")
        .prepare()
        .expect("sealed identities seat without allocation")
}

fn generate_bundle(
    generator: &PreparedBatchGenerator,
) -> Vec<batch_nomos_engine::batch::BatchGenerationOutcome> {
    let interface = source("interface.ethos");
    let nexus = source("nexus.ethos");
    let sema = source("sema.ethos");
    generator
        .generate_bundle(&[
            BatchComponent::named("interface", &interface),
            BatchComponent::named("nexus", &nexus),
            BatchComponent::named("sema", &sema),
        ])
        .expect("complete sealed Interface/Nexus/Sema bundle generates")
}

#[test]
fn sealed_configuration_generates_and_compiles_the_complete_bundle() {
    let outcomes = generate_bundle(&generator());
    let [interface, nexus, sema] = outcomes.as_slice() else {
        panic!("one generated artifact per sealed source root")
    };
    assert_eq!(interface.kind(), WholeEthosFileKind::Interface);
    assert_eq!(nexus.kind(), WholeEthosFileKind::Nexus);
    assert_eq!(sema.kind(), WholeEthosFileKind::Sema);
    assert_eq!(interface.version(), 1);
    assert_eq!(nexus.version(), 1);
    assert_eq!(sema.version(), 1);
    assert_eq!(interface.rust(), BUILD_SCRIPT_INTERFACE_RUST);
    assert_eq!(nexus.rust(), BUILD_SCRIPT_NEXUS_RUST);
    assert_eq!(sema.rust(), BUILD_SCRIPT_SEMA_RUST);
    assert!(BUILD_SCRIPT_INTERFACE_OUTCOME.contains("kind Interface"));
    assert!(BUILD_SCRIPT_NEXUS_OUTCOME.contains("kind Nexus"));
    assert!(BUILD_SCRIPT_SEMA_OUTCOME.contains("kind Sema"));
    assert!(interface.rust().contains("impl protos::Input"));
    assert!(interface.rust().contains("protos::Stream"));
    assert!(nexus.rust().contains("crate::generated_interface::"));
    assert_eq!(
        sema.rust()
            .matches("impl sema_engine::TableSpecification for")
            .count(),
        2
    );
    assert!(!interface.rust().contains("Deferred"));
    assert!(!nexus.rust().contains("Deferred"));
    assert!(!sema.rust().contains("Deferred"));
}

struct Bindings {
    by_spelling: BTreeMap<String, VocabularyEncodedId>,
    by_identity: BTreeMap<VocabularyEncodedId, Name>,
}

impl Bindings {
    fn from_configuration(configuration: &serde_json::Value) -> Self {
        let mut by_spelling = BTreeMap::new();
        let mut by_identity = BTreeMap::new();
        for entry in configuration["names"].as_array().expect("names array") {
            let spelling = entry["spelling"]
                .as_str()
                .expect("name spelling")
                .to_owned();
            let local = entry["chain"][0].as_u64().expect("one local identity") as u16;
            let identity = universal(local);
            assert!(
                by_spelling
                    .insert(spelling.clone(), identity.clone())
                    .is_none()
            );
            assert!(by_identity.insert(identity, Name::new(&spelling)).is_none());
        }
        Self {
            by_spelling,
            by_identity,
        }
    }

    fn identity(&self, spelling: &str) -> VocabularyEncodedId {
        self.by_spelling
            .get(spelling)
            .unwrap_or_else(|| panic!("sealed configuration lacks {spelling}"))
            .clone()
    }
}

impl EncodedNameResolver<VocabularyRoot> for Bindings {
    fn resolve(&self, identity: &VocabularyEncodedId) -> Option<&Name> {
        self.by_identity.get(identity)
    }
}

impl DecodeNameBindings<VocabularyRoot> for Bindings {
    fn declaration_assignment(
        &self,
        occurrence: NameOccurrence<'_>,
    ) -> Option<DeclarationAssignment<VocabularyRoot>> {
        self.by_spelling
            .get(occurrence.spelling())
            .cloned()
            .map(DeclarationAssignment::new)
    }

    fn reference_resolution(
        &self,
        occurrence: NameOccurrence<'_>,
    ) -> Option<ResolvedReference<VocabularyRoot>> {
        self.by_spelling
            .get(occurrence.spelling())
            .cloned()
            .map(ResolvedReference::new)
    }
}

fn universal(local: u16) -> VocabularyEncodedId {
    VocabularyEncodedId::new(VocabularyRoot::Universal, vec![LocalEncodedId::new(local)])
        .expect("complete sealed Universal identity")
}

fn grammar_identity(configuration: &serde_json::Value, field: &str) -> VocabularyEncodedId {
    let local = configuration["grammar"][field][0]
        .as_u64()
        .unwrap_or_else(|| panic!("grammar field {field}")) as u16;
    universal(local)
}

fn codec(configuration: &serde_json::Value, bindings: &Bindings) -> EthosCodec {
    let grammar = EthosGrammarIds::new(EthosGrammarIdentities {
        interface_document: grammar_identity(configuration, "interface_document"),
        nexus_document: grammar_identity(configuration, "nexus_document"),
        sema_document: grammar_identity(configuration, "sema_document"),
        header: grammar_identity(configuration, "header"),
        imports: grammar_identity(configuration, "imports"),
        import_entry: grammar_identity(configuration, "import_entry"),
        interface_body: grammar_identity(configuration, "interface_body"),
        nexus_body: grammar_identity(configuration, "nexus_body"),
        sema_body: grammar_identity(configuration, "sema_body"),
        newtype_list: grammar_identity(configuration, "newtype_list"),
        struct_list: grammar_identity(configuration, "struct_list"),
        item_list: grammar_identity(configuration, "item_list"),
        trait_list: grammar_identity(configuration, "trait_list"),
        table_list: grammar_identity(configuration, "table_list"),
        newtype_declaration: grammar_identity(configuration, "newtype_declaration"),
        struct_declaration: grammar_identity(configuration, "struct_declaration"),
        item: grammar_identity(configuration, "item"),
        variant: grammar_identity(configuration, "variant"),
        type_reference: grammar_identity(configuration, "type_reference"),
        trait_declaration: grammar_identity(configuration, "trait_declaration"),
        table: grammar_identity(configuration, "table"),
    })
    .expect("sealed grammar identities are Universal");
    let mut priors =
        WholeEthosBuiltinPriors::new(bindings.identity("Integer"), bindings.identity("Vector"))
            .expect("sealed builtin identities")
            .with_application_head(bindings.identity("Optional"))
            .expect("sealed Optional application head")
            .with_stream_transformer(bindings.identity("Stream"))
            .expect("sealed Stream transformer");
    for identity in bindings.by_spelling.values() {
        priors = priors
            .with_identity(identity.clone())
            .expect("sealed lookup identity");
    }
    EthosCodec::build(grammar, priors).expect("complete Core Ethos codec")
}

#[test]
fn sealed_roots_decode_canonicalize_and_archive_round_trip() {
    let configuration = configuration();
    let bindings = Bindings::from_configuration(&configuration);
    let codec = codec(&configuration, &bindings);
    for root in ["interface.ethos", "nexus.ethos", "sema.ethos"] {
        let original = source(root);
        let decoded = codec
            .decode(&original, &bindings)
            .unwrap_or_else(|error| panic!("{root} decode: {error}"));
        let canonical = codec
            .encode(&decoded, &bindings)
            .unwrap_or_else(|error| panic!("{root} canonical: {error}"));
        let recoded = codec
            .decode(&canonical, &bindings)
            .unwrap_or_else(|error| panic!("{root} canonical decode: {error}"));
        assert_eq!(decoded, recoded, "{root} canonical meaning");
        let archive = decoded
            .ethos()
            .to_archive_bytes()
            .unwrap_or_else(|error| panic!("{root} archive: {error}"));
        let restored = WholeEthos::from_archive_bytes(&archive)
            .unwrap_or_else(|error| panic!("{root} archive restore: {error}"));
        assert_eq!(decoded.ethos(), &restored, "{root} archive meaning");
    }
}

#[test]
fn sealed_source_root_is_not_a_local_fixture_directory() {
    assert_ne!(
        source_root(),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    );
    assert!(source_root().join("allocation-receipt.nota").is_file());
}
