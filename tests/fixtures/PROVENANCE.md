`spirit-domain.ethos` is an exact read-only inventory witness for the separately
pinned `signal-domain` source. A Nix check compares both files byte for byte.
The generated Spirit Interface links that exact revision for its `Domain` and
`DomainScopes` imports.

The Nomos process witness has no checked-in package or pipeline fixture. It
authors and seals its population through the real sema-translator process, then
deploys the resulting authenticated Capsule through the real `nomos-engine`
process in an isolated temporary directory.

The strict Spirit batch witness has no local Ethos or numeric-allocation
fixture.  Nix pins the public `spirit-ethos` producer revision and supplies it
through `SPIRIT_ETHOS_SOURCE`.  The build script consumes only that producer's
sealed `batch-config.json`, allocation manifest, receipt, and three roots as
one named Interface/Nexus/Sema bundle.  Its generated modules compile in the
same crate topology: Nexus and Sema import the generated Interface module and
Sema emits the records and migrations table specifications.  The Rust witness
also proves direct Core Ethos decode, canonical re-decode, and archive restore
for every sealed root.

The source producer records published storage evidence only for actual Sema
storage leaves.  Non-storage imports carry exact Rust paths but no fabricated
storage fingerprint.  The generator refuses a missing external storage
fingerprint if one is reached while lowering a Sema table.
