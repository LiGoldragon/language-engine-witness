# language-engine-witness architecture

This repository owns one capability: launch the four delivered runtime libraries as four separate OS processes, subscribe downstream before input, carry the real `spirit-min.schema` fixture through Schema → Nomos → Logos push relays and typed Unix-socket contracts, and independently compile the emitted and reference Rust libraries with an identical rkyv dependency manifest and shared public-surface behavior tests.

The witness restarts the central Sema process against the same isolated database and fetches the durable Logos document. It never touches production Spirit state.
