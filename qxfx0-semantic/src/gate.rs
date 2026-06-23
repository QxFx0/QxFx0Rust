use qxfx0_types::atom::{PathProof, Relation, RelationSource};

/// GeneratedPredicateGate — 5 gates for graph-generated predicates.
/// All gates are pure, total, deterministic.
pub struct GeneratedPredicateGate;

impl GeneratedPredicateGate {
    /// Gate G1: path contains ≥1 edge whose from_atom is the topic.
    pub fn gate_specificity(proof: &PathProof) -> GateResult {
        let has_topic_edge = proof.edges.iter().any(|e| e.topic == proof.topic);
        if has_topic_edge {
            GateResult::Pass
        } else {
            GateResult::Fail("no edge references the topic atom".into())
        }
    }

    /// Gate G2: no edge has from_atom == to_atom (self-reference).
    pub fn gate_non_tautology(proof: &PathProof) -> GateResult {
        let tautological = proof.edges.iter().any(|e| e.from == e.to);
        if tautological {
            GateResult::Fail("edge has from == to (tautological)".into())
        } else {
            GateResult::Pass
        }
    }

    /// Gate G3: path has ≥1 edge (non-empty proof).
    pub fn gate_path_provenance(proof: &PathProof) -> GateResult {
        if proof.edges.is_empty() {
            GateResult::Fail("empty path proof".into())
        } else {
            GateResult::Pass
        }
    }

    /// Gate G4: every edge's source is in the admissible set.
    /// SubstrateExtractedRaw is blocked.
    pub fn gate_source_whitelist(proof: &PathProof) -> GateResult {
        let inadmissible: Vec<_> = proof
            .edges
            .iter()
            .filter(|e| e.source == RelationSource::SubstrateExtractedRaw)
            .collect();
        if inadmissible.is_empty() {
            GateResult::Pass
        } else {
            GateResult::Fail(format!(
                "edge has source=SubstrateExtractedRaw: {}",
                inadmissible[0].ru_original
            ))
        }
    }

    /// Gate G5: no raw substrate edges.
    pub fn gate_non_substrate_output(proof: &PathProof) -> GateResult {
        let has_raw = proof
            .edges
            .iter()
            .any(|e| e.source == RelationSource::SubstrateExtractedRaw);
        if has_raw {
            GateResult::Fail("path contains raw substrate edge".into())
        } else {
            GateResult::Pass
        }
    }

    /// Run all gates on a single path proof.
    pub fn validate_path(proof: &PathProof) -> GateVerdict {
        let g1 = Self::gate_specificity(proof);
        let g2 = Self::gate_non_tautology(proof);
        let g3 = Self::gate_path_provenance(proof);
        let g4 = Self::gate_source_whitelist(proof);
        let g5 = Self::gate_non_substrate_output(proof);

        let overall = g1.is_pass() && g2.is_pass() && g3.is_pass() && g4.is_pass() && g5.is_pass();

        GateVerdict {
            g1_specificity: g1,
            g2_non_tautology: g2,
            g3_path_provenance: g3,
            g4_source_whitelist: g4,
            g5_non_substrate: g5,
            overall,
        }
    }

    /// Validate a single relation (lightweight check).
    pub fn validate_relation(rel: &Relation) -> bool {
        rel.from != rel.to
            && !rel.ru_original.is_empty()
            && rel.source != RelationSource::SubstrateExtractedRaw
    }

    /// Filter relations through gate validation.
    pub fn filter_gated_relations<'a>(topic: &str, rels: &[&'a Relation]) -> Vec<&'a Relation> {
        rels.iter()
            .filter(|r| {
                let proof = PathProof {
                    edges: vec![(**r).clone()],
                    topic: topic.to_string(),
                };
                Self::validate_path(&proof).overall
            })
            .copied()
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GateResult {
    Pass,
    Fail(String),
}

impl GateResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, GateResult::Pass)
    }
}

#[derive(Debug, Clone)]
pub struct GateVerdict {
    pub g1_specificity: GateResult,
    pub g2_non_tautology: GateResult,
    pub g3_path_provenance: GateResult,
    pub g4_source_whitelist: GateResult,
    pub g5_non_substrate: GateResult,
    pub overall: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::atom::{AtomId, ObjectCase};
    use qxfx0_types::RelationType;

    fn make_rel(from: &str, to: &str, topic: &str) -> Relation {
        Relation {
            from: AtomId::new(from),
            to: AtomId::new(to),
            rel_type: RelationType::RelPresupposes,
            object_case: ObjectCase::CaseAccusative,
            object_text: "test".into(),
            verb_override: None,
            ru_original: "test".into(),
            en_original: String::new(),
            source: RelationSource::SeedFromPredicate,
            topic: topic.into(),
            rationale: None,
            counter: None,
            synthesis: None,
        }
    }

    #[test]
    fn test_gate_passes_valid_path() {
        let rel = make_rel("свобода", "выбор", "свобода");
        let proof = PathProof {
            edges: vec![rel],
            topic: "свобода".into(),
        };
        let verdict = GeneratedPredicateGate::validate_path(&proof);
        assert!(verdict.overall);
    }

    #[test]
    fn test_gate_fails_tautology() {
        let rel = make_rel("свобода", "свобода", "свобода");
        let proof = PathProof {
            edges: vec![rel],
            topic: "свобода".into(),
        };
        let verdict = GeneratedPredicateGate::validate_path(&proof);
        assert!(!verdict.overall);
    }

    #[test]
    fn test_gate_fails_empty() {
        let proof = PathProof {
            edges: vec![],
            topic: "свобода".into(),
        };
        let verdict = GeneratedPredicateGate::validate_path(&proof);
        assert!(!verdict.overall);
    }

    #[test]
    fn test_gate_fails_substrate() {
        let mut rel = make_rel("свобода", "выбор", "свобода");
        rel.source = RelationSource::SubstrateExtractedRaw;
        let proof = PathProof {
            edges: vec![rel],
            topic: "свобода".into(),
        };
        let verdict = GeneratedPredicateGate::validate_path(&proof);
        assert!(!verdict.overall);
    }
}
