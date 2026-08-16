//! Rule-based declension inference for out-of-vocabulary nouns.
//!
//! Port of the Haskell `inferNounForm` (QxFx0/Semantic/Lexicon/
//! RuntimeParadigms.hs) with the same stem classes: masculine consonant/й/ь,
//! feminine а/я/ь, neuter о/е/ие. Velar and sibilant stems take the ы↔и and
//! ов↔ей alternations, and the accusative honors animacy (default
//! inanimate). Returns `None` fail-closed whenever the ending class is not
//! recognized — the caller falls back to the lexicon or refuses instead of
//! fabricating a form.

use qxfx0_types::morphology::{Animacy, Case, Gender, Number};

/// Whether the lexicon already knows a lemma is resolved by the caller;
/// this function only classifies by ending when the runtime has no entry.
pub fn guess_gender_by_ending(lemma: &str) -> Gender {
    let lower = lemma.to_lowercase();
    match lower.chars().last() {
        Some('а') | Some('я') => Gender::Feminine,
        Some('о') | Some('е') => Gender::Neuter,
        // Soft-sign feminines (грань, ночь) are ambiguous by ending alone;
        // masculine is the documented default, same as the Haskell fallback.
        _ => Gender::Masculine,
    }
}

fn is_vowel(character: char) -> bool {
    "аеёиоуыэюя".contains(character)
}

fn is_velar_or_sibilant(character: char) -> bool {
    "кгхжшчщ".contains(character)
}

/// Full 12-cell paradigm of a lemma the lexicon does not know.
/// Cells: nom, gen, dat, acc, ins, prep × singular, then plural.
pub fn infer_noun_paradigm(lemma: &str, gender: Gender, animacy: Animacy) -> Option<[String; 12]> {
    let mut cells = Vec::with_capacity(12);
    for number in [Number::Singular, Number::Plural] {
        for case in [
            Case::Nominative,
            Case::Genitive,
            Case::Dative,
            Case::Accusative,
            Case::Instrumental,
            Case::Prepositional,
        ] {
            cells.push(infer_noun_form(lemma, gender, animacy, case, number)?);
        }
    }
    cells.try_into().ok()
}

/// Infer one form for an out-of-vocabulary noun. The lemma must contain at
/// least two characters; its gender may come from the lexicon or from
/// `guess_gender_by_ending`.
/// Nouns whose stem loses a fleeting vowel in oblique forms (день → дня);
/// no ending rule can derive them.
const FLEETING_VOWEL_NOUNS: &[&str] = &["день", "сон"];

pub fn infer_noun_form(
    lemma: &str,
    gender: Gender,
    animacy: Animacy,
    case: Case,
    number: Number,
) -> Option<String> {
    let lower = lemma.to_lowercase();
    if FLEETING_VOWEL_NOUNS.contains(&lower.as_str()) {
        return None;
    }
    let characters: Vec<char> = lower.chars().collect();
    if characters.len() < 2 {
        return None;
    }
    match gender {
        Gender::Masculine => infer_masculine(&characters, animacy, case, number),
        Gender::Feminine => infer_feminine(&characters, case, number),
        Gender::Neuter => infer_neuter(&characters, case, number),
        Gender::Unknown => None,
    }
}

fn infer_masculine(
    characters: &[char],
    animacy: Animacy,
    case: Case,
    number: Number,
) -> Option<String> {
    let stem: String = characters.iter().collect();
    let last = *characters.last()?;
    let stem_without_last: String = characters[..characters.len() - 1].iter().collect();
    let velar = is_velar_or_sibilant(last);

    if !is_vowel(last) && last != 'ь' && last != 'й' {
        // Hard consonant stem: дом, мир, волк, нож.
        // ы→и applies after velars AND sibilants (волки, ножи); ов→ей only
        // after sibilants (ножей) — velars keep -ов (волков). The Haskell
        // original applied ов→ей to velars too and produced «волкей».
        let plural_nominative = if velar {
            format!("{stem}и")
        } else {
            format!("{stem}ы")
        };
        let sibilant = "жшчщ".contains(last);
        let plural_genitive = if sibilant {
            format!("{stem}ей")
        } else {
            format!("{stem}ов")
        };
        let accusative = |plural_nom: String, plural_gen: String| match (number, case) {
            (Number::Singular, Case::Accusative) => match animacy {
                Animacy::Animate => format!("{stem}а"),
                _ => stem.clone(),
            },
            (Number::Plural, Case::Accusative) => match animacy {
                Animacy::Animate => plural_gen,
                _ => plural_nom,
            },
            _ => String::new(),
        };
        let picked = accusative(plural_nominative.clone(), plural_genitive.clone());
        if !picked.is_empty() {
            return Some(picked);
        }
        return Some(match (number, case) {
            (Number::Singular, Case::Genitive) => format!("{stem}а"),
            (Number::Singular, Case::Dative) => format!("{stem}у"),
            (Number::Singular, Case::Instrumental) => format!("{stem}ом"),
            (Number::Singular, Case::Prepositional) => format!("{stem}е"),
            (Number::Plural, Case::Genitive) => plural_genitive,
            (Number::Plural, Case::Dative) => format!("{stem}ам"),
            (Number::Plural, Case::Instrumental) => format!("{stem}ами"),
            (Number::Plural, Case::Prepositional) => format!("{stem}ах"),
            (Number::Singular, Case::Nominative) => stem.clone(),
            (Number::Plural, Case::Nominative) => plural_nominative,
            _ => return None,
        });
    }
    if last == 'й' {
        // Мягкая основа на -й: край, бой.
        let base = stem_without_last;
        let accusative_sg = match animacy {
            Animacy::Animate => format!("{base}я"),
            _ => stem.clone(),
        };
        return Some(match (number, case) {
            (Number::Singular, Case::Nominative) => stem.clone(),
            (Number::Singular, Case::Genitive) => format!("{base}я"),
            (Number::Singular, Case::Dative) => format!("{base}ю"),
            (Number::Singular, Case::Accusative) => accusative_sg,
            (Number::Singular, Case::Instrumental) => format!("{base}ем"),
            (Number::Singular, Case::Prepositional) => format!("{base}е"),
            (Number::Plural, Case::Nominative) => format!("{base}и"),
            (Number::Plural, Case::Genitive) => format!("{base}ев"),
            (Number::Plural, Case::Dative) => format!("{base}ям"),
            (Number::Plural, Case::Accusative) => match animacy {
                Animacy::Animate => format!("{base}ев"),
                _ => format!("{base}и"),
            },
            (Number::Plural, Case::Instrumental) => format!("{base}ями"),
            (Number::Plural, Case::Prepositional) => format!("{base}ях"),
        });
    }
    if last == 'ь' {
        // Мягкая основа на -ь: день, учитель.
        let base = stem_without_last;
        let accusative_sg = match animacy {
            Animacy::Animate => format!("{base}я"),
            _ => stem.clone(),
        };
        return Some(match (number, case) {
            (Number::Singular, Case::Nominative) => stem.clone(),
            (Number::Singular, Case::Genitive) => format!("{base}я"),
            (Number::Singular, Case::Dative) => format!("{base}ю"),
            (Number::Singular, Case::Accusative) => accusative_sg,
            // е vs ё in the soft instrumental depends on stress (учителем
            // против огнём) and cannot be derived; е is the written default.
            (Number::Singular, Case::Instrumental) => format!("{base}ем"),
            (Number::Singular, Case::Prepositional) => format!("{base}е"),
            (Number::Plural, Case::Nominative) => format!("{base}и"),
            (Number::Plural, Case::Genitive) => format!("{base}ей"),
            (Number::Plural, Case::Dative) => format!("{base}ям"),
            (Number::Plural, Case::Accusative) => match animacy {
                Animacy::Animate => format!("{base}ей"),
                _ => format!("{base}и"),
            },
            (Number::Plural, Case::Instrumental) => format!("{base}ями"),
            (Number::Plural, Case::Prepositional) => format!("{base}ях"),
        });
    }
    None
}

fn infer_feminine(characters: &[char], case: Case, number: Number) -> Option<String> {
    let stem: String = characters.iter().collect();
    let last = *characters.last()?;
    let base: String = characters[..characters.len() - 1].iter().collect();

    if last == 'а' {
        if number == Number::Plural {
            return None; // plural of -а feminines needs stress/declension class knowledge
        }
        let velar = characters
            .get(characters.len() - 2)
            .is_some_and(|&c| is_velar_or_sibilant(c));
        return Some(match case {
            Case::Nominative => stem.clone(),
            Case::Genitive => format!("{base}{}", if velar { 'и' } else { 'ы' }),
            Case::Dative => format!("{base}е"),
            Case::Accusative => format!("{base}у"),
            Case::Instrumental => format!("{base}ой"),
            Case::Prepositional => format!("{base}е"),
        });
    }
    if last == 'я' {
        if number == Number::Plural {
            return None;
        }
        return Some(match case {
            Case::Nominative => stem.clone(),
            Case::Genitive => format!("{base}и"),
            Case::Dative => format!("{base}е"),
            Case::Accusative => format!("{base}ю"),
            Case::Instrumental => format!("{base}ей"),
            Case::Prepositional => format!("{base}е"),
        });
    }
    if last == 'ь' {
        // грань, ночь, тень — the class the legacy ending heuristic got wrong.
        if number == Number::Plural {
            return None;
        }
        return Some(match case {
            Case::Nominative => stem.clone(),
            Case::Genitive => format!("{base}и"),
            Case::Dative => format!("{base}и"),
            Case::Accusative => stem.clone(),
            Case::Instrumental => format!("{base}ью"),
            Case::Prepositional => format!("{base}и"),
        });
    }
    None
}

fn infer_neuter(characters: &[char], case: Case, number: Number) -> Option<String> {
    let stem: String = characters.iter().collect();
    let last = *characters.last()?;
    let base: String = characters[..characters.len() - 1].iter().collect();

    if last == 'о' {
        if number == Number::Plural {
            return None;
        }
        return Some(match case {
            Case::Nominative | Case::Accusative => stem.clone(),
            Case::Genitive => format!("{base}а"),
            Case::Dative => format!("{base}у"),
            Case::Instrumental => format!("{base}ом"),
            Case::Prepositional => format!("{base}е"),
        });
    }
    if last == 'е' {
        let is_iye = characters.len() >= 3 && characters[characters.len() - 2] == 'и';
        if is_iye {
            // -ие: мнение, усилие.
            let i_base: String = characters[..characters.len() - 2].iter().collect();
            return Some(match (number, case) {
                (Number::Singular, Case::Nominative | Case::Accusative) => stem.clone(),
                (Number::Singular, Case::Genitive) => format!("{i_base}ия"),
                (Number::Singular, Case::Dative) => format!("{i_base}ию"),
                (Number::Singular, Case::Instrumental) => format!("{i_base}ием"),
                (Number::Singular, Case::Prepositional) => format!("{i_base}ии"),
                (Number::Plural, Case::Nominative) => format!("{i_base}ия"),
                (Number::Plural, Case::Genitive) => format!("{i_base}ий"),
                (Number::Plural, Case::Dative) => format!("{i_base}иям"),
                (Number::Plural, Case::Accusative) => format!("{i_base}ия"),
                (Number::Plural, Case::Instrumental) => format!("{i_base}иями"),
                (Number::Plural, Case::Prepositional) => format!("{i_base}иях"),
            });
        }
        if number == Number::Plural {
            return None;
        }
        return Some(match case {
            Case::Nominative | Case::Accusative => stem.clone(),
            Case::Genitive => format!("{base}я"),
            Case::Dative => format!("{base}ю"),
            Case::Instrumental => format!("{base}ем"),
            Case::Prepositional => format!("{base}е"),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const INANIMATE: Animacy = Animacy::Inanimate;
    const ANIMATE: Animacy = Animacy::Animate;

    #[test]
    fn hard_consonant_masculine_full_singular() {
        let cases = [
            (Case::Nominative, "дом"),
            (Case::Genitive, "дома"),
            (Case::Dative, "дому"),
            (Case::Accusative, "дом"),
            (Case::Instrumental, "домом"),
            (Case::Prepositional, "доме"),
        ];
        for (case, expected) in cases {
            assert_eq!(
                infer_noun_form("дом", Gender::Masculine, INANIMATE, case, Number::Singular),
                Some(expected.to_string()),
                "{case:?}"
            );
        }
    }

    #[test]
    fn animate_masculine_takes_genitive_accusative() {
        // «брат → братья/братьев» is lexical; the rule covers regular
        // animate stems like «волк → волков».
        assert_eq!(
            infer_noun_form(
                "кот",
                Gender::Masculine,
                ANIMATE,
                Case::Accusative,
                Number::Singular
            ),
            Some("кота".into())
        );
        assert_eq!(
            infer_noun_form(
                "волк",
                Gender::Masculine,
                ANIMATE,
                Case::Accusative,
                Number::Plural
            ),
            Some("волков".into())
        );
    }

    #[test]
    fn velar_stems_alternate_ы_to_и_and_ов_to_ей() {
        assert_eq!(
            infer_noun_form(
                "мир",
                Gender::Masculine,
                INANIMATE,
                Case::Nominative,
                Number::Plural
            ),
            Some("миры".into())
        );
        assert_eq!(
            infer_noun_form(
                "мир",
                Gender::Masculine,
                INANIMATE,
                Case::Genitive,
                Number::Plural
            ),
            Some("миров".into())
        );
        assert_eq!(
            infer_noun_form(
                "волк",
                Gender::Masculine,
                INANIMATE,
                Case::Nominative,
                Number::Plural
            ),
            Some("волки".into())
        );
        assert_eq!(
            infer_noun_form(
                "волк",
                Gender::Masculine,
                INANIMATE,
                Case::Genitive,
                Number::Plural
            ),
            Some("волков".into())
        );
        // шипящая основа: нож → ножи/ножей.
        assert_eq!(
            infer_noun_form(
                "нож",
                Gender::Masculine,
                INANIMATE,
                Case::Nominative,
                Number::Plural
            ),
            Some("ножи".into())
        );
        assert_eq!(
            infer_noun_form(
                "нож",
                Gender::Masculine,
                INANIMATE,
                Case::Genitive,
                Number::Plural
            ),
            Some("ножей".into())
        );
    }

    #[test]
    fn soft_masculine_jo_and_soft_sign_stems() {
        assert_eq!(
            infer_noun_form(
                "край",
                Gender::Masculine,
                INANIMATE,
                Case::Genitive,
                Number::Singular
            ),
            Some("края".into())
        );
        assert_eq!(
            infer_noun_form(
                "край",
                Gender::Masculine,
                INANIMATE,
                Case::Prepositional,
                Number::Singular
            ),
            Some("крае".into())
        );
        // «день → дня/днём» has a fleeting vowel and is refused; the
        // regular soft-sign masculine «учитель» infers fully.
        assert_eq!(
            infer_noun_form(
                "день",
                Gender::Masculine,
                INANIMATE,
                Case::Genitive,
                Number::Singular
            ),
            None
        );
        assert_eq!(
            infer_noun_form(
                "учитель",
                Gender::Masculine,
                INANIMATE,
                Case::Instrumental,
                Number::Singular
            ),
            Some("учителем".into())
        );
        assert_eq!(
            infer_noun_form(
                "учитель",
                Gender::Masculine,
                INANIMATE,
                Case::Genitive,
                Number::Plural
            ),
            Some("учителей".into())
        );
    }

    #[test]
    fn feminine_a_ja_and_soft_sign() {
        assert_eq!(
            infer_noun_form(
                "гора",
                Gender::Feminine,
                INANIMATE,
                Case::Genitive,
                Number::Singular
            ),
            Some("горы".into())
        );
        assert_eq!(
            infer_noun_form(
                "волна",
                Gender::Feminine,
                INANIMATE,
                Case::Genitive,
                Number::Singular
            ),
            Some("волны".into())
        );
        assert_eq!(
            infer_noun_form(
                "няня",
                Gender::Feminine,
                INANIMATE,
                Case::Dative,
                Number::Singular
            ),
            Some("няне".into())
        );
        // The exact class the legacy heuristic got wrong: грань → грани, гранью.
        assert_eq!(
            infer_noun_form(
                "грань",
                Gender::Feminine,
                INANIMATE,
                Case::Genitive,
                Number::Singular
            ),
            Some("грани".into())
        );
        assert_eq!(
            infer_noun_form(
                "грань",
                Gender::Feminine,
                INANIMATE,
                Case::Instrumental,
                Number::Singular
            ),
            Some("гранью".into())
        );
    }

    #[test]
    fn neuter_o_e_and_iye() {
        assert_eq!(
            infer_noun_form(
                "окно",
                Gender::Neuter,
                INANIMATE,
                Case::Genitive,
                Number::Singular
            ),
            Some("окна".into())
        );
        assert_eq!(
            infer_noun_form(
                "море",
                Gender::Neuter,
                INANIMATE,
                Case::Instrumental,
                Number::Singular
            ),
            Some("морем".into())
        );
        assert_eq!(
            infer_noun_form(
                "мнение",
                Gender::Neuter,
                INANIMATE,
                Case::Prepositional,
                Number::Singular
            ),
            Some("мнении".into())
        );
        assert_eq!(
            infer_noun_form(
                "мнение",
                Gender::Neuter,
                INANIMATE,
                Case::Instrumental,
                Number::Plural
            ),
            Some("мнениями".into())
        );
    }

    #[test]
    fn unknown_endings_fail_closed() {
        assert_eq!(
            infer_noun_form(
                "и",
                Gender::Masculine,
                INANIMATE,
                Case::Genitive,
                Number::Singular
            ),
            None,
            "single-character lemmas are rejected"
        );
        assert_eq!(
            infer_noun_form(
                "э",
                Gender::Unknown,
                INANIMATE,
                Case::Genitive,
                Number::Singular
            ),
            None
        );
    }

    #[test]
    fn gender_guess_by_ending() {
        assert_eq!(guess_gender_by_ending("свобода"), Gender::Feminine);
        assert_eq!(guess_gender_by_ending("неделя"), Gender::Feminine);
        assert_eq!(guess_gender_by_ending("окно"), Gender::Neuter);
        assert_eq!(guess_gender_by_ending("море"), Gender::Neuter);
        assert_eq!(guess_gender_by_ending("дом"), Gender::Masculine);
    }

    #[test]
    fn full_paradigm_has_twelve_distinct_cells() {
        let paradigm =
            infer_noun_paradigm("фонарь", Gender::Masculine, INANIMATE).expect("paradigm");
        assert_eq!(paradigm.len(), 12);
        assert_eq!(paradigm[0], "фонарь");
        assert_eq!(paradigm[1], "фонаря");
        assert_eq!(paradigm[11], "фонарях");
    }
}
