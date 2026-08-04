# Topic corpus expansion framework

## Boundaries

Topic recognition, factual grounding, audited realization, canary authority,
and production-stable canary operation are separate maturity levels:

1. `recognized`: the canonical topic is in `COVERED_TOPICS`.
2. `grounded`: the active pack has 2-5 curated FactIds for the topic.
3. `audited`: claims, discourse identity, authority, surfaces, lexical evidence,
   valency, and morphology pass the audited corpus contracts.
4. `canary`: explicit V2 authority includes the topic. This never changes the
   V1 production default.
5. `production_stable`: an exact-SHA observation window accepted the existing
   canary topic.

Higher maturity implies every lower level. The compiler fails closed if the
repository violates this ordering.

## Batch workflow

Every expansion batch contains 10-20 canonical topics. Its review source must
declare a cluster, target maturity, review status, and claim source. The v1
compiler imports claims only from the current audited corpus; adding genuinely
new topics first requires reviewed semantic assets and FactIds in a separate
PR. This prevents a batch proposal from silently becoming factual authority.

Compile and verify a batch:

```bash
python3 tools/topic_corpus_pipeline.py inventory --output target/topic-maturity.json
python3 tools/topic_corpus_pipeline.py compile \
  data/corpus-batches/2026-08-audited-clusters-v1.json \
  --output target/topic-corpus-batch.json
python3 -m unittest tools/test_topic_corpus_pipeline.py
```

The compiled output includes canonical topic identity, 2-5 FactId bindings,
thesis/counterpoint/consequence roles, relation frames, realization strategy,
lexical or fixed-surface witnesses, approved surface digests, morphology
completeness, generated positive/negative cases, review status, and a
domain-separated deterministic digest.

No compiler command edits `argued_topics.tsv`, a knowledge pack, the runtime
allowlist, or an observation completion record. Promotion always requires a
separate reviewed PR and a new exact-SHA observation window.

## Six-topic stabilization

The existing six-topic canary is frozen while this framework is evaluated.
`.github/workflows/response-plan-v2-operational-evidence.yml` runs weekly and
on demand against reviewed formulations. Evidence records deterministic traces,
case IDs, input classes, expected/actual authority results, and digests; it does
not retain raw user logs.

Real user formulations may be added only after privacy review and
de-identification, as explicit cases in
`data/gates/response-plan-v2/operational-formulations-v1.json`. That change is a
normal reviewed PR. Unsupported intents and semantic-route probes are counted
separately from successful definitions. The workflow never changes authority
or promotes a topic.

## Scale target

The target is a maturity funnel, not a universal allowlist:

```text
1000 recognized
 300 grounded
 100 audited
20-50 canary-authorized
```

Counts are directional capacity goals, not permission to weaken admission,
language review, replay, realization, or observation gates.
