# language-engine-witness architecture

This repository owns process-level acceptance for the current language-engine
boundaries without touching production Spirit state.

The authored Nomos process witness launches the production sema-translator
daemon, plans a complete Nomos document without invented identities, seals its
single nested Universal allocation, and materializes the typed population only
from the durable receipt. It then launches the production `nomos-engine`
daemon against its own isolated `nomos.sema` database and communicates through
the pure length-prefixed `signal-nomos` request/reply contract.

The witness deploys the authority-authored Capsule into an opaque slot,
transforms a nonempty direct Whole Ethos population through the native authored
evaluator, and advances the authenticated NameTree projection through the
configured Unix-peer admin path. It kills Nomos, restarts it against the same
database, and proves the slot, Capsule, and projection history recover by
transforming again at projection version 1. An initial-projection redeploy is
then refused as stale, while an authenticated current deployment is an
`AlreadyCurrent` no-op even with a stale CAS expectation.

Nomos owns one embedded Sema record family for Capsules, projection histories,
and slot bindings. This acceptance path has no central Sema storage daemon,
legacy Nomos relay, upstream engine socket, fixture package, or daemon-side
Logos output write.

The authority witness separately proves durable receipt recovery across a
sema-translator restart, spelling-only rename with unchanged encoded content,
lookup-only unresolved-reference refusal without marker movement, and
multi-file manifest graph containment.

The first vertical component slice remains a direct structural breadth
witness. It decodes a complete six-slot Ethos document with caller-supplied
translator identities, lowers through typed Nomos and Whole Logos, emits and
decodes Rust structurally, and compiles the emitted forms in a scratch Cargo
crate.

The checked-in `spirit-domain.ethos` fixture is an exact read-only inventory
witness for the separately pinned current `signal-domain` source. Its
component-side path carries all 41 items and 369 variants through typed Ethos,
archive/restart, direct Nomos, Whole Logos, and structural Rust emit/decode.
Universal declarations and references use the canonical complete-chain
encoding; immutable Rust vocabulary allocation remains deterministic and
process-local.
