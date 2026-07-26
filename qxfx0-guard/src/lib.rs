use qxfx0_types::system_state::GuardStatus;

mod heuristics;
use heuristics::*;

/// Configuration for content-quality and safety checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardConfig {
    /// Maximum accepted user input length, in Unicode scalar values.
    pub max_input_length: usize,
    /// Maximum allowed length of a rendered response, in bytes.
    pub max_render_length: usize,
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            max_input_length: 8192,
            max_render_length: 5000,
        }
    }
}

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
        if let Some(reason) = check_template_placeholders(rendered) {
            return QualityVerdict::Block(reason);
        }

        // Check 3: generic filler
        if let Some(reason) = check_generic_fillers(rendered) {
            return QualityVerdict::Block(reason);
        }

        // Check 4: topic relevance
        if let Some(reason) = check_topic_relevance(topic, rendered) {
            return QualityVerdict::Block(reason);
        }

        let tokens: Vec<&str> = rendered.split_whitespace().collect();

        // Check 5: content density
        if let Some(reason) = check_content_density(&tokens) {
            return QualityVerdict::Block(reason);
        }

        // Check 6: semantic saturation
        if let Some(reason) = check_semantic_saturation(&tokens) {
            return QualityVerdict::Block(reason);
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
    pub fn post_render_safety(
        rendered: &str,
        history: &[String],
        config: &GuardConfig,
    ) -> GuardStatus {
        let trimmed = rendered.trim();

        // Empty
        if trimmed.is_empty() || trimmed == "..." || trimmed == "?" {
            return GuardStatus::InvariantBlock("пустой вывод".into());
        }

        // Too long
        if rendered.len() > config.max_render_length {
            return GuardStatus::InvariantBlock("слишком длинный вывод".into());
        }

        // Metadata leak
        if let Some(reason) = check_metadata_leaks(rendered) {
            return GuardStatus::InvariantBlock(reason);
        }

        // Toxicity
        if let Some(reason) = check_toxicity(rendered) {
            return GuardStatus::InvariantBlock(reason);
        }

        // Stuck repetition (advisory, not blocking)
        if let Some(reason) = check_stuck_repetition(rendered, history) {
            return GuardStatus::InvariantWarn(reason);
        }

        GuardStatus::InvariantOk
    }

    /// Finalize output — apply safety + quality gates.
    /// Returns (final_text, was_blocked).
    pub fn finalize_output(
        topic: &str,
        rendered: &str,
        history: &[String],
        config: &GuardConfig,
    ) -> (String, bool) {
        let safety = Self::post_render_safety(rendered, history, config);
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
        let cfg = GuardConfig::default();
        let status = ContentQualityGate::post_render_safety("ты должен это сделать", &[], &cfg);
        assert!(matches!(status, GuardStatus::InvariantBlock(_)));
    }

    #[test]
    fn test_safety_ok() {
        let cfg = GuardConfig::default();
        let status =
            ContentQualityGate::post_render_safety("свобода предполагает выбор", &[], &cfg);
        assert_eq!(status, GuardStatus::InvariantOk);
    }

    #[test]
    fn test_finalize_blocks_bad() {
        let cfg = GuardConfig::default();
        let (text, blocked) = ContentQualityGate::finalize_output("свобода", "", &[], &cfg);
        assert!(blocked);
        assert!(text.contains("перенастраиваю"));
    }

    #[test]
    fn test_finalize_passes_good() {
        let cfg = GuardConfig::default();
        let (text, blocked) =
            ContentQualityGate::finalize_output("свобода", "свобода предполагает выбор", &[], &cfg);
        assert!(!blocked);
        assert_eq!(text, "свобода предполагает выбор");
    }

    #[test]
    fn test_topic_relevance_short_output() {
        let verdict = ContentQualityGate::evaluate("свобода", "ок");
        assert!(matches!(verdict, QualityVerdict::Block(_)));
    }

    #[test]
    fn test_filler_substring_match() {
        let verdict = ContentQualityGate::evaluate("свобода", "понятно, спасибо.");
        assert!(matches!(verdict, QualityVerdict::Block(_)));
    }
}
