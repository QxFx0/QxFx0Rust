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
//!
//! The head surface is no longer trusted prose. Each row carries
//! `head_lemma` plus a `conjugation` strategy:
//!
//! * `finite3` — the head is a finite verb; the pinned surface must equal the
//!   3rd-person singular derived from `head_lemma` through the embedded verb
//!   lexicon, and every person of the paradigm becomes addressable
//!   (`conjugated_head`), so a future discourse can vary person without a new
//!   pinned string.
//! * `agreeing_short` — the head is a short adjective/participle; all four
//!   pinned forms must equal the short cells of `head_lemma` in the embedded
//!   adjective lexicon.
//! * `pinned` — no paradigm exists to derive from (copulas, participles the
//!   lexicons do not materialize). The surface stays pinned and honestly
//!   labeled.
//!
//! A mismatch between a pinned surface and its derivation is a load error:
//! the embedded asset is release-validated, and a drifted TSV must fail loud,
//! not realize a fabricated form.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::OnceLock;

use qxfx0_morphology::verbs::VerbPerson;
use qxfx0_morphology::{Case, Gender, Number};

const VALENCY_FRAMES_TSV: &str = include_str!("../assets/valency_frames.tsv");

const FINITE_PERSON_KEYS: [(VerbPerson, &str); 6] = [
    (VerbPerson::FirstSingular, "f1sg"),
    (VerbPerson::SecondSingular, "f2sg"),
    (VerbPerson::ThirdSingular, "f3sg"),
    (VerbPerson::FirstPlural, "f1pl"),
    (VerbPerson::SecondPlural, "f2pl"),
    (VerbPerson::ThirdPlural, "f3pl"),
];

const PAST_KEYS: [&str; 4] = ["pm", "pf", "pn", "ppl"];
const SHORT_KEYS: [&str; 4] = ["short_m", "short_f", "short_n", "short_pl"];

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

/// Does `text` begin with `word` as a complete whitespace-delimited token?
///
/// Kept at the valency boundary so gates and realization use exactly the same
/// interpretation of an embedded governed preposition.
pub fn starts_with_word(text: &str, word: &str) -> bool {
    text.strip_prefix(word)
        .is_some_and(|rest| rest.chars().next().is_some_and(char::is_whitespace))
}

/// How the head surface is derived, and from which lemma.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "strategy")]
pub enum ConjugationStrategy {
    /// A finite verb; the pinned surface is the f3sg of `lemma` in the
    /// embedded verb lexicon, verified at load time.
    Finite3 { lemma: String },
    /// A short adjective or participle; the four pinned forms are the short
    /// cells of `lemma` in the embedded adjective lexicon, verified at load
    /// time.
    AgreeingShort { lemma: String },
    /// No derivable paradigm (copulas, unmaterialized participles). The
    /// surface stays pinned.
    Pinned,
}

impl ConjugationStrategy {
    fn tag(&self) -> &'static str {
        match self {
            Self::Finite3 { .. } => "finite3",
            Self::AgreeingShort { .. } => "agreeing_short",
            Self::Pinned => "pinned",
        }
    }

    fn lemma(&self) -> Option<&str> {
        match self {
            Self::Finite3 { lemma } | Self::AgreeingShort { lemma } => Some(lemma),
            Self::Pinned => None,
        }
    }
}

/// The derived paradigm certificate for one head.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadConjugation {
    strategy: ConjugationStrategy,
    paradigm_digest: String,
}

impl HeadConjugation {
    pub fn strategy(&self) -> &ConjugationStrategy {
        &self.strategy
    }

    /// SHA-256 over the materialized cells of this head's paradigm.
    pub fn paradigm_digest(&self) -> &str {
        &self.paradigm_digest
    }
}

/// How the head realizes itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadKind {
    /// A finite verb. Russian present tense agrees in person and number but
    /// not in gender. Both surfaces are derived from the verb lexicon at
    /// load time (3rd singular + 3rd plural); the TSV pins the singular.
    Finite { singular: String, plural: String },
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
    /// A finite head selects by number only (gender never matters for finite
    /// verbs); an agreeing head additionally selects by gender in singular.
    /// This is why passing the wrong gender to a finite verb is harmless
    /// while passing it to a short form is the `разум направлена` defect.
    pub fn realize(&self, features: AgreementFeatures) -> &str {
        match self {
            Self::Finite { singular, plural } => match features.number {
                Number::Plural => plural,
                Number::Singular => singular,
            },
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
    conjugation: HeadConjugation,
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

    pub fn conjugation(&self) -> &HeadConjugation {
        &self.conjugation
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValencyError {
    #[error("valency row {line}: expected 6 columns, got {columns}")]
    MalformedRow { line: usize, columns: usize },
    #[error("valency row {line}: unknown head_kind '{value}'")]
    UnknownHeadKind { line: usize, value: String },
    #[error("valency row {line}: an agreeing head needs 4 comma-separated forms, got {count}")]
    IncompleteAgreement { line: usize, count: usize },
    #[error("valency row {line}: unknown complement '{value}'")]
    UnknownComplement { line: usize, value: String },
    #[error("valency row {line}: unknown case '{value}'")]
    UnknownCase { line: usize, value: String },
    #[error("valency row {line}: unknown conjugation strategy '{value}'")]
    UnknownConjugation { line: usize, value: String },
    #[error("valency row {line}: strategy '{strategy}' requires a head_lemma")]
    MissingHeadLemma { line: usize, strategy: String },
    #[error(
        "valency row {line}: relation '{relation}' pins head '{pinned}' but '{lemma}' conjugates to '{derived}'"
    )]
    ConjugationMismatch {
        line: usize,
        relation: String,
        pinned: String,
        lemma: String,
        derived: String,
    },
    #[error(
        "valency row {line}: relation '{relation}' pins head '{pinned}' but '{lemma}' has no {cell} cell"
    )]
    MissingDerivationCell {
        line: usize,
        relation: String,
        pinned: String,
        lemma: String,
        cell: String,
    },
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
            if columns.len() != 6 {
                return Err(ValencyError::MalformedRow {
                    line,
                    columns: columns.len(),
                });
            }
            let relation_id = columns[0].trim().to_string();
            let head = parse_head(line, columns[1].trim(), columns[2].trim())?;
            let complement = parse_complement(line, columns[3].trim())?;
            let strategy = parse_conjugation(line, columns[4].trim(), columns[5].trim())?;
            let conjugation = verify_conjugation(line, &relation_id, &head, strategy.clone())?;
            let head = complete_finite_plural(line, &relation_id, head, &strategy)?;
            if frames.contains_key(&relation_id) {
                return Err(ValencyError::DuplicateRelation(relation_id));
            }
            frames.insert(
                relation_id.clone(),
                ValencyFrame {
                    relation_id,
                    head,
                    complement,
                    conjugation,
                },
            );
        }

        let conjugation_digest = frames_conjugation_digest(&frames);
        let mut hasher = Sha256::new();
        hasher.update(b"qxfx0:valency-lexicon:v2");
        hasher.update(source.as_bytes());
        hasher.update(conjugation_digest.as_bytes());
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

    /// Part of the realization snapshot: a changed lexicon — including a
    /// changed derivation behind an unchanged surface — is a changed
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

    /// Conjugate a finite head for an arbitrary person, deriving the form
    /// from the embedded verb lexicon instead of the pinned 3rd-person
    /// surface. Returns `Ok(None)` when the head carries no verb paradigm or
    /// the lexicon has no cell for that person — never a fabricated form.
    pub fn conjugated_head(
        &self,
        relation_id: &str,
        person: VerbPerson,
    ) -> Result<Option<String>, ValencyError> {
        let frame = self.get(relation_id)?;
        let ConjugationStrategy::Finite3 { lemma } = frame.conjugation.strategy() else {
            return Ok(None);
        };
        let entry = qxfx0_morphology::verb_lexicon::lookup(lemma);
        let key = FINITE_PERSON_KEYS
            .iter()
            .find(|(candidate, _)| *candidate == person)
            .expect("person table is total")
            .1;
        Ok(entry.and_then(|entry| entry.form(key)).map(str::to_string))
    }
}

fn parse_head(line: usize, kind: &str, forms: &str) -> Result<HeadKind, ValencyError> {
    match kind {
        "finite" => Ok(HeadKind::Finite {
            singular: forms.to_string(),
            // The plural is derived from the verb paradigm at load time by
            // `complete_finite_plural`; the TSV pins the singular only.
            plural: String::new(),
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

fn parse_conjugation(
    line: usize,
    lemma: &str,
    strategy: &str,
) -> Result<ConjugationStrategy, ValencyError> {
    let lemma = lemma.trim();
    let strategy = strategy.trim();
    match strategy {
        "finite3" => {
            if lemma.is_empty() || lemma == "—" {
                return Err(ValencyError::MissingHeadLemma {
                    line,
                    strategy: strategy.to_string(),
                });
            }
            Ok(ConjugationStrategy::Finite3 {
                lemma: lemma.to_string(),
            })
        }
        "agreeing_short" => {
            if lemma.is_empty() || lemma == "—" {
                return Err(ValencyError::MissingHeadLemma {
                    line,
                    strategy: strategy.to_string(),
                });
            }
            Ok(ConjugationStrategy::AgreeingShort {
                lemma: lemma.to_string(),
            })
        }
        "pinned" => Ok(ConjugationStrategy::Pinned),
        other => Err(ValencyError::UnknownConjugation {
            line,
            value: other.to_string(),
        }),
    }
}

/// Derive the 3rd-plural surface for a finite head. Fail-closed like the
/// singular verification: a verb paradigm without f3pl cannot head a
/// clause over a plural subject, and silently reusing the singular would
/// fabricate agreement («деньги требует»).
fn complete_finite_plural(
    line: usize,
    relation_id: &str,
    head: HeadKind,
    strategy: &ConjugationStrategy,
) -> Result<HeadKind, ValencyError> {
    match (head, strategy) {
        (HeadKind::Finite { singular, .. }, ConjugationStrategy::Finite3 { lemma }) => {
            let plural = qxfx0_morphology::verb_lexicon::lookup(lemma)
                .and_then(|entry| entry.form("f3pl"))
                .ok_or_else(|| ValencyError::MissingDerivationCell {
                    line,
                    relation: relation_id.to_string(),
                    pinned: singular.clone(),
                    lemma: lemma.clone(),
                    cell: "f3pl".into(),
                })?;
            Ok(HeadKind::Finite {
                singular,
                plural: plural.to_string(),
            })
        }
        // Pinned heads (copulas) do not inflect: both numbers share the
        // single pinned surface.
        (HeadKind::Finite { singular, .. }, ConjugationStrategy::Pinned) => Ok(HeadKind::Finite {
            plural: singular.clone(),
            singular,
        }),
        (head, _) => Ok(head),
    }
}

/// Derive the paradigm behind a strategy and cross-check it against the
/// pinned surface. Fail-closed: a drifted asset is a release invariant, not
/// a turn-level error.
fn verify_conjugation(
    line: usize,
    relation_id: &str,
    head: &HeadKind,
    strategy: ConjugationStrategy,
) -> Result<HeadConjugation, ValencyError> {
    match (&strategy, head) {
        (ConjugationStrategy::Finite3 { lemma }, HeadKind::Finite { singular, .. }) => {
            let Some(entry) = qxfx0_morphology::verb_lexicon::lookup(lemma) else {
                return Err(ValencyError::MissingDerivationCell {
                    line,
                    relation: relation_id.to_string(),
                    pinned: singular.clone(),
                    lemma: lemma.clone(),
                    cell: "verb_lexicon".into(),
                });
            };
            match entry.form("f3sg") {
                Some(derived) if derived == singular => {}
                Some(derived) => {
                    return Err(ValencyError::ConjugationMismatch {
                        line,
                        relation: relation_id.to_string(),
                        pinned: singular.clone(),
                        lemma: lemma.clone(),
                        derived: derived.to_string(),
                    })
                }
                None => {
                    return Err(ValencyError::MissingDerivationCell {
                        line,
                        relation: relation_id.to_string(),
                        pinned: singular.clone(),
                        lemma: lemma.clone(),
                        cell: "f3sg".into(),
                    })
                }
            }
        }
        (
            ConjugationStrategy::AgreeingShort { lemma },
            HeadKind::Agreeing {
                masculine,
                feminine,
                neuter,
                plural,
            },
        ) => {
            let Some(entry) = qxfx0_morphology::adjective_lexicon::lookup(lemma) else {
                return Err(ValencyError::MissingDerivationCell {
                    line,
                    relation: relation_id.to_string(),
                    pinned: masculine.clone(),
                    lemma: lemma.clone(),
                    cell: "adjective_lexicon".into(),
                });
            };
            for (cell, pinned) in [
                ("short_m", masculine),
                ("short_f", feminine),
                ("short_n", neuter),
                ("short_pl", plural),
            ] {
                match entry.form(cell) {
                    Some(derived) if derived == pinned => {}
                    Some(derived) => {
                        return Err(ValencyError::ConjugationMismatch {
                            line,
                            relation: relation_id.to_string(),
                            pinned: pinned.clone(),
                            lemma: lemma.clone(),
                            derived: derived.to_string(),
                        })
                    }
                    None => {
                        return Err(ValencyError::MissingDerivationCell {
                            line,
                            relation: relation_id.to_string(),
                            pinned: pinned.clone(),
                            lemma: lemma.clone(),
                            cell: cell.to_string(),
                        })
                    }
                }
            }
        }
        // A pinned strategy makes no claim about the surface.
        (ConjugationStrategy::Pinned, _) => {}
        // A paradigm strategy on the wrong head shape is a TSV authoring
        // error: there is no pinned surface to check against.
        (strategy, _) => {
            return Err(ValencyError::MissingHeadLemma {
                line,
                strategy: strategy.tag().to_string(),
            })
        }
    }
    let paradigm_digest = frame_paradigm_digest(relation_id, &strategy);
    Ok(HeadConjugation {
        strategy,
        paradigm_digest,
    })
}

fn absorb(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn frame_paradigm_digest(relation_id: &str, strategy: &ConjugationStrategy) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"qxfx0:valency-conjugation:v1");
    absorb(&mut hasher, relation_id);
    absorb(&mut hasher, strategy.tag());
    if let Some(lemma) = strategy.lemma() {
        absorb(&mut hasher, lemma);
    }
    match strategy {
        ConjugationStrategy::Finite3 { lemma } => {
            if let Some(entry) = qxfx0_morphology::verb_lexicon::lookup(lemma) {
                for (_, key) in FINITE_PERSON_KEYS {
                    absorb(&mut hasher, key);
                    absorb(&mut hasher, entry.form(key).unwrap_or(""));
                }
                for key in PAST_KEYS {
                    absorb(&mut hasher, key);
                    absorb(&mut hasher, entry.form(key).unwrap_or(""));
                }
            }
        }
        ConjugationStrategy::AgreeingShort { lemma } => {
            if let Some(entry) = qxfx0_morphology::adjective_lexicon::lookup(lemma) {
                for key in SHORT_KEYS {
                    absorb(&mut hasher, key);
                    absorb(&mut hasher, entry.form(key).unwrap_or(""));
                }
            }
        }
        ConjugationStrategy::Pinned => {}
    }
    format!("{:x}", hasher.finalize())
}

/// Digest over every frame's derived paradigm, in relation-id order. Part of
/// the lexicon fingerprint, mirrored by `tools/gen_audited_corpus_manifest.py`.
fn frames_conjugation_digest(frames: &BTreeMap<String, ValencyFrame>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"qxfx0:valency-conjugations:v1");
    hasher.update((frames.len() as u64).to_be_bytes());
    for (relation_id, frame) in frames {
        absorb(&mut hasher, relation_id);
        absorb(&mut hasher, frame.conjugation.paradigm_digest());
    }
    format!("{:x}", hasher.finalize())
}

/// The embedded lexicon, parsed once.
pub fn valency_lexicon() -> &'static ValencyLexicon {
    static LEXICON: OnceLock<ValencyLexicon> = OnceLock::new();
    LEXICON.get_or_init(|| {
        // The infallible process-global accessor is established public API. The source is
        // compile-time embedded and exercised by lexicon/reference-vector tests, so failure
        // is a release-build invariant rather than an external-input error boundary.
        ValencyLexicon::load_from_str(VALENCY_FRAMES_TSV)
            .expect("embedded valency lexicon is release-validated")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct RealizationVectors {
        schema: String,
        vectors: Vec<RealizationVector>,
    }
    #[derive(Deserialize)]
    struct RealizationVector {
        name: String,
        relation: String,
        required_case: Option<String>,
        preposition: Option<String>,
    }

    #[test]
    fn realization_reference_vectors_are_executable() {
        let vectors: RealizationVectors = serde_json::from_str(include_str!(
            "../../docs/reference-vectors/response-plan-v2-realization-v1.json"
        ))
        .expect("realization vectors parse");
        assert_eq!(vectors.schema, "qxfx0.response-plan-v2.realization.v1");
        for vector in vectors.vectors {
            let frame = valency_lexicon()
                .get(&vector.relation)
                .unwrap_or_else(|error| panic!("{}: {error}", vector.name));
            let actual_case = frame.complement().required_case().map(|case| {
                match case {
                    Case::Nominative => "nominative",
                    Case::Genitive => "genitive",
                    Case::Dative => "dative",
                    Case::Accusative => "accusative",
                    Case::Instrumental => "instrumental",
                    Case::Prepositional => "prepositional",
                }
                .to_string()
            });
            assert_eq!(actual_case, vector.required_case, "{}", vector.name);
            assert_eq!(
                frame.complement().preposition().map(str::to_string),
                vector.preposition,
                "{}",
                vector.name
            );
        }
    }

    #[test]
    fn embedded_lexicon_parses_and_covers_the_audited_relations() {
        let lexicon = valency_lexicon();
        assert_eq!(lexicon.len(), 32, "one frame per admitted relation");
        assert!(!lexicon.fingerprint().is_empty());
    }

    /// Every non-copula finite head is derived, not trusted: the pinned
    /// surface equals the paradigm table's cell for it.
    #[test]
    fn finite_heads_are_derived_from_the_verb_lexicon() {
        let lexicon = valency_lexicon();
        for (id, frame) in lexicon.iter() {
            let HeadKind::Finite { singular, plural } = frame.head() else {
                continue;
            };
            match frame.conjugation().strategy() {
                ConjugationStrategy::Finite3 { lemma } => {
                    let derived = lexicon
                        .conjugated_head(id, VerbPerson::ThirdSingular)
                        .expect("relation exists")
                        .expect("f3sg cell exists");
                    assert_eq!(
                        &derived, singular,
                        "{id}: pinned surface disagrees with {lemma} paradigm"
                    );
                    let derived_plural = lexicon
                        .conjugated_head(id, VerbPerson::ThirdPlural)
                        .expect("relation exists")
                        .expect("f3pl cell exists");
                    assert_eq!(
                        &derived_plural, plural,
                        "{id}: plural surface disagrees with {lemma} paradigm"
                    );
                }
                ConjugationStrategy::Pinned => {
                    assert_eq!(
                        singular, plural,
                        "{id}: pinned finite head must share one surface"
                    );
                }
                ConjugationStrategy::AgreeingShort { .. } => {
                    panic!("{id}: short-form strategy on a finite head")
                }
            }
        }
    }

    #[test]
    fn conjugation_reaches_every_person_of_the_paradigm() {
        let lexicon = valency_lexicon();
        assert_eq!(
            lexicon
                .conjugated_head("predpolagaet", VerbPerson::FirstSingular)
                .unwrap(),
            Some("предполагаю".into())
        );
        assert_eq!(
            lexicon
                .conjugated_head("predpolagaet", VerbPerson::ThirdPlural)
                .unwrap(),
            Some("предполагают".into())
        );
        // Reflexive heads conjugate too — the class the verb lexicon
        // generator had silently omitted.
        assert_eq!(
            lexicon
                .conjugated_head("stroitsya", VerbPerson::ThirdSingular)
                .unwrap(),
            Some("строится".into())
        );
        assert_eq!(
            lexicon
                .conjugated_head("otlichaetsya", VerbPerson::ThirdPlural)
                .unwrap(),
            Some("отличаются".into())
        );
        // Heads without a verb paradigm return None, never a fabrication.
        assert_eq!(
            lexicon
                .conjugated_head("eto", VerbPerson::FirstSingular)
                .unwrap(),
            None
        );
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
        let source = "relation_id\thead_kind\thead_forms\tcomplement\thead_lemma\tconjugation\n\
                      broken\tagreeing\tсвязан,связана\tnone\t—\tpinned\n";
        assert!(matches!(
            ValencyLexicon::load_from_str(source),
            Err(ValencyError::IncompleteAgreement { .. })
        ));
    }

    #[test]
    fn duplicate_relations_are_rejected() {
        let source = "relation_id\thead_kind\thead_forms\tcomplement\thead_lemma\tconjugation\n\
                      x\tfinite\tа\tnone\t—\tpinned\n\
                      x\tfinite\tб\tnone\t—\tpinned\n";
        assert!(matches!(
            ValencyLexicon::load_from_str(source),
            Err(ValencyError::DuplicateRelation(_))
        ));
    }

    #[test]
    fn unknown_case_is_rejected() {
        let source = "relation_id\thead_kind\thead_forms\tcomplement\thead_lemma\tconjugation\n\
                      x\tfinite\tа\tdirect:vocative\t—\tpinned\n";
        assert!(matches!(
            ValencyLexicon::load_from_str(source),
            Err(ValencyError::UnknownCase { .. })
        ));
    }

    #[test]
    fn a_drifted_pinned_surface_fails_loud() {
        // The lemma conjugates to «делает», not the pinned «делает бы».
        let source = "relation_id\thead_kind\thead_forms\tcomplement\thead_lemma\tconjugation\n\
                      x\tfinite\tделает бы\tnone\tделать\tfinite3\n";
        assert!(matches!(
            ValencyLexicon::load_from_str(source),
            Err(ValencyError::ConjugationMismatch { .. })
        ));
    }

    #[test]
    fn a_missing_lemma_for_a_strategy_is_rejected() {
        let source = "relation_id\thead_kind\thead_forms\tcomplement\thead_lemma\tconjugation\n\
                      x\tfinite\tа\tnone\t—\tfinite3\n";
        assert!(matches!(
            ValencyLexicon::load_from_str(source),
            Err(ValencyError::MissingHeadLemma { .. })
        ));
    }

    #[test]
    fn fingerprint_tracks_the_source_and_the_derivation() {
        let base = "relation_id\thead_kind\thead_forms\tcomplement\thead_lemma\tconjugation\n\
                    x\tfinite\tделает\tnone\tделать\tfinite3\n";
        let changed_complement =
            "relation_id\thead_kind\thead_forms\tcomplement\thead_lemma\tconjugation\n\
                                  x\tfinite\tделает\tdirect:gen\tделать\tfinite3\n";
        // Same surface, but the derivation contract is gone: pinned heads
        // carry no paradigm digest, so the fingerprint must still change.
        let changed_strategy =
            "relation_id\thead_kind\thead_forms\tcomplement\thead_lemma\tconjugation\n\
                                x\tfinite\tделает\tnone\t—\tpinned\n";
        let left = ValencyLexicon::load_from_str(base).expect("lexicon");
        let middle = ValencyLexicon::load_from_str(changed_complement).expect("lexicon");
        let right = ValencyLexicon::load_from_str(changed_strategy).expect("lexicon");
        assert_ne!(left.fingerprint(), middle.fingerprint());
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
