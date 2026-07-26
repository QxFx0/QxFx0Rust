/// Heuristics for evaluating the quality and safety of generated content.
pub const HISTORY_LOOKBACK: usize = 5;
pub const REPETITION_THRESHOLD: usize = 3;

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
    for ph in PLACEHOLDERS {
        if rendered.contains(ph) {
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
    let lower_trimmed = rendered.trim().to_lowercase();
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

    let topic_tokens: Vec<&str> = topic.split_whitespace().filter(|t| t.len() >= 3).collect();
    let lower = rendered.to_lowercase();
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
        Some(format!(
            "нулевое совпадение с темой: {}",
            topic.to_lowercase()
        ))
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
            let t = t.trim_matches(|c: char| !c.is_alphabetic());
            t.len() >= 2 && !is_stop_word(t)
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
    let found: Vec<_> = PLACEHOLDERS
        .iter()
        .filter(|p| rendered.contains(*p))
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
    let lower = rendered.to_lowercase();
    let mut found_toxic: Vec<&str> = Vec::new();

    for phrase in &toxic {
        let phrase_tokens: Vec<&str> = phrase.split_whitespace().collect();
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
    let normalized = rendered.trim().to_lowercase();
    let match_count = history
        .iter()
        .rev()
        .take(HISTORY_LOOKBACK)
        .filter(|h| h.trim().to_lowercase() == normalized)
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
