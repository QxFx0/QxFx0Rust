//! Preposition → case government for Russian.
//!
//! Ported from the Haskell analyzer (QxFx0/Semantic/Morphology.hs,
//! `defaultPrepositionCaseMap`) with the standard vowel-variant prepositions
//! (во, ко, со, обо) added. Ambiguous governors list both readings; the
//! caller disambiguates by context (motion vs location for в/на, source vs
//! accompaniment for с).

use qxfx0_types::morphology::Case;

/// Cases a preposition can govern. Empty slice is never returned; unknown
/// prepositions yield `None`.
pub fn governing_cases(preposition: &str) -> Option<&'static [Case]> {
    let cases = match preposition {
        "в" | "во" => &[Case::Accusative, Case::Prepositional][..],
        "на" => &[Case::Accusative, Case::Prepositional],
        "к" | "ко" => &[Case::Dative],
        "по" => &[Case::Dative],
        "от" | "до" | "из" | "у" => &[Case::Genitive],
        "с" | "со" => &[Case::Genitive, Case::Instrumental],
        "о" | "об" | "обо" => &[Case::Prepositional],
        "про" | "через" | "сквозь" => &[Case::Accusative],
        "вдоль" | "вокруг" | "после" | "для" | "без" | "кроме" => {
            &[Case::Genitive]
        }
        "перед" | "над" | "между" | "под" => &[Case::Instrumental],
        "за" => &[Case::Accusative, Case::Instrumental],
        "при" => &[Case::Prepositional],
        _ => return None,
    };
    Some(cases)
}

/// The single unambiguous governing case, when there is exactly one.
pub fn governing_case(preposition: &str) -> Option<Case> {
    match governing_cases(preposition)? {
        [single] => Some(*single),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unambiguous_governors() {
        assert_eq!(governing_case("к"), Some(Case::Dative));
        assert_eq!(governing_case("ко"), Some(Case::Dative));
        assert_eq!(governing_case("по"), Some(Case::Dative));
        assert_eq!(governing_case("от"), Some(Case::Genitive));
        assert_eq!(governing_case("из"), Some(Case::Genitive));
        assert_eq!(governing_case("у"), Some(Case::Genitive));
        assert_eq!(governing_case("о"), Some(Case::Prepositional));
        assert_eq!(governing_case("об"), Some(Case::Prepositional));
        assert_eq!(governing_case("обо"), Some(Case::Prepositional));
        assert_eq!(governing_case("про"), Some(Case::Accusative));
        assert_eq!(governing_case("через"), Some(Case::Accusative));
        assert_eq!(governing_case("над"), Some(Case::Instrumental));
        assert_eq!(governing_case("между"), Some(Case::Instrumental));
        assert_eq!(governing_case("перед"), Some(Case::Instrumental));
    }

    #[test]
    fn ambiguous_governors_list_both_readings() {
        assert_eq!(
            governing_cases("в"),
            Some(&[Case::Accusative, Case::Prepositional][..])
        );
        assert_eq!(
            governing_cases("на"),
            Some(&[Case::Accusative, Case::Prepositional][..])
        );
        assert_eq!(
            governing_case("в"),
            None,
            "в is ambiguous: motion vs location"
        );
        assert_eq!(governing_case("с"), None, "с is ambiguous: from vs with");
        assert_eq!(governing_case("за"), None);
        assert_eq!(
            governing_cases("с"),
            Some(&[Case::Genitive, Case::Instrumental][..])
        );
    }

    #[test]
    fn unknown_prepositions_fail_closed() {
        assert_eq!(governing_cases("мимо"), None);
        assert_eq!(governing_cases("не-предлог"), None);
        assert_eq!(governing_case(""), None);
    }

    #[test]
    fn every_mapped_preposition_has_at_least_one_case() {
        for preposition in [
            "в",
            "во",
            "на",
            "к",
            "ко",
            "по",
            "от",
            "до",
            "из",
            "у",
            "с",
            "со",
            "о",
            "об",
            "обо",
            "про",
            "через",
            "сквозь",
            "вдоль",
            "вокруг",
            "после",
            "для",
            "без",
            "кроме",
            "перед",
            "над",
            "между",
            "под",
            "за",
            "при",
        ] {
            assert!(
                governing_cases(preposition).is_some_and(|cases| !cases.is_empty()),
                "{preposition} must govern at least one case"
            );
        }
    }
}
