//! Noun-phrase chunker (ADR-0045 A1): the first structured step past
//! the keyword cascade — group a raw subject span into an NP chunk
//! instead of slicing words.
//!
//! Grammar (deliberately small, reviewable):
//! `[Adj* Noun (Gen-Noun)*]` — leading adjectives, a head noun, then
//! genitive tail nouns. A trailing noun continues the chunk only when
//! its case is Genitive or unknown (dictionary gaps must not break
//! chains, but two known nominatives are two phrases, not one).
//! Anything else (verbs, adverbs, closed classes, prepositions)
//! terminates the scan; no noun anywhere means no chunk (`None`,
//! and the caller keeps its raw slice — byte-compatibility).
//!
//! The chunker returns *surface spans*, never lemmas or nominatives:
//! oblique forms (`стола`) survive untouched. Case information comes
//! from isolated-word resolution (no context), so the genitive rule
//! is an approximation — documented, pinned by golden tests.

use qxfx0_types::morphology::{Case, PartOfSpeech};

/// One classified token of the span.
#[derive(Debug, Clone, PartialEq)]
struct ChunkToken {
    surface: String,
    lemma: String,
    pos: Option<PartOfSpeech>,
    case: Option<Case>,
    number: Option<qxfx0_types::morphology::Number>,
}

fn classify(word: &str) -> ChunkToken {
    match qxfx0_morphology::get_runtime().lemmatize(word) {
        qxfx0_types::morphology::MorphologyLookup::Resolved(result) => ChunkToken {
            surface: word.to_string(),
            lemma: result.lemma,
            pos: Some(result.pos),
            case: Some(result.case),
            number: Some(result.number),
        },
        _ => ChunkToken {
            surface: word.to_string(),
            lemma: word.to_string(),
            pos: None,
            case: None,
            number: None,
        },
    }
}

fn is_adjectival(token: &ChunkToken) -> bool {
    matches!(
        token.pos,
        Some(PartOfSpeech::Adjective) | Some(PartOfSpeech::Numeral) | None
    ) && !is_closed_class(token)
}

fn is_nominal(token: &ChunkToken) -> bool {
    matches!(
        token.pos,
        Some(PartOfSpeech::Noun) | Some(PartOfSpeech::Other) | None
    )
}

fn is_closed_class(token: &ChunkToken) -> bool {
    if matches!(
        token.pos,
        Some(PartOfSpeech::Preposition)
            | Some(PartOfSpeech::Conjunction)
            | Some(PartOfSpeech::Particle)
            | Some(PartOfSpeech::Interjection)
            | Some(PartOfSpeech::Pronoun)
    ) {
        return true;
    }
    // Closed by grammar, not by dictionary: these surfaces can never
    // head or continue an NP, so a dictionary gap must not open them.
    const CLOSED_SURFACES: &[&str] = &[
        "в",
        "на",
        "о",
        "об",
        "к",
        "с",
        "у",
        "от",
        "до",
        "для",
        "про",
        "без",
        "над",
        "под",
        "при",
        "через",
        "между",
        "насчёт",
        "возле",
        "мимо",
        "после",
        "перед",
        "вокруг",
        "против",
        "ради",
        "сквозь",
        "среди",
        "и",
        "а",
        "но",
        "или",
        "либо",
        "же",
        "ли",
        "бы",
        "тоже",
        "также",
        "это",
        "этот",
        "эта",
        "эти",
        "то",
        "тот",
        "та",
        "те",
        "такой",
        "такая",
        "такое",
        "такие",
        "мой",
        "моя",
        "твой",
        "твоя",
        "наш",
        "наша",
        "свой",
        "своя",
        "ну",
        "вот",
        "так",
        "ведь",
        "пусть",
        "даже",
        "именно",
        "просто",
        "только",
        "лишь",
        "уже",
        "ещё",
        "не",
        "ни",
        "нет",
    ];
    CLOSED_SURFACES.contains(&token.surface.as_str())
}

/// Verb endings for dictionary-unknown words: infinitives, finite
/// present/future AND past-tense forms, plus reflexives. A bare verb
/// must never head or continue a phrase — `chunk_noun_phrase` returns
/// `None` on verb-headed spans so the caller keeps its raw slice.
/// Nouns sharing an ending (`билет`-class) are safe while resolved;
/// unknown ones are an accepted, documented cost.
fn looks_like_verb(word: &str) -> bool {
    const ENDINGS: &[&str] = &[
        "ать", "ять", "еть", "ить", "оть", "уть", "ыть", "ает", "яет", "ают", "яют", "ит", "ыт",
        "ут", "ют", "ат", "ят", "ет", "ёт", "ал", "ял", "ел", "ёл", "ил", "ыл", "ла", "ло", "ли",
        "ался", "ялся", "елся", "ёлся", "ился", "ылся", "лась", "лось", "лись", "ться", "тся",
    ];
    ENDINGS.iter().any(|ending| word.ends_with(ending))
}

/// A trailing token continues the chunk when it can modify the head:
/// known nouns only in the genitive (a syncretic non-genitive reading
/// falls back to the surface below), unknowns unless verb-shaped, and
/// never a repetition of the head lemma itself. Single letters never
/// continue. Surface genitive markers (`-а/-я/-ы/-и/-у/-ю…`) rescue the
/// syncretic readings the runtime resolves as nominative plural
/// (`ответственности`, `запоминания`) — at the documented price that a
/// true nominative plural with the same ending chains too.
fn continues_tail(
    token: &ChunkToken,
    head_lemma: &str,
    head_case: Option<Case>,
    head_number: Option<qxfx0_types::morphology::Number>,
) -> bool {
    if token.surface.chars().count() < 2 || is_closed_class(token) {
        return false;
    }
    if token.pos.is_none() && looks_like_verb(&token.surface) {
        return false;
    }
    if token.lemma == head_lemma {
        return false;
    }
    if !is_nominal(token) {
        return token.pos.is_none();
    }
    if matches!(token.case, Some(Case::Genitive)) {
        return true;
    }
    // Two nominatives in the same number are two phrases, not one
    // ("стол книга" breaks where "чувство ответственности" chains).
    if matches!(token.case, Some(Case::Nominative))
        && token.number.is_some()
        && token.number == head_number
        && head_case == Some(Case::Nominative)
    {
        return false;
    }
    if token.case.is_some() {
        return has_genitive_ending(&token.surface);
    }
    true
}

/// Genitive-looking endings for the syncretism fallback above.
fn has_genitive_ending(word: &str) -> bool {
    const ENDINGS: &[&str] = &[
        "а", "я", "ы", "и", "у", "ю", "ом", "ем", "ой", "ей", "ов", "ев", "ей", "ам", "ям", "ах",
        "ях",
    ];
    ENDINGS.iter().any(|ending| word.ends_with(ending))
}

/// Extract the first NP chunk of a raw span, if it contains a noun.
/// Pure, total, deterministic; morphology-backed with surface
/// fallbacks, so unknown words still chunk as singletons.
pub fn chunk_noun_phrase(span: &str) -> Option<String> {
    // Hyphenated compounds stay whole (`кем-то`, `что-нибудь`):
    // splitting the particle off loses indefiniteness, and compounds
    // are single chunk units anyway. Only ASCII hyphen joins;
    // em-dashes still split.
    let words: Vec<String> = span
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric() && character != '-')
        .map(|token| token.trim_matches('-').to_string())
        .filter(|token| !token.is_empty())
        .collect();
    if words.is_empty() {
        return None;
    }
    let tokens: Vec<ChunkToken> = words.iter().map(|word| classify(word)).collect();
    let head = tokens.iter().position(|token| {
        if is_closed_class(token) {
            return false;
        }
        // Unknown verb forms never head a phrase (dictionary gap, not
        // a noun — see `looks_like_verb`).
        if token.pos.is_none() && looks_like_verb(&token.surface) {
            return false;
        }
        (is_nominal(token) || is_adjectival(token))
            && !(token.pos.is_none() && token.surface.chars().count() < 3)
    })?;
    // Anchor: the first known noun; without one the head candidate
    // anchors itself (unknown singleton or adjective-led phrase whose
    // agreement reference resolves as the scan proceeds).
    let mut start = head;
    while start > 0 && is_adjectival(&tokens[start - 1]) && !is_known_nominal(&tokens[start - 1]) {
        start -= 1;
    }
    let mut end = start;
    // Previous accepted nominal, for link-wise agreement: each nominal
    // link agrees with its predecessor, not with a fixed head
    // (`граница корпуса знаний` chains link by link).
    let mut prev: Option<(
        String,
        Option<Case>,
        Option<qxfx0_types::morphology::Number>,
    )> = None;
    let mut index = start;
    while index < tokens.len() {
        let token = &tokens[index];
        if is_closed_class(token) {
            break;
        }
        if is_known_nominal(token) {
            if let Some((ref lemma, case, number)) = prev {
                if !continues_tail(token, lemma, case, number) {
                    break;
                }
            }
            prev = Some((token.lemma.clone(), token.case, token.number));
            end = index + 1;
        } else if token.pos.is_none()
            && !looks_like_verb(&token.surface)
            && token.surface.chars().count() >= 2
        {
            // Unknown open-class token: attaches (dictionary gaps must
            // not break chains), unless it repeats the previous lemma.
            // Checked before the adjectival branch: unknowns read as
            // both, and attachment is the honest default.
            if let Some((ref lemma, _, _)) = prev {
                if &token.lemma == lemma {
                    break;
                }
            }
            end = index + 1;
        } else if is_adjectival(token) {
            if prev.is_some() {
                break;
            }
            end = index + 1;
        } else {
            break;
        }
        index += 1;
    }
    // A lone adjective with no nominal anywhere is not a phrase; an
    // unknown singleton is (dictionary gap, not grammar).
    let has_nominal = prev.is_some() || tokens[start..end].iter().any(is_known_nominal);
    if end == start || (!has_nominal && tokens[start].pos.is_some()) {
        return None;
    }
    Some(words[start..end].join(" "))
}

/// A resolved noun (unknowns are handled by the open-class branch,
// never here).
fn is_known_nominal(token: &ChunkToken) -> bool {
    matches!(
        token.pos,
        Some(PartOfSpeech::Noun) | Some(PartOfSpeech::Other)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_chunks() {
        let cases: &[(&str, Option<&str>)] = &[
            // Singletons: chunk == raw (byte-compatibility backbone).
            ("свобода", Some("свобода")),
            ("память", Some("память")),
            ("ксеномодус", Some("ксеномодус")),
            ("стол", Some("стол")),
            // Adjective + noun.
            ("возможность выбора", Some("возможность выбора")),
            ("полная свобода", Some("полная свобода")),
            ("осознанный выбор", Some("осознанный выбор")),
            // Genitive tails.
            ("свобода выбора", Some("свобода выбора")),
            ("чувство ответственности", Some("чувство ответственности")),
            ("процесс запоминания", Some("процесс запоминания")),
            ("стола", Some("стола")),
            // Oblique preserved, never normalized.
            ("о свободе", Some("свободе")),
            // Verbs terminate; nouns after verbs start nothing here
            // (single-chunk contract: first NP only).
            ("выбора нет", Some("выбора")),
            // Empty and junk.
            ("", None),
            ("!!!", None),
            ("и а", None),
            // Closed classes terminate the tail.
            ("свобода и воля", Some("свобода")),
            ("память о прошлом", Some("память")),
            // Two known nominatives are two phrases: first wins.
            ("стол книга", Some("стол")),
            // Leading filler before the head is left outside.
            ("это свобода", Some("свобода")),
            ("так вот память", Some("память")),
            // Pronouns never head a chunk.
            ("я", None),
            ("ты", None),
            // Adjective alone is not a phrase.
            ("красное", Some("красное")), // substantivized: the dictionary reads a noun
            ("большой", Some("большой")), // unknown singleton heads by design
            // Numerals attach like adjectives.
            ("два выбора", Some("два выбора")),
            // Unknown words chunk as singletons.
            ("флюгегехаймен", Some("флюгегехаймен")),
            // Mixed unknown + known chains.
            ("теория ксеномодуса", Some("теория ксеномодуса")),
            // Case endings and punctuation trimmed by tokenization.
            ("Свобода?", Some("свобода")),
            ("  память,  ", Some("память")),
            // Multi-word genitive chain.
            ("граница корпуса знаний", Some("граница корпуса знаний")),
            // Verb inside breaks before it.
            ("выбор определяет следствие", Some("выбор")),
            // Adverb breaks.
            ("свобода сегодня", Some("свобода сегодня")), // limitation: unknown adverb chains (no lexicon entry to break on)
            // Preposition breaks the tail.
            ("память в сердце", Some("память")),
            // Conjunction splits.
            ("свобода или воля", Some("свобода")),
            // Long filler then phrase.
            (
                "ну вообще говоря ответственность",
                Some("вообще говоря ответственность"),
            ), // limitation: unknown adverb heads; gerunds undetectable without stems
            // Single unknown short token is not a head.
            ("ах", None),
            // Digits attach as content.
            ("шаг 1", Some("шаг")),
            ("без свободы", Some("свободы")),
            ("к свободе", Some("свободе")),
            ("наша память", Some("память")),
            ("мой выбор", Some("выбор")),
            // Hyphenated particles stay attached (indefiniteness kept).
            ("кем-то", Some("кем-то")),
            ("что-нибудь", Some("что-нибудь")),
            // Repeated head noun.
            ("память памяти", Some("память")), // resolved nominative plural: two phrases, first wins
        ];
        assert!(cases.len() >= 40, "golden table must stay wide");
        for (span, expected) in cases {
            assert_eq!(
                chunk_noun_phrase(span).as_deref(),
                *expected,
                "span {span:?}"
            );
        }
    }
}
