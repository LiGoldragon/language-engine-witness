//! Acceptance of the sealed Spirit Ethos bundle through the strict batch API.

use std::collections::BTreeMap;
use std::fs;
use std::mem::{align_of, size_of};
use std::path::{Path, PathBuf};

use batch_nomos_engine::batch::{
    BatchComponent, BatchConfiguration, OfflineBatchConfiguration, OfflineBatchGeneration,
    PreparedBatchGenerator,
};
use language_engine_witness::{
    BUILD_SCRIPT_INTERFACE_OUTCOME, BUILD_SCRIPT_INTERFACE_RUST, BUILD_SCRIPT_NEXUS_OUTCOME,
    BUILD_SCRIPT_NEXUS_RUST, BUILD_SCRIPT_SEMA_OUTCOME, BUILD_SCRIPT_SEMA_RUST,
    generated_interface, generated_sema,
};
use nexus_core_ethos::{
    EthosCodec, EthosGrammarIdentities, EthosGrammarIds, WholeEthos, WholeEthosBuiltinPriors,
    WholeEthosFileKind,
};
use nexus_structural_codec::{
    DeclarationAssignment, DecodeNameBindings, EncodedNameResolver, NameOccurrence,
    ResolvedReference,
};
use sema_engine::{Engine as SemaEngine, EngineOpen, SchemaVersion, TableSpecification};
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

fn fixture_hex(value: &serde_json::Value, section: &str, field: &str) -> Vec<u8> {
    let text = value[section][field]
        .as_str()
        .unwrap_or_else(|| panic!("frozen current-v14 fixture lacks {section}.{field}"));
    assert_eq!(text.len() % 2, 0, "{section}.{field} has an odd hex length");
    (0..text.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&text[index..index + 2], 16)
                .unwrap_or_else(|error| panic!("{section}.{field} invalid hex: {error}"))
        })
        .collect()
}

fn fixture_usize(value: &serde_json::Value, section: &str, field: &str) -> usize {
    value[section][field]
        .as_u64()
        .unwrap_or_else(|| panic!("frozen current-v14 fixture lacks {section}.{field}"))
        as usize
}

macro_rules! archived_bytes {
    ($value:expr) => {{
        rkyv::to_bytes::<rkyv::rancor::Error>(&$value)
            .expect("archive generated current-v14-compatible value")
            .to_vec()
    }};
}

#[test]
fn frozen_current_v14_archives_cross_restore_the_generated_bundle() {
    // The fixture is produced and locked by the isolated, exact `spirit`
    // 7405eee producer.  That isolation is essential: historical v14 and the
    // current bundle carry incompatible native signal-domain revisions and
    // Cargo rightly refuses both in one process graph.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("current-v14-fixture/archive-fixture.json"))
            .expect("parse frozen current-v14 fixture");
    assert_eq!(
        fixture["source_revision"].as_str(),
        Some("7405eee89e3b1b5b6764eb1a50cbdf467b93c9a7")
    );
    assert_eq!(fixture["store_schema_version"].as_u64(), Some(14));

    let records = &fixture["records"];
    let migrations = &fixture["migrations"];
    assert_eq!(
        <generated_sema::z2VKoj as TableSpecification>::TABLE_NAME.as_str(),
        records["table"].as_str().expect("frozen records table")
    );
    assert_eq!(
        <generated_sema::z2VKoj as TableSpecification>::FAMILY_NAME,
        records["family"].as_str().expect("frozen records family")
    );
    assert_eq!(
        <generated_sema::z2VKoj as TableSpecification>::SCHEMA_HASH.to_string(),
        records["schema_hash"]
            .as_str()
            .expect("frozen records hash")
    );
    assert_eq!(
        <generated_sema::z2VKoi as TableSpecification>::TABLE_NAME.as_str(),
        migrations["table"]
            .as_str()
            .expect("frozen migrations table")
    );
    assert_eq!(
        <generated_sema::z2VKoi as TableSpecification>::FAMILY_NAME,
        migrations["family"]
            .as_str()
            .expect("frozen migrations family")
    );
    assert_eq!(
        <generated_sema::z2VKoi as TableSpecification>::SCHEMA_HASH.to_string(),
        migrations["schema_hash"]
            .as_str()
            .expect("frozen migrations hash")
    );

    assert_eq!(
        size_of::<generated_sema::z2VKoT>(),
        fixture_usize(&fixture, "records", "stored_record_size")
    );
    assert_eq!(
        align_of::<generated_sema::z2VKoT>(),
        fixture_usize(&fixture, "records", "stored_record_align")
    );
    assert_eq!(
        size_of::<generated_interface::z2VKoA>(),
        fixture_usize(&fixture, "records", "record_identifier_size")
    );
    assert_eq!(
        align_of::<generated_interface::z2VKoA>(),
        fixture_usize(&fixture, "records", "record_identifier_align")
    );
    assert_eq!(
        size_of::<generated_interface::z2VKmn>(),
        fixture_usize(&fixture, "records", "entry_size")
    );
    assert_eq!(
        align_of::<generated_interface::z2VKmn>(),
        fixture_usize(&fixture, "records", "entry_align")
    );
    assert_eq!(
        size_of::<generated_sema::z2VKnj>(),
        fixture_usize(&fixture, "migrations", "migration_size")
    );
    assert_eq!(
        align_of::<generated_sema::z2VKnj>(),
        fixture_usize(&fixture, "migrations", "migration_align")
    );
    assert_eq!(
        size_of::<generated_sema::z2VKoN>(),
        fixture_usize(&fixture, "migrations", "source_schema_version_size")
    );
    assert_eq!(
        align_of::<generated_sema::z2VKoN>(),
        fixture_usize(&fixture, "migrations", "source_schema_version_align")
    );

    let stored_record_bytes = fixture_hex(&fixture, "records", "stored_record_hex");
    let record_identifier_bytes = fixture_hex(&fixture, "records", "record_identifier_hex");
    let entry_bytes = fixture_hex(&fixture, "records", "entry_hex");
    let migration_bytes = fixture_hex(&fixture, "migrations", "migration_hex");
    let source_schema_version_bytes =
        fixture_hex(&fixture, "migrations", "source_schema_version_hex");

    let stored_record =
        rkyv::from_bytes::<generated_sema::z2VKoT, rkyv::rancor::Error>(&stored_record_bytes)
            .expect("current-v14 StoredRecord bytes restore generated record");
    assert_eq!(stored_record, generated_sema::preserved_v14_stored_record());
    assert_eq!(archived_bytes!(stored_record), stored_record_bytes);

    let record_identifier = rkyv::from_bytes::<generated_interface::z2VKoA, rkyv::rancor::Error>(
        &record_identifier_bytes,
    )
    .expect("current-v14 RecordIdentifier bytes restore generated key");
    assert_eq!(
        record_identifier,
        generated_interface::preserved_v14_record_identifier()
    );
    assert_eq!(archived_bytes!(record_identifier), record_identifier_bytes);

    let entry = rkyv::from_bytes::<generated_interface::z2VKmn, rkyv::rancor::Error>(&entry_bytes)
        .expect("current-v14 Entry bytes restore generated Entry closure");
    assert_eq!(entry, generated_interface::preserved_v14_entry());
    assert_eq!(archived_bytes!(entry), entry_bytes);

    let migration =
        rkyv::from_bytes::<generated_sema::z2VKnj, rkyv::rancor::Error>(&migration_bytes)
            .expect("current-v14 Migration bytes restore generated migration");
    assert_eq!(migration, generated_sema::preserved_v14_migration());
    assert_eq!(archived_bytes!(migration), migration_bytes);

    let source_schema_version = rkyv::from_bytes::<generated_sema::z2VKoN, rkyv::rancor::Error>(
        &source_schema_version_bytes,
    )
    .expect("current-v14 SourceSchemaVersion bytes restore generated migration key");
    assert_eq!(
        source_schema_version,
        generated_sema::preserved_v14_source_schema_version()
    );
    assert_eq!(
        archived_bytes!(source_schema_version),
        source_schema_version_bytes
    );

    // Register, write, reopen, and query the generated tables.  This proves
    // that the fresh store uses the exact v14 physical descriptors rather
    // than merely reproducing their bytes in memory.
    let temporary = tempfile::tempdir().expect("create generated Sema sandbox");
    let path = temporary.path().join("generated-current-v14.sema");
    let fresh_key = generated_interface::preserved_v14_record_identifier();
    let fresh_record = generated_sema::preserved_v14_stored_record();
    {
        let mut store = SemaEngine::open(EngineOpen::new(&path, SchemaVersion::new(14)))
            .expect("open fresh generated Sema store");
        store
            .register_table(<generated_sema::z2VKoj as TableSpecification>::descriptor())
            .expect("register generated records descriptor");
        store
            .register_table(<generated_sema::z2VKoi as TableSpecification>::descriptor())
            .expect("register generated migrations descriptor");
        store
            .assert_keyed(
                <generated_sema::z2VKoj as TableSpecification>::assertion(
                    &fresh_key,
                    fresh_record.clone(),
                )
                .expect("derive generated records assertion"),
            )
            .expect("write generated current-v14 record");
    }
    let mut reopened = SemaEngine::open(EngineOpen::new(&path, SchemaVersion::new(14)))
        .expect("reopen fresh generated Sema store");
    reopened
        .register_table(<generated_sema::z2VKoj as TableSpecification>::descriptor())
        .expect("re-register generated records descriptor");
    reopened
        .register_table(<generated_sema::z2VKoi as TableSpecification>::descriptor())
        .expect("re-register generated migrations descriptor");
    let found = reopened
        .match_records(
            <generated_sema::z2VKoj as TableSpecification>::query(&fresh_key)
                .expect("derive generated records lookup"),
        )
        .expect("look up generated current-v14 record after reopen");
    assert_eq!(found.records(), &[fresh_record]);
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
