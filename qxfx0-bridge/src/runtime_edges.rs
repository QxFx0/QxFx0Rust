//! Runtime bridge edges — the exact port of Haskell `QxFx0.Learning.
//! RuntimeLLMFeedback` (reinforce / decay / retire / prune / promote).
//!
//! The algebra is total and deterministic; there is no wall clock, no random
//! and no I/O. Input is a bounded store plus one [`CorroborationEvent`],
//! output is a new store plus an applied [`BridgeOutcome`]. The key
//! (`(from, to)`) mirrors the Haskell `M.Map`; the value is a
//! [`BridgeEdge`] — deliberately *surface-free* (no `ru_original`, no
//! `Relation`): an associative trace that renders nothing.

use serde::{Deserialize, Serialize};

use qxfx0_types::{AtomId, RelationType};

/// Confidence at which a runtime bridge edge becomes promotable (pending
/// the U5 human release; the source tag only marks readiness here).
pub const RUNTIME_PROMOTION_CONFIDENCE: f64 = 0.75;
/// Co-occurrence count a runtime bridge edge needs to become promotable.
pub const RUNTIME_PROMOTION_CO_OCCURRENCE: usize = 3;
/// Hard cap on runtime bridge edges kept per turn boundary (Haskell
/// `dcMaxEdges`).
pub const RUNTIME_EDGE_CAP: usize = 500;

/// Provenance of a bridge edge. `RuntimeBridge` is the working, decaying
/// evidence (Haskell `ProvenanceRuntimeLLM`); `Promoted` marks an edge the
/// reinforcement ladder accepted into the stable tier (Haskell
/// `ProvenanceDialogueFeedback`) — exempt from decay, subject to the U5
/// admission bar before it ever enters the rendered graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeEdgeSource {
    RuntimeBridge,
    Promoted,
}

/// A surface-free associative edge. `confidence` is the scalar the ladder
/// moves; `weight` mirrors it scaled by the relation type, recomputed
/// whenever confidence changes (the Haskell `seWeight` invariant).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeEdge {
    pub from: AtomId,
    pub to: AtomId,
    pub rel_type: RelationType,
    pub topic: String,
    pub confidence: f64,
    pub co_occurrence: usize,
    pub weight: f64,
    pub source: BridgeEdgeSource,
}

impl BridgeEdge {
    /// A fresh runtime bridge edge at unit weight scaling. Total over the
    /// supplied confidence (clamped to `[0, 1]`).
    pub fn new(
        from: AtomId,
        to: AtomId,
        rel_type: RelationType,
        topic: impl Into<String>,
        confidence: f64,
    ) -> Self {
        let confidence = confidence.clamp(0.0, 1.0);
        Self {
            from,
            to,
            rel_type,
            topic: topic.into(),
            confidence,
            co_occurrence: 1,
            weight: confidence * relation_type_weight(rel_type),
            source: BridgeEdgeSource::RuntimeBridge,
        }
    }
}

/// Deterministic per-relation scale, mirroring Haskell
/// `relationTypeWeight`. Editorial defaults, not calibrated (ADR-0043);
/// every known type maps to a fixed finite weight and unknown ones share
/// the `RelRelatedTo` scale.
pub fn relation_type_weight(rel_type: RelationType) -> f64 {
    match rel_type {
        RelationType::RelPresupposes | RelationType::RelRequires => 1.0,
        RelationType::RelLimitedBy | RelationType::RelDetermines => 0.8,
        RelationType::RelClaims | RelationType::RelVerifiedBy => 0.7,
        RelationType::RelSignals | RelationType::RelReveals => 0.6,
        RelationType::RelExpresses | RelationType::RelDenotes => 0.6,
        RelationType::RelDiffersFrom => 0.5,
        RelationType::RelPreserves | RelationType::RelOrientsToward => 0.5,
        RelationType::RelStructures => 0.7,
        RelationType::RelTransforms | RelationType::RelTransformsInto => 0.4,
        RelationType::RelCreatedFrom | RelationType::RelGives => 0.4,
        RelationType::RelRecognizes | RelationType::RelUnifies => 0.6,
        RelationType::RelConnects | RelationType::RelPrecedes => 0.5,
        RelationType::RelDependsOn | RelationType::RelPrescribes => 0.6,
        _ => 0.5,
    }
}

/// The three corroboration outcomes (Haskell `RuntimeLLMOutcome`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeOutcome {
    Positive,
    Negative,
    Conflict,
}

/// The applied delta a single corroboration produced — trace evidence for
/// U5 promotion decisions; carries no surface.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Corroboration {
    pub applied: bool,
    pub from_confidence: f64,
    pub to_confidence: f64,
    pub co_occurrence: usize,
    pub retired: bool,
    pub promoted: bool,
}

impl Corroboration {
    /// No edge was present to corroborate (a no-op trace).
    pub fn not_applied() -> Self {
        Self {
            applied: false,
            from_confidence: 0.0,
            to_confidence: 0.0,
            co_occurrence: 0,
            retired: false,
            promoted: false,
        }
    }
}

/// The runtime bridge store: `(from, to) -> edge`. Ordered by key, so
/// serialization and pruning are deterministic across processes.
pub type RuntimeEdgeStore = std::collections::BTreeMap<(AtomId, AtomId), BridgeEdge>;

/// Tunable turn-boundary decay parameters (Haskell `DecayConfig`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DecayConfig {
    /// Multiplicative decay for unused runtime edges per turn boundary.
    pub decay_rate: f64,
    /// Confidence below which a decayed runtime edge is retired.
    pub retire_threshold: f64,
    /// Hard cap on runtime edges kept per boundary; excess lowest-confidence
    /// runtime edges are pruned.
    pub max_edges: usize,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            decay_rate: 0.95,
            retire_threshold: 0.3,
            max_edges: RUNTIME_EDGE_CAP,
        }
    }
}

/// The runtime-bridge edges only (Haskell filters by provenance; promotion
/// excludes them from decay and cap pruning).
pub fn runtime_edges(store: &RuntimeEdgeStore) -> Vec<&BridgeEdge> {
    store
        .values()
        .filter(|edge| edge.source == BridgeEdgeSource::RuntimeBridge)
        .collect()
}

/// Count of decaying runtime-bridge edges (the promoted tier is exempt).
pub fn runtime_edge_count(store: &RuntimeEdgeStore) -> usize {
    store
        .values()
        .filter(|edge| edge.source == BridgeEdgeSource::RuntimeBridge)
        .count()
}

/// Look up an edge by its `(from, to)` key.
pub fn edge_by_pair<'a>(
    store: &'a RuntimeEdgeStore,
    from: &AtomId,
    to: &AtomId,
) -> Option<&'a BridgeEdge> {
    store.get(&(from.clone(), to.clone()))
}

/// Serialize the store for persistence. A `BTreeMap` with a tuple key is
/// not a legal JSON object (keys must be strings), so the canonical wire
/// shape is the ordered edge vector — deterministic because the map is
/// already key-ordered. `None` (empty store) encodes to `null` so the
/// maintenance path can clear the row.
pub fn edges_to_json(store: &RuntimeEdgeStore) -> String {
    if store.is_empty() {
        return "null".to_string();
    }
    let edges: Vec<&BridgeEdge> = store.values().collect();
    serde_json::to_string(&edges).expect("BridgeEdge is a JSON array of objects")
}

/// Rebuild the store from its persisted vector form. A corrupt blob is a
/// hard error (fail-closed) — the maintenance path refuses to proceed on
/// unparseable evidence rather than silently dropping the store.
pub fn edges_from_json(json: &str) -> Result<RuntimeEdgeStore, String> {
    if json.trim() == "null" {
        return Ok(RuntimeEdgeStore::new());
    }
    let edges: Vec<BridgeEdge> =
        serde_json::from_str(json).map_err(|error| format!("bridge edges_json: {error}"))?;
    let mut store = RuntimeEdgeStore::new();
    for edge in edges {
        store.insert((edge.from.clone(), edge.to.clone()), edge);
    }
    Ok(store)
}

/// The reinforcement step (Haskell `reinforceRuntimeEdge`): total; a
/// non-runtime edge is returned unchanged (corroboration only moves
/// working evidence), a conflict retires the edge (`None`).
pub fn apply_corroboration(outcome: BridgeOutcome, edge: &BridgeEdge) -> Option<BridgeEdge> {
    if edge.source != BridgeEdgeSource::RuntimeBridge {
        return Some(edge.clone());
    }
    match outcome {
        BridgeOutcome::Positive => {
            let reinforced = BridgeEdge {
                confidence: (edge.confidence + 0.05).min(1.0),
                co_occurrence: edge.co_occurrence + 1,
                ..edge.clone()
            };
            Some(promote_if_ready(reinforced))
        }
        BridgeOutcome::Negative => Some(BridgeEdge {
            confidence: (edge.confidence - 0.10).max(0.0),
            ..edge.clone()
        }),
        BridgeOutcome::Conflict => None,
    }
}

/// The Haskell `promoteIfReady` morphism: when confidence and co-occurrence
/// clear the ladder, the source flips to `Promoted` (the stable tier,
/// pending U5 admission).
pub fn promote_if_ready(mut edge: BridgeEdge) -> BridgeEdge {
    if edge.confidence >= RUNTIME_PROMOTION_CONFIDENCE
        && edge.co_occurrence >= RUNTIME_PROMOTION_CO_OCCURRENCE
        && edge.source == BridgeEdgeSource::RuntimeBridge
    {
        edge.source = BridgeEdgeSource::Promoted;
    }
    edge
}

/// Apply one corroboration to the store (Haskell `applyRuntimeLLMFeedback`):
/// `M.update` semantics — absent key no-ops, `None` removes the edge.
/// Returns the updated store plus an applied [`Corroboration`] trace.
pub fn apply_corroboration_event(
    mut store: RuntimeEdgeStore,
    from: &AtomId,
    to: &AtomId,
    outcome: BridgeOutcome,
) -> (RuntimeEdgeStore, Corroboration) {
    let key = (from.clone(), to.clone());
    let trace = match store.get(&key) {
        None => Corroboration::not_applied(),
        Some(before) => {
            let after = apply_corroboration(outcome, before);
            let promoted = after.as_ref().is_some_and(|edge| {
                edge.source == BridgeEdgeSource::Promoted
                    && before.source != BridgeEdgeSource::Promoted
            });
            Corroboration {
                applied: true,
                from_confidence: before.confidence,
                to_confidence: after.as_ref().map(|edge| edge.confidence).unwrap_or(0.0),
                co_occurrence: after.as_ref().map(|edge| edge.co_occurrence).unwrap_or(0),
                retired: after.is_none(),
                promoted,
            }
        }
    };
    match store
        .get(&key)
        .cloned()
        .and_then(|edge| apply_corroboration(outcome, &edge))
    {
        Some(updated) => {
            store.insert(key, updated);
        }
        None => {
            store.remove(&key);
        }
    }
    (store, trace)
}

/// Turn-boundary decay and retirement (Haskell `applyEdgeDecayAndRetire`).
/// Edges touching the current topic are "used" and skipped; other runtime
/// edges decay multiplicatively, retire below the threshold, and the store
/// is pruned to `max_edges` runtime-bridge edges (kept by highest
/// confidence, tie-broken by key for determinism). Promoted edges are
/// untouched.
pub fn apply_edge_decay_and_retire(
    store: RuntimeEdgeStore,
    topic: &str,
    config: &DecayConfig,
) -> RuntimeEdgeStore {
    let mut decayed: RuntimeEdgeStore = store
        .into_iter()
        .filter_map(|(key, edge)| decay_edge(&edge, key.0.as_str(), key.1.as_str(), topic, config))
        .collect();
    if runtime_edge_count(&decayed) > config.max_edges {
        let mut runtime: Vec<(AtomId, AtomId, f64)> = decayed
            .iter()
            .filter(|((_, _), edge)| edge.source == BridgeEdgeSource::RuntimeBridge)
            .map(|((from, to), edge)| (from.clone(), to.clone(), edge.confidence))
            .collect();
        runtime.sort_by(|left, right| {
            right
                .2
                .partial_cmp(&left.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| (&left.0, &left.1).cmp(&(&right.0, &right.1)))
        });
        let keep: std::collections::BTreeSet<(AtomId, AtomId)> = runtime
            .into_iter()
            .take(config.max_edges)
            .map(|(from, to, _)| (from, to))
            .collect();
        decayed.retain(|key, edge| {
            edge.source != BridgeEdgeSource::RuntimeBridge || keep.contains(key)
        });
    }
    decayed
}

fn decay_edge(
    edge: &BridgeEdge,
    from: &str,
    to: &str,
    topic: &str,
    config: &DecayConfig,
) -> Option<((AtomId, AtomId), BridgeEdge)> {
    let key = (edge.from.clone(), edge.to.clone());
    if edge.source != BridgeEdgeSource::RuntimeBridge || from == topic || to == topic {
        return Some((key, edge.clone()));
    }
    let new_confidence = edge.confidence * config.decay_rate;
    if new_confidence < config.retire_threshold {
        None
    } else {
        Some((
            key,
            BridgeEdge {
                confidence: new_confidence,
                weight: new_confidence * relation_type_weight(edge.rel_type),
                ..edge.clone()
            },
        ))
    }
}

/// Encode a runtime-edge store to its persisted JSON form. A `BTreeMap`
/// with a non-string key does not serialize as a JSON object, so the
/// canonical shape is the key-sorted edge *list* — the pair key lives in
/// each edge's own `from`/`to`. Deterministic: BTreeMap order is the
/// serialization order, so a fresh process reads back exactly what the
/// worker wrote.
pub fn encode_store(store: &RuntimeEdgeStore) -> Result<String, serde_json::Error> {
    serde_json::to_string(&store.values().collect::<Vec<_>>())
}

/// Decode the persisted JSON form back into a store. A malformed blob is an
/// error (fail-closed), never a silently-empty store: a corrupt bridge
/// table is an operator problem, not a reason to forget the evidence.
pub fn decode_store(encoded: &str) -> Result<RuntimeEdgeStore, serde_json::Error> {
    let edges: Vec<BridgeEdge> = serde_json::from_str(encoded)?;
    Ok(edges
        .into_iter()
        .map(|edge| ((edge.from.clone(), edge.to.clone()), edge))
        .collect())
}

/// Structural invariants for `doctor`. The bridge must be a pure scalar
/// ladder on bounded, surface-free evidence: promotion thresholds in range
/// and coherent, the default config positive and bounded, and the
/// weight-scaling total and finite. Returns the violations (empty = ok).
pub fn validate_bridge_invariants() -> Vec<String> {
    let mut violations = Vec::new();
    if !(0.0..=1.0).contains(&RUNTIME_PROMOTION_CONFIDENCE) {
        violations.push("bridge RUNTIME_PROMOTION_CONFIDENCE out of [0,1]".into());
    }
    if RUNTIME_PROMOTION_CO_OCCURRENCE == 0 {
        violations.push("bridge RUNTIME_PROMOTION_CO_OCCURRENCE must be positive".into());
    }
    let config = DecayConfig::default();
    if !(0.0..1.0).contains(&config.decay_rate) {
        violations.push(format!(
            "bridge decay_rate out of (0,1): {}",
            config.decay_rate
        ));
    }
    if !(0.0..=1.0).contains(&config.retire_threshold) {
        violations.push(format!(
            "bridge retire_threshold out of [0,1]: {}",
            config.retire_threshold
        ));
    }
    if config.max_edges == 0 {
        violations.push("bridge max_edges must be positive".into());
    }
    if config.retire_threshold >= 1.0 && config.decay_rate >= 1.0 {
        violations
            .push("bridge decay would never retire (threshold and rate both saturated)".into());
    }
    for rel in [
        RelationType::RelRelatedTo,
        RelationType::RelPresupposes,
        RelationType::RelTransformsInto,
        RelationType::RelDependsOn,
    ] {
        let weight = relation_type_weight(rel);
        if !weight.is_finite() || weight <= 0.0 {
            violations.push(format!(
                "bridge relation weight invalid for {rel:?}: {weight}"
            ));
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(confidence: f64) -> BridgeEdge {
        BridgeEdge::new(
            AtomId::new("свобода"),
            AtomId::new("выбор"),
            RelationType::RelRelatedTo,
            "свобода",
            confidence,
        )
    }

    fn store_with(edge: BridgeEdge) -> RuntimeEdgeStore {
        let mut store = RuntimeEdgeStore::new();
        store.insert((edge.from.clone(), edge.to.clone()), edge);
        store
    }

    #[test]
    fn positive_reinforce_raises_confidence_and_count() {
        let (store, trace) = apply_corroboration_event(
            store_with(edge(0.5)),
            &AtomId::new("свобода"),
            &AtomId::new("выбор"),
            BridgeOutcome::Positive,
        );
        let kept = store
            .get(&(AtomId::new("свобода"), AtomId::new("выбор")))
            .expect("edge present");
        assert!((kept.confidence - 0.55).abs() < 1e-9);
        assert_eq!(kept.co_occurrence, 2);
        assert!(trace.applied && !trace.promoted);
    }

    #[test]
    fn negative_punishes_flooring_at_zero() {
        let (store, _) = apply_corroboration_event(
            store_with(edge(0.05)),
            &AtomId::new("свобода"),
            &AtomId::new("выбор"),
            BridgeOutcome::Negative,
        );
        let kept = store
            .get(&(AtomId::new("свобода"), AtomId::new("выбор")))
            .expect("negative never removes, only lowers");
        assert_eq!(kept.confidence, 0.0);
    }

    #[test]
    fn conflict_retires_the_edge() {
        let (store, trace) = apply_corroboration_event(
            store_with(edge(0.9)),
            &AtomId::new("свобода"),
            &AtomId::new("выбор"),
            BridgeOutcome::Conflict,
        );
        assert!(store.is_empty());
        assert!(trace.retired);
    }

    #[test]
    fn promotion_lands_after_three_reinforcements_at_threshold() {
        // 0.6 → +0.05 → 0.65 → 0.70 → 0.75 with co 2 → 3 → 4 (initial co 1).
        let mut store = store_with(edge(0.6));
        let from = AtomId::new("свобода");
        let to = AtomId::new("выбор");
        for _ in 0..3 {
            let (next, _) = apply_corroboration_event(store, &from, &to, BridgeOutcome::Positive);
            store = next;
        }
        let kept = store.get(&(from, to)).expect("promoted edge survives");
        assert_eq!(kept.source, BridgeEdgeSource::Promoted);
        assert!(kept.confidence >= RUNTIME_PROMOTION_CONFIDENCE);
    }

    #[test]
    fn promoted_edges_are_exempt_from_reinforcement_and_decay() {
        let promoted = BridgeEdge {
            source: BridgeEdgeSource::Promoted,
            confidence: 0.8,
            co_occurrence: 5,
            ..edge(0.8)
        };
        // Reinforcement returns it unchanged (Haskell guard on provenance).
        let unchanged = apply_corroboration(BridgeOutcome::Positive, &promoted).expect("kept");
        assert_eq!(unchanged.confidence, 0.8);
        assert_eq!(unchanged.co_occurrence, 5);
        // Decay skips it even off-topic.
        let decayed =
            apply_edge_decay_and_retire(store_with(promoted), "другая", &DecayConfig::default());
        assert!(decayed.values().all(|edge| edge.confidence == 0.8));
    }

    #[test]
    fn decay_skips_topic_touching_edges_and_retires_below_threshold() {
        let mut store = RuntimeEdgeStore::new();
        let on_topic = BridgeEdge::new(
            AtomId::new("свобода"),
            AtomId::new("воля"),
            RelationType::RelRelatedTo,
            "свобода",
            0.5,
        );
        let off_topic_low = BridgeEdge::new(
            AtomId::new("разум"),
            AtomId::new("мысль"),
            RelationType::RelRelatedTo,
            "разум",
            0.31,
        );
        store.insert((on_topic.from.clone(), on_topic.to.clone()), on_topic);
        store.insert(
            (off_topic_low.from.clone(), off_topic_low.to.clone()),
            off_topic_low,
        );
        let decayed = apply_edge_decay_and_retire(store, "свобода", &DecayConfig::default());
        // on-topic stays at full confidence
        assert!(
            decayed
                .get(&(AtomId::new("свобода"), AtomId::new("воля")))
                .expect("used edge survives")
                .confidence
                > 0.49
        );
        // 0.31 * 0.95 = 0.2945 < 0.3 → retired
        assert!(!decayed.contains_key(&(AtomId::new("разум"), AtomId::new("мысль"))));
    }

    #[test]
    fn prune_to_cap_keeps_highest_confidence_tie_broken_by_key() {
        let mut store = RuntimeEdgeStore::new();
        for (index, confidence) in [0.9, 0.4, 0.7].into_iter().enumerate() {
            let edge = BridgeEdge::new(
                AtomId::new(format!("a{index}")),
                AtomId::new(format!("b{index}")),
                RelationType::RelRelatedTo,
                "unrelated",
                confidence,
            );
            store.insert((edge.from.clone(), edge.to.clone()), edge);
        }
        let config = DecayConfig {
            decay_rate: 1.0,
            retire_threshold: 0.0,
            max_edges: 2,
        };
        let pruned = apply_edge_decay_and_retire(store, "topic", &config);
        assert_eq!(pruned.len(), 2);
        assert!(pruned.contains_key(&(AtomId::new("a0"), AtomId::new("b0"))));
        assert!(pruned.contains_key(&(AtomId::new("a2"), AtomId::new("b2"))));
    }

    #[test]
    fn weight_recomputation_is_deterministic_and_total() {
        let e = edge(0.5);
        assert!((e.weight - 0.5 * relation_type_weight(RelationType::RelRelatedTo)).abs() < 1e-12);
        assert!(relation_type_weight(RelationType::RelRelatedTo) > 0.0);
    }

    #[test]
    fn store_codec_round_trips_and_empty_is_not_none() {
        let mut store = RuntimeEdgeStore::new();
        store.insert((AtomId::new("свобода"), AtomId::new("выбор")), edge(0.6));
        let encoded = encode_store(&store).expect("serializes");
        let decoded = decode_store(&encoded).expect("deserializes");
        assert_eq!(decoded, store);
        // Key order is preserved (BTreeMap), so the encoding is stable.
        assert_eq!(encoded, encode_store(&decoded).unwrap());
        // An empty store encodes; decode of "[]" is an empty map, not None.
        assert_eq!(encode_store(&RuntimeEdgeStore::new()).unwrap(), "[]");
        assert!(decode_store("[]").unwrap().is_empty());
        // A malformed blob is a decode error, never a silent empty store.
        assert!(decode_store("{not a list}").is_err());
    }

    #[test]
    fn invariants_hold_on_the_builtins() {
        assert!(validate_bridge_invariants().is_empty());
    }
}
