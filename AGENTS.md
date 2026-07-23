# Agent guidance
Keep this a process-level acceptance witness. Never replace component processes with stubs or touch production Spirit state.

The acceptance bar is working programs: the pipeline's emitted Rust must compile under its locked manifest and pass its public-surface behavior tests. The witness does not byte-compare the emission against a schema-rust oracle projection; do not reintroduce a reference-generation or byte-equivalence stage.

This repository is under fast development and constantly breaking.
