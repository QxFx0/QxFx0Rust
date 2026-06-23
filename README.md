# QxFx0 Rust

**Deterministic philosophical dialogue runtime — Rust re-architecture**

Rust implementation of QxFx0, a system that builds meaning through typed semantic graphs and morphological reconstruction — not templates, not stochastic sampling. Same input + state → same output, every time.

Haskell QxFx0 v0.1.0 (1247 tests, CI green) serves as the specification. This is a re-architecture, not a port.

## Architecture

```
qxfx0-types        — 47 RelationType, 35 PropositionType, 16 SemanticIntent, Field, Frame, SystemState, AtomGraph, CommitmentStore
qxfx0-self         — Conatus (C(b,v) functional), Adjunction (triangle identities), Essence (Σ-typed), SelfBlanket, Salience
qxfx0-semantic     — AtomStore seed graph, PathFinder, GeneratedPredicateGate (5 gates), ContextualComposer (5 modes), PropositionParser, GraphEngagement
qxfx0-commitment   — SemanticCommitmentStore (commit/revise/contradict/retrieve/detect_engagement)
qxfx0-morphology   — 6-case Russian morphology (heuristic suffix rules + lookup)
qxfx0-guard        — ContentQualityGate (6 checks), post-render safety, finalize_output (fail-closed)
qxfx0-persistence  — SQLite (rusqlite), save/load/list/delete sessions, SystemState round-trip
qxfx0-pipeline     — 6-stage TurnPipeline (Prepare→Route→Render→Finalize→Guard→Persist)
qxfx0-governance   — (stub — append-only history, replay gate)
qxfx0-render       — (stub — frame dispatch in pipeline)
qxfx0-cli          — CLI binary (turn/chat/doctor/sessions/version)
```

## Key Design Decisions

- **BTreeMap throughout** — deterministic iteration order (no HashMap randomization)
- **No GC** — Rust ownership model, no `unsafePerformIO` equivalent
- **No GF dependency** — custom Rust morphology engine (heuristic suffix rules)
- **85K LOC lexicon** — replaced by runtime data (seed graph in code, extensible via data files)
- **Categorical adjunction** — runtime checks + property tests (triangle identities verified)

## Quick Start

```bash
# Build
cargo build

# Single turn
cargo run -- turn "что такое свобода?"

# Multi-turn (with persistence)
cargo run -- --session-id my-session --db /tmp/qxfx0.db turn "что такое свобода?"
cargo run -- --session-id my-session --db /tmp/qxfx0.db turn "свобода это просто отсутствие ограничений"
cargo run -- --session-id my-session --db /tmp/qxfx0.db turn "что ты думаешь об ответственности?"

# Interactive
cargo run -- chat --session-id my-session --db /tmp/qxfx0.db

# Health check
cargo run -- doctor

# List sessions
cargo run -- sessions --db /tmp/qxfx0.db
```

## Example Output

```
> что такое свобода?
Известно, что свобода предполагает возможность выбора. Но свобода контрастирует с истина.
Именно поэтому свобода требует не только возможности, но и осознанности выбора.

> свобода это просто отсутствие ограничений
Я удерживаю позицию: свобода предполагает возможность выбора. свобода определяет
отсутствие принуждения. свобода требует сознание. Но свобода контрастирует с истина
```

## Testing

```bash
cargo test        # 55 tests
cargo clippy      # 0 warnings
```

| Crate | Tests |
|-------|-------|
| qxfx0-self | 8 |
| qxfx0-semantic | 10 |
| qxfx0-commitment | 8 |
| qxfx0-morphology | 9 |
| qxfx0-guard | 6 |
| qxfx0-persistence | 6 |
| qxfx0-pipeline | 8 |
| **Total** | **55** |

## Determinism Guarantees

- BTreeMap for all collections (deterministic iteration)
- No `HashMap` with random seed
- No floating-point nondeterminism (same inputs → same outputs)
- Pipeline test verifies: same input + same state → same output

## License

MIT
