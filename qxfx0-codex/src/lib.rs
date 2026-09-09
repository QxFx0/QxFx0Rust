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

pub mod journal;

use qxfx0_pipeline::RendererAuthority;
use qxfx0_semantic::argued_topic_registry;
use qxfx0_types::system_state::SystemState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Seconds in one UTC day, for the deterministic topic-of-the-day index.
const SECONDS_PER_DAY: u64 = 86_400;
/// A topic becomes due for an intentional callback after this many days.
const REVISIT_AFTER_DAYS: u64 = 7;
/// Essence angst from which the practice surfaces call it out explicitly —
/// below this the numbers speak for themselves in the report.
pub const PRACTICE_ANGST_ATTENTION: f64 = 0.5;

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

/// State-aware topic of the day — the revisit policy of the practice.
///
/// Deterministic in `(day, state)`, never random:
/// 1. topics due for a callback (at least seven days since their last answer)
///    come first, ordered by last practice day;
/// 2. otherwise an unvisited topic is selected from the sorted corpus;
/// 3. once all topics have been visited, the oldest topic returns.
///
/// The day participates in each tie break, so the policy is a pure function
/// of `(day, state)` and has no random source.
pub fn select_topic_of_day(day: u64, state: Option<&SystemState>) -> String {
    let Some(state) = state else {
        return daily_topic_name(day);
    };
    let registry = argued_topic_registry().expect("embedded audited registry is available");
    let mut names: Vec<&str> = registry
        .topics()
        .map(|topic| topic.topic().as_str())
        .collect();
    names.sort_unstable();

    // The calendar is the source of truth. Fall back to a commitment's turn
    // for pre-calendar sessions loaded from older state files.
    let mut last_day_by_topic = state.dialogue.topic_last_practice_day.clone();
    if let Some(store) = state.semantic.semantic_commitments.as_ref() {
        for (payload, turn) in store.active.values() {
            let entry = last_day_by_topic
                .entry(payload.topic.clone())
                .or_insert(*turn as u64);
            *entry = (*entry).max(*turn as u64);
        }
    }

    let due: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| {
            last_day_by_topic
                .get(*name)
                .is_some_and(|last| day.saturating_sub(*last) >= REVISIT_AFTER_DAYS)
        })
        .collect();
    if !due.is_empty() {
        let oldest_due = due
            .iter()
            .map(|name| last_day_by_topic[*name])
            .min()
            .expect("due topics have recorded practice days");
        let oldest_due: Vec<&str> = due
            .into_iter()
            .filter(|name| last_day_by_topic[*name] == oldest_due)
            .collect();
        return oldest_due[(day % oldest_due.len() as u64) as usize].to_string();
    }

    let never: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| !last_day_by_topic.contains_key(*name))
        .collect();
    if !never.is_empty() {
        return never[(day % never.len() as u64) as usize].to_string();
    }

    let oldest_day = names
        .iter()
        .map(|name| last_day_by_topic[*name])
        .min()
        .expect("every topic has a position here");
    let oldest: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| last_day_by_topic[*name] == oldest_day)
        .collect();
    oldest[(day % oldest.len() as u64) as usize].to_string()
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

/// A prior position on the card's topic, echoed into the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorPosition {
    pub statement: String,
    pub turn: usize,
}

/// The last recorded contradiction, as the practice sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContradictionEcho {
    pub left: String,
    pub right: String,
    pub turn: usize,
}

/// How many prior positions the card echoes.
const CARD_CALLBACK_LIMIT: usize = 2;

/// The graph's answer to the practitioner's own recorded position: the
/// newest held statement on the topic, challenged by a typed opposing edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionChallenge {
    /// Turn of the challenged position.
    pub turn: usize,
    /// The practitioner's own words.
    pub position: String,
    /// The opposing edge's curated sentence (`ru_original`).
    pub challenge: String,
}

/// Deterministically pick the graph's challenge for a held position: the
/// topic's typed opposing edges (shared selection with the turn response,
/// `qxfx0_semantic::challenge`), indexed by `(day + byte-salt of the
/// position + turn)`. Different held positions rotate to different
/// challenges; the same position on the same day is stable. `None` when the
/// graph carries no opposing edge for the topic — the card then honestly
/// falls back to the corpus counterpoint.
pub fn position_challenge(
    topic: &str,
    position: &str,
    turn: usize,
    day: u64,
) -> Option<PositionChallenge> {
    let salt = position
        .bytes()
        .map(u64::from)
        .sum::<u64>()
        .wrapping_add(u64::try_from(turn).unwrap_or(0))
        .wrapping_add(day);
    let challenge = qxfx0_semantic::challenge::opposing_challenge_sentence(topic, salt)?;
    Some(PositionChallenge {
        turn,
        position: position.to_string(),
        challenge,
    })
}

/// A reflection card with the journal's memory attached.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryCard {
    pub card: ReflectionCard,
    /// True when the topic was chosen by the revisit policy.
    pub revisited: bool,
    pub prior_positions: Vec<PriorPosition>,
    /// The graph's challenge to the practitioner's newest position on the
    /// topic, replacing the corpus counterpoint on revisits.
    pub challenge: Option<PositionChallenge>,
    pub contradiction: Option<ContradictionEcho>,
    /// Essence angst — the practice's living tension, echoed when elevated.
    pub angst: f64,
    pub practice_days: usize,
}

/// Attach the journal's memory to a reflection card: prior positions on the
/// topic (newest first, bounded), the graph's challenge to the newest one,
/// the last unresolved contradiction and the practice calendar. A pure
/// function of `(state, day)`.
pub fn build_memory_card(card: ReflectionCard, state: &SystemState, day: u64) -> MemoryCard {
    let mut prior_positions: Vec<PriorPosition> = state
        .semantic
        .semantic_commitments
        .as_ref()
        .map(|store| {
            let mut positions: Vec<PriorPosition> = store
                .active
                .values()
                .filter(|(payload, _)| payload.topic == card.topic)
                .map(|(payload, turn)| PriorPosition {
                    statement: payload.statement.clone(),
                    turn: *turn,
                })
                .collect();
            positions.sort_by_key(|position| std::cmp::Reverse(position.turn));
            positions
        })
        .unwrap_or_default();
    prior_positions.truncate(CARD_CALLBACK_LIMIT);

    let challenge = prior_positions
        .first()
        .and_then(|newest| position_challenge(&card.topic, &newest.statement, newest.turn, day));

    let contradiction = state
        .semantic
        .semantic_commitments
        .as_ref()
        .and_then(|store| {
            let (event, left, right) = store
                .contradictions
                .iter()
                .rev()
                .filter_map(|event| {
                    let payload_of = |id: &qxfx0_types::system_state::CommitmentId| {
                        store
                            .active
                            .get(id)
                            .map(|entry| &entry.0)
                            .or_else(|| store.quarantine.get(id).map(|entry| &entry.0))
                    };
                    let left = payload_of(&event.left)?;
                    let right = payload_of(&event.right)?;
                    // A contradiction is attached to every topic it touches;
                    // the two positions may come from different semantic
                    // topics when engagement finds a cross-topic conflict.
                    (left.topic == card.topic || right.topic == card.topic)
                        .then_some((event, left, right))
                })
                .next()?;
            Some(ContradictionEcho {
                left: left.statement.clone(),
                right: right.statement.clone(),
                turn: event.turn,
            })
        });

    MemoryCard {
        revisited: !prior_positions.is_empty(),
        card,
        prior_positions,
        challenge,
        contradiction,
        angst: state.semantic.essence.angst,
        practice_days: state.dialogue.practice_days.len(),
    }
}

/// Console rendering of the memory card. On a revisit with a recorded
/// position, the corpus counterpoint gives way to the graph's challenge to
/// the practitioner's own words — the practitioner has already met the
/// corpus counterpoint when they first answered this topic.
pub fn render_memory_card(memory: &MemoryCard) -> String {
    let card = &memory.card;
    let mut out = String::new();
    out.push_str(&format!("Кодекс — тема дня: {}\n\n", card.topic));
    if !card.thesis.is_empty() {
        out.push_str(&format!("Тезис: {}\n", card.thesis));
    }
    match &memory.challenge {
        Some(challenge) => {
            out.push_str(&format!(
                "\n— Граф возражает твоей позиции [ход {}]:\n",
                challenge.turn
            ));
            out.push_str(&format!("  твоя позиция: {}\n", challenge.position));
            out.push_str(&format!("  возражение:   {}\n", challenge.challenge));
            out.push_str("  Как их совместить — или одна из них должна уйти?\n");
        }
        None => {
            if !card.counterpoint.is_empty() {
                out.push_str(&format!("Контрпункт: {}\n", card.counterpoint));
            }
        }
    }
    out.push('\n');
    for (index, question) in card.questions.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", index + 1, question));
    }
    out.push_str("\nЗапиши ответ в дневник:\n");
    out.push_str("  qxfx0 --session-id <сессия> turn \"...\"\n");
    out.push_str("Затем посмотри протокол:  qxfx0 --session-id <сессия> report\n");
    if memory.revisited {
        out.push_str("\n— В прошлый раз ты писал об этом:\n");
        for position in &memory.prior_positions {
            out.push_str(&format!(
                "  [ход {}] {}\n",
                position.turn, position.statement
            ));
        }
        out.push_str("Вернись к прежней мысли: она всё ещё твоя — или уже нет?\n");
    }
    if let Some(contradiction) = &memory.contradiction {
        out.push_str("\n— Событие практики: твои мысли столкнулись.\n");
        out.push_str(&format!("  новая:   {}\n", contradiction.left));
        out.push_str(&format!("  прежняя: {}\n", contradiction.right));
        out.push_str(
            "  Противоречие не ошибка — это точка роста. Разберись, что именно изменилось.\n",
        );
        if memory.angst >= PRACTICE_ANGST_ATTENTION {
            out.push_str(&format!(
                "  Тревога практики: {:.2} — столкновения позиций копятся; разберись с ними, прежде чем идти дальше.\n",
                memory.angst
            ));
        }
    }
    if memory.practice_days > 0 {
        out.push_str(&format!("\nДней практики: {}\n", memory.practice_days));
    }
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
    /// Distinct UTC days on which the session saw a turn.
    pub practice_days: usize,
    /// Longest run of consecutive practice days ending on the last one.
    pub practice_streak: usize,
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
    /// Per-topic position dynamics: every held position in turn order,
    /// marked when it participated in a caught contradiction. The diary's
    /// own trajectory — what the practitioner actually said, and where it
    /// broke.
    pub topic_timelines: Vec<TopicTimeline>,
    pub contradictions: usize,
    pub governance_completed: usize,
    pub governance_blocked: usize,
    pub governance_capacity_reached: usize,
    pub governance_commitment_contradicted: usize,
}

/// One held position in a topic's timeline.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TimelinePosition {
    pub turn: usize,
    pub statement: String,
    /// True when a caught contradiction event involves this position.
    pub contradicted: bool,
}

/// The position dynamics of one topic.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TopicTimeline {
    pub topic: String,
    /// Positions in turn order.
    pub positions: Vec<TimelinePosition>,
}

/// How many recent positions the report lists.
const RECENT_COMMITMENT_LIMIT: usize = 5;

/// Longest run of consecutive days ending at the last practiced day.
/// The streak lives in the journal's own calendar — it does not know
/// "today", so a gap simply ends the run at the last entry.
fn practice_streak(days: &std::collections::BTreeSet<u64>) -> usize {
    let mut streak = 0usize;
    let mut previous: Option<u64> = None;
    for day in days.iter().rev() {
        match previous {
            Some(expected) if expected == *day => streak += 1,
            Some(_) => break,
            None => streak = 1,
        }
        previous = day.checked_sub(1);
        if previous.is_none() {
            break;
        }
    }
    streak
}

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

    // Per-topic timelines: every held position in turn order, marked when a
    // caught contradiction event names it. Topics ordered by their latest
    // position — the most recently practiced topic leads.
    let mut timelines: BTreeMap<String, Vec<TimelinePosition>> = BTreeMap::new();
    if let Some(store) = store {
        let contradicted_ids: std::collections::BTreeSet<&qxfx0_types::system_state::CommitmentId> =
            store
                .contradictions
                .iter()
                .flat_map(|event| [&event.left, &event.right])
                .collect();
        let mut entries: Vec<(String, TimelinePosition)> = store
            .active
            .iter()
            .map(|(id, (payload, turn))| {
                (
                    payload.topic.clone(),
                    TimelinePosition {
                        turn: *turn,
                        statement: payload.statement.clone(),
                        contradicted: contradicted_ids.contains(id),
                    },
                )
            })
            .collect();
        entries.sort_by_key(|(_, position)| (position.turn, position.statement.clone()));
        for (topic, position) in entries {
            timelines.entry(topic).or_default().push(position);
        }
    }
    let mut topic_timelines: Vec<TopicTimeline> = timelines
        .into_iter()
        .map(|(topic, positions)| TopicTimeline { topic, positions })
        .collect();
    topic_timelines.sort_by_key(|timeline| {
        std::cmp::Reverse(timeline.positions.last().map(|p| p.turn).unwrap_or(0))
    });

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
        practice_days: state.dialogue.practice_days.len(),
        practice_streak: practice_streak(&state.dialogue.practice_days),
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
        topic_timelines,
        contradictions: store.map(|store| store.contradictions.len()).unwrap_or(0),
        governance_completed: log.count_by_type(&Event::TurnCompleted),
        governance_blocked: log.count_by_type(&Event::GuardBlocked),
        governance_capacity_reached: log.count_by_type(&Event::CommitmentCapacityReached),
        governance_commitment_contradicted: log.count_by_type(&Event::CommitmentContradicted),
    }
}

/// Console rendering of the report.
pub fn render_report_console(report: &ReflectionReport) -> String {
    let mut out = String::new();
    out.push_str("Кодекс — протокол размышлений\n");
    out.push_str(&format!(
        "Сессия: {} | ходов: {} | записей истории: {} | дней практики: {} (серия {})\n",
        report.session_id,
        report.turns,
        report.history_entries,
        report.practice_days,
        report.practice_streak
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
    if !report.topic_timelines.is_empty() {
        out.push_str("  динамика позиций:\n");
        for timeline in &report.topic_timelines {
            out.push_str(&format!("    {}:\n", timeline.topic));
            for position in &timeline.positions {
                let mark = if position.contradicted { " ✗" } else { "" };
                out.push_str(&format!(
                    "      [ход {}{mark}] {}\n",
                    position.turn, position.statement
                ));
            }
        }
    }
    out.push('\n');
    out.push_str(&format!("Противоречия: {}\n", report.contradictions));
    if report.contradictions > 0 && report.angst >= PRACTICE_ANGST_ATTENTION {
        out.push_str(&format!(
            "Тревога практики: {:.2} — твои позиции сталкиваются; вернись к противоречиям и разберись, что изменилось.\n",
            report.angst
        ));
    }
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
        "- **Дней практики**: {} (серия {})\n",
        report.practice_days, report.practice_streak
    ));
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
    if report.contradictions > 0 && report.angst >= PRACTICE_ANGST_ATTENTION {
        out.push_str(
            "- **Тревога практики высокая**: твои позиции сталкиваются; вернись к противоречиям и разберись, что изменилось.\n",
        );
    }
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
    if !report.topic_timelines.is_empty() {
        out.push_str("## Динамика позиций\n\n");
        out.push_str(
            "Что ты говорил и где это сломалось (✗ — позиция поймана в противоречии):\n\n",
        );
        for timeline in &report.topic_timelines {
            out.push_str(&format!("**{}**\n\n", timeline.topic));
            for position in &timeline.positions {
                let mark = if position.contradicted { " ✗" } else { "" };
                out.push_str(&format!(
                    "- ход {}{mark} — {}\n",
                    position.turn, position.statement
                ));
            }
            out.push('\n');
        }
    }
    out.push_str("## Governance\n\n");
    out.push_str(&format!(
        "- Завершено ходов: {}\n- Блокировок: {}\n- Отказов ёмкости убеждений: {}\n",
        report.governance_completed, report.governance_blocked, report.governance_capacity_reached
    ));
    out
}

// ---------------------------------------------------------------------------
// Verifiable diary export
//
// The diary's guarantee is the system's determinism: for the same binary
// (same knowledge packs, same rules) and the same journal inputs, every
// response and every state digest recomputes identically. An export is a
// human-readable Markdown diary with an embedded canonical manifest
// (inputs, responses, days, per-turn state digests, pack fingerprint); a
// verifier replays the journal in a fresh in-memory session and compares.
// An optional HMAC-SHA256 passphrase signature covers the manifest bytes
// and proves the export was authored by whoever knows the phrase.
// ---------------------------------------------------------------------------

/// Schema tag of the embedded diary manifest.
pub const DIARY_MANIFEST_SCHEMA: &str = "qxfx0:codex-diary:v1";

/// One journal turn in manifest form — a mirror of
/// `qxfx0_types::system_state::JournalRecord`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiaryManifestEntry {
    pub turn: usize,
    pub day: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    pub input: String,
    pub response: String,
    pub state_digest: String,
}

/// The canonical, replay-verifiable form of one session's diary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiaryManifest {
    pub schema: String,
    pub session_id: String,
    /// Renderer authority label (`audited_plan`, `legacy_shadow`,
    /// `v2_canary`) — replay must use the same one.
    pub renderer: String,
    pub pack_fingerprint: String,
    pub practice_days: usize,
    pub practice_streak: usize,
    pub turns: usize,
    pub contradictions: usize,
    pub entries: Vec<DiaryManifestEntry>,
    /// Stable digest of the final session state, records included.
    pub session_digest: String,
}

/// Stable renderer-authority label for the manifest.
pub fn renderer_authority_label(authority: RendererAuthority) -> &'static str {
    match authority {
        RendererAuthority::AuditedPlan => "audited_plan",
        RendererAuthority::LegacyShadow => "legacy_shadow",
        RendererAuthority::V2Canary => "v2_canary",
    }
}

/// Parse a renderer-authority label recorded in a manifest.
pub fn renderer_authority_from_label(label: &str) -> Option<RendererAuthority> {
    match label {
        "audited_plan" => Some(RendererAuthority::AuditedPlan),
        "legacy_shadow" => Some(RendererAuthority::LegacyShadow),
        "v2_canary" => Some(RendererAuthority::V2Canary),
        _ => None,
    }
}

fn session_digest(state: &SystemState) -> String {
    // Same witness basis as the per-turn journal digest in
    // `journal::stamp_practice_day`: observational shadow state
    // (`semantic.essence_v2`, ADR-0043 U2; `semantic.blanket_v2`, U3) is
    // excluded so the final diary digest stays comparable across the
    // shadow's landings.
    let mut visible = state.clone();
    visible.semantic.essence_v2 = None;
    visible.semantic.blanket_v2 = None;
    qxfx0_pipeline::execution_trace::calculate_stable_digest(&visible)
        .expect("SystemState serializes deterministically for the stable digest")
}

/// Build the diary manifest as a pure function of the persisted state.
pub fn build_diary_manifest(state: &SystemState, renderer: RendererAuthority) -> DiaryManifest {
    DiaryManifest {
        schema: DIARY_MANIFEST_SCHEMA.to_string(),
        session_id: state.session_id.clone(),
        renderer: renderer_authority_label(renderer).to_string(),
        pack_fingerprint: state.semantic.pack_set_fingerprint.clone(),
        practice_days: state.dialogue.practice_days.len(),
        practice_streak: practice_streak(&state.dialogue.practice_days),
        turns: state.dialogue.turn_count,
        contradictions: state
            .semantic
            .semantic_commitments
            .as_ref()
            .map(|store| store.contradictions.len())
            .unwrap_or(0),
        entries: state
            .dialogue
            .journal
            .iter()
            .map(|record| DiaryManifestEntry {
                turn: record.turn,
                day: record.day,
                topic: record.topic.clone(),
                input: record.input.clone(),
                response: record.response.clone(),
                state_digest: record.state_digest.clone(),
            })
            .collect(),
        session_digest: session_digest(state),
    }
}

/// The export artifact: the Markdown diary, the manifest and the exact
/// manifest bytes embedded in the Markdown (what a signature covers).
#[derive(Debug, Clone, PartialEq)]
pub struct DiaryExport {
    pub markdown: String,
    pub manifest: DiaryManifest,
    pub manifest_json: String,
}

/// Contradiction statements on a given turn, for the prose rendering.
fn contradictions_on_turn(state: &SystemState, turn: usize) -> Vec<(String, String)> {
    let Some(store) = state.semantic.semantic_commitments.as_ref() else {
        return Vec::new();
    };
    store
        .contradictions
        .iter()
        .filter(|event| event.turn == turn)
        .filter_map(|event| {
            let payload = |id: &qxfx0_types::system_state::CommitmentId| {
                store
                    .active
                    .get(id)
                    .map(|entry| &entry.0)
                    .or_else(|| store.quarantine.get(id).map(|entry| &entry.0))
            };
            let left = payload(&event.left)?;
            let right = payload(&event.right)?;
            Some((left.statement.clone(), right.statement.clone()))
        })
        .collect()
}

/// Render the human-readable diary: one section per journal turn, with the
/// practice summary up front and the verification manifest embedded at the
/// end. A pure function of `(state, renderer)`.
pub fn build_diary_export(state: &SystemState, renderer: RendererAuthority) -> DiaryExport {
    let manifest = build_diary_manifest(state, renderer);
    let manifest_json = serde_json::to_string_pretty(&manifest).expect("manifest serializes");

    let mut out = String::new();
    out.push_str("# Кодекс — дневник\n\n");
    out.push_str(&format!(
        "- **Сессия**: {}\n- **Ходов**: {} | **дней практики**: {} (серия {})\n",
        manifest.session_id, manifest.turns, manifest.practice_days, manifest.practice_streak
    ));
    if manifest.contradictions > 0 {
        out.push_str(&format!(
            "- **Противоречий поймано**: {}\n",
            manifest.contradictions
        ));
    }
    if !manifest.pack_fingerprint.is_empty() {
        out.push_str(&format!(
            "- **Пак знаний**: `sha256:{}`\n",
            manifest.pack_fingerprint
        ));
    }
    out.push('\n');

    for entry in &manifest.entries {
        let topic = entry.topic.as_deref().unwrap_or("—");
        out.push_str(&format!(
            "## Ход {} — день {} — {topic}\n\n",
            entry.turn, entry.day
        ));
        out.push_str(&format!("> {}\n\n", entry.input));
        out.push_str(&format!("{}\n\n", entry.response));
        for (left, right) in contradictions_on_turn(state, entry.turn) {
            out.push_str(&format!(
                "— Событие практики: столкнулись позиции:\n  новая:   {left}\n  прежняя: {right}\n\n"
            ));
        }
    }

    out.push_str("## Верификация\n\n");
    out.push_str("Дневник можно проверить: `qxfx0 verify-diary <этот-файл>` — ");
    out.push_str("команда пересобирает каждую запись этой же версией системы ");
    out.push_str("и сверяет ответы и дайджесты состояний. Изменённая хотя бы на букву ");
    out.push_str("запись не пройдёт проверку.\n\n");
    out.push_str("```codex-manifest\n");
    out.push_str(&manifest_json);
    out.push_str("\n```\n");

    DiaryExport {
        markdown: out,
        manifest,
        manifest_json,
    }
}

/// Append an HMAC-SHA256 signature block over the embedded manifest bytes.
/// The passphrase is used as the raw HMAC key (authenticity of the export
/// artifact, not secrecy of the diary).
pub fn append_diary_signature(markdown: &mut String, manifest_json: &str, passphrase: &str) {
    let signature = hmac_sha256_hex(passphrase.as_bytes(), manifest_json.as_bytes());
    markdown.push_str(&format!(
        "```codex-signature\nhmac-sha256:{signature}\n```\n"
    ));
}

/// HMAC-SHA256 (RFC 2104) over `message` with `key`, using only `sha2`.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36u8; BLOCK];
    let mut outer_pad = [0x5cu8; BLOCK];
    for index in 0..BLOCK {
        inner_pad[index] ^= key_block[index];
        outer_pad[index] ^= key_block[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner.finalize());
    outer.finalize().into()
}

fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    let digest = hmac_sha256(key, message);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Length-independent comparison of two hex digests (compares fixed-size
/// bytes, so timing does not leak the position of a mismatch).
fn hex_digests_equal(left: &str, right: &str) -> bool {
    fn parse_hex64(text: &str) -> Option<[u8; 32]> {
        if text.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (index, chunk) in text.as_bytes().chunks(2).enumerate() {
            let high = (chunk[0] as char).to_digit(16)?;
            let low = (chunk[1] as char).to_digit(16)?;
            bytes[index] = ((high << 4) | low) as u8;
        }
        Some(bytes)
    }
    match (parse_hex64(left), parse_hex64(right)) {
        (Some(left), Some(right)) => {
            let mut difference = 0u8;
            for index in 0..32 {
                difference |= left[index] ^ right[index];
            }
            difference == 0
        }
        _ => false,
    }
}

/// The blocks extracted from an exported diary file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedDiary {
    pub manifest_json: String,
    pub signature: Option<String>,
}

fn fenced_block<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("```{tag}\n");
    let start = text.find(&open)? + open.len();
    let rest = &text[start..];
    let end = rest.find("\n```")?;
    Some(&rest[..end])
}

/// Extract the manifest (and optional signature) from a diary export.
/// Fails closed on anything malformed — verification then reports the
/// extraction error instead of trusting partial content.
pub fn extract_diary_blocks(markdown: &str) -> Result<ExtractedDiary, String> {
    let manifest_json = fenced_block(markdown, "codex-manifest")
        .ok_or_else(|| "блок ```codex-manifest не найден".to_string())?
        .to_string();
    if manifest_json.trim().is_empty() {
        return Err("блок манифеста пуст".into());
    }
    let signature = fenced_block(markdown, "codex-signature").map(|block| {
        block
            .strip_prefix("hmac-sha256:")
            .unwrap_or(block)
            .trim()
            .to_string()
    });
    Ok(ExtractedDiary {
        manifest_json,
        signature,
    })
}

/// Outcome of a diary verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryVerification {
    pub session_id: String,
    pub turns: usize,
    pub signature_checked: bool,
    /// `None` — verified; `Some(reason)` — the first failure found.
    pub failure: Option<String>,
}

impl DiaryVerification {
    pub fn verified(&self) -> bool {
        self.failure.is_none()
    }
}

fn verification_failed(
    session_id: &str,
    turns: usize,
    signature_checked: bool,
    reason: String,
) -> DiaryVerification {
    DiaryVerification {
        session_id: session_id.to_string(),
        turns,
        signature_checked,
        failure: Some(reason),
    }
}

/// Verify a diary export by deterministic replay: every journal entry is
/// re-run in a fresh in-memory session with the recorded renderer authority
/// and day; responses, per-turn state digests and the final session digest
/// must all match. When the export carries an HMAC signature, the
/// passphrase must be supplied and must match before replay even starts.
pub fn verify_diary(markdown: &str, passphrase: Option<&str>) -> DiaryVerification {
    let extracted = match extract_diary_blocks(markdown) {
        Ok(extracted) => extracted,
        Err(error) => {
            return verification_failed("", 0, false, error);
        }
    };
    let signature_checked = extracted.signature.is_some();
    if let Some(signature) = extracted.signature.as_deref() {
        let Some(passphrase) = passphrase else {
            return verification_failed(
                "",
                0,
                true,
                "манифест подписан (hmac-sha256): укажите --passphrase".into(),
            );
        };
        let expected = hmac_sha256_hex(passphrase.as_bytes(), extracted.manifest_json.as_bytes());
        if !hex_digests_equal(&expected, signature) {
            return verification_failed(
                "",
                0,
                true,
                "подпись не сходится: неверная парольная фраза или манифест изменён".into(),
            );
        }
    }

    let manifest: DiaryManifest = match serde_json::from_str(&extracted.manifest_json) {
        Ok(manifest) => manifest,
        Err(error) => {
            return verification_failed(
                "",
                0,
                signature_checked,
                format!("манифест не читается: {error}"),
            );
        }
    };
    let unknown_session = manifest.session_id.clone();
    if manifest.schema != DIARY_MANIFEST_SCHEMA {
        return verification_failed(
            &unknown_session,
            manifest.entries.len(),
            signature_checked,
            format!("неизвестная схема манифеста: {}", manifest.schema),
        );
    }
    let Some(authority) = renderer_authority_from_label(&manifest.renderer) else {
        return verification_failed(
            &unknown_session,
            manifest.entries.len(),
            signature_checked,
            format!("неизвестный рендерер: {}", manifest.renderer),
        );
    };

    let db = match qxfx0_persistence::Persistence::open_memory() {
        Ok(db) => db,
        Err(error) => {
            return verification_failed(
                &unknown_session,
                manifest.entries.len(),
                signature_checked,
                format!("не удалось открыть сессию для реплея: {error}"),
            );
        }
    };

    let mut state = None;
    for entry in &manifest.entries {
        let response = match crate::journal::run_journal_turn(
            &db,
            &manifest.session_id,
            &entry.input,
            entry.day,
            authority,
        ) {
            Ok(response) => response,
            Err(error) => {
                return verification_failed(
                    &manifest.session_id,
                    manifest.entries.len(),
                    signature_checked,
                    format!("ход {}: реплей не выполнился: {error}", entry.turn),
                );
            }
        };
        if response != entry.response {
            return verification_failed(
                &manifest.session_id,
                manifest.entries.len(),
                signature_checked,
                format!(
                    "ход {}: ответ реплея не совпал с дневником — запись изменена или реплайется другой версией системы",
                    entry.turn
                ),
            );
        }
        state = match db.load_state(&manifest.session_id) {
            Ok(Some(loaded)) => Some(loaded),
            _ => {
                return verification_failed(
                    &manifest.session_id,
                    manifest.entries.len(),
                    signature_checked,
                    format!("ход {}: состояние реплея не читается", entry.turn),
                );
            }
        };
        let replayed_digest = state
            .as_ref()
            .and_then(|state| state.dialogue.journal.last())
            .map(|record| record.state_digest.as_str())
            .unwrap_or_default();
        if replayed_digest != entry.state_digest {
            return verification_failed(
                &manifest.session_id,
                manifest.entries.len(),
                signature_checked,
                format!(
                    "ход {}: дайджест состояния реплея не совпал с дневником",
                    entry.turn
                ),
            );
        }
    }

    let Some(state) = state else {
        return verification_failed(
            &manifest.session_id,
            0,
            signature_checked,
            "дневник пуст: реплею нечего проверять".into(),
        );
    };
    let final_digest = session_digest(&state);
    if final_digest != manifest.session_digest {
        return verification_failed(
            &manifest.session_id,
            manifest.entries.len(),
            signature_checked,
            "итоговый дайджест сессии не совпал — дневник и реплей разошлись".into(),
        );
    }

    DiaryVerification {
        session_id: manifest.session_id,
        turns: manifest.entries.len(),
        signature_checked,
        failure: None,
    }
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
        assert!(
            !console.contains("Тревога практики"),
            "angst 0.25 with a contradiction stays below the attention threshold"
        );
        let markdown = render_report_markdown(&report);
        assert!(markdown.contains("# Кодекс"));
        assert!(markdown.contains("| свобода | 2 |"));
        assert!(markdown.contains("## Governance"));
    }

    #[test]
    fn elevated_angst_with_contradictions_is_called_out_in_both_formats() {
        let mut state = state_with_commitments();
        state.semantic.essence.angst = 0.6;
        let report = build_reflection_report(&state);
        let console = render_report_console(&report);
        assert!(console.contains("Тревога практики: 0.6"));
        assert!(render_report_markdown(&report).contains("Тревога практики высокая"));
        // Without contradictions the attention line stays silent even at
        // high angst — the call-out is about colliding positions.
        state
            .semantic
            .semantic_commitments
            .as_mut()
            .expect("store exists")
            .contradictions
            .clear();
        let report = build_reflection_report(&state);
        assert!(!render_report_console(&report).contains("Тревога практики"));
    }

    #[test]
    fn memory_card_echoes_angst_only_when_elevated() {
        let mut state = state_with_commitments();
        let card = build_reflection_card("свобода", 20_000).unwrap();
        let calm = build_memory_card(card.clone(), &state, 20_000);
        assert!(calm.contradiction.is_some());
        assert!(calm.angst < PRACTICE_ANGST_ATTENTION);
        assert!(!render_memory_card(&calm).contains("Тревога практики"));

        state.semantic.essence.angst = 0.75;
        let tense = build_memory_card(card, &state, 20_000);
        let rendered = render_memory_card(&tense);
        assert!(rendered.contains("Тревога практики: 0.75"));
    }

    #[test]
    fn report_traces_the_position_timeline_with_contradiction_marks() {
        let state = state_with_commitments();
        let report = build_reflection_report(&state);
        // «свобода» is the most recently practiced topic and leads.
        assert_eq!(report.topic_timelines.len(), 2);
        assert_eq!(report.topic_timelines[0].topic, "свобода");
        let freedom = &report.topic_timelines[0];
        assert_eq!(
            freedom
                .positions
                .iter()
                .map(|position| position.turn)
                .collect::<Vec<_>>(),
            vec![2, 3],
            "positions come in turn order"
        );
        // The contradiction event names both freedom positions.
        assert!(freedom.positions.iter().all(|p| p.contradicted));
        let duty = &report.topic_timelines[1];
        assert_eq!(duty.topic, "долг");
        assert!(duty.positions.iter().all(|p| !p.contradicted));

        let console = render_report_console(&report);
        assert!(console.contains("динамика позиций"));
        assert!(console.contains("[ход 3 ✗]"));
        let markdown = render_report_markdown(&report);
        assert!(markdown.contains("## Динамика позиций"));
        assert!(markdown.contains("- ход 3 ✗ —"));
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

    fn state_with_topic_positions(positions: &[(&str, usize)]) -> SystemState {
        let mut state = SystemState {
            session_id: "diary".into(),
            ..SystemState::default()
        };
        let mut store = SemanticCommitmentStore::default();
        for (index, (topic, turn)) in positions.iter().enumerate() {
            store.active.insert(
                CommitmentId(index),
                (
                    FactualClaimPayload {
                        statement: format!("позиция по теме {topic}"),
                        confidence: 0.7,
                        origin: CommitmentOrigin::OriginDialogueOutcome,
                        turn_seq: *turn,
                        deps: Vec::new(),
                        topic: topic.to_string(),
                    },
                    *turn,
                ),
            );
        }
        state.semantic.semantic_commitments = Some(store);
        state.dialogue.topic_last_practice_day = positions
            .iter()
            .map(|(topic, turn)| ((*topic).to_string(), *turn as u64))
            .collect();
        state
    }

    #[test]
    fn selection_prefers_never_reflected_topics() {
        let state = state_with_topic_positions(&[("свобода", 1), ("долг", 5)]);
        for day in 0..8u64 {
            let chosen = select_topic_of_day(day, Some(&state));
            assert!(
                chosen != "свобода" && chosen != "долг",
                "day {day}: unreflected topics must come first, got {chosen}"
            );
        }
        assert_eq!(select_topic_of_day(8, Some(&state)), "свобода");
        // Without state the choice degenerates to the stateless daily topic.
        assert_eq!(select_topic_of_day(7, None), daily_topic_name(7));
    }

    #[test]
    fn selection_revisits_the_oldest_position_deterministically() {
        let registry = argued_topic_registry().unwrap();
        let positions: Vec<(String, usize)> = registry
            .topics()
            .enumerate()
            .map(|(index, topic)| (topic.topic().as_str().to_string(), index + 1))
            .collect();
        let refs: Vec<(&str, usize)> = positions
            .iter()
            .map(|(topic, turn)| (topic.as_str(), *turn))
            .collect();
        let state = state_with_topic_positions(&refs);

        // Oldest position is the topic with turn 1.
        let registry_names: Vec<String> = {
            let mut names: Vec<String> = registry
                .topics()
                .map(|topic| topic.topic().as_str().to_string())
                .collect();
            names.sort();
            names
        };
        let oldest = &registry_names[0];
        assert_eq!(
            select_topic_of_day(REVISIT_AFTER_DAYS, Some(&state)),
            *oldest
        );
        assert_eq!(
            select_topic_of_day(REVISIT_AFTER_DAYS, Some(&state)),
            select_topic_of_day(REVISIT_AFTER_DAYS, Some(&state)),
            "deterministic in (day, state)"
        );
    }

    #[test]
    fn practice_streak_counts_only_the_trailing_run() {
        let days: std::collections::BTreeSet<u64> = [5u64, 6, 7, 10, 11].into_iter().collect();
        assert_eq!(practice_streak(&days), 2);
        let contiguous: std::collections::BTreeSet<u64> = (1..=9).collect();
        assert_eq!(practice_streak(&contiguous), 9);
        let empty: std::collections::BTreeSet<u64> = Default::default();
        assert_eq!(practice_streak(&empty), 0);
    }

    #[test]
    fn memory_card_echoes_positions_contradiction_and_days() {
        let mut state = state_with_commitments();
        state.dialogue.practice_days = [100u64, 101, 102].into_iter().collect();
        let card = build_reflection_card("свобода", 20_000).unwrap();
        let memory = build_memory_card(card, &state, 20_000);
        assert!(memory.revisited);
        assert_eq!(memory.prior_positions.len(), 2, "bounded to two echoes");
        assert_eq!(memory.prior_positions[0].turn, 3, "newest first");
        assert!(memory.contradiction.is_some());
        assert_eq!(memory.practice_days, 3);
        let rendered = render_memory_card(&memory);
        assert!(rendered.contains("В прошлый раз"));
        assert!(rendered.contains("Событие практики"));
        assert!(rendered.contains("Дней практики: 3"));
    }

    #[test]
    fn revisit_card_challenges_the_practitioners_own_position() {
        let state = state_with_topic_positions(&[("свобода", 4)]);
        let card = build_reflection_card("свобода", 20_000).unwrap();
        let memory = build_memory_card(card, &state, 20_000);
        let challenge = memory
            .challenge
            .clone()
            .expect("«свобода» carries opposing graph edges");
        assert_eq!(challenge.turn, 4, "the newest position is challenged");
        assert!(
            challenge.challenge.contains("свобод"),
            "the challenge is a graph sentence about the topic: {}",
            challenge.challenge
        );
        // Deterministic in (position, turn, day).
        let card_again = build_reflection_card("свобода", 20_000).unwrap();
        let again = build_memory_card(card_again, &state, 20_000);
        assert_eq!(again.challenge, Some(challenge.clone()));

        let rendered = render_memory_card(&memory);
        assert!(rendered.contains("Граф возражает твоей позиции"));
        assert!(rendered.contains(&challenge.challenge));
        assert!(
            !rendered.contains("Контрпункт:"),
            "a challenge replaces the corpus counterpoint on a revisit"
        );
    }

    #[test]
    fn different_positions_draw_different_challenges() {
        // The selection salt is the byte sum of the held statement, so two
        // sessions whose positions differ by exactly one byte land on
        // adjacent challenges — guaranteed different for ≥2 edges.
        let left =
            position_challenge("свобода", "позиция а", 1, 20_000).expect("opposing edges exist");
        let right =
            position_challenge("свобода", "позиция б", 1, 20_000).expect("opposing edges exist");
        assert_ne!(left.challenge, right.challenge);
    }

    #[test]
    fn positionless_card_keeps_the_corpus_counterpoint() {
        let card = build_reflection_card("свобода", 20_000).unwrap();
        let state = SystemState {
            session_id: "fresh".into(),
            ..SystemState::default()
        };
        let memory = build_memory_card(card, &state, 20_000);
        assert!(memory.challenge.is_none());
        assert!(!memory.revisited);
        let rendered = render_memory_card(&memory);
        assert!(
            rendered.contains("Контрпункт:"),
            "without a held position the corpus counterpoint stands"
        );
    }

    #[test]
    fn hmac_matches_rfc_4231_vectors() {
        // RFC 4231, test cases 1-3 (SHA-256).
        let cases: [(String, String, &str); 3] = [
            (
                "0b".repeat(20),
                "4869205468657265".to_string(),
                "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
            ),
            (
                "4a656665".to_string(),
                "7768617420646f2079612077616e7420666f72206e6f7468696e673f".to_string(),
                "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            ),
            (
                "aa".repeat(20),
                "dd".repeat(50),
                "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe",
            ),
        ];
        for (key, message, expected) in cases {
            let key = hex_bytes(&key);
            let message = hex_bytes(&message);
            assert_eq!(hmac_sha256_hex(&key, &message), expected);
        }
    }

    fn hex_bytes(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|index| {
                u8::from_str_radix(&text[index..index + 2], 16).expect("valid hex in test vector")
            })
            .collect()
    }

    #[test]
    fn signature_and_extraction_round_trip() {
        let manifest_json = "{\n  \"schema\": \"qxfx0:codex-diary:v1\"\n}";
        let mut markdown = String::from("дневник...\n\n```codex-manifest\n");
        markdown.push_str(manifest_json);
        markdown.push_str("\n```\n");
        append_diary_signature(&mut markdown, manifest_json, "фраза");

        let extracted = extract_diary_blocks(&markdown).expect("blocks extract");
        assert_eq!(extracted.manifest_json, manifest_json);
        let signature = extracted.signature.expect("signature extracted");
        assert_eq!(
            signature,
            hmac_sha256_hex("фраза".as_bytes(), manifest_json.as_bytes())
        );
        assert!(hex_digests_equal(
            &signature,
            &hmac_sha256_hex("фраза".as_bytes(), manifest_json.as_bytes())
        ));
        assert!(!hex_digests_equal(
            &signature,
            &hmac_sha256_hex("другая фраза".as_bytes(), manifest_json.as_bytes())
        ));
    }

    #[test]
    fn extraction_fails_closed_without_a_manifest_block() {
        assert!(extract_diary_blocks("просто дневник без блока").is_err());
    }

    #[test]
    fn renderer_labels_round_trip() {
        for authority in [
            RendererAuthority::AuditedPlan,
            RendererAuthority::LegacyShadow,
            RendererAuthority::V2Canary,
        ] {
            let label = renderer_authority_label(authority);
            assert_eq!(renderer_authority_from_label(label), Some(authority));
        }
        assert_eq!(renderer_authority_from_label("nope"), None);
    }
}
