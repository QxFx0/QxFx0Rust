//! «Кодекс» — первая продуктовая оболочка QxFx0: локальный дневник
//! размышлений с протоколом убеждений.
//!
//! Две поверхности:
//! - `reflect` — детерминированная тема дня из аудированного корпуса с
//!   материалом для размышления (тезис и контрпункт) и вопросами;
//! - `report` — сводка по сессии: какие темы возвращаются, какие позиции
//!   высказаны (commitments), где противоречия, состояние эссенции и
//!   governance-статистика. Отчёт выводится в консоль или Markdown.
//!
//! Всё работает офлайн: `reflect` не открывает БД вовсе, `report` читает
//! только локальную сессию. Содержимое отчёста — чистая функция состояния,
//! поэтому два отчёта по одному состоянию байт-в-байт совпадают.

use qxfx0_semantic::argued_topic_registry;
use qxfx0_types::system_state::SystemState;
use serde::Serialize;
use std::collections::BTreeMap;

/// Seconds in one UTC day, for the deterministic topic-of-the-day index.
const SECONDS_PER_DAY: u64 = 86_400;

/// UTC epoch day of a Unix timestamp, clamped to 0 before the epoch.
pub fn epoch_day(unix_seconds: u64) -> u64 {
    unix_seconds / SECONDS_PER_DAY
}

/// Deterministic topic of the day: audited topic names in sorted order,
/// indexed by epoch day. No randomness, no state, no network — the same
/// day always yields the same topic for every copy of the binary.
pub fn daily_topic_name(day: u64) -> String {
    let registry = argued_topic_registry().expect("embedded audited registry is available");
    let mut names: Vec<&str> = registry
        .topics()
        .map(|topic| topic.topic().as_str())
        .collect();
    assert!(!names.is_empty(), "audited corpus is never empty");
    names.sort_unstable();
    names[(day % names.len() as u64) as usize].to_string()
}

/// A reflection card: audited material plus questions for one topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflectionCard {
    pub topic: String,
    pub thesis: String,
    pub counterpoint: String,
    pub questions: Vec<String>,
}

/// Build the reflection card for a topic. `None` when the topic is not in
/// the audited corpus — the caller decides how to present that honestly.
pub fn build_reflection_card(topic_name: &str, day: u64) -> Option<ReflectionCard> {
    let registry = argued_topic_registry().ok()?;
    let entry = registry.get(topic_name)?;
    let surfaces: Vec<&str> = entry.statements().map(|s| s.surface()).collect();
    Some(ReflectionCard {
        topic: topic_name.to_string(),
        thesis: surfaces.first().copied().unwrap_or("").to_string(),
        counterpoint: surfaces.get(1).copied().unwrap_or("").to_string(),
        questions: reflection_questions(topic_name, day),
    })
}

fn reflection_questions(topic: &str, day: u64) -> Vec<String> {
    let questions = [
        format!("Что «{topic}» значит для тебя лично — не по чужим словам, а по опыту этого года?"),
        format!("Вспомни решение, где ты действовал за или против «{topic}». Что ты выбрал на самом деле?"),
        format!("Какова цена твоего понимания «{topic}» — и кто её платит?"),
        format!("Где твои слова о «{topic}» и твои поступки расходятся сильнее всего?"),
        format!("Допустим, ты ошибался насчёт «{topic}». Что изменится завтра, если это правда?"),
    ];
    let start = (day as usize) % questions.len();
    let first = &questions[start];
    let second = &questions[(start + 2) % questions.len()];
    vec![first.clone(), second.clone()]
}

/// Console rendering of the reflection card.
pub fn render_reflection_card(card: &ReflectionCard) -> String {
    let mut out = String::new();
    out.push_str(&format!("Кодекс — тема дня: {}\n\n", card.topic));
    if !card.thesis.is_empty() {
        out.push_str(&format!("Тезис: {}\n", card.thesis));
    }
    if !card.counterpoint.is_empty() {
        out.push_str(&format!("Контрпункт: {}\n", card.counterpoint));
    }
    out.push('\n');
    for (index, question) in card.questions.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", index + 1, question));
    }
    out.push_str("\nЗапиши ответ в дневник:\n");
    out.push_str("  qxfx0 --session-id <сессия> turn \"...\"\n");
    out.push_str("Затем посмотри протокол:  qxfx0 --session-id <сессия> report\n");
    out
}

/// One held position, in report form.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RecentCommitment {
    pub id: usize,
    pub topic: String,
    pub turn: usize,
    pub statement: String,
}

/// Aggregated reflection protocol over one session state.
#[derive(Debug, Clone, Serialize)]
pub struct ReflectionReport {
    pub session_id: String,
    pub turns: usize,
    pub history_entries: usize,
    pub pack_fingerprint: String,
    pub witnesses: usize,
    pub witness_capacity: usize,
    pub angst: f64,
    pub trajectory_committed: bool,
    pub last_topic: Option<String>,
    pub active_commitments: usize,
    pub quarantined_commitments: usize,
    /// Topic → number of active positions, most frequent first.
    pub commitments_by_topic: Vec<(String, usize)>,
    /// Latest held positions, newest first.
    pub recent_commitments: Vec<RecentCommitment>,
    pub contradictions: usize,
    pub governance_completed: usize,
    pub governance_blocked: usize,
    pub governance_capacity_reached: usize,
}

/// How many recent positions the report lists.
const RECENT_COMMITMENT_LIMIT: usize = 5;

/// Build the report as a pure function of the persisted state.
pub fn build_reflection_report(state: &SystemState) -> ReflectionReport {
    let store = state.semantic.semantic_commitments.as_ref();
    let active = store.map(|store| &store.active);
    let mut by_topic: BTreeMap<String, usize> = BTreeMap::new();
    if let Some(active) = active {
        for (payload, _) in active.values() {
            *by_topic.entry(payload.topic.clone()).or_insert(0) += 1;
        }
    }
    let mut commitments_by_topic: Vec<(String, usize)> = by_topic.into_iter().collect();
    commitments_by_topic.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let recent_commitments = active
        .map(|active| {
            let mut recent: Vec<&(qxfx0_types::system_state::FactualClaimPayload, usize)> =
                active.values().collect();
            recent.sort_by_key(|(payload, turn)| std::cmp::Reverse((*turn, payload.turn_seq)));
            recent
                .into_iter()
                .take(RECENT_COMMITMENT_LIMIT)
                .map(|(payload, turn)| RecentCommitment {
                    id: payload.turn_seq,
                    topic: payload.topic.clone(),
                    turn: *turn,
                    statement: payload.statement.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    use qxfx0_types::governance::GovernanceEventType as Event;
    let log = &state.governance_log;
    ReflectionReport {
        session_id: state.session_id.clone(),
        turns: state.dialogue.turn_count,
        history_entries: state.dialogue.history.len(),
        pack_fingerprint: state.semantic.pack_set_fingerprint.clone(),
        witnesses: state.semantic.essence.witnesses.len(),
        witness_capacity: state.semantic.essence.capacity,
        angst: state.semantic.essence.angst,
        trajectory_committed: state.semantic.essence.trajectory_committed,
        last_topic: state.dialogue.last_topic.clone(),
        active_commitments: store.map(|store| store.active.len()).unwrap_or(0),
        quarantined_commitments: store.map(|store| store.quarantine.len()).unwrap_or(0),
        commitments_by_topic,
        recent_commitments,
        contradictions: store.map(|store| store.contradictions.len()).unwrap_or(0),
        governance_completed: log.count_by_type(&Event::TurnCompleted),
        governance_blocked: log.count_by_type(&Event::GuardBlocked),
        governance_capacity_reached: log.count_by_type(&Event::CommitmentCapacityReached),
    }
}

/// Console rendering of the report.
pub fn render_report_console(report: &ReflectionReport) -> String {
    let mut out = String::new();
    out.push_str("Кодекс — протокол размышлений\n");
    out.push_str(&format!(
        "Сессия: {} | ходов: {} | записей истории: {}\n",
        report.session_id, report.turns, report.history_entries
    ));
    if !report.pack_fingerprint.is_empty() {
        let short: String = report.pack_fingerprint.chars().take(12).collect();
        out.push_str(&format!("Активный пак: sha256:{short}…\n"));
    }
    out.push('\n');
    out.push_str(&format!(
        "Эссенция: свидетелей {}/{}, тревога {:.2}, траектория зафиксирована: {}\n",
        report.witnesses,
        report.witness_capacity.max(32),
        report.angst,
        if report.trajectory_committed {
            "да"
        } else {
            "нет"
        }
    ));
    if let Some(topic) = &report.last_topic {
        out.push_str(&format!("Последняя тема: {topic}\n"));
    }
    out.push('\n');
    out.push_str(&format!(
        "Убеждения: активных {}, в карантине {}\n",
        report.active_commitments, report.quarantined_commitments
    ));
    if !report.commitments_by_topic.is_empty() {
        out.push_str("  по темам:\n");
        for (topic, count) in &report.commitments_by_topic {
            out.push_str(&format!("    {topic} — {count}\n"));
        }
    }
    if !report.recent_commitments.is_empty() {
        out.push_str("  последние позиции:\n");
        for commitment in &report.recent_commitments {
            out.push_str(&format!(
                "    [ход {} | {}] {}\n",
                commitment.turn, commitment.topic, commitment.statement
            ));
        }
    }
    out.push('\n');
    out.push_str(&format!("Противоречия: {}\n", report.contradictions));
    out.push_str(&format!(
        "Governance: завершено {}, блокировок {}, отказов ёмкости {}\n",
        report.governance_completed, report.governance_blocked, report.governance_capacity_reached
    ));
    out
}

/// Markdown export of the report — the journal artifact.
pub fn render_report_markdown(report: &ReflectionReport) -> String {
    let mut out = String::new();
    out.push_str("# Кодекс — протокол размышлений\n\n");
    out.push_str(&format!("- **Сессия**: {}\n", report.session_id));
    out.push_str(&format!("- **Ходов**: {}\n", report.turns));
    out.push_str(&format!(
        "- **Записей истории**: {}\n",
        report.history_entries
    ));
    if !report.pack_fingerprint.is_empty() {
        out.push_str(&format!(
            "- **Активный пак**: `sha256:{}`\n",
            report.pack_fingerprint
        ));
    }
    out.push('\n');
    out.push_str("## Эссенция\n\n");
    out.push_str(&format!(
        "- Свидетелей: {} из {}\n",
        report.witnesses,
        report.witness_capacity.max(32)
    ));
    out.push_str(&format!("- Тревога: {:.2}\n", report.angst));
    out.push_str(&format!(
        "- Траектория зафиксирована: {}\n",
        if report.trajectory_committed {
            "да"
        } else {
            "нет"
        }
    ));
    if let Some(topic) = &report.last_topic {
        out.push_str(&format!("- Последняя тема: {topic}\n"));
    }
    out.push('\n');
    out.push_str("## Убеждения\n\n");
    out.push_str(&format!(
        "Активных: {} · в карантине: {} · противоречий: {}\n\n",
        report.active_commitments, report.quarantined_commitments, report.contradictions
    ));
    if !report.commitments_by_topic.is_empty() {
        out.push_str("| Тема | Позиций |\n|---|---|\n");
        for (topic, count) in &report.commitments_by_topic {
            out.push_str(&format!("| {topic} | {count} |\n"));
        }
        out.push('\n');
    }
    if !report.recent_commitments.is_empty() {
        out.push_str("Последние позиции:\n\n");
        for commitment in &report.recent_commitments {
            out.push_str(&format!(
                "> **ход {}, {}** — {}\n\n",
                commitment.turn, commitment.topic, commitment.statement
            ));
        }
    }
    out.push_str("## Governance\n\n");
    out.push_str(&format!(
        "- Завершено ходов: {}\n- Блокировок: {}\n- Отказов ёмкости убеждений: {}\n",
        report.governance_completed, report.governance_blocked, report.governance_capacity_reached
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::system_state::*;

    #[test]
    fn daily_topic_is_deterministic_and_cycles_the_whole_corpus() {
        let first_day = daily_topic_name(20_000);
        assert_eq!(daily_topic_name(20_000), first_day, "same day, same topic");
        let registry = argued_topic_registry().unwrap();
        let total = registry.topics().count();
        let mut seen = std::collections::BTreeSet::new();
        for day in 0..total as u64 {
            seen.insert(daily_topic_name(day));
        }
        assert_eq!(
            seen.len(),
            total,
            "consecutive days must cover every audited topic"
        );
    }

    #[test]
    fn reflection_card_carries_audited_material_and_two_questions() {
        let card = build_reflection_card("свобода", 20_000).expect("audited topic");
        assert!(!card.thesis.is_empty(), "thesis surface must be present");
        assert!(
            !card.counterpoint.is_empty(),
            "counterpoint must be present"
        );
        assert_eq!(card.questions.len(), 2);
        assert!(card.questions.iter().all(|q| q.contains("свобода")));
        let rendered = render_reflection_card(&card);
        assert!(rendered.contains("тема дня"));
        assert!(rendered.contains("Контрпункт"));
    }

    #[test]
    fn reflection_questions_rotate_with_the_day() {
        let monday = build_reflection_card("долг", 100).unwrap().questions;
        let tuesday = build_reflection_card("долг", 101).unwrap().questions;
        assert_ne!(monday, tuesday, "questions must rotate across days");
    }

    #[test]
    fn unaudited_topic_has_no_card() {
        assert!(build_reflection_card("не-аудированная-тема", 0).is_none());
    }

    fn state_with_commitments() -> SystemState {
        let mut state = SystemState {
            session_id: "diary".into(),
            ..SystemState::default()
        };
        state.dialogue.turn_count = 3;
        state.dialogue.history = vec!["a".into(), "b".into(), "c".into()];
        state.dialogue.last_topic = Some("свобода".into());
        state.semantic.essence.witnesses = vec![EssenceWitness {
            turn: 1,
            mode: "Define".into(),
            statement: "свидетельство".into(),
            salience_driver: "rule".into(),
            reconcile_rule: "rule".into(),
            agreement: "PartialAgreement".into(),
            divergence: 0.1,
            conatus_scalar: 0.5,
        }];
        state.semantic.essence.capacity = 32;
        state.semantic.essence.angst = 0.25;

        let mut store = SemanticCommitmentStore::default();
        for (index, (topic, statement, turn)) in [
            ("свобода", "свобода предполагает ответственность", 3usize),
            ("свобода", "свобода требует границ", 2),
            ("долг", "долг важнее настроения", 1),
        ]
        .into_iter()
        .enumerate()
        {
            store.active.insert(
                CommitmentId(index),
                (
                    FactualClaimPayload {
                        statement: statement.into(),
                        confidence: 0.7,
                        origin: CommitmentOrigin::OriginDialogueOutcome,
                        turn_seq: turn,
                        deps: Vec::new(),
                        topic: topic.into(),
                    },
                    turn,
                ),
            );
        }
        store.contradictions.push(ContradictionEvent {
            left: CommitmentId(0),
            right: CommitmentId(1),
            kind: ContradictionKind::ContradictionStatement,
            turn: 3,
        });
        state.semantic.semantic_commitments = Some(store);

        state
            .governance_log
            .append(qxfx0_types::governance::GovernanceEvent {
                turn: 1,
                event_type: qxfx0_types::governance::GovernanceEventType::TurnCompleted,
                family: qxfx0_types::CanonicalMoveFamily::CMGround,
                guard_status: GuardStatus::Allowed,
                timestamp: "turn-1".into(),
            });
        state
            .governance_log
            .append(qxfx0_types::governance::GovernanceEvent {
                turn: 2,
                event_type: qxfx0_types::governance::GovernanceEventType::GuardBlocked,
                family: qxfx0_types::CanonicalMoveFamily::CMGround,
                guard_status: GuardStatus::Blocked("тест".into()),
                timestamp: "turn-2".into(),
            });
        state
            .governance_log
            .append(qxfx0_types::governance::GovernanceEvent {
                turn: 3,
                event_type: qxfx0_types::governance::GovernanceEventType::CommitmentCapacityReached,
                family: qxfx0_types::CanonicalMoveFamily::CMGround,
                guard_status: GuardStatus::Allowed,
                timestamp: "turn-3".into(),
            });
        state
    }

    #[test]
    fn report_aggregates_positions_essence_and_governance() {
        let report = build_reflection_report(&state_with_commitments());
        assert_eq!(report.session_id, "diary");
        assert_eq!(report.turns, 3);
        assert_eq!(report.active_commitments, 3);
        assert_eq!(report.contradictions, 1);
        assert_eq!(report.witnesses, 1);
        assert_eq!((report.angst * 100.0) as u64, 25);
        assert_eq!(
            report.commitments_by_topic,
            vec![("свобода".to_string(), 2), ("долг".to_string(), 1)],
            "topics sorted by frequency, then name"
        );
        assert_eq!(report.recent_commitments.len(), 3);
        assert_eq!(report.recent_commitments[0].turn, 3, "newest first");
        assert_eq!(report.governance_completed, 1);
        assert_eq!(report.governance_blocked, 1);
        assert_eq!(report.governance_capacity_reached, 1);
    }

    #[test]
    fn report_renders_deterministically_in_both_formats() {
        let state = state_with_commitments();
        let report = build_reflection_report(&state);
        assert_eq!(
            render_report_console(&report),
            render_report_console(&report)
        );
        assert_eq!(
            render_report_markdown(&report),
            render_report_markdown(&report)
        );
        let console = render_report_console(&report);
        assert!(console.contains("протокол размышлений"));
        assert!(console.contains("свобода — 2"));
        let markdown = render_report_markdown(&report);
        assert!(markdown.contains("# Кодекс"));
        assert!(markdown.contains("| свобода | 2 |"));
        assert!(markdown.contains("## Governance"));
    }

    #[test]
    fn empty_state_report_is_all_zeros_and_still_renders() {
        let state = SystemState {
            session_id: "fresh".into(),
            ..SystemState::default()
        };
        let report = build_reflection_report(&state);
        assert_eq!(report.active_commitments, 0);
        assert_eq!(report.commitments_by_topic, Vec::<(String, usize)>::new());
        assert_eq!(report.recent_commitments, Vec::<RecentCommitment>::new());
        assert!(!render_report_console(&report).is_empty());
        assert!(!render_report_markdown(&report).is_empty());
    }

    #[test]
    fn epoch_day_converts_unix_seconds() {
        assert_eq!(epoch_day(0), 0);
        assert_eq!(epoch_day(SECONDS_PER_DAY), 1);
        assert_eq!(epoch_day(SECONDS_PER_DAY * 7 + 123), 7);
    }
}
