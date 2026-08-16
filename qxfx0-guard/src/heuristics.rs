/// Heuristics for evaluating the quality and safety of generated content.
pub const HISTORY_LOOKBACK: usize = 5;
pub const REPETITION_THRESHOLD: usize = 3;

/// Canonical form used by every guard heuristic: Unicode-lowercased,
/// zero-width and combining characters removed, and the common Latin
/// lookalikes of Cyrillic letters folded to their Cyrillic equivalents.
/// Case tricks, invisible characters and homoglyph substitution must not
/// bypass a safety or quality check.
pub fn canonicalize(text: &str) -> String {
    let mut canonical = String::with_capacity(text.len());
    for character in text.chars() {
        for lower in character.to_lowercase() {
            if let Some(folded) = fold_canonical(lower) {
                canonical.push(folded);
            }
        }
    }
    canonical
}

/// Drop invisible characters, fold Latin/Cyrillic homoglyphs.
fn fold_canonical(character: char) -> Option<char> {
    match character {
        // Zero-width and invisible formatting characters.
        '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}' => None,
        // Combining marks (diacritics), including Cyrillic ranges.
        '\u{0300}'..='\u{036F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE20}'..='\u{FE2F}' => None,
        // Latin glyphs visually identical to Cyrillic letters.
        'a' => Some('а'),
        'c' => Some('с'),
        'e' => Some('е'),
        'o' => Some('о'),
        'p' => Some('р'),
        'x' => Some('х'),
        'y' => Some('у'),
        _ => Some(character),
    }
}

const PLACEHOLDERS: &[&str] = &[
    "{FROM}",
    "{TO",
    "{OBJ",
    "{RATIONALE}",
    "{SYNTHESIS}",
    "{FROM_G:",
    "{TO_G:",
    "{OBJ_G:",
];

/// Check for unfilled template placeholders.
pub fn check_template_placeholders(rendered: &str) -> Option<String> {
    let canonical = canonicalize(rendered);
    for ph in PLACEHOLDERS {
        if canonical.contains(&canonicalize(ph)) {
            return Some(format!("незаполненный шаблон: {}", ph));
        }
    }
    None
}

/// Check for generic filler responses.
pub fn check_generic_fillers(rendered: &str) -> Option<String> {
    let fillers = [
        "я не знаю что сказать",
        "произошла ошибка",
        "не удалось сгенерировать ответ",
        "[пусто]",
        "[нет данных]",
        "понятно.",
        "я понимаю.",
    ];
    let lower_trimmed = canonicalize(rendered.trim());
    for filler in &fillers {
        if lower_trimmed.starts_with(filler) {
            return Some("генерический filler-ответ".into());
        }
    }
    None
}

/// Check if the output is relevant to the given topic.
pub fn check_topic_relevance(topic: &str, rendered: &str) -> Option<String> {
    if topic.is_empty() {
        return None;
    }

    let canonical_topic = canonicalize(topic);
    let topic_tokens: Vec<&str> = canonical_topic
        .split_whitespace()
        .filter(|t| t.len() >= 3)
        .collect();
    let lower = canonicalize(rendered);
    let has_overlap = topic_tokens.iter().any(|t| {
        if lower.contains(t) {
            return true;
        }
        let stripped = t.trim_end_matches(|c: char| !c.is_alphabetic());
        let chars: Vec<char> = stripped.chars().collect();
        let char_len = chars.len();
        if char_len >= 5 {
            let stem: String = chars[..char_len - 1].iter().collect();
            if lower.contains(&stem) {
                return true;
            }
            if char_len >= 6 {
                let stem2: String = chars[..char_len - 2].iter().collect();
                lower.contains(&stem2)
            } else {
                false
            }
        } else {
            false
        }
    });

    if !has_overlap {
        Some(format!("нулевое совпадение с темой: {}", canonical_topic))
    } else {
        None
    }
}

/// Check for content density.
pub fn check_content_density(tokens: &[&str]) -> Option<String> {
    if tokens.len() < 16 {
        return None;
    }

    let content_words = tokens
        .iter()
        .filter(|t| {
            let t = canonicalize(t.trim_matches(|c: char| !c.is_alphabetic()));
            t.chars().count() >= 2 && !is_stop_word(&t)
        })
        .count();
    let density = content_words as f64 / tokens.len() as f64;
    if density < 0.15 {
        Some("низкая плотность содержания".into())
    } else {
        None
    }
}

/// Check for semantic saturation (repetition of bigrams).
pub fn check_semantic_saturation(tokens: &[&str]) -> Option<String> {
    if tokens.len() < 20 {
        return None;
    }

    let mut unique: std::collections::BTreeSet<(&str, &str)> = std::collections::BTreeSet::new();
    let mut total: usize = 0;
    for w in tokens.windows(2) {
        if w.len() == 2 {
            unique.insert((w[0], w[1]));
            total += 1;
        }
    }
    if total > 0 {
        let repeat_ratio = 1.0 - unique.len() as f64 / total as f64;
        if repeat_ratio > 0.8 {
            return Some("высокая повторяемость".into());
        }
    }
    None
}

/// Check for metadata leaks.
pub fn check_metadata_leaks(rendered: &str) -> Option<String> {
    let canonical = canonicalize(rendered);
    let found: Vec<_> = PLACEHOLDERS
        .iter()
        .filter(|p| canonical.contains(&canonicalize(p)))
        .collect();
    if !found.is_empty() {
        Some(format!(
            "утечка метаданных: {}",
            found.iter().map(|s| **s).collect::<Vec<_>>().join(", ")
        ))
    } else {
        None
    }
}

/// Check for toxic patterns.
pub fn check_toxicity(rendered: &str) -> Option<String> {
    let toxic = [
        "ты должен",
        "ты обязан",
        "это глупо",
        "ты неправ",
        "бред",
        "чушь",
        "идиот",
        "тупой",
    ];
    let lower = canonicalize(rendered);
    let mut found_toxic: Vec<&str> = Vec::new();

    for phrase in &toxic {
        let canonical_phrase = canonicalize(phrase);
        let phrase_tokens: Vec<&str> = canonical_phrase.split_whitespace().collect();
        if phrase_tokens.is_empty() {
            continue;
        }

        let matched = if phrase_tokens.len() == 1 {
            lower
                .split(|c: char| !c.is_alphanumeric())
                .any(|t| t == phrase_tokens[0])
        } else {
            let mut tokens = lower
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty());
            let mut window: Vec<&str> = Vec::with_capacity(phrase_tokens.len());

            if let Some(first) = tokens.next() {
                window.push(first);
                let mut found = false;
                for next in tokens {
                    window.push(next);
                    if window.len() == phrase_tokens.len() {
                        if window
                            .iter()
                            .zip(phrase_tokens.iter())
                            .all(|(a, b)| *a == *b)
                        {
                            found = true;
                            break;
                        }
                        window.remove(0);
                    }
                }
                found
            } else {
                false
            }
        };

        if matched {
            found_toxic.push(phrase);
        }
    }

    if !found_toxic.is_empty() {
        Some(format!("токсичные паттерны: {}", found_toxic.join(", ")))
    } else {
        None
    }
}

/// Check for stuck repetition in history.
pub fn check_stuck_repetition(rendered: &str, history: &[String]) -> Option<String> {
    let normalized = canonicalize(rendered.trim());
    let match_count = history
        .iter()
        .rev()
        .take(HISTORY_LOOKBACK)
        .filter(|h| canonicalize(h.trim()) == normalized)
        .count();
    if match_count >= REPETITION_THRESHOLD {
        Some("застревание на повторе".into())
    } else {
        None
    }
}

fn is_stop_word(word: &str) -> bool {
    const STOP_WORDS: &[&str] = &[
        "что",
        "это",
        "как",
        "так",
        "его",
        "ей",
        "этом",
        "этот",
        "эта",
        "эти",
        "для",
        "при",
        "или",
        "но",
        "не",
        "ни",
        "же",
        "ли",
        "бы",
        "то",
        "вот",
        "там",
        "тут",
        "где",
        "когда",
        "потому",
        "потому что",
        "если",
        "чтобы",
        "все",
        "всё",
        "всех",
        "всего",
        "еще",
        "ещё",
        "уже",
        "только",
        "было",
        "будет",
        "есть",
        "нет",
        "да",
        "над",
        "под",
        "за",
        "из",
        "от",
        "до",
        "по",
        "в",
        "с",
        "к",
        "у",
        "о",
        "об",
        "и",
        "а",
        "ну",
        "вы",
        "ты",
        "он",
        "она",
        "оно",
        "они",
        "мы",
        "мой",
        "моя",
        "твой",
        "твоя",
        "свой",
        "своя",
        "их",
        "наш",
        "ваш",
        "который",
        "которая",
        "которое",
        "которые",
        "тобой",
        "тому",
        "тем",
        "сам",
        "сама",
        "само",
        "сами",
        "один",
        "одна",
        "одно",
        "два",
        "три",
    ];
    STOP_WORDS.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_lowercases_and_strips_invisible_characters() {
        assert_eq!(canonicalize("Свобода"), "свобода");
        // Zero-width space, ZWNJ, soft hyphen, BOM.
        assert_eq!(canonicalize("св\u{200B}обо\u{200C}да"), "свобода");
        assert_eq!(canonicalize("св\u{00AD}обода"), "свобода");
        assert_eq!(canonicalize("\u{FEFF}свобода"), "свобода");
        // Combining acute accent.
        assert_eq!(canonicalize("свобо\u{0301}да"), "свобода");
    }

    #[test]
    fn canonicalize_folds_latin_homoglyphs_to_cyrillic() {
        assert_eq!(canonicalize("свoбода"), "свобода"); // Latin 'o'
        assert_eq!(canonicalize("тeрпение"), "терпение"); // Latin 'e'
        assert_eq!(canonicalize("Paзум"), "разум"); // Latin 'P' and 'a'
    }

    #[test]
    fn placeholders_are_matched_case_insensitively_and_through_invisible_chars() {
        assert!(check_template_placeholders("{FROM} — это понятие").is_some());
        assert!(check_template_placeholders("{from} — это понятие").is_some());
        assert!(
            check_template_placeholders("{F\u{200B}ROM} — это понятие").is_some(),
            "zero-width characters must not split a placeholder"
        );
        assert!(check_template_placeholders("свобода предполагает выбор").is_none());
    }

    #[test]
    fn metadata_leaks_are_matched_through_case_and_invisible_chars() {
        assert!(check_metadata_leaks("утечка {rationale} в тексте").is_some());
        assert!(check_metadata_leaks("утечка {SYNTH\u{FEFF}ESIS} в тексте").is_some());
        assert!(check_metadata_leaks("чистый текст").is_none());
    }

    #[test]
    fn toxicity_is_matched_through_homoglyphs_and_invisible_chars() {
        assert!(check_toxicity("ты должен это сделать").is_some());
        assert!(
            check_toxicity("ты дoлжен это сделать").is_some(),
            "Latin 'o' must not bypass the toxicity window"
        );
        assert!(
            check_toxicity("т\u{200B}ы должен это сделать").is_some(),
            "zero-width characters must not bypass the toxicity window"
        );
        assert!(
            check_toxicity("ТЫ ОБЯЗАН").is_some(),
            "case must not bypass"
        );
        assert!(check_toxicity("свобода предполагает выбор").is_none());
    }

    #[test]
    fn generic_fillers_are_matched_through_obfuscation() {
        assert!(check_generic_fillers("Понятно.").is_some());
        assert!(check_generic_fillers("Понятно\u{0301}.").is_some());
        assert!(
            check_generic_fillers("пoнятно.").is_some(),
            "Latin 'o' must not bypass the filler check"
        );
        assert!(check_generic_fillers("свобода предполагает выбор").is_none());
    }

    #[test]
    fn topic_relevance_survives_homoglyph_topics() {
        // Topic containing a Latin homoglyph still matches a Cyrillic render.
        assert!(check_topic_relevance("свoбода", "свобода предполагает выбор").is_none());
        assert!(check_topic_relevance("свобода", "рассуждая о свободе").is_none());
        assert!(check_topic_relevance("свобода", "механика шестерёнок").is_some());
    }

    #[test]
    fn stuck_repetition_is_compared_canonically() {
        let history = vec![
            "Свобода предполагает выбор".to_string(),
            "Свобода предполагает выбор".to_string(),
            "Свобода предполагает выбор".to_string(),
        ];
        assert!(check_stuck_repetition("свобода предполагает выбор", &history).is_some());
        let obfuscated = vec![
            "свoбода предполагает выбор".to_string(),
            "свобода предполагает выбор".to_string(),
            "свобода предполагает выбор".to_string(),
        ];
        assert!(
            check_stuck_repetition("свобода предполагает выбор", &obfuscated).is_some(),
            "homoglyph history entries must still count as repeats"
        );
    }

    #[test]
    fn content_density_flags_sparse_output() {
        let sparse: Vec<&str> =
            "что это как так его ей этом этот эта эти для при или но не ни же ли бы то"
                .split_whitespace()
                .collect();
        assert!(
            sparse.len() >= 16,
            "fixture must reach the density threshold"
        );
        assert!(check_content_density(&sparse).is_some());
        let dense: Vec<&str> = "свобода предполагает ответственность перед другими людьми"
            .split_whitespace()
            .collect();
        assert!(check_content_density(&dense).is_none());
    }

    #[test]
    fn semantic_saturation_flags_repeated_bigrams() {
        let tokens: Vec<&str> =
            "дом дом дом дом дом дом дом дом дом дом дом дом дом дом дом дом дом дом дом дом"
                .split_whitespace()
                .collect();
        assert!(check_semantic_saturation(&tokens).is_some());
        let varied: Vec<&str> = "свобода предполагает ответственность ответственность требует различения различение создаёт смысл"
            .split_whitespace()
            .collect();
        assert!(check_semantic_saturation(&varied).is_none());
    }
}
