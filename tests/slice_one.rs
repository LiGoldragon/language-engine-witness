use std::{cell::RefCell, collections::BTreeMap, fs, process::Command};

use slice_core_ethos::{
    SixSlotGrammarIds, SixSlotNewtypeCodec, SliceOneBuiltinPriors, WholeEthos, WholeEthosItem,
    WholeEthosVisibility,
};
use slice_core_logos::{WholeLogos, WholeLogosItem, WholeLogosVisibility};
use slice_core_nomos::SliceOneTransformation;
use slice_name_table::{LocalEncodedId, Name};
use slice_raw_discovery::SourceBound;
use slice_rust_logos::{
    RustEmittedIdentifier, RustLogos, RustNameProjectionTable, RustNewtypeVocabulary,
    RustNewtypeVocabularyIds,
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
    fn bind_declaration(&mut self, source: &str, spelling: &str, encoded_id: VocabularyEncodedId) {
        let bound = bound(source, spelling);
        self.names.insert(encoded_id.clone(), Name::new(spelling));
        self.declarations.insert(
            (bound.start(), bound.end()),
            (spelling.to_owned(), encoded_id),
        );
    }

    fn bind_reference(&mut self, source: &str, spelling: &str, encoded_id: VocabularyEncodedId) {
        let bound = bound(source, spelling);
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

fn bound(source: &str, spelling: &str) -> SourceBound {
    let start = source.find(spelling).expect("fixture spelling");
    SourceBound::checked(source, start, start + spelling.len()).expect("fixture source bound")
}

fn grammar_ids() -> SixSlotGrammarIds {
    SixSlotGrammarIds::new(
        issued(VocabularyRoot::Universal, &[40, 1]),
        issued(VocabularyRoot::Universal, &[40, 2]),
        issued(VocabularyRoot::Universal, &[40, 3]),
        issued(VocabularyRoot::Universal, &[40, 4]),
        issued(VocabularyRoot::Universal, &[40, 5]),
    )
    .expect("fixture grammar identities are Universal")
}

fn rust_codec() -> RustLogos {
    let struct_keyword_type = issued(VocabularyRoot::Rust, &[10]);
    let public_keyword_type = issued(VocabularyRoot::Rust, &[11]);
    let declaration_type = issued(VocabularyRoot::Rust, &[12]);
    let reference_type = issued(VocabularyRoot::Rust, &[13]);
    let struct_keyword = issued(VocabularyRoot::Rust, &[1]);
    let public_keyword = issued(VocabularyRoot::Rust, &[2]);
    let mut names = BTreeMap::new();
    for (encoded_id, spelling) in [
        (struct_keyword_type.clone(), "StructKeywordToken"),
        (public_keyword_type.clone(), "PublicKeywordToken"),
        (declaration_type.clone(), "DeclarationToken"),
        (reference_type.clone(), "ReferenceToken"),
        (struct_keyword.clone(), "struct"),
        (public_keyword.clone(), "pub"),
    ] {
        names.insert(encoded_id, Name::new(spelling));
    }

    struct RustNames(BTreeMap<VocabularyEncodedId, Name>);
    impl EncodedNameResolver<VocabularyRoot> for RustNames {
        fn resolve(&self, encoded_id: &VocabularyEncodedId) -> Option<&Name> {
            self.0.get(encoded_id)
        }
    }

    let vocabulary = RustNewtypeVocabulary::seal(
        RustNewtypeVocabularyIds::new(
            struct_keyword_type,
            public_keyword_type,
            declaration_type,
            reference_type,
            struct_keyword,
            public_keyword,
        ),
        &RustNames(names),
    )
    .expect("fixture Rust vocabulary is sealed");
    RustLogos::new(vocabulary)
}

fn assert_six_source_bounds(bounds: &slice_core_ethos::SixSlotSourceBounds) {
    for (bound, expected) in [
        (bounds.imports(), "{}"),
        (bounds.input(), "[]"),
        (bounds.output(), "[]"),
        (bounds.types(), "{\n  CommitSequence.Integer\n}"),
        (bounds.generics(), "{}"),
        (bounds.impls(), "{}"),
    ] {
        assert_eq!(&ETHOS_SOURCE[bound.start()..bound.end()], expected);
    }
}

#[test]
fn six_slot_ethos_lowers_and_emits_a_working_integer_newtype() {
    let declaration = issued(VocabularyRoot::Universal, &[42, 7, 9]);
    let integer = issued(VocabularyRoot::Universal, &[3]);
    let mut ethos_bindings = Bindings::default();
    ethos_bindings.bind_declaration(ETHOS_SOURCE, "CommitSequence", declaration.clone());
    ethos_bindings.bind_reference(ETHOS_SOURCE, "Integer", integer.clone());

    let codec = SixSlotNewtypeCodec::build(
        grammar_ids(),
        SliceOneBuiltinPriors::new(integer.clone()).expect("fixture Integer prior is Universal"),
    )
    .expect("six-slot structuretree seals");
    let decoded = codec
        .decode(ETHOS_SOURCE, &ethos_bindings)
        .expect("typed six-slot Ethos decode");
    assert_six_source_bounds(decoded.source_bounds());
    assert_eq!(
        ethos_bindings.declaration_queries.borrow().as_slice(),
        [bound(ETHOS_SOURCE, "CommitSequence")]
    );
    assert_eq!(
        ethos_bindings.reference_queries.borrow().as_slice(),
        [bound(ETHOS_SOURCE, "Integer")]
    );

    let [WholeEthosItem::Newtype(ethos_newtype)] = decoded.ethos().items() else {
        panic!("the document contains one typed newtype")
    };
    assert_eq!(ethos_newtype.name(), &declaration);
    assert_eq!(ethos_newtype.visibility(), &WholeEthosVisibility::Public);
    assert!(ethos_newtype.attributes().is_empty());
    assert_eq!(
        ethos_newtype.wrapped_field().visibility(),
        &WholeEthosVisibility::Private
    );
    assert_eq!(ethos_newtype.wrapped_field().reference(), &integer);

    let ethos_archive = decoded
        .ethos()
        .to_archive_bytes()
        .expect("archive whole Ethos");
    let restored_ethos =
        WholeEthos::from_archive_bytes(&ethos_archive).expect("restore whole Ethos");
    assert_eq!(&restored_ethos, decoded.ethos());

    let logos = SliceOneTransformation::new().lower(&restored_ethos);
    let [WholeLogosItem::Newtype(logos_newtype)] = logos.items() else {
        panic!("Nomos preserves the one-item whole Logos")
    };
    assert_eq!(logos_newtype.name(), &declaration);
    assert_eq!(logos_newtype.visibility(), &WholeLogosVisibility::Public);
    assert_eq!(
        logos_newtype.wrapped_visibility(),
        &WholeLogosVisibility::Private
    );
    assert_eq!(logos_newtype.wrapped(), &integer);
    let _whole_identity = logos
        .content_identity()
        .expect("whole Logos has a pure-content identity");

    let logos_archive = logos.to_archive_bytes().expect("archive whole Logos");
    let restored_logos =
        WholeLogos::from_archive_bytes(&logos_archive).expect("restore whole Logos");
    assert_eq!(restored_logos, logos);

    let rust = rust_codec();
    let projections = RustNameProjectionTable::try_from_entries([
        (
            declaration.clone(),
            RustEmittedIdentifier::try_new("SliceOneValue").expect("opaque Rust identifier"),
        ),
        (
            integer.clone(),
            RustEmittedIdentifier::try_new("u64").expect("opaque Rust identifier"),
        ),
    ])
    .expect("fixture projections are one-to-one");
    let emitted = rust
        .emit(&restored_logos, &projections)
        .expect("structural Rust emission");

    let mut emitted_bindings = Bindings::default();
    emitted_bindings.bind_declaration(&emitted, "SliceOneValue", declaration.clone());
    emitted_bindings.bind_reference(&emitted, "u64", integer.clone());
    assert_eq!(
        rust.decode(&emitted, &emitted_bindings)
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
            "{emitted}\nfn main() {{ let value = SliceOneValue(41); println!(\"{{}}\", value.0); }}\n"
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
        "41\n"
    );
}

#[test]
fn vertical_slice_pins_the_published_producers() {
    for revision in [
        "d979778aa9d79199785f7b683f1029534aea3604",
        "e47bec61c81fba80deb44c5920f6a15420bbf962",
        "5bace8ae21468401a07af262b6b9c15dd8543cb6",
        "a7dd1e2b8d0c55d26e96c5b1b7154a534cf03e55",
        "cc10e53f49f272ddbd061bf6dea35be072508df9",
        "3f1fa92ec268210777f27878a1a02287a7e2a2a8",
    ] {
        assert!(
            MANIFEST.contains(revision),
            "the vertical slice must pin published producer {revision}"
        );
    }
}
