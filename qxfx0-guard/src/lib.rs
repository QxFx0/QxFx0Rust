use qxfx0_types::system_state::GuardStatus;

/// Number of recent history items considered for the stuck-repetition check.
const HISTORY_LOOKBACK: usize = 5;
/// Threshold (matches within HISTORY_LOOKBACK) at which the advisory fires.
const REPETITION_THRESHOLD: usize = 3;

/// Content quality gate — evaluates rendered text for semantic content.
/// Blocking (fail-closed): output replaced with recovery if checks fail.
pub struct ContentQualityGate;

impl ContentQualityGate {
    /// Evaluate content quality with an explicit topic.
    pub fn evaluate(topic: &str, rendered: &str) -> QualityVerdict {
        let trimmed = rendered.trim();

        // Check 1: empty
        if trimmed.is_empty() || trimmed == "..." || trimmed == "?" {
            return QualityVerdict::Block("пустой вывод".into());
        }

        // Check 2: unfilled template placeholders.
        // Use the real template syntax from TemplateRegistry/SyntacticGenerator.
        let placeholders = [
            "{FROM}", "{TO", "{OBJ", "{RATIONALE}", "{SYNTHESIS}", "{FROM_G:", "{TO_G:", "{OBJ_G:",
        ];
        for ph in &placeholders {
            if rendered.contains(ph) {
                return QualityVerdict::Block(format!("незаполненный шаблон: {}", ph));
            }
        }

        // Check 3: generic filler — match prefix or substring to catch
        // variants like "понятно, спасибо." or "я понимаю тебя".
        let fillers = [
            "я не знаю что сказать",
            "произошла ошибка",
            "не удалось сгенерировать ответ",
            "[пусто]",
            "[нет данных]",
            "понятно.",
            "я понимаю.",
        ];
        let lower_trimmed = trimmed.to_lowercase();
        for filler in &fillers {
            let f_lower = filler.to_lowercase();
            if lower_trimmed.starts_with(&f_lower) || lower_trimmed.contains(&f_lower) {
                return QualityVerdict::Block("генерический filler-ответ".into());
            }
        }

        // Check 4: topic relevance — run on any non-empty output.
        // Normalizes case endings: a topic in oblique case (e.g. "ответственности")
        // must still match the nominative form in the response ("ответственность").
        let tokens: Vec<&str> = rendered.split_whitespace().collect();
        if !topic.is_empty() {
            let topic_tokens: Vec<&str> =
                topic.split_whitespace().filter(|t| t.len() >= 3).collect();
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
                return QualityVerdict::Block(format!(
                    "нулевое совпадение с темой: {}",
                    topic.to_lowercase()
                ));
            }
        }

        // Check 5: content density (only for 16+ tokens)
        if tokens.len() >= 16 {
            let content_words = tokens
                .iter()
                .filter(|t| {
                    let t = t.trim_matches(|c: char| !c.is_alphabetic());
                    t.len() >= 2 && !is_stop_word(t)
                })
                .count();
            let density = content_words as f64 / tokens.len() as f64;
            if density < 0.15 {
                return QualityVerdict::Block("низкая плотность содержания".into());
            }
        }

        // Check 6: semantic saturation (only for 20+ tokens)
        if tokens.len() >= 20 {
            // Single-pass: build a BTreeSet of unique bigrams and count
            // total occurrences in one walk, avoiding the intermediate Vec.
            let mut unique: std::collections::BTreeSet<(&str, &str)> =
                std::collections::BTreeSet::new();
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
                    return QualityVerdict::Block("высокая повторяемость".into());
                }
            }
        }

        QualityVerdict::Pass
    }

    /// Convert quality verdict to guard status.
    pub fn to_guard_status(verdict: &QualityVerdict) -> GuardStatus {
        match verdict {
            QualityVerdict::Pass => GuardStatus::InvariantOk,
            QualityVerdict::Block(reason) => GuardStatus::InvariantBlock(reason.clone()),
        }
    }

    /// Post-render safety check — structural checks (empty, metadata, toxicity, length, injection).
    pub fn post_render_safety(rendered: &str, history: &[String]) -> GuardStatus {
        let trimmed = rendered.trim();

        // Empty
        if trimmed.is_empty() || trimmed == "..." || trimmed == "?" {
            return GuardStatus::InvariantBlock("пустой вывод".into());
        }

        // Too long
        if rendered.len() > 5000 {
            return GuardStatus::InvariantBlock("слишком длинный вывод".into());
        }

        // Metadata leak
        let leak_patterns = [
            "{FROM}", "{TO", "{OBJ", "{RATIONALE}", "{SYNTHESIS}", "{FROM_G:", "{TO_G:", "{OBJ_G:",
        ];
        let found: Vec<_> = leak_patterns
            .iter()
            .filter(|p| rendered.contains(*p))
            .collect();
        if !found.is_empty() {
            return GuardStatus::InvariantBlock(format!(
                "утечка метаданных: {}",
                found.iter().map(|s| **s).collect::<Vec<_>>().join(", ")
            ));
        }

        // Toxicity — word-boundary-aware exact phrase matching to avoid
        // false positives like "это глупое недоразумение" matching "это глупо".
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
        let word_tokens: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
        let mut found_toxic: Vec<&str> = Vec::new();
        for phrase in &toxic {
            let phrase_tokens: Vec<&str> = phrase.split_whitespace().collect();
            if phrase_tokens.is_empty() {
                continue;
            }
            // Look for the phrase as a contiguous run of tokens.
            let matched = if phrase_tokens.len() == 1 {
                word_tokens.iter().any(|t| *t == phrase_tokens[0])
            } else {
                word_tokens.windows(phrase_tokens.len()).any(|w| {
                    w.iter().zip(phrase_tokens.iter()).all(|(a, b)| *a == *b)
                })
            };
            if matched {
                found_toxic.push(phrase);
            }
        }
        if !found_toxic.is_empty() {
            return GuardStatus::InvariantBlock(format!(
                "токсичные паттерны: {}",
                found_toxic.join(", ")
            ));
        }

        // Stuck repetition (advisory, not blocking)
        let normalized = lower.trim();
        let match_count = history
            .iter()
            .take(HISTORY_LOOKBACK)
            .filter(|h| h.trim().to_lowercase() == normalized)
            .count();
        if match_count >= REPETITION_THRESHOLD {
            return GuardStatus::InvariantWarn("застревание на повторе".into());
        }

        GuardStatus::InvariantOk
    }

    /// Finalize output — apply safety + quality gates.
    /// Returns (final_text, was_blocked).
    pub fn finalize_output(topic: &str, rendered: &str, history: &[String]) -> (String, bool) {
        let safety = Self::post_render_safety(rendered, history);
        let quality = Self::evaluate(topic, rendered);

        let blocked = matches!(safety, GuardStatus::InvariantBlock(_))
            || matches!(quality, QualityVerdict::Block(_));

        if blocked {
            (
                "Извини, я сейчас перенастраиваю ход мысли. Можем продолжить через секунду?"
                    .to_string(),
                true,
            )
        } else {
            (rendered.to_string(), false)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum QualityVerdict {
    Pass,
    Block(String),
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
    fn test_pass_valid_content() {
        let verdict =
            ContentQualityGate::evaluate("свобода", "свобода предполагает возможность выбора");
        assert_eq!(verdict, QualityVerdict::Pass);
    }

    #[test]
    fn test_block_empty() {
        let verdict = ContentQualityGate::evaluate("свобода", "");
        assert!(matches!(verdict, QualityVerdict::Block(_)));
    }

    #[test]
    fn test_block_template() {
        let verdict = ContentQualityGate::evaluate("свобода", "{FROM} — это понятие");
        assert!(matches!(verdict, QualityVerdict::Block(_)));
    }

    #[test]
    fn test_block_filler() {
        let verdict = ContentQualityGate::evaluate("свобода", "понятно.");
        assert!(matches!(verdict, QualityVerdict::Block(_)));
    }

    #[test]
    fn test_safety_toxic() {
        let status = ContentQualityGate::post_render_safety("ты должен это сделать", &[]);
        assert!(matches!(status, GuardStatus::InvariantBlock(_)));
    }

    #[test]
    fn test_safety_ok() {
        let status = ContentQualityGate::post_render_safety("свобода предполагает выбор", &[]);
        assert_eq!(status, GuardStatus::InvariantOk);
    }

    #[test]
    fn test_finalize_blocks_bad() {
        let (text, blocked) = ContentQualityGate::finalize_output("свобода", "", &[]);
        assert!(blocked);
        assert!(text.contains("перенастраиваю"));
    }

    #[test]
    fn test_finalize_passes_good() {
        let (text, blocked) =
            ContentQualityGate::finalize_output("свобода", "свобода предполагает выбор", &[]);
        assert!(!blocked);
        assert_eq!(text, "свобода предполагает выбор");
    }

    #[test]
    fn test_topic_relevance_short_output() {
        // H-5: short off-topic output should be blocked even with few tokens.
        let verdict = ContentQualityGate::evaluate("свобода", "ок");
        assert!(matches!(verdict, QualityVerdict::Block(_)));
    }

    #[test]
    fn test_filler_substring_match() {
        // H-6: filler prefix with extra trailing words should still be blocked.
        let verdict = ContentQualityGate::evaluate("свобода", "понятно, спасибо.");
        assert!(matches!(verdict, QualityVerdict::Block(_)));
    }
}
