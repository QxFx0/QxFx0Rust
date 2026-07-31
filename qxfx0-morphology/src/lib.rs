pub mod runtime;
pub use runtime::{
    get_runtime, load_from_directory, MorphologyError, MorphologyResult, MorphologyRuntime,
    MorphologyStats, EMBEDDED_BUNDLE_SIZE_BYTES, EMBEDDED_LEXEMES_SIZE_BYTES,
    EMBEDDED_MANIFEST_SIZE_BYTES,
};

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Russian morphology engine — replaces GF (Grammatical Framework).
/// Handles 6-case inflection for philosophical dialogue.
/// Deterministic: same input → same output, always.
pub use qxfx0_types::morphology::{
    Animacy, Case, Gender, GrammarFeatures, InflectionForms, LexemeCandidate, LexemeEntry,
    MorphologyBundleManifest, MorphologyLookup, Number, PartOfSpeech, SourceTier,
};

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
        seed_cached().clone()
    }

    /// Lemmatize a word back to its nominative form.
    /// Checks the seed dictionary across all cases.
    pub fn lemmatize(&self, word: &str) -> String {
        let lower = word.to_lowercase();

        // 1. Check if it's already nominative
        if self.nominative.contains_key(&lower) {
            return lower;
        }

        // 2. Check other case maps for the value
        let maps = [
            &self.genitive,
            &self.dative,
            &self.accusative,
            &self.instrumental,
            &self.prepositional,
        ];

        for map in maps {
            for (nom, form) in map {
                if form == &lower {
                    return nom.clone();
                }
            }
        }

        // 3. Fallback: return original word
        lower
    }

    fn build_seed() -> Self {
        let mut morph = Self::new();

        let feminine_a = [
            "свобода",
            "истина",
            "вера",
            "красота",
            "надежда",
            "воля",
            "правда",
            "культура",
            "традиция",
            "гармония",
            "причина",
            "эмоция",
            "интерпретация",
            "коммуникация",
            "природа",
            "игра",
            "жизнь",
            "работа",
            "дружба",
            "семья",
            "музыка",
            "наука",
            "технология",
            "мотивация",
            "демократия",
            "политика",
            "экономика",
            "дисциплина",
            "информация",
            "система",
            "этика",
            "интуиция",
        ];
        let feminine_soft = [
            "мысль",
            "ответственность",
            "справедливость",
            "воспроизводимость",
            "способность",
            "осмысленность",
            "привязанность",
            "ревность",
            "реальность",
            "вседозволенность",
            "сущность",
            "мудрость",
            "скорость",
            "личность",
        ];
        let masculine_consonant = [
            "произвол",
            "разум",
            "язык",
            "долг",
            "страх",
            "труд",
            "покой",
            "выбор",
            "самоотчёт",
            "рефлекс",
            "закон",
            "нейрон",
            "свидетель",
            "опыт",
            "прогресс",
            "порядок",
            "хаос",
            "поступок",
            "знак",
            "символ",
            "человек",
            "дом",
            "стресс",
            "ресурс",
            "обмен",
            "конфликт",
            "интерес",
            "успех",
            "талант",
            "гнев",
            "метод",
            "процесс",
            "результат",
            "диалог",
            "спор",
        ];
        let masculine_soft: &[&str] = &[];
        let neuter = [
            "мнение",
            "бытие",
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
            "доверие",
            "будущее",
            "последствие",
            "действие",
            "условие",
            "знание",
            "понимание",
            "сомнение",
            "убеждение",
            "доказательство",
            "заблуждение",
            "существование",
            "воображение",
            "мышление",
            "желание",
            "значение",
            "творчество",
            "искусство",
            "прекрасное",
            "общество",
            "восприятие",
            "следствие",
            "понятие",
            "страдание",
            "чувство",
            "образование",
            "путешествие",
            "здоровье",
            "познание",
            "развитие",
            "равенство",
            "государство",
            "отношение",
            "уважение",
            "внимание",
            "призвание",
            "качество",
            "спокойствие",
        ];
        let irregulars: &[(&str, &str, &str, &str, &str, &str)] = &[
            (
                "история",
                "истории",
                "истории",
                "историю",
                "историей",
                "истории",
            ),
            ("смерть", "смерти", "смерти", "смерть", "смертью", "смерти"),
            (
                "последствия",
                "последствий",
                "последствиям",
                "последствия",
                "последствиями",
                "последствиях",
            ),
            (
                "совесть",
                "совести",
                "совести",
                "совесть",
                "совестью",
                "совести",
            ),
            ("честь", "чести", "чести", "честь", "честью", "чести"),
            ("любовь", "любви", "любви", "любовь", "любовью", "любви"),
            ("мысль", "мысли", "мысли", "мысль", "мыслью", "мысли"),
            ("власть", "власти", "власти", "власть", "властью", "власти"),
            ("память", "памяти", "памяти", "память", "памятью", "памяти"),
            (
                "добродетель",
                "добродетели",
                "добродетели",
                "добродетель",
                "добродетелью",
                "добродетели",
            ),
            (
                "возможность",
                "возможности",
                "возможности",
                "возможность",
                "возможностью",
                "возможности",
            ),
            (
                "необходимость",
                "необходимости",
                "необходимости",
                "необходимость",
                "необходимостью",
                "необходимости",
            ),
            (
                "случайность",
                "случайности",
                "случайности",
                "случайность",
                "случайностью",
                "случайности",
            ),
            (
                "время",
                "времени",
                "времени",
                "время",
                "временем",
                "времени",
            ),
            ("знание", "знания", "знанию", "знание", "знанием", "знании"),
            (
                "сознание",
                "сознания",
                "сознанию",
                "сознание",
                "сознанием",
                "сознании",
            ),
            (
                "воспоминание",
                "воспоминания",
                "воспоминанию",
                "воспоминание",
                "воспоминанием",
                "воспоминании",
            ),
            (
                "понимание",
                "понимания",
                "пониманию",
                "понимание",
                "пониманием",
                "понимании",
            ),
            (
                "желание",
                "желания",
                "желанию",
                "желание",
                "желанием",
                "желании",
            ),
            (
                "существование",
                "существования",
                "существованию",
                "существование",
                "существованием",
                "существовании",
            ),
            (
                "значение",
                "значения",
                "значению",
                "значение",
                "значением",
                "значении",
            ),
            (
                "творчество",
                "творчества",
                "творчеству",
                "творчество",
                "творчеством",
                "творчестве",
            ),
            (
                "искусство",
                "искусства",
                "искусству",
                "искусство",
                "искусством",
                "искусстве",
            ),
            (
                "общество",
                "общества",
                "обществу",
                "общество",
                "обществом",
                "обществе",
            ),
            (
                "мышление",
                "мышления",
                "мышлению",
                "мышление",
                "мышлением",
                "мышлении",
            ),
            ("жизнь", "жизни", "жизни", "жизнь", "жизнью", "жизни"),
            (
                "счастье",
                "счастья",
                "счастью",
                "счастье",
                "счастьем",
                "счастье",
            ),
            ("смысл", "смысла", "смыслу", "смысл", "смыслом", "смысле"),
            (
                "отношения",
                "отношений",
                "отношениям",
                "отношения",
                "отношениями",
                "отношениях",
            ),
            (
                "радость",
                "радости",
                "радости",
                "радость",
                "радостью",
                "радости",
            ),
            ("грусть", "грусти", "грусти", "грусть", "грустью", "грусти"),
            ("мораль", "морали", "морали", "мораль", "моралью", "морали"),
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

    pub fn detect_gender(word: &str) -> Gender {
        let lc = word.chars().last().unwrap_or(' ');
        match lc {
            'а' | 'я' => Gender::Feminine,
            'о' | 'е' | 'ё' => Gender::Neuter,
            'ь' => {
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

fn inflect_feminine_a(w: &str) -> InflectedForms {
    let stem = drop_last(w);
    let is_iya = w.ends_with("ия");
    let is_soft_ya = w.ends_with("я") && !is_iya;
    let is_7letter = stem.ends_with(['к', 'г', 'х', 'ж', 'ш', 'щ', 'ч']);

    let gen_suffix = if is_iya || is_soft_ya || is_7letter {
        "и"
    } else {
        "ы"
    };
    let datprep_suffix = if is_iya { "и" } else { "е" };
    let acc_suffix = if is_iya || is_soft_ya { "ю" } else { "у" };
    let inst_suffix = if is_iya || is_soft_ya { "ей" } else { "ой" };
    InflectedForms {
        nom: w.into(),
        gen: format!("{}{}", stem, gen_suffix),
        dat: format!("{}{}", stem, datprep_suffix),
        acc: format!("{}{}", stem, acc_suffix),
        inst: format!("{}{}", stem, inst_suffix),
        prep: format!("{}{}", stem, datprep_suffix),
    }
}

fn inflect_feminine_soft(w: &str) -> InflectedForms {
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
        _ => InflectedForms {
            nom: w.into(),
            gen: format!("{}а", w),
            dat: format!("{}у", w),
            acc: w.into(),
            inst: format!("{}ом", w),
            prep: format!("{}е", w),
        },
    }
}

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
        Gender::Unknown => inflect_neuter(word),
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

fn seed_cached() -> &'static MorphologyData {
    static SEED: OnceLock<MorphologyData> = OnceLock::new();
    SEED.get_or_init(MorphologyData::build_seed)
}

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

pub fn prep_about(morph: &MorphologyData, word: &str) -> String {
    let prep = morph.to_prepositional(word);
    if prep.starts_with('а')
        || prep.starts_with('о')
        || prep.starts_with('у')
        || prep.starts_with('и')
        || prep.starts_with('э')
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
        assert_eq!(morph.to_genitive("наука"), "науки");
        assert_eq!(morph.to_genitive("стол"), "стола");
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
        assert_eq!(prep_about(&morph, "свобода"), "о свободе");
        assert_eq!(prep_about(&morph, "ответственность"), "об ответственности");
        assert_eq!(prep_about(&morph, "истина"), "об истине");
        assert_eq!(prep_about(&morph, "язык"), "о языке");
    }

    #[test]
    fn test_rc_pilot_irregular_forms() {
        let morph = MorphologyData::with_seed();
        assert_eq!(morph.to_genitive("бытие"), "бытия");
        assert_eq!(morph.to_dative("бытие"), "бытию");
        assert_eq!(morph.to_prepositional("бытие"), "бытии");
        assert_eq!(morph.to_genitive("последствия"), "последствий");
        assert_eq!(morph.to_instrumental("последствия"), "последствиями");
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
        assert_eq!(morph.to_accusative("разум"), "разум");
        assert_eq!(morph.to_accusative("сознание"), "сознание");
    }

    #[test]
    fn test_ost_feminine_detection() {
        assert_eq!(
            MorphologyData::detect_gender("ответственность"),
            Gender::Feminine
        );
        assert_eq!(
            MorphologyData::detect_gender("справедливость"),
            Gender::Feminine
        );
    }

    #[test]
    fn test_morphology_no_duplicate_seed() {
        let morph = MorphologyData::with_seed();
        assert_ne!(morph.to_genitive("смысл"), "смыслы");
        assert_ne!(morph.to_instrumental("смысл"), "смыслой");
        assert_eq!(morph.to_genitive("смысл"), "смысла");
        assert_eq!(morph.to_instrumental("смысл"), "смыслом");
        assert_eq!(morph.to_prepositional("смысл"), "смысле");
    }

    #[test]
    fn test_ost_feminine_3rd_declension() {
        let morph = MorphologyData::with_seed();
        assert_eq!(morph.to_genitive("ответственность"), "ответственности");
        assert_eq!(morph.to_dative("ответственность"), "ответственности");
        assert_eq!(morph.to_prepositional("ответственность"), "ответственности");
        assert_eq!(morph.to_accusative("ответственность"), "ответственность");
        assert_eq!(morph.to_instrumental("ответственность"), "ответственностью");

        assert_eq!(morph.to_genitive("справедливость"), "справедливости");
        assert_eq!(morph.to_prepositional("справедливость"), "справедливости");
        assert_eq!(morph.to_instrumental("справедливость"), "справедливостью");

        assert_eq!(morph.to_genitive("способность"), "способности");
        assert_eq!(morph.to_prepositional("способность"), "способности");

        assert_eq!(morph.to_genitive("реальность"), "реальности");
        assert_eq!(morph.to_prepositional("реальность"), "реальности");
        assert_eq!(morph.to_accusative("реальность"), "реальность");
        assert_eq!(morph.to_instrumental("реальность"), "реальностью");

        assert_eq!(morph.to_genitive("мудрость"), "мудрости");
        assert_eq!(morph.to_prepositional("мудрость"), "мудрости");
    }

    #[test]
    fn test_iya_feminine_soft_stem() {
        let morph = MorphologyData::with_seed();
        assert_eq!(morph.to_genitive("интуиция"), "интуиции");
        assert_eq!(morph.to_dative("интуиция"), "интуиции");
        assert_eq!(morph.to_prepositional("интуиция"), "интуиции");
        assert_eq!(morph.to_accusative("интуиция"), "интуицию");
        assert_eq!(morph.to_instrumental("интуиция"), "интуицией");

        assert_eq!(morph.to_genitive("демократия"), "демократии");
        assert_eq!(morph.to_accusative("демократия"), "демократию");
        assert_eq!(morph.to_instrumental("демократия"), "демократией");

        assert_eq!(morph.to_genitive("мотивация"), "мотивации");
        assert_eq!(morph.to_accusative("мотивация"), "мотивацию");
        assert_eq!(morph.to_instrumental("мотивация"), "мотивацией");

        assert_eq!(morph.to_genitive("интерпретация"), "интерпретации");
        assert_eq!(morph.to_accusative("интерпретация"), "интерпретацию");
        assert_eq!(morph.to_instrumental("интерпретация"), "интерпретацией");
    }

    #[test]
    fn test_soft_ya_feminine() {
        let morph = MorphologyData::with_seed();
        assert_eq!(morph.to_genitive("семья"), "семьи");
        assert_eq!(morph.to_dative("семья"), "семье");
        assert_eq!(morph.to_prepositional("семья"), "семье");
        assert_eq!(morph.to_accusative("семья"), "семью");
        assert_eq!(morph.to_instrumental("семья"), "семьей");

        assert_eq!(morph.to_genitive("воля"), "воли");
        assert_eq!(morph.to_dative("воля"), "воле");
        assert_eq!(morph.to_accusative("воля"), "волю");
        assert_eq!(morph.to_instrumental("воля"), "волей");
    }

    #[test]
    fn test_lemmatize() {
        let morph = MorphologyData::with_seed();
        assert_eq!(morph.lemmatize("свобода"), "свобода");
        assert_eq!(morph.lemmatize("свободой"), "свобода");
        assert_eq!(morph.lemmatize("свободы"), "свобода");
        assert_eq!(morph.lemmatize("разумом"), "разум");
        assert_eq!(morph.lemmatize("сознании"), "сознание");
        assert_eq!(morph.lemmatize("абракадабра"), "абракадабра");
    }
}
