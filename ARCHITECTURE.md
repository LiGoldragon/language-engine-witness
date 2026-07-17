# language-engine-witness architecture

This repository owns one capability: launch the four delivered runtime libraries as four separate OS processes, subscribe downstream before input, carry the real `spirit-min.schema` fixture through Schema → Nomos → Logos push relays and typed Unix-socket contracts, and independently compile the emitted and reference Rust libraries with an identical rkyv dependency manifest and shared public-surface behavior tests.

The witness then kills all four processes and restarts every one of them against the same isolated database and Unix sockets. It proves the first document and every stored root recover durably, re-establishes the downstream subscription on the rebound Logos socket, and drives a genuinely second fixture (`second-min.schema`) end to end through the restarted pipeline — asserting its generated Rust arrives byte-exact through a push (timeout-guarded, never polled) and stores durably in Sema. It never touches production Spirit state.
