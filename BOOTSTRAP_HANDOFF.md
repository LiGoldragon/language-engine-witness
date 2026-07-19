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

## Cross-examination: manifest foundation

The train was compared with current `main` in producer order after the
manifest-backed TextualForm foundation landed.

- **Protos:** retain the full encoded-form rename and the new named-chunk
  TextualForm operation. A manifest now selects exactly one declared chunk; an
  absent or duplicate file is a typed failure. This is a general view-side
  operation and does not introduce name work into Nomos.
- **core-schema:** retain the explicit file manifest, dependency ordering, and
  manifest StructureTree. One shared NameTable serves every file; the same
  structure drives dependency-first read and manifest-order emission. The new
  surface passed full Nix checks with the matching Protos pin.
- **Not merged:** no bootstrap commit was merged into `main`. The next producer
  step needs a positional source/encoded-form ruling for aliases, unit and
  payload interface variants, streaming relations, and trait/impl document
  slots. Those forms have no verified lawful spelling, so the train must not
  invent one or silently lower an alias as a newtype.
- **Witness status:** the four-daemon process/restart proof remains acceptance
  evidence for the completed consolidated-Protos daemon cascade only. It does
  not claim the blocked Spirit source migration or a production-snapshot run.

At the next cross-examination, retain, revise, or discard this foundation only
with the subsequent source-form ruling, then rerun the witness process check
and the full Nix checks over the resulting merged closure.
