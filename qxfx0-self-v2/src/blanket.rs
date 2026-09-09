//! Self-blanket invariants — the structural self-identity check over the
//! blanket snapshot (ADR-0043 U3, ported from Haskell `QxFx0.Self.Invariants`
//! + `QxFx0.Self.Blanket`, ADR-0007/0012 there).
//!
//! The blanket is the Markov boundary of /this system/: a session identity
//! that does not silently change, morphology that does not vanish, turn
//! count and identity claims that do not regress. Violations are not
//! recoverable runtime errors — they mean the system has ceased to be
//! itself (the Haskell `IdentityRupture` class). V2 ports them as
//! fail-closed data: the pipeline feeds the violation list into
//! `compute_conatus_energy` (penalty −λ·|v|) and the essence trajectory
//! witnesses the eroded energy; nothing panics on the turn path.
//!
//! Haskell checks at two transition points; this port matches them:
//! [`check_initial_blanket`] post-bootstrap, [`check_blanket_transition`]
//! between consecutive commits. The session identity lives in the snapshot
//! (`SelfBlanketSnapshot` gained the field in U3; pre-U3 persisted
//! snapshots load it as the empty string and the *transition* check skips
//! the session comparison for them, so an upgrade never fabricates a
//! rupture from history the old schema could not carry).

use serde::{Deserialize, Serialize};

/// The structural self-identity snapshot: the V2 blanket plus the session
/// identity that must stay stable across it. `morphology_total_size`,
/// `identity_claims_count` and `turn_count` carry the persisted U2 shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlanketRecord {
    pub session_id: String,
    pub morphology_total_size: u64,
    pub identity_claims_count: u64,
    pub turn_count: u64,
}

/// One violated structural invariant. `code` is the stable snake_case tag
/// (trace-schema-stable); `detail` carries the human-readable rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralViolation {
    pub code: &'static str,
    pub detail: String,
}

fn violation(code: &'static str, detail: impl Into<String>) -> StructuralViolation {
    StructuralViolation {
        code,
        detail: detail.into(),
    }
}

impl From<StructuralViolation> for crate::conatus::BlanketViolation {
    fn from(violation: StructuralViolation) -> Self {
        crate::conatus::BlanketViolation {
            code: violation.code.to_string(),
            detail: violation.detail,
        }
    }
}

/// Invariants a freshly bootstrapped blanket must satisfy: a non-empty
/// session identity and strictly positive morphology.
pub fn check_initial_blanket(blanket: &BlanketRecord) -> Vec<StructuralViolation> {
    let mut violations = Vec::new();
    if blanket.session_id.is_empty() {
        violations.push(violation(
            "empty_session",
            "self-blanket: session identifier is empty",
        ));
    }
    if blanket.morphology_total_size == 0 {
        violations.push(violation(
            "empty_morphology",
            "self-blanket: morphology dictionaries are empty",
        ));
    }
    violations
}

/// Invariants across a commit-time transition from the previous blanket to
/// the current one, including all initial checks on the current side.
///
/// A previous blanket persisted before U3 carries no session identity
/// (`session_id` empty): the session-comparison leg is then skipped rather
/// than reporting a rupture the old schema could not witness. Turn and
/// identity-claim monotonicity still hold against it.
pub fn check_blanket_transition(
    previous: &BlanketRecord,
    current: &BlanketRecord,
) -> Vec<StructuralViolation> {
    let mut violations = check_initial_blanket(current);
    if !previous.session_id.is_empty() && previous.session_id != current.session_id {
        violations.push(violation(
            "session_changed",
            format!(
                "self-blanket: session identifier changed (was=\"{}\" now=\"{}\")",
                previous.session_id, current.session_id
            ),
        ));
    }
    if current.turn_count < previous.turn_count {
        violations.push(violation(
            "turn_regressed",
            format!(
                "self-blanket: turn count regressed (was={} now={})",
                previous.turn_count, current.turn_count
            ),
        ));
    }
    if current.identity_claims_count < previous.identity_claims_count {
        violations.push(violation(
            "identity_erased",
            format!(
                "self-blanket: identity claim count erased (was={} now={})",
                previous.identity_claims_count, current.identity_claims_count
            ),
        ));
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blanket(session: &str, morphology: u64, claims: u64, turns: u64) -> BlanketRecord {
        BlanketRecord {
            session_id: session.into(),
            morphology_total_size: morphology,
            identity_claims_count: claims,
            turn_count: turns,
        }
    }

    #[test]
    fn healthy_blanket_has_no_violations() {
        assert!(check_initial_blanket(&blanket("s1", 20_000, 3, 7)).is_empty());
        assert!(check_blanket_transition(
            &blanket("s1", 20_000, 3, 7),
            &blanket("s1", 20_001, 4, 8)
        )
        .is_empty());
    }

    #[test]
    fn initial_checks_name_empty_identity_and_vanishing_morphology() {
        let violations = check_initial_blanket(&blanket("", 0, 0, 0));
        let codes: Vec<&str> = violations.iter().map(|v| v.code).collect();
        assert_eq!(codes, vec!["empty_session", "empty_morphology"]);
    }

    #[test]
    fn transition_checks_are_stable_and_catch_both_regressions() {
        let previous = blanket("s1", 100, 5, 10);
        // Same transition repeated: still clean (pure function).
        let next = blanket("s1", 101, 6, 11);
        assert!(check_blanket_transition(&previous, &next).is_empty());
        assert!(check_blanket_transition(&previous, &next).is_empty());
        let regressed = check_blanket_transition(&previous, &blanket("s1", 101, 2, 9));
        let codes: Vec<&str> = regressed.iter().map(|v| v.code).collect();
        assert_eq!(codes, vec!["turn_regressed", "identity_erased"]);
    }

    #[test]
    fn session_change_is_a_violation_but_legacy_history_is_exonerated() {
        let crossed =
            check_blanket_transition(&blanket("s1", 100, 0, 10), &blanket("s2", 100, 0, 11));
        assert_eq!(
            crossed.iter().map(|v| v.code).collect::<Vec<_>>(),
            vec!["session_changed"]
        );
        // A pre-U3 persisted previous blanket: no session leg, monotonicity holds.
        assert!(
            check_blanket_transition(&blanket("", 100, 0, 10), &blanket("s2", 100, 0, 11))
                .is_empty()
        );
    }
}
