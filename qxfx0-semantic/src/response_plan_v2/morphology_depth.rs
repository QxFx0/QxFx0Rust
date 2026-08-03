//! Morphology-depth contracts for V2 realization (ADR-0034 §7).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::OnceLock;

use qxfx0_morphology::{Case, MorphologyLookup, MorphologyRuntime, Number};

const PREPOSITION_ALLOMORPHS_TSV: &str = include_str!("../../assets/preposition_allomorphs.tsv");

/// Strength of the generate/analyze round-trip promised for one morphology
/// triple. Non-bijective classes retain the wanted analysis instead of
/// pretending that one surface has one lemma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundTripClass {
    Bijective,
    Ambiguous,
    Suppletive,
    OrthographicVariant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MorphologyRoundTripWitness {
    pub lemma: String,
    pub surface: String,
    pub case: Case,
    pub number: Number,
    pub class: RoundTripClass,
    pub analyses_considered: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MorphologyRoundTripError {
    #[error("surface '{surface}' has no morphology analysis")]
    UnknownSurface { surface: String },
    #[error(
        "bijective round-trip for '{surface}' resolved to {actual_lemma}/{actual_case:?}/{actual_number:?}, expected {expected_lemma}/{expected_case:?}/{expected_number:?}"
    )]
    BijectiveMismatch {
        surface: String,
        expected_lemma: String,
        expected_case: Case,
        expected_number: Number,
        actual_lemma: String,
        actual_case: Case,
        actual_number: Number,
    },
    #[error(
        "analysis set for '{surface}' does not contain {lemma}/{case:?}/{number:?} required by {class:?}"
    )]
    AnalysisMissing {
        surface: String,
        lemma: String,
        case: Case,
        number: Number,
        class: RoundTripClass,
    },
}

/// Verify `lemma -> generate(features) -> analyze(surface)` under the declared
/// round-trip strength.
pub fn verify_round_trip(
    morphology: &MorphologyRuntime,
    lemma: &str,
    case: Case,
    number: Number,
    surface: &str,
    class: RoundTripClass,
) -> Result<MorphologyRoundTripWitness, MorphologyRoundTripError> {
    let candidates = morphology.get_candidates(surface);
    let analyses_considered = candidates.len();
    match (class, morphology.lemmatize(surface)) {
        (RoundTripClass::Bijective, MorphologyLookup::Resolved(resolution))
            if resolution.lemma == lemma => {}
        (RoundTripClass::Bijective, MorphologyLookup::Resolved(resolution)) => {
            return Err(MorphologyRoundTripError::BijectiveMismatch {
                surface: surface.to_string(),
                expected_lemma: lemma.to_string(),
                expected_case: case,
                expected_number: number,
                actual_lemma: resolution.lemma,
                actual_case: resolution.case,
                actual_number: resolution.number,
            });
        }
        (RoundTripClass::Bijective, MorphologyLookup::Unknown) => {
            return Err(MorphologyRoundTripError::UnknownSurface {
                surface: surface.to_string(),
            });
        }
        (RoundTripClass::Bijective, MorphologyLookup::Ambiguous(_)) => {
            return Err(MorphologyRoundTripError::AnalysisMissing {
                surface: surface.to_string(),
                lemma: lemma.to_string(),
                case,
                number,
                class,
            });
        }
        (_, MorphologyLookup::Unknown) => {
            return Err(MorphologyRoundTripError::UnknownSurface {
                surface: surface.to_string(),
            });
        }
        (_, MorphologyLookup::Resolved(_) | MorphologyLookup::Ambiguous(_)) => {
            if !candidates.iter().any(|candidate| {
                candidate.entry.lemma == lemma
                    && candidate.case_number.case == case
                    && candidate.case_number.number == number
            }) {
                return Err(MorphologyRoundTripError::AnalysisMissing {
                    surface: surface.to_string(),
                    lemma: lemma.to_string(),
                    case,
                    number,
                    class,
                });
            }
        }
    }

    Ok(MorphologyRoundTripWitness {
        lemma: lemma.to_string(),
        surface: surface.to_string(),
        case,
        number,
        class,
        analyses_considered,
    })
}

/// Fingerprinted lexical choices for prepositions whose long form cannot be
/// selected safely by a phonological heuristic.
#[derive(Debug, Clone)]
pub struct PrepositionAllomorphLexicon {
    entries: BTreeMap<(String, String), String>,
    fingerprint: String,
}

impl PrepositionAllomorphLexicon {
    fn load(source: &str) -> Self {
        let entries = source
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .skip(1)
            .map(|line| {
                let mut columns = line.split('\t');
                let base = columns.next().expect("allomorph base");
                let lemma = columns.next().expect("allomorph lemma");
                let surface = columns.next().expect("allomorph surface");
                assert!(columns.next().is_none(), "allomorph row has three columns");
                ((base.to_string(), lemma.to_string()), surface.to_string())
            })
            .collect();
        let mut hasher = Sha256::new();
        hasher.update(b"qxfx0:preposition-allomorphs:v1");
        hasher.update(source.as_bytes());
        Self {
            entries,
            fingerprint: format!("{:x}", hasher.finalize()),
        }
    }

    pub fn realize<'a>(
        &'a self,
        base: &'a str,
        complement_lemma: Option<&str>,
        _complement_surface: &str,
    ) -> &'a str {
        complement_lemma
            .and_then(|lemma| self.entries.get(&(base.to_string(), lemma.to_string())))
            .map(String::as_str)
            .unwrap_or(base)
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

pub fn preposition_allomorphs() -> &'static PrepositionAllomorphLexicon {
    static LEXICON: OnceLock<PrepositionAllomorphLexicon> = OnceLock::new();
    LEXICON.get_or_init(|| PrepositionAllomorphLexicon::load(PREPOSITION_ALLOMORPHS_TSV))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bijective_round_trip_requires_the_generated_lemma() {
        let morphology = qxfx0_morphology::get_runtime();
        assert!(verify_round_trip(
            morphology,
            "свобода",
            Case::Accusative,
            Number::Singular,
            "свободу",
            RoundTripClass::Bijective,
        )
        .is_ok());
        assert!(verify_round_trip(
            morphology,
            "истина",
            Case::Genitive,
            Number::Singular,
            "свободу",
            RoundTripClass::Bijective,
        )
        .is_err());
    }

    #[test]
    fn weaker_classes_keep_the_wanted_analysis() {
        let morphology = qxfx0_morphology::get_runtime();
        let witness = verify_round_trip(
            morphology,
            "время",
            Case::Genitive,
            Number::Singular,
            "времени",
            RoundTripClass::Ambiguous,
        )
        .expect("wanted analysis is present");
        assert!(witness.analyses_considered >= 1);
    }

    #[test]
    fn s_allomorph_is_lexical_not_a_prefix_heuristic() {
        let lexicon = preposition_allomorphs();
        assert_eq!(lexicon.realize("с", Some("время"), "временем"), "со");
        assert_eq!(lexicon.realize("с", Some("воля"), "волей"), "с");
        assert_eq!(lexicon.realize("с", Some("сознание"), "сознанием"), "с");
    }
}
