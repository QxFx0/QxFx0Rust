# ADR-0034: ResponsePlan V2 — proposition, derivation, authority, discourse, realization boundaries

- Status: Accepted
- Date: 2026-08-02

## Context

The Rust renderer currently generates declarative surface from graph-selected
predicates that are verbalized by string templates in
`data/semantic/templates/templates.json`. Three independent encodings carry the
same rhetorical semantics — `ClaimRole`, `SemanticProposition`, and
`DiscourseRelation` — so a plan whose `ClaimRole::Thesis` carries a
`SemanticProposition::Counterpoint` under `DiscourseRelation::Elaboration`
assembles without error. This is not hypothetical: a ratified census confirms
that the current `audited_plan` renderer validates claim roles but then extracts
pre-written surfaces (qxfx0-render/src/content_plan.rs), so none of the three
encodings is derived from the other two.

The same census found a live production defect. Of 127 surface templates, 12
are gender-sensitive: 5 already use the explicit `{FROM_G}`/`{TO_G}` slot
mechanism, and 7 hard-code a fixed agreement in the surface string
(`ограничена`, `направлена`, `способна`, `абсолютной`, `невозможна`,
`подлинную`, `невозможно`); of those, six are feminine and one is neuter. The
30 audited topics include 15 non-feminine subjects (разум, долг, страх, труд,
произвол, язык, бытие, сознание …), and the release binary currently emits
`разум направлена на истину`. `renderer-audit` does not catch this class of
error because the emitted string is unique — uniqueness is not grammatical
correctness.

The lexical census fed this boundary set: 127 templates across 33 relation
types, 5 with explicit `{_G}` variants, 7 with hard-coded fixed agreement.
The total 127/33/5/7 figures are confirmed against the embedded
`templates.json`; the byte-parity count `120` is diagnostic output of the F0
census, not a contract value.

The Haskell reference ship has richer discourse scaffolding (typed
propositions with `Conditional`, `Conjunction`, `Contrast`, `Question`,
`Qualification`; a `ResponsePlan` with proposed plan objects; a shared GF
abstract syntax). The Rust port deliberately shipped a route-based renderer
first (ADR-0028) and a shadow audited plan (ADR-0013 … ADR-0016). The Rust
codebase does not contain a recursive proposition algebra, an explicit
derivation layer, an assertion gate distinct from evidence authority, or a
linearization layer separated from template strings.

This ADR defines the V2 response planning architecture as a strict chain of
boundaries. It is a design record, not a release; adoption is phased and always
fail-closed to the V1 audited renderer (ADR-0028) or a typed non-declarative
fallback.

## Decision

The V2 architecture separates seven responsibilities and passes data through
five artifact states:

```
CandidateDraft                       prepared by planner enumeration
→ CandidateResponsePlan               try_new packs two DAGs and one tree
→ LeafAdmittedPlan                    LeafAdmissionProof
→ EvidenceCertifiedPlan               EvidenceAuthorityCertificate (as_of)
→ AssertionAuthorizedPlan             recursive per-constructor policy
→ RealizablePlan                      ResolvedSynTree + completeness certificate
→ RealizedSurface                     execution receipt (surface + digests)
```

Each boundary is a distinct typed certificate. The boundaries are not to be
conflated, and the certificate of one boundary never borrows the assertions of
another.

### 1. Candidate structure

```
struct CandidateResponsePlan {
    propositions: BTreeMap<PropositionId, PropositionNode>,
    derivations:  BTreeMap<DerivationId, DerivationNode>,
    discourse:    DiscourseTree<PropositionId>,
}
```

- Built by three independent constructors, then validated by
  `CandidateResponsePlan::try_new`, which is the only way to construct the
  type. It returns `CandidateInvariantError` on any inconsistency; `try_new`
  treats a raw triple of collections as a structural certificate.
- The error vocabulary of `try_new` is reusable by the gates but is not the
  whole fallback enumerable; `V2Failure` wraps the full closure:

```
enum V2Failure {
    Candidate(CandidateInvariantError),
    Admission(LeafAdmissionError),
    Evidence(EvidenceAuthorityError),
    Assertion(AssertionAuthorityError),
    Realization(RealizationError),
    Budget(BudgetExceeded),
    Snapshot(SnapshotError),
}
```

- `V2Failure` is the turn-level envelope. Startup-level failures (a corrupt or
  drifted pack, an internally inconsistent schema) fail loud at startup /
  `doctor`, never as turn-level typed rejection.

### 2. Identity addressing

- `PropositionId` is content-addressed and Merkle-shaped:

  ```
  PropositionId = h("qxfx0:proposition:v1" ∥ type ∥ canonical_payload ∥ child_ids)
  ```

  Per-constructor canon is fixed in the canonical serialization version:
  conjunctions sort their children; `Conditional` premises are positional;
  repeated premises keep multiset semantics.

- `DiscourseOccurrenceId` is a local occurrence inside one canonical plan and
  must be globally unique. The bare path form `[0,1]` is not globally stable;
  it is either scoped to the plan or extended to
  `(discourse_root_digest, canonical_path)`.

- `ClaimId` is derived on load / never stored:

  `ClaimId = H("qxfx0:claim:v1" ∥ proposition_id ∥ canonical_constructor_path)`

- `ClaimRole` is a projection of the discourse tree:
  `projected_roles(&DiscourseTree) -> BTreeMap<ClaimId, ClaimRole>`. It is not
  a stored field and never part of persisted JSON. The rhetorical constructor
  must be the single role namespace; `ArguedTopic` remains the admission
  canon that validates leaves, not a second role source.

- No `root_ids` collection in the DAG: roots are derived from discourse.

### 3. Derivation

- Entailment is a separate typed layer; graph connectivity (BFS, adjacency)
  is not implication:

  ```
  - premises:  NonEmpty<PropositionId>
  - conclusion: PropositionId
  - rule: InferenceRuleId           // whitelist gated
  - evidence: EvidenceRef
  - confidence: BoundedConfidence
  ```

- Only whitelisted-composed rules may produce a conclusion. The inference
  gate must confirm the derivation; connectivity only produces candidates.

### 4. Admission and evidence — two certificates, not one

- `AdmissionProof`: membership — a `PredicateRef`/`FactId` belongs to the
  allowed registry/profile (statically, per pack set). Stable: the same
  membership holds across versions.
- `EvidenceAuthorityCertificate`: selectability — the claim is active under
  the current `AuthoritySnapshot` and `as_of`. Temporal, provenance-validated,
  dependency-resolved. References `admission_proof_digest` rather than copying
  its content.

Both are confirmed by an existing behavior: changing pack conditions
without changing the `FactId`s invalidates a persisted `Perspective`
(see `changed_pack_conditions_with_same_fact_ids_invalidate_perspective`).
A merged certificate cannot express whether a same-`FactId`, updated-conditions
change should invalidate evidence; therefore the two boundaries are not merged.

### 5. Assertion authority

`Assertable(p)` and `Derivable(p)` are two different verdicts. Being provable
does not confer the right to assert the conclusion to a user.

Recursive per-constructor policy:

```
Predicate(A)        → curated FactId(A)
Conjunction(A, B)   → authority(A) ∧ authority(B) ∧ admitted conjunction rule
Conditional(A, B)   → authority(A) ∧ authority(B) ∧ admitted implication proof
Qualification(q, A) → authority(A) ∧ admitted_qualification(q)
                     ∧ confidence(A) satisfies q
Consequence(A, B)   → authority(B) ∧ admitted derivation(A ⇒ B)
```

`required(tree) := ∧ required(children) ⊔ leaf_requirement(kind)`;
`supplied(evidence) ⊇ required` with fail-closed on the complement. This is
checked on the plan as a whole, not on a single proposition leaf: a dialogue
act wrapping a declarative claim must satisfy the claim's fact binding.

`ClaimAuthority` is closed:

```
enum ClaimAuthority {
    Curated { fact_id: FactId },
    DerivedCuratedConclusion { conclusion_fact_id: FactId, derivation_id: DerivationId },
    DerivedNovelConclusion { derivation_id: DerivationId, assertion_policy: AssertionPolicyId },
}
```

- `Curated` and `DerivedCuratedConclusion` are acceptable in V1.
- `DerivedNovelConclusion` is a future release, staged onto the existing
  signed-attestation / temporal-provenance discipline (ADRs 0024-0026).

Semantic self-constraints follow the established `attestation` rule set:

- `NotAuthorized ≠ False`
- `NotAuthorized ≠ Refuted`
- `Rejected ≠ Opposed`

### 6. Fallback and failure

The fallback chain is fixed and narrowed:

```
V2 realizable                        → V2 renderer
V2 not realizable + audited V1      → audited V1 renderer (ADR-0028)
otherwise                           → typed non-declarative fallback
```

`legacy_graph` is forbidden as a declarative fallback; the path it represents
was closed by ADR-0028. Every failure receives a typed reason:

```
enum V2FailureReason {
    NoDerivablePlan,
    NoAdmittedEvidence,
    NoRealizableSyntax,
    UnsupportedTemporalPolicy,
    AuthoritySnapshotMismatch,
}
```

- `V2Attempt` distinguishes three cases:

  ```
  NotApplicable { route }
  Rejected { artifact, failure }
  Realizable(RealizablePlan)
  ```

- A rejected attempt preserves everything that was canonically certified up to
  the point of failure as a bounded partial artifact with a preserved proof.
- Truncation is explicit, never silent:

  ```
  TruncationWitness {
    phase, triggered_limit,
    planning_policy_digest,
    attempt_input_digest,
    visited_digest,
    pending_frontier_digest,
  }
  ```

  `attempt_input_digest` and `pending_frontier_digest` distinguish two inputs
  stopped at the same failed limit. A truncation witness is an infrastructure
  artifact and is never a node of the proposition DAG: it is not part of the
  semantic meaning. `PropositionClosure` contains only actually constructed
  semantic nodes.

- `Failure → fallback` is a deterministic table, not a renderer choice:

  - `AssertionAuthorityError::NotAuthorized` → typed non-declarative fallback,
    not V1 (the claim is not realizable in this frame).
  - `RealizationError::IncompleteForm` → audited V1 possible.
  - `SnapshotError` / authority mismatch → fail loud at startup; no
    turn-level fallback and no recomputation under a different snapshot.

### 7. Realization

Realization is separated from the plan constructor and the realization
adapter is deliberately late-bound, but the completeness of a pre-render
boundary is proven structurally before linearization:

```
RealizablePlan {
    authorized: AssertionAuthorizedPlan,
    resolved_syn_tree: ResolvedSynTree,
    realization_snapshot_digest: Digest,
    completeness_certificate: RealizationCompletenessCertificate,
}
```

- `ResolvedSynTree` contains no unresolved slots: every word is chosen, every
  frame and form resolved.
- `linearize(RealizablePlan) -> RealizedSurface` is total for that snapshot.
  An error after `RealizablePlan` is issued can only be an invariant violation
  or a `RealizationSnapshotMismatch`; "incomplete morphology" must have been
  detected at the completeness gate before the plan was issued.
- Case is assigned by the valency frame of the governing head (lexicon), not
  by the semantic plan: `зависеть → от + gen`; `управлять → + inst`. The plan
  carries only which relation is said; the lexicon, which government; the
  linearizer computes the forms.
- The valency/number/lexicon inventory is defined independently of the GF
  five-case source, which lacks the dative case already present in the
  templates (`{FROM|dat}`, `{OBJ|dat}`).
- The surface results from a typed chain, not from string-slot replacement:

  ```
  ProposalDag
  → DiscourseTree<PropositionId>
  → SynTree(DiscourseOccurrenceId)   // NP/VP/Clause
  → Linearizer → Surface
  ```

- `RoundTripClass` marks morphology triples:
  `{ Bijective | Ambiguous | Suppletive | OrthographicVariant }`.
  `Bijective` keeps the strong equality `lemma(inflect(x)) == x`; the others
  are tested as `lemma ∈ analyze(generate(lemma, features))` plus membership
  of the wanted feature bundle.

### 8. Snapshot and determinism

The turn contract is divided into four domains so a change of cause is never
masked as a change of authority:

```
TurnContractSnapshot {
    authority:  AuthoritySnapshot,     // pack_set ∘ inference_rules ∘ assertion_policy
    planning:   PlanningPolicySnapshot, // budgets + canonicalization_version
    realization RealizationSnapshot,    // lexicon ∘ grammar ∘ morphology
    selection:  SelectionPolicySnapshot, // self_policy ∘ ranking_version ∘ numeric_semantics
}
```

- The whole `TurnContractSnapshot` goes into the `TurnRecord` and the stage
  digest. Each changeable domain has its own fingerprint, so a change of
  planning policy is visible as a planning-policy digest change, not an
  authority one.
- Fixed-point (basis-points) inputs and integer/fixed-point scoring. Where a
  field value such as `conatus` is currently computed via `f64::ln`, it is
  deterministically quantified at the boundary of
  field state → `SelfSelectionContext`. Everything after that boundary is
  fixed-point. `numeric_semantics_version` binds the quantification function,
  its rounding mode, and the tie-break, so cross-platform replay is not merely
  described but enforced. A CI reference-vector digest is run against an
  alternative `libm` platform.
- Selection total order: score descending, then candidate Merkle root
  ascending as tie-break. The `SelfSelectionContext` (conatus/salience/doubt)
  supplies input; `SelectionPolicySnapshot` supplies policy. The subject
  state never re-opens the plan to mutation after selection; certificates are
  revoked on any mutation, requiring a fresh `try_new`.
- Replay is defined as three levels: integrity verification (no assets),
  authority verification (manifest/attestation available), and reproduction
  (exact assets + binary available). V1 implements integrity and authority
  verification; full reproduction remains an explicit `SnapshotUnavailable`
  fail-closed rather than a silent current-grammar host replay.

### 9. Evidence temporal axes

- `EvidenceEvaluationContext`:
  - `logical_turn: LogicalTurnId` — `(session_id, turn_seq)`, the governance
    epoch of ADRs 0012–0027.
  - `authority_as_of: Option<PersistedTimestamp>` — trusted persisted
    calendar instant for calendar-relative temporal policies; wall clock may be
    read only at the creation of the input envelope and recorded.
  - `authority_snapshot_digest`.
  - V1: `authority_as_of` is `None`; calendar-relative policies fail closed as
    unsupported. Evidence is evaluated at the turn-relative `logical_turn`.

### 10. CLI and gates

- `qxfx0 doctor` gains version-contract gates named by
  `--gate response-plan-v2-phase-{a,b,c}`, `--gate response-plan-v2-replay`,
  `--gate response-plan-v2-zero-downgrade`, and
  `--gate response-plan-v2-canary-report`.
- Phase A gate: byte-parity is required only on the fingerprinted
  `template-agreement-matrix` rows whose `parity_class` is `byte`, restricted
  to instances without hard-coded agreement features; on the remaining rows
  (including the 7 gender-hard-coded templates after the F0 fix) the gate
  checks semantics and authority parity plus approved golden surfaces. The
  `response-plan-v2-audited-corpus` manifest and the
  `template-agreement-matrix` are two separate gates and are never merged.
- The two gate matrices:
  - `response-plan-v2-audited-corpus`: 30 topics × semantic + authority parity
    + approved V2 surfaces (fixtures are response surfaces).
  - `template-agreement-matrix`: 127 templates × compatible feature fixtures
    (masculine/feminine/neuter/plural), with
    `relation_type, template_index, fixture_id, parity_class, reason,
    expected_surface_digest`.
- A doctor "topic × template" cross-product is *not* used: a template is
  verified against compatible grammatical fixtures, not against every topic;
  that matrix would be partly inapplicable (many templates are irrelevant to
  any of the 30 topics).
- F0 census emits this manifest; a human approves it; gates read it. Counts
  like "120/127" are diagnostics, not contract rules.
- Approved golden diffs are themselves fingerprinted release artifacts.
- The canary report runs all 30 audited topics through the observational
  pipeline with `AuditedAuthority` eligibility, while V1 remains the emitted
  renderer. It must report zero `RealizationDowngrade`, state/output parity
  violations, semantic/authority/realization/replay/attestation parity
  violations, and unauthorized V1 fallback.
- The pipeline trace records the typed `V2AuthorityOutcome`, its digest, the
  emitted surface digest, the audited source digest when applicable, claim
  identity/fact-binding/claim-authority digests, and explicit parity fields.
  These are observational evidence only and do not enter persisted turn state.
- Authority selection is an explicit second switch, independent of V2
  observation: `ResponsePlanV2Authority::Disabled` is the default and keeps
  `RendererAuthority::LegacyShadow`; `ResponsePlanV2Authority::Canary` forces
  `ResponsePlanV2Mode::Canary` and may emit V2 only for `правда`, `произвол`,
  and `свобода` when the receipt is compositional or audited-verbatim.
- `AuthorityDecisionReceipt` binds the selected authority, typed outcome,
  contract digest, artifact digest, emitted surface digest, and replay bundle
  digest. The receipt is passed to rendering but is not persisted in semantic
  state. Disabling the authority switch is the explicit rollback to V1.
- Operators enable the canary only on `turn` with
  `--response-plan-v2-authority`; omission is the rollback control and keeps
  the production default on V1. `--response-plan-v2-trace-jsonl PATH` writes a
  create-new external `qxfx0.authority-trace.v1` record containing the full
  receipt and deterministic pipeline trace, never session state.
- `RealizationDowngrade` and `TypedNonDeclarative` receipts are never eligible
  for authoritative emission. Canary authority fails closed instead of calling
  any V1 renderer; only `Compositional` and `AuditedVerbatim` may emit.

### 11. F0 data placement

Authority-sensitive distinction: verified 30 topics and their
`FactRecord`s may live under `data/packs`; the raw Haskell corpus stays under
`data/imports/quarantine`. Moving the whole corpus into `packs` would
semantically be a promotion, which is closed by the gap record ADR-0029 until
reviewed admission.

### 12. Qualification and assertion inventory

- Every `Qualification(q, A)` requires `admitted(q)` with an exhaustive
  reviewable qualification map, and `confidence(A) satisfies q`. The map of
  admissible qualifiers is part of the `assertion_policy` and therefore of the
  `AuthoritySnapshot`. The system cannot assert "безусловно A" or "сомнительно
  A" for a fact whose authority authorized only A's content, not its epistemic
  strength.

## Consequences

- Determinism remains the top-level contract. Every V2 structure participates
  in the existing SHA-256 stage digest via canonical JSON; witness digests
  (e.g. `TruncationWitness`, `rejected_artifact`) live in the trace digest but
  not in the proposition closure digest, so infrastructure failures do not
  enter the semantic meaning.
- The V1 audited renderer (ADR-0028) remains authoritative while the V2 gates
  and cumulative canary report are observed. Passing those gates does not
  itself promote V2: authority promotion is a separate reviewed release
  action. V1 stays as the rollback oracle in the CI culture.
- The final semantics are asserted as one invariant, restated:
  `Derivable ≠ Assertable`, and `Assertable(declarative) requires a curated
  FactId in V1`. The derived stratum is compositional and explanatory even in
  V1; it does not invent facts.
- Runtime-graph-derived values can never become either an admissible leaf or
  an assertion premise for declarative output; memory is never an implicit
  authority source.

## Resolved in this ADR

The following debate outcomes are fixed:
- `TurnContractSnapshot` has four domains (authority, planning, realization,
  selection), so performance, policy, and right changes are never masked.
- The boundary chain is seven responsibilities → five artifacts via fixed
  names: `Candidate→LeafAdmitted→EvidenceCertified→AssertionAuthorized→
  Realizable→RealizedSurface`.
- Split of admission vs evidence is reaffirmed (both exist and reference one
  another via digest, cf. existing `Perspective` invalidation test).
- `TruncationWitness` is an infrastructure artifact, excluded from the
  proposition DAG.
- Realization has a pre-render completeness certificate and a post-render
  receipt; no realized surface digest appears before linearization.
- The versioned V2 joiner (`capitalized-punctuated-space-v2`) capitalizes the
  first alphabetic character of every emitted sentence and applies terminal
  punctuation deterministically. The audited corpus remains semantic source
  data; orthographic normalization belongs to realization.
- V1 retains the reviewed lexical `с` → `со` rule for `временем`; V2 uses the
  fingerprinted morphology-depth allomorph lexicon. This keeps the default V1
  renderer and V2 realization orthographically aligned for the audited case.
- `derived` faces untouched in V1: no `DerivedNovelConclusion` reaches the
  surface.

## Contract metadata

- Relates to: ADR-0013–0016 (shadow/audited plans), ADR-0028 (fact model
  authority), ADR-0029 (Haskell gap), ADR-0024–0027 (stance attestation and
  temporal provenance), ADR-0030–0033 (knowledge packs, fact-grounded
  boundaries).
- Supersedes: none; this is the V2 planning boundary ADR adopted on the
  audited V1 renderer as authority.
- Contract version: ResponsePlan V2 boundaries v1.
- Reference vectors: candidate archetypes, PropositionId Merkle vectors,
  `template-agreement-matrix` (7 gender-hardcoded rows), and truncated
  measurement vectors under F0. Reference executable endpoints are release
  binary traces comparing V1 output with V2 realizations via unchanged
  `structural_corpus.rs`, the phase/replay gates, and the cumulative
  `--gate response-plan-v2-canary-report` report.
