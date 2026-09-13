//! Content saliency and the concentrate rule (density doctrine).
//!
//! Two related instruments, both deterministic and total:
//!
//! - [`content_novelty`]: the top-down saliency signal — the share of the
//!   prompt's content words the topic's admitted surfaces do not cover.
//!   A bare `что такое X?` scores ~0 (nothing new said); substantive
//!   practice entries score mid; unknown territory (no admitted
//!   surfaces) scores 1. Feeds the V2 salience controller as
//!   `content_saliency`. Deliberately coarse (word-level, no morphology):
//!   it measures *coverage*, not understanding — the first reviewable
//!   top-down signal, replacing the reserved 0.0.
//! - [`concentrate`]: the receiver-under-pressure rule (Haskell
//!   `decompressForReceiver` analog, adapted): past the arousal gate the
//!   response collapses to its densest sentence. Haskell keeps the
//!   *first* sentence (densest by move-layer construction); Rust legacy
//!   output leads with an intro, so density is measured, not assumed:
//!   content-word ratio, ties to the first. The audited path never
//!   concentrates (curated text is editorial, not eligible).

/// Interrogative and pronominal scaffolding: present in almost every
/// prompt, carrying no topic content. Kept small and reviewable; any
/// change re-pins the golden tests below.
const SCAFFOLDING: &[&str] = &[
    "что",
    "такое",
    "есть",
    "это",
    "этот",
    "эта",
    "эти",
    "как",
    "какой",
    "какая",
    "какое",
    "какие",
    "кто",
    "где",
    "когда",
    "почему",
    "зачем",
    "меня",
    "мне",
    "тебя",
    "тебе",
    "себя",
    "такой",
    "такая",
    "ли",
];

/// Lowercase alphanumeric tokens of length ≥ 3 outside the scaffolding.
pub fn content_tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() >= 3)
        .filter(|token| !SCAFFOLDING.contains(token))
        .map(str::to_string)
        .collect()
}

/// Share of the prompt's content words absent from every admitted
/// surface (case-insensitive substring). `0.0` when the prompt carries
/// no content words; `1.0` when nothing is admitted (unknown territory
/// is salient — it needs attention). Always in `[0, 1]`.
pub fn content_novelty(raw_text: &str, admitted_surfaces: &[&str]) -> f64 {
    let tokens = content_tokens(raw_text);
    if tokens.is_empty() {
        return 0.0;
    }
    if admitted_surfaces.is_empty() {
        return 1.0;
    }
    let haystacks: Vec<String> = admitted_surfaces
        .iter()
        .map(|surface| surface.to_lowercase())
        .collect();
    let uncovered = tokens
        .iter()
        .filter(|token| {
            !haystacks
                .iter()
                .any(|haystack| haystack.contains(token.as_str()))
        })
        .count();
    uncovered as f64 / tokens.len() as f64
}

/// Content-word density of one sentence, Laplace-smoothed (+2): a bare
/// ratio crowns trivial sentences (`Понятно?` scores 1.0), while the
/// smoothing asks for actual mass — the sentence with the most meaning
/// per word *among sentences that say something*.
fn sentence_density(sentence: &str) -> f64 {
    let total = sentence
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .count();
    if total == 0 {
        return 0.0;
    }
    content_tokens(sentence).len() as f64 / (total as f64 + 2.0)
}

/// Receiver-under-pressure gate, mirroring the Haskell 0.6 threshold.
pub const CONCENTRATE_AROUSAL_GATE: f64 = 0.6;

/// Collapse a multi-sentence legacy response to its densest sentence
/// past the arousal gate; byte-identical input below it. Single
/// sentences and empty text pass through; the winner keeps its terminal
/// punctuation, ties go to the first sentence. Guard rails: the winner
/// must share a lemma with the topic — a concentrate that drops the
/// topic would trip the guard's zero-overlap block, so without overlap
/// the full text stands (an explicit, reviewable no-op).
pub fn concentrate(text: &str, arousal: f64, topic: &str) -> String {
    if arousal <= CONCENTRATE_AROUSAL_GATE {
        return text.to_string();
    }
    let sentences = split_sentences(text);
    if sentences.len() < 2 {
        return text.to_string();
    }
    let topic_lemmas = lemmatized_content(topic);
    let mut best: Option<&str> = None;
    let mut best_density = 0.0;
    for candidate in &sentences {
        if !shares_lemma(candidate, &topic_lemmas) {
            continue;
        }
        let density = sentence_density(candidate);
        if density > best_density {
            best = Some(candidate);
            best_density = density;
        }
    }
    let Some(winner) = best else {
        return text.to_string();
    };
    let trimmed = winner.trim();
    if trimmed.ends_with(['.', '!', '?']) {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

/// Lemmatized content tokens (unresolvable words stay as-is — the same
/// fallback the resolver uses, so overlap never depends on dictionary
/// luck alone).
fn lemmatized_content(text: &str) -> Vec<String> {
    let runtime = qxfx0_morphology::get_runtime();
    content_tokens(text)
        .iter()
        .map(|word| match runtime.lemmatize(word) {
            qxfx0_types::morphology::MorphologyLookup::Resolved(result) => result.lemma,
            _ => word.clone(),
        })
        .collect()
}

/// True when the sentence shares a lemma with the topic, or carries the
/// raw topic string (unlemmatizable topics still match verbatim).
fn shares_lemma(sentence: &str, topic_lemmas: &[String]) -> bool {
    if topic_lemmas.is_empty() {
        return true;
    }
    let lowered = sentence.to_lowercase();
    let sentence_lemmas = lemmatized_content(sentence);
    topic_lemmas.iter().any(|lemma| {
        lowered.contains(lemma.as_str()) || sentence_lemmas.iter().any(|word| word == lemma)
    })
}

/// Sentence splitter shared with the composer: breaks on `.`/`!`/`?`
/// except common Russian abbreviations.
fn split_sentences(text: &str) -> Vec<&str> {
    // Mirrors `discourse_composer::split_sentences` boundaries without
    // depending on its private helper: keep the two in lockstep by test
    // below (same outputs on the abbreviation battery).
    const ABBREVIATIONS: &[&str] = &[
        "т.д", "т.п", "т.е", "г", "ул", "пр", "им", "см", "напр", "стр",
    ];
    let mut sentences = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'.' || byte == b'!' || byte == b'?' {
            let mut end = index + 1;
            while end < bytes.len() && bytes[end] == b'.' {
                end += 1;
            }
            let segment = text[start..end].trim();
            let is_abbreviation = ABBREVIATIONS.iter().any(|abbr| segment.ends_with(abbr));
            if !is_abbreviation {
                if !segment.is_empty() {
                    sentences.push(segment);
                }
                start = end;
            }
            index = end;
        } else {
            index += 1;
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        sentences.push(tail);
    }
    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn novelty_separates_bare_substantive_and_unknown() {
        let admitted = [
            "свобода предполагает возможность выбора",
            "не любой выбор свободен",
        ];
        // Bare topic question: only the topic word survives the
        // scaffolding, and it is covered.
        assert_eq!(content_novelty("что такое свобода?", &admitted), 0.0);
        // Substantive entry: novel claims about the covered topic.
        let mid = content_novelty("свобода без ответственности это произвол", &admitted);
        assert!(mid > 0.3 && mid < 1.0, "novelty={mid}");
        // Unknown territory: everything is novel.
        assert_eq!(content_novelty("что такое ксеномодус?", &[]), 1.0);
        // No content at all: nothing to be novel about.
        assert_eq!(content_novelty("что?", &admitted), 0.0);
    }

    #[test]
    fn concentrate_gates_and_picks_density() {
        let text = "Давай разберёмся. Свобода предполагает возможность выбора и требует осознанности. Понятно?";
        assert_eq!(concentrate(text, 0.6, "свобода"), text, "gate is strict");
        assert_eq!(
            concentrate(text, 0.2, "свобода"),
            text,
            "below gate unchanged"
        );
        assert_eq!(
            concentrate(text, 0.9, "свобода"),
            "Свобода предполагает возможность выбора и требует осознанности.",
            "densest sentence wins, intro loses"
        );
        assert_eq!(
            concentrate("Одно предложение.", 0.9, "смысл"),
            "Одно предложение."
        );
        assert_eq!(concentrate("", 0.9, "смысл"), "");
    }

    #[test]
    fn concentrate_keeps_topic_overlap_or_stands_down() {
        // Densest sentence without the topic: full text stands, so the
        // guard's zero-overlap block can never fire on a concentrate.
        let text = "Давай разберёмся. Всё сложно и запутано очень. Свобода.";
        assert_eq!(
            concentrate(text, 0.9, "справедливость"),
            text,
            "no overlap anywhere: no-op"
        );
        // Overlap in an inflected form still counts (lemma match).
        let inflected = "Давай разберёмся. О справедливости сказано много верного. Понятно?";
        assert_eq!(
            concentrate(inflected, 0.9, "справедливость"),
            "О справедливости сказано много верного."
        );
    }
}
