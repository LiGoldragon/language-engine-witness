`spirit-domain.ethos` is an exact read-only inventory witness for the separately
pinned `signal-domain` source. A Nix check compares both files byte for byte.
The generated Sema execution witness also links that exact revision so its
`records` table uses the real `signal_domain::Domain` key type and value.

The Nomos process witness has no checked-in package or pipeline fixture. It
authors and seals its population through the real sema-translator process, then
deploys the resulting authenticated Capsule through the real `nomos-engine`
process in an isolated temporary directory.

`nexus.ethos` is the unchanged psyche-reviewed Spirit Nexus fixture pinned by
the Slice 2 producer train. Its witness decodes the source into typed WholeEthos,
lowers it through the allocation-free Nexus transformer, projects typed
WholeLogos to Rust, and compiles and runs that output in an isolated scratch
crate.

`interface.ethos` and `sema.ethos` are the other two unchanged psyche-reviewed
Spirit fixtures. The socket-free Nomos batch API and installed CLI consume the
explicit translator-issued identity view in `batch-config.json`. Interface
Input, Output, and Refusal positions now generate universal memberships, with
Refusal also generating `Display` and `Error`; the emitted Interface is compiled
and exercised in this crate. Only its two Stream applications remain typed
deferrals. Sema's six stored record declarations and three local table
specifications generate without deferral. The fixture's `Migration` record and
`migrations` table are witnessed only as inert generic record/table structure:
they do not authorize migration behavior, schema evolution, or old-store
compatibility. The generated `records` specification is registered in a fresh
real redb-backed Sema store and its `StoredRecord`, keyed by `Domain`, survives
write/read and reopen/read equality.

[assumption primary-vq6.7-A1 — external storage contracts] The batch assembly
owns explicit storage fingerprints for Rust builtins and imported types. The
`signal-domain.Domain` contract is the SHA-256 digest of the exact schema file
at the pinned source revision; the Interface contracts are explicit versioned
assembly inputs and must change whenever their archived layouts change. The
generator neither guesses nor coerces an external layout.
