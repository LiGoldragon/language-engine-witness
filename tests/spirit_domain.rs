use std::{cell::RefCell, collections::BTreeMap, fs, process::Command};

use slice_core_ethos::{
    SixSlotEthosCodec, SixSlotGrammarIds, WholeEthos, WholeEthosBuiltinPriors, WholeEthosItem,
    WholeEthosTypeReference, WholeEthosVariantPayload,
};
use slice_core_logos::{
    WholeLogos, WholeLogosItem, WholeLogosTypeReference, WholeLogosVariantPayload,
};
use slice_core_nomos::{SliceOneTransformation, SliceOneVocabularyReferenceMapping};
use slice_name_table::{LocalEncodedId, Name};
use slice_raw_discovery::SourceBound;
use slice_rust_logos::{
    FixtureRustVocabulary, FixtureRustVocabularyIds, RustEncodedIdCodec, RustLogos,
};
use slice_signal_sema_translator::{VocabularyEncodedId, VocabularyRoot};
use slice_structural_codec::{
    DeclarationAssignment, DecodeNameBindings, EncodedNameResolver, NameOccurrence,
    ResolvedReference,
};

const ETHOS_SOURCE: &str = include_str!("fixtures/spirit-domain.ethos");
const CARGO_MANIFEST: &str = include_str!("../Cargo.toml");
const FLAKE: &str = include_str!("../flake.nix");
const SIGNAL_DOMAIN_REVISION: &str = "c24059de43614e6fb2128e47f959dc11748bd7e7";

#[derive(Clone)]
struct FixtureName {
    spelling: String,
    bound: SourceBound,
}

#[derive(Clone)]
struct FixtureReference {
    head: FixtureName,
    payload: FixtureName,
}

#[derive(Clone)]
struct FixtureVariant {
    name: FixtureName,
    payload: Option<FixtureName>,
}

#[derive(Clone)]
enum FixtureItem {
    Enumeration(FixtureName, Vec<FixtureVariant>),
    Newtype(FixtureName, FixtureReference),
}

impl FixtureItem {
    fn name(&self) -> &FixtureName {
        match self {
            Self::Enumeration(name, _) | Self::Newtype(name, _) => name,
        }
    }

    fn variants(&self) -> &[FixtureVariant] {
        match self {
            Self::Enumeration(_, variants) => variants,
            Self::Newtype(_, _) => &[],
        }
    }
}

struct FixtureInventory(Vec<FixtureItem>);

impl FixtureInventory {
    fn parse(source: &str) -> Self {
        let mut cursor = FixtureCursor::new(source);
        cursor.empty_slot('{', '}');
        cursor.empty_slot('[', ']');
        cursor.empty_slot('[', ']');
        cursor.expect('{');
        let mut items = Vec::new();
        while cursor.peek() != Some('}') {
            let name = cursor.name();
            cursor.expect('.');
            if cursor.consume('[') {
                let mut variants = Vec::new();
                while cursor.peek() != Some(']') {
                    let variant = cursor.name();
                    let payload = cursor.consume('.').then(|| cursor.name());
                    variants.push(FixtureVariant {
                        name: variant,
                        payload,
                    });
                }
                cursor.expect(']');
                items.push(FixtureItem::Enumeration(name, variants));
            } else {
                let head = cursor.name();
                cursor.expect('.');
                let payload = cursor.name();
                items.push(FixtureItem::Newtype(
                    name,
                    FixtureReference { head, payload },
                ));
            }
        }
        cursor.expect('}');
        cursor.empty_slot('{', '}');
        cursor.empty_slot('{', '}');
        cursor.finish();
        Self(items)
    }

    fn items(&self) -> &[FixtureItem] {
        &self.0
    }

    fn enumeration_count(&self) -> usize {
        self.0
            .iter()
            .filter(|item| matches!(item, FixtureItem::Enumeration(_, _)))
            .count()
    }

    fn newtype_count(&self) -> usize {
        self.0.len() - self.enumeration_count()
    }

    fn variant_count(&self) -> usize {
        self.0.iter().map(|item| item.variants().len()).sum()
    }

    fn payload_variant_count(&self) -> usize {
        self.0
            .iter()
            .flat_map(FixtureItem::variants)
            .filter(|variant| variant.payload.is_some())
            .count()
    }

    fn application_count(&self, head: &str) -> usize {
        self.0
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    FixtureItem::Newtype(_, reference) if reference.head.spelling == head
                )
            })
            .count()
    }

    fn declaration_bounds(&self) -> Vec<SourceBound> {
        self.0
            .iter()
            .flat_map(|item| {
                std::iter::once(item.name().bound)
                    .chain(item.variants().iter().map(|variant| variant.name.bound))
            })
            .collect()
    }

    fn reference_bounds(&self) -> Vec<SourceBound> {
        self.0
            .iter()
            .flat_map(|item| match item {
                FixtureItem::Enumeration(_, variants) => variants
                    .iter()
                    .filter_map(|variant| variant.payload.as_ref().map(|name| name.bound))
                    .collect(),
                FixtureItem::Newtype(_, reference) => {
                    vec![reference.head.bound, reference.payload.bound]
                }
            })
            .collect()
    }
}

struct FixtureCursor<'source> {
    source: &'source str,
    offset: usize,
}

impl<'source> FixtureCursor<'source> {
    fn new(source: &'source str) -> Self {
        Self { source, offset: 0 }
    }

    fn empty_slot(&mut self, open: char, close: char) {
        self.expect(open);
        self.expect(close);
    }

    fn name(&mut self) -> FixtureName {
        self.skip_whitespace();
        let start = self.offset;
        while self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_alphanumeric)
        {
            self.offset += 1;
        }
        assert!(self.offset > start, "expected fixture name at {start}");
        FixtureName {
            spelling: self.source[start..self.offset].to_owned(),
            bound: SourceBound::checked(self.source, start, self.offset)
                .expect("fixture name has a valid source bound"),
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_whitespace();
        self.source[self.offset..].chars().next()
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.offset += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: char) {
        assert!(
            self.consume(expected),
            "expected `{expected}` at fixture byte {}",
            self.offset
        );
    }

    fn finish(&mut self) {
        self.skip_whitespace();
        assert_eq!(self.offset, self.source.len(), "unconsumed fixture source");
    }

    fn skip_whitespace(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.offset += 1;
        }
    }
}

fn issued(chain: &[u16]) -> VocabularyEncodedId {
    VocabularyEncodedId::new(
        VocabularyRoot::Universal,
        chain.iter().copied().map(LocalEncodedId::new).collect(),
    )
    .expect("fixture carries a complete Universal chain")
}

struct FixtureIdentities {
    declarations: BTreeMap<(usize, usize), VocabularyEncodedId>,
    items_by_spelling: BTreeMap<String, VocabularyEncodedId>,
    integer: VocabularyEncodedId,
    vector: VocabularyEncodedId,
    scope_of: VocabularyEncodedId,
    rust_vector: VocabularyEncodedId,
}

impl FixtureIdentities {
    fn assign(inventory: &FixtureInventory) -> Self {
        let mut declarations = BTreeMap::new();
        let mut items_by_spelling = BTreeMap::new();
        for (item_index, item) in inventory.items().iter().enumerate() {
            let item_local = u16::try_from(item_index + 1).expect("fixture item local");
            let item_id = issued(&[42, 77, item_local]);
            let name = item.name();
            declarations.insert((name.bound.start(), name.bound.end()), item_id.clone());
            items_by_spelling.insert(name.spelling.clone(), item_id);
            for (variant_index, variant) in item.variants().iter().enumerate() {
                let variant_local =
                    u16::try_from(variant_index + 1).expect("fixture variant local");
                declarations.insert(
                    (variant.name.bound.start(), variant.name.bound.end()),
                    issued(&[42, 77, item_local, variant_local]),
                );
            }
        }
        Self {
            declarations,
            items_by_spelling,
            integer: issued(&[3]),
            vector: issued(&[4]),
            scope_of: issued(&[5]),
            rust_vector: issued_rust(&[20]),
        }
    }

    fn declaration(&self, name: &FixtureName) -> VocabularyEncodedId {
        self.declarations[&(name.bound.start(), name.bound.end())].clone()
    }

    fn item(&self, spelling: &str) -> VocabularyEncodedId {
        self.items_by_spelling[spelling].clone()
    }

    fn priors(&self) -> WholeEthosBuiltinPriors {
        let mut priors = WholeEthosBuiltinPriors::new(self.integer.clone(), self.vector.clone())
            .expect("fixture builtin chains are Universal")
            .with_application_head(self.scope_of.clone())
            .expect("fixture ScopeOf chain is Universal");
        for identity in self.items_by_spelling.values() {
            priors = priors
                .with_identity(identity.clone())
                .expect("fixture declaration chain is Universal");
        }
        priors
    }
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
    fn bind_declaration(&mut self, name: &FixtureName, encoded_id: VocabularyEncodedId) {
        self.bind_declaration_bound(name.spelling.as_str(), name.bound, encoded_id);
    }

    fn bind_reference(&mut self, name: &FixtureName, encoded_id: VocabularyEncodedId) {
        self.bind_reference_bound(name.spelling.as_str(), name.bound, encoded_id);
    }

    fn bind_declaration_bound(
        &mut self,
        spelling: &str,
        bound: SourceBound,
        encoded_id: VocabularyEncodedId,
    ) {
        self.names.insert(encoded_id.clone(), Name::new(spelling));
        self.declarations.insert(
            (bound.start(), bound.end()),
            (spelling.to_owned(), encoded_id),
        );
    }

    fn bind_reference_bound(
        &mut self,
        spelling: &str,
        bound: SourceBound,
        encoded_id: VocabularyEncodedId,
    ) {
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

fn ethos_bindings(inventory: &FixtureInventory, identities: &FixtureIdentities) -> Bindings {
    let mut bindings = Bindings::default();
    for item in inventory.items() {
        bindings.bind_declaration(item.name(), identities.declaration(item.name()));
        match item {
            FixtureItem::Enumeration(_, variants) => {
                for variant in variants {
                    bindings.bind_declaration(&variant.name, identities.declaration(&variant.name));
                    if let Some(payload) = &variant.payload {
                        bindings
                            .bind_reference(payload, identities.item(payload.spelling.as_str()));
                    }
                }
            }
            FixtureItem::Newtype(_, reference) => {
                let head = match reference.head.spelling.as_str() {
                    "Vector" => identities.vector.clone(),
                    "ScopeOf" => identities.scope_of.clone(),
                    spelling => panic!("unexpected fixture application head {spelling}"),
                };
                bindings.bind_reference(&reference.head, head);
                bindings.bind_reference(
                    &reference.payload,
                    identities.item(reference.payload.spelling.as_str()),
                );
            }
        }
    }
    bindings
        .names
        .insert(identities.rust_vector.clone(), Name::new("Vec"));
    bindings
}

fn slice_transformation(identities: &FixtureIdentities) -> SliceOneTransformation {
    let vector = SliceOneVocabularyReferenceMapping::new(
        identities.vector.clone(),
        identities.rust_vector.clone(),
    )
    .expect("Universal Vector maps to immutable Rust Vec");
    SliceOneTransformation::with_reference_mappings(vec![vector])
        .expect("Spirit mapping sources are unique")
}

fn grammar_ids() -> SixSlotGrammarIds {
    SixSlotGrammarIds::new(
        issued(&[40, 1]),
        issued(&[40, 2]),
        issued(&[40, 3]),
        issued(&[40, 4]),
        issued(&[40, 5]),
        issued(&[40, 6]),
        issued(&[40, 7]),
    )
    .expect("fixture grammar chains are Universal")
}

fn rust_codec() -> RustLogos {
    let newtype_item = issued_rust(&[10]);
    let enumeration_item = issued_rust(&[11]);
    let variant = issued_rust(&[12]);
    let tuple_field = issued_rust(&[13]);
    let type_reference = issued_rust(&[14]);
    let struct_keyword = issued_rust(&[1]);
    let enum_keyword = issued_rust(&[2]);
    let public_keyword = issued_rust(&[3]);
    let comma = issued_rust(&[4]);
    let semicolon = issued_rust(&[5]);
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

fn issued_rust(chain: &[u16]) -> VocabularyEncodedId {
    VocabularyEncodedId::new(
        VocabularyRoot::Rust,
        chain.iter().copied().map(LocalEncodedId::new).collect(),
    )
    .expect("fixture carries a complete Rust chain")
}

struct ProductionSpellings(BTreeMap<VocabularyEncodedId, String>);

impl ProductionSpellings {
    fn build(logos: &WholeLogos, allocated: &Bindings) -> Self {
        let mut spellings = Self(BTreeMap::new());
        for item in logos.items() {
            match item {
                WholeLogosItem::Newtype(newtype) => {
                    spellings.insert(newtype.name(), allocated);
                    spellings.insert_reference(newtype.wrapped(), allocated);
                }
                WholeLogosItem::Enumeration(enumeration) => {
                    spellings.insert(enumeration.name(), allocated);
                    for variant in enumeration.variants() {
                        spellings.insert(variant.name(), allocated);
                        if let WholeLogosVariantPayload::Tuple(fields) = variant.payload() {
                            for reference in fields.fields() {
                                spellings.insert_reference(reference, allocated);
                            }
                        }
                    }
                }
            }
        }
        spellings
    }

    fn insert_reference(&mut self, reference: &WholeLogosTypeReference, allocated: &Bindings) {
        match reference {
            WholeLogosTypeReference::Identity(encoded_id) => {
                self.insert(encoded_id, allocated);
            }
            WholeLogosTypeReference::Application(application) => {
                self.insert(application.head(), allocated);
                self.insert_reference(application.payload(), allocated);
            }
        }
    }

    fn insert(&mut self, encoded_id: &VocabularyEncodedId, allocated: &Bindings) {
        let spelling = match encoded_id.root_variant() {
            VocabularyRoot::Universal => RustEncodedIdCodec::encode(encoded_id),
            VocabularyRoot::Rust => allocated
                .resolve(encoded_id)
                .expect("Rust vocabulary identity is allocated")
                .as_str()
                .to_owned(),
        };
        if let Some(previous) = self.0.insert(encoded_id.clone(), spelling.clone()) {
            assert_eq!(previous, spelling, "one identity has one emitted spelling");
        }
    }

    fn spelling(&self, encoded_id: &VocabularyEncodedId) -> &str {
        self.0[encoded_id].as_str()
    }
}

fn emitted_bindings(source: &str, logos: &WholeLogos, spellings: &ProductionSpellings) -> Bindings {
    let mut bindings = Bindings::default();
    for (encoded_id, spelling) in &spellings.0 {
        for bound in identifier_bounds(source, spelling) {
            bindings.bind_reference_bound(spelling, bound, encoded_id.clone());
        }
    }
    for item in logos.items() {
        match item {
            WholeLogosItem::Newtype(newtype) => {
                let spelling = spellings.spelling(newtype.name());
                bindings.bind_declaration_bound(
                    spelling,
                    declaration_bound(source, "struct", spelling),
                    newtype.name().clone(),
                );
            }
            WholeLogosItem::Enumeration(enumeration) => {
                let spelling = spellings.spelling(enumeration.name());
                bindings.bind_declaration_bound(
                    spelling,
                    declaration_bound(source, "enum", spelling),
                    enumeration.name().clone(),
                );
                for variant in enumeration.variants() {
                    let spelling = spellings.spelling(variant.name());
                    let occurrences = identifier_bounds(source, spelling);
                    let [bound] = occurrences.as_slice() else {
                        panic!("fixture variant projection must occur exactly once")
                    };
                    bindings.bind_declaration_bound(spelling, *bound, variant.name().clone());
                }
            }
        }
    }
    bindings
}

fn declaration_bound(source: &str, keyword: &str, spelling: &str) -> SourceBound {
    let prefix = format!("{keyword} {spelling}");
    let prefix_start = source
        .find(prefix.as_str())
        .expect("emitted item declaration");
    let start = prefix_start + keyword.len() + 1;
    SourceBound::checked(source, start, start + spelling.len()).expect("emitted declaration bound")
}

fn identifier_bounds(source: &str, spelling: &str) -> Vec<SourceBound> {
    source
        .match_indices(spelling)
        .filter_map(|(start, _)| {
            let end = start + spelling.len();
            let before = source[..start].chars().next_back();
            let after = source[end..].chars().next();
            (!before.is_some_and(is_rust_identifier_character)
                && !after.is_some_and(is_rust_identifier_character))
            .then(|| SourceBound::checked(source, start, end).expect("identifier bound"))
        })
        .collect()
}

fn is_rust_identifier_character(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

fn assert_query_bounds(actual: &[SourceBound], expected: &[SourceBound]) {
    assert!(
        actual.iter().all(|bound| expected.contains(bound)),
        "the evaluator queried an unexpected source bound"
    );
    assert!(
        expected.iter().all(|bound| actual.contains(bound)),
        "the evaluator skipped an expected source bound"
    );
}

fn executable_enumerations(logos: &WholeLogos) -> WholeLogos {
    WholeLogos::new(
        logos
            .items()
            .iter()
            .filter(|item| matches!(item, WholeLogosItem::Enumeration(_)))
            .cloned()
            .collect(),
    )
}

fn behavior_source(emitted: &str, logos: &WholeLogos, spellings: &ProductionSpellings) -> String {
    let mut source = String::from(emitted);
    source.push_str("\nfn main() {\n");
    let mut exercised = 0;
    for item in logos.items() {
        let WholeLogosItem::Enumeration(enumeration) = item else {
            continue;
        };
        let enumeration_spelling = spellings.spelling(enumeration.name());
        for variant in enumeration.variants() {
            let variant_spelling = spellings.spelling(variant.name());
            match variant.payload() {
                WholeLogosVariantPayload::Unit => {
                    source.push_str(
                        format!("    let _ = {enumeration_spelling}::{variant_spelling};\n")
                            .as_str(),
                    );
                }
                WholeLogosVariantPayload::Tuple(fields) => {
                    let [WholeLogosTypeReference::Identity(payload)] = fields.fields() else {
                        panic!("Spirit payload variants each carry one identity")
                    };
                    source.push_str(
                        format!(
                            "    let _ = {enumeration_spelling}::{variant_spelling}({});\n",
                            construct_enumeration_value(logos, payload, spellings),
                        )
                        .as_str(),
                    );
                }
            }
            exercised += 1;
        }
    }
    source.push_str(format!("    println!(\"{exercised}\");\n}}\n").as_str());
    source
}

fn construct_enumeration_value(
    logos: &WholeLogos,
    identity: &VocabularyEncodedId,
    spellings: &ProductionSpellings,
) -> String {
    let enumeration = logos
        .items()
        .iter()
        .find_map(|item| match item {
            WholeLogosItem::Enumeration(candidate) if candidate.name() == identity => {
                Some(candidate)
            }
            _ => None,
        })
        .expect("payload target enumeration is present");
    let variant = enumeration
        .variants()
        .iter()
        .find(|candidate| candidate.payload() == &WholeLogosVariantPayload::Unit)
        .unwrap_or_else(|| &enumeration.variants()[0]);
    let prefix = format!(
        "{}::{}",
        spellings.spelling(identity),
        spellings.spelling(variant.name()),
    );
    match variant.payload() {
        WholeLogosVariantPayload::Unit => prefix,
        WholeLogosVariantPayload::Tuple(fields) => {
            let [WholeLogosTypeReference::Identity(payload)] = fields.fields() else {
                panic!("Spirit payload variants each carry one identity")
            };
            format!(
                "{prefix}({})",
                construct_enumeration_value(logos, payload, spellings)
            )
        }
    }
}

#[test]
fn complete_spirit_domain_reaches_durable_logos_and_production_rust() {
    let inventory = FixtureInventory::parse(ETHOS_SOURCE);
    assert_eq!(inventory.items().len(), 41);
    assert_eq!(inventory.enumeration_count(), 38);
    assert_eq!(inventory.newtype_count(), 3);
    assert_eq!(inventory.variant_count(), 369);
    assert_eq!(inventory.payload_variant_count(), 37);
    assert_eq!(inventory.application_count("Vector"), 2);
    assert_eq!(inventory.application_count("ScopeOf"), 1);

    let identities = FixtureIdentities::assign(&inventory);
    let bindings = ethos_bindings(&inventory, &identities);
    let codec = SixSlotEthosCodec::build(grammar_ids(), identities.priors())
        .expect("Spirit fixture structuretree seals");
    let decoded = codec
        .decode(ETHOS_SOURCE, &bindings)
        .expect("complete typed Spirit Ethos decode");
    assert_eq!(decoded.ethos().items().len(), 41);
    assert_query_bounds(
        bindings.declaration_queries.borrow().as_slice(),
        inventory.declaration_bounds().as_slice(),
    );
    assert_query_bounds(
        bindings.reference_queries.borrow().as_slice(),
        inventory.reference_bounds().as_slice(),
    );

    for (item, identity) in inventory.items().iter().zip(decoded.ethos().items()) {
        match (item, identity) {
            (FixtureItem::Enumeration(_, expected), WholeEthosItem::Enumeration(actual)) => {
                assert_eq!(actual.variants().len(), expected.len());
                for (expected_variant, actual_variant) in expected.iter().zip(actual.variants()) {
                    assert_eq!(
                        actual_variant.name(),
                        &identities.declaration(&expected_variant.name)
                    );
                    match (&expected_variant.payload, actual_variant.payload()) {
                        (None, WholeEthosVariantPayload::Unit)
                        | (Some(_), WholeEthosVariantPayload::Tuple(_)) => {}
                        _ => panic!("variant payload form changed"),
                    }
                }
            }
            (FixtureItem::Newtype(_, _), WholeEthosItem::Newtype(_)) => {}
            _ => panic!("Spirit fixture item form changed"),
        }
    }

    let ethos_archive = decoded
        .ethos()
        .to_archive_bytes()
        .expect("archive complete Spirit Ethos");
    let restored_ethos =
        WholeEthos::from_archive_bytes(&ethos_archive).expect("restart complete Spirit Ethos");
    assert_eq!(&restored_ethos, decoded.ethos());

    let logos = slice_transformation(&identities).lower(&restored_ethos);
    assert_eq!(logos.items().len(), 41);
    let identity = logos
        .content_identity()
        .expect("complete Spirit Logos has whole-content identity");
    let logos_archive = logos
        .to_archive_bytes()
        .expect("archive complete Spirit Logos");
    let restored_logos =
        WholeLogos::from_archive_bytes(&logos_archive).expect("restart complete Spirit Logos");
    assert_eq!(restored_logos, logos);
    assert_eq!(
        restored_logos
            .content_identity()
            .expect("restarted Spirit Logos identity"),
        identity
    );

    let spellings = ProductionSpellings::build(&restored_logos, &bindings);
    let rust = rust_codec();
    let emitted = rust
        .emit(&restored_logos, &bindings)
        .expect("production Rust emission for every Spirit declaration");
    assert!(emitted.contains(&RustEncodedIdCodec::encode(&identities.scope_of)));
    assert!(emitted.contains("Vec"));
    assert!(!emitted.contains("FixtureType"));
    assert!(!emitted.contains("ScopeOf"));
    assert_eq!(
        rust.decode_fixture(
            &emitted,
            &emitted_bindings(&emitted, &restored_logos, &spellings),
        )
        .expect("production Rust decode for every Spirit declaration"),
        restored_logos
    );

    let executable = executable_enumerations(&restored_logos);
    let executable_spellings = ProductionSpellings::build(&executable, &bindings);
    let executable_rust = rust
        .emit(&executable, &bindings)
        .expect("production Rust emission for Spirit enumerations");
    assert_eq!(
        rust.decode_fixture(
            &executable_rust,
            &emitted_bindings(&executable_rust, &executable, &executable_spellings),
        )
        .expect("production Rust decode for Spirit enumerations"),
        executable
    );
    let temporary = tempfile::tempdir().expect("scratch crate directory");
    fs::create_dir(temporary.path().join("src")).expect("scratch source directory");
    fs::write(
        temporary.path().join("Cargo.toml"),
        "[package]\nname=\"spirit-domain-generated\"\nversion=\"0.0.0\"\nedition=\"2024\"\n",
    )
    .expect("scratch manifest");
    fs::write(
        temporary.path().join("src/main.rs"),
        behavior_source(&executable_rust, &executable, &executable_spellings),
    )
    .expect("scratch generated source and behavior harness");
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
        "369\n"
    );
}

#[test]
fn scope_of_stays_typed_application_data_without_a_runtime_substitute() {
    let inventory = FixtureInventory::parse(ETHOS_SOURCE);
    let identities = FixtureIdentities::assign(&inventory);
    let bindings = ethos_bindings(&inventory, &identities);
    let decoded = SixSlotEthosCodec::build(grammar_ids(), identities.priors())
        .expect("Spirit fixture structuretree seals")
        .decode(ETHOS_SOURCE, &bindings)
        .expect("complete typed Spirit Ethos decode");
    let domain_scope = decoded
        .ethos()
        .items()
        .iter()
        .find_map(|item| match item {
            WholeEthosItem::Newtype(newtype)
                if newtype.name() == &identities.item("DomainScope") =>
            {
                Some(newtype)
            }
            _ => None,
        })
        .expect("DomainScope declaration");
    assert_eq!(
        domain_scope.wrapped_field().reference(),
        &WholeEthosTypeReference::Application(slice_core_ethos::WholeEthosTypeApplication::new(
            identities.scope_of.clone(),
            WholeEthosTypeReference::Identity(identities.item("Domain")),
        ))
    );
    for spelling in ["DomainScopes", "ScopeSet"] {
        let vector = decoded
            .ethos()
            .items()
            .iter()
            .find_map(|item| match item {
                WholeEthosItem::Newtype(newtype)
                    if newtype.name() == &identities.item(spelling) =>
                {
                    Some(newtype)
                }
                _ => None,
            })
            .expect("Vector application declaration");
        assert_eq!(
            vector.wrapped_field().reference(),
            &WholeEthosTypeReference::Application(
                slice_core_ethos::WholeEthosTypeApplication::new(
                    identities.vector.clone(),
                    WholeEthosTypeReference::Identity(identities.item("DomainScope")),
                )
            )
        );
    }

    let logos = slice_transformation(&identities).lower(decoded.ethos());
    let domain_scope = logos
        .items()
        .iter()
        .find_map(|item| match item {
            WholeLogosItem::Newtype(newtype)
                if newtype.name() == &identities.item("DomainScope") =>
            {
                Some(newtype)
            }
            _ => None,
        })
        .expect("lowered DomainScope declaration");
    assert_eq!(
        domain_scope.wrapped(),
        &WholeLogosTypeReference::Application(slice_core_logos::WholeLogosTypeApplication::new(
            identities.scope_of.clone(),
            WholeLogosTypeReference::Identity(identities.item("Domain")),
        ))
    );
    for spelling in ["DomainScopes", "ScopeSet"] {
        let vector = logos
            .items()
            .iter()
            .find_map(|item| match item {
                WholeLogosItem::Newtype(newtype)
                    if newtype.name() == &identities.item(spelling) =>
                {
                    Some(newtype)
                }
                _ => None,
            })
            .expect("lowered Vector application declaration");
        assert_eq!(
            vector.wrapped(),
            &WholeLogosTypeReference::Application(
                slice_core_logos::WholeLogosTypeApplication::new(
                    identities.rust_vector.clone(),
                    WholeLogosTypeReference::Identity(identities.item("DomainScope")),
                )
            )
        );
    }
}

#[test]
fn spirit_fixture_and_runtime_domain_type_share_the_exact_pin() {
    assert!(FLAKE.contains(SIGNAL_DOMAIN_REVISION));
    assert!(!CARGO_MANIFEST.contains("schema-rust"));
    assert!(CARGO_MANIFEST.contains("signal-domain"));
    assert!(CARGO_MANIFEST.contains(SIGNAL_DOMAIN_REVISION));
}
