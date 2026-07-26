//! Audited admission registry for the first content-bearing response plans.

use crate::response_plan::{PredicateRef, SemanticId, SemanticProposition};
use crate::seed::COVERED_TOPICS;
use qxfx0_types::AtomId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const ARGUED_TOPICS_TSV: &str = include_str!("../assets/argued_topics.tsv");
pub const CONTENT_PROFILE: &str = "audited_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdmittedStatement {
    predicate_ref: PredicateRef,
    surface: String,
}

impl AdmittedStatement {
    pub fn predicate_ref(&self) -> &PredicateRef {
        &self.predicate_ref
    }

    /// Grounded renderer leaf. Response plans carry only `predicate_ref`.
    pub fn surface(&self) -> &str {
        &self.surface
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArguedTopic {
    topic: AtomId,
    primary_predicate_ref: PredicateRef,
    primary_proposition: SemanticProposition,
    thesis: AdmittedStatement,
    counterpoint: AdmittedStatement,
    consequence: Option<AdmittedStatement>,
    evidence_record: u16,
}

impl ArguedTopic {
    pub fn topic(&self) -> &AtomId {
        &self.topic
    }

    pub fn primary_predicate_ref(&self) -> &PredicateRef {
        &self.primary_predicate_ref
    }

    pub fn primary_proposition(&self) -> &SemanticProposition {
        &self.primary_proposition
    }

    pub fn thesis(&self) -> &AdmittedStatement {
        &self.thesis
    }

    pub fn counterpoint(&self) -> &AdmittedStatement {
        &self.counterpoint
    }

    pub fn consequence(&self) -> Option<&AdmittedStatement> {
        self.consequence.as_ref()
    }

    /// Resolve a grounded leaf only inside this topic's admitted predicate
    /// boundary. A renderer must not use the wider semantic graph as a
    /// substitute for this lookup.
    pub fn statement_for(&self, predicate_ref: &PredicateRef) -> Option<&AdmittedStatement> {
        std::iter::once(&self.thesis)
            .chain(std::iter::once(&self.counterpoint))
            .chain(self.consequence.iter())
            .find(|statement| statement.predicate_ref() == predicate_ref)
    }

    pub fn evidence_record(&self) -> u16 {
        self.evidence_record
    }

    pub fn statement_count(&self) -> usize {
        2 + usize::from(self.consequence.is_some())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentAssetMetrics {
    pub recognition_topics_total: usize,
    pub content_predicates_total: usize,
    pub argued_topics_admitted: usize,
    pub argued_predicates_admitted: usize,
    pub profile_enabled: &'static str,
}

#[derive(Debug, Clone)]
pub struct ArguedTopicRegistry {
    topics: BTreeMap<String, ArguedTopic>,
    predicate_refs: BTreeSet<PredicateRef>,
    content_predicates_total: usize,
}

impl ArguedTopicRegistry {
    fn parse(source: &str) -> Result<Self, String> {
        let mut topics = BTreeMap::new();
        let mut predicate_refs = BTreeSet::new();
        let mut content_predicates_total = 0;
        let mut evidence_record = 0u16;

        for (line_index, line) in source.lines().enumerate() {
            if line.trim().is_empty()
                || line.starts_with('#')
                || line.starts_with("topic\tpredicate_id\t")
            {
                continue;
            }
            let columns = line.split('\t').collect::<Vec<_>>();
            if !(7..=8).contains(&columns.len()) {
                return Err(format!(
                    "argued_topics.tsv line {} has {} columns, expected 7 or 8",
                    line_index + 1,
                    columns.len()
                ));
            }
            let topic = columns[0];
            let predicate_id = columns[1];
            let subject_id = columns[2];
            let relation_id = columns[3];
            let object_id = columns[4];
            let thesis = columns[5];
            let counterpoint = columns[6];
            let consequence = columns.get(7).copied().unwrap_or_default();
            for (name, value) in [
                ("topic", &topic),
                ("predicate_id", &predicate_id),
                ("subject_id", &subject_id),
                ("relation_id", &relation_id),
                ("object_id", &object_id),
                ("thesis", &thesis),
                ("counterpoint", &counterpoint),
            ] {
                if value.trim().is_empty() {
                    return Err(format!(
                        "argued_topics.tsv line {} has an empty {name}",
                        line_index + 1
                    ));
                }
            }
            if !COVERED_TOPICS.contains(&topic) {
                return Err(format!("admitted topic '{topic}' is not recognized"));
            }
            if !thesis.to_lowercase().starts_with(&topic.to_lowercase()) {
                return Err(format!("thesis for '{topic}' is not topic-grounded"));
            }

            evidence_record = evidence_record
                .checked_add(1)
                .ok_or_else(|| "too many argued topic records".to_string())?;
            let primary_ref = PredicateRef::try_new(predicate_id)?;
            let counterpoint_ref = PredicateRef::try_new(format!("{predicate_id}.counterpoint"))?;
            let consequence_ref = (!consequence.is_empty())
                .then(|| PredicateRef::try_new(format!("{predicate_id}.consequence")))
                .transpose()?;
            for predicate_ref in std::iter::once(&primary_ref)
                .chain(std::iter::once(&counterpoint_ref))
                .chain(consequence_ref.iter())
            {
                if !predicate_refs.insert(predicate_ref.clone()) {
                    return Err(format!(
                        "duplicate predicate reference '{}'",
                        predicate_ref.as_str()
                    ));
                }
            }

            let thesis_statement = AdmittedStatement {
                predicate_ref: primary_ref.clone(),
                surface: thesis.into(),
            };
            let counterpoint_statement = AdmittedStatement {
                predicate_ref: counterpoint_ref,
                surface: counterpoint.into(),
            };
            let consequence_statement = consequence_ref.map(|predicate_ref| AdmittedStatement {
                predicate_ref,
                surface: consequence.into(),
            });
            let entry = ArguedTopic {
                topic: AtomId::new(topic),
                primary_predicate_ref: primary_ref,
                primary_proposition: SemanticProposition::CanonicalPredicate {
                    subject: SemanticId::try_new(subject_id)?,
                    relation: SemanticId::try_new(relation_id)?,
                    object: SemanticId::try_new(object_id)?,
                },
                thesis: thesis_statement,
                counterpoint: counterpoint_statement,
                consequence: consequence_statement,
                evidence_record,
            };
            content_predicates_total += entry.statement_count();
            if topics.insert(topic.into(), entry).is_some() {
                return Err(format!("duplicate argued topic '{topic}'"));
            }
        }

        if topics.len() != 30 {
            return Err(format!(
                "audited_v1 must admit exactly 30 topics, found {}",
                topics.len()
            ));
        }

        Ok(Self {
            topics,
            predicate_refs,
            content_predicates_total,
        })
    }

    pub fn get(&self, topic: &str) -> Option<&ArguedTopic> {
        self.topics.get(topic)
    }

    pub fn topics(&self) -> impl Iterator<Item = &ArguedTopic> {
        self.topics.values()
    }

    pub fn contains_predicate(&self, predicate_ref: &PredicateRef) -> bool {
        self.predicate_refs.contains(predicate_ref)
    }

    pub fn metrics(&self) -> ContentAssetMetrics {
        ContentAssetMetrics {
            recognition_topics_total: COVERED_TOPICS.len(),
            content_predicates_total: self.content_predicates_total,
            argued_topics_admitted: self.topics.len(),
            argued_predicates_admitted: self.topics.len(),
            profile_enabled: CONTENT_PROFILE,
        }
    }
}

static REGISTRY: OnceLock<Result<ArguedTopicRegistry, String>> = OnceLock::new();

pub fn argued_topic_registry() -> Result<&'static ArguedTopicRegistry, &'static str> {
    REGISTRY
        .get_or_init(|| ArguedTopicRegistry::parse(ARGUED_TOPICS_TSV))
        .as_ref()
        .map_err(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audited_profile_has_expected_admission_boundary() {
        let registry = argued_topic_registry().unwrap();
        let metrics = registry.metrics();

        assert_eq!(metrics.recognition_topics_total, 107);
        assert_eq!(metrics.argued_topics_admitted, 30);
        assert_eq!(metrics.argued_predicates_admitted, 30);
        assert_eq!(metrics.content_predicates_total, 69);
        assert_eq!(metrics.profile_enabled, "audited_v1");
    }

    #[test]
    fn predicate_ids_and_grounded_leaves_are_stable() {
        let registry = argued_topic_registry().unwrap();
        let freedom = registry.get("свобода").unwrap();

        assert_eq!(freedom.primary_predicate_ref().as_str(), "freedom_choice");
        assert_eq!(
            freedom.counterpoint().predicate_ref().as_str(),
            "freedom_choice.counterpoint"
        );
        assert_eq!(freedom.statement_count(), 3);
        assert!(registry.contains_predicate(freedom.thesis().predicate_ref()));
    }

    #[test]
    fn every_admitted_topic_is_recognized_and_argued() {
        let registry = argued_topic_registry().unwrap();

        for topic in registry.topics() {
            assert!(COVERED_TOPICS.contains(&topic.topic().as_str()));
            assert!(!topic.thesis().surface().trim().is_empty());
            assert!(!topic.counterpoint().surface().trim().is_empty());
        }
    }
}
