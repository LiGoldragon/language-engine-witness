# language-engine-witness architecture

This repository owns process-level acceptance for the current language-engine
boundaries without touching production Spirit state.

The authority process witness launches the production sema-translator daemon,
plans complete Nomos documents without invented identities, seals their nested
Universal allocations, and materializes typed populations only from durable
receipts. It proves receipt recovery across daemon restart, spelling-only
rename with unchanged encoded content, lookup-only refusal without marker
movement, and multi-file manifest graph containment.

The current bootstrap transaction explicitly reports
`BootstrapArchiveStatus::NotYetArchived`. Its successful transformation is
therefore proved in process from `VerifiedBootstrapAssembly` through the
current Nomos lowering and a restorable Whole Logos archive. The production
`nomos-engine` daemon is exercised through the length-prefixed `signal-nomos`
wire and must return the typed `EthosPopulationInvalid` refusal for the opaque
transform request. `primary-eyr.2` owns the future persisted wire/restart car;
this witness does not fabricate that archive early.

Both daemon readiness handshakes and every process socket read/write are
bounded to ten seconds. Startup failure kills and reaps the child and removes
its temporary socket, so a live but stalled process cannot hang the owning Nix
gate indefinitely.

The first vertical component slice is a direct structural breadth witness. It
decodes the Nexus header, imports, and body with an explicit authority catalog,
seals that authority, lowers through typed Nomos and Whole Logos, emits and
decodes Rust structurally, and compiles the emitted forms in a scratch Cargo
crate. Its unit, unary, and product declarations are deliberately small enough
that the entire encoded transition remains inspectable.

The checked-in `spirit-domain.ethos` fixture is an exact read-only inventory
witness for the separately pinned current `signal-domain` source. Its
component-side path carries all 41 items and 369 variants through typed Ethos,
direct Nomos, restorable Whole Logos, and structural Rust emit/decode.
Universal declarations and references use the canonical complete-chain
encoding; immutable Rust vocabulary allocation remains deterministic and
process-local. The former sealed-Spirit batch harness is absent: it generated
Rust against the retired signal-domain type surface and retaining it would
create a second domain model beside the current encoded authority.
