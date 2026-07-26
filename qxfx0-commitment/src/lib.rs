use qxfx0_types::system_state::*;
use std::collections::BTreeSet;

/// Result of a commit operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitResult {
    /// A new commitment was created.
    New(CommitmentId),
    /// The id already existed; no insertion occurred.
    Duplicate(CommitmentId),
    /// The bounded store is full; state is returned unchanged.
    CapacityReached,
}

/// Persistent commitments are bounded so a long-running or adversarial
/// session cannot grow state indefinitely.
pub const MAX_COMMITMENTS: usize = 1_024;

/// Commitment store operations — commit, revise, retract, contradict.
/// All operations are pure (return new store, don't mutate).
pub struct CommitmentOps;

impl CommitmentOps {
    /// Create a new commitment from a parsed observation.
    /// Returns `CommitResult::Duplicate` if the id already exists in either active or quarantine
    /// and leaves `next_id` unchanged.
    pub fn commit(
        payload: FactualClaimPayload,
        store: &SemanticCommitmentStore,
    ) -> (SemanticCommitmentStore, CommitResult) {
        if let Some(existing) = Self::find_duplicate(&payload, store) {
            return (store.clone(), CommitResult::Duplicate(existing));
        }
        if store.active.len() + store.quarantine.len() >= MAX_COMMITMENTS {
            return (store.clone(), CommitResult::CapacityReached);
        }
        let cid = CommitmentId(store.next_id);

        if store.active.contains_key(&cid) || store.quarantine.contains_key(&cid) {
            return (store.clone(), CommitResult::Duplicate(cid));
        }

        let mut new_store = store.clone();
        new_store.next_id = store.next_id + 1;
        new_store.active.insert(cid.clone(), (payload, 0));
        new_store
            .lineage
            .insert(cid.clone(), vec![LineageEvent::Committed { turn: 0 }]);
        (new_store, CommitResult::New(cid))
    }

    /// Commit an observation with turn sequence.
    /// Returns `CommitResult::Duplicate` if the id already exists in either active or quarantine
    /// and leaves `next_id` unchanged.
    pub fn commit_observation(
        payload: FactualClaimPayload,
        store: &SemanticCommitmentStore,
    ) -> (SemanticCommitmentStore, CommitResult) {
        if let Some(existing) = Self::find_duplicate(&payload, store) {
            return (store.clone(), CommitResult::Duplicate(existing));
        }
        if store.active.len() + store.quarantine.len() >= MAX_COMMITMENTS {
            return (store.clone(), CommitResult::CapacityReached);
        }
        let cid = CommitmentId(store.next_id);
        let turn = payload.turn_seq;

        if store.active.contains_key(&cid) || store.quarantine.contains_key(&cid) {
            return (store.clone(), CommitResult::Duplicate(cid));
        }

        let mut new_store = store.clone();
        new_store.next_id = store.next_id + 1;
        new_store.active.insert(cid.clone(), (payload, turn));
        new_store
            .lineage
            .insert(cid.clone(), vec![LineageEvent::Committed { turn }]);
        (new_store, CommitResult::New(cid))
    }

    fn find_duplicate(
        payload: &FactualClaimPayload,
        store: &SemanticCommitmentStore,
    ) -> Option<CommitmentId> {
        let normalized_statement = payload
            .statement
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        store
            .active
            .iter()
            .chain(store.quarantine.iter())
            .find_map(|(id, (existing, _))| {
                let same_statement = existing
                    .statement
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase()
                    == normalized_statement;
                (existing.topic.to_lowercase() == payload.topic.to_lowercase() && same_statement)
                    .then(|| id.clone())
            })
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
    /// Returns `Err` if the cid is not found in `active`.
    pub fn revise(
        cid: &CommitmentId,
        new_payload: FactualClaimPayload,
        turn: usize,
        store: &SemanticCommitmentStore,
    ) -> Result<SemanticCommitmentStore, String> {
        let mut new_store = store.clone();

        if !new_store.active.contains_key(cid) {
            return Err(format!("cid {:?} not found in active commitments", cid));
        }

        new_store
            .active
            .insert(cid.clone(), (new_payload.clone(), turn));
        let lineage = new_store.lineage.entry(cid.clone()).or_default();
        lineage.push(LineageEvent::Revised { turn });

        Ok(new_store)
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
                let exact = query_words.intersection(&stmt_words).count();
                // Stem-based match: first 5 characters, char-safe for UTF-8.
                let stem_overlap: usize = query_words
                    .iter()
                    .filter(|qw| qw.chars().count() >= 5)
                    .map(|qw| {
                        let qw_chars: Vec<char> = qw.chars().collect();
                        let stem: String = qw_chars[..5].iter().collect();
                        stmt_words
                            .iter()
                            .filter(|sw| {
                                let sw_chars: Vec<char> = sw.chars().collect();
                                sw_chars.len() >= 5
                                    && sw_chars[..5].iter().collect::<String>() == stem
                            })
                            .count()
                    })
                    .sum();
                (exact * 2 + stem_overlap, payload.clone())
            })
            .filter(|(overlap, _)| *overlap > 0)
            .collect();

        matches.sort_by_key(|b| std::cmp::Reverse(b.0));
        matches.into_iter().take(5).map(|(_, p)| p).collect()
    }

    /// Detect whether the current turn engages or contradicts held commitments.
    /// Contradiction detection includes both Russian and English keywords,
    /// and routes through semantic signals where available.
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
        let lower = input_topic.to_lowercase();
        let contradicted = lower.contains("не ")
            || lower.contains("противореч")
            || lower.contains("ошиба")
            || lower.contains("не верно")
            || lower.contains("contradict")
            || lower.contains("wrong")
            || lower.contains("error")
            || lower.contains("refute")
            || lower.contains("oppose")
            || lower.contains("deny")
            || lower.contains("incorrect");

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
        let (new_store, result) = CommitmentOps::commit(payload, &store);

        assert_eq!(result, CommitResult::New(CommitmentId(0)));
        assert!(new_store.active.contains_key(&CommitmentId(0)));
        assert_eq!(new_store.next_id, 1);
        assert_eq!(new_store.lineage.get(&CommitmentId(0)).unwrap().len(), 1);
    }

    #[test]
    fn test_commit_duplicate_no_id_skip() {
        let mut store = SemanticCommitmentStore::default();
        // Force a collision: next_id points to an existing key.
        store
            .active
            .insert(CommitmentId(0), (make_payload("a", "b"), 0));
        store.next_id = 0;

        let payload = make_payload("свобода", "свобода предполагает выбор");
        let (store2, result) = CommitmentOps::commit(payload, &store);

        assert_eq!(result, CommitResult::Duplicate(CommitmentId(0)));
        assert_eq!(store2.next_id, 0, "next_id must not advance on duplicate");
        assert_eq!(store2.active.len(), 1, "duplicate should not insert");
    }

    #[test]
    fn test_commit_observation_dedup() {
        let mut store = SemanticCommitmentStore::default();
        // Force a collision: next_id points to an existing key.
        store
            .active
            .insert(CommitmentId(0), (make_payload("x", "y"), 0));
        store.next_id = 0;

        let mut payload = make_payload("topic", "stmt");
        payload.turn_seq = 2;
        let (store, result) = CommitmentOps::commit_observation(payload.clone(), &store);
        assert_eq!(result, CommitResult::Duplicate(CommitmentId(0)));
        assert_eq!(store.next_id, 0, "next_id must not advance on duplicate");
        assert_eq!(store.active.len(), 1, "duplicate should not insert");
    }

    #[test]
    fn test_commit_observation_deduplicates_content() {
        let store = SemanticCommitmentStore::default();
        let payload = make_payload("свобода", "свобода предполагает выбор");
        let (store, first) = CommitmentOps::commit_observation(payload.clone(), &store);
        assert!(matches!(first, CommitResult::New(_)));

        let mut duplicate = payload;
        duplicate.statement = "  СВОБОДА   предполагает выбор ".into();
        duplicate.turn_seq = 99;
        let (store, result) = CommitmentOps::commit_observation(duplicate, &store);
        assert_eq!(result, CommitResult::Duplicate(CommitmentId(0)));
        assert_eq!(store.active.len(), 1);
    }

    #[test]
    fn test_commitment_capacity_is_enforced() {
        let mut store = SemanticCommitmentStore::default();
        for index in 0..MAX_COMMITMENTS {
            store.active.insert(
                CommitmentId(index),
                (make_payload(&format!("topic-{index}"), "statement"), index),
            );
        }
        store.next_id = MAX_COMMITMENTS;
        let (unchanged, result) =
            CommitmentOps::commit_observation(make_payload("overflow", "new statement"), &store);
        assert_eq!(result, CommitResult::CapacityReached);
        assert_eq!(unchanged.active.len(), MAX_COMMITMENTS);
        assert_eq!(unchanged.next_id, MAX_COMMITMENTS);
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
    fn test_detect_engagement_contradiction_english() {
        let store = SemanticCommitmentStore::default();
        let payload = make_payload("свобода", "свобода предполагает выбор");
        let (store, _) = CommitmentOps::commit(payload, &store);

        let eng = CommitmentOps::detect_engagement(&store, "that is wrong about свобода");
        assert!(eng.contradicted);
        assert_eq!(eng.match_kind, MatchKind::ContradictedStrong);
    }

    #[test]
    fn test_revise_updates_payload() {
        let store = SemanticCommitmentStore::default();
        let payload = make_payload("истина", "истина — это соответствие");
        let (store, result) = CommitmentOps::commit(payload, &store);
        let CommitResult::New(cid) = result else {
            panic!("expected New")
        };

        let new_payload = make_payload("истина", "истина — это воспроизводимость");
        let store = CommitmentOps::revise(&cid, new_payload, 2, &store).unwrap();

        let updated = store.active.get(&cid).unwrap();
        assert!(updated.0.statement.contains("воспроизводимость"));
        assert_eq!(store.lineage.get(&cid).unwrap().len(), 2);
    }

    #[test]
    fn test_revise_missing_cid_returns_err() {
        let store = SemanticCommitmentStore::default();
        let new_payload = make_payload("x", "y");
        let result = CommitmentOps::revise(&CommitmentId(99), new_payload, 2, &store);
        assert!(result.is_err());
    }

    #[test]
    fn test_contradict_records_event() {
        let store = SemanticCommitmentStore::default();
        let (store, left_res) = CommitmentOps::commit(make_payload("a", "a is x"), &store);
        let CommitResult::New(left) = left_res else {
            panic!("expected New")
        };
        let (store, right_res) = CommitmentOps::commit(make_payload("a", "a is not x"), &store);
        let CommitResult::New(right) = right_res else {
            panic!("expected New")
        };

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
