`spirit-min.schema` is the read-only compatibility fixture from `golden-bridge @ 03929a478dec18b597ba8994aaea9e576b41a717`. Reference Rust is generated during the witness by the exact pinned public `schema-rust` producer; no checked-in generated Rust is used as the reference.

`second-min.schema` is an author-minted minimal legacy-form schema, distinct from the frozen golden-bridge fixture. It exists only to drive a genuinely second document through the restarted four-process pipeline; its reference Rust is likewise generated during the witness by the same pinned `schema-rust` producer.
