//! Typed, owned contexts for each turn-pipeline stage.
//!
//! Constructors are crate-private, so callers cannot manufacture a parsed or
//! already-routed turn. Public accessors expose only immutable stage evidence.

use crate::conversation_fsm::ConversationState;
use crate::shadow_plan::ShadowPlanOutcome;
use crate::RendererAuthority;
use qxfx0_self::deliberation::ReconcileRule;
use qxfx0_semantic::{ParsedProposition, PlanOutcome, PropositionMode, RecoveryTrace};
use qxfx0_types::system_state::GuardStatus;
use qxfx0_types::CanonicalMoveFamily;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
pub struct TurnInputContext {
    session_id: String,
    raw_text: String,
    proposition: ParsedProposition,
    is_challenge: bool,
}

impl TurnInputContext {
    pub(crate) fn new(
        session_id: String,
        raw_text: String,
        proposition: ParsedProposition,
        is_challenge: bool,
    ) -> Self {
        Self {
            session_id,
            raw_text,
            proposition,
            is_challenge,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn raw_text(&self) -> &str {
        &self.raw_text
    }

    pub fn proposition(&self) -> &ParsedProposition {
        &self.proposition
    }

    pub fn subject(&self) -> &str {
        &self.proposition.subject
    }

    pub fn mode(&self) -> PropositionMode {
        self.proposition.mode
    }

    pub fn is_challenge(&self) -> bool {
        self.is_challenge
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PreparedTurnContext {
    input: TurnInputContext,
    conatus_energy: f64,
    salience: f64,
    holistic_dominant: bool,
    essence_strength: f64,
    deliberation_family: CanonicalMoveFamily,
    deliberation_rule: ReconcileRule,
    has_enough: bool,
}

impl PreparedTurnContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        input: TurnInputContext,
        conatus_energy: f64,
        salience: f64,
        holistic_dominant: bool,
        essence_strength: f64,
        deliberation_family: CanonicalMoveFamily,
        deliberation_rule: ReconcileRule,
        has_enough: bool,
    ) -> Self {
        Self {
            input,
            conatus_energy,
            salience,
            holistic_dominant,
            essence_strength,
            deliberation_family,
            deliberation_rule,
            has_enough,
        }
    }

    pub fn input(&self) -> &TurnInputContext {
        &self.input
    }

    pub fn conatus_energy(&self) -> f64 {
        self.conatus_energy
    }

    pub fn salience(&self) -> f64 {
        self.salience
    }

    pub fn holistic_dominant(&self) -> bool {
        self.holistic_dominant
    }

    pub fn essence_strength(&self) -> f64 {
        self.essence_strength
    }

    pub fn deliberation_family(&self) -> CanonicalMoveFamily {
        self.deliberation_family
    }

    pub fn deliberation_rule(&self) -> ReconcileRule {
        self.deliberation_rule
    }

    pub fn has_enough(&self) -> bool {
        self.has_enough
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutedTurnContext {
    prepared: PreparedTurnContext,
    family: CanonicalMoveFamily,
    conversation_state: ConversationState,
}

impl RoutedTurnContext {
    pub(crate) fn new(
        prepared: PreparedTurnContext,
        family: CanonicalMoveFamily,
        conversation_state: ConversationState,
    ) -> Self {
        Self {
            prepared,
            family,
            conversation_state,
        }
    }

    pub fn prepared(&self) -> &PreparedTurnContext {
        &self.prepared
    }

    pub fn family(&self) -> CanonicalMoveFamily {
        self.family
    }

    pub fn conversation_state(&self) -> ConversationState {
        self.conversation_state
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedTurnContext {
    routed: RoutedTurnContext,
    shadow_plan: ShadowPlanOutcome,
}

impl PlannedTurnContext {
    pub(crate) fn new(routed: RoutedTurnContext, shadow_plan: ShadowPlanOutcome) -> Self {
        Self {
            routed,
            shadow_plan,
        }
    }

    pub fn routed(&self) -> &RoutedTurnContext {
        &self.routed
    }

    pub fn shadow_plan(&self) -> &ShadowPlanOutcome {
        &self.shadow_plan
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderedTurnContext {
    planned: PlannedTurnContext,
    response: String,
    path_depth: usize,
    has_bridge: bool,
    renderer_authority: RendererAuthority,
    renderer_source: RendererSource,
    plan_surface_available: bool,
    plan_surface_matches_output: Option<bool>,
    plan_render_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RenderEvidence {
    pub(crate) renderer_authority: RendererAuthority,
    pub(crate) renderer_source: RendererSource,
    pub(crate) plan_surface_available: bool,
    pub(crate) plan_surface_matches_output: Option<bool>,
    pub(crate) plan_render_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum RendererSource {
    LegacyGraph,
    AuditedPlan,
    LegacyFallback,
}

impl RendererSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyGraph => "legacy_graph",
            Self::AuditedPlan => "audited_plan",
            Self::LegacyFallback => "legacy_fallback",
        }
    }
}

impl RenderedTurnContext {
    pub(crate) fn new(
        planned: PlannedTurnContext,
        response: String,
        path_depth: usize,
        has_bridge: bool,
        evidence: RenderEvidence,
    ) -> Self {
        Self {
            planned,
            response,
            path_depth,
            has_bridge,
            renderer_authority: evidence.renderer_authority,
            renderer_source: evidence.renderer_source,
            plan_surface_available: evidence.plan_surface_available,
            plan_surface_matches_output: evidence.plan_surface_matches_output,
            plan_render_error: evidence.plan_render_error,
        }
    }

    pub fn planned(&self) -> &PlannedTurnContext {
        &self.planned
    }

    pub fn routed(&self) -> &RoutedTurnContext {
        self.planned.routed()
    }

    pub fn response(&self) -> &str {
        &self.response
    }

    pub fn path_depth(&self) -> usize {
        self.path_depth
    }

    pub fn has_bridge(&self) -> bool {
        self.has_bridge
    }

    pub fn renderer_authority(&self) -> RendererAuthority {
        self.renderer_authority
    }

    pub fn renderer_source(&self) -> RendererSource {
        self.renderer_source
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FinalizedTurnContext {
    rendered: RenderedTurnContext,
}

impl FinalizedTurnContext {
    pub(crate) fn new(rendered: RenderedTurnContext) -> Self {
        Self { rendered }
    }

    pub fn rendered(&self) -> &RenderedTurnContext {
        &self.rendered
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GuardedTurnContext {
    finalized: FinalizedTurnContext,
    family: CanonicalMoveFamily,
    guard_status: GuardStatus,
    blocked: bool,
    rejection: Option<String>,
    recovery: Option<RecoveryTrace>,
}

impl GuardedTurnContext {
    pub(crate) fn new(
        finalized: FinalizedTurnContext,
        family: CanonicalMoveFamily,
        guard_status: GuardStatus,
        blocked: bool,
        rejection: Option<String>,
        recovery: Option<RecoveryTrace>,
    ) -> Self {
        Self {
            finalized,
            family,
            guard_status,
            blocked,
            rejection,
            recovery,
        }
    }

    pub fn finalized(&self) -> &FinalizedTurnContext {
        &self.finalized
    }

    pub fn family(&self) -> CanonicalMoveFamily {
        self.family
    }

    pub fn guard_status(&self) -> &GuardStatus {
        &self.guard_status
    }

    pub fn blocked(&self) -> bool {
        self.blocked
    }

    pub fn rejection(&self) -> Option<&str> {
        self.rejection.as_deref()
    }

    pub fn recovery(&self) -> Option<&RecoveryTrace> {
        self.recovery.as_ref()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PersistedTurnContext {
    guarded: GuardedTurnContext,
}

impl PersistedTurnContext {
    pub(crate) fn new(guarded: GuardedTurnContext) -> Self {
        Self { guarded }
    }

    pub fn guarded(&self) -> &GuardedTurnContext {
        &self.guarded
    }
}

pub(crate) trait StageTraceContext {
    fn trace_family(&self) -> Option<CanonicalMoveFamily> {
        None
    }

    fn trace_status(&self) -> &'static str {
        "ok"
    }

    fn trace_metadata(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
}

impl StageTraceContext for PreparedTurnContext {}

impl StageTraceContext for RoutedTurnContext {
    fn trace_family(&self) -> Option<CanonicalMoveFamily> {
        Some(self.family())
    }
}

impl StageTraceContext for PlannedTurnContext {
    fn trace_family(&self) -> Option<CanonicalMoveFamily> {
        Some(self.routed().family())
    }

    fn trace_metadata(&self) -> BTreeMap<String, String> {
        let mut metadata = BTreeMap::from([(
            "plan_outcome".into(),
            self.shadow_plan().kind().as_str().into(),
        )]);
        match self.shadow_plan() {
            PlanOutcome::Ready(plan) => {
                metadata.insert("plan_version".into(), plan.version().as_str().into());
                metadata.insert("response_goal".into(), plan.goal().as_str().into());
                metadata.insert("subject_kind".into(), plan.subject().kind().into());
                if let qxfx0_semantic::PlanSubject::Topic(topic) = plan.subject() {
                    metadata.insert("plan_topic".into(), topic.as_str().into());
                }
                metadata.insert("plan_claim_count".into(), plan.claims().len().to_string());
                metadata.insert(
                    "claim_roles".into(),
                    plan.claims()
                        .iter()
                        .map(|claim| claim.role().as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                metadata.insert(
                    "claim_ids".into(),
                    plan.claims()
                        .iter()
                        .map(|claim| claim.id().as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                metadata.insert(
                    "plan_derivation_count".into(),
                    plan.derivation().len().to_string(),
                );
                metadata.insert(
                    "derivation_rules".into(),
                    plan.derivation()
                        .iter()
                        .map(|step| step.rule().as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                metadata.insert(
                    "discourse_relation".into(),
                    plan.discourse().relation().as_str().into(),
                );
                metadata.insert(
                    "sentence_budget".into(),
                    plan.discourse().sentence_budget().get().to_string(),
                );
                if let Some(obligation) = plan.obligation() {
                    metadata.insert("dialogue_obligation".into(), obligation.as_str().into());
                }
                if let Some((subject, relation, object)) = plan
                    .claims()
                    .iter()
                    .find_map(|claim| claim.proposition().canonical_slots())
                {
                    metadata.insert(
                        "canonical_slots".into(),
                        [subject.as_str(), relation.as_str(), object.as_str()].join(","),
                    );
                }
                let predicate_refs = plan
                    .claims()
                    .iter()
                    .flat_map(|claim| claim.predicate_refs().iter())
                    .map(qxfx0_semantic::PredicateRef::as_str)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(",");
                metadata.insert("predicate_refs".into(), predicate_refs);
                let provenances = plan
                    .claims()
                    .iter()
                    .map(|claim| claim.evidence().provenance().as_str())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(",");
                let argued = plan.claims().iter().any(|claim| {
                    claim.evidence().provenance()
                        == qxfx0_semantic::EvidenceProvenance::CuratedReleaseCorpus
                });
                metadata.insert("claim_provenance".into(), provenances);
                metadata.insert("argued_topic_admitted".into(), argued.to_string());
                if argued {
                    metadata.insert(
                        "content_profile".into(),
                        qxfx0_semantic::CONTENT_PROFILE.into(),
                    );
                }
            }
            PlanOutcome::Fallback(plan) => {
                metadata.insert("plan_version".into(), plan.version().as_str().into());
                metadata.insert("response_goal".into(), plan.goal().as_str().into());
                append_recovery_metadata(&mut metadata, plan.recovery());
                metadata.insert(
                    "subject_kind".into(),
                    plan.subject()
                        .map_or("none", qxfx0_semantic::FallbackSubject::kind)
                        .into(),
                );
                match plan.subject() {
                    Some(qxfx0_semantic::FallbackSubject::KnownTopic(topic)) => {
                        metadata.insert("plan_topic".into(), topic.as_str().into());
                    }
                    Some(qxfx0_semantic::FallbackSubject::UnresolvedTopic(topic)) => {
                        metadata.insert("plan_topic".into(), topic.clone());
                    }
                    Some(qxfx0_semantic::FallbackSubject::Dialogue(_)) | None => {}
                }
            }
        }
        metadata
    }
}

impl StageTraceContext for RenderedTurnContext {
    fn trace_family(&self) -> Option<CanonicalMoveFamily> {
        Some(self.routed().family())
    }

    fn trace_metadata(&self) -> BTreeMap<String, String> {
        let mut metadata = BTreeMap::from([
            (
                "renderer_authority".into(),
                self.renderer_authority().as_str().into(),
            ),
            (
                "renderer_source".into(),
                self.renderer_source().as_str().into(),
            ),
            (
                "plan_surface_available".into(),
                self.plan_surface_available.to_string(),
            ),
        ]);
        if let Some(matches_output) = self.plan_surface_matches_output {
            metadata.insert(
                "plan_surface_matches_output".into(),
                matches_output.to_string(),
            );
        }
        if let Some(error) = &self.plan_render_error {
            metadata.insert("plan_render_error".into(), error.clone());
        }
        metadata
    }
}

impl StageTraceContext for FinalizedTurnContext {
    fn trace_family(&self) -> Option<CanonicalMoveFamily> {
        Some(self.rendered().routed().family())
    }
}

impl StageTraceContext for GuardedTurnContext {
    fn trace_family(&self) -> Option<CanonicalMoveFamily> {
        Some(self.family())
    }

    fn trace_status(&self) -> &'static str {
        if self.blocked() {
            "error"
        } else {
            "ok"
        }
    }

    fn trace_metadata(&self) -> BTreeMap<String, String> {
        self.recovery().map_or_else(BTreeMap::new, |recovery| {
            let mut metadata = BTreeMap::new();
            append_recovery_metadata(&mut metadata, recovery);
            metadata
        })
    }
}

impl StageTraceContext for PersistedTurnContext {
    fn trace_family(&self) -> Option<CanonicalMoveFamily> {
        Some(self.guarded().family())
    }
}

fn append_recovery_metadata(metadata: &mut BTreeMap<String, String>, recovery: &RecoveryTrace) {
    let cause = recovery.cause().as_str();
    metadata.insert("fallback_reason".into(), cause.into());
    metadata.insert("recovery_cause".into(), cause.into());
    metadata.insert("recovery_policy".into(), recovery.policy().as_str().into());
    metadata.insert(
        "recovery_strategy".into(),
        recovery.strategy().as_str().into(),
    );
    metadata.insert(
        "recovery_evidence_count".into(),
        recovery.evidence().len().to_string(),
    );
    metadata.insert(
        "recovery_evidence".into(),
        serde_json::to_string(recovery.evidence())
            .unwrap_or_else(|error| format!("serialization_error:{error}")),
    );
}
