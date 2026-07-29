use std::{cell::RefCell, collections::BTreeMap, fs, process::Command};

use slice_core_ethos::{
    SixSlotEthosCodec, SixSlotGrammarIds, WholeEthos, WholeEthosBuiltinPriors, WholeEthosItem,
    WholeEthosTypeApplication, WholeEthosTypeReference, WholeEthosVariantPayload,
    WholeEthosVisibility,
};
use slice_core_logos::{
    WholeLogos, WholeLogosEnumeration, WholeLogosItem, WholeLogosNewtype, WholeLogosTupleFields,
    WholeLogosTypeApplication, WholeLogosTypeReference, WholeLogosVariant,
    WholeLogosVariantPayload, WholeLogosVisibility,
};
use slice_core_nomos::SliceOneTransformation;
use slice_name_table::{LocalEncodedId, Name};
use slice_raw_discovery::SourceBound;
use slice_rust_logos::{
    Error as RustLogosError, FixtureRustEmittedIdentifier, FixtureRustNameProjectionTable,
    FixtureRustVocabulary, FixtureRustVocabularyIds, RustLogos,
};
use slice_signal_sema_translator::{VocabularyEncodedId, VocabularyRoot};
use slice_structural_codec::{
    DeclarationAssignment, DecodeNameBindings, EncodedNameResolver, NameOccurrence,
    ResolvedReference,
};

const ETHOS_SOURCE: &str = include_str!("fixtures/slice-one-newtype.ethos");
const MANIFEST: &str = include_str!("../Cargo.toml");

fn issued(root: VocabularyRoot, chain: &[u16]) -> VocabularyEncodedId {
    VocabularyEncodedId::new(
        root,
        chain.iter().copied().map(LocalEncodedId::new).collect(),
    )
    .expect("fixture carries a complete translator-issued chain")
}

#[derive(Default)]
struct Bindings {
    declarations: BTreeMap<(usize, usize), (String, VocabularyEncodedId)>,
    references: BTreeMap<(usize, usize), (String, VocabularyEncodedId)>,
    names: BTreeMap<VocabularyEncodedId, Name>,
    declaration_queries: RefCell<Vec<SourceBound>>,
    reference_queries: RefCell<Vec<SourceBound>>,
}

impl Bindings {
    fn bind_declaration(
        &mut self,
        source: &str,
        spelling: &str,
        occurrence: usize,
        encoded_id: VocabularyEncodedId,
    ) {
        let bound = bound(source, spelling, occurrence);
        self.names.insert(encoded_id.clone(), Name::new(spelling));
        self.declarations.insert(
            (bound.start(), bound.end()),
            (spelling.to_owned(), encoded_id),
        );
    }

    fn bind_reference(
        &mut self,
        source: &str,
        spelling: &str,
        occurrence: usize,
        encoded_id: VocabularyEncodedId,
    ) {
        let bound = bound(source, spelling, occurrence);
        self.names.insert(encoded_id.clone(), Name::new(spelling));
        self.references.insert(
            (bound.start(), bound.end()),
            (spelling.to_owned(), encoded_id),
        );
    }
}

impl EncodedNameResolver<VocabularyRoot> for Bindings {
    fn resolve(&self, encoded_id: &VocabularyEncodedId) -> Option<&Name> {
        self.names.get(encoded_id)
    }
}

impl DecodeNameBindings<VocabularyRoot> for Bindings {
    fn declaration_assignment(
        &self,
        occurrence: NameOccurrence<'_>,
    ) -> Option<DeclarationAssignment<VocabularyRoot>> {
        self.declaration_queries
            .borrow_mut()
            .push(occurrence.bound());
        self.declarations
            .get(&(occurrence.bound().start(), occurrence.bound().end()))
            .filter(|(spelling, _)| spelling == occurrence.spelling())
            .map(|(_, encoded_id)| DeclarationAssignment::new(encoded_id.clone()))
    }

    fn reference_resolution(
        &self,
        occurrence: NameOccurrence<'_>,
    ) -> Option<ResolvedReference<VocabularyRoot>> {
        self.reference_queries.borrow_mut().push(occurrence.bound());
        self.references
            .get(&(occurrence.bound().start(), occurrence.bound().end()))
            .filter(|(spelling, _)| spelling == occurrence.spelling())
            .map(|(_, encoded_id)| ResolvedReference::new(encoded_id.clone()))
    }
}

fn bound(source: &str, spelling: &str, occurrence: usize) -> SourceBound {
    let start = source
        .match_indices(spelling)
        .nth(occurrence)
        .expect("fixture spelling occurrence")
        .0;
    SourceBound::checked(source, start, start + spelling.len()).expect("fixture source bound")
}

fn grammar_ids() -> SixSlotGrammarIds {
    SixSlotGrammarIds::new(
        issued(VocabularyRoot::Universal, &[40, 1]),
        issued(VocabularyRoot::Universal, &[40, 2]),
        issued(VocabularyRoot::Universal, &[40, 3]),
        issued(VocabularyRoot::Universal, &[40, 4]),
        issued(VocabularyRoot::Universal, &[40, 5]),
        issued(VocabularyRoot::Universal, &[40, 6]),
        issued(VocabularyRoot::Universal, &[40, 7]),
    )
    .expect("fixture grammar identities are Universal")
}

#[derive(Clone)]
struct FixtureIdentities {
    newtype: VocabularyEncodedId,
    enumeration: VocabularyEncodedId,
    unit_variant: VocabularyEncodedId,
    single_variant: VocabularyEncodedId,
    batch_variant: VocabularyEncodedId,
    integer: VocabularyEncodedId,
    vector: VocabularyEncodedId,
}

impl FixtureIdentities {
    fn new() -> Self {
        Self {
            newtype: issued(VocabularyRoot::Universal, &[42, 7, 9]),
            enumeration: issued(VocabularyRoot::Universal, &[42, 7, 10]),
            unit_variant: issued(VocabularyRoot::Universal, &[42, 7, 10, 1]),
            single_variant: issued(VocabularyRoot::Universal, &[42, 7, 10, 2]),
            batch_variant: issued(VocabularyRoot::Universal, &[42, 7, 10, 3]),
            integer: issued(VocabularyRoot::Universal, &[3]),
            vector: issued(VocabularyRoot::Universal, &[4]),
        }
    }
}

fn ethos_bindings(ids: &FixtureIdentities) -> Bindings {
    let mut bindings = Bindings::default();
    for (spelling, encoded_id) in [
        ("Identifiers", ids.newtype.clone()),
        ("Status", ids.enumeration.clone()),
        ("Pending", ids.unit_variant.clone()),
        ("Ready", ids.single_variant.clone()),
        ("Batch", ids.batch_variant.clone()),
    ] {
        bindings.bind_declaration(ETHOS_SOURCE, spelling, 0, encoded_id);
    }
    for occurrence in 0..2 {
        bindings.bind_reference(ETHOS_SOURCE, "Vector", occurrence, ids.vector.clone());
    }
    for occurrence in 0..4 {
        bindings.bind_reference(ETHOS_SOURCE, "Integer", occurrence, ids.integer.clone());
    }
    bindings
}

fn rust_codec() -> RustLogos {
    let newtype_item = issued(VocabularyRoot::Rust, &[10]);
    let enumeration_item = issued(VocabularyRoot::Rust, &[11]);
    let variant = issued(VocabularyRoot::Rust, &[12]);
    let tuple_field = issued(VocabularyRoot::Rust, &[13]);
    let type_reference = issued(VocabularyRoot::Rust, &[14]);
    let struct_keyword = issued(VocabularyRoot::Rust, &[1]);
    let enum_keyword = issued(VocabularyRoot::Rust, &[2]);
    let public_keyword = issued(VocabularyRoot::Rust, &[3]);
    let comma = issued(VocabularyRoot::Rust, &[4]);
    let semicolon = issued(VocabularyRoot::Rust, &[5]);
    let mut names = BTreeMap::new();
    for (encoded_id, spelling) in [
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
        names.insert(encoded_id, Name::new(spelling));
    }

    struct RustNames(BTreeMap<VocabularyEncodedId, Name>);
    impl EncodedNameResolver<VocabularyRoot> for RustNames {
        fn resolve(&self, encoded_id: &VocabularyEncodedId) -> Option<&Name> {
            self.0.get(encoded_id)
        }
    }

    let vocabulary = FixtureRustVocabulary::seal(
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
        &RustNames(names),
    )
    .expect("fixture Rust vocabulary is sealed");
    RustLogos::new(vocabulary)
}

fn projections(ids: &FixtureIdentities) -> FixtureRustNameProjectionTable {
    FixtureRustNameProjectionTable::try_from_entries([
        (
            ids.newtype.clone(),
            FixtureRustEmittedIdentifier::try_new("Id16").expect("opaque Rust identifier"),
        ),
        (
            ids.enumeration.clone(),
            FixtureRustEmittedIdentifier::try_new("Id17").expect("opaque Rust identifier"),
        ),
        (
            ids.unit_variant.clone(),
            FixtureRustEmittedIdentifier::try_new("Id171").expect("opaque Rust identifier"),
        ),
        (
            ids.single_variant.clone(),
            FixtureRustEmittedIdentifier::try_new("Id172").expect("opaque Rust identifier"),
        ),
        (
            ids.batch_variant.clone(),
            FixtureRustEmittedIdentifier::try_new("Id173").expect("opaque Rust identifier"),
        ),
        (
            ids.integer.clone(),
            FixtureRustEmittedIdentifier::try_new("u64").expect("opaque Rust identifier"),
        ),
        (
            ids.vector.clone(),
            FixtureRustEmittedIdentifier::try_new("Vec").expect("opaque Rust identifier"),
        ),
    ])
    .expect("fixture projections are one-to-one")
}

fn emitted_bindings(source: &str, ids: &FixtureIdentities) -> Bindings {
    let mut bindings = Bindings::default();
    for (spelling, encoded_id) in [
        ("Id16", ids.newtype.clone()),
        ("Id17", ids.enumeration.clone()),
        ("Id171", ids.unit_variant.clone()),
        ("Id172", ids.single_variant.clone()),
        ("Id173", ids.batch_variant.clone()),
    ] {
        bindings.bind_declaration(source, spelling, 0, encoded_id);
    }
    for occurrence in 0..2 {
        bindings.bind_reference(source, "Vec", occurrence, ids.vector.clone());
    }
    for occurrence in 0..4 {
        bindings.bind_reference(source, "u64", occurrence, ids.integer.clone());
    }
    bindings
}

fn expected_logos(ids: &FixtureIdentities) -> WholeLogos {
    let vector_integer = WholeLogosTypeReference::Application(WholeLogosTypeApplication::new(
        ids.vector.clone(),
        WholeLogosTypeReference::Identity(ids.integer.clone()),
    ));
    WholeLogos::new(vec![
        WholeLogosItem::Newtype(WholeLogosNewtype::new(
            WholeLogosVisibility::Public,
            ids.newtype.clone(),
            WholeLogosVisibility::Private,
            vector_integer.clone(),
        )),
        WholeLogosItem::Enumeration(WholeLogosEnumeration::new(
            WholeLogosVisibility::Public,
            ids.enumeration.clone(),
            vec![
                WholeLogosVariant::new(ids.unit_variant.clone(), WholeLogosVariantPayload::Unit),
                WholeLogosVariant::new(
                    ids.single_variant.clone(),
                    WholeLogosVariantPayload::Tuple(
                        WholeLogosTupleFields::new(vec![WholeLogosTypeReference::Identity(
                            ids.integer.clone(),
                        )])
                        .expect("single-field tuple"),
                    ),
                ),
                WholeLogosVariant::new(
                    ids.batch_variant.clone(),
                    WholeLogosVariantPayload::Tuple(
                        WholeLogosTupleFields::new(vec![
                            vector_integer,
                            WholeLogosTypeReference::Identity(ids.integer.clone()),
                        ])
                        .expect("two-field tuple"),
                    ),
                ),
            ],
        )),
    ])
}

fn assert_six_source_bounds(bounds: &slice_core_ethos::SixSlotSourceBounds) {
    for (bound, expected) in [
        (bounds.imports(), "{}"),
        (bounds.input(), "[]"),
        (bounds.output(), "[]"),
        (
            bounds.types(),
            "{\n  Identifiers.Vector.Integer\n  Status.{Pending Ready.{Integer} Batch.{Vector.Integer Integer}}\n}",
        ),
        (bounds.generics(), "{}"),
        (bounds.impls(), "{}"),
    ] {
        assert_eq!(&ETHOS_SOURCE[bound.start()..bound.end()], expected);
    }
}

fn assert_query_bounds(actual: &[SourceBound], expected: &[SourceBound]) {
    assert!(
        actual.iter().all(|bound| expected.contains(bound)),
        "the evaluator queried an unexpected source bound: {actual:?}"
    );
    assert!(
        expected.iter().all(|bound| actual.contains(bound)),
        "the evaluator skipped an expected source bound: {actual:?}"
    );
}

#[test]
fn six_slot_ethos_lowers_and_emits_working_enum_and_application_shapes() {
    let ids = FixtureIdentities::new();
    let bindings = ethos_bindings(&ids);
    let codec = SixSlotEthosCodec::build(
        grammar_ids(),
        WholeEthosBuiltinPriors::new(ids.integer.clone(), ids.vector.clone())
            .expect("fixture builtin priors are Universal"),
    )
    .expect("six-slot structuretree seals");
    let decoded = codec
        .decode(ETHOS_SOURCE, &bindings)
        .expect("typed six-slot Ethos decode");
    assert_six_source_bounds(decoded.source_bounds());
    assert_query_bounds(
        bindings.declaration_queries.borrow().as_slice(),
        &[
            bound(ETHOS_SOURCE, "Identifiers", 0),
            bound(ETHOS_SOURCE, "Status", 0),
            bound(ETHOS_SOURCE, "Pending", 0),
            bound(ETHOS_SOURCE, "Ready", 0),
            bound(ETHOS_SOURCE, "Batch", 0),
        ],
    );
    assert_query_bounds(
        bindings.reference_queries.borrow().as_slice(),
        &[
            bound(ETHOS_SOURCE, "Vector", 0),
            bound(ETHOS_SOURCE, "Integer", 0),
            bound(ETHOS_SOURCE, "Integer", 1),
            bound(ETHOS_SOURCE, "Vector", 1),
            bound(ETHOS_SOURCE, "Integer", 2),
            bound(ETHOS_SOURCE, "Integer", 3),
        ],
    );

    let [
        WholeEthosItem::Newtype(ethos_newtype),
        WholeEthosItem::Enumeration(ethos_enumeration),
    ] = decoded.ethos().items()
    else {
        panic!("one application-backed newtype followed by one enumeration")
    };
    assert_eq!(ethos_newtype.name(), &ids.newtype);
    assert_eq!(ethos_newtype.visibility(), &WholeEthosVisibility::Public);
    assert!(ethos_newtype.attributes().is_empty());
    assert_eq!(
        ethos_newtype.wrapped_field().visibility(),
        &WholeEthosVisibility::Private
    );
    assert_eq!(
        ethos_newtype.wrapped_field().reference(),
        &WholeEthosTypeReference::Application(WholeEthosTypeApplication::new(
            ids.vector.clone(),
            WholeEthosTypeReference::Identity(ids.integer.clone()),
        ))
    );
    assert_eq!(ethos_enumeration.name(), &ids.enumeration);
    assert_eq!(ethos_enumeration.variants().len(), 3);
    assert_eq!(ethos_enumeration.variants()[0].name(), &ids.unit_variant);
    assert_eq!(
        ethos_enumeration.variants()[0].payload(),
        &WholeEthosVariantPayload::Unit
    );
    assert_eq!(ethos_enumeration.variants()[1].name(), &ids.single_variant);
    let WholeEthosVariantPayload::Tuple(single_fields) = ethos_enumeration.variants()[1].payload()
    else {
        panic!("Ready has one positional field")
    };
    assert_eq!(
        single_fields.fields(),
        &[WholeEthosTypeReference::Identity(ids.integer.clone())]
    );
    assert_eq!(ethos_enumeration.variants()[2].name(), &ids.batch_variant);
    let WholeEthosVariantPayload::Tuple(batch_fields) = ethos_enumeration.variants()[2].payload()
    else {
        panic!("Batch has two positional fields")
    };
    assert_eq!(
        batch_fields.fields(),
        &[
            WholeEthosTypeReference::Application(WholeEthosTypeApplication::new(
                ids.vector.clone(),
                WholeEthosTypeReference::Identity(ids.integer.clone()),
            )),
            WholeEthosTypeReference::Identity(ids.integer.clone()),
        ]
    );

    let ethos_archive = decoded
        .ethos()
        .to_archive_bytes()
        .expect("archive whole Ethos");
    let restored_ethos =
        WholeEthos::from_archive_bytes(&ethos_archive).expect("restore whole Ethos");
    assert_eq!(&restored_ethos, decoded.ethos());

    let logos = SliceOneTransformation::new().lower(&restored_ethos);
    assert_eq!(logos, expected_logos(&ids));
    let whole_identity = logos
        .content_identity()
        .expect("whole Logos has a pure-content identity");
    let logos_archive = logos.to_archive_bytes().expect("archive whole Logos");
    let restored_logos =
        WholeLogos::from_archive_bytes(&logos_archive).expect("restore whole Logos");
    assert_eq!(restored_logos, logos);
    assert_eq!(
        restored_logos
            .content_identity()
            .expect("restored whole Logos identity"),
        whole_identity
    );

    let rust = rust_codec();
    let emitted = rust
        .emit_fixture(&restored_logos, &projections(&ids))
        .expect("structural Rust emission");
    assert_eq!(
        rust.decode_fixture(&emitted, &emitted_bindings(&emitted, &ids))
            .expect("structural Rust decode"),
        restored_logos
    );

    let temporary = tempfile::tempdir().expect("scratch crate directory");
    fs::create_dir(temporary.path().join("src")).expect("scratch source directory");
    fs::write(
        temporary.path().join("Cargo.toml"),
        "[package]\nname=\"slice-one-generated\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
    )
    .expect("scratch manifest");
    fs::write(
        temporary.path().join("src/main.rs"),
        format!(
            "{emitted}\nfn score(value: Id17) -> usize {{ match value {{ Id17::Id171 => 1, Id17::Id172(number) => number as usize, Id17::Id173(values, number) => values.len() + number as usize, }} }}\nfn main() {{ let wrapped = Id16(vec![1, 2, 3]); assert_eq!(wrapped.0.len(), 3); assert_eq!(score(Id17::Id171), 1); assert_eq!(score(Id17::Id172(41)), 41); let batch = score(Id17::Id173(vec![1, 2], 40)); assert_eq!(batch, 42); println!(\"{{}} {{}}\", wrapped.0.len(), batch); }}\n"
        ),
    )
    .expect("scratch generated source");
    let run = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--offline",
            "--manifest-path",
            temporary
                .path()
                .join("Cargo.toml")
                .to_str()
                .expect("UTF-8 scratch path"),
        ])
        .env("CARGO_TARGET_DIR", temporary.path().join("target"))
        .output()
        .expect("run scratch Cargo build");
    assert!(
        run.status.success(),
        "scratch Cargo stderr:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8(run.stdout).expect("scratch output is UTF-8"),
        "3 42\n"
    );
}

#[test]
fn incomplete_projection_refuses_without_returning_partial_rust() {
    let ids = FixtureIdentities::new();
    let incomplete = FixtureRustNameProjectionTable::try_from_entries([(
        ids.newtype.clone(),
        FixtureRustEmittedIdentifier::try_new("Id16").expect("opaque Rust identifier"),
    )])
    .expect("an incomplete fixture table is representable");

    assert!(matches!(
        rust_codec().emit_fixture(&expected_logos(&ids), &incomplete),
        Err(RustLogosError::MissingProjection { .. })
    ));
}

#[test]
fn vertical_slice_pins_the_published_producers() {
    for revision in [
        "7290f65bbb5e7825ab2ca58340631d154d69d110",
        "31d2e4c14943802ce75a910edd54ea1796273f78",
        "0840dbd96d250b15b750b5b17a3a7c86bacfb7ee",
        "e4cefc413cfafbe589e64d961fba34457d214316",
        "f7071fb9f8879d17cd2436ed14894799958e9b08",
        "c8e4ebc16dbea75880b3034a7c46cb6812ab4ef7",
    ] {
        assert!(
            MANIFEST.contains(revision),
            "the vertical slice must pin published producer {revision}"
        );
    }
}
