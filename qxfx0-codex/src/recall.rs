//! Recall scoring (Memory program, M1): which held positions does a
//! topic call back, and in what order?
//!
//! Pure function over persisted state — no I/O, no clock, no network.
//! Score per candidate (same-topic commitments only; cross-topic recall
//! is deferred, not faked):
//!
//! ```text
//! score = status_factor * recency + contradiction_bonus
//! recency = 1 / (1 + (current_turn - effective_turn))
//! effective_turn = max(commit turn, topic's last journal mention)
//! status_factor = 1.0 active / 0.5 quarantined (stale is recallable
//!   at half weight — the point of memory is that the past counts
//!   even after it stopped being held)
//! contradiction_bonus = +0.25 when the id appears in any
//!   contradiction event (contested positions surface first)
//! ```
//!
//! Total deterministic order: score desc, effective turn desc, id asc.
//! The caller takes the top N. Journal linkage is read-only: the last
//! mention refreshes recency, never rewrites history.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use qxfx0_types::system_state::{CommitmentId, JournalRecord, SemanticCommitmentStore};

/// Live or stale standing of a recalled position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecallStatus {
    Active,
    Quarantined,
}

/// One ranked recall candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallCandidate {
    pub id: CommitmentId,
    pub statement: String,
    pub topic: String,
    pub turn: usize,
    pub status: RecallStatus,
    pub contradicted: bool,
    pub score: f64,
}

/// Rank the session's positions for `topic` at `current_turn`.
/// Empty topic or empty store reads as empty — never an error.
pub fn recall_candidates(
    store: &SemanticCommitmentStore,
    journal: &[JournalRecord],
    topic: &str,
    current_turn: usize,
    top_n: usize,
) -> Vec<RecallCandidate> {
    if topic.is_empty() || top_n == 0 {
        return Vec::new();
    }
    let last_mention = journal
        .iter()
        .filter(|record| record.topic.as_deref() == Some(topic))
        .map(|record| record.turn)
        .max()
        .unwrap_or(0);
    let contested: BTreeSet<&CommitmentId> = store
        .contradictions
        .iter()
        .flat_map(|event| [&event.left, &event.right])
        .collect();
    let mut candidates = Vec::new();
    for (entries, status, factor) in [
        (&store.active, RecallStatus::Active, 1.0),
        (&store.quarantine, RecallStatus::Quarantined, 0.5),
    ] {
        for (id, (payload, turn)) in entries {
            if payload.topic != topic {
                continue;
            }
            let effective = (*turn).max(last_mention).max(1);
            let age = current_turn.saturating_sub(effective) as f64;
            let contradicted = contested.contains(id);
            let score = factor * (1.0 / (1.0 + age)) + if contradicted { 0.25 } else { 0.0 };
            candidates.push(RecallCandidate {
                id: id.clone(),
                statement: payload.statement.clone(),
                topic: payload.topic.clone(),
                turn: effective,
                status,
                contradicted,
                score,
            });
        }
    }
    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.turn.cmp(&left.turn))
            .then_with(|| left.id.cmp(&right.id))
    });
    candidates.truncate(top_n);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::system_state::{
        CommitmentOrigin, ContradictionEvent, ContradictionKind, FactualClaimPayload,
    };

    fn payload(topic: &str, statement: &str) -> FactualClaimPayload {
        FactualClaimPayload {
            statement: statement.to_string(),
            confidence: 0.9,
            origin: CommitmentOrigin::OriginManual,
            turn_seq: 1,
            deps: Vec::new(),
            topic: topic.to_string(),
        }
    }

    fn record(turn: usize, topic: &str) -> JournalRecord {
        JournalRecord {
            turn,
            day: 20_000,
            topic: Some(topic.to_string()),
            input: String::new(),
            response: String::new(),
            state_digest: String::new(),
            subject_authority: qxfx0_types::system_state::default_subject_authority(),
        }
    }

    fn fixture() -> (SemanticCommitmentStore, Vec<JournalRecord>) {
        let mut store = SemanticCommitmentStore::default();
        store
            .active
            .insert(CommitmentId(0), (payload("свобода", "позиция держится"), 3));
        store
            .active
            .insert(CommitmentId(1), (payload("свобода", "позиция свежая"), 15));
        store
            .active
            .insert(CommitmentId(2), (payload("память", "позиция о памяти"), 10));
        store
            .quarantine
            .insert(CommitmentId(3), (payload("свобода", "позиция старая"), 2));
        store.next_id = 4;
        store.contradictions.push(ContradictionEvent {
            left: CommitmentId(0),
            right: CommitmentId(1),
            kind: ContradictionKind::ContradictionStatement,
            turn: 16,
        });
        let journal = vec![
            record(5, "свобода"),
            record(12, "свобода"),
            record(8, "память"),
        ];
        (store, journal)
    }

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() <= 1e-12 * left.abs().max(right.abs()).max(1.0)
    }

    #[test]
    fn ranking_orders_by_score_then_turn_then_id() {
        let (store, journal) = fixture();
        let ranked = recall_candidates(&store, &journal, "свобода", 20, 10);
        assert_eq!(ranked.len(), 3);
        // id1: effective max(15,12)=15, 1/6 + 0.25; id0: max(3,12)=12,
        // 1/9 + 0.25; id3 quarantined: max(2,12)=12, 0.5 * 1/9.
        assert_eq!(ranked[0].id, CommitmentId(1));
        assert!(close(ranked[0].score, 1.0 / 6.0 + 0.25));
        assert_eq!(ranked[1].id, CommitmentId(0));
        assert!(close(ranked[1].score, 1.0 / 9.0 + 0.25));
        assert_eq!(ranked[2].id, CommitmentId(3));
        assert_eq!(ranked[2].status, RecallStatus::Quarantined);
        assert!(close(ranked[2].score, 0.5 / 9.0));
        assert!(ranked[0].contradicted && ranked[1].contradicted);
        assert!(!ranked[2].contradicted);
    }

    #[test]
    fn topic_scoping_journal_boost_and_limits() {
        let (store, journal) = fixture();
        // память: effective max(10,8)=10 → 1/11, uncontested.
        let memory = recall_candidates(&store, &journal, "память", 20, 10);
        assert_eq!(memory.len(), 1);
        assert!(close(memory[0].score, 1.0 / 11.0));
        // Unknown topic: empty, never an error.
        assert!(recall_candidates(&store, &journal, "время", 20, 10).is_empty());
        assert!(recall_candidates(&store, &journal, "", 20, 10).is_empty());
        assert!(recall_candidates(&store, &journal, "свобода", 20, 0).is_empty());
        // top_n truncates after ranking.
        let top = recall_candidates(&store, &journal, "свобода", 20, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[1].id, CommitmentId(0));
    }

    #[test]
    fn scoring_is_deterministic() {
        let (store, journal) = fixture();
        let first = recall_candidates(&store, &journal, "свобода", 20, 10);
        let second = recall_candidates(&store, &journal, "свобода", 20, 10);
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
    }
}
