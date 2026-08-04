# Debate Core v1 observation completion

The first Debate Core v1 observation window completed on 2026-08-04 against
exact merge commit `c014a1cfd8aa93f26ffe2eb1e91adffcc59833da` from `main`.
GitHub Actions run
[`30938119122`](https://github.com/QxFx0/QxFx0Rust/actions/runs/30938119122)
completed successfully and uploaded artifact `8904024099`.

The artifact SHA-256 is
`fc3c8d6f302eed4fb25aa473c6600ed7713334c6b031c358d20410ea1dbede04`.
Its internal deterministic evidence digest is
`cf4b3f83fc3557c1d53f7cb6b13b821726a6f6f39833ef94cfcfbf960514ccd4`.

The window covered 16 scenarios and 17 turns. It generated 17 validated
receipts (10000 basis points) with zero validation failures, replay failures,
privacy violations, output parity violations, state parity violations, digest
mismatches, or invalid graph references. A separate scan of the downloaded
artifact found no reviewed prompts, rendered responses, session markers,
unknown-topic labels, or SQLite payloads.

Observed move coverage was intentionally descriptive rather than promotional:
`ground` 5, `define` 4, `assert` 2, `challenge` 2, `connect` 2, `contact` 1,
and `reflect` 1. Five receipts had empty fallback graphs, two had one-node
dialogue/external graphs, and ten had three nodes with two edges. These
distributions expose current v1 convergence and do not imply full taxonomy
coverage.

This completion record changes no authority, persistence, renderer, routing,
planning, guard, stance, governance, or feedback behavior. Debate Core remains
default-off and observation-only. User Argument Parsing v1, feedback, and any
cross-turn position ledger remain separate design and promotion boundaries.

The machine-readable record is
[`debate-core-observation-completion-v1.json`](debate-core-observation-completion-v1.json).
