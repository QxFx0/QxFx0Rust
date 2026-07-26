//! Structural acceptance gate for the audited content-plan corpus.
//!
//! This gate observes the shadow plan. The legacy renderer intentionally
//! remains authoritative until a later change makes it render these plans.

use qxfx0_pipeline::{process_turn_with_trace, TurnInput};
use qxfx0_semantic::{argued_topic_registry, FallbackReason};
use qxfx0_types::system_state::SystemState;
use std::collections::{BTreeMap, BTreeSet};

const AUDITED_V1_PROMPTS: &str = include_str!("fixtures/audited_v1_prompts.tsv");

#[derive(Debug, Clone, Copy)]
struct CorpusCase {
    prompt: &'static str,
    topic: &'static str,
}

fn corpus_cases() -> Vec<CorpusCase> {
    AUDITED_V1_PROMPTS
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (prompt, topic) = line
                .split_once('\t')
                .expect("corpus fixture must contain prompt and topic");
            CorpusCase { prompt, topic }
        })
        .collect()
}

fn test_state(session_id: &str) -> SystemState {
    SystemState {
        session_id: session_id.into(),
        ..SystemState::default()
    }
}

fn plan_metadata(
    trace: &qxfx0_pipeline::execution_trace::PipelineTrace,
) -> &BTreeMap<String, String> {
    &trace
        .steps
        .iter()
        .find(|step| step.stage == "plan_shadow")
        .expect("every successful turn must record plan_shadow")
        .metadata
}

fn csv_set(value: &str) -> BTreeSet<&str> {
    value.split(',').filter(|entry| !entry.is_empty()).collect()
}

fn expected_predicates(topic: &qxfx0_semantic::ArguedTopic) -> BTreeSet<&str> {
    std::iter::once(topic.primary_predicate_ref().as_str())
        .chain(std::iter::once(
            topic.counterpoint().predicate_ref().as_str(),
        ))
        .chain(
            topic
                .consequence()
                .iter()
                .map(|entry| entry.predicate_ref().as_str()),
        )
        .collect()
}

fn assert_single_terminal_mark(response: &str, case: CorpusCase) {
    let mut chars = response.chars().rev();
    let terminal = chars.next().expect("successful response must be non-empty");
    assert!(
        matches!(terminal, '.' | '!' | '?'),
        "{} has no terminal mark: {response}",
        case.topic
    );
    assert!(
        !matches!(chars.next(), Some('.' | '!' | '?')),
        "{} has repeated terminal marks: {response}",
        case.topic
    );
}

fn assert_structural_plan(
    case: CorpusCase,
    state: &mut SystemState,
    session_id: &str,
    turn: usize,
) {
    let registry = argued_topic_registry().expect("bundled audited profile must parse");
    let expected = registry
        .get(case.topic)
        .unwrap_or_else(|| panic!("fixture topic '{}' must be admitted", case.topic));
    let (output, trace) = process_turn_with_trace(
        &TurnInput {
            session_id: session_id.into(),
            raw_text: case.prompt.into(),
        },
        state,
    );
    let metadata = plan_metadata(&trace);
    let expected_predicates = expected_predicates(expected);
    let actual_predicates = csv_set(
        metadata
            .get("predicate_refs")
            .expect("ready plan must expose predicate refs"),
    );
    let (subject, relation, object) = expected
        .primary_proposition()
        .canonical_slots()
        .expect("admitted thesis must be canonical");
    let expected_claim_count = expected.statement_count();
    let expected_claim_count_str = expected_claim_count.to_string();
    let expected_roles = if expected.consequence().is_some() {
        "thesis,counterpoint,consequence"
    } else {
        "thesis,counterpoint"
    };
    let expected_derivation = if expected.consequence().is_some() {
        "selected_admitted_predicate,added_counterpoint,added_consequence"
    } else {
        "selected_admitted_predicate,added_counterpoint"
    };
    let expected_canonical_slots = [subject.as_str(), relation.as_str(), object.as_str()].join(",");

    assert!(
        !output.blocked,
        "turn {turn} for {} was blocked",
        case.topic
    );
    assert!(
        !output.response.is_empty(),
        "turn {turn} for {} is empty",
        case.topic
    );
    assert_single_terminal_mark(&output.response, case);
    assert_eq!(
        metadata.get("plan_outcome").map(String::as_str),
        Some("ready")
    );
    assert_eq!(
        metadata.get("plan_topic").map(String::as_str),
        Some(case.topic)
    );
    assert_eq!(
        metadata.get("argued_topic_admitted").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        metadata.get("content_profile").map(String::as_str),
        Some("audited_v1")
    );
    assert_eq!(
        metadata.get("plan_claim_count").map(String::as_str),
        Some(expected_claim_count_str.as_str())
    );
    assert_eq!(
        metadata.get("claim_roles").map(String::as_str),
        Some(expected_roles)
    );
    assert_eq!(
        metadata.get("plan_derivation_count").map(String::as_str),
        Some(expected_claim_count_str.as_str())
    );
    assert_eq!(
        metadata.get("derivation_rules").map(String::as_str),
        Some(expected_derivation)
    );
    assert_eq!(
        metadata.get("sentence_budget").map(String::as_str),
        Some(expected_claim_count_str.as_str())
    );
    assert_eq!(
        metadata.get("discourse_relation").map(String::as_str),
        Some("counterpoint")
    );
    assert_eq!(
        metadata.get("dialogue_obligation").map(String::as_str),
        Some("check_agreement")
    );
    assert_eq!(
        metadata.get("canonical_slots").map(String::as_str),
        Some(expected_canonical_slots.as_str())
    );
    assert_eq!(
        actual_predicates, expected_predicates,
        "turn {turn} substituted a predicate"
    );
    assert_eq!(
        actual_predicates.len(),
        expected_claim_count,
        "turn {turn} repeated or omitted a content predicate"
    );
    assert_eq!(
        csv_set(
            metadata
                .get("claim_ids")
                .expect("ready plan must expose claim IDs")
        )
        .len(),
        expected_claim_count,
        "turn {turn} repeated a claim"
    );
    assert_eq!(
        metadata.get("claim_provenance").map(String::as_str),
        Some("curated_release_corpus")
    );
}

#[test]
fn audited_v1_fixture_matches_the_admission_boundary() {
    let cases = corpus_cases();
    let fixture_topics = cases.iter().map(|case| case.topic).collect::<BTreeSet<_>>();
    let admitted_topics = argued_topic_registry()
        .unwrap()
        .topics()
        .map(|topic| topic.topic().as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(cases.len(), 30);
    assert_eq!(fixture_topics.len(), 30);
    assert_eq!(fixture_topics, admitted_topics);
}

#[test]
fn audited_v1_structural_gate_passes_in_fresh_sessions() {
    for (turn, case) in corpus_cases().into_iter().enumerate() {
        let session_id = format!("structural-fresh-{turn}");
        let mut state = test_state(&session_id);
        assert_structural_plan(case, &mut state, &session_id, turn + 1);
    }
}

#[test]
fn audited_v1_structural_gate_passes_in_one_thirty_turn_session() {
    let session_id = "structural-long-session";
    let mut state = test_state(session_id);

    for (turn, case) in corpus_cases().into_iter().enumerate() {
        assert_structural_plan(case, &mut state, session_id, turn + 1);
    }

    assert_eq!(state.dialogue.turn_count, 30);
    assert_eq!(state.dialogue.history.len(), 30);
}

#[test]
fn recognized_but_unadmitted_topic_keeps_an_explicit_fallback_reason() {
    let session_id = "structural-unadmitted";
    let mut state = test_state(session_id);
    let (output, trace) = process_turn_with_trace(
        &TurnInput {
            session_id: session_id.into(),
            raw_text: "что такое знание?".into(),
        },
        &mut state,
    );
    let metadata = plan_metadata(&trace);

    assert!(
        !output.blocked,
        "legacy renderer remains active during shadow mode"
    );
    assert_eq!(
        metadata.get("plan_outcome").map(String::as_str),
        Some("fallback")
    );
    assert_eq!(
        metadata.get("fallback_reason").map(String::as_str),
        Some(FallbackReason::NoAdmissiblePredicate.as_str())
    );
    assert_eq!(
        metadata.get("plan_topic").map(String::as_str),
        Some("знание")
    );
}
