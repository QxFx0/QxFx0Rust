//! Valency frames: which case a relation's head governs (ADR-0034 §7).
//!
//! The separation this module exists for: **the plan carries which relation is
//! said, the lexicon carries which government, and the linearizer computes the
//! forms.** Before it, government lived inside template strings — `{OBJ|acc}`
//! was written out per template, 127 times, and two templates for the same
//! relation could disagree about the case without anything noticing.
//!
//! It also moves subject agreement out of the surface. Three admitted
//! relations — `napravlena`, `svyazan`, `neobratimo` — are short forms whose
//! *gender is baked into the identifier*, so the same relation could not be
//! reused for a subject of another gender. The lemma and its four forms live
//! here instead.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::OnceLock;

use qxfx0_morphology::{Case, Gender, Number};

const VALENCY_FRAMES_TSV: &str = include_str!("../../assets/valency_frames.tsv");

/// The agreement features a head needs from its subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgreementFeatures {
    pub gender: Gender,
    pub number: Number,
}

impl AgreementFeatures {
    pub const fn new(gender: Gender, number: Number) -> Self {
        Self { gender, number }
    }
}

/// What the head requires of its complement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Complement {
    /// No complement at all: `время необратимо`.
    None,
    /// Bare case government: `требует` + genitive.
    Direct(Case),
    /// Preposition plus case: `зависит от` + genitive.
    Prepositional { preposition: String, case: Case },
    /// An infinitive phrase or predicate nominal, carried verbatim. It is not
    /// case-governed, so no case can be demanded of it.
    Uninflected,
}

impl Complement {
    /// The case the complement must appear in, when one is demanded.
    pub fn required_case(&self) -> Option<Case> {
        match self {
            Self::None | Self::Uninflected => None,
            Self::Direct(case) => Some(*case),
            Self::Prepositional { case, .. } => Some(*case),
        }
    }

    pub fn preposition(&self) -> Option<&str> {
        match self {
            Self::Prepositional { preposition, .. } => Some(preposition),
            _ => None,
        }
    }
}

/// How the head realizes itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadKind {
    /// A finite verb. Russian present tense agrees in person and number but
    /// not in gender, so one surface serves every subject.
    Finite { surface: String },
    /// A short participle or adjective, which agrees with the subject in
    /// gender and number. Order is fixed: masculine, feminine, neuter, plural.
    Agreeing {
        masculine: String,
        feminine: String,
        neuter: String,
        plural: String,
    },
}

impl HeadKind {
    /// Realize the head for a subject's features.
    ///
    /// A finite head ignores them, which is why passing the wrong gender to a
    /// finite verb is harmless while passing it to a short form is the
    /// `разум направлена` defect.
    pub fn realize(&self, features: AgreementFeatures) -> &str {
        match self {
            Self::Finite { surface } => surface,
            Self::Agreeing {
                masculine,
                feminine,
                neuter,
                plural,
            } => match (features.number, features.gender) {
                (Number::Plural, _) => plural,
                (Number::Singular, Gender::Feminine) => feminine,
                (Number::Singular, Gender::Neuter) => neuter,
                // An unknown gender falls back to masculine only after the
                // morphology bundle has already been consulted and had no
                // answer; it is never a silent default for a known lemma.
                (Number::Singular, Gender::Masculine | Gender::Unknown) => masculine,
            },
        }
    }

    pub const fn agrees_with_subject(&self) -> bool {
        matches!(self, Self::Agreeing { .. })
    }
}

/// One relation's government and realization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValencyFrame {
    relation_id: String,
    head: HeadKind,
    complement: Complement,
}

impl ValencyFrame {
    pub fn relation_id(&self) -> &str {
        &self.relation_id
    }

    pub fn head(&self) -> &HeadKind {
        &self.head
    }

    pub fn complement(&self) -> &Complement {
        &self.complement
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValencyError {
    #[error("valency row {line}: expected 4 columns, got {columns}")]
    MalformedRow { line: usize, columns: usize },
    #[error("valency row {line}: unknown head_kind '{value}'")]
    UnknownHeadKind { line: usize, value: String },
    #[error("valency row {line}: an agreeing head needs 4 comma-separated forms, got {count}")]
    IncompleteAgreement { line: usize, count: usize },
    #[error("valency row {line}: unknown complement '{value}'")]
    UnknownComplement { line: usize, value: String },
    #[error("valency row {line}: unknown case '{value}'")]
    UnknownCase { line: usize, value: String },
    #[error("duplicate relation id '{0}'")]
    DuplicateRelation(String),
    #[error("no valency frame for relation '{0}'")]
    UnknownRelation(String),
}

/// Fingerprinted registry of valency frames.
#[derive(Debug, Clone)]
pub struct ValencyLexicon {
    frames: BTreeMap<String, ValencyFrame>,
    fingerprint: String,
}

impl ValencyLexicon {
    pub fn load_from_str(source: &str) -> Result<Self, ValencyError> {
        let mut frames: BTreeMap<String, ValencyFrame> = BTreeMap::new();
        for (index, raw) in source.lines().enumerate() {
            let line = index + 1;
            let trimmed = raw.trim_end_matches(['\r', '\n']);
            if trimmed.trim().is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let columns: Vec<&str> = trimmed.split('\t').collect();
            if columns.first() == Some(&"relation_id") {
                continue;
            }
            if columns.len() != 4 {
                return Err(ValencyError::MalformedRow {
                    line,
                    columns: columns.len(),
                });
            }
            let relation_id = columns[0].trim().to_string();
            let head = parse_head(line, columns[1].trim(), columns[2].trim())?;
            let complement = parse_complement(line, columns[3].trim())?;
            if frames.contains_key(&relation_id) {
                return Err(ValencyError::DuplicateRelation(relation_id));
            }
            frames.insert(
                relation_id.clone(),
                ValencyFrame {
                    relation_id,
                    head,
                    complement,
                },
            );
        }

        let mut hasher = Sha256::new();
        hasher.update(b"qxfx0:valency-lexicon:v1");
        hasher.update(source.as_bytes());
        Ok(Self {
            frames,
            fingerprint: format!("{:x}", hasher.finalize()),
        })
    }

    pub fn get(&self, relation_id: &str) -> Result<&ValencyFrame, ValencyError> {
        self.frames
            .get(relation_id)
            .ok_or_else(|| ValencyError::UnknownRelation(relation_id.to_string()))
    }

    pub fn contains(&self, relation_id: &str) -> bool {
        self.frames.contains_key(relation_id)
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &ValencyFrame)> {
        self.frames.iter()
    }

    /// Part of the realization snapshot: a changed lexicon is a changed
    /// realization contract, not a changed authority (ADR-0034 §8).
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn agreeing_relations(&self) -> Vec<&str> {
        self.frames
            .values()
            .filter(|frame| frame.head.agrees_with_subject())
            .map(|frame| frame.relation_id.as_str())
            .collect()
    }
}

fn parse_head(line: usize, kind: &str, forms: &str) -> Result<HeadKind, ValencyError> {
    match kind {
        "finite" => Ok(HeadKind::Finite {
            surface: forms.to_string(),
        }),
        "agreeing" => {
            let parts: Vec<&str> = forms.split(',').map(str::trim).collect();
            if parts.len() != 4 || parts.iter().any(|part| part.is_empty()) {
                return Err(ValencyError::IncompleteAgreement {
                    line,
                    count: parts.len(),
                });
            }
            Ok(HeadKind::Agreeing {
                masculine: parts[0].to_string(),
                feminine: parts[1].to_string(),
                neuter: parts[2].to_string(),
                plural: parts[3].to_string(),
            })
        }
        other => Err(ValencyError::UnknownHeadKind {
            line,
            value: other.to_string(),
        }),
    }
}

fn parse_complement(line: usize, value: &str) -> Result<Complement, ValencyError> {
    if value == "none" {
        return Ok(Complement::None);
    }
    if value == "uninflected" {
        return Ok(Complement::Uninflected);
    }
    if let Some(case) = value.strip_prefix("direct:") {
        return Ok(Complement::Direct(parse_case(line, case)?));
    }
    if let Some(rest) = value.strip_prefix("prep:") {
        let mut parts = rest.splitn(2, ':');
        let preposition = parts.next().unwrap_or_default().trim();
        let case = parts.next().unwrap_or_default().trim();
        if preposition.is_empty() || case.is_empty() {
            return Err(ValencyError::UnknownComplement {
                line,
                value: value.to_string(),
            });
        }
        return Ok(Complement::Prepositional {
            preposition: preposition.to_string(),
            case: parse_case(line, case)?,
        });
    }
    Err(ValencyError::UnknownComplement {
        line,
        value: value.to_string(),
    })
}

fn parse_case(line: usize, value: &str) -> Result<Case, ValencyError> {
    match value {
        "nom" => Ok(Case::Nominative),
        "gen" => Ok(Case::Genitive),
        "dat" => Ok(Case::Dative),
        "acc" => Ok(Case::Accusative),
        "ins" => Ok(Case::Instrumental),
        "prep" => Ok(Case::Prepositional),
        other => Err(ValencyError::UnknownCase {
            line,
            value: other.to_string(),
        }),
    }
}

/// The embedded lexicon, parsed once.
pub fn valency_lexicon() -> &'static ValencyLexicon {
    static LEXICON: OnceLock<ValencyLexicon> = OnceLock::new();
    LEXICON.get_or_init(|| {
        ValencyLexicon::load_from_str(VALENCY_FRAMES_TSV)
            .expect("embedded valency lexicon must parse")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_lexicon_parses_and_covers_the_audited_relations() {
        let lexicon = valency_lexicon();
        assert_eq!(lexicon.len(), 21, "one frame per admitted relation");
        assert!(!lexicon.fingerprint().is_empty());
    }

    /// The example ADR-0034 §7 names directly.
    #[test]
    fn government_matches_the_adr_examples() {
        let lexicon = valency_lexicon();
        assert_eq!(
            lexicon.get("zavisit").expect("frame").complement(),
            &Complement::Prepositional {
                preposition: "от".into(),
                case: Case::Genitive,
            }
        );
        assert_eq!(
            lexicon.get("trebuet").expect("frame").complement(),
            &Complement::Direct(Case::Genitive)
        );
        assert_eq!(
            lexicon.get("predpolagaet").expect("frame").complement(),
            &Complement::Direct(Case::Accusative)
        );
    }

    /// Government is one fact per relation, so two callers cannot disagree
    /// about the case the way two templates could.
    #[test]
    fn every_frame_has_exactly_one_government() {
        for (id, frame) in valency_lexicon().iter() {
            let complement = frame.complement();
            let case = complement.required_case();
            match complement {
                Complement::None | Complement::Uninflected => {
                    assert!(case.is_none(), "{id} demands a case it cannot govern");
                }
                _ => assert!(case.is_some(), "{id} governs without naming a case"),
            }
        }
    }

    /// The three legacy identifiers encode a gender. The lexicon must supply
    /// all four forms so the relation can serve any subject.
    #[test]
    fn agreeing_heads_supply_every_form() {
        let lexicon = valency_lexicon();
        let agreeing = lexicon.agreeing_relations();
        assert_eq!(agreeing, vec!["napravlena", "neobratimo", "svyazan"]);

        let frame = lexicon.get("napravlena").expect("frame");
        for (gender, expected) in [
            (Gender::Masculine, "направлен"),
            (Gender::Feminine, "направлена"),
            (Gender::Neuter, "направлено"),
        ] {
            assert_eq!(
                frame
                    .head()
                    .realize(AgreementFeatures::new(gender, Number::Singular)),
                expected
            );
        }
        assert_eq!(
            frame
                .head()
                .realize(AgreementFeatures::new(Gender::Feminine, Number::Plural)),
            "направлены"
        );
    }

    /// A finite verb must not vary with subject gender; if it did, the lexicon
    /// would be modelling agreement Russian does not have in this tense.
    #[test]
    fn finite_heads_ignore_subject_gender() {
        let frame = valency_lexicon().get("predpolagaet").expect("frame");
        let masculine = frame
            .head()
            .realize(AgreementFeatures::new(Gender::Masculine, Number::Singular));
        let feminine = frame
            .head()
            .realize(AgreementFeatures::new(Gender::Feminine, Number::Singular));
        assert_eq!(masculine, feminine);
        assert!(!frame.head().agrees_with_subject());
    }

    #[test]
    fn an_agreeing_head_missing_a_form_is_rejected() {
        let source = "relation_id\thead_kind\thead_forms\tcomplement\n\
                      broken\tagreeing\tсвязан,связана\tnone\n";
        assert!(matches!(
            ValencyLexicon::load_from_str(source),
            Err(ValencyError::IncompleteAgreement { .. })
        ));
    }

    #[test]
    fn duplicate_relations_are_rejected() {
        let source = "relation_id\thead_kind\thead_forms\tcomplement\n\
                      x\tfinite\tа\tnone\n\
                      x\tfinite\tб\tnone\n";
        assert!(matches!(
            ValencyLexicon::load_from_str(source),
            Err(ValencyError::DuplicateRelation(_))
        ));
    }

    #[test]
    fn unknown_case_is_rejected() {
        let source = "relation_id\thead_kind\thead_forms\tcomplement\n\
                      x\tfinite\tа\tdirect:vocative\n";
        assert!(matches!(
            ValencyLexicon::load_from_str(source),
            Err(ValencyError::UnknownCase { .. })
        ));
    }

    #[test]
    fn fingerprint_tracks_the_source() {
        let base = "relation_id\thead_kind\thead_forms\tcomplement\n\
                    x\tfinite\tа\tdirect:acc\n";
        let changed = "relation_id\thead_kind\thead_forms\tcomplement\n\
                       x\tfinite\tа\tdirect:gen\n";
        let left = ValencyLexicon::load_from_str(base).expect("lexicon");
        let right = ValencyLexicon::load_from_str(changed).expect("lexicon");
        assert_ne!(left.fingerprint(), right.fingerprint());
    }

    #[test]
    fn unknown_relation_is_an_error_not_a_default() {
        assert!(matches!(
            valency_lexicon().get("no_such_relation"),
            Err(ValencyError::UnknownRelation(_))
        ));
    }
}
