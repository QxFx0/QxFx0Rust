pub use qxfx0_types::governance::{GovernanceEvent, GovernanceEventType, GovernanceLog};
pub use qxfx0_types::system_state::GuardStatus;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Authority map — delegation chains with escalation prevention.
///
/// Delegation semantics
/// --------------------
/// A delegation `from -> to` with permissions `[P]` means that `to` is
/// granted permission `P`.  `has_permission(principal, P)` checks whether
/// `principal` receives `P` directly from any delegator, or transitively
/// from an ancestor in the delegation graph (i.e. it follows the chain
/// upward from `principal` to whoever delegated to them, and so on).
///
/// Cycle prevention
/// ----------------
/// `delegate` rejects any edge that would create a cycle of arbitrary
/// length, not just a 2-cycle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthorityMap {
    pub delegations: Vec<DelegationEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationEntry {
    pub from: String,
    pub to: String,
    pub permissions: Vec<String>,
    pub turn: usize,
}

impl AuthorityMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a delegation. Prevents escalation (can't delegate up).
    /// Rejects self-delegation and any cycle of arbitrary length.
    pub fn delegate(
        &mut self,
        from: &str,
        to: &str,
        permissions: Vec<String>,
        turn: usize,
    ) -> Result<(), String> {
        // Prevent self-delegation
        if from == to {
            return Err("self-delegation not allowed".into());
        }

        // Prevent cycles of any length
        if self.would_create_cycle(from, to) {
            return Err(format!(
                "escalation prevented: adding {} -> {} would create a delegation cycle",
                from, to
            ));
        }

        self.delegations.push(DelegationEntry {
            from: from.into(),
            to: to.into(),
            permissions,
            turn,
        });
        Ok(())
    }

    /// Return true if adding `from -> to` would create a cycle.
    fn would_create_cycle(&self, from: &str, to: &str) -> bool {
        let mut stack = vec![to.to_string()];
        let mut visited = BTreeSet::new();

        while let Some(node) = stack.pop() {
            if !visited.insert(node.clone()) {
                continue;
            }
            if node == from {
                return true;
            }
            for d in &self.delegations {
                if d.from == node {
                    stack.push(d.to.clone());
                }
            }
        }
        false
    }

    /// Check if a principal has a permission (directly or via delegation chain).
    ///
    /// Standard delegation semantics: a principal has a permission if any
    /// delegator in their ancestor chain granted it to them.  The search
    /// follows delegations **to** the current principal and then ascends to
    /// the delegator, tracking visited nodes to avoid infinite loops.
    pub fn has_permission(&self, principal: &str, permission: &str) -> bool {
        let mut stack = vec![principal.to_string()];
        let mut visited = BTreeSet::new();

        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }

            for d in &self.delegations {
                if d.to == current {
                    if d.permissions.contains(&permission.to_string()) {
                        return true;
                    }
                    stack.push(d.from.clone());
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::CanonicalMoveFamily;

    fn make_event(turn: usize, etype: GovernanceEventType) -> GovernanceEvent {
        GovernanceEvent {
            turn,
            event_type: etype,
            family: CanonicalMoveFamily::CMDefine,
            guard_status: GuardStatus::InvariantOk,
            timestamp: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn test_append_and_retrieve() {
        let mut log = GovernanceLog::new();
        log.append(make_event(1, GovernanceEventType::TurnCompleted));
        log.append(make_event(2, GovernanceEventType::TurnCompleted));

        assert_eq!(log.len(), 2);
        assert_eq!(log.recent(1).len(), 1);
        assert_eq!(log.recent(1)[0].turn, 2);
    }

    #[test]
    fn test_replay_check_ok() {
        let mut log = GovernanceLog::new();
        log.append(make_event(1, GovernanceEventType::TurnCompleted));
        log.append(make_event(2, GovernanceEventType::TurnCompleted));
        log.append(make_event(3, GovernanceEventType::TurnCompleted));

        assert!(log.replay_check().is_empty());
    }

    #[test]
    fn test_replay_check_turn_regression() {
        let mut log = GovernanceLog::new();
        log.append(make_event(3, GovernanceEventType::TurnCompleted));
        log.append(make_event(2, GovernanceEventType::TurnCompleted)); // regression

        let violations = log.replay_check();
        assert!(!violations.is_empty());
        assert!(violations[0].contains("regression"));
    }

    #[test]
    fn test_replay_check_guard_blocked_status() {
        let mut log = GovernanceLog::new();
        log.append(GovernanceEvent {
            turn: 1,
            event_type: GovernanceEventType::GuardBlocked,
            family: CanonicalMoveFamily::CMDefine,
            guard_status: GuardStatus::InvariantOk, // wrong! should be Block
            timestamp: "2026-01-01T00:00:00Z".into(),
        });

        let violations = log.replay_check();
        assert!(!violations.is_empty());
        assert!(violations[0].contains("non-block status"));
    }

    #[test]
    fn test_has_blocks() {
        let mut log = GovernanceLog::new();
        log.append(make_event(1, GovernanceEventType::TurnCompleted));
        assert!(!log.has_blocks());

        log.append(GovernanceEvent {
            turn: 2,
            event_type: GovernanceEventType::GuardBlocked,
            family: CanonicalMoveFamily::CMRepair,
            guard_status: GuardStatus::InvariantBlock("test".into()),
            timestamp: "2026-01-01T00:00:00Z".into(),
        });
        assert!(log.has_blocks());
    }

    #[test]
    fn test_has_permission_standard_semantics() {
        let mut auth = AuthorityMap::new();
        auth.delegate("system", "render", vec!["generate".into()], 1)
            .unwrap();
        // The delegatee has the permission; the delegator does not.
        assert!(auth.has_permission("render", "generate"));
        assert!(!auth.has_permission("system", "generate"));
        assert!(!auth.has_permission("render", "delete"));
    }

    #[test]
    fn test_authority_escalation_prevention() {
        let mut auth = AuthorityMap::new();
        auth.delegate("a", "b", vec!["read".into()], 1).unwrap();
        // b → a should fail (would create cycle)
        let result = auth.delegate("b", "a", vec!["write".into()], 2);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("escalation"));
    }

    #[test]
    fn test_authority_self_delegation() {
        let mut auth = AuthorityMap::new();
        let result = auth.delegate("x", "x", vec!["read".into()], 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_authority_chain() {
        let mut auth = AuthorityMap::new();
        auth.delegate("root", "mid", vec!["read".into()], 1)
            .unwrap();
        auth.delegate("mid", "leaf", vec!["read".into()], 2)
            .unwrap();
        // leaf should have read via direct delegation from mid
        assert!(auth.has_permission("leaf", "read"));
        assert!(!auth.has_permission("leaf", "write"));
    }

    #[test]
    fn test_governance_3cycle_rejected() {
        let mut auth = AuthorityMap::new();
        auth.delegate("A", "B", vec!["read".into()], 1).unwrap();
        auth.delegate("B", "C", vec!["read".into()], 2).unwrap();
        // C → A would close the cycle A → B → C → A
        let result = auth.delegate("C", "A", vec!["write".into()], 3);
        assert!(result.is_err(), "3-cycle must be rejected");
        assert!(result.unwrap_err().contains("escalation"));
    }

    #[test]
    fn test_count_by_type() {
        let mut log = GovernanceLog::new();
        log.append(make_event(1, GovernanceEventType::TurnCompleted));
        log.append(make_event(2, GovernanceEventType::TurnCompleted));
        log.append(make_event(
            3,
            GovernanceEventType::GraphEnriched { new_relations: 2 },
        ));

        assert_eq!(log.count_by_type(&GovernanceEventType::TurnCompleted), 2);
        assert_eq!(
            log.count_by_type(&GovernanceEventType::GraphEnriched { new_relations: 0 }),
            1
        );
    }
}
