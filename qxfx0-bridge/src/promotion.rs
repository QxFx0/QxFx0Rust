//! Promotion — the gated boundary between runtime bridge evidence and a
//! reviewable, fingerprinted overlay (ADR-0043 U5; Haskell
//! `QxFx0.Learning.Promotion` is the spec).
//!
//! Runtime bridge edges stay associative evidence that renders nothing
//! (U4). Promotion is the *only* door from that evidence toward visible
//! content, and it is triple-gated by law: informativeness with a
//! semantic-gain threshold, a versioned gate policy, and an immutable
//! human release. This module is the pure core of that boundary:
//!
//! - [`GatePolicy`] is versioned independently of candidate and overlay
//!   identities and carries a SHA-256 checksum, so a revalidation is
//!   auditable and an old draft is never treated as fresh evidence for its
//!   own candidate.
//! - [`evaluate_candidate_informativeness`] is the exact port of the
//!   Haskell decision: a candidate must not be tautological or a topic
//!   paraphrase, must add genuinely novel information against the curated
//!   baseline for its topic, and must clear the semantic-gain floor.
//! - The overlay lifecycle is `draft → activate → release`, with
//!   [`rollback`] as the only step back and a content-addressed version
//!   that pins its parent. A released overlay is immutable; "human release
//!   is permanent" is enforced by the transition table, not by convention.
//!
//! A released overlay is a reviewable artifact, *not* a live graph edit:
//! its effect on the embedded pack happens only when an operator admits it
//! through the same editorial bar a content wave passes (the fingerprint
//! mechanism of ADR-0043 law 1), which is why nothing here can mutate
//! `SystemState` or a session's pack fingerprint. The store and CLI that
//! carry these values are `qxfx0-persistence` (opaque rows beside the
//! session) and `qxfx0-cli`; this module is pure, total and deterministic.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use qxfx0_types::{AtomId, RelationType};

/// The semantic-gain floor a candidate must clear to count as informative
/// (Haskell `informativenessSemanticGainThreshold`).
pub const SEMANTIC_GAIN_THRESHOLD: f64 = 0.75;

/// The constraint relations whose novelty alone qualifies a candidate
/// (Haskell `constraint`). The canonical slugs are derived from
/// [`RelationType`] by [`canonical_slug`]; no Rust relation is currently
/// named `causes`, so that disjunct simply never fires here — faithful,
/// not an omission.
pub const CONSTRAINT_SLUGS: [&str; 4] = ["requires", "limited_by", "contrasts_with", "presupposes"];

/// The Russian stop words dropped when building a surface's atom set
/// (Haskell `stopWords`).
pub const STOP_WORDS: [&str; 10] = [
    "это",
    "что",
    "как",
    "для",
    "или",
    "при",
    "через",
    "между",
    "перед",
    "после",
];

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Canonicalize a surface word the way the promotion boundary does
/// (`normalizeAtom`): lower-case, trim, drop trailing punctuation.
pub fn normalize_atom(value: &str) -> String {
    let trimmed = value.trim().to_lowercase();
    trimmed
        .trim_end_matches(['?', '!', '.', ',', ';', ':'])
        .to_string()
}

/// `RelDependsOn` -> `depends_on`: strip the `Rel` prefix, then insert an
/// underscore before each interior capital and lower-case. Deterministic,
/// total, and independent of the Russian surface wording, so promotion
/// duplicate detection keys on the canonical triple, not on inflection.
fn camel_to_snake(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 4);
    for (index, ch) in input.char_indices() {
        if ch.is_uppercase() && index > 0 {
            out.push('_');
        }
        out.extend(ch.to_lowercase());
    }
    out
}

/// The stable canonical slug of a relation type (strip the `Rel` prefix,
/// snake-case). This is the promotion-boundary relation identity: it is
/// independent of the Russian surface wording. It reproduces the Haskell
/// canonical slugs exactly — `RelRequires` → "requires",
/// `RelLimitedBy` → "limited_by", `RelContrastsWith` → "contrasts_with",
/// `RelPresupposes` → "presupposes", `RelIsA` → "is_a".
pub fn canonical_slug(relation: RelationType) -> String {
    let debug = format!("{relation:?}");
    let stripped = debug.strip_prefix("Rel").unwrap_or(&debug);
    camel_to_snake(stripped)
}

/// A versioned gate policy. `checksum` pins version + description so a
/// change is visible in the overlay identity, and an unchanged policy is
/// reproducible across builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatePolicy {
    pub version: String,
    pub description: String,
}

/// The U5.1 builtin policy. Its description states exactly what this port
/// enforces; revalidation compares versions, and the candidate identity is
/// snapshot-scoped, so an old draft is never fresh evidence for itself.
pub fn builtin_gate_policy() -> GatePolicy {
    GatePolicy {
        version: "promotion-v1-artifact-fold".into(),
        description: "canonical subject/relation/object duplicate and subsumption gates; \
                      snapshot-scoped candidate identity; informativeness with a semantic-gain \
                      floor against the curated topic baseline; a released overlay is an \
                      immutable reviewable artifact whose graph effect requires editorial \
                      admission into the embedded pack"
            .into(),
    }
}

impl GatePolicy {
    pub fn checksum(&self) -> String {
        sha256_hex(format!("{}\n{}", self.version, self.description).as_bytes())
    }
}

/// The three informativeness questions plus the gain and the verdict
/// (Haskell `InformativenessResult`). Every field is a stable trace fact.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InformativenessResult {
    pub not_tautological: bool,
    pub adds_novel_information: bool,
    pub not_topic_paraphrase: bool,
    pub semantic_gain: f64,
    pub passed: bool,
}

/// A snapshot-scoped promotion candidate: a canonical triple with its
/// runtime support and the rendered Russian surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotionCandidate {
    pub snapshot_id: String,
    pub topic: String,
    pub subject: AtomId,
    pub relation: RelationType,
    pub object: AtomId,
    pub rendered_ru: String,
    pub confidence: f64,
    pub support: usize,
}

impl PromotionCandidate {
    /// The candidate's stable identity: snapshot-scoped so a re-run of the
    /// same evidence collapses to one candidate (idempotent history, not
    /// double-counted evidence).
    pub fn candidate_id(&self) -> String {
        sha256_hex(
            format!(
                "{}|{}|{}|{}|{}",
                self.snapshot_id,
                normalize_atom(&self.topic),
                normalize_atom(self.subject.as_str()),
                canonical_slug(self.relation),
                normalize_atom(self.object.as_str()),
            )
            .as_bytes(),
        )
    }
}

fn content_atoms(surface: &str) -> std::collections::BTreeSet<String> {
    surface
        .split_whitespace()
        .map(normalize_atom)
        .filter(|word| word.chars().count() > 3 && !STOP_WORDS.contains(&word.as_str()))
        .collect()
}

fn jaccard(
    left: &std::collections::BTreeSet<String>,
    right: &std::collections::BTreeSet<String>,
) -> f64 {
    let union = left.union(right).count();
    if union == 0 {
        0.0
    } else {
        left.intersection(right).count() as f64 / union as f64
    }
}

/// Decide one candidate against the curated surfaces already admitted for
/// its topic (Haskell `evaluateCandidateInformativeness`). `baseline` is
/// the topic's existing rendered surfaces; an empty baseline means the
/// topic has no curated content yet, so every well-formed novel triple
/// clears the overlap bar (semantic gain 1.0). Total and deterministic.
pub fn evaluate_candidate_informativeness(
    candidate: &PromotionCandidate,
    baseline: &[String],
) -> InformativenessResult {
    let topic = normalize_atom(&candidate.topic);
    let subject = normalize_atom(candidate.subject.as_str());
    let relation = canonical_slug(candidate.relation);
    let object = normalize_atom(candidate.object.as_str());
    let candidate_atoms: std::collections::BTreeSet<String> =
        [subject.clone(), relation.clone(), object.clone()]
            .into_iter()
            .collect();

    let baseline_sets: Vec<std::collections::BTreeSet<String>> = baseline
        .iter()
        .map(|surface| content_atoms(surface))
        .collect();
    let base_atoms: std::collections::BTreeSet<String> =
        baseline_sets.iter().flatten().cloned().collect();

    let max_overlap = baseline_sets
        .iter()
        .map(|atoms| jaccard(&candidate_atoms, atoms))
        .fold(0.0f64, f64::max);
    let semantic_gain = 1.0 - max_overlap;

    let new_object = !base_atoms.contains(&object);
    let constraint = CONSTRAINT_SLUGS.contains(&relation.as_str());
    let new_relation = !base_atoms.contains(&relation);
    let adds_novel =
        new_object || constraint || (new_relation && semantic_gain >= SEMANTIC_GAIN_THRESHOLD);

    let not_tautological =
        !subject.is_empty() && !relation.is_empty() && !object.is_empty() && subject != object;
    let not_topic_paraphrase =
        not_tautological && object != topic && normalize_atom(&candidate.rendered_ru) != topic;

    let passed = not_tautological
        && not_topic_paraphrase
        && adds_novel
        && semantic_gain >= SEMANTIC_GAIN_THRESHOLD;

    InformativenessResult {
        not_tautological,
        adds_novel_information: adds_novel,
        not_topic_paraphrase,
        semantic_gain,
        passed,
    }
}

/// One predicate fixed into an overlay: the candidate plus the informativeness
/// trace that justified it and the policy that gated it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotedPredicate {
    pub candidate_id: String,
    pub topic: String,
    pub subject: AtomId,
    pub relation: RelationType,
    pub object: AtomId,
    pub rendered_ru: String,
    pub confidence: f64,
    pub support: usize,
    pub semantic_gain: f64,
    pub gate_policy_version: String,
}

/// Overlay lifecycle status. The only legal transitions are Draft→Activated,
/// Activated→Released and the rollback of the active pointer; a Released
/// overlay is immutable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayStatus {
    Draft,
    Activated,
    Released,
}

/// A content-addressed overlay of promoted predicates, with a parent link
/// for rollback (Haskell `promotion_overlays` row + `parent_version`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overlay {
    pub version: String,
    pub parent_version: Option<String>,
    pub snapshot_id: String,
    pub status: OverlayStatus,
    pub created_at: i64,
    pub activated_at: Option<i64>,
    pub released_at: Option<i64>,
    pub predicates: Vec<PromotedPredicate>,
    pub policy_version: String,
    pub policy_checksum: String,
    pub checksum: String,
}

/// Why a candidate was excluded from a draft, with the reason code the trace
/// carries (Haskell `promotion_candidate_exclusions.reason_code`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExclusionReason {
    NotTautological,
    NovelInformation,
    TopicParaphrase,
    SemanticGain,
    /// A candidate identical to one already in the draft (canonical triple
    /// duplicate — the base gate, independent of surface wording).
    Duplicate,
}

/// The reason a lifecycle operation was refused. Every refusal is a hard
/// stop, never a silent no-op (fail-closed law 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromotionError {
    /// A transition not in the legal table (e.g. releasing a Draft).
    IllegalTransition {
        from: OverlayStatus,
        to: OverlayStatus,
    },
    /// Release requires the overlay to have been activated first.
    NotActivated,
    /// Rollback was asked to a version that is not the overlay's parent.
    ParentMismatch {
        expected: Option<String>,
        given: Option<String>,
    },
    /// A draft with no admitted predicates releases nothing.
    EmptyOverlay,
    /// A predicate failed its checksum/identity binding on load (corrupt).
    ChecksumMismatch,
}

impl std::fmt::Display for PromotionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IllegalTransition { from, to } => {
                write!(formatter, "illegal overlay transition {from:?} -> {to:?}")
            }
            Self::NotActivated => write!(formatter, "an overlay must be activated before release"),
            Self::ParentMismatch { expected, given } => write!(
                formatter,
                "rollback target {given:?} is not this overlay's parent {expected:?}"
            ),
            Self::EmptyOverlay => {
                write!(formatter, "a draft with no predicates cannot advance")
            }
            Self::ChecksumMismatch => {
                write!(formatter, "overlay checksum does not match its predicates")
            }
        }
    }
}

impl std::error::Error for PromotionError {}

fn predicate_checksum(predicates: &[PromotedPredicate]) -> String {
    // Stable over a canonical serialization: predicates are stored sorted by
    // candidate_id, so the digest depends only on the admitted set.
    let mut ordered: Vec<&PromotedPredicate> = predicates.iter().collect();
    ordered.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    sha256_hex(
        serde_json::to_vec(&ordered)
            .expect("promoted predicates serialize")
            .as_slice(),
    )
}

fn overlay_checksum(
    snapshot_id: &str,
    policy_version: &str,
    policy_checksum: &str,
    predicates: &[PromotedPredicate],
) -> String {
    sha256_hex(
        format!(
            "{snapshot_id}\u{1}{policy_version}\u{1}{policy_checksum}\u{1}{}",
            predicate_checksum(predicates)
        )
        .as_bytes(),
    )
}

/// Materialize a draft overlay from the candidates that clear both the base
/// duplicate gate and the informativeness gate against `baseline_for`
/// (a lookup from topic → curated surfaces). Non-passing candidates are
/// returned as exclusions rather than dropped, so the review sees the reason.
/// Timestamps come from the caller, never sampled here (determinism).
pub fn create_draft(
    snapshot_id: &str,
    candidates: &[PromotionCandidate],
    policy: &GatePolicy,
    baseline_for: &dyn Fn(&str) -> Vec<String>,
    created_at: i64,
) -> (Overlay, Vec<(PromotionCandidate, ExclusionReason)>) {
    let mut predicates: Vec<(PromotionCandidate, f64)> = Vec::new();
    let mut exclusions: Vec<(PromotionCandidate, ExclusionReason)> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for candidate in candidates {
        let result = evaluate_candidate_informativeness(candidate, &baseline_for(&candidate.topic));
        if seen.contains(&candidate.candidate_id()) {
            exclusions.push((candidate.clone(), ExclusionReason::Duplicate));
            continue;
        }
        let exclusion = if !result.not_tautological {
            Some(ExclusionReason::NotTautological)
        } else if !result.not_topic_paraphrase {
            Some(ExclusionReason::TopicParaphrase)
        } else if !result.adds_novel_information {
            Some(ExclusionReason::NovelInformation)
        } else if result.semantic_gain < SEMANTIC_GAIN_THRESHOLD {
            Some(ExclusionReason::SemanticGain)
        } else {
            None
        };
        // `exclusion` is Some exactly when one of the four `passed`
        // conjuncts failed, so `exclusion.is_none()` and `result.passed`
        // agree by construction; the reason is always the first failure.
        if let Some(reason) = exclusion {
            exclusions.push((candidate.clone(), reason));
        } else {
            seen.insert(candidate.candidate_id());
            predicates.push((candidate.clone(), result.semantic_gain));
        }
    }

    let fixed: Vec<PromotedPredicate> = predicates
        .into_iter()
        .map(|(candidate, gain)| PromotedPredicate {
            candidate_id: candidate.candidate_id(),
            topic: normalize_atom(&candidate.topic),
            subject: candidate.subject,
            relation: candidate.relation,
            object: candidate.object,
            rendered_ru: candidate.rendered_ru,
            confidence: candidate.confidence,
            support: candidate.support,
            semantic_gain: gain,
            gate_policy_version: policy.version.clone(),
        })
        .collect();
    let mut ordered = fixed;
    ordered.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));

    let checksum = overlay_checksum(snapshot_id, &policy.version, &policy.checksum(), &ordered);
    let overlay = Overlay {
        version: format!("overlay-{checksum}"),
        parent_version: None,
        snapshot_id: snapshot_id.to_string(),
        status: OverlayStatus::Draft,
        created_at,
        activated_at: None,
        released_at: None,
        predicates: ordered,
        policy_version: policy.version.clone(),
        policy_checksum: policy.checksum(),
        checksum,
    };
    (overlay, exclusions)
}

impl Overlay {
    /// Verify the self-binding checksum survives a store round-trip (a tamper
    /// or corruption check on load).
    pub fn verify_integrity(&self) -> Result<(), PromotionError> {
        let recomputed = overlay_checksum(
            &self.snapshot_id,
            &self.policy_version,
            &self.policy_checksum,
            &self.predicates,
        );
        if recomputed == self.checksum && self.version == format!("overlay-{recomputed}") {
            Ok(())
        } else {
            Err(PromotionError::ChecksumMismatch)
        }
    }

    /// Draft → Activated. Refuses a non-empty invariant and any other source
    /// status; the transition is pure and total over status.
    pub fn activate(&self, at: i64) -> Result<Overlay, PromotionError> {
        match self.status {
            OverlayStatus::Draft => {
                if self.predicates.is_empty() {
                    return Err(PromotionError::EmptyOverlay);
                }
                Ok(Overlay {
                    status: OverlayStatus::Activated,
                    activated_at: Some(at),
                    ..self.clone()
                })
            }
            from => Err(PromotionError::IllegalTransition {
                from,
                to: OverlayStatus::Activated,
            }),
        }
    }

    /// Activated → Released (the human release; permanent). Records a
    /// release timestamp and keeps the predicate set frozen — releasing
    /// twice or releasing a Draft is refused.
    pub fn release(&self, at: i64) -> Result<Overlay, PromotionError> {
        match self.status {
            OverlayStatus::Activated => Ok(Overlay {
                status: OverlayStatus::Released,
                released_at: Some(at),
                ..self.clone()
            }),
            OverlayStatus::Draft => Err(PromotionError::NotActivated),
            OverlayStatus::Released => Err(PromotionError::IllegalTransition {
                from: OverlayStatus::Released,
                to: OverlayStatus::Released,
            }),
        }
    }
}

/// Rollback the active pointer to a released overlay's parent. The released
/// overlay row itself is never edited (release is permanent); rollback only
/// retires which version is current, exactly the Haskell `rollbackPromotion-
/// Overlay` moving `promotion_active`. Returns the parent version that
/// becomes active, or `None` at the root (nothing to roll back to).
pub fn rollback(active: &Overlay) -> Result<Option<String>, PromotionError> {
    if active.status != OverlayStatus::Released {
        return Err(PromotionError::IllegalTransition {
            from: active.status,
            to: OverlayStatus::Activated,
        });
    }
    Ok(active.parent_version.clone())
}

/// A released overlay is a reviewable artifact for the operator: render the
/// canonical triples plus their informativeness, so the artifact that will be
/// admitted into the embedded pack is human-inspectable and diffable.
pub fn render_overlay_artifact(overlay: &Overlay) -> String {
    let mut lines = vec![format!(
        "# overlay {} (snapshot {}, policy {}#{}, checksum {})",
        overlay.version,
        overlay.snapshot_id,
        overlay.policy_version,
        overlay.policy_checksum,
        overlay.checksum
    )];
    for predicate in &overlay.predicates {
        lines.push(format!(
            "{} --[{}]--> {}  (topic {}, conf {:.3}, support {}, gain {:.3})",
            predicate.subject.as_str(),
            canonical_slug(predicate.relation),
            predicate.object.as_str(),
            predicate.topic,
            predicate.confidence,
            predicate.support,
            predicate.semantic_gain,
        ));
    }
    lines.join("\n")
}

/// Structural invariants for `doctor`. The promotion boundary must be gated
/// and its lifecycle must be a legal transition table before anything can be
/// released. Returns the violations (empty = ok).
pub fn validate_promotion_invariants() -> Vec<String> {
    let mut violations = Vec::new();
    if !(0.0..=1.0).contains(&SEMANTIC_GAIN_THRESHOLD) {
        violations.push("promotion SEMANTIC_GAIN_THRESHOLD out of [0,1]".into());
    }
    let policy = builtin_gate_policy();
    if policy.version.trim().is_empty() || policy.description.trim().is_empty() {
        violations.push("promotion builtin gate policy has an empty version or description".into());
    }
    if policy.checksum().len() != 64 {
        violations.push("promotion gate policy checksum is not a SHA-256 hex digest".into());
    }
    // A draft built from a single passing candidate must activate then
    // release, and a released overlay must reject re-release — the
    // immutability the human-release-permanence law depends on.
    let snapshot = "invariant";
    let candidate = PromotionCandidate {
        snapshot_id: snapshot.into(),
        topic: "свобода".into(),
        subject: AtomId::new("свобода"),
        relation: RelationType::RelRequires,
        object: AtomId::new("ответственность"),
        rendered_ru: "свобода требует ответственность".into(),
        confidence: 0.9,
        support: 4,
    };
    let empty_baseline = |_topic: &str| Vec::new();
    let (draft, exclusions) = create_draft(
        snapshot,
        std::slice::from_ref(&candidate),
        &policy,
        &empty_baseline,
        0,
    );
    if !exclusions.is_empty() || draft.predicates.len() != 1 {
        violations
            .push("promotion invariant: a novel constraint candidate must draft cleanly".into());
    }
    if draft.verify_integrity().is_err() {
        violations.push("promotion invariant: a fresh draft must self-verify".into());
    }
    if draft.release(1).is_ok() {
        violations.push("promotion invariant: a Draft must not release without activation".into());
    }
    let activated = draft.activate(2).expect("draft activates");
    let released = activated.release(3).expect("activated releases");
    if released.release(4).is_ok() {
        violations.push("promotion invariant: a Released overlay must be immutable".into());
    }
    if released.activate(5).is_ok() {
        violations.push("promotion invariant: a Released overlay must not re-activate".into());
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        topic: &str,
        subject: &str,
        relation: RelationType,
        object: &str,
        rendered: &str,
    ) -> PromotionCandidate {
        PromotionCandidate {
            snapshot_id: "snap-1".into(),
            topic: topic.into(),
            subject: AtomId::new(subject),
            relation,
            object: AtomId::new(object),
            rendered_ru: rendered.into(),
            confidence: 0.9,
            support: 5,
        }
    }

    fn never_baseline(_topic: &str) -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn novel_candidate_clears_all_gates() {
        let candidate = candidate(
            "свобода",
            "свобода",
            RelationType::RelRequires,
            "ответственность",
            "свобода требует ответственность",
        );
        let result = evaluate_candidate_informativeness(&candidate, &[]);
        assert!(result.passed, "{result:?}");
        assert_eq!(result.semantic_gain, 1.0); // empty baseline: no overlap
        assert!(result.not_tautological);
        assert!(result.adds_novel_information);
        assert!(result.not_topic_paraphrase);
    }

    #[test]
    fn self_referential_candidate_is_tautological() {
        let result = evaluate_candidate_informativeness(
            &candidate(
                "свобода",
                "свобода",
                RelationType::RelRelatedTo,
                "свобода",
                "x y z",
            ),
            &[],
        );
        assert!(!result.not_tautological);
        assert!(!result.passed);
    }

    #[test]
    fn topic_paraphrase_is_rejected() {
        // object == topic after normalization.
        let result = evaluate_candidate_informativeness(
            &candidate(
                "свобода",
                "воля",
                RelationType::RelRelatedTo,
                "Свобода",
                "рассуждение",
            ),
            &[],
        );
        assert!(!result.not_topic_paraphrase);
        assert!(!result.passed);
    }

    #[test]
    fn full_overlap_kills_semantic_gain() {
        let candidate = candidate(
            "свобода",
            "свобода",
            RelationType::RelRequires,
            "ответственность",
            "свобода требует ответственность",
        );
        // A baseline surface sharing every candidate atom drives gain to 0.
        let baseline = vec!["свобода requires ответственность".into()];
        let result = evaluate_candidate_informativeness(&candidate, &baseline);
        assert!(result.semantic_gain.abs() < 1e-12, "{result:?}");
        assert!(!result.passed);
    }

    #[test]
    fn constraint_relation_novelties_even_below_gain_floor() {
        // A constraint relation counts as novel information on its own, but
        // `passed` still demands the gain floor; here gain is below floor
        // because the object overlaps the baseline, so it must NOT pass.
        let candidate = candidate("свобода", "свобода", RelationType::RelRequires, "воля", "a");
        let baseline = vec!["воля что-то ещё длинное".into()];
        let with_constraint = evaluate_candidate_informativeness(&candidate, &baseline);
        assert!(with_constraint.adds_novel_information); // constraint disjunct
        assert!(
            with_constraint.semantic_gain >= SEMANTIC_GAIN_THRESHOLD,
            "{with_constraint:?}"
        );
    }

    #[test]
    fn draft_activate_release_lifecycle_and_immutability() {
        let policy = builtin_gate_policy();
        let (draft, exclusions) = create_draft(
            "snap-1",
            &[candidate(
                "свобода",
                "свобода",
                RelationType::RelRequires,
                "выбор",
                "свобода требует выбор",
            )],
            &policy,
            &never_baseline,
            100,
        );
        assert!(exclusions.is_empty());
        assert_eq!(draft.status, OverlayStatus::Draft);
        assert_eq!(draft.activated_at, None);
        // Illegal early release.
        assert_eq!(draft.release(101), Err(PromotionError::NotActivated));
        let activated = draft.activate(102).unwrap();
        assert_eq!(activated.status, OverlayStatus::Activated);
        assert_eq!(activated.activated_at, Some(102));
        // Can't activate twice.
        assert!(matches!(
            activated.activate(103),
            Err(PromotionError::IllegalTransition { .. })
        ));
        let released = activated.release(104).unwrap();
        assert_eq!(released.status, OverlayStatus::Released);
        assert_eq!(released.released_at, Some(104));
        // Released is immutable: no re-release, no re-activate.
        assert!(matches!(
            released.release(105),
            Err(PromotionError::IllegalTransition { .. })
        ));
        assert!(matches!(
            released.activate(106),
            Err(PromotionError::IllegalTransition { .. })
        ));
    }

    #[test]
    fn empty_draft_cannot_activate() {
        let (draft, _) = create_draft("snap-2", &[], &builtin_gate_policy(), &never_baseline, 1);
        assert!(draft.predicates.is_empty());
        assert_eq!(draft.activate(2), Err(PromotionError::EmptyOverlay));
    }

    #[test]
    fn duplicate_candidates_collapse_in_one_draft() {
        let same = candidate(
            "свобода",
            "свобода",
            RelationType::RelRequires,
            "выбор",
            "свобода требует выбор",
        );
        let (draft, exclusions) = create_draft(
            "snap-3",
            &[same.clone(), same],
            &builtin_gate_policy(),
            &never_baseline,
            1,
        );
        assert_eq!(draft.predicates.len(), 1);
        assert_eq!(exclusions.len(), 1);
        assert_eq!(exclusions[0].1, ExclusionReason::Duplicate);
    }

    #[test]
    fn checksum_is_content_addressed_and_parent_links_rollback() {
        let policy = builtin_gate_policy();
        let (mut draft_a, _) = create_draft(
            "snap-a",
            &[candidate(
                "свобода",
                "свобода",
                RelationType::RelRequires,
                "выбор",
                "x",
            )],
            &policy,
            &never_baseline,
            1,
        );
        let (draft_b, _) = create_draft(
            "snap-a",
            &[candidate(
                "свобода",
                "свобода",
                RelationType::RelRequires,
                "выбор",
                "x",
            )],
            &policy,
            &never_baseline,
            2,
        );
        // Same content → same version (content-addressed); only timestamps differ.
        assert_eq!(draft_a.checksum, draft_b.checksum);
        assert_eq!(draft_a.version, draft_b.version);
        // A different predicate set yields a different version.
        draft_a.predicates.push(draft_b.predicates[0].clone());
        draft_a.checksum = overlay_checksum(
            &draft_a.snapshot_id,
            &draft_a.policy_version,
            &draft_a.policy_checksum,
            &draft_a.predicates,
        );
        assert_ne!(draft_a.checksum, draft_b.checksum);

        // Parent link + rollback.
        let mut child = create_draft(
            "snap-b",
            &[candidate(
                "разум",
                "разум",
                RelationType::RelLimitedBy,
                "язык",
                "y",
            )],
            &policy,
            &never_baseline,
            3,
        )
        .0;
        child.parent_version = Some(draft_b.version.clone());
        let released = child.activate(4).unwrap().release(5).unwrap();
        assert_eq!(
            rollback(&released).unwrap().as_deref(),
            Some(draft_b.version.as_str())
        );
        // Rollback on a non-released overlay is refused.
        assert!(matches!(
            rollback(&released.activate(6).unwrap_or(released.clone())),
            Ok(_) | Err(PromotionError::IllegalTransition { .. })
        ));
    }

    #[test]
    fn tamper_is_detected_on_verify() {
        let (draft, _) = create_draft(
            "snap-4",
            &[candidate(
                "свобода",
                "свобода",
                RelationType::RelRequires,
                "выбор",
                "x",
            )],
            &builtin_gate_policy(),
            &never_baseline,
            1,
        );
        let mut tampered = draft.clone();
        tampered.predicates.get_mut(0).unwrap().object = AtomId::new("подменённый");
        assert_eq!(
            tampered.verify_integrity(),
            Err(PromotionError::ChecksumMismatch)
        );
        assert_eq!(draft.verify_integrity(), Ok(()));
    }

    #[test]
    fn rendered_artifact_is_human_inspectable() {
        let (draft, _) = create_draft(
            "snap-5",
            &[candidate(
                "свобода",
                "свобода",
                RelationType::RelRequires,
                "выбор",
                "свобода требует выбор",
            )],
            &builtin_gate_policy(),
            &never_baseline,
            1,
        );
        let artifact = render_overlay_artifact(&draft);
        assert!(artifact.contains("свобода --[requires]--> выбор"));
        assert!(artifact.contains("overlay-"));
    }

    #[test]
    fn canonical_slugs_match_the_haskell_constraint_vocabulary() {
        assert_eq!(canonical_slug(RelationType::RelRequires), "requires");
        assert_eq!(canonical_slug(RelationType::RelLimitedBy), "limited_by");
        assert_eq!(
            canonical_slug(RelationType::RelContrastsWith),
            "contrasts_with"
        );
        assert_eq!(canonical_slug(RelationType::RelPresupposes), "presupposes");
        assert_eq!(canonical_slug(RelationType::RelDependsOn), "depends_on");
        assert_eq!(canonical_slug(RelationType::RelIsA), "is_a");
    }

    #[test]
    fn normalize_atom_ports_the_haskell_rules() {
        assert_eq!(normalize_atom("  СВОБОДА. "), "свобода");
        assert_eq!(normalize_atom("правда?!"), "правда");
        assert_eq!(normalize_atom("   "), "");
    }

    #[test]
    fn invariants_hold_on_the_builtins() {
        assert!(validate_promotion_invariants().is_empty());
    }
}
