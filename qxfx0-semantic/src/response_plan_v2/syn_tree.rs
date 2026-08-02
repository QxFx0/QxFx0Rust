//! Syntactic realization: NP/VP/Clause, resolution, linearization
//! (ADR-0034 §7).
//!
//! The chain is `plan → SynTree → ResolvedSynTree → surface`. Its point is that
//! **no case appears in the plan and none appears in a template string**: the
//! plan says which relation is asserted, the valency lexicon says which case
//! that relation's head governs, and resolution computes the form from the
//! morphology bundle.
//!
//! Completeness is proven *before* linearization. Once a [`ResolvedSynTree`]
//! exists, every word is chosen and every form resolved, so linearization is
//! total for that snapshot and cannot fail on a missing form. "Incomplete
//! morphology" is therefore an error of resolution, never of rendering.
//!
//! One honest limitation is visible in the type system rather than hidden.
//! Admitted object phrases are stored in the corpus as whole strings
//! (`возможность выбора`), not decomposed into a head noun with modifiers, so
//! they cannot be inflected compositionally. [`NounPhrase::FixedPhrase`] carries
//! such a phrase together with the case it is already in, and resolution checks
//! that the declared case is the one the head governs. That check is what turns
//! the corpus gap into a detectable mismatch instead of a silent wrong ending.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::discourse::DiscourseOccurrenceId;
use super::valency::{AgreementFeatures, Complement, ValencyError, ValencyLexicon};
use qxfx0_morphology::{Case, Gender, MorphologyRuntime, Number};

/// A nominal, either inflectable or admitted verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NounPhrase {
    /// A single lemma the morphology bundle can inflect into any case.
    Lexical { lemma: String },
    /// An admitted multi-word phrase carried verbatim, with the case it is
    /// already in. `None` means the phrase is not case-marked at all, which is
    /// only admissible against an uninflected complement.
    FixedPhrase {
        text: String,
        declared_case: Option<Case>,
    },
}

impl NounPhrase {
    pub fn lexical(lemma: impl Into<String>) -> Self {
        Self::Lexical {
            lemma: lemma.into(),
        }
    }

    pub fn fixed(text: impl Into<String>, declared_case: Option<Case>) -> Self {
        Self::FixedPhrase {
            text: text.into(),
            declared_case,
        }
    }
}

/// A predication: the relation plus whatever it governs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerbPhrase {
    relation_id: String,
    complement: Option<NounPhrase>,
}

impl VerbPhrase {
    pub fn new(relation_id: impl Into<String>, complement: Option<NounPhrase>) -> Self {
        Self {
            relation_id: relation_id.into(),
            complement,
        }
    }

    pub fn relation_id(&self) -> &str {
        &self.relation_id
    }

    pub fn complement(&self) -> Option<&NounPhrase> {
        self.complement.as_ref()
    }
}

/// Subject plus predication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clause {
    subject: NounPhrase,
    predicate: VerbPhrase,
}

impl Clause {
    pub fn new(subject: NounPhrase, predicate: VerbPhrase) -> Self {
        Self { subject, predicate }
    }

    pub fn subject(&self) -> &NounPhrase {
        &self.subject
    }

    pub fn predicate(&self) -> &VerbPhrase {
        &self.predicate
    }
}

/// Unresolved syntax, addressed by discourse occurrence.
///
/// Transient by design: it is built, resolved and discarded within one turn,
/// so it is serializable for tracing but never deserialized from storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct SynTree {
    clauses: Vec<(DiscourseOccurrenceId, Clause)>,
}

impl SynTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, occurrence: DiscourseOccurrenceId, clause: Clause) {
        self.clauses.push((occurrence, clause));
    }

    pub fn len(&self) -> usize {
        self.clauses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(DiscourseOccurrenceId, Clause)> {
        self.clauses.iter()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RealizationError {
    #[error("valency: {0}")]
    Valency(#[from] ValencyError),
    #[error("no {case:?} form for '{lemma}' in the morphology bundle")]
    IncompleteForm { lemma: String, case: Case },
    #[error("'{lemma}' is absent from the morphology bundle")]
    UnknownLemma { lemma: String },
    #[error(
        "relation '{relation}' governs {required:?} but the phrase '{phrase}' is marked {declared:?}"
    )]
    CaseMismatch {
        relation: String,
        phrase: String,
        required: Case,
        declared: Option<Case>,
    },
    #[error("relation '{relation}' governs {required:?} but no complement was supplied")]
    MissingComplement { relation: String, required: Case },
    #[error("relation '{relation}' takes no complement but one was supplied")]
    UnexpectedComplement { relation: String },
    #[error("the subject must be an inflectable lemma, not a verbatim phrase")]
    UninflectableSubject,
}

/// A clause whose every slot is filled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedClause {
    pub occurrence: DiscourseOccurrenceId,
    pub subject_surface: String,
    pub head_surface: String,
    pub preposition: Option<String>,
    pub complement_surface: Option<String>,
    pub agreement: AgreementFeatures,
    pub governed_case: Option<Case>,
}

/// Evidence that resolution left nothing open (ADR-0034 §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RealizationCompletenessCertificate {
    pub clauses: usize,
    pub resolved_slots: usize,
    pub agreeing_heads: usize,
    pub fixed_phrases: usize,
    pub valency_fingerprint: String,
    pub morphology_sha256: String,
}

/// Syntax with no unresolved slots. Linearization of this type is total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedSynTree {
    clauses: Vec<ResolvedClause>,
    certificate: RealizationCompletenessCertificate,
}

impl ResolvedSynTree {
    pub fn clauses(&self) -> &[ResolvedClause] {
        &self.clauses
    }

    pub fn certificate(&self) -> &RealizationCompletenessCertificate {
        &self.certificate
    }

    /// Total for the snapshot this tree was resolved under.
    ///
    /// There is no fallible path here by construction: every surface was
    /// already chosen at resolution, so this only concatenates.
    pub fn linearize(&self) -> Vec<String> {
        self.clauses
            .iter()
            .map(|clause| {
                let mut parts = vec![clause.subject_surface.clone(), clause.head_surface.clone()];
                if let Some(preposition) = &clause.preposition {
                    parts.push(preposition.clone());
                }
                if let Some(complement) = &clause.complement_surface {
                    parts.push(complement.clone());
                }
                parts.join(" ")
            })
            .collect()
    }
}

/// Resolve every slot against the valency lexicon and the morphology bundle.
pub fn resolve(
    tree: &SynTree,
    lexicon: &ValencyLexicon,
    morphology: &MorphologyRuntime,
) -> Result<ResolvedSynTree, RealizationError> {
    let mut clauses = Vec::with_capacity(tree.len());
    let mut resolved_slots = 0usize;
    let mut agreeing_heads = 0usize;
    let mut fixed_phrases = 0usize;

    for (occurrence, clause) in tree.iter() {
        let frame = lexicon.get(clause.predicate().relation_id())?;

        // The subject must be inflectable: its features drive agreement, and a
        // verbatim phrase carries none that can be read off reliably.
        let subject_lemma = match clause.subject() {
            NounPhrase::Lexical { lemma } => lemma,
            NounPhrase::FixedPhrase { .. } => return Err(RealizationError::UninflectableSubject),
        };
        let subject_surface = inflect(morphology, subject_lemma, Case::Nominative)?;
        let agreement = subject_agreement(morphology, subject_lemma);
        resolved_slots += 1;

        let head_surface = frame.head().realize(agreement).to_string();
        if frame.head().agrees_with_subject() {
            agreeing_heads += 1;
        }
        resolved_slots += 1;

        let complement = clause.predicate().complement();
        let (preposition, complement_surface, governed_case) = match frame.complement() {
            Complement::None => {
                if complement.is_some() {
                    return Err(RealizationError::UnexpectedComplement {
                        relation: frame.relation_id().to_string(),
                    });
                }
                (None, None, None)
            }
            Complement::Uninflected => {
                let surface = match complement {
                    Some(NounPhrase::FixedPhrase { text, .. }) => {
                        fixed_phrases += 1;
                        Some(text.clone())
                    }
                    Some(NounPhrase::Lexical { lemma }) => {
                        Some(inflect(morphology, lemma, Case::Nominative)?)
                    }
                    None => None,
                };
                if surface.is_some() {
                    resolved_slots += 1;
                }
                (None, surface, None)
            }
            governing => {
                let required = governing
                    .required_case()
                    .expect("a governing complement names a case");
                let Some(phrase) = complement else {
                    return Err(RealizationError::MissingComplement {
                        relation: frame.relation_id().to_string(),
                        required,
                    });
                };
                let surface = match phrase {
                    NounPhrase::Lexical { lemma } => inflect(morphology, lemma, required)?,
                    NounPhrase::FixedPhrase {
                        text,
                        declared_case,
                    } => {
                        // The corpus cannot inflect this phrase, so the only
                        // available check is that it already stands in the case
                        // the head governs.
                        if *declared_case != Some(required) {
                            return Err(RealizationError::CaseMismatch {
                                relation: frame.relation_id().to_string(),
                                phrase: text.clone(),
                                required,
                                declared: *declared_case,
                            });
                        }
                        fixed_phrases += 1;
                        text.clone()
                    }
                };
                resolved_slots += 1;
                (
                    governing.preposition().map(str::to_string),
                    Some(surface),
                    Some(required),
                )
            }
        };

        clauses.push(ResolvedClause {
            occurrence: occurrence.clone(),
            subject_surface,
            head_surface,
            preposition,
            complement_surface,
            agreement,
            governed_case,
        });
    }

    Ok(ResolvedSynTree {
        certificate: RealizationCompletenessCertificate {
            clauses: clauses.len(),
            resolved_slots,
            agreeing_heads,
            fixed_phrases,
            valency_fingerprint: lexicon.fingerprint().to_string(),
            morphology_sha256: morphology.lexemes_sha256().to_string(),
        },
        clauses,
    })
}

fn inflect(
    morphology: &MorphologyRuntime,
    lemma: &str,
    case: Case,
) -> Result<String, RealizationError> {
    if morphology.get_lexeme(lemma).is_none() {
        return Err(RealizationError::UnknownLemma {
            lemma: lemma.to_string(),
        });
    }
    morphology
        .inflect(lemma, case, Number::Singular)
        .ok_or_else(|| RealizationError::IncompleteForm {
            lemma: lemma.to_string(),
            case,
        })
}

/// Read agreement features from the curated bundle.
///
/// Endings are not consulted: they do not decide gender in Russian, which is
/// how `разум направлена` reached a release binary.
fn subject_agreement(morphology: &MorphologyRuntime, lemma: &str) -> AgreementFeatures {
    let gender = morphology
        .get_lexeme(lemma)
        .map(|entry| entry.features.gender)
        .unwrap_or(Gender::Unknown);
    AgreementFeatures::new(gender, Number::Singular)
}

/// Build a `BTreeMap` view of the resolved clauses by occurrence.
pub fn by_occurrence(tree: &ResolvedSynTree) -> BTreeMap<&DiscourseOccurrenceId, &ResolvedClause> {
    tree.clauses()
        .iter()
        .map(|clause| (&clause.occurrence, clause))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response_plan::SemanticId;
    use crate::response_plan_v2::discourse::{DiscoursePlan, DiscourseTree};
    use crate::response_plan_v2::proposition::{PropositionDagBuilder, PropositionNode};
    use crate::response_plan_v2::valency::valency_lexicon;

    fn occurrence() -> DiscourseOccurrenceId {
        let mut builder = PropositionDagBuilder::new();
        let id = builder.insert(PropositionNode::Predicate {
            subject: SemanticId::try_new("свобода").expect("semantic id"),
            relation: SemanticId::try_new("predpolagaet").expect("semantic id"),
            object: SemanticId::try_new("возможность выбора").expect("semantic id"),
        });
        DiscoursePlan::try_new(DiscourseTree::Thesis(id))
            .expect("plan")
            .projected_claims()
            .remove(0)
            .occurrence
    }

    fn morphology() -> &'static MorphologyRuntime {
        qxfx0_morphology::get_runtime()
    }

    fn resolve_one(
        subject: &str,
        relation: &str,
        complement: Option<NounPhrase>,
    ) -> ResolvedClause {
        let mut tree = SynTree::new();
        tree.push(
            occurrence(),
            Clause::new(
                NounPhrase::lexical(subject),
                VerbPhrase::new(relation, complement),
            ),
        );
        resolve(&tree, valency_lexicon(), morphology())
            .expect("resolution")
            .clauses
            .remove(0)
    }

    /// The case comes from the lexicon, never from the plan or a template.
    #[test]
    fn government_selects_the_complement_case() {
        let clause = resolve_one("свобода", "trebuet", Some(NounPhrase::lexical("выбор")));
        assert_eq!(clause.governed_case, Some(Case::Genitive));
        assert_eq!(clause.complement_surface.as_deref(), Some("выбора"));
        assert_eq!(clause.preposition, None);
    }

    #[test]
    fn prepositional_government_carries_its_preposition() {
        let clause = resolve_one("истина", "zavisit", Some(NounPhrase::lexical("разум")));
        assert_eq!(clause.preposition.as_deref(), Some("от"));
        assert_eq!(clause.governed_case, Some(Case::Genitive));
        assert_eq!(clause.complement_surface.as_deref(), Some("разума"));
    }

    /// The same relation with a different governed case produces a different
    /// ending without any change to the plan.
    #[test]
    fn changing_only_the_relation_changes_the_ending() {
        let genitive = resolve_one("свобода", "trebuet", Some(NounPhrase::lexical("выбор")));
        let accusative = resolve_one(
            "свобода",
            "predpolagaet",
            Some(NounPhrase::lexical("выбор")),
        );
        assert_eq!(genitive.complement_surface.as_deref(), Some("выбора"));
        assert_eq!(accusative.complement_surface.as_deref(), Some("выбор"));
    }

    /// The regression this layer exists for. `разум` is masculine, so an
    /// agreeing head must not surface in the feminine.
    #[test]
    fn agreeing_head_follows_the_subject_gender() {
        let masculine = resolve_one("разум", "napravlena", Some(NounPhrase::lexical("истина")));
        assert_eq!(masculine.head_surface, "направлен");
        assert_eq!(masculine.agreement.gender, Gender::Masculine);

        let feminine = resolve_one("любовь", "napravlena", Some(NounPhrase::lexical("истина")));
        assert_eq!(feminine.head_surface, "направлена");
        assert_eq!(feminine.agreement.gender, Gender::Feminine);

        let neuter = resolve_one("время", "napravlena", Some(NounPhrase::lexical("истина")));
        assert_eq!(neuter.head_surface, "направлено");
        assert_eq!(neuter.agreement.gender, Gender::Neuter);
    }

    /// Gender comes from the bundle, not from the ending: these four lemmas
    /// are exactly the ones an ending heuristic gets wrong.
    #[test]
    fn agreement_uses_the_bundle_not_the_ending() {
        for (lemma, expected) in [
            ("память", Gender::Feminine),
            ("смерть", Gender::Feminine),
            ("любовь", Gender::Feminine),
            ("время", Gender::Neuter),
        ] {
            let clause = resolve_one(lemma, "svyazan", Some(NounPhrase::lexical("разум")));
            assert_eq!(clause.agreement.gender, expected, "{lemma}");
        }
    }

    #[test]
    fn finite_head_is_invariant_across_subject_gender() {
        let masculine = resolve_one("разум", "vyrazhaet", Some(NounPhrase::lexical("истина")));
        let feminine = resolve_one("свобода", "vyrazhaet", Some(NounPhrase::lexical("истина")));
        assert_eq!(masculine.head_surface, feminine.head_surface);
        assert_eq!(masculine.head_surface, "выражает");
    }

    /// A verbatim corpus phrase must already stand in the governed case. This
    /// check is what makes the undecomposed-object gap detectable.
    #[test]
    fn fixed_phrase_must_match_the_governed_case() {
        let mut tree = SynTree::new();
        tree.push(
            occurrence(),
            Clause::new(
                NounPhrase::lexical("свобода"),
                VerbPhrase::new(
                    "predpolagaet",
                    Some(NounPhrase::fixed(
                        "возможность выбора",
                        Some(Case::Accusative),
                    )),
                ),
            ),
        );
        let resolved = resolve(&tree, valency_lexicon(), morphology()).expect("resolution");
        assert_eq!(
            resolved.clauses()[0].complement_surface.as_deref(),
            Some("возможность выбора")
        );
        assert_eq!(resolved.certificate().fixed_phrases, 1);
    }

    #[test]
    fn a_fixed_phrase_in_the_wrong_case_is_rejected() {
        let mut tree = SynTree::new();
        tree.push(
            occurrence(),
            Clause::new(
                NounPhrase::lexical("свобода"),
                VerbPhrase::new(
                    "trebuet",
                    Some(NounPhrase::fixed(
                        "возможность выбора",
                        Some(Case::Accusative),
                    )),
                ),
            ),
        );
        assert!(matches!(
            resolve(&tree, valency_lexicon(), morphology()),
            Err(RealizationError::CaseMismatch {
                required: Case::Genitive,
                ..
            })
        ));
    }

    #[test]
    fn a_head_without_a_complement_takes_none() {
        let clause = resolve_one("время", "neobratimo", None);
        assert_eq!(clause.head_surface, "необратимо");
        assert_eq!(clause.complement_surface, None);
        assert_eq!(clause.governed_case, None);
    }

    #[test]
    fn supplying_a_complement_to_an_intransitive_head_is_rejected() {
        let mut tree = SynTree::new();
        tree.push(
            occurrence(),
            Clause::new(
                NounPhrase::lexical("время"),
                VerbPhrase::new("neobratimo", Some(NounPhrase::lexical("разум"))),
            ),
        );
        assert!(matches!(
            resolve(&tree, valency_lexicon(), morphology()),
            Err(RealizationError::UnexpectedComplement { .. })
        ));
    }

    #[test]
    fn omitting_a_governed_complement_is_rejected() {
        let mut tree = SynTree::new();
        tree.push(
            occurrence(),
            Clause::new(
                NounPhrase::lexical("свобода"),
                VerbPhrase::new("trebuet", None),
            ),
        );
        assert!(matches!(
            resolve(&tree, valency_lexicon(), morphology()),
            Err(RealizationError::MissingComplement { .. })
        ));
    }

    #[test]
    fn an_unknown_lemma_fails_at_resolution_not_at_rendering() {
        let mut tree = SynTree::new();
        tree.push(
            occurrence(),
            Clause::new(
                NounPhrase::lexical("кванточайник"),
                VerbPhrase::new("vyrazhaet", Some(NounPhrase::lexical("истина"))),
            ),
        );
        assert!(matches!(
            resolve(&tree, valency_lexicon(), morphology()),
            Err(RealizationError::UnknownLemma { .. })
        ));
    }

    #[test]
    fn linearization_is_total_once_resolved() {
        let mut tree = SynTree::new();
        tree.push(
            occurrence(),
            Clause::new(
                NounPhrase::lexical("истина"),
                VerbPhrase::new("zavisit", Some(NounPhrase::lexical("разум"))),
            ),
        );
        let resolved = resolve(&tree, valency_lexicon(), morphology()).expect("resolution");
        assert_eq!(resolved.linearize(), vec!["истина зависит от разума"]);
    }

    #[test]
    fn certificate_records_the_realization_snapshot() {
        let mut tree = SynTree::new();
        tree.push(
            occurrence(),
            Clause::new(
                NounPhrase::lexical("разум"),
                VerbPhrase::new("svyazan", Some(NounPhrase::lexical("истина"))),
            ),
        );
        let resolved = resolve(&tree, valency_lexicon(), morphology()).expect("resolution");
        let certificate = resolved.certificate();
        assert_eq!(certificate.clauses, 1);
        assert_eq!(certificate.agreeing_heads, 1);
        assert_eq!(
            certificate.valency_fingerprint,
            valency_lexicon().fingerprint()
        );
        assert!(!certificate.morphology_sha256.is_empty());
        assert_eq!(resolved.linearize(), vec!["разум связан с истиной"]);
    }

    #[test]
    fn a_verbatim_subject_is_rejected() {
        let mut tree = SynTree::new();
        tree.push(
            occurrence(),
            Clause::new(
                NounPhrase::fixed("возможность выбора", Some(Case::Nominative)),
                VerbPhrase::new("vyrazhaet", Some(NounPhrase::lexical("истина"))),
            ),
        );
        assert!(matches!(
            resolve(&tree, valency_lexicon(), morphology()),
            Err(RealizationError::UninflectableSubject)
        ));
    }

    #[test]
    fn resolution_is_deterministic() {
        let build = || {
            let mut tree = SynTree::new();
            tree.push(
                occurrence(),
                Clause::new(
                    NounPhrase::lexical("свобода"),
                    VerbPhrase::new("predpolagaet", Some(NounPhrase::lexical("выбор"))),
                ),
            );
            resolve(&tree, valency_lexicon(), morphology()).expect("resolution")
        };
        assert_eq!(build().linearize(), build().linearize());
        assert_eq!(build(), build());
    }
}
