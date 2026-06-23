use qxfx0_types::system_state::*;
use std::collections::BTreeSet;

/// Commitment store operations — commit, revise, retract, contradict.
/// All operations are pure (return new store, don't mutate).
pub struct CommitmentOps;

impl CommitmentOps {
    /// Create a new commitment from a parsed observation.
    /// No-op if the id already exists in either active or quarantine.
    pub fn commit(
        payload: FactualClaimPayload,
        store: &SemanticCommitmentStore,
    ) -> (SemanticCommitmentStore, CommitmentId) {
        let cid = CommitmentId(store.next_id);
        let mut new_store = store.clone();
        new_store.next_id = store.next_id + 1;

        if new_store.active.contains_key(&cid) || new_store.quarantine.contains_key(&cid) {
            return (new_store, cid);
        }

        new_store.active.insert(cid.clone(), (payload, 0));
        new_store
            .lineage
            .insert(cid.clone(), vec![LineageEvent::Committed { turn: 0 }]);
        (new_store, cid)
    }

    /// Commit an observation with turn sequence.
    pub fn commit_observation(
        payload: FactualClaimPayload,
        store: &SemanticCommitmentStore,
    ) -> (SemanticCommitmentStore, CommitmentId) {
        let cid = CommitmentId(store.next_id);
        let turn = payload.turn_seq;
        let mut new_store = store.clone();
        new_store.next_id = store.next_id + 1;

        new_store.active.insert(cid.clone(), (payload, turn));
        new_store
            .lineage
            .insert(cid.clone(), vec![LineageEvent::Committed { turn }]);
        (new_store, cid)
    }

    /// Quarantine an observation (suppressed claim).
    pub fn quarantine_observation(
        payload: FactualClaimPayload,
        store: &SemanticCommitmentStore,
    ) -> (SemanticCommitmentStore, CommitmentId) {
        let cid = CommitmentId(store.next_id);
        let turn = payload.turn_seq;
        let mut new_store = store.clone();
        new_store.next_id = store.next_id + 1;

        new_store.quarantine.insert(cid.clone(), (payload, turn));
        (new_store, cid)
    }

    /// Revise a commitment — replace payload, record lineage.
    pub fn revise(
        cid: &CommitmentId,
        new_payload: FactualClaimPayload,
        turn: usize,
        store: &SemanticCommitmentStore,
    ) -> SemanticCommitmentStore {
        let mut new_store = store.clone();

        if let Some((_, _)) = new_store.active.get(cid) {
            new_store
                .active
                .insert(cid.clone(), (new_payload.clone(), turn));
            let lineage = new_store.lineage.entry(cid.clone()).or_default();
            lineage.push(LineageEvent::Revised { turn });
        }

        new_store
    }

    /// Record a contradiction between two commitments.
    pub fn contradict(
        left: &CommitmentId,
        right: &CommitmentId,
        kind: ContradictionKind,
        turn: usize,
        store: &SemanticCommitmentStore,
    ) -> SemanticCommitmentStore {
        let mut new_store = store.clone();
        new_store.contradictions.push(ContradictionEvent {
            left: left.clone(),
            right: right.clone(),
            kind,
            turn,
        });
        new_store
    }

    /// Retrieve active commitments matching a query (word-set overlap).
    /// Returns up to 5 matches.
    pub fn retrieve(query: &str, store: &SemanticCommitmentStore) -> Vec<FactualClaimPayload> {
        let query_words: BTreeSet<&str> =
            query.split_whitespace().filter(|w| w.len() >= 3).collect();

        let mut matches: Vec<(usize, FactualClaimPayload)> = store
            .active
            .values()
            .map(|(payload, _)| {
                let stmt_words: BTreeSet<&str> = payload
                    .statement
                    .split_whitespace()
                    .filter(|w| w.len() >= 3)
                    .collect();
                let overlap = query_words.intersection(&stmt_words).count();
                (overlap, payload.clone())
            })
            .filter(|(overlap, _)| *overlap > 0)
            .collect();

        // Sort by overlap descending (deterministic via BTreeSet)
        matches.sort_by_key(|b| std::cmp::Reverse(b.0));

        matches.into_iter().take(5).map(|(_, p)| p).collect()
    }

    /// Detect whether the current turn engages or contradicts held commitments.
    pub fn detect_engagement(
        store: &SemanticCommitmentStore,
        input_topic: &str,
    ) -> CommitmentEngagement {
        let engaged = Self::retrieve(input_topic, store);

        if engaged.is_empty() {
            return CommitmentEngagement {
                engaged_ids: Vec::new(),
                contradicted: false,
                match_kind: MatchKind::NoMatch,
            };
        }

        // Find which commitment IDs are engaged
        let query_words: BTreeSet<&str> = input_topic
            .split_whitespace()
            .filter(|w| w.len() >= 3)
            .collect();

        let engaged_ids: Vec<CommitmentId> = store
            .active
            .iter()
            .filter(|(_, (payload, _))| {
                let stmt_words: BTreeSet<&str> = payload
                    .statement
                    .split_whitespace()
                    .filter(|w| w.len() >= 3)
                    .collect();
                !query_words
                    .intersection(&stmt_words)
                    .collect::<Vec<_>>()
                    .is_empty()
            })
            .map(|(cid, _)| cid.clone())
            .collect();

        // Check for contradiction signals in input
        let contradicted = input_topic.contains("не ")
            || input_topic.contains("противореч")
            || input_topic.contains("ошиба")
            || input_topic.contains("не верно");

        let match_kind = if contradicted {
            MatchKind::ContradictedStrong
        } else {
            MatchKind::EngagedOnly
        };

        CommitmentEngagement {
            engaged_ids,
            contradicted,
            match_kind,
        }
    }

    /// Promote matching quarantined commitments to active.
    pub fn promote_matching_quarantine(
        store: &SemanticCommitmentStore,
        topic: &str,
        turn: usize,
    ) -> SemanticCommitmentStore {
        let mut new_store = store.clone();

        let to_promote: Vec<CommitmentId> = new_store
            .quarantine
            .iter()
            .filter(|(_, (payload, _))| payload.topic == topic)
            .map(|(cid, _)| cid.clone())
            .collect();

        for cid in to_promote {
            if let Some((payload, _)) = new_store.quarantine.remove(&cid) {
                new_store.active.insert(cid.clone(), (payload, turn));
                let lineage = new_store.lineage.entry(cid).or_default();
                lineage.push(LineageEvent::Promoted { turn });
            }
        }

        new_store
    }
}

/// Engagement result — whether the turn engages or contradicts held commitments.
#[derive(Debug, Clone)]
pub struct CommitmentEngagement {
    pub engaged_ids: Vec<CommitmentId>,
    pub contradicted: bool,
    pub match_kind: MatchKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    NoMatch,
    EngagedOnly,
    ContradictedStrong,
    ContradictedWeak,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_payload(topic: &str, stmt: &str) -> FactualClaimPayload {
        FactualClaimPayload {
            statement: stmt.to_string(),
            confidence: 0.5,
            origin: CommitmentOrigin::OriginParser("test".into()),
            turn_seq: 1,
            deps: Vec::new(),
            topic: topic.to_string(),
        }
    }

    #[test]
    fn test_commit_creates_active() {
        let store = SemanticCommitmentStore::default();
        let payload = make_payload("свобода", "свобода предполагает выбор");
        let (new_store, cid) = CommitmentOps::commit(payload, &store);

        assert!(new_store.active.contains_key(&cid));
        assert_eq!(new_store.next_id, 1);
        assert_eq!(new_store.lineage.get(&cid).unwrap().len(), 1);
    }

    #[test]
    fn test_retrieve_finds_matches() {
        let store = SemanticCommitmentStore::default();
        let payload = make_payload("свобода", "свобода предполагает возможность выбора");
        let (store, _) = CommitmentOps::commit(payload, &store);

        let results = CommitmentOps::retrieve("свобода", &store);
        assert!(!results.is_empty());
        assert!(results[0].statement.contains("свобода"));
    }

    #[test]
    fn test_retrieve_no_match() {
        let store = SemanticCommitmentStore::default();
        let results = CommitmentOps::retrieve("квадратный корень", &store);
        assert!(results.is_empty());
    }

    #[test]
    fn test_detect_engagement_no_match() {
        let store = SemanticCommitmentStore::default();
        let eng = CommitmentOps::detect_engagement(&store, "неизвестный topic");
        assert_eq!(eng.match_kind, MatchKind::NoMatch);
    }

    #[test]
    fn test_detect_engagement_match() {
        let store = SemanticCommitmentStore::default();
        let payload = make_payload("свобода", "свобода предполагает выбор");
        let (store, _) = CommitmentOps::commit(payload, &store);

        let eng = CommitmentOps::detect_engagement(&store, "свобода");
        assert_ne!(eng.match_kind, MatchKind::NoMatch);
        assert!(!eng.engaged_ids.is_empty());
    }

    #[test]
    fn test_revise_updates_payload() {
        let store = SemanticCommitmentStore::default();
        let payload = make_payload("истина", "истина — это соответствие");
        let (store, cid) = CommitmentOps::commit(payload, &store);

        let new_payload = make_payload("истина", "истина — это воспроизводимость");
        let store = CommitmentOps::revise(&cid, new_payload, 2, &store);

        let updated = store.active.get(&cid).unwrap();
        assert!(updated.0.statement.contains("воспроизводимость"));
        assert_eq!(store.lineage.get(&cid).unwrap().len(), 2);
    }

    #[test]
    fn test_contradict_records_event() {
        let store = SemanticCommitmentStore::default();
        let (store, left) = CommitmentOps::commit(make_payload("a", "a is x"), &store);
        let (store, right) = CommitmentOps::commit(make_payload("a", "a is not x"), &store);

        let store = CommitmentOps::contradict(
            &left,
            &right,
            ContradictionKind::ContradictionStatement,
            2,
            &store,
        );

        assert_eq!(store.contradictions.len(), 1);
    }

    #[test]
    fn test_deterministic_iteration() {
        // BTreeMap should iterate in same order every time
        let mut store = SemanticCommitmentStore::default();
        for i in 0..10 {
            let payload = make_payload(&format!("topic{}", i), &format!("statement {}", i));
            let (s, _) = CommitmentOps::commit(payload, &store);
            store = s;
        }

        let ids1: Vec<_> = store.active.keys().collect();
        let ids2: Vec<_> = store.active.keys().collect();
        assert_eq!(ids1, ids2, "BTreeMap iteration should be deterministic");
    }
}
