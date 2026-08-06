use std::{collections::BTreeMap, fs, process::Command};

use slice_core_ethos::bootstrap::{
    BootstrapCatalog, BootstrapGrammarIdentities, BootstrapPriorIdentities,
    BootstrapPriorVocabulary, BootstrapVersionPolicy, CanonicalIdentityOrder, EthosKind,
    EthosVersion, IdentitySchema, IdentitySchemaCatalog, InterfaceRole, NomosSchema, SchemaRole,
    TextualMetadataRecord, TextualMetadataSnapshot, TextualProjectionAddress,
};
use slice_core_logos::{
    WholeLogos, WholeLogosEnumeration, WholeLogosItem, WholeLogosNewtype, WholeLogosTupleFields,
    WholeLogosTypeApplication, WholeLogosTypeReference, WholeLogosVariant,
    WholeLogosVariantPayload, WholeLogosVisibility,
};
use slice_core_nomos::BootstrapSliceOneLowering;
use slice_name_table::{LocalEncodedId, Name};
use slice_raw_discovery::SourceBound;
use slice_rust_logos::{
    Error as RustLogosError, FixtureRustEmittedIdentifier, FixtureRustNameProjectionTable,
    FixtureRustVocabulary, FixtureRustVocabularyIds, RustLogos,
};
use slice_sema_translator::bootstrap::{
    AuthorizedBootstrapTransition, BootstrapAuthorityIdentity, BootstrapAuthorityRevision,
    BootstrapTransactionAssembler, VerifiedBootstrapAssembly,
};
use slice_signal_sema_translator::{VocabularyEncodedId, VocabularyRoot};
use slice_structural_codec::{
    DeclarationAssignment, DecodeNameBindings, EncodedNameResolver, NameOccurrence,
    ResolvedReference,
};

const NEXUS_SOURCE: &str = include_str!("fixtures/slice-one-newtype.ethos");
const MANIFEST: &str = include_str!("../Cargo.toml");
const MODULE_PATH: &[&str] = &["slice_one"];

fn issued(root: VocabularyRoot, local: u16) -> VocabularyEncodedId {
    VocabularyEncodedId::new(root, vec![LocalEncodedId::new(local)])
        .expect("fixture carries one explicit nonempty identity seat")
}

fn universal(local: u16) -> VocabularyEncodedId {
    issued(VocabularyRoot::Universal, local)
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
            newtype: universal(100),
            enumeration: universal(101),
            unit_variant: universal(102),
            single_variant: universal(103),
            batch_variant: universal(104),
            integer: universal(8),
            vector: universal(11),
        }
    }
}

fn record(
    owner: Option<VocabularyEncodedId>,
    spelling: &str,
    identity: VocabularyEncodedId,
) -> TextualMetadataRecord {
    TextualMetadataRecord {
        address: TextualProjectionAddress {
            module_path: MODULE_PATH.iter().map(|part| (*part).to_owned()).collect(),
            lexical_owner: owner,
            visible_name: spelling.to_owned(),
        },
        encoded_name: identity,
    }
}

fn fixed_specifications() -> Vec<(u16, &'static str, Vec<SchemaRole>)> {
    vec![
        (
            1,
            "Interface",
            vec![SchemaRole::FileKind(EthosKind::Interface)],
        ),
        (2, "Nexus", vec![SchemaRole::FileKind(EthosKind::Nexus)]),
        (3, "Sema", vec![SchemaRole::FileKind(EthosKind::Sema)]),
        (
            4,
            "Input",
            vec![SchemaRole::InterfaceRole(InterfaceRole::Input)],
        ),
        (
            5,
            "Output",
            vec![SchemaRole::InterfaceRole(InterfaceRole::Output)],
        ),
        (
            6,
            "Refusal",
            vec![SchemaRole::InterfaceRole(InterfaceRole::Refusal)],
        ),
        (7, "String", vec![SchemaRole::Nominal { persistent: true }]),
        (8, "Integer", vec![SchemaRole::Nominal { persistent: true }]),
        (9, "Boolean", vec![SchemaRole::Nominal { persistent: true }]),
        (10, "Unit", vec![SchemaRole::Nominal { persistent: true }]),
        (11, "Vector", vec![SchemaRole::Shape { arity: 1 }]),
        (12, "Option", vec![SchemaRole::Shape { arity: 1 }]),
        (13, "Map", vec![SchemaRole::Shape { arity: 2 }]),
        (14, "Result", vec![SchemaRole::Shape { arity: 2 }]),
        (
            15,
            "Stream",
            vec![
                SchemaRole::Shape { arity: 1 },
                SchemaRole::Nomos(NomosSchema::StreamInitiation { arity: 2 }),
            ],
        ),
        (16, "StreamIdentity", vec![SchemaRole::Shape { arity: 1 }]),
    ]
}

fn catalog() -> BootstrapCatalog {
    let specifications = fixed_specifications();
    let metadata = TextualMetadataSnapshot::new(
        specifications
            .iter()
            .map(|(local, spelling, _)| record(None, spelling, universal(*local)))
            .collect(),
    )
    .expect("fixture prior metadata is exact");
    let schemas = IdentitySchemaCatalog::new(
        specifications
            .iter()
            .map(|(local, _, roles)| {
                IdentitySchema::new(universal(*local), roles.clone())
                    .expect("fixture prior schema is coherent")
            })
            .collect(),
    )
    .expect("fixture prior identities are unique");
    let priors = BootstrapPriorVocabulary::new(
        BootstrapPriorIdentities {
            interface_kind: universal(1),
            nexus_kind: universal(2),
            sema_kind: universal(3),
            input_role: universal(4),
            output_role: universal(5),
            refusal_role: universal(6),
            string_type: universal(7),
            integer_type: universal(8),
            boolean_type: universal(9),
            unit_type: universal(10),
            vector_shape: universal(11),
            option_shape: universal(12),
            map_shape: universal(13),
            result_shape: universal(14),
            stream_nomos: universal(15),
            stream_shape: universal(15),
            stream_identity_shape: universal(16),
        },
        &schemas,
        &metadata,
    )
    .expect("fixture prior roles are exact");
    let order = CanonicalIdentityOrder::new(
        specifications
            .iter()
            .map(|(local, _, _)| (universal(*local), vec![0x10, *local as u8])),
    )
    .expect("fixture prior order is unique");
    BootstrapCatalog::new(
        MODULE_PATH.iter().map(|part| (*part).to_owned()).collect(),
        metadata,
        schemas,
        priors,
        BootstrapVersionPolicy::exact(EthosVersion::new(1, 0, 0)),
        order,
    )
    .expect("fixture bootstrap catalog is complete")
}

fn assembly(ids: &FixtureIdentities) -> VerifiedBootstrapAssembly {
    let catalog = catalog();
    let records = [
        record(None, "Identifiers", ids.newtype.clone()),
        record(None, "Status", ids.enumeration.clone()),
        record(
            Some(ids.enumeration.clone()),
            "Pending",
            ids.unit_variant.clone(),
        ),
        record(
            Some(ids.enumeration.clone()),
            "Ready",
            ids.single_variant.clone(),
        ),
        record(
            Some(ids.enumeration.clone()),
            "Batch",
            ids.batch_variant.clone(),
        ),
    ];
    let mut after = catalog.metadata().records().to_vec();
    after.extend(records);
    let transition = AuthorizedBootstrapTransition::new(
        TextualMetadataSnapshot::new(after).expect("fixture authority transition is exact"),
        [
            (ids.newtype.clone(), vec![0x80, 0, 100]),
            (ids.enumeration.clone(), vec![0x80, 0, 101]),
            (ids.unit_variant.clone(), vec![0x80, 0, 102]),
            (ids.single_variant.clone(), vec![0x80, 0, 103]),
            (ids.batch_variant.clone(), vec![0x80, 0, 104]),
        ]
        .into_iter()
        .collect(),
        BTreeMap::new(),
    );
    BootstrapTransactionAssembler::new(
        BootstrapAuthorityIdentity::new([0x51; 32]),
        BootstrapAuthorityRevision::new(1),
        BootstrapGrammarIdentities {
            document: universal(900),
            syntax: universal(901),
        },
        catalog,
    )
    .assemble(NEXUS_SOURCE, transition)
    .expect("Nexus header/imports/body seals through the fixture authority")
}

fn rust_codec() -> RustLogos {
    let newtype_item = issued(VocabularyRoot::Rust, 10);
    let enumeration_item = issued(VocabularyRoot::Rust, 11);
    let variant = issued(VocabularyRoot::Rust, 12);
    let tuple_field = issued(VocabularyRoot::Rust, 13);
    let type_reference = issued(VocabularyRoot::Rust, 14);
    let struct_keyword = issued(VocabularyRoot::Rust, 1);
    let enum_keyword = issued(VocabularyRoot::Rust, 2);
    let public_keyword = issued(VocabularyRoot::Rust, 3);
    let comma = issued(VocabularyRoot::Rust, 4);
    let semicolon = issued(VocabularyRoot::Rust, 5);
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
            &RustNames(names),
        )
        .expect("fixture Rust vocabulary is sealed"),
    )
}

fn projections(ids: &FixtureIdentities) -> FixtureRustNameProjectionTable {
    FixtureRustNameProjectionTable::try_from_entries([
        (
            ids.newtype.clone(),
            FixtureRustEmittedIdentifier::try_new("Id16").unwrap(),
        ),
        (
            ids.enumeration.clone(),
            FixtureRustEmittedIdentifier::try_new("Id17").unwrap(),
        ),
        (
            ids.unit_variant.clone(),
            FixtureRustEmittedIdentifier::try_new("Id171").unwrap(),
        ),
        (
            ids.single_variant.clone(),
            FixtureRustEmittedIdentifier::try_new("Id172").unwrap(),
        ),
        (
            ids.batch_variant.clone(),
            FixtureRustEmittedIdentifier::try_new("Id173").unwrap(),
        ),
        (
            ids.integer.clone(),
            FixtureRustEmittedIdentifier::try_new("u64").unwrap(),
        ),
        (
            ids.vector.clone(),
            FixtureRustEmittedIdentifier::try_new("Vec").unwrap(),
        ),
    ])
    .expect("fixture projections are one-to-one")
}

#[derive(Default)]
struct Bindings {
    declarations: BTreeMap<(usize, usize), (String, VocabularyEncodedId)>,
    references: BTreeMap<(usize, usize), (String, VocabularyEncodedId)>,
    names: BTreeMap<VocabularyEncodedId, Name>,
}

impl EncodedNameResolver<VocabularyRoot> for Bindings {
    fn resolve(&self, encoded_id: &VocabularyEncodedId) -> Option<&Name> {
        self.names.get(encoded_id)
    }
}

impl Bindings {
    fn bind(
        table: &mut BTreeMap<(usize, usize), (String, VocabularyEncodedId)>,
        source: &str,
        spelling: &str,
        occurrence: usize,
        identity: VocabularyEncodedId,
    ) {
        let start = source
            .match_indices(spelling)
            .nth(occurrence)
            .expect("emitted fixture occurrence")
            .0;
        let bound = SourceBound::checked(source, start, start + spelling.len())
            .expect("emitted source bound");
        table.insert(
            (bound.start(), bound.end()),
            (spelling.to_owned(), identity),
        );
    }
}

impl DecodeNameBindings<VocabularyRoot> for Bindings {
    fn declaration_assignment(
        &self,
        occurrence: NameOccurrence<'_>,
    ) -> Option<DeclarationAssignment<VocabularyRoot>> {
        self.declarations
            .get(&(occurrence.bound().start(), occurrence.bound().end()))
            .filter(|(spelling, _)| spelling == occurrence.spelling())
            .map(|(_, identity)| DeclarationAssignment::new(identity.clone()))
    }

    fn reference_resolution(
        &self,
        occurrence: NameOccurrence<'_>,
    ) -> Option<ResolvedReference<VocabularyRoot>> {
        self.references
            .get(&(occurrence.bound().start(), occurrence.bound().end()))
            .filter(|(spelling, _)| spelling == occurrence.spelling())
            .map(|(_, identity)| ResolvedReference::new(identity.clone()))
    }
}

fn emitted_bindings(source: &str, ids: &FixtureIdentities) -> Bindings {
    let mut bindings = Bindings::default();
    for (spelling, identity) in [
        ("Id16", ids.newtype.clone()),
        ("Id17", ids.enumeration.clone()),
        ("Id171", ids.unit_variant.clone()),
        ("Id172", ids.single_variant.clone()),
        ("Id173", ids.batch_variant.clone()),
    ] {
        Bindings::bind(&mut bindings.declarations, source, spelling, 0, identity);
    }
    for occurrence in 0..2 {
        Bindings::bind(
            &mut bindings.references,
            source,
            "Vec",
            occurrence,
            ids.vector.clone(),
        );
    }
    for occurrence in 0..4 {
        Bindings::bind(
            &mut bindings.references,
            source,
            "u64",
            occurrence,
            ids.integer.clone(),
        );
    }
    bindings.names.extend([
        (ids.newtype.clone(), Name::new("Id16")),
        (ids.enumeration.clone(), Name::new("Id17")),
        (ids.unit_variant.clone(), Name::new("Id171")),
        (ids.single_variant.clone(), Name::new("Id172")),
        (ids.batch_variant.clone(), Name::new("Id173")),
        (ids.integer.clone(), Name::new("u64")),
        (ids.vector.clone(), Name::new("Vec")),
    ]);
    bindings
}

fn expected_logos(ids: &FixtureIdentities) -> WholeLogos {
    let vector_integer = WholeLogosTypeReference::Application(
        WholeLogosTypeApplication::new(
            ids.vector.clone(),
            vec![WholeLogosTypeReference::Identity(ids.integer.clone())],
        )
        .expect("Vector application carries its Integer argument"),
    );
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

#[test]
fn nexus_bootstrap_lowers_and_runs_unit_unary_and_product_semantics() {
    let ids = FixtureIdentities::new();
    let assembly = assembly(&ids);
    assert_eq!(assembly.canonical_source(), NEXUS_SOURCE);

    let logos = BootstrapSliceOneLowering::new()
        .lower(assembly.reader(), assembly.transaction())
        .expect("current Nomos lowers the authority-sealed Nexus transaction");
    assert_eq!(logos, expected_logos(&ids));
    let whole_identity = logos.content_identity().expect("whole Logos identity");
    let archive = logos.to_archive_bytes().expect("archive whole Logos");
    let restored = WholeLogos::from_archive_bytes(&archive).expect("restore whole Logos");
    assert_eq!(restored, logos);
    assert_eq!(
        restored.content_identity().expect("restored identity"),
        whole_identity
    );

    let rust = rust_codec();
    let emitted = rust
        .emit_fixture(&restored, &projections(&ids))
        .expect("structural Rust projection");
    assert_eq!(
        rust.decode_fixture(&emitted, &emitted_bindings(&emitted, &ids))
            .expect("structural Rust round-trip"),
        restored
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
    assert_eq!(String::from_utf8(run.stdout).unwrap(), "3 42\n");
}

#[test]
fn incomplete_projection_refuses_without_partial_rust() {
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
fn vertical_slice_pins_the_published_bootstrap_train() {
    for revision in [
        "2d4e2e00f2821c4e2893fa96028cef0ac76e9644",
        "413e3744569ca237e837a1fd57d9ba6ad6adc3de",
        "7a1384874f3747de97c6ccbb4ae6fa2149b27330",
        "abee4036fbeb58c767ef7dc3489804e2afd5c6e1",
        "4758e8db3c72e7c84c30c1a0b597b6d9ed65d35d",
        "250e728fa9e5a02e3c9a6d4f0cfee0683863df83",
        "4675e5ddfdd0d24144498ec9b7d2e5b9cb422249",
    ] {
        assert!(MANIFEST.contains(revision), "missing producer {revision}");
    }
}
