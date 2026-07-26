//! DiscourseComposer — composes multi-sentence responses from selected predicates.
//!
//! Structure:
//!   TopicIntroduction → Definition → Elaboration → Counterpoint → Synthesis → Transition
//!
//! Each component uses SyntacticGenerator for surface forms, with anti-repetition
//! tracking (used template indices passed through the composition).
use crate::content_selector::SelectedPredicate;
use crate::syntactic_generator::{detect_gender, DiscourseStyle, SyntacticGenerator, Verbosity};
use qxfx0_morphology::{Case, MorphologyData};
use std::collections::BTreeSet;

pub struct DiscourseComposer {
    generator: SyntacticGenerator,
}

fn pronoun_for(word: &str, _count: usize, _prev: &str) -> String {
    if word.is_empty() {
        return word.to_string();
    }
    match detect_gender(word) {
        "m" => "он".into(),
        "f" => "она".into(),
        "n" => "оно".into(),
        "pl" => "они".into(),
        _ => "он".into(),
    }
}

/// Split text into sentences without breaking common Russian abbreviations.
fn split_sentences(text: &str) -> Vec<&str> {
    let abbreviations: &[&str] = &[
        "т.д.",
        "т.п.",
        "т.е.",
        "и т.д.",
        "и т.п.",
        "см.",
        "ул.",
        "г.",
        "др.",
        "пр.",
        "ст.",
    ];
    let mut result = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '.' || c == '!' || c == '?' {
            let next_is_space = chars
                .peek()
                .map(|(_, nc)| nc.is_whitespace())
                .unwrap_or(true);
            if next_is_space {
                let segment = &text[start..=i];
                if !abbreviations.iter().any(|abbr| segment.ends_with(abbr)) {
                    result.push(segment);
                    start = i + c.len_utf8();
                    while let Some((j, nc)) = chars.peek() {
                        if nc.is_whitespace() {
                            start = *j + nc.len_utf8();
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }

    if start < text.len() {
        result.push(&text[start..]);
    }
    result
}

fn apply_topic_pronouns(text: &str, topic: &str) -> String {
    // Pronoun substitution is only safe for a one-word topic in nominative
    // position. Replacing the first repeated four-letter word used to turn
    // discourse connectors into phrases such as "Более оно по-видимому".
    let clean_topic = topic
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    if clean_topic.chars().count() < 3 || topic.split_whitespace().count() != 1 {
        return text.to_string();
    }

    let mut seen_topic = false;
    let mut result = Vec::new();
    for sentence in split_sentences(text) {
        let mut rendered = sentence.to_string();
        let Some(word) = sentence.split_whitespace().next() else {
            continue;
        };
        let clean_word = word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        if clean_word == clean_topic {
            if seen_topic {
                let mut replacement = pronoun_for(&clean_topic, 2, "");
                if word.starts_with(|c: char| c.is_uppercase()) {
                    let mut chars = replacement.chars();
                    if let Some(first) = chars.next() {
                        replacement = first.to_uppercase().chain(chars).collect::<String>();
                    }
                }
                if let Some(start) = rendered.find(word) {
                    rendered.replace_range(start..start + word.len(), &replacement);
                }
            }
            seen_topic = true;
        }
        result.push(rendered);
    }
    result.join(" ")
}

/// Normalize sentence punctuation in one place for every rendering path.
/// Adjacent terminal marks collapse to one, and whitespace before punctuation
/// is removed. Question/exclamation marks take precedence over a period.
pub fn normalize_punctuation(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.trim().chars() {
        if ch.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            continue;
        }

        if matches!(ch, '.' | '!' | '?') {
            while out.ends_with(' ') {
                out.pop();
            }
            if let Some(previous) = out.chars().last() {
                if matches!(previous, '.' | '!' | '?') {
                    if previous == '.' && ch != '.' {
                        out.pop();
                        out.push(ch);
                    }
                    continue;
                }
            }
            out.push(ch);
            continue;
        }

        if matches!(ch, ',' | ';' | ':') {
            while out.ends_with(' ') {
                out.pop();
            }
            if !out.ends_with(ch) {
                out.push(ch);
            }
            continue;
        }

        out.push(ch);
    }
    out.trim().to_string()
}

fn join_sentences(parts: &[String]) -> String {
    let joined = parts
        .iter()
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                return None;
            }
            let capitalized = capitalize_first(trimmed);
            if capitalized.ends_with(['.', '!', '?', ':']) {
                Some(capitalized)
            } else {
                Some(format!("{}.", capitalized))
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    normalize_punctuation(&joined)
}

fn capitalize_first(text: &str) -> String {
    let Some((index, character)) = text
        .char_indices()
        .find(|(_, character)| character.is_alphabetic())
    else {
        return text.to_string();
    };
    let mut result = String::with_capacity(text.len());
    result.push_str(&text[..index]);
    result.extend(character.to_uppercase());
    result.push_str(&text[index + character.len_utf8()..]);
    result
}

impl Default for DiscourseComposer {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscourseComposer {
    pub fn new() -> Self {
        DiscourseComposer {
            generator: SyntacticGenerator::new(),
        }
    }

    pub fn compose(
        &self,
        selected: &[SelectedPredicate],
        topic: &str,
        style: &DiscourseStyle,
        seed: u64,
        history: &[String],
    ) -> String {
        let mut used_indices: Vec<usize> = Vec::new();
        let mut used_connectors: BTreeSet<String> = BTreeSet::new();
        let mut parts: Vec<String> = Vec::new();
        let (defining, supporting, countering) = classify_selected(selected);

        let pattern = seed % 5;
        let skip_intro = pattern == 4 && history.is_empty();

        if !skip_intro && style.verbosity != Verbosity::Brief {
            if pattern == 3 && history.is_empty() {
                parts.push(format!(
                    "{}? Давай разберёмся.",
                    topic_introduction_short(topic, seed)
                ));
            } else {
                parts.push(topic_introduction(topic, seed, history));
            }
        }
        if let Some(def) = defining {
            parts.push(
                self.generator
                    .verbalize(&def.relation, style, seed, &mut used_indices),
            );
            if let Some(ref r) = def.relation.rationale {
                if style.verbosity == Verbosity::Elaborate {
                    let c = pick_unique("causation", seed, &mut used_connectors);
                    parts.push(format!("{} {}", c, r));
                }
            }
        }
        let n = match style.verbosity {
            Verbosity::Brief => {
                if defining.is_none() {
                    1
                } else {
                    0
                }
            }
            Verbosity::Medium => 1,
            Verbosity::Elaborate => 2,
        };
        for (i, edge) in supporting.iter().take(n).enumerate() {
            let c = pick_unique(
                "elaboration",
                seed.wrapping_add(i as u64 * 17),
                &mut used_connectors,
            );
            let t = self.generator.verbalize(
                &edge.relation,
                style,
                seed.wrapping_add(edge.score as u64 * 100),
                &mut used_indices,
            );
            parts.push(format!("{} {}", c, t));
        }
        if let Some(ctr) = countering {
            if style.verbosity != Verbosity::Brief {
                let c = pick_unique("contrast", seed.wrapping_add(1000), &mut used_connectors);
                let t = self.generator.verbalize(
                    &ctr.relation,
                    style,
                    seed.wrapping_add(2000),
                    &mut used_indices,
                );
                parts.push(format!("{} {}", c, t));
            }
        }
        if let Some(def) = defining {
            if let Some(ref syn) = def.relation.synthesis {
                if style.verbosity != Verbosity::Brief {
                    let c = pick_unique("synthesis", seed.wrapping_add(3000), &mut used_connectors);
                    parts.push(format!("{} {}", c, syn));
                }
            }
        }

        if style.verbosity != Verbosity::Brief
            && style.register == "conversational"
            && seed.is_multiple_of(7)
        {
            let outros = [
                "Что думаешь об этом?",
                "Интересно услышать твой взгляд.",
                "Согласен или есть что возразить?",
                "Как тебе такая картина?",
            ];
            parts.push(outros[(seed as usize / 7) % outros.len()].to_string());
        }

        if seed.is_multiple_of(5) && parts.len() >= 2 && style.verbosity == Verbosity::Elaborate {
            let examples = [
                format!("Например, смысл «{}» можно проверить на конкретной ситуации и её последствиях.", topic),
                format!("Практический пример для темы «{}» должен показывать наблюдаемое действие, а не подменять определение.", topic),
                format!("В качестве иллюстрации к теме «{}» полезно сравнить два случая с разными условиями.", topic),
                format!("Пример из жизни про «{}» будет точнее, если явно назвать участников, действие и результат.", topic),
            ];
            parts.push(examples[(seed as usize / 11) % examples.len()].clone());
        }

        let joined = join_sentences(&parts);
        normalize_punctuation(&apply_topic_pronouns(&joined, topic))
    }
}

fn classify_selected(
    selected: &[SelectedPredicate],
) -> (
    Option<&SelectedPredicate>,
    Vec<&SelectedPredicate>,
    Option<&SelectedPredicate>,
) {
    use qxfx0_types::RelationType;
    let mut defining: Option<&SelectedPredicate> = None;
    let mut supporting: Vec<&SelectedPredicate> = Vec::new();
    let mut countering: Option<&SelectedPredicate> = None;

    for sp in selected {
        match sp.relation.rel_type {
            RelationType::RelPresupposes
            | RelationType::RelMeans
            | RelationType::RelDenotes
            | RelationType::RelIsA
            | RelationType::RelDetermines
            | RelationType::RelClaims => {
                if defining.is_none_or(|d| sp.score > d.score) {
                    defining = Some(sp);
                }
            }
            rt if rt.is_counter() => {
                if countering.is_none_or(|d| sp.score > d.score) {
                    countering = Some(sp);
                }
            }
            _ => {
                supporting.push(sp);
            }
        }
    }
    supporting.sort_by(|a, b| b.score.total_cmp(&a.score));
    (defining, supporting, countering)
}

fn pick_unique(category: &str, seed: u64, used: &mut BTreeSet<String>) -> String {
    let all: &[&str] = match category {
        "elaboration" => &[
            "Более того,",
            "Кроме того,",
            "Следует добавить:",
            "В дополнение к этому,",
            "Также важно, что",
            "А ещё интереснее то, что",
            "Вот что любопытно:",
            "Заметь:",
            "Взгляни на это так:",
        ],
        "causation" => &[
            "потому что",
            "поскольку",
            "в силу того что",
            "причина здесь в том, что",
            "дело в том, что",
            "объясняется это тем, что",
        ],
        "contrast" => &[
            "Однако",
            "Но",
            "Вместе с тем,",
            "С другой стороны,",
            "Хотя есть и обратная сторона:",
            "При этом не стоит забывать:",
            "И всё же",
            "Правда, есть нюанс:",
        ],
        "synthesis" => &[
            "Именно поэтому",
            "Таким образом,",
            "В итоге",
            "Обобщая, можно сказать:",
            "В сухом остатке:",
            "К чему я это всё? А к тому, что",
        ],
        _ => return String::new(),
    };
    for i in 0..all.len() {
        let idx = ((seed as usize) + i) % all.len();
        let c = all[idx].to_string();
        if !used.contains(&c) {
            used.insert(c.clone());
            return c;
        }
    }
    let c = all[(seed as usize) % all.len()].to_string();
    used.insert(c.clone());
    c
}

fn prep_for(word: &str) -> &'static str {
    let first = word.chars().next().unwrap_or(' ');
    match first {
        'а' | 'и' | 'о' | 'у' | 'э' => "об",
        _ => "о",
    }
}

fn adapt_template(tpl: &str, inflected: &str) -> String {
    let prep = prep_for(inflected);
    let capitalized_prep = if prep == "об" { "Об" } else { "О" };
    let result = tpl
        .replace(" об {t}", &format!(" {} {{t}}", prep))
        .replace(" о {t}", &format!(" {} {{t}}", prep))
        .replace("Об {t}", &format!("{} {{t}}", capitalized_prep))
        .replace("О {t}", &format!("{} {{t}}", capitalized_prep));
    result.replace("{t}", inflected)
}

struct IntroTemplate {
    pattern: &'static str,
    case: Case,
}

fn topic_introduction_short(topic: &str, seed: u64) -> String {
    let morph = MorphologyData::with_seed();
    let qs = [
        IntroTemplate {
            pattern: "Что я думаю об {t}",
            case: Case::Prepositional,
        },
        IntroTemplate {
            pattern: "А что насчёт {t}",
            case: Case::Genitive,
        },
        IntroTemplate {
            pattern: "Ты спрашиваешь об {t}",
            case: Case::Prepositional,
        },
    ];
    let tpl = &qs[(seed as usize) % qs.len()];
    let inflected = morph.to_case(tpl.case, topic);
    adapt_template(tpl.pattern, &inflected)
}

fn topic_introduction(topic: &str, seed: u64, history: &[String]) -> String {
    let morph = MorphologyData::with_seed();

    let use_continuation = !history.is_empty() && seed.is_multiple_of(3);
    if use_continuation {
        let cont = [
            IntroTemplate {
                pattern: "Продолжая наш разговор — что касается {t}.",
                case: Case::Genitive,
            },
            IntroTemplate {
                pattern: "Ты спрашиваешь о {t}. Давай разберёмся.",
                case: Case::Prepositional,
            },
            IntroTemplate {
                pattern: "Хороший вопрос про {t}. Смотри.",
                case: Case::Accusative,
            },
            IntroTemplate {
                pattern: "Вернёмся к теме. О {t} я думаю так.",
                case: Case::Prepositional,
            },
            IntroTemplate {
                pattern: "Теперь о {t}. Вот что приходит на ум.",
                case: Case::Prepositional,
            },
            IntroTemplate {
                pattern: "Интересно, что ты спросил про {t}. Мой взгляд:",
                case: Case::Accusative,
            },
        ];
        let tpl = &cont[(seed as usize / 3) % cont.len()];
        let inflected = morph.to_case(tpl.case, topic);
        return adapt_template(tpl.pattern, &inflected);
    }

    let fresh = [
        IntroTemplate {
            pattern: "Размышляя о {t}, можно сказать следующее.",
            case: Case::Prepositional,
        },
        IntroTemplate {
            pattern: "Когда речь заходит о {t}, я вижу это так.",
            case: Case::Prepositional,
        },
        IntroTemplate {
            pattern: "На вопрос о {t} мой ответ таков.",
            case: Case::Prepositional,
        },
        IntroTemplate {
            pattern: "Позволь сформулировать мысль о {t}.",
            case: Case::Prepositional,
        },
        IntroTemplate {
            pattern: "О {t} я могу сказать вот что.",
            case: Case::Prepositional,
        },
        IntroTemplate {
            pattern: "Давай подумаем о {t}.",
            case: Case::Prepositional,
        },
        IntroTemplate {
            pattern: "Что такое {t}? Сформулирую так:",
            case: Case::Nominative,
        },
        IntroTemplate {
            pattern: "Если говорить о {t}, выделю главное:",
            case: Case::Prepositional,
        },
        IntroTemplate {
            pattern: "В двух словах о {t} не скажешь, но я попробую.",
            case: Case::Prepositional,
        },
    ];
    let tpl = &fresh[(seed as usize) % fresh.len()];
    let inflected = morph.to_case(tpl.case, topic);
    adapt_template(tpl.pattern, &inflected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_selector::ContentSelector;
    use crate::network::activate as network_activate;
    use crate::network::build_semantic_network;
    use crate::seed_graph;
    use qxfx0_types::atom::AtomId;
    use qxfx0_types::field::FieldProfile;

    #[test]
    fn test_compose_produces_structured_output() {
        let graph = seed_graph();
        let sn = build_semantic_network(&graph);
        let cs = ContentSelector::build(&graph);
        let activated = network_activate(&AtomId::new("свобода"), &sn);
        let selected = cs.compose_from_activation(&FieldProfile::default(), "свобода", &activated);
        let dc = DiscourseComposer::new();
        let result = dc.compose(&selected, "свобода", &DiscourseStyle::default(), 42, &[]);
        assert!(!result.is_empty());
        assert!(result.contains("свобода"));
    }

    #[test]
    fn test_compose_deterministic() {
        let graph = seed_graph();
        let sn = build_semantic_network(&graph);
        let cs = ContentSelector::build(&graph);
        let activated = network_activate(&AtomId::new("свобода"), &sn);
        let selected = cs.compose_from_activation(&FieldProfile::default(), "свобода", &activated);
        let dc = DiscourseComposer::new();
        let a = dc.compose(&selected, "свобода", &DiscourseStyle::default(), 42, &[]);
        let b = dc.compose(&selected, "свобода", &DiscourseStyle::default(), 42, &[]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_compose_elaborate_is_longer() {
        let graph = seed_graph();
        let sn = build_semantic_network(&graph);
        let cs = ContentSelector::build(&graph);
        let activated = network_activate(&AtomId::new("свобода"), &sn);
        let selected = cs.compose_from_activation(&FieldProfile::default(), "свобода", &activated);
        let dc = DiscourseComposer::new();
        let brief = DiscourseStyle {
            verbosity: Verbosity::Brief,
            ..Default::default()
        };
        let elaborate = DiscourseStyle {
            verbosity: Verbosity::Elaborate,
            ..Default::default()
        };
        let b = dc.compose(&selected, "свобода", &brief, 0, &[]);
        let e = dc.compose(&selected, "свобода", &elaborate, 0, &[]);
        assert!(e.len() >= b.len());
    }

    #[test]
    fn test_pronouns_not_applied_to_connectors() {
        let text = "Более того, свобода предполагает выбор. Более того, свобода требует сознания.";
        let result = apply_topic_pronouns(text, "свобода");
        assert!(
            !result.contains("оно того") && !result.contains("Оно того"),
            "connectors should not be replaced by pronouns, got: {}",
            result
        );
        assert!(
            result.contains("Более"),
            "connector 'Более' should be preserved"
        );
    }

    #[test]
    fn test_pronouns_applied_to_repeated_topic() {
        let text = "свобода предполагает выбор. свобода требует сознания.";
        let result = apply_topic_pronouns(text, "свобода");
        assert!(
            !result.contains("свобода требует"),
            "repeated topic should be pronounized, got: {}",
            result
        );
    }

    #[test]
    fn test_pronouns_ignore_repeated_non_topic_word() {
        let text = "Более того, смысл ясен. Более того, вывод устойчив.";
        let result = apply_topic_pronouns(text, "свобода");
        assert_eq!(result, text);
    }

    #[test]
    fn test_normalize_punctuation_collapses_runs() {
        assert_eq!(
            normalize_punctuation("Фраза...  Вопрос.. ?  Ответ!!"),
            "Фраза. Вопрос? Ответ!"
        );
    }

    #[test]
    fn test_join_sentences_does_not_duplicate_periods() {
        let parts = vec!["Первая фраза.".into(), "Вторая фраза".into()];
        assert_eq!(join_sentences(&parts), "Первая фраза. Вторая фраза.");
        let parts = vec!["мой взгляд:".into(), "возможно, это так".into()];
        assert_eq!(join_sentences(&parts), "Мой взгляд: Возможно, это так.");
    }

    #[test]
    fn test_topic_preposition_adapts_case_and_iotated_vowels() {
        assert_eq!(
            adapt_template("О {t} я думаю так.", "ответственности"),
            "Об ответственности я думаю так."
        );
        assert_eq!(
            adapt_template("Ты спрашиваешь об {t}.", "языке"),
            "Ты спрашиваешь о языке."
        );
        assert_eq!(
            adapt_template("О {t} я могу сказать.", "истине"),
            "Об истине я могу сказать."
        );
    }
}
