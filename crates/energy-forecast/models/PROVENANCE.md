# Provenance — `pv_base.json`

The served learned PV artifact, copied byte for byte from the Python add-on. It is **not** generated
by this repository; it is the model the add-on already serves, and P4's job is to reproduce its
inference exactly.

| | |
| --- | --- |
| Source | `/root/HomeAssitant/ha-energy-manager/energy_manager/app/forecast/models/pv_base.json` |
| Upstream commit | `00092d769d1ec574dc56a152771e83e5aa3f70cc` (add-on 0.38.8) |
| Byte-identical copy at | `tools/forecast_ml/models/pv_base_v6.json` (same sha256) |
| Size | 1,193,351 bytes |
| sha256 | `25d96fcaf05850c017322ddae4c083ec3032077f79f8c122b395787e977c3f26` |
| Kind | `hist_gradient_boosted_trees_exact_bits` |
| Features | 28, frozen order, digest `028ba35d4f44d148` |
| Ensemble | 400 trees, 23,336 nodes, 11,868 leaves, max depth 5 |
| Baseline | `0.8870297807686697`, clamp `[0.3, 1.8]` |
| Trained | `2026-10-14T17:24:32Z`, 1,885,671 rows over 174 systems, 1,461 days |
| Corpus | Utrecht 175 residential PV systems, CC-BY-4.0, doi `10.5281/zenodo.10953360` |

## What is pinned

* `tests/pv_golden.rs` checks `sha256`-level behaviour through the committed golden vectors: the
  artifact's 800 ratios compare equal bit for bit, including the raw sum, the clamp and the physics
  that the features describe.
* The loader refuses a different `kind`, a reordered or renamed feature list, a digest that does not
  match this build, a truncated tree, a dangling child index and an unusable clamp. A stale model
  cannot be served silently.

## Refreshing the artifact

Copy the new `pv_base.json` over this one, regenerate the golden vectors with the add-on's
`tools/forecast_ml/emit_golden.py`, refresh `tests/fixtures/pv_golden.jsonl`,
`tests/fixtures/pv_golden_full.jsonl` and `tests/fixtures/pv_features.jsonl` (see
`tests/fixtures/PROVENANCE.md`), then update the sha256 and metadata here. A model change is a
contract change: the parity gate is the only thing that says the port still serves it.

## Licence

The artifact carries its own `corpus` and `attribution` fields; both are CC-BY-4.0 and are read into
`ModelMetadata` so the add-on's status can show them.
