# Fact-grounded integration evidence

## Baseline

- Clean integration worktree created from `origin/main` at `dd9ef3d`.
- Feature source remained at `040965f`; common base is `6f0288c`.
- Main worktree and feature worktree remained clean and unchanged.

## Implemented boundaries

- Curated immutable `FactId`/`FactRecord`/`FactRegistry` and manifest-validated
  `KnowledgePackSet`; active pack fingerprint is
  `deb023728e10a0ba2b3a475df7e303e3e7f0a617a97189d12104d64b2796166b`.
- Main `PerspectiveRegistry`/`PerspectiveProjection` and signed Ed25519
  authority contracts were preserved.
- Fact-grounded `PerspectiveState`/`OpinionCore`/`PerspectiveEpisode` is a
  separate bounded session layer; no raw or generated text is representable.
- SQLite schema v9 adds `perspective_json` and fail-closed pack/fact/JSON
  validation.
- `FactGroundedRollout` defaults to `Disabled`; composition preserves exact
  signed authority and never maps `Rejected` to `Opposed`.
- Feature ADRs are numbered 0028–0032; ADR-0033 documents composition.

## Verification

- `cargo fmt --all -- --check`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `cargo test --workspace --all-targets -q`: pass.
- `cargo build --workspace --release`: pass.
- Doctor: 11/11, including pack, FactRegistry, Perspective and stance checks.
- Renderer audit test: 30 admitted topics pass.
- `git diff --check`: pass.
- Final bundle verified by `git bundle verify`; SHA-256 is recorded in
  `.forge/session-backups/20260801-integration-final/SHA256SUMS`.

External `QxFx0TurnService` and `QxFx0StanceIssuer` repositories were not
modified; their existing main contracts remain transport/key-boundary
interfaces. PR merge, limited enablement, soak, recovery and stable release
remain explicit follow-up operations.
