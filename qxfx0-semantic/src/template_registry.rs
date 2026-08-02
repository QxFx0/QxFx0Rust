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
#[derive(Debug, Clone, Default)]
pub struct TemplateRegistry {
    templates: BTreeMap<RelationType, Vec<SurfaceTemplate>>,
}

/// The embedded template source, verbatim.
///
/// Exposed so a gate can hash exactly the bytes the binary was built with and
/// detect drift between `templates.json` and a census manifest generated from
/// it (ADR-0034 §10). Reading the file from disk would defeat that check.
pub const EMBEDDED_TEMPLATES_JSON: &str =
    include_str!("../../data/semantic/templates/templates.json");

impl TemplateRegistry {
    /// The embedded template source the registry is built from.
    pub const fn embedded_source() -> &'static str {
        EMBEDDED_TEMPLATES_JSON
    }

    /// Load from the embedded templates.json data.
    ///
    /// If the embedded JSON is malformed (e.g. after manual edits), falls back
    /// to an empty registry and logs a warning instead of panicking.
    pub fn load() -> Self {
        let json = EMBEDDED_TEMPLATES_JSON;
        let raw: BTreeMap<String, Vec<SurfaceTemplate>> = match serde_json::from_str(json) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    "Failed to parse embedded templates.json: {}. Using empty registry.",
                    e
                );
                return Self::default();
            }
        };

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

    pub fn relation_type_count(&self) -> usize {
        self.templates.len()
    }

    pub fn template_count(&self) -> usize {
        self.templates.values().map(Vec::len).sum()
    }

    /// Validate embedded surface templates for health checks.
    pub fn validate(&self) -> Vec<String> {
        let mut violations = Vec::new();
        if self.templates.is_empty() {
            violations.push("template registry is empty".into());
        }
        for (relation_type, templates) in &self.templates {
            if templates.is_empty() {
                violations.push(format!("{relation_type:?} has no templates"));
            }
            for (index, template) in templates.iter().enumerate() {
                if template.pattern.trim().is_empty()
                    || !template.pattern.contains("{FROM")
                    || !(template.pattern.contains("{TO") || template.pattern.contains("{OBJ"))
                {
                    violations.push(format!(
                        "{relation_type:?} template {index} has invalid placeholders"
                    ));
                }
                if template.register.trim().is_empty() {
                    violations.push(format!(
                        "{relation_type:?} template {index} has an empty register"
                    ));
                }
                if !(1..=3).contains(&template.complexity) {
                    violations.push(format!(
                        "{relation_type:?} template {index} has complexity {}",
                        template.complexity
                    ));
                }
                if !template.weight.is_finite() || template.weight <= 0.0 {
                    violations.push(format!(
                        "{relation_type:?} template {index} has invalid weight {}",
                        template.weight
                    ));
                }
            }
        }
        violations
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

        let second = reg.select(
            RelationType::RelPresupposes,
            "philosophical",
            3,
            20,
            &[idx1],
        );
        assert!(second.is_some());
        let (idx2, _) = second.unwrap();
        assert_ne!(idx1, idx2);
    }

    #[test]
    fn test_select_fallback_when_all_excluded() {
        let reg = TemplateRegistry::load();
        let tmpls = reg.get(RelationType::RelPresupposes);
        let all_indices: Vec<usize> = (0..tmpls.len()).collect();
        let result = reg.select(
            RelationType::RelPresupposes,
            "philosophical",
            3,
            99,
            &all_indices,
        );
        assert!(result.is_some());
    }
}
