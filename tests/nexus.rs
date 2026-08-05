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
    NexusTransformation, StreamLifecycleIdentities,
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
    WireDivergence.Result<Left.Sortable Right.Sortable>
    Observer.Stream.(WireEnvelope WireChoice)
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
    "AgentGuardian",
    "Integer",
    "Vector",
    "WireEnvelope",
    "WireProduct",
    "WireChoice",
    "WireResult",
    "WireDivergence",
    "Empty",
    "Batch",
    "Result",
    "Ordered",
    "Left",
    "Right",
    "Sortable",
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
                .with_trait_quality(self.identity("Ordered"))
                .expect("Universal Ordered trait quality")
                .with_trait_quality(self.identity("Sortable"))
                .expect("Universal Sortable trait quality")
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
        let mut entries = FIXTURE_VOCABULARY
            .iter()
            .map(|spelling| {
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
            })
            .collect::<Vec<_>>();
        entries.extend([
            (
                universal(400),
                FixtureRustEmittedIdentifier::try_new("ObserverInitiation")
                    .expect("stream initiation spelling"),
            ),
            (
                universal(401),
                FixtureRustEmittedIdentifier::try_new("ObserverHandle")
                    .expect("stream handle spelling"),
            ),
            (
                universal(402),
                FixtureRustEmittedIdentifier::try_new("ObserverInitiationRefusal")
                    .expect("stream initiation refusal spelling"),
            ),
            (
                universal(403),
                FixtureRustEmittedIdentifier::try_new("ObserverTermination")
                    .expect("stream termination spelling"),
            ),
            (
                universal(404),
                FixtureRustEmittedIdentifier::try_new("ObserverTerminationRefusal")
                    .expect("stream termination refusal spelling"),
            ),
        ]);
        FixtureRustNameProjectionTable::try_from_entries(entries)
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

impl NexusEncodedNameResolver<VocabularyRoot> for RustVocabularyNames {
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
fn sectionless_nexus_ethos_generates_plain_traits_and_decisions_that_compile_and_run() {
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
            "pub struct Entry;\npub struct Referent;\npub struct RecordSet;\npub struct GuardianReason;\n{emitted}\nfn main() {{}}\n"
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
fn strict_interface_stream_lowers_through_nomos_logos_and_rust_without_deferral() {
    let bindings = FixtureBindings::new();
    let ethos = EthosCodec::build(grammar_ids(), bindings.priors())
        .expect("composite Ethos codec")
        .decode(INTERFACE_TYPE_SOURCE, &bindings)
        .expect("current Interface type syntax decodes");
    let outcome = NexusTransformation::new()
        .with_stream_lifecycle_identities(vec![
            StreamLifecycleIdentities::new(
                bindings.identity("Observer"),
                universal(400),
                universal(401),
                universal(402),
                universal(403),
                universal(404),
            )
            .expect("distinct caller-authored stream lifecycle identities"),
        ])
        .expect("one strict stream lifecycle assignment")
        .lower_interface(
            ethos.ethos(),
            &InterfaceRoleIdentities::new(universal(300), universal(301), universal(302))
                .expect("distinct Interface role identities"),
        )
        .expect("lower Interface declarations and strict stream lifecycle");
    let logos = outcome.logos();
    assert_eq!(logos.items().len(), 6);
    for item in logos.items() {
        let attributes = match item {
            WholeLogosItem::Newtype(item) => item.attributes(),
            WholeLogosItem::Struct(item) => item.attributes(),
            WholeLogosItem::Enumeration(item) => item.attributes(),
            WholeLogosItem::StreamLifecycle(lifecycle) => {
                assert_eq!(lifecycle.stream(), &bindings.identity("Observer"));
                assert_eq!(
                    lifecycle.initiation().query(),
                    &nexus_core_logos::WholeLogosTypeReference::Identity(
                        bindings.identity("WireEnvelope")
                    )
                );
                assert_eq!(
                    lifecycle.initiation().success().event(),
                    &nexus_core_logos::WholeLogosTypeReference::Identity(
                        bindings.identity("WireChoice")
                    )
                );
                assert_eq!(lifecycle.initiation().input(), &universal(400));
                assert_eq!(lifecycle.initiation().success().identity(), &universal(401));
                assert_eq!(lifecycle.initiation().refusal(), &universal(402));
                assert_eq!(lifecycle.termination().input(), &universal(403));
                assert_eq!(
                    lifecycle.termination().identity(),
                    lifecycle.initiation().success().identity()
                );
                assert_eq!(lifecycle.termination().refusal(), &universal(404));
                continue;
            }
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
    assert_eq!(emitted.matches("#[rustfmt::skip]").count(), 5, "{emitted}");
    assert_eq!(emitted.matches("rkyv::Archive").count(), 5, "{emitted}");
    assert_eq!(
        emitted.matches("nota::NotaDecodeTraced").count(),
        5,
        "{emitted}"
    );
    assert!(emitted.contains("Batch(Vec<Entry>)"), "{emitted}");
    assert!(
        emitted.contains("pub struct WireResult<Ordered: Ord>(Result<Vec<Ordered>, Error>);"),
        "{emitted}"
    );
    assert!(
        emitted.contains(
            "pub struct WireDivergence<Left: Sortable, Right: Sortable>(Result<Left, Right>);"
        ),
        "{emitted}"
    );
    assert!(
        emitted.contains("pub type ObserverHandle = protos::Stream<WireChoice>;"),
        "{emitted}"
    );
    assert!(
        emitted.contains("pub struct ObserverTermination {\n    pub stream: ObserverHandle,"),
        "{emitted}"
    );

    let temporary = tempfile::tempdir().expect("scratch crate directory");
    fs::create_dir(temporary.path().join("src")).expect("scratch source directory");
    fs::write(
        temporary.path().join("Cargo.toml"),
        "[package]\nname = \"interface-wire-slice\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[features]\nnota-text = []\n\n[dependencies]\nrkyv = { version = \"0.8\", default-features = false, features = [\"std\", \"bytecheck\", \"little_endian\", \"pointer_width_32\", \"unaligned\"] }\nprotos = { git = \"https://github.com/LiGoldragon/protos.git\", rev = \"95aeb1470c549a404518faf1ab0280a36583a2b3\" }\n",
    )
    .expect("scratch manifest with the wire archive contract");
    fs::copy(
        format!("{}/Cargo.lock", env!("CARGO_MANIFEST_DIR")),
        temporary.path().join("Cargo.lock"),
    )
    .expect("scratch lockfile with the pinned Protos source");
    let runtime = r#"
use std::collections::{BTreeMap, VecDeque};
use protos::{StreamEvent as _, StreamOpen as _};

struct ObserverState {
    open: bool,
    events: VecDeque<WireChoice>,
}

struct ObserverRuntime {
    reject_next: bool,
    next_identity: u64,
    streams: BTreeMap<u64, ObserverState>,
}

impl ObserverRuntime {
    fn new(reject_next: bool) -> Self {
        Self {
            reject_next,
            next_identity: 1,
            streams: BTreeMap::new(),
        }
    }

    fn terminate(
        &mut self,
        termination: ObserverTermination,
    ) -> Result<(), ObserverTerminationRefusal> {
        let identity = termination.stream.identity().value();
        let Some(state) = self.streams.get_mut(&identity) else {
            return Err(ObserverTerminationRefusal::UnknownStream);
        };
        if !state.open {
            return Err(ObserverTerminationRefusal::AlreadyClosed);
        }
        state.open = false;
        state.events.clear();
        Ok(())
    }
}

impl protos::StreamOpen for ObserverRuntime {
    type Initiation = ObserverInitiation;
    type Event = WireChoice;
    type InitiationRefusal = ObserverInitiationRefusal;

    fn open(
        &mut self,
        _initiation: Self::Initiation,
    ) -> Result<protos::Stream<Self::Event>, Self::InitiationRefusal> {
        if self.reject_next {
            self.reject_next = false;
            return Err(ObserverInitiationRefusal::InvalidQuery);
        }
        let identity = self.next_identity;
        self.next_identity += 1;
        self.streams.insert(
            identity,
            ObserverState {
                open: true,
                events: VecDeque::from([WireChoice::Empty]),
            },
        );
        Ok(protos::Stream::new(protos::StreamIdentity::new(identity)))
    }
}

impl protos::StreamEvent for ObserverRuntime {
    type Event = WireChoice;

    fn next(&mut self, stream: &protos::Stream<Self::Event>) -> Option<Self::Event> {
        self.streams
            .get_mut(&stream.identity().value())
            .filter(|state| state.open)
            .and_then(|state| state.events.pop_front())
    }
}

fn observer_initiation() -> ObserverInitiation {
    ObserverInitiation {
        query: WireEnvelope(Entry),
    }
}

fn main() {
    let choice = WireChoice::Batch(vec![Entry]);
    let choice_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&choice).unwrap();
    let restored_choice = rkyv::from_bytes::<WireChoice, rkyv::rancor::Error>(&choice_bytes).unwrap();
    assert_eq!(restored_choice, choice);
    let result = WireResult::<Ordered>(Ok(vec![Ordered]));
    let result_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&result).unwrap();
    let restored_result = rkyv::from_bytes::<WireResult<Ordered>, rkyv::rancor::Error>(&result_bytes).unwrap();
    assert_eq!(restored_result, result);

    let mut runtime = ObserverRuntime::new(true);
    assert!(matches!(runtime.open(observer_initiation()), Err(ObserverInitiationRefusal::InvalidQuery)));
    let first = runtime.open(observer_initiation()).unwrap();
    let second = runtime.open(observer_initiation()).unwrap();
    assert_eq!(first.identity().value(), 1);
    assert_eq!(second.identity().value(), 2);
    assert!(matches!(runtime.next(&first), Some(WireChoice::Empty)));
    assert_eq!(runtime.next(&first), None);
    runtime.terminate(ObserverTermination { stream: first.clone() }).unwrap();
    assert!(matches!(
        runtime.terminate(ObserverTermination { stream: first }),
        Err(ObserverTerminationRefusal::AlreadyClosed)
    ));
    assert!(matches!(
        runtime.terminate(ObserverTermination {
            stream: protos::Stream::new(protos::StreamIdentity::new(999)),
        }),
        Err(ObserverTerminationRefusal::UnknownStream)
    ));
}
"#;
    fs::write(
        temporary.path().join("src/main.rs"),
        format!(
            "pub trait Sortable {{}}\n#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]\npub struct Entry;\n#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]\npub struct Ordered;\n#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]\npub struct Left;\nimpl Sortable for Left {{}}\n#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]\npub struct Right;\nimpl Sortable for Right {{}}\n#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Clone, Debug, PartialEq, Eq)]\npub struct Error;\n{emitted}\n{runtime}\n"
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
