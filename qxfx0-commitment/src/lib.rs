pub mod thesis_lifecycle;
pub use thesis_lifecycle::{LifecycleApply, LifecycleError, ThesisLifecycleOps};

use qxfx0_types::system_state::*;
use qxfx0_types::system_state::{MAX_CONTRADICTIONS, MAX_LINEAGE_PER_ID};
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

/// Revision dynamics (Haskell `Logic/BeliefRevision` analog, bounded):
/// the weaker side of a contradiction is halved, dependents by a
/// quarter-step, and anything below the floor is quarantined (never
/// deleted — the lineage testifies). Single-level propagation only.
pub const REVISION_WEAKEN_FACTOR: f64 = 0.5;
pub const REVISION_PROPAGATION_FACTOR: f64 = 0.75;
pub const REVISION_QUARANTINE_FLOOR: f64 = 0.3;

/// Governed forgetting (Memory M4): a live position retires only when
/// ALL hold — untouched for `FORGET_TTL_TURNS` turns, confidence below
/// `FORGET_CONFIDENCE_CEILING`, uncontested, no live dependents.
/// At most `MAX_FORGET_PER_TURN` retire per turn (id order), lineage
/// records `Retracted(Forgotten)` — forgetting stays visible, never
/// the silent eviction the capacity path refuses.
pub const FORGET_TTL_TURNS: usize = 50;
pub const FORGET_CONFIDENCE_CEILING: f64 = 0.5;
pub const MAX_FORGET_PER_TURN: usize = 8;

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
    /// Honors the same combined `MAX_COMMITMENTS` bound as commit paths:
    /// `Duplicate` when the content already exists, `CapacityReached` when
    /// the store is full (state unchanged), `New` otherwise.
    pub fn quarantine_observation(
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
        new_store.quarantine.insert(cid.clone(), (payload, turn));
        (new_store, CommitResult::New(cid))
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
        Self::push_lineage(&mut new_store, cid, LineageEvent::Revised { turn });

        Ok(new_store)
    }

    /// Record a contradiction between two commitments. Bounded: oldest
    /// events drain past `MAX_CONTRADICTIONS`, so the log can never
    /// wedge the state permanently invalid.
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
        let excess = new_store
            .contradictions
            .len()
            .saturating_sub(MAX_CONTRADICTIONS);
        if excess > 0 {
            new_store.contradictions.drain(..excess);
        }
        new_store
    }

    /// Push one lineage event, draining oldest past the per-id bound.
    fn push_lineage(store: &mut SemanticCommitmentStore, id: &CommitmentId, event: LineageEvent) {
        let lineage = store.lineage.entry(id.clone()).or_default();
        lineage.push(event);
        let excess = lineage.len().saturating_sub(MAX_LINEAGE_PER_ID);
        if excess > 0 {
            lineage.drain(..excess);
        }
    }

    /// Belief revision on a caught contradiction: the weaker live side
    /// is weakened (halved), its direct dependents by a quarter-step,
    /// and anything below the quarantine floor moves to quarantine
    /// with a `ParserContradiction` lineage (never deleted). Ties
    /// weaken the challenger (`left` by call convention) — held
    /// positions stand. Missing ids (already retired) are a no-op:
    /// revision applies to live positions only. Pure and deterministic
    /// (dependents visit in id order).
    pub fn revise_on_contradiction(
        left: &CommitmentId,
        right: &CommitmentId,
        turn: usize,
        store: &SemanticCommitmentStore,
    ) -> SemanticCommitmentStore {
        let mut new_store = store.clone();
        let (left_confidence, right_confidence) =
            match (new_store.active.get(left), new_store.active.get(right)) {
                (Some((left_payload, _)), Some((right_payload, _))) => {
                    (left_payload.confidence, right_payload.confidence)
                }
                _ => return new_store,
            };
        let weaker = if left_confidence <= right_confidence {
            left.clone()
        } else {
            right.clone()
        };
        Self::weaken(&weaker, REVISION_WEAKEN_FACTOR, turn, &mut new_store);
        let mut dependents: Vec<CommitmentId> = new_store
            .active
            .iter()
            .filter(|(id, (payload, _))| *id != &weaker && payload.deps.contains(&weaker))
            .map(|(id, _)| id.clone())
            .collect();
        dependents.sort();
        for dependent in dependents {
            Self::weaken(
                &dependent,
                REVISION_PROPAGATION_FACTOR,
                turn,
                &mut new_store,
            );
        }
        new_store
    }

    /// Governed forgetting: retire stale, low-confidence, uncontested
    /// live positions with no live dependents. A contradiction keeps
    /// its sides alive only while IT is live (within the TTL): an
    /// ancient dispute is stale history, not an open disagreement.
    /// Returns the new store and the retired ids (id order, capped).
    /// Retired positions leave `active` but keep their lineage with
    /// `Retracted(Forgotten)`. Pure and deterministic.
    pub fn forget_stale(
        turn: usize,
        store: &SemanticCommitmentStore,
    ) -> (SemanticCommitmentStore, Vec<CommitmentId>) {
        let contested: BTreeSet<CommitmentId> = store
            .contradictions
            .iter()
            .filter(|event| turn.saturating_sub(event.turn) < FORGET_TTL_TURNS)
            .flat_map(|event| [event.left.clone(), event.right.clone()])
            .collect();
        let depended_on: BTreeSet<CommitmentId> = store
            .active
            .values()
            .flat_map(|(payload, _)| payload.deps.iter().cloned())
            .collect();
        let mut candidates: Vec<CommitmentId> = store
            .active
            .iter()
            .filter(|(id, (payload, touched))| {
                turn.saturating_sub(*touched) >= FORGET_TTL_TURNS
                    && payload.confidence < FORGET_CONFIDENCE_CEILING
                    && !contested.contains(*id)
                    && !depended_on.contains(*id)
            })
            .map(|(id, _)| id.clone())
            .collect();
        candidates.sort();
        candidates.truncate(MAX_FORGET_PER_TURN);
        let mut new_store = store.clone();
        for id in &candidates {
            new_store.active.remove(id);
            Self::push_lineage(
                &mut new_store,
                id,
                LineageEvent::Retracted {
                    turn,
                    reason: RetractionReason::Forgotten,
                },
            );
        }
        (new_store, candidates)
    }

    /// Weaken one live position by `factor`, quarantining below the
    /// floor. No-op on ids outside `active`.
    fn weaken(id: &CommitmentId, factor: f64, turn: usize, store: &mut SemanticCommitmentStore) {
        let Some((payload, _)) = store.active.get(id) else {
            return;
        };
        let weakened = payload.confidence * factor;
        if weakened < REVISION_QUARANTINE_FLOOR {
            let (payload, committed_turn) = store.active.remove(id).expect("checked above");
            store
                .quarantine
                .insert(id.clone(), (payload, committed_turn));
            Self::push_lineage(
                store,
                id,
                LineageEvent::Retracted {
                    turn,
                    reason: RetractionReason::ParserContradiction,
                },
            );
        } else {
            let mut revised = payload.clone();
            revised.confidence = weakened;
            store.active.insert(id.clone(), (revised, turn));
            Self::push_lineage(store, id, LineageEvent::Revised { turn });
        }
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
                let stem_overlap = stem_overlap_count(&query_words, &stmt_words);
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

        // Find which commitment IDs are engaged. Stem-aware: Russian
        // inflection means the user's «о свободе» must match a stored
        // «свобода» — an exact-word filter here would go blind on every
        // oblique form the journal actually receives.
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
                    || stem_overlap_count(&query_words, &stmt_words) > 0
            })
            .map(|(cid, _)| cid.clone())
            .collect();

        // Check for contradiction signals in input. Token-based, not
        // substring-with-space: the old `contains("не ")` went blind on
        // the most natural Russian disagreement — a leading «нет»
        // (practice-2 turn 8: «нет свобода первична…» engaged but never
        // contradicted) — and on «не» before punctuation. A bare «нет»
        // counts only sentence-initially: «у меня нет ответа» reports
        // absence, not disagreement. A bare signal without an engaged
        // counterpart is harmless (no counterpart → no record).
        let lower = input_topic.to_lowercase();
        let contradicted = has_contradiction_signal(&lower);

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
            // Promotion moves an entry between maps, so the combined bound
            // is preserved; the active guard only protects legacy states
            // that already exceed MAX_COMMITMENTS.
            if new_store.active.len() >= MAX_COMMITMENTS {
                break;
            }
            if let Some((payload, _)) = new_store.quarantine.remove(&cid) {
                new_store.active.insert(cid.clone(), (payload, turn));
                Self::push_lineage(&mut new_store, &cid, LineageEvent::Promoted { turn });
            }
        }

        new_store
    }
}

/// Stem-based overlap between two word sets: first 5 characters, char-safe
/// for UTF-8. Shared by retrieval and engagement so both see the same
/// Russian inflection.
fn stem_overlap_count(query: &BTreeSet<&str>, statement: &BTreeSet<&str>) -> usize {
    query
        .iter()
        .filter(|qw| qw.chars().count() >= 5)
        .map(|qw| {
            let qw_chars: Vec<char> = qw.chars().collect();
            let stem: String = qw_chars[..5].iter().collect();
            statement
                .iter()
                .filter(|sw| {
                    let sw_chars: Vec<char> = sw.chars().collect();
                    sw_chars.len() >= 5 && sw_chars[..5].iter().collect::<String>() == stem
                })
                .count()
        })
        .sum()
}

/// Word-token signal: `word` present as a standalone token
/// (unicode-alphanumeric boundaries), so «не» fires before any
/// punctuation, not just before a space.
fn tokens_contain(lower: &str, word: &str) -> bool {
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| token == word)
}

/// A bare «нет» is disagreement only sentence-initially —
/// «у меня нет ответа» reports absence. Segments split on sentence
/// punctuation; quotes and dashes around the first word are skipped
/// by the non-alphanumeric filter.
fn sentence_starts_with_net(lower: &str) -> bool {
    lower.split(['.', '?', '!', ':', ';', '\n']).any(|segment| {
        segment
            .split(|c: char| !c.is_alphanumeric())
            .find(|token| !token.is_empty())
            == Some("нет")
    })
}

/// Pure contradiction-signal half of engagement detection, testable
/// without a store (the store gate only decides NoMatch vs engaged).
fn has_contradiction_signal(lower: &str) -> bool {
    lower.contains("противореч")
        || lower.contains("ошиб")
        || lower.contains("возража")
        || lower.contains("несоглас")
        || lower.contains("неверно")
        || lower.contains("не верно")
        || lower.contains("напротив")
        || lower.contains("неправ")
        || lower.contains("contradict")
        || lower.contains("wrong")
        || lower.contains("error")
        || lower.contains("refute")
        || lower.contains("oppose")
        || lower.contains("deny")
        || lower.contains("incorrect")
        || tokens_contain(lower, "не")
        || tokens_contain(lower, "no")
        || sentence_starts_with_net(lower)
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
    fn test_contradiction_signals_russian_matrix() {
        // (input, contradicted?) — signals must fire on words, not on
        // space-terminated substrings, and a bare «нет» only initially.
        for (input, expected) in [
            // Practice-2 turn 8 regression: leading «нет» disagrees.
            ("нет свобода первична: без выбора нечего держать", true),
            ("но ведь выбор под принуждением не свободен?", true),
            ("выбор не, прямо скажем, свободен", true),
            ("это неверно", true),
            ("не верно", true),
            ("ты ошибаешься", true),
            ("здесь ошибка", true),
            ("я несогласен", true),
            ("напротив, свобода первична", true),
            ("это неправда", true),
            ("я возражаю", true),
            ("противоречишь себе", true),
            ("no, freedom is primary", true),
            // No signal: plain statements and mid-sentence «нет».
            ("свобода это возможность выбора", false),
            ("у меня нет ответа", false),
            ("ответа нет, но есть вопрос", false),
            ("монета лежит на столе", false),
        ] {
            assert_eq!(
                has_contradiction_signal(&input.to_lowercase()),
                expected,
                "input: {input}"
            );
        }
    }

    #[test]
    fn test_turn_eight_regression_end_to_end() {
        // Practice-2 turn 8 against a held position: engaged AND
        // contradicted, so the pipeline records the contradiction.
        let store = SemanticCommitmentStore::default();
        let payload = make_payload("свобода", "ответственность первична а свобода вторична");
        let (store, _) = CommitmentOps::commit(payload, &store);
        let eng = CommitmentOps::detect_engagement(
            &store,
            "нет свобода первична: без выбора нечего держать",
        );
        assert!(eng.contradicted);
        assert_eq!(eng.match_kind, MatchKind::ContradictedStrong);
        assert!(!eng.engaged_ids.is_empty());
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

    #[test]
    fn test_quarantine_enforces_capacity() {
        let mut store = SemanticCommitmentStore::default();
        for index in 0..MAX_COMMITMENTS {
            store.active.insert(
                CommitmentId(index),
                (make_payload(&format!("topic-{index}"), "statement"), index),
            );
        }
        store.next_id = MAX_COMMITMENTS;
        let (unchanged, result) = CommitmentOps::quarantine_observation(
            make_payload("overflow", "new statement"),
            &store,
        );
        assert_eq!(result, CommitResult::CapacityReached);
        assert_eq!(unchanged.active.len(), MAX_COMMITMENTS);
        assert!(unchanged.quarantine.is_empty());
        assert_eq!(unchanged.next_id, MAX_COMMITMENTS);
    }

    fn stale_store() -> SemanticCommitmentStore {
        // id0: stale + weak -> forgotten. id1: fresh -> kept. id2:
        // stale but confident -> kept. id3: stale + weak but
        // contested -> kept. id4: stale + weak but depended-on -> kept.
        let mut store = SemanticCommitmentStore::default();
        let mut payload = make_payload("память", "старая слабая позиция");
        payload.confidence = 0.4;
        store.active.insert(CommitmentId(0), (payload, 1));
        let mut fresh = make_payload("память", "свежая позиция");
        fresh.confidence = 0.4;
        store.active.insert(CommitmentId(1), (fresh, 100));
        let mut strong = make_payload("память", "старая сильная позиция");
        strong.confidence = 0.9;
        store.active.insert(CommitmentId(2), (strong, 1));
        let mut contested = make_payload("память", "оспоренная позиция");
        contested.confidence = 0.4;
        store.active.insert(CommitmentId(3), (contested, 1));
        store.contradictions.push(ContradictionEvent {
            left: CommitmentId(3),
            right: CommitmentId(2),
            kind: ContradictionKind::ContradictionStatement,
            turn: 90,
        });
        let mut keeper = make_payload("память", "позиция-опора");
        keeper.confidence = 0.4;
        store.active.insert(CommitmentId(4), (keeper, 1));
        let mut dependent = make_payload("память", "зависимая позиция");
        dependent.confidence = 0.9;
        dependent.deps = vec![CommitmentId(4)];
        store.active.insert(CommitmentId(5), (dependent, 100));
        store.next_id = 6;
        store
    }

    #[test]
    fn test_forget_stale_retires_only_the_forgettable() {
        let store = stale_store();
        let (forgotten_store, forgotten) = CommitmentOps::forget_stale(100, &store);
        assert_eq!(forgotten, vec![CommitmentId(0)]);
        assert!(!forgotten_store.active.contains_key(&CommitmentId(0)));
        for kept in [1, 2, 3, 4, 5] {
            assert!(
                forgotten_store.active.contains_key(&CommitmentId(kept)),
                "id{kept} must survive forgetting"
            );
        }
        // Forgetting is visible: lineage records the retirement.
        let lineage = forgotten_store.lineage.get(&CommitmentId(0)).unwrap();
        assert!(matches!(
            lineage.last(),
            Some(LineageEvent::Retracted {
                reason: RetractionReason::Forgotten,
                turn: 100,
            })
        ));
        // Pure: the input store is untouched.
        assert!(store.active.contains_key(&CommitmentId(0)));
    }

    #[test]
    fn test_forget_stale_ancient_dispute_is_history() {
        // Same stale weak position, but the contradiction is older than
        // the TTL: the dispute is stale history, the position retires.
        let mut store = stale_store();
        store.contradictions.clear();
        store.contradictions.push(ContradictionEvent {
            left: CommitmentId(3),
            right: CommitmentId(2),
            kind: ContradictionKind::ContradictionStatement,
            turn: 10,
        });
        let (forgotten_store, forgotten) = CommitmentOps::forget_stale(100, &store);
        assert!(forgotten.contains(&CommitmentId(3)));
        assert!(!forgotten_store.active.contains_key(&CommitmentId(3)));
    }

    #[test]
    fn test_forget_stale_caps_per_turn_in_id_order() {
        let mut store = SemanticCommitmentStore::default();
        for index in 0..(MAX_FORGET_PER_TURN + 3) {
            let mut payload = make_payload("память", &format!("старая позиция {index}"));
            payload.confidence = 0.4;
            store.active.insert(CommitmentId(index), (payload, 1));
        }
        store.next_id = MAX_FORGET_PER_TURN + 3;
        let (forgotten_store, forgotten) = CommitmentOps::forget_stale(100, &store);
        assert_eq!(forgotten.len(), MAX_FORGET_PER_TURN);
        let mut ordered = forgotten.clone();
        ordered.sort();
        assert_eq!(forgotten, ordered, "id order, deterministic");
        assert_eq!(
            forgotten_store.active.len(),
            3,
            "the remainder retires next turn"
        );
    }

    #[test]
    fn test_quarantine_deduplicates_content() {
        let store = SemanticCommitmentStore::default();
        let payload = make_payload("свобода", "свобода предполагает выбор");
        let (store, first) = CommitmentOps::quarantine_observation(payload.clone(), &store);
        assert!(matches!(first, CommitResult::New(_)));

        let (store, second) = CommitmentOps::quarantine_observation(payload, &store);
        assert!(matches!(second, CommitResult::Duplicate(_)));
        assert_eq!(store.quarantine.len(), 1);
    }

    #[test]
    fn test_promote_respects_active_bound_for_legacy_states() {
        // A legacy state may already exceed the bound via the previously
        // uncapped quarantine path; promotion must not grow active further.
        let mut store = SemanticCommitmentStore::default();
        for index in 0..MAX_COMMITMENTS {
            store.active.insert(
                CommitmentId(index),
                (make_payload(&format!("active-{index}"), "statement"), index),
            );
        }
        store.quarantine.insert(
            CommitmentId(MAX_COMMITMENTS),
            (make_payload("legacy", "legacy stmt"), 1),
        );
        store.next_id = MAX_COMMITMENTS + 1;

        let promoted = CommitmentOps::promote_matching_quarantine(&store, "legacy", 2);
        assert_eq!(promoted.active.len(), MAX_COMMITMENTS);
        assert_eq!(
            promoted.quarantine.len(),
            1,
            "legacy quarantine entry must stay put"
        );
    }

    #[test]
    fn test_promote_moves_matching_entries_within_bound() {
        let store = SemanticCommitmentStore::default();
        let (store, first) =
            CommitmentOps::quarantine_observation(make_payload("тема", "заявление"), &store);
        let CommitResult::New(cid) = first else {
            panic!("expected New")
        };
        let (store, _) = CommitmentOps::quarantine_observation(
            make_payload("другое", "другое заявление"),
            &store,
        );

        let promoted = CommitmentOps::promote_matching_quarantine(&store, "тема", 5);
        assert!(promoted.active.contains_key(&cid));
        assert!(!promoted.quarantine.contains_key(&cid));
        assert_eq!(promoted.quarantine.len(), 1);
        assert_eq!(promoted.active.len() + promoted.quarantine.len(), 2);
    }

    fn revised_store() -> SemanticCommitmentStore {
        let store = SemanticCommitmentStore::default();
        let strong = FactualClaimPayload {
            confidence: 0.9,
            ..make_payload("свобода", "свобода предполагает выбор")
        };
        let weak = FactualClaimPayload {
            confidence: 0.8,
            ..make_payload("свобода", "свобода это произвол")
        };
        let (store, _) = CommitmentOps::commit(strong, &store);
        let (store, _) = CommitmentOps::commit(weak, &store);
        store
    }

    #[test]
    fn revision_weakens_the_weaker_live_side() {
        let store = revised_store();
        let revised =
            CommitmentOps::revise_on_contradiction(&CommitmentId(1), &CommitmentId(0), 5, &store);
        assert_eq!(
            revised
                .active
                .get(&CommitmentId(1))
                .map(|(payload, _)| payload.confidence),
            Some(0.4),
            "0.8 halved"
        );
        assert_eq!(
            revised
                .active
                .get(&CommitmentId(0))
                .map(|(payload, _)| payload.confidence),
            Some(0.9),
            "stronger side untouched"
        );
        assert!(matches!(
            revised.lineage.get(&CommitmentId(1)).map(Vec::as_slice),
            Some([.., LineageEvent::Revised { turn: 5 }])
        ));
    }

    #[test]
    fn revision_ties_weaken_the_challenger() {
        let store = SemanticCommitmentStore::default();
        let (store, _) = CommitmentOps::commit(make_payload("свобода", "позиция один"), &store);
        let (store, _) = CommitmentOps::commit(make_payload("свобода", "позиция два"), &store);
        let revised =
            CommitmentOps::revise_on_contradiction(&CommitmentId(1), &CommitmentId(0), 5, &store);
        // Tie weakens the left (challenger) side: 0.5 halved to 0.25
        // lands below the floor, so it quarantines with lineage.
        assert!(!revised.active.contains_key(&CommitmentId(1)));
        assert!(revised.quarantine.contains_key(&CommitmentId(1)));
        assert!(
            revised.active.contains_key(&CommitmentId(0)),
            "holder stands"
        );
    }

    #[test]
    fn revision_quarantines_below_the_floor_and_propagates_once() {
        let store = SemanticCommitmentStore::default();
        let weak = FactualClaimPayload {
            confidence: 0.5,
            ..make_payload("свобода", "слабая позиция")
        };
        let (store, _) = CommitmentOps::commit(weak, &store);
        let dependent = FactualClaimPayload {
            confidence: 0.9,
            deps: vec![CommitmentId(0)],
            ..make_payload("свобода", "зависимая позиция")
        };
        let (store, _) = CommitmentOps::commit(dependent, &store);
        let strong = FactualClaimPayload {
            confidence: 0.9,
            ..make_payload("свобода", "сильная позиция")
        };
        let (store, _) = CommitmentOps::commit(strong, &store);
        let revised =
            CommitmentOps::revise_on_contradiction(&CommitmentId(2), &CommitmentId(0), 7, &store);
        assert!(
            !revised.active.contains_key(&CommitmentId(0)),
            "0.5 halved to 0.25 < 0.3 quarantines"
        );
        assert!(revised.quarantine.contains_key(&CommitmentId(0)));
        assert!(matches!(
            revised.lineage.get(&CommitmentId(0)).map(Vec::as_slice),
            Some([.., LineageEvent::Retracted { turn: 7, .. }])
        ));
        assert_eq!(
            revised
                .active
                .get(&CommitmentId(1))
                .map(|(payload, _)| payload.confidence),
            Some(0.675),
            "dependent quarter-stepped: 0.9 * 0.75"
        );
        assert_eq!(
            revised
                .active
                .get(&CommitmentId(2))
                .map(|(payload, _)| payload.confidence),
            Some(0.9),
            "stronger side untouched"
        );
    }

    #[test]
    fn revision_ignores_retired_ids() {
        let store = revised_store();
        let unchanged =
            CommitmentOps::revise_on_contradiction(&CommitmentId(99), &CommitmentId(0), 5, &store);
        assert_eq!(
            serde_json::to_string(&unchanged).unwrap(),
            serde_json::to_string(&store).unwrap()
        );
    }

    #[test]
    fn contradiction_log_drains_oldest_past_the_cap() {
        let store = revised_store();
        let mut flooded = store.clone();
        for turn in 0..(MAX_CONTRADICTIONS + 100) {
            flooded = CommitmentOps::contradict(
                &CommitmentId(0),
                &CommitmentId(1),
                ContradictionKind::ContradictionStatement,
                turn,
                &flooded,
            );
        }
        assert_eq!(flooded.contradictions.len(), MAX_CONTRADICTIONS);
        assert_eq!(
            flooded.contradictions.first().map(|event| event.turn),
            Some(100)
        );
        assert_eq!(
            flooded.contradictions.last().map(|event| event.turn),
            Some(MAX_CONTRADICTIONS + 99)
        );
    }

    #[test]
    fn lineage_drains_oldest_past_the_per_id_cap() {
        // Halving quarantines in ≤3 steps, so the cap is exercised
        // directly: promote-churn paths accumulate over long sessions.
        let mut store = revised_store();
        for turn in 0..(MAX_LINEAGE_PER_ID + 10) {
            CommitmentOps::push_lineage(
                &mut store,
                &CommitmentId(0),
                LineageEvent::Revised { turn },
            );
        }
        let events = store.lineage.get(&CommitmentId(0)).expect("lineage kept");
        assert_eq!(events.len(), MAX_LINEAGE_PER_ID);
        assert!(matches!(
            events.last(),
            Some(LineageEvent::Revised { turn }) if *turn == MAX_LINEAGE_PER_ID + 9
        ));
    }
}
