//! DiscourseComposer — composes multi-sentence responses from selected predicates.
//!
//! Structure:
//!   TopicIntroduction → Definition → Elaboration → Counterpoint → Synthesis → Transition
//!
//! Each component uses SyntacticGenerator for surface forms, with anti-repetition
//! tracking (used template indices passed through the composition).
use std::collections::{BTreeMap, BTreeSet};
use qxfx0_morphology::{Case, MorphologyData};
use crate::content_selector::SelectedPredicate;
use crate::syntactic_generator::{detect_gender, DiscourseStyle, SyntacticGenerator, Verbosity};

pub struct DiscourseComposer {
    generator: SyntacticGenerator,
}

fn pronoun_for(word: &str, _count: usize, _prev: &str) -> String {
    if word.is_empty() { return word.to_string(); }
    match detect_gender(word) {
        "m" => "он".into(),
        "f" => "она".into(),
        "n" => "оно".into(),
        "pl" => "они".into(),
        _ => "он".into(),
    }
}

fn apply_pronouns(text: &str) -> String {
    let sentences: Vec<&str> = text.split(". ").collect();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut result: Vec<String> = Vec::new();
    for sentence in sentences {
        let mut s = sentence.to_string();
        let first_word = s.split_whitespace().next()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .unwrap_or_default();
        if first_word.chars().count() >= 4 {
            let count = seen.entry(first_word.clone()).or_insert(0);
            *count += 1;
            if *count >= 2 {
                let pronoun = pronoun_for(&first_word, *count, "");
                if let Some(end) = s.find(|c: char| c == ' ' || c == ',') {
                    let mut replacement = pronoun;
                    if s.starts_with(|c: char| c.is_uppercase()) {
                        let mut ch: Vec<char> = replacement.chars().collect();
                        if !ch.is_empty() { ch[0] = ch[0].to_uppercase().next().unwrap_or(ch[0]); }
                        replacement = ch.into_iter().collect();
                    }
                    s.replace_range(0..end, &replacement);
                }
            }
        }
        result.push(s);
    }
    result.join(". ")
}

impl DiscourseComposer {
    pub fn new() -> Self { DiscourseComposer { generator: SyntacticGenerator::new() } }

    pub fn compose(&self, selected: &[SelectedPredicate], topic: &str, style: &DiscourseStyle, seed: u64, history: &[String]) -> String {
        let mut used_indices: Vec<usize> = Vec::new();
        let mut used_connectors: BTreeSet<String> = BTreeSet::new();
        let mut parts: Vec<String> = Vec::new();
        let (defining, supporting, countering) = classify_selected(selected);

        // Response pattern variety: sometimes skip intro or lead with question
        let pattern = seed % 5;
        let skip_intro = pattern == 4 && history.is_empty();

        if !skip_intro && style.verbosity != Verbosity::Brief {
            if pattern == 3 && history.is_empty() {
                // Lead with a rhetorical question
                parts.push(format!("{}? Давай разберёмся.", topic_introduction_short(topic, seed)));
            } else {
                parts.push(topic_introduction(topic, seed, history));
            }
        }
        if let Some(def) = defining {
            parts.push(self.generator.verbalize(&def.relation, style, seed, &mut used_indices));
            if let Some(ref r) = def.relation.rationale {
                if style.verbosity == Verbosity::Elaborate {
                    let c = pick_unique("causation", seed, &mut used_connectors);
                    parts.push(format!("{} {}", c, r));
                }
            }
        }
        let n = match style.verbosity { Verbosity::Brief=>0, Verbosity::Medium=>1, Verbosity::Elaborate=>2 };
        for (i, edge) in supporting.iter().take(n).enumerate() {
            let c = pick_unique("elaboration", seed.wrapping_add(i as u64*17), &mut used_connectors);
            let t = self.generator.verbalize(&edge.relation, style, seed.wrapping_add(edge.score as u64*100), &mut used_indices);
            parts.push(format!("{} {}", c, t));
        }
        if let Some(ctr) = countering {
            if style.verbosity != Verbosity::Brief {
                let c = pick_unique("contrast", seed.wrapping_add(1000), &mut used_connectors);
                let t = self.generator.verbalize(&ctr.relation, style, seed.wrapping_add(2000), &mut used_indices);
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

        // Outro: conversational invitation (30% chance, not for brief)
        if style.verbosity != Verbosity::Brief && style.register == "conversational" && seed % 7 == 0 {
            let outros = [
                "Что думаешь об этом?",
                "Интересно услышать твой взгляд.",
                "Согласен или есть что возразить?",
                "Как тебе такая картина?",
            ];
            parts.push(outros[(seed as usize / 7) % outros.len()].to_string());
        }

        // Exemplification: add a concrete example if elaboration is rich (40% chance)
        if seed % 5 == 0 && parts.len() >= 2 && style.verbosity == Verbosity::Elaborate {
            let examples = [
                "Например, представь себе такую ситуацию: ты стоишь перед выбором, и от твоего решения зависит не только твоя жизнь, но и жизнь других. Это и есть свобода в действии.",
                "Скажем, когда человек жертвует личным временем ради близких — это проявление ответственности через свободу выбора.",
                "В качестве иллюстрации: учёный, публикующий спорную гипотезу, рискует репутацией, но движет познание вперёд.",
                "Пример из жизни: художник, отказывающийся от коммерческого успеха ради подлинного искусства.",
            ];
            parts.push(examples[(seed as usize / 11) % examples.len()].to_string());
        }

        apply_pronouns(&parts.join(". "))
    }
}

fn classify_selected(selected: &[SelectedPredicate]) -> (Option<&SelectedPredicate>, Vec<&SelectedPredicate>, Option<&SelectedPredicate>) {
    use qxfx0_types::RelationType;
    let mut defining: Option<&SelectedPredicate> = None;
    let mut supporting: Vec<&SelectedPredicate> = Vec::new();
    let mut countering: Option<&SelectedPredicate> = None;

    for sp in selected {
        match sp.relation.rel_type {
            RelationType::RelPresupposes|RelationType::RelMeans|RelationType::RelDenotes|RelationType::RelIsA|RelationType::RelDetermines|RelationType::RelClaims => {
                if defining.map_or(true, |d| sp.score > d.score) { defining = Some(sp); }
            }
            rt if rt.is_counter() => {
                if countering.map_or(true, |d| sp.score > d.score) { countering = Some(sp); }
            }
            _ => { supporting.push(sp); }
        }
    }
    supporting.sort_by(|a,b| b.score.total_cmp(&a.score));
    (defining, supporting, countering)
}

fn pick_unique(category: &str, seed: u64, used: &mut BTreeSet<String>) -> String {
    let all: &[&str] = match category {
        "elaboration" => &["Более того,","Кроме того,","Следует добавить:","В дополнение к этому,","Также важно, что","А ещё интереснее то, что","Вот что любопытно:","Заметь:","Взгляни на это так:"],
        "causation" => &["потому что","поскольку","в силу того что","причина здесь в том, что","дело в том, что","объясняется это тем, что"],
        "contrast" => &["Однако","Но","Вместе с тем,","С другой стороны,","Хотя есть и обратная сторона:","При этом не стоит забывать:","И всё же","Правда, есть нюанс:"],
        "synthesis" => &["Именно поэтому","Таким образом,","В итоге","Обобщая, можно сказать:","В сухом остатке:","К чему я это всё? А к тому, что"],
        _ => return String::new(),
    };
    for i in 0..all.len() {
        let idx = ((seed as usize)+i) % all.len();
        let c = all[idx].to_string();
        if !used.contains(&c) { used.insert(c.clone()); return c; }
    }
    let c = all[(seed as usize)%all.len()].to_string();
    used.insert(c.clone());
    c
}

/// Choose preposition "о"/"об" based on the next word's first letter.
fn prep_for(word: &str) -> &'static str {
    let first = word.chars().next().unwrap_or(' ');
    match first {
        'а'|'е'|'ё'|'и'|'о'|'у'|'ы'|'э'|'ю'|'я' => "об",
        _ => "о",
    }
}

/// Adapt a template string by replacing " о {t}" with " об {t}" or " обо {t}"
fn adapt_template(tpl: &str, inflected: &str) -> String {
    let prep = prep_for(inflected);
    let result = tpl.replace(" о {t}", &format!(" {} {{t}}", prep));
    result.replace("{t}", inflected)
}

fn topic_introduction_short(topic: &str, seed: u64) -> String {
    let morph = MorphologyData::with_seed();
    let inflected = morph.to_case(Case::Prepositional, topic);
    let qs = [
        "Что я думаю об {t}",
        "А что насчёт {t}",
        "Ты спрашиваешь об {t}",
    ];
    adapt_template(qs[(seed as usize) % qs.len()], &inflected)
}

fn topic_introduction(topic: &str, seed: u64, history: &[String]) -> String {
    let morph = MorphologyData::with_seed();
    let inflected = morph.to_case(Case::Prepositional, topic);

    // If there's prior conversation, sometimes reference it.
    let use_continuation = !history.is_empty() && (seed % 3 == 0);
    if use_continuation {
        let cont = [
            "Продолжая наш разговор — что касается {t}.",
            "Ты спрашиваешь о {t}. Давай разберёмся.",
            "Хороший вопрос про {t}. Смотри.",
            "Вернёмся к теме. О {t} я думаю так.",
            "Теперь о {t}. Вот что приходит на ум.",
            "Интересно, что ты спросил про {t}. Мой взгляд:",
        ];
        return adapt_template(&cont[(seed as usize / 3) % cont.len()], &inflected);
    }

    let fresh = [
        "Размышляя о {t}, можно сказать следующее.",
        "Когда речь заходит о {t}, я вижу это так.",
        "На вопрос о {t} мой ответ таков.",
        "Позволь сформулировать мысль о {t}.",
        "О {t} я могу сказать вот что.",
        "Давай подумаем о {t}.",
        "Что такое {t}? Для меня это...",
        "Если говорить о {t}, то...",
        "В двух словах о {t} не скажешь, но я попробую.",
    ];
    adapt_template(&fresh[(seed as usize) % fresh.len()], &inflected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed_graph;
    use crate::network::build_semantic_network;
    use crate::network::activate as network_activate;
    use crate::content_selector::ContentSelector;
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
        let brief = DiscourseStyle { verbosity: Verbosity::Brief, ..Default::default() };
        let elaborate = DiscourseStyle { verbosity: Verbosity::Elaborate, ..Default::default() };
        let b = dc.compose(&selected, "свобода", &brief, 0, &[]);
        let e = dc.compose(&selected, "свобода", &elaborate, 0, &[]);
        assert!(e.len() >= b.len());
    }
}
