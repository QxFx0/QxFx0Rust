//! Typed, owned contexts for each turn-pipeline stage.
//!
//! Constructors are crate-private, so callers cannot manufacture a parsed or
//! already-routed turn. Public accessors expose only immutable stage evidence.

use crate::conversation_fsm::ConversationState;
use qxfx0_self::deliberation::ReconcileRule;
use qxfx0_semantic::{ParsedProposition, PropositionMode};
use qxfx0_types::system_state::GuardStatus;
use qxfx0_types::CanonicalMoveFamily;
use serde::Serialize;

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
pub struct RenderedTurnContext {
    routed: RoutedTurnContext,
    response: String,
    path_depth: usize,
    has_bridge: bool,
}

impl RenderedTurnContext {
    pub(crate) fn new(
        routed: RoutedTurnContext,
        response: String,
        path_depth: usize,
        has_bridge: bool,
    ) -> Self {
        Self {
            routed,
            response,
            path_depth,
            has_bridge,
        }
    }

    pub fn routed(&self) -> &RoutedTurnContext {
        &self.routed
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
}

impl GuardedTurnContext {
    pub(crate) fn new(
        finalized: FinalizedTurnContext,
        family: CanonicalMoveFamily,
        guard_status: GuardStatus,
        blocked: bool,
        rejection: Option<String>,
    ) -> Self {
        Self {
            finalized,
            family,
            guard_status,
            blocked,
            rejection,
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
}

impl StageTraceContext for PreparedTurnContext {}

impl StageTraceContext for RoutedTurnContext {
    fn trace_family(&self) -> Option<CanonicalMoveFamily> {
        Some(self.family())
    }
}

impl StageTraceContext for RenderedTurnContext {
    fn trace_family(&self) -> Option<CanonicalMoveFamily> {
        Some(self.routed().family())
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
}

impl StageTraceContext for PersistedTurnContext {
    fn trace_family(&self) -> Option<CanonicalMoveFamily> {
        Some(self.guarded().family())
    }
}
