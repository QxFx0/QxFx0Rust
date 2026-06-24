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

/// Gender of a Russian noun.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Masculine,
    Feminine,
    Neuter,
}

/// Morphology data — case forms lookup + heuristic rules.
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
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with seed data covering philosophical terms + common objects.
    pub fn with_seed() -> Self {
        let mut morph = Self::new();

        // Feminine nouns ending in -а/-я
        let feminine_a = [
            "свобода",
            "ответственность",
            "истина",
            "память",
            "вера",
            "красота",
            "надежда",
            "справедливость",
            "воля",
            "любовь",
            "власть",
            "правда",
            "смысл",
        ];
        // Feminine ending in -ь
        let feminine_soft = ["доверие", "мысль"];
        // Masculine consonant-ending
        let masculine_consonant = [
            "произвол",
            "разум",
            "бытие",
            "язык",
            "долг",
            "страх",
            "труд",
            "покой",
            "выбор",
        ];
        // Masculine ending in -ь
        let masculine_soft = ["смысл"];
        // Neuter ending in -о/-е
        let neuter = [
            "мнение",
            "воспоминание",
            "сознание",
            "самосознание",
            "время",
            "одиночество",
            "молчание",
            "добро",
            "зло",
            "принуждение",
            "отсутствие",
        ];
        // Irregulars
        let irregulars: &[(&str, &str, &str, &str, &str, &str)] = &[
            // (nom, gen, dat, acc, inst, prep)
            (
                "история",
                "истории",
                "истории",
                "историю",
                "историей",
                "истории",
            ),
            ("смерть", "смерти", "смерти", "смерть", "смертью", "смерти"),
        ];

        for word in feminine_a {
            insert_all(&mut morph, word, &inflect_feminine_a(word));
        }
        for word in feminine_soft {
            insert_all(&mut morph, word, &inflect_feminine_soft(word));
        }
        for word in masculine_consonant {
            insert_all(&mut morph, word, &inflect_masculine_consonant(word));
        }
        for word in masculine_soft {
            insert_all(&mut morph, word, &inflect_masculine_soft(word));
        }
        for word in neuter {
            insert_all(&mut morph, word, &inflect_neuter(word));
        }
        for &(nom, gen, dat, acc, inst, prep) in irregulars {
            morph.nominative.insert(nom.into(), nom.into());
            morph.genitive.insert(nom.into(), gen.into());
            morph.dative.insert(nom.into(), dat.into());
            morph.accusative.insert(nom.into(), acc.into());
            morph.instrumental.insert(nom.into(), inst.into());
            morph.prepositional.insert(nom.into(), prep.into());
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

        if let Some(form) = table.get(&lower) {
            return form.clone();
        }

        heuristic_inflect(case, &lower)
    }

    pub fn to_nominative(&self, word: &str) -> String {
        self.to_case(Case::Nominative, word)
    }
    pub fn to_genitive(&self, word: &str) -> String {
        self.to_case(Case::Genitive, word)
    }
    pub fn to_dative(&self, word: &str) -> String {
        self.to_case(Case::Dative, word)
    }
    pub fn to_accusative(&self, word: &str) -> String {
        self.to_case(Case::Accusative, word)
    }
    pub fn to_instrumental(&self, word: &str) -> String {
        self.to_case(Case::Instrumental, word)
    }
    pub fn to_prepositional(&self, word: &str) -> String {
        self.to_case(Case::Prepositional, word)
    }

    /// Detect gender from word ending.
    pub fn detect_gender(word: &str) -> Gender {
        let lc = word.chars().last().unwrap_or(' ');
        match lc {
            'а' | 'я' => Gender::Feminine,
            'о' | 'е' | 'ё' => Gender::Neuter,
            'ь' => {
                // Could be masculine or feminine — heuristic: check for -тель, -ость
                if word.ends_with("ость") || word.ends_with("сть") {
                    Gender::Feminine
                } else {
                    Gender::Masculine
                }
            }
            _ => Gender::Masculine,
        }
    }
}

fn insert_all(morph: &mut MorphologyData, word: &str, forms: &InflectedForms) {
    morph.nominative.insert(word.into(), forms.nom.clone());
    morph.genitive.insert(word.into(), forms.gen.clone());
    morph.dative.insert(word.into(), forms.dat.clone());
    morph.accusative.insert(word.into(), forms.acc.clone());
    morph.instrumental.insert(word.into(), forms.inst.clone());
    morph.prepositional.insert(word.into(), forms.prep.clone());
}

struct InflectedForms {
    nom: String,
    gen: String,
    dat: String,
    acc: String,
    inst: String,
    prep: String,
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

// Gender-specific inflection patterns

fn inflect_feminine_a(w: &str) -> InflectedForms {
    // -а → -ы/-и (gen, 7-letter rule), -е (dat/prep), -у (acc), -ой (inst)
    let stem = drop_last(w);
    // 7-letter spelling rule: after к/г/х/ж/ш/щ/ч use -и instead of -ы
    let gen_suffix = if stem.ends_with(['к', 'г', 'х', 'ж', 'ш', 'щ', 'ч']) {
        "и"
    } else {
        "ы"
    };
    InflectedForms {
        nom: w.into(),
        gen: format!("{}{}", stem, gen_suffix),
        dat: format!("{}е", stem),
        acc: format!("{}у", stem),
        inst: format!("{}ой", stem),
        prep: format!("{}е", stem),
    }
}

fn inflect_feminine_soft(w: &str) -> InflectedForms {
    // -ь → -и (gen/dat/prep), -ь (acc), -ью (inst)
    let stem = drop_last(w);
    InflectedForms {
        nom: w.into(),
        gen: format!("{}и", stem),
        dat: format!("{}и", stem),
        acc: w.into(),
        inst: format!("{}ью", stem),
        prep: format!("{}и", stem),
    }
}

fn inflect_masculine_consonant(w: &str) -> InflectedForms {
    // consonant → -а (gen), -у (dat), consonant (acc), -ом (inst), -е (prep)
    InflectedForms {
        nom: w.into(),
        gen: format!("{}а", w),
        dat: format!("{}у", w),
        acc: w.into(),
        inst: format!("{}ом", w),
        prep: format!("{}е", w),
    }
}

fn inflect_masculine_soft(w: &str) -> InflectedForms {
    // -ь → -я (gen), -ю (dat), -ь (acc), -ем (inst), -е (prep)
    let stem = drop_last(w);
    InflectedForms {
        nom: w.into(),
        gen: format!("{}я", stem),
        dat: format!("{}ю", stem),
        acc: w.into(),
        inst: format!("{}ем", stem),
        prep: format!("{}е", stem),
    }
}

fn inflect_neuter(w: &str) -> InflectedForms {
    let lc = last_char(w);
    let stem = drop_last(w);
    match lc {
        'о' => InflectedForms {
            nom: w.into(),
            gen: format!("{}а", stem),
            dat: format!("{}у", stem),
            acc: w.into(),
            inst: format!("{}ом", stem),
            prep: format!("{}е", stem),
        },
        'е' | 'ё' => InflectedForms {
            nom: w.into(),
            gen: format!("{}я", stem),
            dat: format!("{}ю", stem),
            acc: w.into(),
            inst: format!("{}ем", stem),
            prep: format!("{}и", stem),
        },
        _ => {
            // Fallback for words that don't end in -о/-е: treat as inanimate
            InflectedForms {
                nom: w.into(),
                gen: format!("{}а", w),
                dat: format!("{}у", w),
                acc: w.into(),
                inst: format!("{}ом", w),
                prep: format!("{}е", w),
            }
        }
    }
}

/// Heuristic fallback — detect gender and apply rules.
fn heuristic_inflect(case: Case, word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }

    let gender = MorphologyData::detect_gender(word);
    let forms = match gender {
        Gender::Feminine => {
            if word.ends_with('а') || word.ends_with('я') {
                inflect_feminine_a(word)
            } else {
                inflect_feminine_soft(word)
            }
        }
        Gender::Masculine => {
            if word.ends_with('ь') {
                inflect_masculine_soft(word)
            } else {
                inflect_masculine_consonant(word)
            }
        }
        Gender::Neuter => inflect_neuter(word),
    };

    match case {
        Case::Nominative => forms.nom,
        Case::Genitive => forms.gen,
        Case::Dative => forms.dat,
        Case::Accusative => forms.acc,
        Case::Instrumental => forms.inst,
        Case::Prepositional => forms.prep,
    }
}

/// Strip a leading preposition from a phrase.
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

/// Build a prepositional phrase: "о + [prepositional form]"
pub fn prep_about(morph: &MorphologyData, word: &str) -> String {
    let prep = morph.to_prepositional(word);
    if prep.starts_with('а')
        || prep.starts_with('о')
        || prep.starts_with('у')
        || prep.starts_with('и')
    {
        format!("об {}", prep)
    } else {
        format!("о {}", prep)
    }
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
        assert_eq!(gen, "свободы");
    }

    #[test]
    fn test_instrumental_feminine_a() {
        let morph = MorphologyData::with_seed();
        let inst = morph.to_instrumental("свобода");
        assert_eq!(inst, "свободой");
    }

    #[test]
    fn test_dative_masculine_consonant() {
        let morph = MorphologyData::with_seed();
        let dat = morph.to_dative("разум");
        assert_eq!(dat, "разуму");
    }

    #[test]
    fn test_genitive_neuter() {
        let morph = MorphologyData::with_seed();
        let gen = morph.to_genitive("сознание");
        assert_eq!(gen, "сознания");
    }

    #[test]
    fn test_prepositional_neuter_ie() {
        let morph = MorphologyData::with_seed();
        let prep = morph.to_prepositional("сознание");
        assert_eq!(prep, "сознании");
    }

    #[test]
    fn test_instrumental_masculine_consonant() {
        let morph = MorphologyData::with_seed();
        let inst = morph.to_instrumental("долг");
        assert_eq!(inst, "долгом");
    }

    #[test]
    fn test_irregular_smert() {
        let morph = MorphologyData::with_seed();
        assert_eq!(morph.to_genitive("смерть"), "смерти");
        assert_eq!(morph.to_instrumental("смерть"), "смертью");
    }

    #[test]
    fn test_deterministic() {
        let m1 = MorphologyData::with_seed();
        let m2 = MorphologyData::with_seed();
        assert_eq!(m1.to_genitive("истина"), m2.to_genitive("истина"));
    }

    #[test]
    fn test_heuristic_unknown_word() {
        let morph = MorphologyData::new();
        // Unknown feminine -а word
        assert_eq!(morph.to_genitive("наука"), "науки");
        // Unknown masculine consonant word
        assert_eq!(morph.to_genitive("стол"), "стола");
        // Unknown neuter -о word
        assert_eq!(morph.to_genitive("окно"), "окна");
    }

    #[test]
    fn test_gender_detection() {
        assert_eq!(MorphologyData::detect_gender("свобода"), Gender::Feminine);
        assert_eq!(MorphologyData::detect_gender("разум"), Gender::Masculine);
        assert_eq!(MorphologyData::detect_gender("сознание"), Gender::Neuter);
        assert_eq!(
            MorphologyData::detect_gender("ответственность"),
            Gender::Feminine
        );
    }

    #[test]
    fn test_strip_preposition() {
        assert_eq!(strip_preposition("с ответственностью"), "ответственностью");
        assert_eq!(strip_preposition("на соответствие"), "соответствие");
        assert_eq!(strip_preposition("свобода"), "свобода");
    }

    #[test]
    fn test_prep_about() {
        let morph = MorphologyData::with_seed();
        let phrase = prep_about(&morph, "свобода");
        assert!(phrase.starts_with("о ") || phrase.starts_with("об "));
        assert!(phrase.contains("свобод"));
    }

    #[test]
    fn test_all_cases_all_topics() {
        let morph = MorphologyData::with_seed();
        let topics = [
            "свобода",
            "истина",
            "сознание",
            "ответственность",
            "разум",
            "долг",
            "вера",
            "память",
            "время",
            "смысл",
        ];
        for topic in &topics {
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

    #[test]
    fn test_accusative_inanimate_same_as_nominative() {
        let morph = MorphologyData::with_seed();
        // Inanimate nouns: acc = nom
        assert_eq!(morph.to_accusative("разум"), "разум");
        assert_eq!(morph.to_accusative("сознание"), "сознание");
    }

    #[test]
    fn test_ost_feminine_detection() {
        // Words ending in -ость are feminine
        assert_eq!(
            MorphologyData::detect_gender("ответственность"),
            Gender::Feminine
        );
        assert_eq!(
            MorphologyData::detect_gender("справедливость"),
            Gender::Feminine
        );
    }
}
