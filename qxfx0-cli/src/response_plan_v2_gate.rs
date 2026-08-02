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
//! `response-plan-v2-phase-b` reads the `audited-corpus` manifest and runs the
//! whole certificate chain — admission, evidence, assertion — over all 30
//! audited topics: every stated claim of every topic must land on a
//! `ClaimAuthority` (semantic + authority parity). The manifest's source
//! digests lock the gates to the exact asset bytes a release binary carries.
//!
//! `response-plan-v2-phase-c` verifies realization parity over the approved
//! V2 surfaces: a `byte` row must be reproduced byte-for-byte by the V2 clause
//! realization, a `semantic` row records that the audited surface is a
//! multi-clause rhetorical sentence approved by digest instead.
//!
//! The audited-corpus manifest and the template-agreement matrix are two
//! separate gates and are never merged.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use qxfx0_semantic::response_plan_v2::valency::{starts_with_word, valency_lexicon, Complement};
use qxfx0_semantic::response_plan_v2::{
    build_audited_topic, Clause, NounPhrase, SynTree, VerbPhrase,
};

const MATRIX_PATH: &str = "data/gates/response-plan-v2/template-agreement-matrix.json";
const MATRIX_SCHEMA_VERSION: u32 = 1;
const MATRIX_ID: &str = "template-agreement-matrix-v1";

const AUDITED_CORPUS_PATH: &str = "data/gates/response-plan-v2/audited-corpus-manifest.json";
const AUDITED_CORPUS_SCHEMA_VERSION: u32 = 1;
const AUDITED_CORPUS_ID: &str = "response-plan-v2-audited-corpus-v1";

/// Embedded so a release binary can run the gate without a working tree.
const EMBEDDED_MATRIX: &str =
    include_str!("../../data/gates/response-plan-v2/template-agreement-matrix.json");

/// Embedded so a release binary can run the gate without a working tree.
const EMBEDDED_AUDITED_CORPUS: &str =
    include_str!("../../data/gates/response-plan-v2/audited-corpus-manifest.json");

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
        GatePhase::B => run_phase_b(),
        GatePhase::C => run_phase_c(),
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

/// Path of the on-disk census artifact, for operator messages.
pub const fn audited_corpus_path() -> &'static str {
    AUDITED_CORPUS_PATH
}

#[derive(Debug, Clone, Deserialize)]
struct AuditedCorpusDiagnostics {
    topics_total: usize,
    statements_total: usize,
    parity_byte: usize,
    parity_semantic: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct AuditedCorpusRow {
    topic: String,
    predicate_id: String,
    relation_id: String,
    subject_lemma: String,
    object_lemma: String,
    statement_count: usize,
    fact_ids: Vec<String>,
    approved_surfaces: Vec<String>,
    surface_digests: Vec<String>,
    parity_class: String,
    /// Census note for operators; the gate does not enforce it.
    #[allow(dead_code)]
    reason: String,
}
#[derive(Debug, Clone, Deserialize)]
struct AuditedCorpusManifest {
    schema_version: u32,
    manifest_id: String,
    #[allow(dead_code)]
    manifest_digest: String,
    source_files: BTreeMap<String, String>,
    diagnostics: AuditedCorpusDiagnostics,
    rows: Vec<AuditedCorpusRow>,
}

/// Phase B: the audited corpus — semantic + authority parity over all 30
/// topics. Every stated claim of every topic must traverse the whole chain
/// (admission → evidence → assertion) and land on a `ClaimAuthority`; the
/// manifest must lock the exact asset bytes the release binary carries.
fn run_phase_b() -> GateReport {
    let manifest: AuditedCorpusManifest = match serde_json::from_str(EMBEDDED_AUDITED_CORPUS) {
        Ok(manifest) => manifest,
        Err(error) => {
            return GateReport::failed(
                GatePhase::B,
                vec![format!("audited-corpus manifest parse failed: {error}")],
            )
        }
    };

    let mut violations = Vec::new();
    if manifest.schema_version != AUDITED_CORPUS_SCHEMA_VERSION {
        violations.push(format!(
            "audited-corpus schema_version {} != {AUDITED_CORPUS_SCHEMA_VERSION}",
            manifest.schema_version
        ));
    }
    if manifest.manifest_id != AUDITED_CORPUS_ID {
        violations.push(format!(
            "audited-corpus manifest_id {} != {AUDITED_CORPUS_ID}",
            manifest.manifest_id
        ));
    }

    // The manifest is only authority over the assets it was generated from.
    // A drifted asset must fail the gate, not silently change what the census
    // approved.
    let mut asset_digests = BTreeMap::new();
    asset_digests.insert(
        "argued_topics.tsv".to_string(),
        qxfx0_semantic::argued_topics_source_digest(),
    );
    asset_digests.insert(
        "valency_frames.tsv".to_string(),
        valency_lexicon().fingerprint().to_string(),
    );
    for (name, digest) in qxfx0_semantic::active_pack_asset_digests() {
        asset_digests.insert(name.to_string(), digest);
    }
    for (name, recorded) in &manifest.source_files {
        match asset_digests.get(name) {
            Some(actual) if actual == recorded => {}
            Some(actual) => violations.push(format!(
                "{name} drifted from the census: manifest={recorded}, actual={actual}"
            )),
            None => violations.push(format!("manifest records an unknown asset {name}")),
        }
    }

    if manifest.diagnostics.topics_total != 30 {
        violations.push(format!(
            "audited-corpus must cover exactly 30 topics, manifest says {}",
            manifest.diagnostics.topics_total
        ));
    }
    if manifest.diagnostics.statements_total != 69 {
        violations.push(format!(
            "audited-corpus must cover exactly 69 statements, manifest says {}",
            manifest.diagnostics.statements_total
        ));
    }
    if manifest.diagnostics.parity_byte + manifest.diagnostics.parity_semantic
        != manifest.diagnostics.topics_total
    {
        violations.push("parity diagnostics must partition the topics".into());
    }

    let argued = qxfx0_semantic::argued_topic_registry().map_err(|error| error.to_string());
    let mut byte_rows = 0usize;
    let mut semantic_rows = 0usize;
    let mut claims_authorized = 0usize;

    for row in &manifest.rows {
        let argued = match &argued {
            Ok(registry) => registry,
            Err(error) => {
                violations.push(format!("argued registry unavailable: {error}"));
                break;
            }
        };
        let Some(topic) = argued.get(&row.topic) else {
            violations.push(format!("manifest topic '{}' is not audited", row.topic));
            continue;
        };
        if topic.primary_predicate_ref().as_str() != row.predicate_id {
            violations.push(format!(
                "{}: primary predicate drifted from the registry: manifest={}, registry={}",
                row.topic,
                row.predicate_id,
                topic.primary_predicate_ref().as_str()
            ));
        }
        if topic.statement_count() != row.statement_count {
            violations.push(format!(
                "{}: registry states {} statements, manifest records {}",
                row.topic,
                topic.statement_count(),
                row.statement_count
            ));
        }
        let registry_fact_ids: Vec<String> = topic
            .statements()
            .map(|statement| statement.fact_id().as_str().to_string())
            .collect();
        if registry_fact_ids != row.fact_ids {
            violations.push(format!(
                "{}: fact ids drifted from the registry: manifest={:?}, registry={:?}",
                row.topic, row.fact_ids, registry_fact_ids
            ));
        }
        let registry_surfaces: Vec<String> = topic
            .statements()
            .map(|statement| statement.surface().to_string())
            .collect();
        if registry_surfaces != row.approved_surfaces {
            violations.push(format!(
                "{}: approved surfaces drifted from the registry",
                row.topic
            ));
        }
        for (surface, recorded) in row.approved_surfaces.iter().zip(&row.surface_digests) {
            let actual = sha256_hex(surface.as_bytes());
            if actual != *recorded {
                violations.push(format!(
                    "{}: surface digest drifted: manifest={recorded}, actual={actual}",
                    row.topic
                ));
            }
        }

        // Semantic + authority parity: the whole certificate chain must
        // authorize every stated claim of the topic.
        match build_audited_topic(&row.topic) {
            Ok(plan) => {
                let claims = plan.claims();
                if claims.len() != row.statement_count {
                    violations.push(format!(
                        "{}: chain authorized {} claims for {} statements",
                        row.topic,
                        claims.len(),
                        row.statement_count
                    ));
                }
                claims_authorized += claims.len();
            }
            Err(error) => violations.push(format!("{}: {}", row.topic, error)),
        }

        match row.parity_class.as_str() {
            "byte" => byte_rows += 1,
            "semantic" => semantic_rows += 1,
            other => violations.push(format!("{}: unknown parity_class '{other}'", row.topic)),
        }
    }

    if violations.is_empty() {
        GateReport {
            gate: GatePhase::B.as_str(),
            passed: true,
            details: format!(
                "manifest={}, topics={}, statements={}, claims_authorized={}, \
                 parity byte/semantic={}/{}",
                &manifest.manifest_digest[..16],
                manifest.diagnostics.topics_total,
                manifest.diagnostics.statements_total,
                claims_authorized,
                byte_rows,
                semantic_rows,
            ),
            violations,
        }
    } else {
        GateReport::failed(GatePhase::B, violations)
    }
}

/// Phase C: realization parity over the approved V2 surfaces. A `byte` row
/// must be reproduced byte-for-byte by the V2 clause realization; a
/// `semantic` row records that the audited surface is a multi-clause
/// rhetorical sentence approved by digest, and the gate verifies the clause
/// still realizes with the lexicon-governed case.
fn run_phase_c() -> GateReport {
    let manifest: AuditedCorpusManifest = match serde_json::from_str(EMBEDDED_AUDITED_CORPUS) {
        Ok(manifest) => manifest,
        Err(error) => {
            return GateReport::failed(
                GatePhase::C,
                vec![format!("audited-corpus manifest parse failed: {error}")],
            )
        }
    };

    let mut violations = Vec::new();
    let mut byte_rows = 0usize;
    let mut byte_matches = 0usize;
    let mut semantic_rows = 0usize;

    for row in &manifest.rows {
        let plan = match build_audited_topic(&row.topic) {
            Ok(plan) => plan,
            Err(error) => {
                violations.push(format!(
                    "{}: chain failed before realization: {error}",
                    row.topic
                ));
                continue;
            }
        };
        let thesis_claim = plan
            .authorized()
            .certified()
            .candidate()
            .projected_claims()
            .remove(0);

        let frame = match valency_lexicon().get(&row.relation_id) {
            Ok(frame) => frame,
            Err(error) => {
                violations.push(format!(
                    "{}: no valency frame for relation '{}': {error}",
                    row.topic, row.relation_id
                ));
                continue;
            }
        };
        let complement = match frame.complement() {
            Complement::None => None,
            // An uninflected complement is carried verbatim: no case is
            // demanded of it.
            Complement::Uninflected => Some(NounPhrase::fixed(row.object_lemma.clone(), None)),
            governing => {
                let required = governing.required_case().expect("governing names a case");
                if row.object_lemma.contains(' ') {
                    // The corpus cannot inflect this phrase; it must already
                    // stand in the governed case. When the phrase already
                    // begins with the governed preposition, the frame's own
                    // preposition must not be emitted again.
                    let embedded = governing
                        .preposition()
                        .is_some_and(|p| starts_with_word(&row.object_lemma, p));
                    if embedded {
                        Some(NounPhrase::fixed_with_preposition(
                            row.object_lemma.clone(),
                            required,
                        ))
                    } else {
                        Some(NounPhrase::fixed(row.object_lemma.clone(), Some(required)))
                    }
                } else {
                    Some(NounPhrase::lexical(row.object_lemma.clone()))
                }
            }
        };

        let mut tree = SynTree::new();
        tree.push(
            thesis_claim.occurrence,
            Clause::new(
                NounPhrase::lexical(row.subject_lemma.clone()),
                VerbPhrase::new(row.relation_id.clone(), complement),
            ),
        );
        let resolved = match qxfx0_semantic::response_plan_v2::resolve(
            &tree,
            valency_lexicon(),
            qxfx0_morphology::get_runtime(),
        ) {
            Ok(resolved) => resolved,
            Err(error) => {
                violations.push(format!("{}: realization failed: {error}", row.topic));
                continue;
            }
        };
        let surfaces = resolved.linearize();
        let Some(surface) = surfaces.first() else {
            violations.push(format!("{}: realization produced no surface", row.topic));
            continue;
        };

        match row.parity_class.as_str() {
            "byte" => {
                byte_rows += 1;
                if *surface == row.approved_surfaces[0] {
                    byte_matches += 1;
                } else {
                    violations.push(format!(
                        "{}: byte parity violated: realized '{surface}', approved '{}'",
                        row.topic, row.approved_surfaces[0]
                    ));
                }
            }
            "semantic" => {
                semantic_rows += 1;
                let actual = sha256_hex(row.approved_surfaces[0].as_bytes());
                if actual != row.surface_digests[0] {
                    violations.push(format!(
                        "{}: approved surface digest drifted: recorded={}, actual={actual}",
                        row.topic, row.surface_digests[0]
                    ));
                }
                let governed = resolved
                    .clauses()
                    .first()
                    .expect("realization produced a clause")
                    .governed_case;
                let required = frame.complement().required_case();
                if required.is_some() && governed != required {
                    violations.push(format!(
                        "{}: realized case {governed:?} does not match lexicon {required:?}",
                        row.topic
                    ));
                }
            }
            other => violations.push(format!("{}: unknown parity_class '{other}'", row.topic)),
        }
    }

    if violations.is_empty() {
        GateReport {
            gate: GatePhase::C.as_str(),
            passed: true,
            details: format!(
                "manifest={}, byte parity {byte_matches}/{byte_rows}, \
                 semantic approved {semantic_rows}/{}",
                &manifest.manifest_digest[..16],
                manifest.diagnostics.topics_total,
            ),
            violations,
        }
    } else {
        GateReport::failed(GatePhase::C, violations)
    }
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

    /// Every declared phase must reach a verdict from real evidence.
    ///
    /// This replaces an earlier guard that asserted B and C *fail* closed while
    /// they were unimplemented. That guard existed so a release could not claim
    /// an unreached phase; now that both are implemented it would have to be
    /// deleted or inverted, and deleting it would leave nothing checking that a
    /// phase actually evaluates rather than returning a vacuous pass. So it is
    /// inverted here: each phase must pass *and* report the manifest it read.
    #[test]
    fn every_declared_phase_reaches_a_verdict_from_evidence() {
        for phase in [GatePhase::A, GatePhase::B, GatePhase::C] {
            let report = run_gate(phase);
            assert!(
                report.passed,
                "{} failed: {:?}",
                phase.as_str(),
                report.violations
            );
            assert!(
                report.details.contains("manifest") || report.details.contains("matrix"),
                "{} passed without naming the evidence it read: {}",
                phase.as_str(),
                report.details
            );
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
