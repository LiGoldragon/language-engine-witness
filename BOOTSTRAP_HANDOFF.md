# Bootstrap handoff

`SpiritLineageBTrain` is the dedicated bootstrap train for the Protos engine.
It deliberately remains separate from `main` while the remaining Spirit
Lineage-B capability work lands.

## Delivered in this train

- the full public `Core*` to `Encoded*` API rename through Protos, schema,
  Logos, Nomos, Rust projection, storage contracts, daemons, and witness;
- all machinery and daemon pins converged onto the consolidated `protos`
  workspace; and
- the four-process `language-engine-witness` working-program check, including
  restart recovery, passed with this train's exact pins.

## Required next cross-examination

Before any train commit is merged into `main`, compare every `main` versus
`SpiritLineageBTrain` diff across the producer-to-witness dependency order.
Record which bootstrap changes are retained, revised, or discarded, then rerun
both the witness process check and the relevant full Nix checks from the merged
closure. Do not treat this branch as merged before that comparison.

## Remaining accepted work

The Spirit migration still needs the authorized schema-manifest, richer
contract/streaming relation, daemon-generation, and private isolated
production-snapshot acceptance phases. The production database has not been
read or copied by this train.
