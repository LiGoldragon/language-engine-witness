`spirit-domain.ethos` is an exact read-only inventory witness for the separately
pinned `signal-domain` source. A Nix check compares both files byte for byte
without evaluating or linking the legacy `signal-domain` crate.

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
Spirit fixtures. The Slice 3 witness sends all three through the socket-free
Nomos batch API and installed CLI using the explicit translator-issued identity
view in `batch-config.json`. Interface membership, refusal behavior, Stream
applications, and Sema tables remain visible in typed deferred receipts rather
than being claimed as implemented semantics.
