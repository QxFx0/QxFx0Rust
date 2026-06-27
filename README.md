# QxFx0 Rust

**Deterministic philosophical dialogue runtime — Rust re-architecture**

Rust implementation of QxFx0, a deterministic philosophical dialogue system. Semantics come from typed graphs + spreading activation; surface form from template-based syntactic generation with morphological agreement. Same input + state → same output, every time.

Haskell QxFx0 v0.1.0 (1247 tests, CI green) serves as the specification. This is a re-architecture, not a port.

## Architecture

```
qxfx0-types        — AtomGraph, RelationType (47 variants), Field, SystemState, EssenceState, GovernanceLog
qxfx0-self         — Conatus, Adjunction, Salience, Essence (Σ-typed trajectory), Deliberation (6-rule reconciliation)
qxfx0-semantic     — SeedGraph, SemanticNetwork (spreading activation), ContentSelector, DiscourseComposer, SyntacticGenerator, TemplateRegistry, DeriveAtoms, PathFinder, ConjugateComposer, PropositionParser
qxfx0-commitment   — SemanticCommitmentStore (commit/revise/contradict/retrieve/detect_engagement)
qxfx0-morphology   — 6-case Russian morphology (heuristic suffix rules + lookup)
qxfx0-guard        — ContentQualityGate (6 checks) + post_render_safety (empty, length, metadata-leak, toxicity, repetition)
qxfx0-persistence  — SQLite (rusqlite), save/load/list/delete sessions, SystemState round-trip
qxfx0-pipeline     — 6-stage TurnPipeline (Prepare→Route→Render→Finalize→Guard→Persist)
qxfx0-governance   — Append-only governance log with cycle detection, replay verification
qxfx0-render       — Frame-based rendering with graph-derived composition (PathFinder integration)
qxfx0-cli          — CLI binary (turn/chat/selfplay/discover/doctor/sessions/version)
```

## Key Design Decisions

- **BTreeMap throughout** — deterministic iteration order (no HashMap randomization)
- **Spreading activation** — multi-hop (3 hops, decay 0.5) over two-layer graph (explicit + substrate co-occurrence)
- **ContentSelector** — field-modulated predicate selection with affinity scoring
- **Deliberation framework** — 6-rule priority-ordered reconciliation (Haskell ADR-0011 port)
- **DeriveAtoms inference** — 3 production rules creating new atoms from state patterns
- **No GF dependency** — custom Rust morphology engine (heuristic suffix rules)
- **85K LOC lexicon** — replaced by runtime data (seed graph + substrate enrichment)

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
свобода предполагает возможность выбора. потому что без выбора действие не отличается
от рефлекса. свобода определяет отсутствие принуждения. свобода требует сознания.
свобода ограничена ответственностью. именно поэтому ответственность не враг свободы,
а условие её осмысленности. но свобода контрастирует с истиной
```

## Testing

```bash
cargo test        # 203 tests
cargo clippy      # 33 warnings (style-only)
```

| Crate | Tests |
|-------|-------|
| qxfx0-types | 22 |
| qxfx0-self | 20 |
| qxfx0-semantic | 71 |
| qxfx0-commitment | 12 |
| qxfx0-morphology | 17 |
| qxfx0-guard | 10 |
| qxfx0-persistence | 7 |
| qxfx0-pipeline | 24 |
| qxfx0-governance | 11 |
| qxfx0-render | 6 |
| qxfx0-cli | 3 |
| **Total** | **203** |

## Determinism Guarantees

- BTreeMap for all collections (deterministic iteration)
- No `HashMap` with random seed
- No floating-point nondeterminism (same inputs → same outputs)
- Pipeline test verifies: same input + same state → same output

## License

MIT
