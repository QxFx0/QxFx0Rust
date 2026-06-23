use qxfx0_types::system_state::GuardStatus;

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

        // Check 2: template placeholders
        let placeholders = [
            "{topic}", "{left}", "{right}", "{style}", "{move}", "{slot}",
        ];
        for ph in &placeholders {
            if rendered.contains(ph) {
                return QualityVerdict::Block(format!("незаполненный шаблон: {}", ph));
            }
        }

        // Check 3: generic filler
        let fillers = [
            "я не знаю что сказать",
            "произошла ошибка",
            "не удалось сгенерировать ответ",
            "[пусто]",
            "[нет данных]",
            "понятно.",
            "я понимаю.",
        ];
        for filler in &fillers {
            if trimmed == *filler {
                return QualityVerdict::Block("генерический filler-ответ".into());
            }
        }

        // Check 4: topic relevance (only for 50+ tokens to avoid false positives)
        let tokens: Vec<&str> = rendered.split_whitespace().collect();
        if !topic.is_empty() && tokens.len() >= 50 {
            let topic_tokens: Vec<&str> =
                topic.split_whitespace().filter(|t| t.len() >= 3).collect();
            let lower = rendered.to_lowercase();
            let has_overlap = topic_tokens.iter().any(|t| lower.contains(t));
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
            let bigrams: Vec<(&str, &str)> = tokens
                .windows(2)
                .filter_map(|w| {
                    if w.len() == 2 {
                        Some((w[0], w[1]))
                    } else {
                        None
                    }
                })
                .collect();
            if !bigrams.is_empty() {
                let unique = bigrams
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len();
                let repeat_ratio = 1.0 - unique as f64 / bigrams.len() as f64;
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
            "{topic}", "{left}", "{right}", "{style}", "{move}", "{slot}",
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

        // Toxicity (basic)
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
        let found_toxic: Vec<_> = toxic.iter().filter(|t| lower.contains(*t)).collect();
        if !found_toxic.is_empty() {
            return GuardStatus::InvariantBlock(format!(
                "токсичные паттерны: {}",
                found_toxic
                    .iter()
                    .map(|s| **s)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // Stuck repetition (advisory, not blocking)
        let normalized = lower.trim();
        let match_count = history
            .iter()
            .take(5)
            .filter(|h| h.trim().to_lowercase() == normalized)
            .count();
        if match_count >= 3 {
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
        let verdict = ContentQualityGate::evaluate("свобода", "{topic} — это понятие");
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
}
