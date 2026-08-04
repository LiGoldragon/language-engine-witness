//! Untouched Spirit Nexus fixture through typed Ethos, Nomos, Logos, and Rust.

use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use nexus_core_ethos::{
    EthosCodec, EthosGrammarIdentities, EthosGrammarIds, WholeEthosBuiltinPriors,
};
use nexus_core_logos::{WholeLogos, WholeLogosItem, WholeLogosTypeAttributes};
use nexus_core_nomos::{
    InterfaceRoleIdentities, InterfaceStructuralTransformation, NexusStructuralTransformation,
    NexusTransformation,
};
use nexus_rust_logos::{
    FixtureRustEmittedIdentifier, FixtureRustNameProjectionTable, FixtureRustVocabulary,
    FixtureRustVocabularyIds, RustLogos,
};
use nexus_structural_codec::{
    DeclarationAssignment, DecodeNameBindings, EncodedNameResolver as NexusEncodedNameResolver,
    NameOccurrence, ResolvedReference,
};
use slice_name_table::{LocalEncodedId, Name};
use slice_signal_sema_translator::{VocabularyEncodedId, VocabularyRoot};
use slice_structural_codec::EncodedNameResolver as RustEncodedNameResolver;

const NEXUS_SOURCE: &str = include_str!("fixtures/nexus-strict.ethos");
const INTERFACE_TYPE_SOURCE: &str = r#"Interface.1
[]
{
  []
  []
  []
  [
    WireEnvelope.Entry
    WireProduct.{Entry Vector<Entry>}
    WireChoice.[Empty Batch.Vector<Entry>]
    WireResult.Result<Vector<Ordered> Error>
    Observer.Stream.(WireEnvelope WireProduct WireChoice)
  ]
}
"#;
const FIXTURE_VOCABULARY: &[&str] = &[
    "Entry",
    "Referent",
    "RecordSet",
    "GuardianReason",
    "AdmissionDecision",
    "Accepted",
    "Rejected",
    "GuardianDecision",
    "Admit",
    "Refuse",
    "SignalAdmission",
    "admit",
    "recordDecision",
    "Unit",
    "AgentGuardian",
    "guard",
    "guardReferent",
    "Integer",
    "Vector",
    "WireEnvelope",
    "WireProduct",
    "WireChoice",
    "WireResult",
    "Empty",
    "Batch",
    "Result",
    "Ordered",
    "Error",
    "Observer",
    "Stream",
];

fn encoded(root: VocabularyRoot, local: u16) -> VocabularyEncodedId {
    VocabularyEncodedId::new(root, vec![LocalEncodedId::new(local)])
        .expect("complete fixture identity")
}

fn universal(local: u16) -> VocabularyEncodedId {
    encoded(VocabularyRoot::Universal, local)
}

fn grammar_ids() -> EthosGrammarIds {
    EthosGrammarIds::new(EthosGrammarIdentities {
        interface_document: universal(200),
        nexus_document: universal(201),
        sema_document: universal(202),
        header: universal(203),
        imports: universal(204),
        import_entry: universal(205),
        interface_body: universal(206),
        nexus_body: universal(207),
        sema_body: universal(208),
        newtype_list: universal(209),
        struct_list: universal(210),
        item_list: universal(211),
        trait_list: universal(212),
        table_list: universal(213),
        newtype_declaration: universal(214),
        struct_declaration: universal(215),
        item: universal(216),
        variant: universal(217),
        type_reference: universal(218),
        trait_declaration: universal(220),
        method: universal(221),
        table: universal(222),
    })
    .expect("Universal Ethos grammar identities")
}

struct FixtureBindings {
    by_spelling: BTreeMap<&'static str, VocabularyEncodedId>,
    spellings: BTreeMap<VocabularyEncodedId, Name>,
}

impl FixtureBindings {
    fn new() -> Self {
        let mut by_spelling = BTreeMap::new();
        let mut spellings = BTreeMap::new();
        for (offset, spelling) in FIXTURE_VOCABULARY.iter().copied().enumerate() {
            let identity = universal(1000 + u16::try_from(offset).expect("fixture offset"));
            by_spelling.insert(spelling, identity.clone());
            spellings.insert(identity, Name::new(spelling));
        }
        Self {
            by_spelling,
            spellings,
        }
    }

    fn identity(&self, spelling: &str) -> VocabularyEncodedId {
        self.by_spelling
            .get(spelling)
            .unwrap_or_else(|| panic!("fixture vocabulary contains {spelling}"))
            .clone()
    }

    fn priors(&self) -> WholeEthosBuiltinPriors {
        let mut priors =
            WholeEthosBuiltinPriors::new(self.identity("Integer"), self.identity("Vector"))
                .expect("Universal builtins")
                .with_application_head(self.identity("Result"))
                .expect("Universal Result application head")
                .with_stream_transformer(self.identity("Stream"))
                .expect("Universal Stream transformer");
        for spelling in FIXTURE_VOCABULARY {
            priors = priors
                .with_identity(self.identity(spelling))
                .expect("Universal fixture identity");
        }
        priors
    }

    fn rust_projections(&self) -> FixtureRustNameProjectionTable {
        FixtureRustNameProjectionTable::try_from_entries(FIXTURE_VOCABULARY.iter().map(
            |spelling| {
                let rust_spelling = if *spelling == "Vector" {
                    "Vec"
                } else {
                    *spelling
                };
                (
                    self.identity(spelling),
                    FixtureRustEmittedIdentifier::try_new(rust_spelling)
                        .expect("fixture spelling is a Rust identifier"),
                )
            },
        ))
        .expect("one-to-one Rust projections")
    }
}

impl NexusEncodedNameResolver<VocabularyRoot> for FixtureBindings {
    fn resolve(&self, encoded_id: &VocabularyEncodedId) -> Option<&Name> {
        self.spellings.get(encoded_id)
    }
}

impl DecodeNameBindings<VocabularyRoot> for FixtureBindings {
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

#[derive(Default)]
struct RustVocabularyNames(BTreeMap<VocabularyEncodedId, Name>);

impl RustVocabularyNames {
    fn add(&mut self, identity: VocabularyEncodedId, spelling: &str) {
        self.0.insert(identity, Name::new(spelling));
    }
}

impl RustEncodedNameResolver<VocabularyRoot> for RustVocabularyNames {
    fn resolve(&self, encoded_id: &VocabularyEncodedId) -> Option<&Name> {
        self.0.get(encoded_id)
    }
}

fn rust_logos() -> RustLogos {
    let newtype_item = encoded(VocabularyRoot::Rust, 10);
    let enumeration_item = encoded(VocabularyRoot::Rust, 11);
    let variant = encoded(VocabularyRoot::Rust, 12);
    let tuple_field = encoded(VocabularyRoot::Rust, 13);
    let type_reference = encoded(VocabularyRoot::Rust, 14);
    let struct_keyword = encoded(VocabularyRoot::Rust, 1);
    let enum_keyword = encoded(VocabularyRoot::Rust, 2);
    let public_keyword = encoded(VocabularyRoot::Rust, 3);
    let comma = encoded(VocabularyRoot::Rust, 4);
    let semicolon = encoded(VocabularyRoot::Rust, 5);
    let mut names = RustVocabularyNames::default();
    for (identity, spelling) in [
        (newtype_item.clone(), "NewtypeItemRecord"),
        (enumeration_item.clone(), "EnumerationItemRecord"),
        (variant.clone(), "VariantRecord"),
        (tuple_field.clone(), "TupleFieldRecord"),
        (type_reference.clone(), "TypeReferenceRecord"),
        (struct_keyword.clone(), "struct"),
        (enum_keyword.clone(), "enum"),
        (public_keyword.clone(), "pub"),
        (comma.clone(), ","),
        (semicolon.clone(), ";"),
    ] {
        names.add(identity, spelling);
    }
    RustLogos::new(
        FixtureRustVocabulary::seal(
            FixtureRustVocabularyIds::new(
                newtype_item,
                enumeration_item,
                variant,
                tuple_field,
                type_reference,
                struct_keyword,
                enum_keyword,
                public_keyword,
                comma,
                semicolon,
            ),
            &names,
        )
        .expect("sealed Rust fixture vocabulary"),
    )
}

#[test]
fn untouched_nexus_ethos_generates_plain_traits_and_decisions_that_compile_and_run() {
    let bindings = FixtureBindings::new();
    let ethos = EthosCodec::build(grammar_ids(), bindings.priors())
        .expect("composite Ethos codec")
        .decode(NEXUS_SOURCE, &bindings)
        .expect("untouched Nexus fixture decodes");
    let logos = NexusTransformation::new()
        .lower(ethos.ethos())
        .expect("Nexus structural transformation");
    assert!(matches!(logos.items()[0], WholeLogosItem::TraitDef(_)));
    assert!(matches!(logos.items()[1], WholeLogosItem::TraitDef(_)));
    assert!(matches!(logos.items()[2], WholeLogosItem::Enumeration(_)));
    assert!(matches!(logos.items()[3], WholeLogosItem::Enumeration(_)));

    let emitted = rust_logos()
        .emit_fixture(&logos, &bindings.rust_projections())
        .expect("project Nexus Logos to Rust");
    assert!(emitted.contains("pub trait SignalAdmission"), "{emitted}");
    assert!(emitted.contains("pub trait AgentGuardian"), "{emitted}");
    assert!(
        emitted.contains("fn admit(&self, parameter_0: Entry) -> AdmissionDecision;"),
        "{emitted}"
    );
    assert!(emitted.contains("fn record_decision(&self"), "{emitted}");
    assert!(emitted.contains("fn guard_referent("), "{emitted}");
    assert!(emitted.contains("pub enum AdmissionDecision"), "{emitted}");
    assert!(emitted.contains("pub enum GuardianDecision"), "{emitted}");
    for forbidden in ["rkyv", "derive", "archive_attr"] {
        assert!(
            !emitted.contains(forbidden),
            "plain Nexus output: {emitted}"
        );
    }

    let temporary = tempfile::tempdir().expect("scratch crate directory");
    fs::create_dir(temporary.path().join("src")).expect("scratch source directory");
    fs::write(
        temporary.path().join("Cargo.toml"),
        "[package]\nname = \"nexus-slice\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .expect("scratch manifest");
    fs::write(
        temporary.path().join("src/main.rs"),
        format!(
            "pub struct Entry;\npub struct Referent;\npub struct RecordSet;\npub struct GuardianReason;\npub type Unit = ();\n{emitted}\nstruct Service;\nimpl SignalAdmission for Service {{ fn admit(&self, _entry: Entry) -> AdmissionDecision {{ AdmissionDecision::Accepted }} fn record_decision(&self, _decision: AdmissionDecision) -> Unit {{}} }}\nimpl AgentGuardian for Service {{ fn guard(&self, _entry: Entry, _records: RecordSet) -> GuardianDecision {{ GuardianDecision::Admit }} fn guard_referent(&self, _referent: Referent, _records: RecordSet) -> GuardianDecision {{ GuardianDecision::Admit }} }}\nfn main() {{ let service = Service; assert!(matches!(service.admit(Entry), AdmissionDecision::Accepted)); service.record_decision(AdmissionDecision::Accepted); assert!(matches!(service.guard(Entry, RecordSet), GuardianDecision::Admit)); assert!(matches!(service.guard_referent(Referent, RecordSet), GuardianDecision::Admit)); }}\n"
        ),
    )
    .expect("scratch generated source and behavior harness");
    let run = Command::new("cargo")
        .args(["run", "--quiet", "--jobs", "2"])
        .current_dir(temporary.path())
        .env("CARGO_TARGET_DIR", temporary.path().join("target"))
        .output()
        .expect("run scratch Cargo crate");
    assert!(
        run.status.success(),
        "scratch Cargo stderr:\n{}\ngenerated:\n{emitted}",
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn strict_interface_stream_initiation_defers_through_nomos_before_logos_rust_and_execution() {
    let bindings = FixtureBindings::new();
    let ethos = EthosCodec::build(grammar_ids(), bindings.priors())
        .expect("composite Ethos codec")
        .decode(INTERFACE_TYPE_SOURCE, &bindings)
        .expect("current Interface type syntax decodes");
    let outcome = NexusTransformation::new()
        .lower_interface(
            ethos.ethos(),
            &InterfaceRoleIdentities::new(universal(300), universal(301), universal(302))
                .expect("distinct Interface role identities"),
        )
        .expect("lower Interface declarations and strict stream initiation");
    assert_eq!(outcome.deferred_stream_initiations().len(), 1);
    let initiation = &outcome.deferred_stream_initiations()[0];
    assert_eq!(initiation.stream, bindings.identity("Observer"));
    assert_eq!(
        initiation.query,
        nexus_core_ethos::WholeEthosTypeReference::Identity(bindings.identity("WireEnvelope"))
    );
    assert_eq!(
        initiation.subscription,
        nexus_core_ethos::WholeEthosTypeReference::Identity(bindings.identity("WireProduct"))
    );
    assert_eq!(
        initiation.event,
        nexus_core_ethos::WholeEthosTypeReference::Identity(bindings.identity("WireChoice"))
    );
    let logos = outcome.logos();
    assert_eq!(logos.items().len(), 4);
    for item in logos.items() {
        let attributes = match item {
            WholeLogosItem::Newtype(item) => item.attributes(),
            WholeLogosItem::Struct(item) => item.attributes(),
            WholeLogosItem::Enumeration(item) => item.attributes(),
            WholeLogosItem::TraitDef(_)
            | WholeLogosItem::TraitImpl(_)
            | WholeLogosItem::Table(_) => {
                panic!("Interface shared types contain declarations only")
            }
        };
        assert_eq!(attributes, WholeLogosTypeAttributes::Wire);
    }
    let archived = logos.to_archive_bytes().expect("archive Interface Logos");
    let restored = WholeLogos::from_archive_bytes(&archived).expect("restore Interface Logos");
    assert_eq!(&restored, logos);

    let emitted = rust_logos()
        .emit_fixture(&restored, &bindings.rust_projections())
        .expect("project wire-attributed Logos to Rust");
    assert_eq!(emitted.matches("#[rustfmt::skip]").count(), 4, "{emitted}");
    assert_eq!(emitted.matches("rkyv::Archive").count(), 4, "{emitted}");
    assert_eq!(
        emitted.matches("nota::NotaDecodeTraced").count(),
        4,
        "{emitted}"
    );
    assert!(emitted.contains("Batch(Vec<Entry>)"), "{emitted}");
    assert!(
        emitted.contains("pub struct WireResult(Result<Vec<Ord>, Error>);"),
        "{emitted}"
    );

    let temporary = tempfile::tempdir().expect("scratch crate directory");
    fs::create_dir(temporary.path().join("src")).expect("scratch source directory");
    fs::write(
        temporary.path().join("Cargo.toml"),
        "[package]\nname = \"interface-wire-slice\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[features]\nnota-text = []\n\n[dependencies]\nrkyv = { version = \"0.8\", default-features = false, features = [\"std\", \"bytecheck\", \"little_endian\", \"pointer_width_32\", \"unaligned\"] }\n",
    )
    .expect("scratch manifest with the wire archive contract");
    fs::write(
        temporary.path().join("src/main.rs"),
        format!(
            "#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]\npub struct Entry;\n#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]\npub struct Ord;\n#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]\npub struct Error;\n{emitted}\nfn main() {{ let choice = WireChoice::Batch(vec![Entry]); let choice_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&choice).unwrap(); let restored_choice = rkyv::from_bytes::<WireChoice, rkyv::rancor::Error>(&choice_bytes).unwrap(); assert_eq!(restored_choice, choice); let result = WireResult(Ok(vec![Ord])); let result_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&result).unwrap(); let restored_result = rkyv::from_bytes::<WireResult, rkyv::rancor::Error>(&result_bytes).unwrap(); assert_eq!(restored_result, result); }}\n"
        ),
    )
    .expect("scratch generated wire source");
    let run = Command::new("cargo")
        .args(["run", "--quiet", "--jobs", "2"])
        .current_dir(temporary.path())
        .env("CARGO_TARGET_DIR", temporary.path().join("target"))
        .output()
        .expect("run scratch Interface wire crate");
    assert!(
        run.status.success(),
        "scratch Cargo stderr:\n{}\ngenerated:\n{emitted}",
        String::from_utf8_lossy(&run.stderr)
    );
}
