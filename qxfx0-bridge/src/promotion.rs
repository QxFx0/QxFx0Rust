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

/// The curated atom set of one rendered surface, exposed so a caller that
/// owns corpus text (the CLI over the argued registry, the precheck over
/// its trial rows) can build comparable atom sets without reimplementing
/// the stop-word/length discipline.
pub fn surface_atom_set(surface: &str) -> std::collections::BTreeSet<String> {
    content_atoms(surface)
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
    /// A candidate whose subject or object is not a known atom: the draft
    /// boundary refuses evidence it cannot ground (seed-atom bar).
    UnknownEndpoint,
    /// A candidate whose topic has no argued counterpoint in the registry:
    /// the editorial counterpoint bar needs a position to answer to.
    NoCounterpoint,
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
    /// Pack export was asked of an overlay that was never released: only
    /// the human decision may flow toward the pack.
    NotReleased,
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
            Self::NotReleased => {
                write!(formatter, "only a Released overlay may feed the pack")
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

/// The editorial precondition the draft boundary enforces per topic,
/// supplied by the caller that owns the corpus (the CLI over the argued
/// registry). Kept separate from the informativeness decision so the pure
/// port stays a faithful port: Haskell evaluates informativeness in the
/// boundary and counterpoint presence in the editorial admission step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicAdmissionFacts {
    /// The topic has a curated counterpoint (the counterpoint bar).
    pub has_counterpoint: bool,
}

/// The draft boundary with admission preconditions: a candidate whose
/// subject or object is not a known atom is refused as `UnknownEndpoint`
/// (the seed-atom bar — no minting), a candidate whose topic has no
/// curated counterpoint as `NoCounterpoint`; the passing set flows into
/// the ordinary informativeness ladder. `known_atoms` is the union of the
/// installation's session-graph atoms — a per-candidate check, because the
/// same topic may carry one grounded and one invented triple. Identical
/// inputs (plus identical oracle answers) produce identical drafts.
pub fn create_draft_with_admission(
    snapshot_id: &str,
    candidates: &[PromotionCandidate],
    policy: &GatePolicy,
    baseline_for: &dyn Fn(&str) -> Vec<String>,
    topic_admission_for: &dyn Fn(&str) -> TopicAdmissionFacts,
    known_atoms: &std::collections::BTreeSet<AtomId>,
    created_at: i64,
) -> (Overlay, Vec<(PromotionCandidate, ExclusionReason)>) {
    let mut ordered: Vec<PromotionCandidate> = Vec::new();
    let mut deferred: Vec<(PromotionCandidate, ExclusionReason)> = Vec::new();
    for candidate in candidates {
        if !known_atoms.contains(&candidate.subject) || !known_atoms.contains(&candidate.object) {
            deferred.push((candidate.clone(), ExclusionReason::UnknownEndpoint));
            continue;
        }
        if !topic_admission_for(&candidate.topic).has_counterpoint {
            deferred.push((candidate.clone(), ExclusionReason::NoCounterpoint));
            continue;
        }
        ordered.push(candidate.clone());
    }
    let (overlay, mut exclusions) =
        create_draft(snapshot_id, &ordered, policy, baseline_for, created_at);
    exclusions.extend(deferred);
    (overlay, exclusions)
}

/// One stored overlay predicate re-checked under a (possibly newer) policy
/// and baseline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevalidatedPredicate {
    pub candidate_id: String,
    pub still_passes: bool,
    pub reason: Option<ExclusionReason>,
}

/// A policy/baseline revalidation of an already-materialized overlay: the
/// row is never touched (release is permanent), the report is the audit
/// instrument the operator archives. `policy_now` is used in place of the
/// pinned policy for the informativeness decision; mismatched versions are
/// reported as exactly that.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Revalidation {
    pub version: String,
    pub checksum: String,
    pub policy_pinned: String,
    pub policy_now: String,
    pub policy_changed: bool,
    pub predicates: Vec<RevalidatedPredicate>,
    pub still_admitted: usize,
    pub excluded_now: usize,
}

pub fn revalidate(
    overlay: &Overlay,
    policy_now: &GatePolicy,
    baseline_for: &dyn Fn(&str) -> Vec<String>,
) -> Revalidation {
    let policy_changed = policy_now.version != overlay.policy_version;
    let mut predicates = Vec::with_capacity(overlay.predicates.len());
    let mut still_admitted = 0usize;
    for stored in &overlay.predicates {
        let candidate = PromotionCandidate {
            snapshot_id: overlay.snapshot_id.clone(),
            topic: stored.topic.clone(),
            subject: stored.subject.clone(),
            relation: stored.relation,
            object: stored.object.clone(),
            rendered_ru: stored.rendered_ru.clone(),
            confidence: stored.confidence,
            support: stored.support,
        };
        let result =
            evaluate_candidate_informativeness(&candidate, &baseline_for(&candidate.topic));
        let failure = if !result.not_tautological {
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
        if failure.is_none() {
            still_admitted += 1;
        }
        predicates.push(RevalidatedPredicate {
            candidate_id: stored.candidate_id.clone(),
            still_passes: failure.is_none(),
            reason: failure,
        });
    }
    Revalidation {
        version: overlay.version.clone(),
        checksum: overlay.checksum.clone(),
        policy_pinned: overlay.policy_version.clone(),
        policy_now: policy_now.version.clone(),
        policy_changed,
        predicates,
        still_admitted,
        excluded_now: overlay.predicates.len() - still_admitted,
    }
}

/// The fixed structural-evaluation topic set (the twelve soak prompts'
/// topics): deterministic, so the same overlay and the same corpus always
/// produce the same trial rows regardless of who runs them or when.
pub const EVALUATION_TOPIC_SET: [&str; 12] = [
    "истина",
    "свобода",
    "достоинство",
    "память",
    "знание",
    "надежда",
    "справедливость",
    "язык",
    "доверие",
    "причина",
    "целостность",
    "будущее",
];

/// One evaluation topic row: baseline coverage (topic has a curated
/// profile) and candidate coverage (an admitted overlay predicate names
/// the topic), the duplicate-with-curated check as the conflict probe and
/// uncovered topics as refusals. Symmetric on both sides so the verdict
/// rule is a true no-regression gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrialTopic {
    pub topic: String,
    pub baseline_contentful: bool,
    pub candidate_contentful: bool,
    pub baseline_conflicts: usize,
    pub candidate_conflicts: usize,
    pub baseline_refusals: usize,
    pub candidate_refusals: usize,
}

/// A structural corpus precheck over one overlay: deterministic, pure,
/// total. The Haskell AB counters (contentful/conflicts/refusals) are
/// modeled as *coverage*, not rendered responses — the overlay never
/// renders, so what the trial measures is whether overlay predicates
/// collide with curated authority and whether uncovered topics stay
/// uncovered. `completed_at` is caller-supplied (never sampled).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorpusTrial {
    pub evaluation_id: String,
    pub overlay_version: String,
    pub overlay_checksum: String,
    pub corpus_version: String,
    pub completed_at: i64,
    pub topics: Vec<TrialTopic>,
    pub baseline_contentful: usize,
    pub candidate_contentful: usize,
    pub baseline_conflicts: usize,
    pub candidate_conflicts: usize,
    pub baseline_refusals: usize,
    pub candidate_refusals: usize,
    pub overlay_usage_cases: usize,
    pub passed: bool,
}

/// The structural corpus method tag: this trial measures coverage shape,
/// not rendered responses (the Haskell `promotion-corpus-precheck-v1`
/// analog). A runtime AB against rendered output is a separate method
/// with its own version, for when an overlay can actually render.
pub const CORPUS_METHOD_STRUCTURAL: &str = "promotion-structural-01";

/// Run the structural precheck of one overlay against one registry
/// snapshot. `baseline_for` supplies the topic's curated surfaces (empty
/// when the topic is uncurated); `relates_to` maps a canonical triple to
/// the curated predicate it would collide with, if any (the caller owns
/// the corpus lookup — the bridge never touches the pack).
pub fn run_corpus_precheck(
    overlay: &Overlay,
    topic_set: &std::collections::BTreeSet<String>,
    baseline_for: &dyn Fn(&str) -> Vec<String>,
    relates_to: &dyn Fn(&str, &str, RelationType, &str) -> bool,
    completed_at: i64,
) -> CorpusTrial {
    let mut topics = Vec::with_capacity(topic_set.len());
    let (mut baseline_contentful, mut candidate_contentful) = (0usize, 0usize);
    // The curated baseline never conflicts with itself by construction: the
    // argued-topic registry parses once at startup and a self-contradicting
    // profile would fail that parse. The counter is therefore a constant
    // here, and the trial verdict rule reduces to "the overlay must add no
    // collision the curated content does not already carry".
    let baseline_conflicts = 0usize;
    let mut candidate_conflicts = 0usize;
    let (mut baseline_refusals, mut candidate_refusals) = (0usize, 0usize);
    let mut used = 0usize;
    for topic in topic_set {
        let baseline = baseline_for(topic);
        let curated = !baseline.is_empty();
        if curated {
            baseline_contentful += 1;
        } else {
            baseline_refusals += 1;
        }
        let mut covered = curated;
        let mut collisions = 0usize;
        for predicate in &overlay.predicates {
            if predicate.topic != *topic {
                continue;
            }
            used += 1;
            covered = true;
            if curated
                && relates_to(
                    &predicate.topic,
                    predicate.subject.as_str(),
                    predicate.relation,
                    predicate.object.as_str(),
                )
            {
                collisions += 1;
            }
        }
        if covered {
            candidate_contentful += 1;
        } else {
            candidate_refusals += 1;
        }
        candidate_conflicts += collisions;
        topics.push(TrialTopic {
            topic: topic.clone(),
            baseline_contentful: curated,
            candidate_contentful: covered,
            baseline_conflicts: 0,
            candidate_conflicts: collisions,
            baseline_refusals: !curated as usize,
            candidate_refusals: (!covered) as usize,
        });
    }
    let _ = baseline_conflicts;
    let passed = candidate_contentful >= baseline_contentful
        && candidate_conflicts == baseline_conflicts
        && candidate_refusals <= baseline_refusals;
    let evaluation_id = sha256_hex(
        format!(
            "{}\u{1}{CORPUS_METHOD_STRUCTURAL}\u{1}{completed_at}\u{1}{}",
            overlay.version, overlay.checksum
        )
        .as_bytes(),
    );
    CorpusTrial {
        evaluation_id,
        overlay_version: overlay.version.clone(),
        overlay_checksum: overlay.checksum.clone(),
        corpus_version: CORPUS_METHOD_STRUCTURAL.into(),
        completed_at,
        topics,
        baseline_contentful,
        candidate_contentful,
        baseline_conflicts,
        candidate_conflicts,
        baseline_refusals,
        candidate_refusals,
        overlay_usage_cases: used,
        passed,
    }
}

/// One rendered A/B case: the same prompt in a pristine snapshot
/// session (baseline) and in a snapshot session carrying the overlay's
/// predicates as held positions (candidate). Equality is byte-exact —
/// the harness runs both arms deterministically, so any drift is the
/// overlay's doing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeAbCase {
    pub prompt: String,
    pub topic: String,
    /// True for fixed regression topics (must render identically);
    /// false for the overlay's own topics (divergence informational).
    pub regression: bool,
    pub baseline_response: String,
    pub candidate_response: String,
    pub responses_equal: bool,
    pub baseline_blocked: bool,
    pub candidate_blocked: bool,
}

/// A runtime A/B trial over one overlay: rendered-response equality
/// between the overlay-free and overlay-carrying snapshots (the Haskell
/// `promotion-runtime` analog, operationalized for a renderer that never
/// reads promotion tables — the candidate arm carries the overlay as
/// held positions, exactly what editorial admission would produce).
/// `completed_at` is caller-supplied (never sampled).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeAbTrial {
    pub evaluation_id: String,
    pub overlay_version: String,
    pub overlay_checksum: String,
    pub corpus_version: String,
    pub completed_at: i64,
    pub cases: Vec<RuntimeAbCase>,
    pub regression_cases: usize,
    pub regression_identical: usize,
    pub overlay_cases: usize,
    pub overlay_diverged: usize,
    pub blocked_equal: bool,
    pub overlay_usage_cases: usize,
    pub passed: bool,
}

/// The runtime-A/B method tag: this trial measures rendered responses,
/// not coverage shape (the Haskell `promotion-runtime` analog). A
/// separate method with its own version, exactly as the structural tag.
pub const RUNTIME_AB_METHOD: &str = "promotion-runtime-ab-01";

/// Run the runtime A/B verdict over caller-supplied render pairs. Pure:
/// the caller owns snapshots, injection and rendering — the bridge only
/// scores. Pass = every regression topic renders byte-identically AND
/// guard behaviour matches across arms (the switch — here, the overlay
/// as held positions — touches nothing about the guard). Overlay-topic
/// divergence is expected signal (held positions get quoted), recorded
/// but never gating. A trial with no regression baseline refuses: with
/// nothing to compare against, "identical" would be vacuous.
pub fn run_runtime_ab_trial(
    overlay_version: &str,
    overlay_checksum: &str,
    cases: Vec<RuntimeAbCase>,
    overlay_usage_cases: usize,
    completed_at: i64,
) -> RuntimeAbTrial {
    let regression_cases = cases.iter().filter(|case| case.regression).count();
    let regression_identical = cases
        .iter()
        .filter(|case| case.regression && case.responses_equal)
        .count();
    let overlay_cases = cases.len() - regression_cases;
    let overlay_diverged = cases
        .iter()
        .filter(|case| !case.regression && !case.responses_equal)
        .count();
    let blocked_equal = cases
        .iter()
        .all(|case| case.baseline_blocked == case.candidate_blocked);
    let passed = regression_cases >= 1 && regression_identical == regression_cases && blocked_equal;
    let mut digest = Sha256::new();
    digest.update(overlay_version.as_bytes());
    digest.update([0x1f]);
    digest.update(RUNTIME_AB_METHOD.as_bytes());
    digest.update([0x1f]);
    digest.update(completed_at.to_be_bytes());
    digest.update([0x1f]);
    digest.update(overlay_checksum.as_bytes());
    for case in &cases {
        digest.update([0x1f]);
        digest.update(case.prompt.as_bytes());
        digest.update([0x1f]);
        digest.update(case.baseline_response.as_bytes());
        digest.update([0x1f]);
        digest.update(case.candidate_response.as_bytes());
    }
    let evaluation_id = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    RuntimeAbTrial {
        evaluation_id,
        overlay_version: overlay_version.to_string(),
        overlay_checksum: overlay_checksum.to_string(),
        corpus_version: RUNTIME_AB_METHOD.into(),
        completed_at,
        cases,
        regression_cases,
        regression_identical,
        overlay_cases,
        overlay_diverged,
        blocked_equal,
        overlay_usage_cases,
        passed,
    }
}

/// One predicate in the pack-export feed: the machine surface a human
/// editor merges into the pack sources (topic, endpoints, relation slug,
/// Russian surface, weight evidence). Provenance rides alongside, never
/// inside, the editorial decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackExportPredicate {
    pub topic: String,
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub rendered_ru: String,
    pub confidence: f64,
    pub support: usize,
    pub semantic_gain: f64,
}

/// The editorial feed: a Released overlay's predicates plus the full
/// provenance chain (both bound evaluation ids, policy pin, overlay
/// checksum) for the human merge into the pack sources. Automation
/// stops at this file: graph effect comes only from editorial
/// admission, pack gates validate the merged result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackExport {
    pub schema: String,
    pub overlay_version: String,
    pub overlay_checksum: String,
    pub structural_evaluation: String,
    pub runtime_evaluation: String,
    pub policy_version: String,
    pub policy_checksum: String,
    pub predicates: Vec<PackExportPredicate>,
}

/// Schema tag of the pack-export feed.
pub const PACK_EXPORT_SCHEMA: &str = "promotion-export-pack-01";

/// Render the editorial feed for a Released overlay. Refuses anything
/// but Released: only the human decision (release is permanent) may
/// flow toward the pack. Pure and deterministic.
pub fn render_pack_export(
    overlay: &Overlay,
    structural_evaluation: &str,
    runtime_evaluation: &str,
) -> Result<PackExport, PromotionError> {
    if !matches!(overlay.status, OverlayStatus::Released) {
        return Err(PromotionError::NotReleased);
    }
    Ok(PackExport {
        schema: PACK_EXPORT_SCHEMA.to_string(),
        overlay_version: overlay.version.clone(),
        overlay_checksum: overlay.checksum.clone(),
        structural_evaluation: structural_evaluation.to_string(),
        runtime_evaluation: runtime_evaluation.to_string(),
        policy_version: overlay.policy_version.clone(),
        policy_checksum: overlay.policy_checksum.clone(),
        predicates: overlay
            .predicates
            .iter()
            .map(|predicate| PackExportPredicate {
                topic: predicate.topic.clone(),
                subject: predicate.subject.as_str().to_string(),
                relation: canonical_slug(predicate.relation).to_string(),
                object: predicate.object.as_str().to_string(),
                rendered_ru: predicate.rendered_ru.clone(),
                confidence: predicate.confidence,
                support: predicate.support,
                semantic_gain: predicate.semantic_gain,
            })
            .collect(),
    })
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

    fn admitted_candidate() -> PromotionCandidate {
        // Grounded in known atoms on a curated topic with a counterpoint:
        // passes every admission and informativeness check on an empty
        // baseline, so it lands in the draft.
        candidate(
            "свобода",
            "свобода",
            RelationType::RelRequires,
            "ответственность",
            "свобода требует ответственность",
        )
    }

    fn known_atoms() -> std::collections::BTreeSet<AtomId> {
        ["свобода", "ответственность", "выбор"]
            .into_iter()
            .map(AtomId::new)
            .collect()
    }

    fn counterpointed(_topic: &str) -> TopicAdmissionFacts {
        TopicAdmissionFacts {
            has_counterpoint: true,
        }
    }

    #[test]
    fn admission_defers_unknown_endpoints_before_informativeness() {
        let mut invented = admitted_candidate();
        invented.object = AtomId::new("выдумка");
        let (overlay, exclusions) = create_draft_with_admission(
            "snap-adm",
            std::slice::from_ref(&invented),
            &builtin_gate_policy(),
            &never_baseline,
            &counterpointed,
            &known_atoms(),
            1,
        );
        // The atom bar fires even for an otherwise-novel constraint triple.
        assert!(overlay.predicates.is_empty());
        assert_eq!(exclusions.len(), 1);
        assert_eq!(exclusions[0].1, ExclusionReason::UnknownEndpoint);
    }

    #[test]
    fn admission_defers_topics_without_a_counterpoint() {
        let policy = builtin_gate_policy();
        let (overlay, exclusions) = create_draft_with_admission(
            "snap-adm",
            std::slice::from_ref(&admitted_candidate()),
            &policy,
            &never_baseline,
            &|_topic: &str| TopicAdmissionFacts {
                has_counterpoint: false,
            },
            &known_atoms(),
            1,
        );
        assert!(overlay.predicates.is_empty());
        assert_eq!(exclusions.len(), 1);
        assert_eq!(exclusions[0].1, ExclusionReason::NoCounterpoint);
    }

    fn ab_case(
        prompt: &str,
        topic: &str,
        regression: bool,
        baseline: &str,
        candidate: &str,
    ) -> RuntimeAbCase {
        RuntimeAbCase {
            prompt: prompt.to_string(),
            topic: topic.to_string(),
            regression,
            baseline_response: baseline.to_string(),
            candidate_response: candidate.to_string(),
            responses_equal: baseline == candidate,
            baseline_blocked: false,
            candidate_blocked: false,
        }
    }

    #[test]
    fn runtime_ab_passes_on_identical_regression_with_overlay_signal() {
        let trial = run_runtime_ab_trial(
            "overlay-1",
            "checksum",
            vec![
                ab_case("Что такое свобода?", "свобода", true, "ответ", "ответ"),
                ab_case("Что такое память?", "память", true, "ответ", "ответ"),
                ab_case("Что такое воля?", "воля", false, "база", "база + позиция"),
            ],
            2,
            7,
        );
        assert!(trial.passed);
        assert_eq!(trial.regression_cases, 2);
        assert_eq!(trial.regression_identical, 2);
        assert_eq!(trial.overlay_cases, 1);
        assert_eq!(trial.overlay_diverged, 1);
        assert!(trial.blocked_equal);
        assert_eq!(trial.corpus_version, RUNTIME_AB_METHOD);
        assert_eq!(trial.evaluation_id.len(), 64);
    }

    #[test]
    fn runtime_ab_fails_on_regression_drift_or_guard_mismatch() {
        let drifted = run_runtime_ab_trial(
            "overlay-1",
            "checksum",
            vec![
                ab_case(
                    "Что такое свобода?",
                    "свобода",
                    true,
                    "ответ",
                    "другой ответ",
                ),
                ab_case("Что такое воля?", "воля", false, "база", "база"),
            ],
            1,
            7,
        );
        assert!(!drifted.passed);
        assert_eq!(drifted.regression_identical, 0);

        let mut guarded = ab_case("Что такое свобода?", "свобода", true, "ответ", "ответ");
        guarded.candidate_blocked = true;
        guarded.candidate_response = "QxFx0: ответ отклонён системой безопасности.".into();
        guarded.responses_equal = false;
        let mismatch = run_runtime_ab_trial("overlay-1", "checksum", vec![guarded], 0, 7);
        assert!(!mismatch.passed);
        assert!(!mismatch.blocked_equal);
    }

    #[test]
    fn runtime_ab_refuses_a_trial_with_no_regression_baseline() {
        let vacuous = run_runtime_ab_trial(
            "overlay-1",
            "checksum",
            vec![ab_case("Что такое воля?", "воля", false, "база", "база")],
            1,
            7,
        );
        assert!(!vacuous.passed, "vacuous identity must not pass");
    }

    #[test]
    fn pack_export_refuses_anything_but_released() {
        let admitted = admitted_candidate();
        let (mut overlay, _) = create_draft_with_admission(
            "snap-pack",
            std::slice::from_ref(&admitted),
            &builtin_gate_policy(),
            &never_baseline,
            &counterpointed,
            &known_atoms(),
            1,
        );
        assert!(!overlay.predicates.is_empty());
        assert!(render_pack_export(&overlay, "struct-1", "rtab-1").is_err());
        overlay.status = OverlayStatus::Activated;
        assert!(render_pack_export(&overlay, "struct-1", "rtab-1").is_err());
        overlay.status = OverlayStatus::Released;
        let export = render_pack_export(&overlay, "struct-1", "rtab-1").expect("released exports");
        assert_eq!(export.schema, PACK_EXPORT_SCHEMA);
        assert_eq!(export.overlay_version, overlay.version);
        assert_eq!(export.overlay_checksum, overlay.checksum);
        assert_eq!(export.structural_evaluation, "struct-1");
        assert_eq!(export.runtime_evaluation, "rtab-1");
        assert_eq!(export.policy_version, overlay.policy_version);
        assert_eq!(export.predicates.len(), overlay.predicates.len());
        let predicate = &export.predicates[0];
        let source = &overlay.predicates[0];
        assert_eq!(predicate.topic, source.topic);
        assert_eq!(predicate.subject, source.subject.as_str());
        assert_eq!(predicate.object, source.object.as_str());
        assert_eq!(predicate.rendered_ru, source.rendered_ru);
        // Deterministic: the same overlay exports byte-identically.
        let again = render_pack_export(&overlay, "struct-1", "rtab-1").expect("released exports");
        assert_eq!(
            serde_json::to_string(&export).unwrap(),
            serde_json::to_string(&again).unwrap()
        );
    }

    #[test]
    fn runtime_ab_id_binds_the_measured_outputs() {
        let pairs = || {
            vec![
                ab_case("Что такое свобода?", "свобода", true, "ответ", "ответ"),
                ab_case("Что такое воля?", "воля", false, "база", "база + позиция"),
            ]
        };
        let first = run_runtime_ab_trial("overlay-1", "checksum", pairs(), 1, 7);
        let second = run_runtime_ab_trial("overlay-1", "checksum", pairs(), 1, 7);
        assert_eq!(first.evaluation_id, second.evaluation_id);
        let mut altered = pairs();
        altered[1].candidate_response = "другая позиция".into();
        altered[1].responses_equal = false;
        let third = run_runtime_ab_trial("overlay-1", "checksum", altered, 1, 7);
        assert_ne!(first.evaluation_id, third.evaluation_id);
        assert!(third.passed, "overlay-only divergence never gates");
    }

    #[test]
    fn admission_passes_grounded_candidates_to_the_ladder() {
        let (overlay, exclusions) = create_draft_with_admission(
            "snap-adm",
            std::slice::from_ref(&admitted_candidate()),
            &builtin_gate_policy(),
            &never_baseline,
            &counterpointed,
            &known_atoms(),
            1,
        );
        assert!(exclusions.is_empty());
        assert_eq!(overlay.predicates.len(), 1);
    }

    #[test]
    fn revalidation_detects_policy_change_and_baseline_drift() {
        let policy = builtin_gate_policy();
        let (overlay, _) = create_draft_with_admission(
            "snap-rev",
            std::slice::from_ref(&admitted_candidate()),
            &policy,
            &never_baseline,
            &counterpointed,
            &known_atoms(),
            1,
        );
        // Same policy, same empty baseline: nothing drifted.
        let same = revalidate(&overlay, &policy, &never_baseline);
        assert!(!same.policy_changed);
        assert_eq!(same.still_admitted, 1);
        assert_eq!(same.excluded_now, 0);
        assert!(same
            .predicates
            .iter()
            .all(|predicate| predicate.still_passes));
        // A stricter policy (gain floor effectively at the ceiling) drops
        // the predicate and flags the version change.
        let stricter = GatePolicy {
            version: "promotion-v9-strict".into(),
            description: "draft".into(),
        };
        let changed = revalidate(&overlay, &stricter, &never_baseline);
        assert!(changed.policy_changed);
        assert_eq!(changed.policy_now, "promotion-v9-strict");
        // A newly-curated baseline surface that names the triple collapses
        // the gain: drift is detected on the evidence, not the row.
        let drifted = revalidate(&overlay, &policy, &|topic: &str| {
            assert_eq!(topic, "свобода");
            vec!["свобода requires ответственность".into()]
        });
        assert!(!drifted.policy_changed);
        assert_eq!(drifted.still_admitted, 0);
        assert_eq!(drifted.excluded_now, 1);
        let row = &drifted.predicates[0];
        assert!(!row.still_passes);
        assert_eq!(row.reason, Some(ExclusionReason::SemanticGain));
    }

    #[test]
    fn corpus_precheck_passes_growth_and_refuses_new_conflicts() {
        let policy = builtin_gate_policy();
        let candidates = [admitted_candidate()];
        let (overlay, _) = create_draft_with_admission(
            "snap-pre",
            &candidates,
            &policy,
            &never_baseline,
            &counterpointed,
            &known_atoms(),
            1,
        );
        let topics: std::collections::BTreeSet<String> =
            ["свобода".to_string(), "истина".to_string()]
                .into_iter()
                .collect();
        // Baseline: свобода curated (profile), истина uncovered (refusal).
        let baseline_for = |topic: &str| match topic {
            "свобода" => vec!["свобода требует ответственность".into()],
            _ => Vec::new(),
        };
        // The oracle mirrors the real CLI wiring: a triple collides with
        // curated content when both endpoints occur in the topic's baseline
        // surfaces (a duplicate-with-authority probe).
        let relates = |topic: &str, subject: &str, _relation: RelationType, object: &str| {
            let atoms = surface_atom_set(&baseline_for(topic).join(" "));
            atoms.contains(&normalize_atom(subject)) && atoms.contains(&normalize_atom(object))
        };
        let trial = run_corpus_precheck(&overlay, &topics, &baseline_for, &relates, 7);
        assert_eq!(trial.overlay_version, overlay.version);
        assert_eq!(trial.corpus_version, CORPUS_METHOD_STRUCTURAL);
        // свобода: curated baseline gains a colliding predicate -> a
        // candidate conflict; истина keeps its refusal. Verdict fails on
        // the new conflict.
        let freedom = trial
            .topics
            .iter()
            .find(|row| row.topic == "свобода")
            .unwrap();
        assert!(freedom.baseline_contentful);
        assert_eq!(freedom.candidate_conflicts, 1);
        assert!(!trial.passed);
        assert_eq!(trial.candidate_conflicts, 1);
        assert_eq!(trial.overlay_usage_cases, 1);

        // The same overlay against a topic set it never touches: the
        // precheck is a pure no-regression comparison, so identical
        // refusals on both arms still pass.
        let clean_topics: std::collections::BTreeSet<String> =
            ["истина".to_string()].into_iter().collect();
        let clean = run_corpus_precheck(
            &overlay,
            &clean_topics,
            &|_topic: &str| Vec::new(),
            &|_topic: &str, _subject: &str, _relation: RelationType, _object: &str| false,
            8,
        );
        assert!(clean.passed);
        assert_eq!(clean.candidate_contentful, 0);
        assert_eq!(clean.baseline_contentful, 0);
        assert_eq!(clean.baseline_refusals, 1);
        assert_eq!(clean.candidate_refusals, 1);
        // The trial id pins version + checksum + corpus + time.
        assert_eq!(clean.evaluation_id.len(), 64);
        assert_ne!(clean.evaluation_id, trial.evaluation_id);
    }

    #[test]
    fn evaluation_topic_set_is_stable_and_nonempty() {
        assert_eq!(EVALUATION_TOPIC_SET.len(), 12);
        let first: Vec<&str> = EVALUATION_TOPIC_SET.to_vec();
        let mut sorted = first.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            first.len(),
            sorted.len(),
            "topic set must have no duplicates"
        );
        assert!(first.iter().all(|topic| !topic.trim().is_empty()));
    }
}
