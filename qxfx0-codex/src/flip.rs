//! Flip proposal (ADR-0043 U6.2): the machine-checkable evidence pack that
//! earns a v2-authority flip *proposal* — never the flip itself.
//!
//! A flip migrates Prepare/Finalize call sites behind the
//! `TurnOptions.essence_v2_ablation` switch and retires v1 modules
//! (ADR-0044); that migration is a human-reviewed code change, and human
//! release is permanent. This module only answers whether the evidence
//! suffices to put the proposal on the table, over two legs:
//!
//! - FELT leg (`felt-sustained`): at least `MIN_FLIP_SESSIONS` distinct
//!   sessions, each a sustained practice (`turns >= FELT_MIN_TURNS`) with
//!   a clean-verifying FELT export. Pass counts are recorded but not
//!   gating — a thin practice testifying «not proven» is honest
//!   measurement, and the flip bar is v2 dynamics, not practice volume.
//! - B2 leg (the ADR-0044 acceptance gate, re-run live by the draft): the
//!   enabled arm commits at least once, violation runs stay below the
//!   hysteresis release window, the ablated control suppresses without
//!   violating, and guard blocks match across arms (the switch touches
//!   nothing about the guard).
//!
//! `draft_flip_proposal` fails closed on any malformed FELT export. The
//! proposal embeds every input (session evidence + full B2 report), so
//! `verify_flip_proposal` re-evaluates the same pure core with no files,
//! no database, no re-run — a mismatch means the file was edited after
//! the draft.

use crate::felt::{verify_felt_export, FeltManifest, FELT_MIN_TURNS};
use qxfx0_pipeline::b2_report::{run_b2_report, B2Report, B2_HYSTERESIS_RELEASE_WINDOW};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Schema tag of the flip proposal.
pub const FLIP_PROPOSAL_SCHEMA: &str = "qxfx0:flip-proposal:v1";
/// A flip proposal needs sustained evidence across sessions — plural, not
/// a single good evening.
pub const MIN_FLIP_SESSIONS: usize = 5;
/// Violation runs must stay below the hysteresis release: one below the
/// window the B2 module mirrors from the v2 default.
pub const MAX_FLIP_VIOLATION_RUN: usize = B2_HYSTERESIS_RELEASE_WINDOW - 1;

/// One of the five flip rubrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlipRubric {
    FeltSustained,
    B2EnabledCommits,
    B2ViolationsBounded,
    B2AblationControl,
    B2GuardStable,
}

impl FlipRubric {
    pub fn name(self) -> &'static str {
        match self {
            FlipRubric::FeltSustained => "felt-sustained",
            FlipRubric::B2EnabledCommits => "b2-enabled-commits",
            FlipRubric::B2ViolationsBounded => "b2-violations-bounded",
            FlipRubric::B2AblationControl => "b2-ablation-control",
            FlipRubric::B2GuardStable => "b2-guard-stable",
        }
    }

    pub fn all() -> [FlipRubric; 5] {
        [
            FlipRubric::FeltSustained,
            FlipRubric::B2EnabledCommits,
            FlipRubric::B2ViolationsBounded,
            FlipRubric::B2AblationControl,
            FlipRubric::B2GuardStable,
        ]
    }
}

/// One FELT session backing the proposal: identity, weight and verdict,
/// plus the digest of the exact manifest bytes the verdict was read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlipSessionEvidence {
    pub session_id: String,
    pub turns: usize,
    pub verdict_passed: bool,
    /// Hex SHA-256 over the FELT manifest JSON — pins the export.
    pub manifest_digest: String,
}

/// One rubric's outcome, as embedded in the proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlipRubricOutcome {
    pub rubric: String,
    pub passed: bool,
    pub detail: String,
}

/// The flip proposal: every input embedded, every rubric recomputable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlipProposal {
    pub schema: String,
    pub sessions: Vec<FlipSessionEvidence>,
    pub felt_passes: usize,
    pub b2: B2Report,
    pub rubrics: Vec<FlipRubricOutcome>,
    pub ready: bool,
}

fn manifest_digest(manifest_json: &str) -> String {
    Sha256::digest(manifest_json.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn collect_session(
    markdown: &str,
    passphrase: Option<&str>,
) -> Result<FlipSessionEvidence, String> {
    let verification = verify_felt_export(markdown, passphrase);
    if !verification.verified() {
        return Err(verification
            .failure
            .unwrap_or_else(|| "неизвестная причина".into()));
    }
    let extracted = crate::extract_felt_blocks(markdown).expect("verified export extracts");
    let manifest: FeltManifest =
        serde_json::from_str(&extracted.manifest_json).expect("verified manifest parses");
    Ok(FlipSessionEvidence {
        session_id: manifest.session_id,
        turns: manifest.turns,
        verdict_passed: manifest.verdict_passed,
        manifest_digest: manifest_digest(&extracted.manifest_json),
    })
}

fn evaluate_rubrics(sessions: &[FlipSessionEvidence], b2: &B2Report) -> Vec<FlipRubricOutcome> {
    let distinct: BTreeSet<&str> = sessions
        .iter()
        .map(|session| session.session_id.as_str())
        .collect();
    let sustained = distinct.len() >= MIN_FLIP_SESSIONS
        && sessions
            .iter()
            .all(|session| session.turns >= FELT_MIN_TURNS);
    let mut outcomes = vec![FlipRubricOutcome {
        rubric: FlipRubric::FeltSustained.name().to_string(),
        passed: sustained,
        detail: format!(
            "сессий: {}, различных: {}, все sustained (≥{FELT_MIN_TURNS} ходов): {}",
            sessions.len(),
            distinct.len(),
            sessions
                .iter()
                .all(|session| session.turns >= FELT_MIN_TURNS)
        ),
    }];
    let long = &b2.enabled_long;
    outcomes.push(FlipRubricOutcome {
        rubric: FlipRubric::B2EnabledCommits.name().to_string(),
        passed: long.v2_committed >= 1,
        detail: format!("enabled-arm коммитов v2: {}", long.v2_committed),
    });
    outcomes.push(FlipRubricOutcome {
        rubric: FlipRubric::B2ViolationsBounded.name().to_string(),
        passed: long.max_run <= MAX_FLIP_VIOLATION_RUN,
        detail: format!(
            "max_run {}, порог {MAX_FLIP_VIOLATION_RUN}, релизов: {}",
            long.max_run, long.releases
        ),
    });
    let ablated = &b2.ablated_long;
    outcomes.push(FlipRubricOutcome {
        rubric: FlipRubric::B2AblationControl.name().to_string(),
        passed: ablated.suppressed >= 1 && ablated.violations == 0,
        detail: format!(
            "ablated подавлений: {}, нарушений: {}",
            ablated.suppressed, ablated.violations
        ),
    });
    outcomes.push(FlipRubricOutcome {
        rubric: FlipRubric::B2GuardStable.name().to_string(),
        passed: b2.enabled_corpus.blocked == b2.ablated_corpus.blocked,
        detail: format!(
            "блокировок enabled/ablated: {}/{}",
            b2.enabled_corpus.blocked, b2.ablated_corpus.blocked
        ),
    });
    outcomes
}

/// Draft a flip proposal: verify every FELT export, run the B2 verdict
/// procedure over `prompts`, evaluate the five rubrics. Fails closed on
/// the first malformed export. Minutes on a full corpus (the B2 long
/// legs); seconds on a handful of prompts.
pub fn draft_flip_proposal(
    felt_markdowns: &[&str],
    prompts: &[String],
    passphrase: Option<&str>,
) -> Result<FlipProposal, String> {
    let mut sessions = Vec::with_capacity(felt_markdowns.len());
    for markdown in felt_markdowns {
        sessions.push(collect_session(markdown, passphrase)?);
    }
    sessions.sort_by(|left, right| left.session_id.cmp(&right.session_id));
    let felt_passes = sessions
        .iter()
        .filter(|session| session.verdict_passed)
        .count();
    let b2 = run_b2_report(prompts);
    let rubrics = evaluate_rubrics(&sessions, &b2);
    let ready = rubrics.iter().all(|outcome| outcome.passed);
    Ok(FlipProposal {
        schema: FLIP_PROPOSAL_SCHEMA.to_string(),
        sessions,
        felt_passes,
        b2,
        rubrics,
        ready,
    })
}

/// Render a proposal as canonical JSON (what `flip-draft --out` writes).
pub fn render_flip_proposal(proposal: &FlipProposal) -> String {
    serde_json::to_string_pretty(proposal).expect("proposal serializes")
}

/// Outcome of a flip-proposal verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlipVerification {
    pub sessions: usize,
    pub ready: bool,
    /// `None` — verified; `Some(reason)` — the first failure found.
    pub failure: Option<String>,
}

impl FlipVerification {
    pub fn verified(&self) -> bool {
        self.failure.is_none()
    }
}

/// Verify a proposal file with no inputs: schema, rubric-table
/// recomputation over the embedded evidence. A mismatch means the file
/// was edited after the draft.
pub fn verify_flip_proposal(json: &str) -> FlipVerification {
    let failed = |sessions: usize, ready: bool, reason: String| FlipVerification {
        sessions,
        ready,
        failure: Some(reason),
    };
    let proposal: FlipProposal = match serde_json::from_str(json) {
        Ok(proposal) => proposal,
        Err(error) => {
            return failed(0, false, format!("предложение не читается: {error}"));
        }
    };
    if proposal.schema != FLIP_PROPOSAL_SCHEMA {
        return failed(
            proposal.sessions.len(),
            false,
            format!("неизвестная схема предложения: {}", proposal.schema),
        );
    }
    if proposal.b2.schema != qxfx0_pipeline::b2_report::B2_REPORT_SCHEMA {
        return failed(
            proposal.sessions.len(),
            false,
            format!("неизвестная схема B2-отчёта: {}", proposal.b2.schema),
        );
    }
    let recomputed = evaluate_rubrics(&proposal.sessions, &proposal.b2);
    let recorded: Vec<(&str, bool)> = proposal
        .rubrics
        .iter()
        .map(|outcome| (outcome.rubric.as_str(), outcome.passed))
        .collect();
    let expected: Vec<(&str, bool)> = recomputed
        .iter()
        .map(|outcome| (outcome.rubric.as_str(), outcome.passed))
        .collect();
    if recorded != expected {
        return failed(
            proposal.sessions.len(),
            proposal.ready,
            "таблица рубрик не совпадает с пересчётом — файл изменён после черновика".into(),
        );
    }
    let recomputed_ready = recomputed.iter().all(|outcome| outcome.passed);
    if recomputed_ready != proposal.ready {
        return failed(
            proposal.sessions.len(),
            recomputed_ready,
            "записанная готовность не совпадает с пересчётом — файл изменён после черновика".into(),
        );
    }
    let recomputed_passes = proposal
        .sessions
        .iter()
        .filter(|session| session.verdict_passed)
        .count();
    if recomputed_passes != proposal.felt_passes {
        return failed(
            proposal.sessions.len(),
            recomputed_ready,
            "счётчик FELT-прохождений не совпадает со списком сессий — файл изменён".into(),
        );
    }
    FlipVerification {
        sessions: proposal.sessions.len(),
        ready: recomputed_ready,
        failure: None,
    }
}

/// Compile-time sanity of the flip thresholds for `doctor`.
pub fn validate_flip_invariants() -> Vec<String> {
    let mut violations = Vec::new();
    if MIN_FLIP_SESSIONS < 2 {
        violations.push("MIN_FLIP_SESSIONS below two: a flip proposal over one session is not sustained evidence".into());
    }
    if MAX_FLIP_VIOLATION_RUN + 1 != B2_HYSTERESIS_RELEASE_WINDOW {
        violations
            .push("MAX_FLIP_VIOLATION_RUN drifted from one-below the B2 hysteresis window".into());
    }
    if !FLIP_PROPOSAL_SCHEMA.starts_with("qxfx0:") {
        violations.push("FLIP_PROPOSAL_SCHEMA lost its qxfx0: namespace".into());
    }
    if FlipRubric::all().len() != 5 {
        violations.push("the flip proposal is no longer five rubrics".into());
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::felt::build_felt_export;
    use qxfx0_pipeline::b2_report::{B2ArmCorpus, B2ArmLong, B2Report, B2_REPORT_SCHEMA};
    use qxfx0_pipeline::RendererAuthority;
    use qxfx0_types::system_state::{JournalRecord, SystemState};

    #[allow(clippy::field_reassign_with_default)]
    fn felt_markdown(session: &str, turns: usize) -> String {
        let mut state = SystemState::default();
        state.session_id = session.into();
        state.semantic.pack_set_fingerprint = "ab".repeat(32);
        state.dialogue.turn_count = turns;
        state.dialogue.journal = (1..=turns)
            .map(|turn| JournalRecord {
                turn,
                day: 20_000,
                topic: Some(
                    if turn % 2 == 0 {
                        "внимание"
                    } else {
                        "память"
                    }
                    .to_string(),
                ),
                input: format!("entry {turn}"),
                response: format!("response {turn}"),
                state_digest: "cd".repeat(32),
            })
            .collect();
        build_felt_export(&state, RendererAuthority::AuditedPlan).markdown
    }

    fn tiny_prompts() -> Vec<String> {
        vec!["что такое свобода?".to_string()]
    }

    /// One full draft shared by the end-to-end tests — the draft re-runs
    /// the B2 procedure, so only one test pays for the execution.
    fn shared_proposal() -> &'static FlipProposal {
        use std::sync::OnceLock;
        static PROPOSAL: OnceLock<FlipProposal> = OnceLock::new();
        PROPOSAL.get_or_init(|| {
            let markdowns: Vec<String> = (0..MIN_FLIP_SESSIONS)
                .map(|index| felt_markdown(&format!("s{index}"), 10))
                .collect();
            let refs: Vec<&str> = markdowns.iter().map(String::as_str).collect();
            draft_flip_proposal(&refs, &tiny_prompts(), None).expect("drafts")
        })
    }

    #[test]
    fn draft_runs_b2_and_evaluates_all_five_rubrics() {
        let proposal = shared_proposal();
        assert_eq!(proposal.rubrics.len(), 5);
        assert_eq!(proposal.sessions.len(), MIN_FLIP_SESSIONS);
        // Five sustained sessions: the FELT leg passes by construction.
        assert!(proposal.rubrics[0].passed, "{:?}", proposal.rubrics[0]);
        // The B2 legs report real measured dynamics (pass or fail, honestly).
        let json = render_flip_proposal(proposal);
        let verification = verify_flip_proposal(&json);
        assert!(verification.verified(), "{:?}", verification.failure);
        assert_eq!(verification.ready, proposal.ready);
    }

    #[test]
    fn thin_coverage_fails_the_sustained_rubric() {
        // One thin session cannot sustain a flip proposal, even against
        // healthy B2 dynamics.
        let sessions = vec![FlipSessionEvidence {
            session_id: "only".into(),
            turns: 2,
            verdict_passed: false,
            manifest_digest: "00".repeat(32),
        }];
        let rubrics = evaluate_rubrics(&sessions, &b2_fixture(2, 0, 5, 0, false));
        assert!(!rubrics[0].passed);
    }

    #[test]
    fn malformed_export_fails_the_draft_closed() {
        let result = draft_flip_proposal(&["не манифест"], &tiny_prompts(), None);
        assert!(result.is_err());
    }

    #[test]
    fn edited_proposal_fails_verification() {
        let proposal = shared_proposal();
        let mut json = render_flip_proposal(proposal);
        assert!(verify_flip_proposal(&json).verified());
        json = json.replacen(
            &format!("\"ready\": {}", proposal.ready),
            &format!("\"ready\": {}", !proposal.ready),
            1,
        );
        assert!(!verify_flip_proposal(&json).verified());
    }

    fn corpus_with_blocked(blocked: usize) -> B2ArmCorpus {
        B2ArmCorpus {
            prompts: 1,
            blocked,
            advances: 1,
            committed: 0,
            sessions_with_commit: 0,
            suppressed: 0,
            violations: 0,
            mean_angst: 0.05,
        }
    }

    fn long_fixture(
        v2_committed: usize,
        max_run: usize,
        suppressed: usize,
        violations: usize,
    ) -> B2ArmLong {
        B2ArmLong {
            turns: 64,
            v1_committed: true,
            v2_committed,
            suppressed,
            violations,
            max_run,
            releases: 0,
            mean_angst: 0.5,
        }
    }

    fn b2_fixture(
        v2_committed: usize,
        max_run: usize,
        suppressed: usize,
        violations: usize,
        blocked_split: bool,
    ) -> B2Report {
        B2Report {
            schema: B2_REPORT_SCHEMA.to_string(),
            corpus_digest: "00".repeat(32),
            prompts: 1,
            enabled_corpus: corpus_with_blocked(0),
            ablated_corpus: corpus_with_blocked(usize::from(blocked_split)),
            enabled_long: long_fixture(v2_committed, max_run, 0, violations.min(1)),
            ablated_long: long_fixture(0, 0, suppressed, violations),
        }
    }

    fn sustained_sessions() -> Vec<FlipSessionEvidence> {
        (0..MIN_FLIP_SESSIONS)
            .map(|index| FlipSessionEvidence {
                session_id: format!("s{index}"),
                turns: 10,
                verdict_passed: false,
                manifest_digest: "00".repeat(32),
            })
            .collect()
    }

    #[test]
    fn b2_fixtures_drive_each_rubric() {
        let sessions = sustained_sessions();
        // Healthy dynamics: every rubric passes.
        let healthy = evaluate_rubrics(&sessions, &b2_fixture(2, 3, 5, 0, false));
        assert!(healthy.iter().all(|outcome| outcome.passed), "{healthy:?}");
        // No v2 commit: only the commits rubric fails.
        let no_commit = evaluate_rubrics(&sessions, &b2_fixture(0, 0, 5, 0, false));
        assert!(!no_commit[1].passed);
        assert!(no_commit[2].passed && no_commit[3].passed && no_commit[4].passed);
        // Sustained violation run: only the bounded rubric fails.
        let runaway = evaluate_rubrics(
            &sessions,
            &b2_fixture(2, MAX_FLIP_VIOLATION_RUN + 1, 5, 4, false),
        );
        assert!(!runaway[2].passed);
        // Silent control: only the control rubric fails.
        let silent = evaluate_rubrics(&sessions, &b2_fixture(2, 0, 0, 0, false));
        assert!(!silent[3].passed);
        // Guard divergence: only the stability rubric fails.
        let unstable = evaluate_rubrics(&sessions, &b2_fixture(2, 0, 5, 0, true));
        assert!(!unstable[4].passed);
    }

    #[test]
    fn invariants_hold() {
        assert!(validate_flip_invariants().is_empty());
    }
}
