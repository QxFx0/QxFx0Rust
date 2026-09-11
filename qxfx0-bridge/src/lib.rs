//! qxfx0-bridge — the learning bridge (ADR-0043 U4).
//!
//! The Rust twin has no knowledge of the external world beyond its
//! fingerprinted knowledge packs, and that boundary is the product. This
//! crate is the *only* place where experience can flow into the graph, and
//! it is deliberately placed outside the turn: a between-turn worker
//! consumes bounded corroboration events, adjusts runtime edge confidence,
//! decays and retires what nothing corroborates, and quarantines what the
//! human release gate rejects (U5). The Haskell `RuntimeLLMFeedback`
//! module is the spec; the algebra here is its exact port.
//!
//! Three laws, inherited and strengthened (ADR-0043 U4):
//!
//! - **Associative evidence only, never rendered.** A bridge edge carries
//!   no `ru_original` surface and no admissible `Relation`: it can steer
//!   activation weight — nothing more. Rendering reads curated graph
//!   content; the bridge feed is separate, and promotion (U5) is the only
//!   door from one to the other, guarded by the same admission bar an
//!   editor passes and a human release.
//! - **No network in the default build.** The `llm-candidates` feature
//!   (off everywhere CI runs) is the single network surface and only ever
//!   fetches candidate events between turns. This module itself is pure:
//!   input is data, not a socket.
//! - **Zero visible behavior change while the bridge sleeps.** Nothing is
//!   wired into the pipeline; the store, queue and quarantine are inert
//!   until U5 opens the promotion door. The corpus-equality asserts in the
//!   tests are the lock for the day it is.

pub mod candidates;
pub mod corroboration;
pub mod promotion;
pub mod quarantine;
pub mod runtime_edges;
pub mod worker;

pub use candidates::{CandidateError, CandidateSource};
pub use corroboration::{BoundedCorroborationQueue, CorroborationEvent, DEFAULT_QUEUE_CAPACITY};
pub use promotion::{
    builtin_gate_policy, canonical_slug, create_draft, create_draft_with_admission,
    evaluate_candidate_informativeness, normalize_atom, render_overlay_artifact, revalidate,
    rollback, run_corpus_precheck, run_runtime_ab_trial, surface_atom_set,
    validate_promotion_invariants, CorpusTrial, ExclusionReason, GatePolicy, InformativenessResult,
    Overlay, OverlayStatus, PromotedPredicate, PromotionCandidate, PromotionError,
    RevalidatedPredicate, Revalidation, RuntimeAbCase, RuntimeAbTrial, TopicAdmissionFacts,
    TrialTopic, CORPUS_METHOD_STRUCTURAL, EVALUATION_TOPIC_SET, RUNTIME_AB_METHOD,
    SEMANTIC_GAIN_THRESHOLD, STOP_WORDS,
};
pub use quarantine::{
    QuarantineLedger, QuarantineReason, QuarantinedEvent, DEFAULT_QUARANTINE_CAPACITY,
};
pub use runtime_edges::{
    apply_corroboration, apply_corroboration_event, decode_store, edge_by_pair, encode_store,
    promote_if_ready, relation_type_weight, runtime_edge_count, runtime_edges,
    validate_bridge_invariants, BridgeEdge, BridgeEdgeSource, BridgeOutcome, Corroboration,
    DecayConfig, RuntimeEdgeStore, RUNTIME_EDGE_CAP, RUNTIME_PROMOTION_CONFIDENCE,
    RUNTIME_PROMOTION_CO_OCCURRENCE,
};
pub use worker::{admission_reason, process_turn_boundary, WorkerReport};

pub mod import_quarantine;
pub use import_quarantine::{
    candidates_from_import, parse_quarantine_jsonl, ImportPredicate, ImportRecord, ImportRefusal,
};
