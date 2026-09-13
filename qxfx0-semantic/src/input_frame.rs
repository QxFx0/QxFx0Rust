//! Input semantic frame (density doctrine; Haskell
//! `input_semantic_contract` analog, v1 scope).
//!
//! The runtime contract, in order: `raw_text → WordUnit[] → InputFrame
//! → route_hint → route/family`, with the legacy proposition detectors
//! as fallback. The frame never routes alone: `route_hint` is emitted
//! only on high-confidence patterns (`confidence >= ROUTE_HINT_FLOOR`);
//! everywhere else the legacy cascade decides exactly as before, so a
//! v1 frame changes routing on pinned patterns only.
//!
//! Deliberate v1 boundaries (documented, not accidental):
//! - `topic` covers explicit prepositional mentions (`про X`, `о X`,
//!   `насчёт X`) and is `None` otherwise — the parser subject stays
//!   authoritative for topic;
//! - `polarity` is single-negation only (`не`/`нет`/`ни-` tokens);
//!   double negation is recorded as Negative (limitation, tested);
//! - `speech_act` mirrors the mode vocabulary; no new routing modes.

use crate::composer::PropositionMode;

use serde::{Deserialize, Serialize};

/// One classified token: the contract requires exactly one unit per
/// token, a non-empty lemma, confidence in `[0, 1]` and a non-empty
/// ambiguity list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordUnit {
    pub surface: String,
    pub lemma: String,
    pub confidence: f64,
    pub ambiguity: Vec<String>,
}

/// Clause shape from terminal punctuation and imperative markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClauseType {
    Declarative,
    Interrogative,
    Imperative,
    Exclamatory,
}

/// What the turn does (mode vocabulary; no new routing modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeechAct {
    Ask,
    Tell,
    Challenge,
    Greet,
}

/// Single-negation polarity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Polarity {
    Affirmative,
    Negative,
}

/// The normalized meaning frame of one user turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputFrame {
    pub normalized_text: String,
    pub units: Vec<WordUnit>,
    pub clause_type: ClauseType,
    pub speech_act: SpeechAct,
    pub polarity: Polarity,
    /// Explicit prepositional mention, if any — the parser subject
    /// stays authoritative otherwise.
    pub topic: Option<String>,
    /// Emphasized constituent, if detectable — first content token
    /// after a sentence-initial interrogative.
    pub focus: Option<String>,
    /// First-person marker (`я`), if present.
    pub agent: Option<String>,
    /// Second-person marker (`ты`/`вы`), if present.
    pub target: Option<String>,
    /// Advisory routing hint: `Some` only at high confidence, else
    /// the legacy detectors decide exactly as before.
    pub route_hint: Option<PropositionMode>,
    pub confidence: f64,
}

/// Confidence floor for emitting `route_hint`.
pub const ROUTE_HINT_FLOOR: f64 = 0.8;

/// Greeting shape (mirrors the parser's deliberately small set, so the
/// hint path and the legacy path agree where both fire).
const GREETING_MARKERS: &[&str] = &["привет", "здравствуй", "здравствуйте", "добрый", "доброе"];
/// Mental verbs: a second-person interrogative about them is a
/// reflective turn even without the parser's `об`-shape (new signal,
// new behavior — pinned by test).
const MENTAL_VERBS: &[&str] = &[
    "думаешь",
    "думаете",
    "чувствуешь",
    "чувствуете",
    "считаешь",
    "считаете",
    "помнишь",
    "помните",
    "знаешь",
    "знаете",
    "полагаешь",
];
/// Interrogative openers for focus extraction.
const INTERROGATIVES: &[&str] = &[
    "что",
    "кто",
    "где",
    "когда",
    "почему",
    "зачем",
    "сколько",
    "какой",
    "какая",
    "какое",
    "какие",
    "как",
];

/// Lowercase alphanumeric tokens, order preserved.
fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn lemmatize(word: &str) -> (String, f64) {
    match qxfx0_morphology::get_runtime().lemmatize(word) {
        qxfx0_types::morphology::MorphologyLookup::Resolved(result) => (result.lemma, 1.0),
        _ => (word.to_string(), 0.5),
    }
}

/// Build the frame. Pure, total, deterministic; morphology-backed
/// lemmatization with a surface fallback, so every token yields
/// exactly one unit even off-dictionary.
pub fn frame_input(raw_text: &str) -> InputFrame {
    let normalized_text = raw_text
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let words = tokens(&normalized_text);
    let mut units = Vec::with_capacity(words.len());
    for word in &words {
        let (lemma, confidence) = lemmatize(word);
        let mut ambiguity = vec![lemma.clone()];
        if ambiguity[0] != *word {
            ambiguity.push(word.clone());
        }
        units.push(WordUnit {
            surface: word.clone(),
            lemma,
            confidence,
            ambiguity,
        });
    }

    let trimmed = normalized_text.trim();
    let clause_type = if trimmed.ends_with('?') {
        ClauseType::Interrogative
    } else if trimmed.ends_with('!') {
        ClauseType::Exclamatory
    } else if words.first().is_some_and(|first| {
        [
            "расскажи",
            "объясни",
            "покажи",
            "дай",
            "напиши",
            "назови",
            "перечисли",
        ]
        .contains(&first.as_str())
    }) {
        ClauseType::Imperative
    } else {
        ClauseType::Declarative
    };

    let is_greeting_shape = words.len() <= 3
        && words
            .iter()
            .any(|word| GREETING_MARKERS.contains(&word.as_str()));
    let has_second_person = words.iter().any(|word| word == "ты" || word == "вы");
    let has_mental_verb = words
        .iter()
        .any(|word| MENTAL_VERBS.contains(&word.as_str()));
    let interrogative = clause_type == ClauseType::Interrogative;

    let (speech_act, route_hint, confidence): (_, _, f64) = if is_greeting_shape {
        (SpeechAct::Greet, Some(PropositionMode::Greeting), 1.0)
    } else if has_second_person && has_mental_verb && interrogative {
        (SpeechAct::Ask, Some(PropositionMode::Reflect), 0.9)
    } else if interrogative {
        (SpeechAct::Ask, None, 0.5)
    } else {
        (SpeechAct::Tell, None, 0.5)
    };

    let polarity = if words.iter().any(|word| {
        word == "не" || word == "нет" || word.starts_with("ни") || word.starts_with("недо")
    }) {
        Polarity::Negative
    } else {
        Polarity::Affirmative
    };

    // Explicit prepositional mention: про/о/об/насчёт X.
    let mut topic = None;
    let mut topic_confidence: f64 = 0.0;
    for (index, word) in words.iter().enumerate() {
        if word == "про" || word == "о" || word == "об" || word == "насчёт" {
            if let Some(mention) = words.get(index + 1) {
                topic = Some(mention.clone());
                topic_confidence = 0.7;
                break;
            }
        }
    }

    // Focus: first content token after a sentence-initial interrogative,
    // skipping scaffolding (`такое`, `есть`, `это` are not emphasis).
    let focus = if words
        .first()
        .is_some_and(|first| INTERROGATIVES.contains(&first.as_str()))
    {
        words
            .iter()
            .skip(1)
            .find(|word| {
                word.chars().count() >= 3
                    && !INTERROGATIVES.contains(&word.as_str())
                    && !["такое", "есть", "это"].contains(&word.as_str())
            })
            .cloned()
    } else {
        None
    };

    let agent = words
        .iter()
        .any(|word| word == "я" || word == "меня" || word == "мне")
        .then(|| "я".to_string());
    let target = words
        .iter()
        .any(|word| word == "ты" || word == "вы")
        .then(|| "ты".to_string());

    let confidence = confidence.max(topic_confidence).max(match &focus {
        Some(_) => 0.6,
        None => 0.0,
    });

    InputFrame {
        normalized_text,
        units,
        clause_type,
        speech_act,
        polarity,
        topic,
        focus,
        agent,
        target,
        route_hint: route_hint.filter(|_| confidence >= ROUTE_HINT_FLOOR),
        confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_hint_matches_the_legacy_shape() {
        let frame = frame_input("Привет!");
        assert_eq!(frame.clause_type, ClauseType::Exclamatory);
        assert_eq!(frame.speech_act, SpeechAct::Greet);
        assert_eq!(frame.route_hint, Some(PropositionMode::Greeting));
        assert_eq!(frame.units.len(), 1);
        assert!(frame.units.iter().all(|unit| !unit.lemma.is_empty()
            && !unit.ambiguity.is_empty()
            && (0.0..=1.0).contains(&unit.confidence)));
    }

    #[test]
    fn mental_verb_reflect_is_new_signal() {
        // The legacy cascade has no `об`-shape here and would not route
        // Reflect; the frame sees a second-person mental question.
        let frame = frame_input("ты помнишь меня?");
        assert_eq!(frame.clause_type, ClauseType::Interrogative);
        assert_eq!(frame.route_hint, Some(PropositionMode::Reflect));
        assert_eq!(frame.target.as_deref(), Some("ты"));
        assert_eq!(frame.polarity, Polarity::Affirmative);
    }

    #[test]
    fn plain_questions_stay_hintless() {
        let frame = frame_input("что такое свобода?");
        assert_eq!(frame.route_hint, None, "legacy detectors decide");
        assert_eq!(frame.focus.as_deref(), Some("свобода"));
        assert_eq!(frame.polarity, Polarity::Affirmative);
    }

    #[test]
    fn polarity_and_topic_and_agent() {
        let frame = frame_input("я не согласен про свободу");
        assert_eq!(frame.polarity, Polarity::Negative);
        assert_eq!(frame.topic.as_deref(), Some("свободу"));
        assert_eq!(frame.agent.as_deref(), Some("я"));
        assert_eq!(frame.route_hint, None);
        // Double negation stays Negative: recorded limitation.
        let double = frame_input("я не не согласен");
        assert_eq!(double.polarity, Polarity::Negative);
    }

    #[test]
    fn every_token_yields_exactly_one_unit() {
        // Punctuation is not a token; words and numbers are — even
        // off-dictionary (surface fallback, confidence 0.5).
        let frame = frame_input("абракадабра 123 !");
        assert_eq!(frame.units.len(), 2);
        assert!(frame.units.iter().all(|unit| !unit.lemma.is_empty()));
    }
}
