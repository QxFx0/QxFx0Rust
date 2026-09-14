//! Dual journal (ADR-0043 U6 «Свидетельства»): the practitioner's turns
//! zipped with the subject's per-turn positions.
//!
//! The diary manifest already carries the practitioner's side (input,
//! response, replay digest per turn). This module adds the symmetric half:
//! what the subject layer recorded on the same turn — V1 essence witness
//! attributes (salience driver, reconcile rule, agreement, divergence,
//! conatus), whether a live essence commitment covered the turn, and whether
//! the turn collapsed into an essence reset. Plus a session-level flag for
//! V2-shadow presence (`essence_v2.is_some()`; decoding V2 stays with the
//! pipeline, the journal only testifies that the shadow was on).
//!
//! Two honest boundaries, documented where they bite:
//! - routing family per turn is a routing-time projection, not a persisted
//!   per-turn position — only `last_turn_decision` survives — so the journal
//!   does not fabricate it. The persisted subject position is the witness.
//! - sessions that predate journal recording have turns with no journal
//!   record; those turns are flagged (`journaled: false`), not filled in.

use qxfx0_types::system_state::SystemState;
use serde::{Deserialize, Serialize};

/// The subject's persisted position on one turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectPosition {
    /// A V1 essence witness was recorded for this turn.
    pub witnessed: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub salience_driver: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reconcile_rule: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub agreement: String,
    #[serde(default)]
    pub divergence: f64,
    #[serde(default)]
    pub conatus: f64,
    /// A live essence commitment covered this turn
    /// (`committed_at <= turn`).
    pub committed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commitment_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commitment_trigger: Option<String>,
    /// The turn collapsed into an essence reset (Anomaly-3 record).
    pub reset_on_turn: bool,
}

/// The practitioner's persisted position on one turn. Input/response text
/// stays with the diary manifest; the dual journal only carries the
/// gate-relevant flags plus the input frame's observational facets
/// (polarity, agent/target — evidence, never routing), so the two
/// artifacts cross-reference by turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PractitionerPosition {
    pub topic: Option<String>,
    /// A journal record exists for this turn (false for sessions that
    /// predate journal recording).
    pub journaled: bool,
    pub response_empty: bool,
    /// The response is the guard-recovery surface.
    pub recovery: bool,
    /// Input polarity (`affirmative` / `negative`); `affirmative` for
    /// unjournaled turns (no input to read).
    #[serde(default = "default_polarity")]
    pub polarity: String,
    /// The input carries a first-person marker (`я`).
    #[serde(default)]
    pub agent: bool,
    /// The input addresses the system (`ты` / `вы`).
    #[serde(default)]
    pub target: bool,
}

/// Polarity label for unjournaled turns and pre-facet exports.
pub fn default_polarity() -> String {
    "affirmative".to_string()
}

/// One zipped turn: both halves of the practice, side by side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DualTurn {
    /// 1-based turn number.
    pub turn: usize,
    pub practitioner: PractitionerPosition,
    pub subject: SubjectPosition,
}

/// The full dual journal: a pure function of the persisted state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DualJournal {
    pub turns: Vec<DualTurn>,
    /// The V2 shadow trajectory was present when the evidence was built.
    pub essence_v2_shadow: bool,
}

/// Build one practitioner's position, reading the input frame's
/// observational facets (polarity, agent/target) from the stored input
/// text. Unjournaled turns take the defaults — flagged, never faked.
fn practitioner_position(
    record: Option<&qxfx0_types::system_state::JournalRecord>,
) -> PractitionerPosition {
    let Some(record) = record else {
        return PractitionerPosition {
            topic: None,
            journaled: false,
            response_empty: true,
            recovery: false,
            polarity: default_polarity(),
            agent: false,
            target: false,
        };
    };
    let frame = qxfx0_semantic::input_frame::frame_input(&record.input);
    PractitionerPosition {
        topic: record.topic.clone(),
        journaled: true,
        response_empty: record.response.trim().is_empty(),
        recovery: record.response.trim() == crate::felt::RECOVERY_RESPONSE,
        polarity: match frame.polarity {
            qxfx0_semantic::input_frame::Polarity::Affirmative => "affirmative".into(),
            qxfx0_semantic::input_frame::Polarity::Negative => "negative".into(),
        },
        agent: frame.agent.is_some(),
        target: frame.target.is_some(),
    }
}

impl DualJournal {
    /// Build the dual journal. Total — never fails: turns with no evidence
    /// on either side are flagged, not rejected. Interpreting the flags is
    /// the FELT gate's job.
    pub fn build(state: &SystemState) -> Self {
        let essence = &state.semantic.essence;
        let mut turns = Vec::with_capacity(state.dialogue.turn_count);
        for turn in 1..=state.dialogue.turn_count {
            let record = state
                .dialogue
                .journal
                .iter()
                .find(|entry| entry.turn == turn);
            let witness = essence
                .witnesses
                .iter()
                .find(|witness| witness.turn == turn);
            let (committed, commitment_mode, commitment_trigger) = match essence.commitment.as_ref()
            {
                Some(commitment) if commitment.committed_at <= turn => (
                    true,
                    Some(format!("{:?}", commitment.mode)),
                    Some(format!("{:?}", commitment.trigger)),
                ),
                _ => (false, None, None),
            };
            turns.push(DualTurn {
                turn,
                practitioner: practitioner_position(record),
                subject: SubjectPosition {
                    witnessed: witness.is_some(),
                    salience_driver: witness
                        .map(|entry| entry.salience_driver.clone())
                        .unwrap_or_default(),
                    reconcile_rule: witness
                        .map(|entry| entry.reconcile_rule.clone())
                        .unwrap_or_default(),
                    agreement: witness
                        .map(|entry| entry.agreement.clone())
                        .unwrap_or_default(),
                    divergence: witness.map(|entry| entry.divergence).unwrap_or(0.0),
                    conatus: witness.map(|entry| entry.conatus_scalar).unwrap_or(0.0),
                    committed,
                    commitment_mode,
                    commitment_trigger,
                    reset_on_turn: essence.reset_events.iter().any(|event| event.turn == turn),
                },
            });
        }
        DualJournal {
            turns,
            essence_v2_shadow: state.semantic.essence_v2.is_some(),
        }
    }

    /// Turns with no journal record — the pre-journal gap. Empty for every
    /// session recorded by the current version.
    pub fn unjournaled_turns(&self) -> Vec<usize> {
        self.turns
            .iter()
            .filter(|turn| !turn.practitioner.journaled)
            .map(|turn| turn.turn)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::system_state::{
        CommitmentMode, CommitmentTrigger, EssenceCommitment, EssenceWitness, JournalRecord,
    };

    fn witness_on(turn: usize) -> EssenceWitness {
        EssenceWitness {
            turn,
            mode: "witnessing".into(),
            statement: format!("position held on turn {turn}"),
            salience_driver: "novelty".into(),
            reconcile_rule: "R3".into(),
            agreement: "high".into(),
            divergence: 0.1,
            conatus_scalar: 0.8,
        }
    }

    fn journal_on(turn: usize, topic: Option<&str>) -> JournalRecord {
        JournalRecord {
            turn,
            day: 20_000,
            topic: topic.map(str::to_string),
            input: format!("entry {turn}"),
            response: format!("response {turn}"),
            state_digest: "ab".repeat(32),
            subject_authority: qxfx0_types::system_state::default_subject_authority(),
        }
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn zips_witnesses_commitment_and_journal_by_turn() {
        let mut state = SystemState::default();
        state.session_id = "dual".into();
        state.dialogue.turn_count = 3;
        state.dialogue.journal = vec![
            journal_on(1, Some("память")),
            journal_on(2, Some("внимание")),
            journal_on(3, Some("память")),
        ];
        state.semantic.essence.witnesses = vec![witness_on(1), witness_on(3)];
        state.semantic.essence.commitment = Some(EssenceCommitment {
            mode: CommitmentMode::Witnessing,
            trigger: CommitmentTrigger::TriggerAngstThreshold,
            committed_at: 2,
            witness_hash: "hash".into(),
        });
        let journal = DualJournal::build(&state);
        assert_eq!(journal.turns.len(), 3);
        assert!(journal.turns[0].subject.witnessed);
        assert!(!journal.turns[0].subject.committed);
        assert!(!journal.turns[1].subject.witnessed);
        assert!(journal.turns[1].subject.committed);
        assert_eq!(
            journal.turns[1].subject.commitment_mode.as_deref(),
            Some("Witnessing")
        );
        assert_eq!(
            journal.turns[1].subject.commitment_trigger.as_deref(),
            Some("TriggerAngstThreshold")
        );
        assert!(journal.unjournaled_turns().is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn flags_pre_journal_gaps_instead_of_filling_them() {
        let mut state = SystemState::default();
        state.session_id = "old".into();
        state.dialogue.turn_count = 2;
        state.dialogue.journal = vec![journal_on(2, None)];
        let journal = DualJournal::build(&state);
        assert_eq!(journal.unjournaled_turns(), vec![1]);
        assert!(journal.turns[0].practitioner.response_empty);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn build_is_deterministic() {
        let mut state = SystemState::default();
        state.session_id = "det".into();
        state.dialogue.turn_count = 1;
        state.dialogue.journal = vec![journal_on(1, Some("память"))];
        state.semantic.essence.witnesses = vec![witness_on(1)];
        assert_eq!(DualJournal::build(&state), DualJournal::build(&state));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn practitioner_facets_come_from_the_stored_input() {
        let mut record = journal_on(1, Some("память"));
        record.input = "я не согласен про память, ты ошибаешься".into();
        let mut state = SystemState::default();
        state.session_id = "facets".into();
        state.dialogue.turn_count = 1;
        state.dialogue.journal = vec![record];
        let journal = DualJournal::build(&state);
        let practitioner = &journal.turns[0].practitioner;
        assert_eq!(practitioner.polarity, "negative");
        assert!(practitioner.agent, "я marker");
        assert!(practitioner.target, "ты marker");
    }

    #[test]
    fn pre_facet_exports_deserialize_with_defaults() {
        // A DualTurn JSON without the facet fields (pre-tail exports)
        // loads as affirmative/speakerless/addresseeless.
        let json = r#"{"turn":1,"practitioner":{"topic":null,"journaled":false,"response_empty":true,"recovery":false},"subject":{"witnessed":false,"agreement":"","divergence":0.0,"conatus":0.0,"committed":false,"reset_on_turn":false}}"#;
        let turn: DualTurn = serde_json::from_str(json).expect("old shape loads");
        assert_eq!(turn.practitioner.polarity, "affirmative");
        assert!(!turn.practitioner.agent);
        assert!(!turn.practitioner.target);
    }
}
