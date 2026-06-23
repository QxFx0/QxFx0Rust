use std::collections::BTreeMap;

/// Russian morphology engine — replaces GF (Grammatical Framework).
/// Handles 6-case inflection for philosophical dialogue.
/// Deterministic: same input → same output, always.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Case {
    Nominative,
    Genitive,
    Dative,
    Accusative,
    Instrumental,
    Prepositional,
}

/// Morphology data — case forms lookup.
#[derive(Debug, Clone, Default)]
pub struct MorphologyData {
    pub nominative: BTreeMap<String, String>,
    pub genitive: BTreeMap<String, String>,
    pub dative: BTreeMap<String, String>,
    pub accusative: BTreeMap<String, String>,
    pub instrumental: BTreeMap<String, String>,
    pub prepositional: BTreeMap<String, String>,
}

impl MorphologyData {
    /// Create empty morphology data.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from seed data with common philosophical terms.
    pub fn with_seed() -> Self {
        let mut morph = Self::new();

        // Nominative is identity — word maps to itself
        let topics = [
            "свобода",
            "произвол",
            "ответственность",
            "истина",
            "мнение",
            "память",
            "воспоминание",
            "сознание",
            "самосознание",
            "вера",
            "красота",
            "долг",
            "доверие",
            "страх",
            "надежда",
            "справедливость",
            "время",
            "разум",
            "бытие",
            "история",
            "язык",
            "воля",
            "смерть",
            "одиночество",
            "любовь",
            "труд",
            "покой",
            "власть",
            "правда",
            "молчание",
            "смысл",
            "выбор",
            "сознание",
            "добро",
            "зло",
        ];

        for topic in &topics {
            morph
                .nominative
                .insert(topic.to_string(), topic.to_string());
            morph
                .genitive
                .insert(topic.to_string(), inflect_genitive(topic));
            morph
                .dative
                .insert(topic.to_string(), inflect_dative(topic));
            morph
                .accusative
                .insert(topic.to_string(), inflect_accusative(topic));
            morph
                .instrumental
                .insert(topic.to_string(), inflect_instrumental(topic));
            morph
                .prepositional
                .insert(topic.to_string(), inflect_prepositional(topic));
        }

        morph
    }

    /// Convert a word to the specified case.
    pub fn to_case(&self, case: Case, word: &str) -> String {
        let lower = word.to_lowercase();
        let table = match case {
            Case::Nominative => &self.nominative,
            Case::Genitive => &self.genitive,
            Case::Dative => &self.dative,
            Case::Accusative => &self.accusative,
            Case::Instrumental => &self.instrumental,
            Case::Prepositional => &self.prepositional,
        };

        // Try exact lookup first
        if let Some(form) = table.get(&lower) {
            return form.clone();
        }

        // Fallback: heuristic inflection
        heuristic_inflect(case, &lower)
    }

    /// Convert to nominative case.
    pub fn to_nominative(&self, word: &str) -> String {
        self.to_case(Case::Nominative, word)
    }

    /// Convert to genitive case.
    pub fn to_genitive(&self, word: &str) -> String {
        self.to_case(Case::Genitive, word)
    }

    /// Convert to instrumental case.
    pub fn to_instrumental(&self, word: &str) -> String {
        self.to_case(Case::Instrumental, word)
    }

    /// Convert to prepositional case.
    pub fn to_prepositional(&self, word: &str) -> String {
        self.to_case(Case::Prepositional, word)
    }
}

/// Heuristic Russian inflection — suffix-based rules.
/// Handles common noun patterns: -а, -я, -ь, -о, -е, consonant.
fn heuristic_inflect(case: Case, word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }

    match case {
        Case::Nominative => word.to_string(),
        Case::Genitive => inflect_genitive(word),
        Case::Dative => inflect_dative(word),
        Case::Accusative => inflect_accusative(word),
        Case::Instrumental => inflect_instrumental(word),
        Case::Prepositional => inflect_prepositional(word),
    }
}

fn drop_last(w: &str) -> String {
    let chars: Vec<char> = w.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    chars[..chars.len() - 1].iter().collect()
}

fn last_char(w: &str) -> char {
    w.chars().last().unwrap_or(' ')
}

fn inflect_genitive(w: &str) -> String {
    let lc = last_char(w);
    match lc {
        'а' => drop_last(w) + "ы",
        'я' => drop_last(w) + "и",
        'ь' => drop_last(w) + "и",
        'о' => drop_last(w) + "а",
        'е' => drop_last(w) + "я",
        'й' => drop_last(w) + "я",
        _ => {
            if is_consonant(lc) {
                w.to_string() + "а"
            } else {
                w.to_string()
            }
        }
    }
}

fn inflect_dative(w: &str) -> String {
    let lc = last_char(w);
    match lc {
        'а' => drop_last(w) + "е",
        'я' => drop_last(w) + "е",
        'ь' => drop_last(w) + "и",
        'о' => drop_last(w) + "у",
        'е' => drop_last(w) + "ю",
        'й' => drop_last(w) + "ю",
        _ => {
            if is_consonant(lc) {
                w.to_string() + "у"
            } else {
                w.to_string()
            }
        }
    }
}

fn inflect_accusative(w: &str) -> String {
    let lc = last_char(w);
    match lc {
        'а' => drop_last(w) + "у",
        'я' => drop_last(w) + "ю",
        'ь' => drop_last(w) + "ь",        // inanimate — same as nominative
        'о' | 'е' | 'й' => w.to_string(), // inanimate — same as nominative
        _ => w.to_string(),               // consonant — inanimate, same as nominative
    }
}

fn inflect_instrumental(w: &str) -> String {
    let lc = last_char(w);
    match lc {
        'а' => drop_last(w) + "ой",
        'я' => drop_last(w) + "ей",
        'ь' => drop_last(w) + "ью",
        'о' => drop_last(w) + "ом",
        'е' => drop_last(w) + "ем",
        'й' => drop_last(w) + "ем",
        _ => {
            if is_consonant(lc) {
                w.to_string() + "ом"
            } else {
                w.to_string()
            }
        }
    }
}

fn inflect_prepositional(w: &str) -> String {
    let lc = last_char(w);
    match lc {
        'а' => drop_last(w) + "е",
        'я' => drop_last(w) + "е",
        'ь' => drop_last(w) + "и",
        'о' => drop_last(w) + "е",
        'е' => drop_last(w) + "е",
        'й' => drop_last(w) + "е",
        _ => {
            if is_consonant(lc) {
                w.to_string() + "е"
            } else {
                w.to_string()
            }
        }
    }
}

fn is_consonant(c: char) -> bool {
    !matches!(
        c,
        'а' | 'е' | 'ё' | 'и' | 'о' | 'у' | 'ы' | 'э' | 'ю' | 'я' | 'ь' | 'й'
    )
}

/// Strip a leading preposition from a phrase.
/// "с ответственностью" → "ответственностью"
pub fn strip_preposition(text: &str) -> String {
    let prepositions = [
        "с ",
        "со ",
        "на ",
        "об ",
        "от ",
        "к ",
        "из ",
        "через ",
        "для ",
        "о ",
    ];
    let lower = text.to_lowercase();
    for prep in &prepositions {
        if lower.starts_with(prep) {
            return text[prep.len()..].trim().to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nominative_identity() {
        let morph = MorphologyData::with_seed();
        assert_eq!(morph.to_nominative("свобода"), "свобода");
    }

    #[test]
    fn test_genitive_feminine_a() {
        let morph = MorphologyData::with_seed();
        let gen = morph.to_genitive("свобода");
        assert!(
            gen.ends_with("ы") || gen.ends_with("и"),
            "genitive of -а should end in -ы or -и, got: {}",
            gen
        );
    }

    #[test]
    fn test_instrumental_feminine_a() {
        let morph = MorphologyData::with_seed();
        let inst = morph.to_instrumental("свобода");
        assert!(
            inst.ends_with("ой") || inst.ends_with("ей"),
            "instrumental of -а should end in -ой or -ей, got: {}",
            inst
        );
    }

    #[test]
    fn test_deterministic() {
        let m1 = MorphologyData::with_seed();
        let m2 = MorphologyData::with_seed();
        assert_eq!(m1.to_genitive("истина"), m2.to_genitive("истина"));
    }

    #[test]
    fn test_heuristic_consonant() {
        let morph = MorphologyData::new();
        let gen = morph.to_genitive("разум");
        assert_eq!(gen, "разума");
    }

    #[test]
    fn test_heuristic_soft_sign() {
        let morph = MorphologyData::new();
        let gen = morph.to_genitive("долг");
        assert_eq!(gen, "долга");
    }

    #[test]
    fn test_prepositional() {
        let morph = MorphologyData::with_seed();
        let prep = morph.to_prepositional("свобода");
        assert!(
            prep.ends_with("е"),
            "prepositional should end in -е, got: {}",
            prep
        );
    }

    #[test]
    fn test_strip_preposition() {
        assert_eq!(strip_preposition("с ответственностью"), "ответственностью");
        assert_eq!(strip_preposition("на соответствие"), "соответствие");
        assert_eq!(strip_preposition("свобода"), "свобода");
    }

    #[test]
    fn test_all_cases_produce_output() {
        let morph = MorphologyData::with_seed();
        for topic in ["свобода", "истина", "сознание", "ответственность"]
        {
            for case in [
                Case::Nominative,
                Case::Genitive,
                Case::Dative,
                Case::Accusative,
                Case::Instrumental,
                Case::Prepositional,
            ] {
                let result = morph.to_case(case, topic);
                assert!(
                    !result.is_empty(),
                    "case {:?} of {} should not be empty",
                    case,
                    topic
                );
            }
        }
    }
}
