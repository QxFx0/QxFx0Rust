//! B2 verdict-procedure report (ADR-0044 acceptance gate, ADR-0043 U6.2
//! flip evidence): the exact procedure of
//! `examples/b2_ablation_probe.rs` as a deterministic library function.
//!
//! Two legs, both arms (`Enabled` vs `CommitDisabled`), all sessions
//! in-memory:
//! - corpus leg: every prompt in its own fresh session;
//! - long leg: the 8-line challenged свобода script, 8 rounds, one session
//!   per arm — the regime where commitment dynamics actually live.
//!
//! The aggregate shapes mirror the probe's printed lines one-to-one, so
//! the example stays a thin printer over this module and the flip
//! proposal (`qxfx0-codex::flip`) consumes the same numbers a human
//! re-running the probe would see. Pure: no database, no network, no
//! wall-clock — two runs over the same prompts are byte-identical.

use crate::{process_turn_with_options_and_trace, EssenceAblation, TurnInput, TurnOptions};
use qxfx0_types::system_state::SystemState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Schema tag of the B2 report.
pub const B2_REPORT_SCHEMA: &str = "qxfx0:b2-report:v1";
/// The hysteresis release window the probe measures against: mirrors the
/// v2 `EssenceModulation::default().violation_release_window`, locked by
/// the coupling test below. The flip rubric derives its bound from here
/// (`WINDOW - 1`), so a v2 retuning breaks loudly instead of silently
/// redefining the flip bar.
pub const B2_HYSTERESIS_RELEASE_WINDOW: usize = 8;

/// The challenged-script regime, shared with the probe example: 8 rounds
/// over 8 lines, one session per arm.
pub const B2_LONG_SCRIPT: [&str; 8] = [
    "что такое свобода?",
    "свобода это просто вседозволенность",
    "я не согласен: свобода требует осознанности",
    "свобода — это отсутствие ограничений, разве нет?",
    "я считаю, что свобода без ответственности невозможна",
    "но ведь произвол — тоже свобода?",
    "свобода для меня — это прежде всего выбор",
    "ты противоречишь себе: определись",
];
/// Rounds over the long script.
pub const B2_LONG_ROUNDS: usize = 8;

/// One arm of the corpus leg: every prompt in a fresh session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct B2ArmCorpus {
    pub prompts: usize,
    pub blocked: usize,
    pub advances: usize,
    pub committed: usize,
    pub sessions_with_commit: usize,
    pub suppressed: usize,
    pub violations: usize,
    /// Mean angst rounded to 4dp — stable across runs and encodings.
    pub mean_angst: f64,
}

/// One arm of the long challenged leg: one session, sustained dynamics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct B2ArmLong {
    pub turns: usize,
    pub v1_committed: bool,
    pub v2_committed: usize,
    pub suppressed: usize,
    pub violations: usize,
    pub max_run: usize,
    pub releases: usize,
    /// Mean angst rounded to 4dp.
    pub mean_angst: f64,
}

/// The full B2 verdict-procedure report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct B2Report {
    pub schema: String,
    /// Hex SHA-256 over the prompt list (LF-joined): pins the corpus the
    /// numbers were measured on.
    pub corpus_digest: String,
    pub prompts: usize,
    pub enabled_corpus: B2ArmCorpus,
    pub ablated_corpus: B2ArmCorpus,
    pub enabled_long: B2ArmLong,
    pub ablated_long: B2ArmLong,
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn corpus_digest(prompts: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prompts.join("\n").as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn run_corpus_arm(prompts: &[String], arm: &str, ablation: EssenceAblation) -> B2ArmCorpus {
    // M4: the B2 experiment compares against the V1 authority by
    // design — pin it, never follow the flipped default.
    let options = TurnOptions::new()
        .with_essence_v2_ablation(ablation)
        .with_subject_authority(crate::SubjectAuthority::V1Authority);
    let mut summary = B2ArmCorpus {
        prompts: prompts.len(),
        blocked: 0,
        advances: 0,
        committed: 0,
        sessions_with_commit: 0,
        suppressed: 0,
        violations: 0,
        mean_angst: 0.0,
    };
    let mut angst_sum = 0.0f64;
    for (index, prompt) in prompts.iter().enumerate() {
        let session_id = format!("b2-{arm}-{index}");
        let mut state = SystemState {
            session_id: session_id.clone(),
            ..SystemState::default()
        };
        let input = TurnInput {
            session_id,
            raw_text: prompt.clone(),
        };
        let (output, trace) = process_turn_with_options_and_trace(&input, &mut state, options);
        if output.blocked {
            summary.blocked += 1;
        }
        if let Some(advance) = trace.essence_advance {
            summary.advances += 1;
            angst_sum += advance.angst_level;
            if advance.committed.is_some() {
                summary.committed += 1;
                summary.sessions_with_commit += 1;
            }
            if advance.ablated_commit_suppressed {
                summary.suppressed += 1;
            }
            if advance.violation.is_some() {
                summary.violations += 1;
            }
        }
    }
    summary.mean_angst = round4(angst_sum / summary.advances.max(1) as f64);
    summary
}

fn run_long_arm(arm: &str, ablation: EssenceAblation) -> B2ArmLong {
    // M4: the B2 experiment compares against the V1 authority by
    // design — pin it, never follow the flipped default.
    let options = TurnOptions::new()
        .with_essence_v2_ablation(ablation)
        .with_subject_authority(crate::SubjectAuthority::V1Authority);
    let session_id = format!("b2-long-{arm}");
    let mut state = SystemState {
        session_id: session_id.clone(),
        ..SystemState::default()
    };
    let mut summary = B2ArmLong {
        turns: 0,
        v1_committed: false,
        v2_committed: 0,
        suppressed: 0,
        violations: 0,
        max_run: 0,
        releases: 0,
        mean_angst: 0.0,
    };
    let mut run = 0usize;
    let mut angst_sum = 0.0f64;
    for _ in 0..B2_LONG_ROUNDS {
        for prompt in &B2_LONG_SCRIPT {
            let input = TurnInput {
                session_id: session_id.clone(),
                raw_text: prompt.to_string(),
            };
            let (_, trace) = process_turn_with_options_and_trace(&input, &mut state, options);
            if let Some(advance) = trace.essence_advance {
                summary.turns += 1;
                angst_sum += advance.angst_level;
                if advance.committed.is_some() {
                    summary.v2_committed += 1;
                }
                if advance.ablated_commit_suppressed {
                    summary.suppressed += 1;
                }
                if advance.violation.is_some() {
                    summary.violations += 1;
                    run += 1;
                    summary.max_run = summary.max_run.max(run);
                } else {
                    run = 0;
                }
                if advance.released_commitment {
                    summary.releases += 1;
                }
            }
        }
    }
    summary.v1_committed = state.semantic.essence.commitment.is_some();
    summary.mean_angst = round4(angst_sum / summary.turns.max(1) as f64);
    summary
}

/// Parse a prompt TSV (same shape as `audited_v1_prompts.tsv`): first
/// tab-separated column, blank lines and `#` comments skipped.
pub fn parse_b2_prompts(tsv: &str) -> Vec<String> {
    tsv.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').next().unwrap().to_string())
        .collect()
}

/// Run the full B2 verdict procedure over `prompts` and report both arms.
/// Deterministic in `prompts`: same list, same report, byte-for-byte.
pub fn run_b2_report(prompts: &[String]) -> B2Report {
    B2Report {
        schema: B2_REPORT_SCHEMA.to_string(),
        corpus_digest: corpus_digest(prompts),
        prompts: prompts.len(),
        enabled_corpus: run_corpus_arm(prompts, "enabled", EssenceAblation::Enabled),
        ablated_corpus: run_corpus_arm(prompts, "ablated", EssenceAblation::CommitDisabled),
        enabled_long: run_long_arm("enabled", EssenceAblation::Enabled),
        ablated_long: run_long_arm("ablated", EssenceAblation::CommitDisabled),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn tiny_prompts() -> Vec<String> {
        vec![
            "что такое свобода?".to_string(),
            "что такое память?".to_string(),
            "что такое внимание?".to_string(),
        ]
    }

    /// One full verdict-procedure run shared by the structural tests — a
    /// run costs minutes in debug (64-turn long legs × 2 arms), so only
    /// the determinism test below pays for a second execution.
    fn shared_report() -> &'static B2Report {
        static REPORT: OnceLock<B2Report> = OnceLock::new();
        REPORT.get_or_init(|| run_b2_report(&tiny_prompts()))
    }

    #[test]
    fn report_is_deterministic_in_prompts() {
        let fresh = run_b2_report(&tiny_prompts());
        assert_eq!(&fresh, shared_report());
        let json_fresh = serde_json::to_string(&fresh).expect("report serializes");
        let json_shared = serde_json::to_string(shared_report()).expect("report serializes");
        assert_eq!(json_fresh, json_shared);
    }

    #[test]
    fn corpus_digest_pins_the_prompt_list() {
        let digest = corpus_digest(&tiny_prompts());
        let mut other = tiny_prompts();
        other.push("что такое время?".to_string());
        assert_ne!(digest, corpus_digest(&other));
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn prompt_parser_matches_the_probe_shape() {
        let tsv = "# comment\n\nпервый\tлишнее\nвторой\n";
        assert_eq!(
            parse_b2_prompts(tsv),
            vec!["первый".to_string(), "второй".to_string()]
        );
    }

    #[test]
    fn flip_window_stays_one_below_the_hysteresis_release() {
        // The U6.2 `b2-violations-bounded` rubric passes max_run <=
        // B2_HYSTERESIS_RELEASE_WINDOW - 1: a run reaching the release
        // window would have triggered the hysteresis backstop, leaving the
        // intermittent regime the ADR-0044 verdict analysis covers. This
        // test locks the coupling to the v2 default.
        assert_eq!(
            qxfx0_self_v2::EssenceModulation::default().violation_release_window,
            B2_HYSTERESIS_RELEASE_WINDOW
        );
    }

    #[test]
    fn tiny_corpus_runs_both_arms_without_divergence() {
        let report = shared_report();
        assert_eq!(report.prompts, 3);
        assert_eq!(report.enabled_corpus.prompts, 3);
        assert_eq!(report.ablated_corpus.prompts, 3);
        // The ablation switch touches nothing about the guard: blocks match.
        assert_eq!(report.enabled_corpus.blocked, report.ablated_corpus.blocked);
        // The long leg runs the full script; blocked turns record no
        // advance, so the count is capped by — not equal to — 64, and
        // identical across arms for the same reason as above.
        assert!(report.enabled_long.turns <= 64);
        assert_eq!(report.enabled_long.turns, report.ablated_long.turns);
    }
}
