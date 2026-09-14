# Editorial wave 1 — inventory (2026-09-14, no merge)

## Inventory result: empty by evidence

Scanned all operator databases for `promotion_overlays` rows:

- `/home/liskil/state/qxfx3.db` — no promotion tables (old schema)
- `/home/liskil/QxFx0Backups/automatic/*.db` (14 files, 2026-08-30…09-13):
  only `qxfx0-20260911/12/13` carry promotion tables, all with
  **0 overlays and 0 evaluations**.

No overlay was ever drafted, let alone released, in any operator
database. There is nothing to export and nothing to merge. This
directory records the empty finding so the next wave starts from
fact, not assumption.

## Procedure for a non-empty wave (when Released overlays exist)

1. `qxfx0 promotion export-pack <version> --out wave-N/<version>.json`
   per Released overlay (refuses anything but Released; never
   overwrites).
2. Group the exported predicates by topic (one section per topic).
3. Human review per predicate: admit into pack sources, refuse with
   reason, or defer. Record all three lists here.
4. Rebuild the pack; pack gates (census/parse/doctor) must pass.
5. Only then: next promotion cycle may reference the admitted content.

Automation stops at step 1 — graph effect comes only from editorial
admission (ADR-0043 U5.4).
