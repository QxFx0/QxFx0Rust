//! TemplateRegistry — loads and indexes surface templates for verbalization.
//!
//! Templates are stored in data/semantic/templates/templates.json and mapped
//! by RelationType. Each template is a Russian sentence pattern with
//! placeholder slots like {FROM}, {TO|gen}, etc.

use qxfx0_types::RelationType;
use serde::Deserialize;
use std::collections::BTreeMap;

/// A surface template for verbalizing a semantic relation.
#[derive(Debug, Clone, Deserialize)]
pub struct SurfaceTemplate {
    /// Format string with placeholders: "{FROM} предполагает {TO|acc}"
    pub pattern: String,
    /// Register: philosophical, explanatory, conversational, dialogical
    pub register: String,
    /// Complexity: 1 (simple) — 3 (complex, with subordinate clauses)
    pub complexity: u8,
    /// Weight (0-1): how often this template is selected relative to others
    pub weight: f64,
}

/// Registry of templates indexed by RelationType.
#[derive(Debug, Clone)]
pub struct TemplateRegistry {
    templates: BTreeMap<RelationType, Vec<SurfaceTemplate>>,
}

impl TemplateRegistry {
    /// Load from the embedded templates.json data.
    pub fn load() -> Self {
        let json = include_str!("../../data/semantic/templates/templates.json");
        let raw: BTreeMap<String, Vec<SurfaceTemplate>> =
            serde_json::from_str(json).expect("templates.json must be valid JSON");

        let mut templates: BTreeMap<RelationType, Vec<SurfaceTemplate>> = BTreeMap::new();
        for (key, tmpls) in raw {
            if let Some(rt) = relation_type_from_template_key(&key) {
                templates.insert(rt, tmpls);
            }
        }

        TemplateRegistry { templates }
    }

    /// Get templates for a RelationType. Falls back to a minimal default.
    pub fn get(&self, rt: RelationType) -> &[SurfaceTemplate] {
        self.templates.get(&rt).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    /// Select a template deterministically:
    /// - Filter by register and complexity constraints
    /// - Use seed for deterministic weighted selection
    pub fn select(
        &self,
        rt: RelationType,
        register: &str,
        max_complexity: u8,
        seed: u64,
        used_indices: &[usize],
    ) -> Option<(usize, &SurfaceTemplate)> {
        let tmpls = self.get(rt);
        if tmpls.is_empty() {
            return None;
        }

        let candidates: Vec<(usize, &SurfaceTemplate)> = tmpls
            .iter()
            .enumerate()
            .filter(|(i, t)| {
                !used_indices.contains(i)
                    && t.register == register
                    && t.complexity <= max_complexity
            })
            .collect();

        if candidates.is_empty() {
            // Fall back: any template, ignoring register/complexity constraints
            let fallback: Vec<(usize, &SurfaceTemplate)> = tmpls
                .iter()
                .enumerate()
                .filter(|(i, _)| !used_indices.contains(i))
                .collect();

            if fallback.is_empty() {
                return tmpls.first().map(|t| (0, t));
            }

            let idx = (seed as usize) % fallback.len();
            return Some(fallback[idx]);
        }

        // Weighted selection: deterministic via seed
        let total_weight: f64 = candidates.iter().map(|(_, t)| t.weight).sum();
        if total_weight <= 0.0 {
            let idx = (seed as usize) % candidates.len();
            return Some(candidates[idx]);
        }

        let selector = (seed as f64 / 1000.0) % total_weight;
        let mut cumulative = 0.0;
        for (i, t) in &candidates {
            cumulative += t.weight;
            if cumulative >= selector {
                return Some((*i, t));
            }
        }

        candidates.last().copied()
    }
}

/// Map template JSON keys to RelationType.
fn relation_type_from_template_key(key: &str) -> Option<RelationType> {
    match key {
        "RelPresupposes" => Some(RelationType::RelPresupposes),
        "RelRequires" => Some(RelationType::RelRequires),
        "RelLimitedBy" => Some(RelationType::RelLimitedBy),
        "RelContrastsWith" => Some(RelationType::RelContrastsWith),
        "RelDetermines" => Some(RelationType::RelDetermines),
        "RelIncludes" => Some(RelationType::RelIncludes),
        "RelStructures" => Some(RelationType::RelStructures),
        "RelExpresses" => Some(RelationType::RelExpresses),
        "RelEvokes" => Some(RelationType::RelEvokes),
        "RelDependsOn" => Some(RelationType::RelDependsOn),
        "RelRelatedTo" => Some(RelationType::RelRelatedTo),
        "RelMeans" => Some(RelationType::RelMeans),
        "RelDiffersFrom" => Some(RelationType::RelDiffersFrom),
        "RelClaims" => Some(RelationType::RelClaims),
        "RelVerifiedBy" => Some(RelationType::RelVerifiedBy),
        "RelSignals" => Some(RelationType::RelSignals),
        "RelPreserves" => Some(RelationType::RelPreserves),
        "RelTransformsInto" => Some(RelationType::RelTransformsInto),
        "RelDirectedAt" => Some(RelationType::RelDirectedAt),
        "RelSupports" => Some(RelationType::RelSupports),
        "RelReconstructs" => Some(RelationType::RelReconstructs),
        "RelNotReducibleTo" => Some(RelationType::RelNotReducibleTo),
        "RelNegates" => Some(RelationType::RelNegates),
        "RelIsA" => Some(RelationType::RelIsA),
        "RelDenotes" => Some(RelationType::RelDenotes),
        "RelNecessaryFor" => Some(RelationType::RelNecessaryFor),
        "RelReliesOn" => Some(RelationType::RelReliesOn),
        "RelPointsTo" => Some(RelationType::RelPointsTo),
        "RelOrientsToward" => Some(RelationType::RelOrientsToward),
        "RelCapableOf" => Some(RelationType::RelCapableOf),
        "RelConnects" => Some(RelationType::RelConnects),
        "RelUnifies" => Some(RelationType::RelUnifies),
        "RelCreatedFrom" => Some(RelationType::RelCreatedFrom),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_templates() {
        let reg = TemplateRegistry::load();
        assert!(!reg.is_empty());
        assert!(reg.get(RelationType::RelPresupposes).len() >= 3);
    }

    #[test]
    fn test_select_deterministic() {
        let reg = TemplateRegistry::load();
        let a = reg.select(RelationType::RelPresupposes, "philosophical", 2, 42, &[]);
        let b = reg.select(RelationType::RelPresupposes, "philosophical", 2, 42, &[]);
        assert_eq!(a.map(|(i, _)| i), b.map(|(i, _)| i));
    }

    #[test]
    fn test_select_no_duplicate_indices() {
        let reg = TemplateRegistry::load();
        let first = reg.select(RelationType::RelPresupposes, "philosophical", 3, 10, &[]);
        assert!(first.is_some());
        let (idx1, _) = first.unwrap();

        let second = reg.select(RelationType::RelPresupposes, "philosophical", 3, 20, &[idx1]);
        assert!(second.is_some());
        let (idx2, _) = second.unwrap();
        assert_ne!(idx1, idx2);
    }

    #[test]
    fn test_select_fallback_when_all_excluded() {
        let reg = TemplateRegistry::load();
        let tmpls = reg.get(RelationType::RelPresupposes);
        let all_indices: Vec<usize> = (0..tmpls.len()).collect();
        let result = reg.select(RelationType::RelPresupposes, "philosophical", 3, 99, &all_indices);
        assert!(result.is_some());
    }
}
