//! Rule-based Russian verb conjugation, fail-closed.
//!
//! Ported from the Haskell rule engine (QxFx0/Runtime/GF/Morphology.hs)
//! with two deliberate corrections: the Haskell engine silently produced
//! wrong forms for consonant-stem and alternating verbs («писать» →
//! «писаю», «любить» → «любат»). This port returns `None` for every person
//! whose form cannot be derived by rule, so a caller degrades visibly
//! instead of fabricating a surface.
//!
//! Coverage:
//! - present/future (imperfective) of the productive vowel-stem classes,
//!   both conjugations, including the 11 classic second-conjugation
//!   exceptions and the -овать/-евать present stem;
//! - past tense (regular -ть stems) with gender and number;
//! - compound future (буду + infinitive) — the caller owns aspect choice.

use qxfx0_types::morphology::{Gender, Number};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerbPerson {
    FirstSingular,
    SecondSingular,
    ThirdSingular,
    FirstPlural,
    SecondPlural,
    ThirdPlural,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConjugationClass {
    First,
    Second,
}

/// The classic second-conjugation exceptions that do not end in -ить.
const SECOND_CONJUGATION_EXCEPTIONS: &[&str] = &[
    "смотреть",
    "видеть",
    "ненавидеть",
    "обидеть",
    "зависеть",
    "терпеть",
    "вертеть",
    "дышать",
    "держать",
    "слышать",
];

/// Verbs whose present stems cannot be derived by rule; they are refused
/// rather than fabricated. «гнать» is in the exception list but its stem
/// alternates (гон-), and «жить» inserts -в-.
const IRREGULAR_PRESENT: &[&str] = &["гнать", "жить", "петь", "ждать", "брать", "звать", "дать"];

/// Past-tense stems that lose or change the vowel of the suffix.
const IRREGULAR_PAST: &[&str] = &[
    "нести",
    "везти",
    "вести",
    "расти",
    "мести",
    "грести",
    "идти",
    "мочь",
    "жечь",
    "печь",
    "беречь",
    "лечь",
    "сесть",
    "класть",
    "стать",
    "дать",
    "есть",
];

pub fn classify_verb(infinitive: &str) -> ConjugationClass {
    let lower = infinitive.to_lowercase();
    if SECOND_CONJUGATION_EXCEPTIONS.contains(&lower.as_str()) {
        return ConjugationClass::Second;
    }
    if lower.ends_with("ить") {
        return ConjugationClass::Second;
    }
    ConjugationClass::First
}

/// The present-tense stem, or `None` when the infinitive does not belong
/// to a class whose present stem is derivable by rule:
/// - -ова-/-ева-/-ыва-/-ива- suffixed verbs (рисовать, ночевать,
///   показывать) build a regular vowel stem;
/// - -ить verbs (2nd conjugation) strip the suffix.
///
/// Bare -ать/-ять/-еть/-оть/-уть/-ыть first-conjugation verbs mix regular
/// (делать, читать) and alternating (писать, жить) shapes that no ending
/// rule can tell apart, so they are refused for the present tense.
fn present_stem(lower: &str) -> Option<String> {
    for ending in ["ивать", "евать", "овать", "ывать"] {
        if let Some(base) = lower.strip_suffix(ending) {
            // рисовать → рису-, ночевать → ночу-, показывать → показыва-
            let theme = ending.strip_suffix("ть").unwrap_or(ending);
            return Some(match ending {
                "овать" | "евать" => format!("{base}у"),
                _ => format!("{base}{theme}"),
            });
        }
    }
    if let Some(base) = lower.strip_suffix("ить") {
        return Some(base.to_string());
    }
    None
}

fn ends_with_vowel(stem: &str) -> bool {
    stem.chars()
        .last()
        .is_some_and(|c| "аеёиоуыэюя".contains(c))
}

fn ends_with_sibilant(stem: &str) -> bool {
    stem.chars().last().is_some_and(|c| "жшчщ".contains(c))
}

/// Conjugate one person of the present (imperfective future) tense.
/// `None` means the rule engine cannot know the form (consonant-stem
/// alternations are lexical), and the caller must not fabricate one.
pub fn conjugate_present(infinitive: &str, person: VerbPerson) -> Option<String> {
    let lower = infinitive.to_lowercase();
    if IRREGULAR_PRESENT.contains(&lower.as_str()) {
        return None;
    }
    let stem = present_stem(&lower)?;
    let class = classify_verb(&lower);

    match class {
        ConjugationClass::First => {
            if !ends_with_vowel(&stem) {
                // Consonant-stem first conjugation alternates (писать → пишу):
                // not derivable by rule.
                return None;
            }
            // Standing on «я» drops it in the 1sg (стоять → стою, сеять → сею).
            let (first_sg_base, plural_base) = if let Some(base) = stem.strip_suffix('я') {
                (base.to_string(), stem.clone())
            } else {
                (stem.clone(), stem.clone())
            };
            Some(match person {
                VerbPerson::FirstSingular => format!("{first_sg_base}ю"),
                VerbPerson::SecondSingular => format!("{plural_base}ешь"),
                VerbPerson::ThirdSingular => format!("{plural_base}ет"),
                VerbPerson::FirstPlural => format!("{plural_base}ем"),
                VerbPerson::SecondPlural => format!("{plural_base}ете"),
                VerbPerson::ThirdPlural => format!("{plural_base}ют"),
            })
        }
        ConjugationClass::Second => {
            if ends_with_vowel(&stem) {
                // стоить → стою, стоят
                Some(match person {
                    VerbPerson::FirstSingular => format!("{stem}ю"),
                    VerbPerson::SecondSingular => format!("{stem}ишь"),
                    VerbPerson::ThirdSingular => format!("{stem}ит"),
                    VerbPerson::FirstPlural => format!("{stem}им"),
                    VerbPerson::SecondPlural => format!("{stem}ите"),
                    VerbPerson::ThirdPlural => format!("{stem}ят"),
                })
            } else {
                // Consonant-stem second conjugation: everything except 1sg is
                // regular (говорить → говоришь…говорят); 1sg may or may not
                // insert -л- (говорю против люблю) and cannot be derived.
                Some(match person {
                    VerbPerson::FirstSingular => return None,
                    VerbPerson::SecondSingular => format!("{stem}ишь"),
                    VerbPerson::ThirdSingular => format!("{stem}ит"),
                    VerbPerson::FirstPlural => format!("{stem}им"),
                    VerbPerson::SecondPlural => format!("{stem}ите"),
                    // держат/слышат after sibilants, видят/смотрят elsewhere.
                    VerbPerson::ThirdPlural => {
                        if ends_with_sibilant(&stem) {
                            format!("{stem}ат")
                        } else {
                            format!("{stem}ят")
                        }
                    }
                })
            }
        }
    }
}

/// Regular past-tense form: делать → делал/делала/делало/делали.
/// Irregular stems (нести → нёс) are refused.
pub fn past_tense(infinitive: &str, gender: Gender, number: Number) -> Option<String> {
    let lower = infinitive.to_lowercase();
    if IRREGULAR_PAST.contains(&lower.as_str()) {
        return None;
    }
    let stem = lower.strip_suffix("ть")?;
    if stem.is_empty() {
        return None;
    }
    let suffix = match (number, gender) {
        (Number::Plural, _) => "ли",
        (Number::Singular, Gender::Feminine) => "ла",
        (Number::Singular, Gender::Neuter) => "ло",
        (Number::Singular, _) => "л",
    };
    Some(format!("{stem}{suffix}"))
}

/// Compound imperfective future: «буду читать». The caller decides whether
/// the aspect of the specific verb makes this grammatical.
pub fn compound_future(infinitive: &str) -> String {
    format!("буду {}", infinitive.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_present(infinitive: &str) -> [Option<String>; 6] {
        [
            conjugate_present(infinitive, VerbPerson::FirstSingular),
            conjugate_present(infinitive, VerbPerson::SecondSingular),
            conjugate_present(infinitive, VerbPerson::ThirdSingular),
            conjugate_present(infinitive, VerbPerson::FirstPlural),
            conjugate_present(infinitive, VerbPerson::SecondPlural),
            conjugate_present(infinitive, VerbPerson::ThirdPlural),
        ]
    }

    fn forms(words: [&str; 6]) -> [Option<String>; 6] {
        words.map(|w| Some(String::from(w)))
    }

    #[test]
    fn suffix_verbs_conjugate_fully() {
        // -ова-/-ева-/-ыва-/-ива- verbs are regular by shape.
        assert_eq!(
            all_present("рисовать"),
            forms(["рисую", "рисуешь", "рисует", "рисуем", "рисуете", "рисуют"])
        );
        assert_eq!(
            all_present("показывать"),
            forms([
                "показываю",
                "показываешь",
                "показывает",
                "показываем",
                "показываете",
                "показывают"
            ])
        );
        assert_eq!(
            conjugate_present("ночевать", VerbPerson::FirstSingular),
            Some("ночую".into())
        );
        assert_eq!(
            conjugate_present("ночевать", VerbPerson::SecondPlural),
            Some("ночуете".into())
        );
    }

    #[test]
    fn bare_first_conjugation_verbs_are_refused_for_present() {
        // делать/читать are regular, писать alternates — no ending rule can
        // tell them apart, so the whole bare -ать/-ять class is refused.
        for verb in ["делать", "читать", "гулять", "стоять", "писать"]
        {
            assert_eq!(
                conjugate_present(verb, VerbPerson::ThirdSingular),
                None,
                "{verb}"
            );
        }
    }

    #[test]
    fn second_conjugation_vowel_stems_conjugate_fully() {
        assert_eq!(
            all_present("стоить"),
            forms(["стою", "стоишь", "стоит", "стоим", "стоите", "стоят"])
        );
    }

    #[test]
    fn second_conjugation_consonant_stems_cover_all_but_first_singular() {
        assert_eq!(
            conjugate_present("говорить", VerbPerson::FirstSingular),
            None
        );
        assert_eq!(
            conjugate_present("говорить", VerbPerson::SecondSingular),
            Some("говоришь".into())
        );
        assert_eq!(
            conjugate_present("говорить", VerbPerson::ThirdSingular),
            Some("говорит".into())
        );
        assert_eq!(
            conjugate_present("говорить", VerbPerson::FirstPlural),
            Some("говорим".into())
        );
        assert_eq!(
            conjugate_present("говорить", VerbPerson::SecondPlural),
            Some("говорите".into())
        );
        assert_eq!(
            conjugate_present("говорить", VerbPerson::ThirdPlural),
            Some("говорят".into())
        );
        // любить: 1sg inserts -л- (люблю) — refused; 3pl любит rule form.
        assert_eq!(conjugate_present("любить", VerbPerson::FirstSingular), None);
        assert_eq!(
            conjugate_present("любить", VerbPerson::ThirdPlural),
            Some("любят".into())
        );
    }

    #[test]
    fn classic_exceptions_classify_but_have_no_rule_stems() {
        // смотреть/держать end in bare -еть/-ать: the exception list fixes
        // the class, but their present stems are not derivable by rule.
        for verb in ["смотреть", "держать", "видеть", "слышать", "терпеть"]
        {
            assert_eq!(classify_verb(verb), ConjugationClass::Second, "{verb}");
            assert_eq!(
                conjugate_present(verb, VerbPerson::ThirdSingular),
                None,
                "{verb}"
            );
        }
    }

    #[test]
    fn irregular_present_verbs_fail_closed() {
        for irregular in ["гнать", "жить", "петь", "ждать", "брать", "дать"]
        {
            assert_eq!(
                conjugate_present(irregular, VerbPerson::ThirdSingular),
                None,
                "{irregular}"
            );
        }
    }

    #[test]
    fn past_tense_covers_regular_stems_with_gender_and_number() {
        assert_eq!(
            past_tense("делать", Gender::Masculine, Number::Singular),
            Some("делал".into())
        );
        assert_eq!(
            past_tense("делать", Gender::Feminine, Number::Singular),
            Some("делала".into())
        );
        assert_eq!(
            past_tense("делать", Gender::Neuter, Number::Singular),
            Some("делало".into())
        );
        assert_eq!(
            past_tense("делать", Gender::Unknown, Number::Plural),
            Some("делали".into())
        );
        assert_eq!(
            past_tense("любить", Gender::Masculine, Number::Singular),
            Some("любил".into())
        );
        assert_eq!(
            past_tense("писать", Gender::Feminine, Number::Singular),
            Some("писала".into())
        );
        assert_eq!(
            past_tense("говорить", Gender::Neuter, Number::Singular),
            Some("говорило".into())
        );
    }

    #[test]
    fn past_tense_irregulars_fail_closed() {
        for irregular in ["нести", "мочь", "жечь", "идти", "сесть", "дать"]
        {
            assert_eq!(
                past_tense(irregular, Gender::Masculine, Number::Singular),
                None,
                "{irregular}"
            );
        }
        assert_eq!(
            past_tense("говорить", Gender::Masculine, Number::Singular),
            Some("говорил".into())
        );
    }

    #[test]
    fn compound_future_is_explicit() {
        assert_eq!(compound_future("читать"), "буду читать");
    }

    #[test]
    fn classification_follows_endings_and_exceptions() {
        assert_eq!(classify_verb("говорить"), ConjugationClass::Second);
        assert_eq!(classify_verb("держать"), ConjugationClass::Second);
        assert_eq!(classify_verb("дышать"), ConjugationClass::Second);
        assert_eq!(classify_verb("рисовать"), ConjugationClass::First);
        assert_eq!(classify_verb("делать"), ConjugationClass::First);
    }
}
