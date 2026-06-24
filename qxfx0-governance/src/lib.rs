use qxfx0_types::system_state::GuardStatus;
use serde::{Deserialize, Serialize};

/// Governance event — append-only history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceEvent {
    pub turn: usize,
    pub event_type: GovernanceEventType,
    pub family: qxfx0_types::CanonicalMoveFamily,
    pub guard_status: GuardStatus,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernanceEventType {
    TurnCompleted,
    GuardBlocked,
    GuardWarning,
    CommitmentRevised,
    CommitmentContradicted,
    GraphEnriched { new_relations: usize },
}

/// Governance log — append-only history of governance events.
/// Deterministic: events are stored in order, never modified.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GovernanceLog {
    pub events: Vec<GovernanceEvent>,
}

impl GovernanceLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event (immutable — never modify existing events).
    pub fn append(&mut self, event: GovernanceEvent) {
        self.events.push(event);
    }

    /// Get the last N events.
    pub fn recent(&self, n: usize) -> &[GovernanceEvent] {
        let start = self.events.len().saturating_sub(n);
        &self.events[start..]
    }

    /// Count events by type.
    pub fn count_by_type(&self, event_type: &GovernanceEventType) -> usize {
        self.events
            .iter()
            .filter(|e| std::mem::discriminant(&e.event_type) == std::mem::discriminant(event_type))
            .count()
    }

    /// Total event count.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Check if any turn was blocked by guard.
    pub fn has_blocks(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e.event_type, GovernanceEventType::GuardBlocked))
    }

    /// Replay gate — verify that the event log is consistent.
    /// Returns violations (empty = ok).
    pub fn replay_check(&self) -> Vec<String> {
        let mut violations = Vec::new();

        for (i, event) in self.events.iter().enumerate() {
            // Turns should be monotonically non-decreasing
            if i > 0 && event.turn < self.events[i - 1].turn {
                violations.push(format!(
                    "turn regression at event {}: {} < {}",
                    i,
                    event.turn,
                    self.events[i - 1].turn
                ));
            }

            // GuardBlocked should have InvariantBlock status
            if matches!(event.event_type, GovernanceEventType::GuardBlocked)
                && !matches!(event.guard_status, GuardStatus::InvariantBlock(_))
            {
                violations.push(format!("GuardBlocked event {} has non-block status", i));
            }
        }

        violations
    }
}

/// Authority map — delegation chains with escalation prevention.
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

        // Prevent escalation: if `to` already delegates to `from`, this would create a cycle
        if self
            .delegations
            .iter()
            .any(|d| d.from == to && d.to == from)
        {
            return Err(format!(
                "escalation prevented: {} already delegates to {}",
                to, from
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

    /// Check if a principal has a permission (directly or via delegation chain).
    /// The chain is followed from the principal UPWARDS (from → to means
    /// "from delegates to to", so "to" inherits "from"'s permissions).
    pub fn has_permission(&self, principal: &str, permission: &str) -> bool {
        // Check if anyone delegates TO this principal with this permission
        if self
            .delegations
            .iter()
            .any(|d| d.to == principal && d.permissions.contains(&permission.to_string()))
        {
            return true;
        }

        // Follow the chain: if principal delegates to someone who has the permission
        let mut current = principal;
        for _ in 0..10 {
            let delegate = self.delegations.iter().find(|d| d.from == current);
            match delegate {
                Some(d) if d.permissions.contains(&permission.to_string()) => return true,
                Some(d) => current = &d.to,
                None => break,
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
    fn test_authority_delegate() {
        let mut auth = AuthorityMap::new();
        auth.delegate("system", "render", vec!["generate".into()], 1)
            .unwrap();
        assert!(auth.has_permission("system", "generate"));
        assert!(!auth.has_permission("system", "delete"));
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
        // leaf should have read via chain
        assert!(auth.has_permission("leaf", "read"));
        assert!(!auth.has_permission("leaf", "write"));
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
