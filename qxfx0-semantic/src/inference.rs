//! DeriveAtoms — rule-based inference for creating new atoms from
//! existing pattern combinations. WP-G: default-on promotion flag.
//!
//! Three inference rules:
//!   1. Contact under stress: NeedContact + Exhaustion → amplified NeedContact
//!   2. Contradiction under doubt: Contradiction + Doubt → amplified Contradiction
//!   3. Agency lost while searching: AgencyLost + Searching → exhaustion marker

use qxfx0_types::atom::AtomId;

/// Atom tags that can be detected from system state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AtomTag {
    Searching(String),
    Exhaustion(String),
    Verification(String),
    Doubt(String),
    NeedContact(String),
    NeedMeaning(String),
    AgencyLost(String),       // conatus energy as string
    AgencyFound(String),      // conatus energy as string
    Anchoring(String),
    Contradiction(String, String),
    CustomAtom(String, String),
    AffectiveAtom(String, String),
}

/// A derived atom produced by inference rules.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DerivedAtom {
    pub id: AtomId,
    pub tag: AtomTag,
    pub rule: DeriveRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeriveRule {
    ContactUnderStress,
    ContradictionUnderDoubt,
    AgencySearchExhaustion,
}

/// Derive additional atoms from existing atom tags via multi-step patterns.
pub fn derive_atoms(tags: &[AtomTag]) -> Vec<DerivedAtom> {
    let mut result = Vec::new();

    let has_need_contact = tags.iter().any(|t| matches!(t, AtomTag::NeedContact(_)));
    let has_exhaustion = tags.iter().any(|t| matches!(t, AtomTag::Exhaustion(_)));
    let has_contradiction = tags.iter().any(|t| matches!(t, AtomTag::Contradiction(_, _)));
    let has_doubt = tags.iter().any(|t| matches!(t, AtomTag::Doubt(_)));
    let has_agency_lost = tags.iter().any(|t| matches!(t, AtomTag::AgencyLost(_)));
    let has_searching = tags.iter().any(|t| matches!(t, AtomTag::Searching(_)));

    if has_need_contact && has_exhaustion {
        result.push(DerivedAtom {
            id: AtomId::new("derived_contact_stress"),
            tag: AtomTag::NeedContact("stressed".into()),
            rule: DeriveRule::ContactUnderStress,
        });
    }

    if has_contradiction && has_doubt {
        result.push(DerivedAtom {
            id: AtomId::new("derived_contradiction_doubt"),
            tag: AtomTag::Contradiction("amplified".into(), "amplified".into()),
            rule: DeriveRule::ContradictionUnderDoubt,
        });
    }

    if has_agency_lost && has_searching {
        result.push(DerivedAtom {
            id: AtomId::new("derived_agency_search"),
            tag: AtomTag::Exhaustion("search_exhausted".into()),
            rule: DeriveRule::AgencySearchExhaustion,
        });
    }

    result
}

/// Classify the current system state into atom tags for inference.
pub fn classify_state_tags(
    topic_in_graph: bool,
    field_confidence: f64,
    field_counterfactual: f64,
    field_resonance: f64,
    conatus_energy: f64,
    angst: f64,
) -> Vec<AtomTag> {
    let mut tags = Vec::new();

    if !topic_in_graph {
        tags.push(AtomTag::Searching("unknown_topic".into()));
    }

    if conatus_energy < 3.0 {
        tags.push(AtomTag::Exhaustion("low_conatus".into()));
    }

    if angst > 0.7 {
        tags.push(AtomTag::Doubt("high_angst".into()));
    }

    if field_counterfactual > 0.7 {
        tags.push(AtomTag::Contradiction("high_counterfactual".into(), "".into()));
    }

    if field_resonance < 0.2 {
        tags.push(AtomTag::NeedMeaning("low_resonance".into()));
    }

    if conatus_energy < 0.3 {
        tags.push(AtomTag::AgencyLost(format!("{:.2}", conatus_energy)));
    } else if conatus_energy > 1.2 {
        tags.push(AtomTag::AgencyFound(format!("{:.2}", conatus_energy)));
    }

    if topic_in_graph && field_confidence > 0.7 {
        tags.push(AtomTag::Anchoring("confident_topic".into()));
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_derivation_without_patterns() {
        let tags = vec![AtomTag::Searching("x".into())];
        let derived = derive_atoms(&tags);
        assert!(derived.is_empty());
    }

    #[test]
    fn test_contact_under_stress() {
        let tags = vec![
            AtomTag::NeedContact("test".into()),
            AtomTag::Exhaustion("test".into()),
        ];
        let derived = derive_atoms(&tags);
        assert_eq!(derived.len(), 1);
        assert!(matches!(derived[0].rule, DeriveRule::ContactUnderStress));
    }

    #[test]
    fn test_contradiction_under_doubt() {
        let tags = vec![
            AtomTag::Contradiction("a".into(), "b".into()),
            AtomTag::Doubt("test".into()),
        ];
        let derived = derive_atoms(&tags);
        assert_eq!(derived.len(), 1);
        assert!(matches!(derived[0].rule, DeriveRule::ContradictionUnderDoubt));
    }

    #[test]
    fn test_agency_search_exhaustion() {
        let tags = vec![
            AtomTag::AgencyLost("0.50".into()),
            AtomTag::Searching("test".into()),
        ];
        let derived = derive_atoms(&tags);
        assert_eq!(derived.len(), 1);
        assert!(matches!(derived[0].rule, DeriveRule::AgencySearchExhaustion));
    }

    #[test]
    fn test_multiple_rules_fire() {
        let tags = vec![
            AtomTag::NeedContact("a".into()),
            AtomTag::Exhaustion("b".into()),
            AtomTag::Contradiction("c".into(), "d".into()),
            AtomTag::Doubt("e".into()),
        ];
        let derived = derive_atoms(&tags);
        assert!(derived.len() >= 2);
    }

    #[test]
    fn test_classify_state_tags() {
        let tags = classify_state_tags(true, 0.8, 0.3, 0.5, 15.0, 0.1);
        assert!(tags.iter().any(|t| matches!(t, AtomTag::Anchoring(_))));
        assert!(tags.iter().any(|t| matches!(t, AtomTag::AgencyFound(_))));
    }
}
