//! SyntacticGenerator — динамическая вербализация семантических отношений
//! через множественные шаблоны с морфологическим согласованием слотов.
//!
//! Placeholder syntax: {FROM}, {FROM|gen}, {TO|acc}, {RATIONALE}, {SYNTHESIS}

use qxfx0_types::atom::Relation;
use qxfx0_morphology::{Case, MorphologyData};
use crate::template_registry::{SurfaceTemplate, TemplateRegistry};

#[derive(Debug, Clone)]
pub struct DiscourseStyle {
    pub register: String,
    pub complexity: u8,
    pub hedging: f64,
    pub verbosity: Verbosity,
    pub use_transitions: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Brief,
    Medium,
    Elaborate,
}

impl Default for DiscourseStyle {
    fn default() -> Self {
        DiscourseStyle {
            register: "philosophical".into(),
            complexity: 2,
            hedging: 0.0,
            verbosity: Verbosity::Medium,
            use_transitions: false,
        }
    }
}

pub struct SyntacticGenerator {
    registry: TemplateRegistry,
    morphology: MorphologyData,
}

impl SyntacticGenerator {
    pub fn new() -> Self {
        SyntacticGenerator {
            registry: TemplateRegistry::load(),
            morphology: MorphologyData::with_seed(),
        }
    }

    pub fn verbalize(
        &self, rel: &Relation, style: &DiscourseStyle,
        seed: u64, used_indices: &mut Vec<usize>,
    ) -> String {
        let selected = self.registry.select(rel.rel_type, &style.register, style.complexity, seed, used_indices);
        let text = match selected {
            Some((idx, template)) => {
                used_indices.push(idx);
                fill_template(template, rel, &self.morphology)
            }
            None => crate::seed::verbalize_relation(rel),
        };
        if style.hedging > 0.0 { hedge(&text, style.hedging, seed) } else { text }
    }

    fn inflect_case(morph: &MorphologyData, word: &str, case_str: &str) -> String {
        if case_str == "nom" || word.contains(' ') { return word.to_string(); }
        let case = match case_str {
            "gen" => Case::Genitive,
            "dat" => Case::Dative,
            "acc" => Case::Accusative,
            "inst" => Case::Instrumental,
            "prep" => Case::Prepositional,
            _ => return word.to_string(),
        };
        let result = morph.to_case(case, word);
        if result == word && case_str != "nom" {
            return word.to_string();
        }
        result
    }
}

impl Default for SyntacticGenerator {
    fn default() -> Self { Self::new() }
}

fn fill_template(template: &SurfaceTemplate, rel: &Relation, morph: &MorphologyData) -> String {
    let mut result = template.pattern.clone();
    result = fill_slot(&result, "FROM", rel.from.as_str(), morph);
    result = fill_slot(&result, "TO", rel.to.as_str(), morph);
    result = fill_obj_slot(&result, "OBJ", &rel.object_text, rel.to.as_str(), morph);
    // Gender-agreement forms: {FROM_G:связан,связана,связано}
    result = fill_gender_slot(&result, "FROM", rel.from.as_str());
    result = fill_gender_slot(&result, "TO", rel.to.as_str());
    result = fill_gender_slot(&result, "OBJ", rel.to.as_str());
    if let Some(ref r) = rel.rationale { result = result.replace("{RATIONALE}", r); }
    if let Some(ref s) = rel.synthesis { result = result.replace("{SYNTHESIS}", s); }
    strip_unreplaced(&result).trim().to_string()
}

/// Detect grammatical gender of a Russian word from its ending.
/// Returns "f", "m", "n", or "pl" for plural forms.
pub fn detect_gender(word: &str) -> &'static str {
    let chars: Vec<char> = word.chars().collect();
    if chars.is_empty() { return "m"; }
    let last = chars[chars.len() - 1];
    // Explicit plural: -и/-ы endings for nouns (but NOT -ия/-тия/-ния which are feminine singular)
    if (last == 'и' || last == 'ы') && chars.len() > 3 {
        let second_last = chars[chars.len() - 2];
        // Words ending in -ия (демократия, линия) are feminine singular, not plural.
        if second_last == 'я' {
            return "f";
        }
        if !matches!(second_last, 'о' | 'е' | 'а' | 'я' | 'и') {
            return "pl";
        }
    }
    let s: String = chars.iter().collect();
    match last {
        'а' | 'я' => "f",
        'о' | 'е' => "n",
        'ь' => {
            if s.ends_with("ость") || s.ends_with("есть") || s.ends_with("знь") { "f" } else { "m" }
        }
        _ => "m",
    }
}

/// Fill gender-agreement slots like {FROM_G:связан,связана,связано}
fn fill_gender_slot(template: &str, name: &str, word: &str) -> String {
    let prefix = format!("{{{name}_G:");
    if !template.contains(&prefix) { return template.to_string(); }

    let mut result = template.to_string();
    loop {
        let pos = match result.find(&prefix) {
            Some(p) => p,
            None => break,
        };
        let start = pos + prefix.len();
        let end = match result[start..].find('}') {
            Some(e) => start + e,
            None => break,
        };
        let forms: Vec<String> = result[start..end].split(',').map(|s| s.to_string()).collect();
        if forms.is_empty() { break; }
        let gender = detect_gender(word);
        let form = match gender {
            "f" => forms.get(1).cloned().unwrap_or_else(|| forms[0].clone()),
            "n" => forms.get(2).cloned().unwrap_or_else(|| forms[0].clone()),
            "pl" => forms.get(3).cloned().unwrap_or_else(|| forms[0].clone()),
            _ => forms[0].clone(),
        };
        result.replace_range(pos..end+1, &form);
        if !result.contains(&prefix) { break; }
    }
    result
}

fn fill_obj_slot(template: &str, name: &str, obj_text: &str, to_nom: &str, morph: &MorphologyData) -> String {
    let mut result = template.to_string();
    // Plain {OBJ} → object_text as-is
    result = result.replace(&format!("{{{}}}", name), obj_text);
    // Case-inflected {OBJ|gen} → inflect from TO atom's nominative form
    for case in &["nom","gen","dat","acc","inst","prep"] {
        let placeholder = format!("{{{}|{}}}", name, case);
        if case == &"nom" {
            result = result.replace(&placeholder, to_nom);
        } else {
            result = result.replace(&placeholder, &SyntacticGenerator::inflect_case(morph, to_nom, case));
        }
    }
    result
}

fn fill_slot(template: &str, name: &str, base: &str, morph: &MorphologyData) -> String {
    let mut result = template.to_string();
    result = result.replace(&format!("{{{}}}", name), base);
    for case in &["nom","gen","dat","acc","inst","prep"] {
        let placeholder = format!("{{{}|{}}}", name, case);
        result = result.replace(&placeholder, &SyntacticGenerator::inflect_case(morph, base, case));
    }
    result
}

fn strip_unreplaced(text: &str) -> String {
    let mut r = String::new();
    let mut in_brace = false;
    for ch in text.chars() {
        match ch {
            '{' => in_brace = true,
            '}' => in_brace = false,
            _ if !in_brace => r.push(ch),
            _ => {}
        }
    }
    r
}

fn hedge(text: &str, level: f64, seed: u64) -> String {
    let markers: &[&str] = if level < 0.5 { &["возможно, ","по-видимому, "] }
        else if level < 0.7 { &["мне кажется, что ","я полагаю, что "] }
        else { &["я не уверен, но ","предположительно, "] };
    format!("{}{}", markers[(seed as usize) % markers.len()], text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::atom::{AtomId, ObjectCase, RelationSource};

    fn make_rel(from: &str, to: &str, rt: qxfx0_types::RelationType) -> Relation {
        Relation {
            from: AtomId::new(from), to: AtomId::new(to), rel_type: rt,
            object_case: ObjectCase::CaseAccusative, object_text: to.to_string(),
            verb_override: None, ru_original: String::new(), en_original: String::new(),
            source: RelationSource::SeedFromPredicate, topic: from.to_string(),
            rationale: None, counter: None, synthesis: None,
        }
    }

    #[test]
    fn test_verbalize_uses_template() {
        let gen = SyntacticGenerator::new();
        let rel = make_rel("свобода","выбор", qxfx0_types::RelationType::RelPresupposes);
        let result = gen.verbalize(&rel, &DiscourseStyle::default(), 0, &mut vec![]);
        assert!(!result.is_empty());
        assert!(result.contains("свобода"));
    }

    #[test]
    fn test_verbalize_deterministic() {
        let gen = SyntacticGenerator::new();
        let rel = make_rel("свобода","выбор", qxfx0_types::RelationType::RelPresupposes);
        let a = gen.verbalize(&rel, &DiscourseStyle::default(), 42, &mut vec![]);
        let b = gen.verbalize(&rel, &DiscourseStyle::default(), 42, &mut vec![]);
        assert_eq!(a, b);
    }

    #[test]
    fn test_template_selection_no_duplicate_indices() {
        let gen = SyntacticGenerator::new();
        let rel = make_rel("свобода","выбор", qxfx0_types::RelationType::RelPresupposes);
        let mut used = vec![];
        let a = gen.verbalize(&rel, &DiscourseStyle::default(), 0, &mut used);
        let b = gen.verbalize(&rel, &DiscourseStyle::default(), 20, &mut used);
        assert!(!a.is_empty()); assert!(!b.is_empty());
        assert_eq!(used.len(), 2);
    }

    #[test]
    fn test_verbalize_fallback_for_untemplated_type() {
        let gen = SyntacticGenerator::new();
        let rel = make_rel("x","y", qxfx0_types::RelationType::RelConnects);
        let result = gen.verbalize(&rel, &DiscourseStyle::default(), 0, &mut vec![]);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_hedge_applied() {
        let gen = SyntacticGenerator::new();
        let rel = make_rel("свобода","выбор", qxfx0_types::RelationType::RelPresupposes);
        let style = DiscourseStyle { hedging: 0.8, ..Default::default() };
        let result = gen.verbalize(&rel, &style, 0, &mut vec![]);
        assert!(!result.is_empty());
    }
}
