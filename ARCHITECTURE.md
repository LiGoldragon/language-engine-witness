# language-engine-witness architecture

The former Signal-domain witness was removed with the caller-owned bootstrap
authority it depended on. It constructed local encoded identities, observed
the retired generated Rust, and therefore could not validate the new opaque
authority model.

This crate has no Signal-domain fixture until hqu.30 can durably commit an
authority result and install its corresponding generated projection. A future
witness must consume that installed output rather than recreate identity seats,
canonical bytes, or an encoded Rust namespace.
