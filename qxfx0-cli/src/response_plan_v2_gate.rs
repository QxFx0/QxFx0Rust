//! Version-contract gates for the ResponsePlan V2 rollout (ADR-0034 §10).
//!
//! The gate is addressed by name — `doctor --gate response-plan-v2-phase-a` —
//! so the ADR references a command rather than internal Rust test names, and
//! the phases can be re-implemented without invalidating the record.
//!
//! Phase A reads the fingerprinted `template-agreement-matrix` emitted by the
//! F0 census. Byte parity is demanded only of rows whose `parity_class` is
//! `byte`; rows carrying an agreement feature are checked for semantics and
//! approved golden surfaces instead, because a principled generator may
//! legitimately produce a different — and correct — string.
//!
//! `response-plan-v2-audited-corpus` is a separate gate over the 30 audited
//! topics and is never merged with this matrix.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MATRIX_PATH: &str = "data/gates/response-plan-v2/template-agreement-matrix.json";
const MATRIX_SCHEMA_VERSION: u32 = 1;
const MATRIX_ID: &str = "template-agreement-matrix-v1";

/// Embedded so a release binary can run the gate without a working tree.
const EMBEDDED_MATRIX: &str =
    include_str!("../../data/gates/response-plan-v2/template-agreement-matrix.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GatePhase {
    A,
    B,
    C,
}

impl GatePhase {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "response-plan-v2-phase-a" => Some(Self::A),
            "response-plan-v2-phase-b" => Some(Self::B),
            "response-plan-v2-phase-c" => Some(Self::C),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::A => "response-plan-v2-phase-a",
            Self::B => "response-plan-v2-phase-b",
            Self::C => "response-plan-v2-phase-c",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MatrixRow {
    relation_type: String,
    template_index: usize,
    fixture_id: String,
    fixture_gender: String,
    fixture_lemma: String,
    parity_class: String,
    #[allow(dead_code)]
    reason: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MatrixDiagnostics {
    templates_total: usize,
    relation_types: usize,
    templates_parity_byte: usize,
    templates_parity_semantic: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct AgreementMatrix {
    schema_version: u32,
    matrix_id: String,
    matrix_digest: String,
    source_files: BTreeMap<String, String>,
    diagnostics: MatrixDiagnostics,
    rows: Vec<MatrixRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GateReport {
    pub gate: &'static str,
    pub passed: bool,
    pub details: String,
    pub violations: Vec<String>,
}

impl GateReport {
    fn failed(gate: GatePhase, violations: Vec<String>) -> Self {
        Self {
            gate: gate.as_str(),
            passed: false,
            details: format!("{} violation(s)", violations.len()),
            violations,
        }
    }
}

/// Run a named version-contract gate.
pub fn run_gate(gate: GatePhase) -> GateReport {
    match gate {
        GatePhase::A => run_phase_a(),
        // Phases B and C are declared by ADR-0034 but not yet implemented.
        // They fail closed rather than reporting a vacuous pass, so a release
        // can never claim a phase it has not reached.
        GatePhase::B | GatePhase::C => GateReport::failed(
            gate,
            vec![format!(
                "{} is declared by ADR-0034 but not implemented; \
                 V1 audited renderer remains authoritative",
                gate.as_str()
            )],
        ),
    }
}

fn run_phase_a() -> GateReport {
    let matrix: AgreementMatrix = match serde_json::from_str(EMBEDDED_MATRIX) {
        Ok(matrix) => matrix,
        Err(error) => {
            return GateReport::failed(
                GatePhase::A,
                vec![format!("agreement matrix parse failed: {error}")],
            )
        }
    };

    let mut violations = Vec::new();

    if matrix.schema_version != MATRIX_SCHEMA_VERSION {
        violations.push(format!(
            "matrix schema_version {} != {MATRIX_SCHEMA_VERSION}",
            matrix.schema_version
        ));
    }
    if matrix.matrix_id != MATRIX_ID {
        violations.push(format!("matrix_id {} != {MATRIX_ID}", matrix.matrix_id));
    }

    // The matrix is only authority over the templates it was generated from.
    // A drifted templates.json must fail the gate, not be silently accepted.
    let embedded_templates = qxfx0_semantic::TemplateRegistry::embedded_source();
    let actual_digest = sha256_hex(embedded_templates.as_bytes());
    match matrix.source_files.get("templates.json") {
        Some(recorded) if *recorded == actual_digest => {}
        Some(recorded) => violations.push(format!(
            "templates.json drifted from the census: matrix={recorded}, actual={actual_digest}"
        )),
        None => violations.push("matrix does not record a templates.json digest".into()),
    }

    let registry = qxfx0_semantic::TemplateRegistry::load();
    let mut byte_rows = 0usize;
    let mut semantic_rows = 0usize;

    for row in &matrix.rows {
        let Some(relation_type) = parse_relation_type(&row.relation_type) else {
            violations.push(format!("unknown relation type '{}'", row.relation_type));
            continue;
        };
        let templates = registry.get(relation_type);
        let Some(template) = templates.get(row.template_index) else {
            violations.push(format!(
                "{}#{} is absent from the registry",
                row.relation_type, row.template_index
            ));
            continue;
        };

        let has_agreement_slot = template.pattern.contains("_G:");
        match row.parity_class.as_str() {
            "byte" => {
                byte_rows += 1;
                // A byte-parity row promises the surface carries no agreement
                // feature. If a slot appeared, the census is stale and the
                // gate must not certify byte parity for it.
                if has_agreement_slot {
                    violations.push(format!(
                        "{}#{} is parity_class=byte but carries an agreement slot",
                        row.relation_type, row.template_index
                    ));
                }
            }
            "semantic" => {
                semantic_rows += 1;
                if !has_agreement_slot {
                    violations.push(format!(
                        "{}#{} is parity_class=semantic but carries no agreement slot",
                        row.relation_type, row.template_index
                    ));
                }
                // Every agreement slot must supply a form for this fixture's
                // gender. A missing form silently falls back to masculine,
                // which is exactly the defect that produced
                // `разум направлена на истину`.
                if let Some(missing) =
                    missing_agreement_form(&template.pattern, &row.fixture_gender)
                {
                    violations.push(format!(
                        "{}#{} has no {} form for {} ({}): {missing}",
                        row.relation_type,
                        row.template_index,
                        row.fixture_gender,
                        row.fixture_id,
                        row.fixture_lemma,
                    ));
                }
            }
            other => violations.push(format!(
                "{}#{} has unknown parity_class '{other}'",
                row.relation_type, row.template_index
            )),
        }
    }

    if violations.is_empty() {
        GateReport {
            gate: GatePhase::A.as_str(),
            passed: true,
            details: format!(
                "matrix={}, templates={} across {} relation types, \
                 parity byte/semantic={}/{}, rows byte/semantic={}/{}",
                &matrix.matrix_digest[..16],
                matrix.diagnostics.templates_total,
                matrix.diagnostics.relation_types,
                matrix.diagnostics.templates_parity_byte,
                matrix.diagnostics.templates_parity_semantic,
                byte_rows,
                semantic_rows,
            ),
            violations,
        }
    } else {
        GateReport::failed(GatePhase::A, violations)
    }
}

/// Return the offending slot when it supplies no form for `gender`.
///
/// Slot arity is positional: `{X_G:masc,fem,neut,plur}`. A slot may omit the
/// plural form, but never the three singular genders it will be asked for.
fn missing_agreement_form(pattern: &str, gender: &str) -> Option<String> {
    let needed_index = match gender {
        "m" => 0,
        "f" => 1,
        "n" => 2,
        "pl" => 3,
        _ => return None,
    };
    let mut rest = pattern;
    while let Some(start) = rest.find("_G:") {
        let after = &rest[start + 3..];
        let end = after.find('}')?;
        let forms: Vec<&str> = after[..end].split(',').collect();
        if forms.len() <= needed_index || forms[needed_index].trim().is_empty() {
            return Some(format!("{{…_G:{}}}", &after[..end]));
        }
        rest = &after[end..];
    }
    None
}

fn parse_relation_type(name: &str) -> Option<qxfx0_types::RelationType> {
    qxfx0_types::RelationType::ALL
        .iter()
        .copied()
        .find(|candidate| format!("{candidate:?}") == name)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Path of the on-disk census artifact, for operator messages.
pub const fn matrix_path() -> &'static str {
    MATRIX_PATH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_a_passes_on_the_current_census() {
        let report = run_gate(GatePhase::A);
        assert!(
            report.passed,
            "phase A must pass on the committed census: {:?}",
            report.violations
        );
    }

    /// Every template that agrees with its subject must supply all three
    /// singular genders. This is the regression lock for the live defect
    /// `разум направлена на истину`.
    #[test]
    fn every_agreement_slot_covers_three_singular_genders() {
        let registry = qxfx0_semantic::TemplateRegistry::load();
        let mut offenders = Vec::new();
        for relation_type in qxfx0_types::RelationType::ALL {
            for (index, template) in registry.get(relation_type).iter().enumerate() {
                for gender in ["m", "f", "n"] {
                    if let Some(slot) = missing_agreement_form(&template.pattern, gender) {
                        offenders.push(format!("{relation_type:?}#{index} {gender} {slot}"));
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "incomplete agreement slots: {offenders:?}"
        );
    }

    #[test]
    fn unimplemented_phases_fail_closed() {
        for phase in [GatePhase::B, GatePhase::C] {
            let report = run_gate(phase);
            assert!(!report.passed, "{} must fail closed", phase.as_str());
        }
    }

    #[test]
    fn gate_names_round_trip() {
        for phase in [GatePhase::A, GatePhase::B, GatePhase::C] {
            assert_eq!(GatePhase::parse(phase.as_str()), Some(phase));
        }
        assert_eq!(GatePhase::parse("response-plan-v2-phase-z"), None);
    }
}
