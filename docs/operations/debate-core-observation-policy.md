# Debate Core v1 observation policy

Debate Core observation remains default-off and has no authority. The curated
manifest at `data/gates/debate-core/observation-corpus-v1.json` contains only
reviewed formulations and explicitly forbids raw-user-log ingestion. The
retained artifact contains receipt-only JSONL files and aggregate digests; it
does not retain prompts, rendered responses, session IDs, request IDs, or
SQLite databases.

Each scenario runs through three isolated fresh-process paths:

1. ordinary baseline;
2. `DebateCoreMode::TraceOnly`;
3. trace-only deterministic replay.

The harness requires byte-identical response output, byte-identical serialized
`SystemState`, byte-identical receipts on replay, valid receipt digests, expected
typed topic/move classifications, and absence of privacy needles. Databases and
captured stdout remain in temporary storage and are deleted before the artifact
is published.

The v1 corpus covers definitions, assertions, challenges, distinctions,
grounding requests, counterarguments, consequences, topic connections,
reflections, clarification, greetings, unknown and repeated-unknown topics,
external subjects, guard-blocked inputs, and fallback plans. Some input classes
currently converge on the same typed move; the report records that distribution
rather than granting missing taxonomy paths authority.

All first-window failure budgets are zero for validation, replay, privacy,
output parity, state parity, digest mismatch, and invalid graph references.
Successful observation does not promote feedback, persistence, routing,
planning, rendering, guard, V2, stance, or governance authority.

Run locally against an already-built binary:

```bash
python3 tools/debate_observation.py validate
python3 tools/debate_observation.py run \
  --binary target/release/qxfx0 \
  --output target/debate-core-observation
```

The GitHub workflow requires an exact lowercase 40-character build SHA that is
already an ancestor of `main`, and uploads the result as a 90-day evidence
artifact. Any authority or cross-turn persistence requires a separate ADR, PR,
canary, and promotion decision.
