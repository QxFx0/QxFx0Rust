# ADR 0040: Thesis-state persistence and rollout

Status: accepted — schema and observation boundary only

Schema v10 adds nullable `thesis_state_json` to `session_semantic` without rewriting v9 rows or removing legacy storage. NULL means the bounded default. The same transaction writes monolithic compatibility state, graph, and semantic projections; validation and the 4 MiB thesis JSON cap happen before opening it.

Pipeline receipts bind session/turn, active pack fingerprint, authority `FactId`, catalog `ThesisId`, canonical `ThesisDigest`, and response digest. Disabled is the safe default and Shadow validates only. Shadow is also suppressed when Response Plan V2 authority is enabled: joint observation would be a new promotion boundary. There is no `Active` projection in the turn pipeline or CLI: the v10 column and typed lifecycle contract are reserved for a later, separately reviewed persistence promotion with retention, correction, export, deletion, transaction, and observation-window evidence. Projection accepts only the validated catalog/pack registry, never user/generated text.
