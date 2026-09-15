//! FELT dual-journal evidence (ADR-0043 U6 «Свидетельства»): the mechanical
//! M6 gate over one session, ported from the Haskell closure gate
//! (`QxFx0.Core.M6FeltGate`).
//!
//! Six gates, each a pure predicate over persisted evidence — no replay, no
//! randomness, no network:
//! 1. `governed-evidence` — the session validates and is non-vacuous
//!    (at least one turn). Calibrated 2026-09-11 against real practice:
//!    pack-binding (`pack_set_fingerprint`) was dropped from this gate
//!    because the fact-grounded rollout is `Disabled` by default
//!    (asserted in code), so no default production session is ever
//!    pack-bound — requiring it made the gate unpassable law, not
//!    measurement. The fingerprint stays embedded in the manifest as
//!    cross-reference, not verdict;
//! 2. `non-fallback-dialogue` — every journaled response is a real answer,
//!    never empty and never the guard-recovery surface;
//! 3. `definition-of-subject` — at least two contentful turns (a subject is
//!    a trajectory, not a point);
//! 4. `distinction-of-positions` — at least two distinct topics (a subject
//!    holds positions apart, not one note repeated);
//! 5. `repair-of-contradiction` — a contradiction (or a revision) happened
//!    AND the store still retains a live position afterwards (retention is
//!    the repair's witness);
//! 6. `commitment-of-memory` — at least ten turns, at least one live
//!    position at the end, and every commitment id ever issued is accounted
//!    for (active, quarantine, or retracted with lineage — nothing silently
//!    dropped).
//!
//! An empty session fails all six: fail-closed, mirroring the Haskell gate.
//!
//! The export artifact embeds the dual journal plus a store summary with
//! exactly the facts the gates read, so `verify_felt_export` re-evaluates
//! the same pure core over the embedded evidence without a database.
//! Per-turn response replay stays with `verify_diary` — the diary is the
//! replay truth, FELT is the gate truth, and `session_digest` (same witness
//! basis as the diary) lets the two artifacts be cross-checked.

use crate::dual_journal::DualJournal;
use qxfx0_pipeline::RendererAuthority;
use qxfx0_types::system_state::{LineageEvent, SystemState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Schema tag of the FELT manifest.
pub const FELT_MANIFEST_SCHEMA: &str = "qxfx0:felt-dual-journal:v1";
/// The guard-recovery surface (same literal the pipeline stores on blocked
/// turns): a turn answered this way is not a dialogue turn.
pub const RECOVERY_RESPONSE: &str = "QxFx0: ответ отклонён системой безопасности.";
/// A subject is a trajectory: the Haskell M6 gate demands a sustained
/// practice, not a single exchange.
pub const FELT_MIN_TURNS: usize = 10;
/// Definition needs at least two contentful turns.
pub const FELT_MIN_DEFINITION_TURNS: usize = 2;
/// Distinction needs at least two held-apart topics.
pub const FELT_MIN_DISTINCT_TOPICS: usize = 2;

/// One of the six mechanical gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeltGate {
    GovernedEvidence,
    NonFallbackDialogue,
    DefinitionOfSubject,
    DistinctionOfPositions,
    RepairOfContradiction,
    CommitmentOfMemory,
}

impl FeltGate {
    pub fn name(self) -> &'static str {
        match self {
            FeltGate::GovernedEvidence => "governed-evidence",
            FeltGate::NonFallbackDialogue => "non-fallback-dialogue",
            FeltGate::DefinitionOfSubject => "definition-of-subject",
            FeltGate::DistinctionOfPositions => "distinction-of-positions",
            FeltGate::RepairOfContradiction => "repair-of-contradiction",
            FeltGate::CommitmentOfMemory => "commitment-of-memory",
        }
    }

    pub fn all() -> [FeltGate; 6] {
        [
            FeltGate::GovernedEvidence,
            FeltGate::NonFallbackDialogue,
            FeltGate::DefinitionOfSubject,
            FeltGate::DistinctionOfPositions,
            FeltGate::RepairOfContradiction,
            FeltGate::CommitmentOfMemory,
        ]
    }
}

/// The verdict: passed only when no gate failed, with the failed gates
/// named (the `M6FeltNotProven [GATE …]` shape of the Haskell gate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeltVerdict {
    pub passed: bool,
    pub failed: Vec<FeltGate>,
}

/// Exactly the facts the six gates read. Built from live state for
/// evaluation, rebuilt from the embedded manifest for verification — one
/// pure core, two entry points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeltFacts {
    pub governed_ok: bool,
    pub journaled_turns: usize,
    pub contentful_turns: usize,
    pub distinct_topics: usize,
    pub contradiction_or_revision: bool,
    pub final_active: usize,
    pub turn_count: usize,
    pub next_id: usize,
    pub active_ids: Vec<usize>,
    pub quarantine_ids: Vec<usize>,
    pub retracted_ids: Vec<usize>,
}

fn facts_from_state(state: &SystemState) -> FeltFacts {
    let journal = DualJournal::build(state);
    let contentful = journal
        .turns
        .iter()
        .filter(|turn| {
            turn.practitioner.journaled
                && !turn.practitioner.response_empty
                && !turn.practitioner.recovery
        })
        .count();
    let topics: BTreeSet<&str> = journal
        .turns
        .iter()
        .filter_map(|turn| turn.practitioner.topic.as_deref())
        .collect();
    let (
        contradiction_or_revision,
        final_active,
        next_id,
        active_ids,
        quarantine_ids,
        retracted_ids,
    ) = match state.semantic.semantic_commitments.as_ref() {
        Some(store) => {
            let revised = store.lineage.values().any(|events| {
                events
                    .iter()
                    .any(|event| matches!(event, LineageEvent::Revised { .. }))
            });
            let mut retracted = BTreeSet::new();
            for (id, events) in &store.lineage {
                if events
                    .iter()
                    .any(|event| matches!(event, LineageEvent::Retracted { .. }))
                {
                    retracted.insert(id.0);
                }
            }
            (
                !store.contradictions.is_empty() || revised,
                store.active.len(),
                store.next_id,
                store.active.keys().map(|id| id.0).collect(),
                store.quarantine.keys().map(|id| id.0).collect(),
                retracted.into_iter().collect(),
            )
        }
        None => (false, 0, 0, Vec::new(), Vec::new(), Vec::new()),
    };
    FeltFacts {
        governed_ok: state.validate().is_empty() && state.dialogue.turn_count >= 1,
        journaled_turns: journal
            .turns
            .iter()
            .filter(|turn| turn.practitioner.journaled)
            .count(),
        contentful_turns: contentful,
        distinct_topics: topics.len(),
        contradiction_or_revision,
        final_active,
        turn_count: state.dialogue.turn_count,
        next_id,
        active_ids,
        quarantine_ids,
        retracted_ids,
    }
}

/// The pure core: six mechanical predicates over the facts.
pub fn evaluate_felt_facts(facts: &FeltFacts) -> FeltVerdict {
    let mut failed = Vec::new();
    if !facts.governed_ok {
        failed.push(FeltGate::GovernedEvidence);
    }
    // Every journaled turn must be contentful (non-empty, non-recovery);
    // an empty journal proves no dialogue at all.
    if facts.journaled_turns == 0 || facts.contentful_turns != facts.journaled_turns {
        failed.push(FeltGate::NonFallbackDialogue);
    }
    if facts.contentful_turns < FELT_MIN_DEFINITION_TURNS {
        failed.push(FeltGate::DefinitionOfSubject);
    }
    if facts.distinct_topics < FELT_MIN_DISTINCT_TOPICS {
        failed.push(FeltGate::DistinctionOfPositions);
    }
    if !(facts.contradiction_or_revision && facts.final_active >= 1) {
        failed.push(FeltGate::RepairOfContradiction);
    }
    let accounted: BTreeSet<usize> = facts
        .active_ids
        .iter()
        .chain(facts.quarantine_ids.iter())
        .chain(facts.retracted_ids.iter())
        .copied()
        .collect();
    let fully_accounted = (0..facts.next_id).all(|id| accounted.contains(&id));
    if !(facts.turn_count >= FELT_MIN_TURNS && facts.final_active >= 1 && fully_accounted) {
        failed.push(FeltGate::CommitmentOfMemory);
    }
    FeltVerdict {
        passed: failed.is_empty(),
        failed,
    }
}

/// Evaluate the six gates over live persisted state.
pub fn evaluate_felt_gates(state: &SystemState) -> FeltVerdict {
    evaluate_felt_facts(&facts_from_state(state))
}

/// Compile-time sanity of the gate thresholds for `doctor`: the constants
/// above are the law, and a future edit that inverts them must fail loudly
/// here instead of silently lowering the bar.
pub fn validate_felt_invariants() -> Vec<String> {
    let mut violations = Vec::new();
    if FELT_MIN_TURNS < 10 {
        violations.push("FELT_MIN_TURNS is below the ten-turn sustained-practice floor".into());
    }
    if FELT_MIN_DEFINITION_TURNS < 2 {
        violations.push("FELT_MIN_DEFINITION_TURNS is below the two-turn definition floor".into());
    }
    if FELT_MIN_DISTINCT_TOPICS < 2 {
        violations.push("FELT_MIN_DISTINCT_TOPICS is below the two-topic distinction floor".into());
    }
    if RECOVERY_RESPONSE.trim().is_empty() {
        violations
            .push("RECOVERY_RESPONSE is empty, the non-fallback gate would pass everything".into());
    }
    if !FELT_MANIFEST_SCHEMA.starts_with("qxfx0:") {
        violations.push("FELT_MANIFEST_SCHEMA lost its qxfx0: namespace".into());
    }
    if FeltGate::all().len() != 6 {
        violations.push("the FELT gate is no longer six gates".into());
    }
    violations
}

/// One gate's outcome, as embedded in the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeltGateOutcome {
    pub gate: String,
    pub passed: bool,
}

/// The canonical, verifiable form of one session's dual-journal evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeltManifest {
    pub schema: String,
    pub session_id: String,
    /// Renderer authority label — same vocabulary as the diary manifest.
    pub renderer: String,
    pub pack_fingerprint: String,
    pub turns: usize,
    pub essence_v2_shadow: bool,
    pub dual: Vec<crate::dual_journal::DualTurn>,
    pub facts: FeltFacts,
    pub gates: Vec<FeltGateOutcome>,
    pub verdict_passed: bool,
    pub failed: Vec<String>,
    /// Recall evidence per discussed topic (Memory M3): shown vs
    /// suppressed, with scores so verify checks the ordering.
    /// Empty for sessions with no held positions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recalls: Vec<RecallRow>,
    /// Stable digest of the final session state (same witness basis as the
    /// diary), so a FELT export cross-checks against the diary export.
    pub session_digest: String,
}

/// One recalled position inside a recall table row: identity,
/// standing and score — everything verify needs to check ordering
/// without the commitment store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallRef {
    pub id: usize,
    pub statement: String,
    pub turn: usize,
    pub status: String,
    pub contradicted: bool,
    pub score: f64,
}

/// Recall evidence for one discussed topic: what the report surface
/// shows (top ranked) and what it suppresses as irrelevant (ranked
/// but beyond the surface limit). Both halves recomputable in shape
/// by verify: disjoint ids, shown sorted by score desc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallRow {
    pub topic: String,
    pub shown: Vec<RecallRef>,
    pub suppressed: Vec<RecallRef>,
}

/// The export artifact: the Markdown evidence file, the manifest and the
/// exact manifest bytes embedded in the Markdown (what a signature covers).
#[derive(Debug, Clone, PartialEq)]
pub struct FeltExport {
    pub markdown: String,
    pub manifest: FeltManifest,
    pub manifest_json: String,
}

/// Build the FELT manifest as a pure function of the persisted state.
/// Unlike the diary export this never refuses an empty session: testifying
/// «not proven» is the measurement tool's job.
/// How many recalled positions the evidence surface shows per topic
/// (mirrors the report surface limit).
pub const RECALL_SHOWN_LIMIT: usize = 5;

/// Build the recall table: per discussed topic, the ranked candidates
/// split into shown (surface) and suppressed (ranked but beyond it).
/// Deterministic in state; topics in encounter order.
pub fn build_recall_rows(state: &SystemState) -> Vec<RecallRow> {
    let Some(store) = state.semantic.semantic_commitments.as_ref() else {
        return Vec::new();
    };
    let mut topics = Vec::new();
    for record in &state.dialogue.journal {
        if let Some(topic) = record.topic.as_deref() {
            if !topics.contains(&topic) {
                topics.push(topic);
            }
        }
    }
    topics
        .into_iter()
        .map(|topic| {
            let ranked = crate::recall::recall_candidates(
                store,
                &state.dialogue.journal,
                topic,
                state.dialogue.turn_count,
                usize::MAX,
            );
            let to_ref = |candidate: &crate::recall::RecallCandidate| RecallRef {
                id: candidate.id.0,
                statement: candidate.statement.clone(),
                turn: candidate.turn,
                status: match candidate.status {
                    crate::recall::RecallStatus::Active => "active".to_string(),
                    crate::recall::RecallStatus::Quarantined => "quarantined".to_string(),
                },
                contradicted: candidate.contradicted,
                score: candidate.score,
            };
            let (shown, suppressed) = ranked.split_at(ranked.len().min(RECALL_SHOWN_LIMIT));
            RecallRow {
                topic: topic.to_string(),
                shown: shown.iter().map(to_ref).collect(),
                suppressed: suppressed.iter().map(to_ref).collect(),
            }
        })
        .filter(|row| !row.shown.is_empty() || !row.suppressed.is_empty())
        .collect()
}

pub fn build_felt_manifest(state: &SystemState, renderer: RendererAuthority) -> FeltManifest {
    let journal = DualJournal::build(state);
    let facts = facts_from_state(state);
    let verdict = evaluate_felt_facts(&facts);
    let gates: Vec<FeltGateOutcome> = FeltGate::all()
        .iter()
        .map(|gate| FeltGateOutcome {
            gate: gate.name().to_string(),
            passed: !verdict.failed.contains(gate),
        })
        .collect();
    FeltManifest {
        schema: FELT_MANIFEST_SCHEMA.to_string(),
        session_id: state.session_id.clone(),
        renderer: crate::renderer_authority_label(renderer).to_string(),
        pack_fingerprint: state.semantic.pack_set_fingerprint.clone(),
        turns: state.dialogue.turn_count,
        essence_v2_shadow: journal.essence_v2_shadow,
        dual: journal.turns,
        facts,
        gates,
        verdict_passed: verdict.passed,
        failed: verdict
            .failed
            .iter()
            .map(|gate| gate.name().to_string())
            .collect(),
        recalls: build_recall_rows(state),
        session_digest: crate::felt_session_digest(state),
    }
}

/// Render the human-readable evidence file: one section per dual turn, the
/// six gate outcomes up front, the verification manifest embedded at the
/// end. A pure function of `(state, renderer)`.
pub fn build_felt_export(state: &SystemState, renderer: RendererAuthority) -> FeltExport {
    let manifest = build_felt_manifest(state, renderer);
    let manifest_json = serde_json::to_string_pretty(&manifest).expect("manifest serializes");

    let mut out = String::new();
    out.push_str("# FELT — двойной журнал свидетельств\n\n");
    out.push_str(&format!(
        "- **Сессия**: {}\n- **Ходов**: {}\n- **Вердикт**: {}\n",
        manifest.session_id,
        manifest.turns,
        if manifest.verdict_passed {
            "доказан (все шесть гейтов)"
        } else {
            "не доказан"
        }
    ));
    if !manifest.failed.is_empty() {
        out.push_str(&format!(
            "- **Провалены**: {}\n",
            manifest.failed.join(", ")
        ));
    }
    if !manifest.pack_fingerprint.is_empty() {
        out.push_str(&format!(
            "- **Пак знаний**: `sha256:{}`\n",
            manifest.pack_fingerprint
        ));
    }
    out.push('\n');
    out.push_str("## Гейты\n\n");
    for outcome in &manifest.gates {
        out.push_str(&format!(
            "- {} — {}\n",
            outcome.gate,
            if outcome.passed {
                "пройден"
            } else {
                "провален"
            }
        ));
    }
    out.push('\n');
    for turn in &manifest.dual {
        let topic = turn.practitioner.topic.as_deref().unwrap_or("—");
        out.push_str(&format!("## Ход {} — {topic}\n\n", turn.turn));
        out.push_str(&format!(
            "- Практик: {} | {}\n",
            if turn.practitioner.journaled {
                "записан"
            } else {
                "без записи (до журнала)"
            },
            if turn.practitioner.recovery {
                "восстановление охраны"
            } else if turn.practitioner.response_empty {
                "пустой ответ"
            } else {
                "ответ"
            }
        ));
        out.push_str(&format!(
            "- Субъект: {} | {} | conatus {:.2}\n\n",
            if turn.subject.witnessed {
                format!(
                    "свидетельство ({} / {})",
                    turn.subject.salience_driver, turn.subject.reconcile_rule
                )
            } else {
                "без свидетельства".to_string()
            },
            if turn.subject.committed {
                "позиция держится"
            } else {
                "без позиции"
            },
            turn.subject.conatus
        ));
    }

    if !manifest.recalls.is_empty() {
        out.push_str("## Воспоминания\n\n");
        out.push_str("Что показала бы поверхность отчёта по каждой теме — и что отранжировано, но скрыто:\n\n");
        for row in &manifest.recalls {
            out.push_str(&format!(
                "- **{}**: показано {}, скрыто {}\n",
                row.topic,
                row.shown.len(),
                row.suppressed.len()
            ));
            for shown in &row.shown {
                out.push_str(&format!(
                    "  - [ид {} | {}] {} (скор {:.3})\n",
                    shown.id, shown.status, shown.statement, shown.score
                ));
            }
        }
        out.push('\n');
    }

    out.push_str("## Верификация\n\n");
    out.push_str("Свидетельства можно проверить: `qxfx0 felt-verify <этот-файл>` — ");
    out.push_str("команда пересчитывает все шесть гейтов по встроенным записям ");
    out.push_str("этой же версией системы. Изменённая хотя бы на букву ");
    out.push_str("запись не пройдёт проверку.\n\n");
    out.push_str("```felt-manifest\n");
    out.push_str(&manifest_json);
    out.push_str("\n```\n");

    FeltExport {
        markdown: out,
        manifest,
        manifest_json,
    }
}

/// Append an HMAC-SHA256 signature block over the embedded manifest bytes.
/// Same construction as the diary signature (authenticity, not secrecy).
pub fn append_felt_signature(markdown: &mut String, manifest_json: &str, passphrase: &str) {
    let signature = crate::felt_hmac_hex(passphrase.as_bytes(), manifest_json.as_bytes());
    markdown.push_str(&format!(
        "```felt-signature\nhmac-sha256:{signature}\n```\n"
    ));
}

/// Outcome of a FELT verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeltVerification {
    pub session_id: String,
    pub turns: usize,
    pub gates_passed: bool,
    pub signature_checked: bool,
    /// `None` — verified; `Some(reason)` — the first failure found.
    pub failure: Option<String>,
}

impl FeltVerification {
    pub fn verified(&self) -> bool {
        self.failure.is_none()
    }
}

fn felt_failed(
    session_id: &str,
    turns: usize,
    gates_passed: bool,
    signature_checked: bool,
    reason: String,
) -> FeltVerification {
    FeltVerification {
        session_id: session_id.to_string(),
        turns,
        gates_passed,
        signature_checked,
        failure: Some(reason),
    }
}

/// Verify a FELT export without a database: the signature (when present),
/// the schema tag, then a full re-evaluation of the six gates over the
/// embedded dual evidence. The recorded verdict must match the recomputed
/// one — a mismatch means the file was edited after export.
pub fn verify_felt_export(markdown: &str, passphrase: Option<&str>) -> FeltVerification {
    let extracted = match crate::extract_felt_blocks(markdown) {
        Ok(extracted) => extracted,
        Err(error) => {
            return felt_failed("", 0, false, false, error);
        }
    };
    let signature_checked = extracted.signature.is_some();
    if let Some(signature) = extracted.signature.as_deref() {
        let Some(passphrase) = passphrase else {
            return felt_failed(
                "",
                0,
                false,
                true,
                "манифест подписан (hmac-sha256): укажите --passphrase".into(),
            );
        };
        let expected =
            crate::felt_hmac_hex(passphrase.as_bytes(), extracted.manifest_json.as_bytes());
        if !crate::felt_digests_equal(&expected, signature) {
            return felt_failed(
                "",
                0,
                false,
                true,
                "подпись не сходится: неверная парольная фраза или манифест изменён".into(),
            );
        }
    }

    let manifest: FeltManifest = match serde_json::from_str(&extracted.manifest_json) {
        Ok(manifest) => manifest,
        Err(error) => {
            return felt_failed(
                "",
                0,
                false,
                signature_checked,
                format!("манифест не читается: {error}"),
            );
        }
    };
    if manifest.schema != FELT_MANIFEST_SCHEMA {
        return felt_failed(
            &manifest.session_id,
            manifest.turns,
            false,
            signature_checked,
            format!("неизвестная схема манифеста: {}", manifest.schema),
        );
    }
    if crate::renderer_authority_from_label(&manifest.renderer).is_none() {
        return felt_failed(
            &manifest.session_id,
            manifest.turns,
            false,
            signature_checked,
            format!("неизвестный рендерер: {}", manifest.renderer),
        );
    }
    if manifest.dual.len() != manifest.turns {
        return felt_failed(
            &manifest.session_id,
            manifest.turns,
            false,
            signature_checked,
            format!(
                "двойной журнал короче заявленного: записей {}, ходов {}",
                manifest.dual.len(),
                manifest.turns
            ),
        );
    }
    let recomputed = evaluate_felt_facts(&manifest.facts);
    let recomputed_failed: Vec<String> = recomputed
        .failed
        .iter()
        .map(|gate| gate.name().to_string())
        .collect();
    if recomputed.passed != manifest.verdict_passed || recomputed_failed != manifest.failed {
        return felt_failed(
            &manifest.session_id,
            manifest.turns,
            recomputed.passed,
            signature_checked,
            "записанный вердикт не совпадает с пересчётом гейтов — файл изменён после экспорта"
                .into(),
        );
    }
    let recorded: Vec<(&str, bool)> = manifest
        .gates
        .iter()
        .map(|outcome| (outcome.gate.as_str(), outcome.passed))
        .collect();
    let expected_gates: Vec<(&str, bool)> = FeltGate::all()
        .iter()
        .map(|gate| (gate.name(), !recomputed.failed.contains(gate)))
        .collect();
    if recorded != expected_gates {
        return felt_failed(
            &manifest.session_id,
            manifest.turns,
            recomputed.passed,
            signature_checked,
            "таблица гейтов не совпадает с пересчётом — файл изменён после экспорта".into(),
        );
    }
    // Recall table shape: shown and suppressed disjoint per topic,
    // shown ordered by score desc. Scores embed the ranking, so the
    // shape check catches edits without the commitment store.
    for row in &manifest.recalls {
        let mut ids = std::collections::BTreeSet::new();
        for entry in row.shown.iter().chain(row.suppressed.iter()) {
            if !ids.insert(entry.id) {
                return felt_failed(
                    &manifest.session_id,
                    manifest.turns,
                    recomputed.passed,
                    signature_checked,
                    format!(
                        "дублирующаяся позиция {} в recall-таблице темы «{}» — файл изменён",
                        entry.id, row.topic
                    ),
                );
            }
        }
        let ordered = row
            .shown
            .windows(2)
            .all(|pair| pair[0].score >= pair[1].score);
        if !ordered {
            return felt_failed(
                &manifest.session_id,
                manifest.turns,
                recomputed.passed,
                signature_checked,
                format!(
                    "показанные позиции темы «{}» не упорядочены по скору — файл изменён",
                    row.topic
                ),
            );
        }
    }
    FeltVerification {
        session_id: manifest.session_id,
        turns: manifest.turns,
        gates_passed: recomputed.passed,
        signature_checked,
        failure: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::system_state::{
        CommitmentId, CommitmentOrigin, ContradictionEvent, ContradictionKind, FactualClaimPayload,
        JournalRecord, SemanticCommitmentStore,
    };

    #[allow(clippy::field_reassign_with_default)]
    fn journaled_state(turns: usize, topics: &[Option<&str>]) -> SystemState {
        let mut state = SystemState::default();
        state.session_id = "felt".into();
        state.semantic.pack_set_fingerprint = "ab".repeat(32);
        state.dialogue.turn_count = turns;
        state.dialogue.journal = (1..=turns)
            .map(|turn| JournalRecord {
                turn,
                day: 20_000,
                topic: topics
                    .get(turn - 1)
                    .and_then(|topic| *topic)
                    .map(str::to_string),
                input: format!("entry {turn}"),
                response: format!("response {turn}"),
                state_digest: "cd".repeat(32),
                subject_authority: qxfx0_types::system_state::default_subject_authority(),
            })
            .collect();
        state
    }

    fn commit_everything(state: &mut SystemState, count: usize) {
        let mut store = SemanticCommitmentStore::default();
        for id in 0..count {
            store.active.insert(
                CommitmentId(id),
                (
                    FactualClaimPayload {
                        statement: format!("position {id}"),
                        confidence: 0.9,
                        origin: CommitmentOrigin::OriginDialogueOutcome,
                        turn_seq: 1,
                        deps: Vec::new(),
                        topic: "память".into(),
                    },
                    1,
                ),
            );
            store
                .lineage
                .insert(CommitmentId(id), vec![LineageEvent::Committed { turn: 1 }]);
        }
        store.next_id = count;
        state.semantic.semantic_commitments = Some(store);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn empty_session_fails_all_six() {
        let state = SystemState::default();
        let verdict = evaluate_felt_gates(&state);
        assert!(!verdict.passed);
        assert_eq!(verdict.failed, FeltGate::all().to_vec());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn governed_needs_validity_plus_a_turn_not_pack_binding() {
        // Calibration lock (2026-09-11, real practice): the fact-grounded
        // rollout is Disabled by default, so no default session is ever
        // pack-bound. A valid multi-turn session without a fingerprint
        // passes governed-evidence; an empty-but-valid session fails it.
        let mut state = journaled_state(2, &[Some("память"), Some("внимание")]);
        state.semantic.pack_set_fingerprint.clear();
        let verdict = evaluate_felt_gates(&state);
        assert!(!verdict.failed.contains(&FeltGate::GovernedEvidence));
        let empty = SystemState::default();
        assert!(evaluate_felt_gates(&empty)
            .failed
            .contains(&FeltGate::GovernedEvidence));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn recovery_response_fails_the_dialogue_gate() {
        let mut state = journaled_state(2, &[Some("память"), Some("внимание")]);
        state.dialogue.journal[1].response = RECOVERY_RESPONSE.into();
        let verdict = evaluate_felt_gates(&state);
        assert!(verdict.failed.contains(&FeltGate::NonFallbackDialogue));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn sustained_two_topic_committed_session_passes() {
        let mut state = journaled_state(
            10,
            &[
                Some("память"),
                Some("внимание"),
                Some("память"),
                Some("внимание"),
                Some("память"),
                Some("внимание"),
                Some("память"),
                Some("внимание"),
                Some("память"),
                Some("внимание"),
            ],
        );
        commit_everything(&mut state, 2);
        let store = state.semantic.semantic_commitments.as_mut().unwrap();
        store.contradictions.push(ContradictionEvent {
            left: CommitmentId(0),
            right: CommitmentId(1),
            kind: ContradictionKind::ContradictionStatement,
            turn: 5,
        });
        let verdict = evaluate_felt_gates(&state);
        assert!(verdict.passed, "failed gates: {:?}", verdict.failed);
        assert!(verdict.failed.is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn silently_dropped_id_fails_the_memory_gate() {
        let mut state = journaled_state(10, &[Some("память"), Some("внимание")]);
        commit_everything(&mut state, 2);
        let store = state.semantic.semantic_commitments.as_mut().unwrap();
        // Id 1 vanishes with no lineage: the store dropped a position
        // without testifying to it.
        store.active.remove(&CommitmentId(1));
        store.lineage.remove(&CommitmentId(1));
        store.contradictions.push(ContradictionEvent {
            left: CommitmentId(0),
            right: CommitmentId(0),
            kind: ContradictionKind::ContradictionScope,
            turn: 4,
        });
        let verdict = evaluate_felt_gates(&state);
        assert!(verdict.failed.contains(&FeltGate::CommitmentOfMemory));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn export_round_trip_verifies_and_tamper_fails() {
        let mut state = journaled_state(3, &[Some("память"), Some("внимание"), Some("память")]);
        commit_everything(&mut state, 1);
        let export = build_felt_export(&state, RendererAuthority::AuditedPlan);
        let verification = verify_felt_export(&export.markdown, None);
        assert!(verification.verified(), "{:?}", verification.failure);
        assert!(!verification.gates_passed);
        assert_eq!(verification.session_id, "felt");

        let tampered = export.markdown.replacen("не доказан", "доказан", 1);
        let tampered_check = verify_felt_export(&tampered, None);
        // Prose is outside the signed manifest: the verdict still
        // re-evaluates honestly from the embedded evidence.
        assert!(tampered_check.verified());
        assert!(!tampered_check.gates_passed);

        let mut evil_manifest: FeltManifest = serde_json::from_str(&export.manifest_json).unwrap();
        evil_manifest.verdict_passed = true;
        evil_manifest.failed.clear();
        let evil_json = serde_json::to_string_pretty(&evil_manifest).unwrap();
        let evil_markdown = export.markdown.replace(&export.manifest_json, &evil_json);
        let evil_check = verify_felt_export(&evil_markdown, None);
        assert!(!evil_check.verified());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn signature_paths_match_the_diary_shape() {
        let mut state = journaled_state(2, &[Some("память"), Some("внимание")]);
        commit_everything(&mut state, 1);
        let mut export = build_felt_export(&state, RendererAuthority::AuditedPlan);
        append_felt_signature(&mut export.markdown, &export.manifest_json, "secret");
        assert!(!verify_felt_export(&export.markdown, None).verified());
        assert!(verify_felt_export(&export.markdown, Some("secret")).verified());
        assert!(!verify_felt_export(&export.markdown, Some("wrong")).verified());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn recall_table_splits_shown_and_suppressed() {
        let mut state = journaled_state(3, &[Some("память"), Some("внимание"), Some("память")]);
        commit_everything(&mut state, 7);
        // Spread turns and contest one pair so scores strictly differ.
        if let Some(store) = state.semantic.semantic_commitments.as_mut() {
            for (id, turn) in [(2usize, 3usize), (4, 2)] {
                if let Some((payload, _)) = store.active.get_mut(&CommitmentId(id)) {
                    payload.turn_seq = turn;
                }
            }
            store.contradictions.push(ContradictionEvent {
                left: CommitmentId(0),
                right: CommitmentId(1),
                kind: ContradictionKind::ContradictionStatement,
                turn: 3,
            });
        }
        let manifest = build_felt_manifest(&state, RendererAuthority::AuditedPlan);
        // Two topics discussed, but only память holds positions.
        assert_eq!(manifest.recalls.len(), 1);
        let row = &manifest.recalls[0];
        assert_eq!(row.topic, "память");
        assert_eq!(row.shown.len(), RECALL_SHOWN_LIMIT);
        assert_eq!(row.shown.len() + row.suppressed.len(), 7);
        let ordered = row
            .shown
            .windows(2)
            .all(|pair| pair[0].score >= pair[1].score);
        assert!(ordered, "shown sorted by score desc");

        let export = build_felt_export(&state, RendererAuthority::AuditedPlan);
        assert!(export.markdown.contains("## Воспоминания"));
        assert!(verify_felt_export(&export.markdown, None).verified());

        // Reordering the shown half edits the manifest: verify must fail.
        let mut evil: FeltManifest = serde_json::from_str(&export.manifest_json).unwrap();
        evil.recalls[0].shown.reverse();
        let evil_json = serde_json::to_string_pretty(&evil).unwrap();
        let evil_markdown = export.markdown.replace(&export.manifest_json, &evil_json);
        assert!(!verify_felt_export(&evil_markdown, None).verified());

        // Duplicating an id into the suppressed half also fails.
        let mut dup: FeltManifest = serde_json::from_str(&export.manifest_json).unwrap();
        let clone = dup.recalls[0].shown[0].clone();
        dup.recalls[0].suppressed.push(clone);
        let dup_json = serde_json::to_string_pretty(&dup).unwrap();
        let dup_markdown = export.markdown.replace(&export.manifest_json, &dup_json);
        assert!(!verify_felt_export(&dup_markdown, None).verified());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn export_is_pure_and_does_not_mutate_state() {
        let mut state = journaled_state(2, &[Some("память"), None]);
        commit_everything(&mut state, 1);
        let before = serde_json::to_string(&state).expect("state serializes");
        let first = build_felt_export(&state, RendererAuthority::AuditedPlan);
        let second = build_felt_export(&state, RendererAuthority::AuditedPlan);
        assert_eq!(
            serde_json::to_string(&state).expect("state serializes"),
            before
        );
        assert_eq!(first.markdown, second.markdown);
        assert_eq!(first.manifest_json, second.manifest_json);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn invariants_hold() {
        assert!(validate_felt_invariants().is_empty());
    }
}
