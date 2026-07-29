# language-engine-witness architecture

This repository owns the process-level acceptance boundary for the language engine. It launches the four delivered pipeline runtimes as separate OS processes, subscribes downstream before input, carries the real `spirit-min.ethos` fixture through Ethos → Nomos → Logos push relays and typed Unix-socket contracts, and proves the emitted Rust is a working program — it compiles under a locked rkyv dependency manifest and passes its public-surface behavior tests across the default, no-default, and all-features builds. A separate acceptance path launches the sema-translator naming authority as a fifth real process for authored Nomos identity. The witness does not byte-compare emission against the historical schema-rust oracle projection; its bar is working programs, not oracle equivalence.

The witness then kills all four processes and restarts every one of them against the same isolated database and Unix sockets. It proves the first document and every stored root recover durably, re-establishes the downstream subscription on the rebound Logos socket, and drives a genuinely second fixture (`second-min.ethos`) end to end through the restarted pipeline — asserting its projection arrives through a push (timeout-guarded, never polled), differs from the first document's Rust, and stores durably in Sema. It never touches production Spirit state.

The first vertical engine slice is a separate component-side witness. It decodes a complete six-slot Ethos document with caller-supplied translator identities into one application-backed newtype and one enumeration containing unit and positional tuple variants. The direct typed Nomos transformation lowers that positional Whole Ethos value into Whole Logos without entering the legacy string-bearing generation path. Rust Logos then structurally emits the newtype, enumeration, recursive `Vector.Integer` applications, and every tuple field from caller-supplied opaque projections. The test archives and restores both whole carriers, checks every declaration, variant, application-head, and reference chain, decodes the emitted Rust back to the same Whole Logos value, proves incomplete projection data returns no partial source, then uses a process-local scratch Cargo crate to compile and exhaustively run all emitted forms. That older test's identities, immutable Rust vocabulary, and projections remain deterministic process-local fixtures.

The authored Nomos identity witness is the authority-backed successor boundary. It structurally plans a complete Nomos document without invented identities, sends the single nested `SealUniversal` request over the production bound Unix-socket contract to a real sema-translator daemon, and materializes the encoded document only from the committed receipt. It proves module-owned transformer and binding chains, durable receipt recovery after killing and restarting the daemon on the same isolated database, spelling-only operational rename with unchanged encoded content, and lookup-only unresolved-reference refusal with no database-marker movement. The daemon is the sole allocator and writer; the witness never touches production state.

The checked-in `spirit-domain.ethos` fixture is an exact read-only inventory
witness for the separately pinned current `signal-domain` source. A Nix check
compares the two sources without evaluating or linking the legacy
`signal-domain` crate. Its component-side witness assigns opaque process-local
Universal chains before decode, then carries all 41 items and 369 variants
through typed Ethos, archive/restart, direct Nomos, Whole Logos identity and
archive/restart, typed Universal-Vector-to-Rust-Vec transformation data, and
production structural Rust emit/decode. Universal declarations and references
use `rust-logos`' canonical Base58BTC encoding of their complete chains; the
two Vector application heads use the immutable Rust spelling `Vec`. The 38
authored enumerations compile under those production names and all 369 emitted
variants are constructed in a scratch Cargo program. The three application
newtypes remain present in the full structural carrier and Rust round trip.
`DomainScope.ScopeOf.Domain` has no approved synthesis mechanism on this path,
and both `Vec<DomainScope>` declarations depend on that missing output, so none
of the three is substituted or claimed as scratch-Cargo behavior. Identities
and the immutable Rust vocabulary allocation remain deterministic process-local
fixtures; the emitted-name algorithm is the production `rust-logos` boundary.
