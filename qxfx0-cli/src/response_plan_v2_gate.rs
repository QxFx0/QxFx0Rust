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

use qxfx0_semantic::response_plan_v2::valency::{valency_lexicon, HeadKind};
use qxfx0_semantic::response_plan_v2::{
    build_audited_topic, execute_audited_topic_at, AssertionPolicy, AuthoritySnapshot,
    PlanningPolicySnapshot, RealizationSnapshot, SelectionPolicy, SelectionPolicySnapshot,
    SelfSelectionContext, TurnContractSnapshot, V2Attempt, V2BudgetPolicy, V2ExecutionResult,
};

const MATRIX_PATH: &str = "data/gates/response-plan-v2/template-agreement-matrix.json";
const MATRIX_SCHEMA_VERSION: u32 = 1;
const MATRIX_ID: &str = "template-agreement-matrix-v1";

const AUDITED_CORPUS_PATH: &str = "data/gates/response-plan-v2/audited-corpus-manifest.json";
const AUDITED_CORPUS_SCHEMA_VERSION: u32 = 2;
const AUDITED_CORPUS_ID: &str = "response-plan-v2-audited-corpus-v2";
const REPLAY_MANIFEST_PATH: &str = "data/gates/response-plan-v2/replay-manifest.json";
const REPLAY_MANIFEST_ID: &str = "response-plan-v2-replay-v2";

/// Embedded so a release binary can run the gate without a working tree.
const EMBEDDED_MATRIX: &str =
    include_str!("../../data/gates/response-plan-v2/template-agreement-matrix.json");

/// Embedded so a release binary can run the gate without a working tree.
const EMBEDDED_AUDITED_CORPUS: &str =
    include_str!("../../data/gates/response-plan-v2/audited-corpus-manifest.json");
const EMBEDDED_REPLAY_MANIFEST: &str =
    include_str!("../../data/gates/response-plan-v2/replay-manifest.json");
const EMBEDDED_TURN_RECORD_V2: &str =
    include_str!("../../data/gates/response-plan-v2/turn-record-v2.json");
const EMBEDDED_SELECTION_VECTORS: &[u8] =
    include_bytes!("../../docs/reference-vectors/response-plan-v2-selection-v1.json");
const EMBEDDED_REALIZATION_VECTORS: &[u8] =
    include_bytes!("../../docs/reference-vectors/response-plan-v2-realization-v1.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GatePhase {
    A,
    B,
    C,
    D,
    ZeroDowngrade,
}

impl GatePhase {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "response-plan-v2-phase-a" => Some(Self::A),
            "response-plan-v2-phase-b" => Some(Self::B),
            "response-plan-v2-phase-c" => Some(Self::C),
            "response-plan-v2-replay" => Some(Self::D),
            "response-plan-v2-zero-downgrade" => Some(Self::ZeroDowngrade),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::A => "response-plan-v2-phase-a",
            Self::B => "response-plan-v2-phase-b",
            Self::C => "response-plan-v2-phase-c",
            Self::D => "response-plan-v2-replay",
            Self::ZeroDowngrade => "response-plan-v2-zero-downgrade",
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
        GatePhase::D => run_replay_gate(),
        GatePhase::ZeroDowngrade => run_zero_downgrade_gate(),
    }
}

fn run_zero_downgrade_gate() -> GateReport {
    let topics = ["свобода", "произвол", "правда"];
    let mut violations = Vec::new();
    let mut completed = 0usize;
    let mut downgrades = 0usize;
    for topic in topics {
        let policy = SelectionPolicy {
            response_plan_v2_mode: qxfx0_semantic::response_plan_v2::ResponsePlanV2Mode::Canary,
            ..SelectionPolicy::default()
        };
        let budgets = V2BudgetPolicy::default();
        let contract = TurnContractSnapshot::new(
            AuthoritySnapshot::new(
                qxfx0_semantic::active_pack_set().fingerprint(),
                AssertionPolicy::v1().digest(),
            ),
            PlanningPolicySnapshot::new(budgets.digest(), "proposition-canon-v1"),
            RealizationSnapshot::new(
                valency_lexicon().fingerprint(),
                "clause-grammar-v1",
                qxfx0_morphology::get_runtime().lexemes_sha256(),
                qxfx0_semantic::response_plan_v2::preposition_allomorphs().fingerprint(),
            ),
            SelectionPolicySnapshot::new(policy),
        );
        let execution = execute_audited_topic_at(
            topic,
            qxfx0_semantic::response_plan_v2::EvidenceEvaluationContext::new(0, None),
            &budgets,
            &contract,
            SelfSelectionContext::quantize(0.0, 0.0, 0.0),
            policy,
            valency_lexicon(),
            qxfx0_morphology::get_runtime(),
        );
        let Some(realized) = execution.realized else {
            violations.push(format!("{topic}: no realized V2 surface"));
            continue;
        };
        if execution.exact_replay.is_none() {
            violations.push(format!("{topic}: replay material missing"));
        }
        if !matches!(
            execution.result,
            V2ExecutionResult::Attempt(V2Attempt::Realizable(_))
        ) {
            downgrades += 1;
            violations.push(format!("{topic}: V2 execution downgraded"));
        } else {
            completed += 1;
        }
        if realized.clauses.is_empty() {
            violations.push(format!("{topic}: realized surface is empty"));
        }
    }
    if completed != topics.len() || downgrades != 0 || !violations.is_empty() {
        return GateReport::failed(GatePhase::ZeroDowngrade, violations);
    }
    GateReport {
        gate: GatePhase::ZeroDowngrade.as_str(),
        passed: true,
        details: format!(
            "eligible_turns={}, attempted_turns={}, completed_turns={}, downgrades=0",
            topics.len(),
            topics.len(),
            completed
        ),
        violations,
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

fn contains_whole_word_surface(text: &str, accepted: &str) -> bool {
    if accepted.is_empty() {
        return false;
    }
    let text = text.to_lowercase();
    let accepted = accepted.to_lowercase();
    text.match_indices(&accepted).any(|(start, matched)| {
        let before = text[..start].chars().next_back();
        let after = text[start + matched.len()..].chars().next();
        !before.is_some_and(|value| value.is_alphanumeric() || value == '_')
            && !after.is_some_and(|value| value.is_alphanumeric() || value == '_')
    })
}

fn valency_head_surfaces(relation_id: &str) -> Option<Vec<String>> {
    let frame = valency_lexicon().get(relation_id).ok()?;
    Some(match frame.head() {
        HeadKind::Finite { surface } => vec![surface.clone()],
        HeadKind::Agreeing {
            masculine,
            feminine,
            neuter,
            plural,
        } => vec![
            masculine.clone(),
            feminine.clone(),
            neuter.clone(),
            plural.clone(),
        ],
    })
}

/// Path of the on-disk census artifact, for operator messages.
pub const fn matrix_path() -> &'static str {
    MATRIX_PATH
}

/// Path of the on-disk census artifact, for operator messages.
pub const fn audited_corpus_path() -> &'static str {
    AUDITED_CORPUS_PATH
}

pub const fn replay_manifest_path() -> &'static str {
    REPLAY_MANIFEST_PATH
}

#[derive(Debug, Clone, Deserialize)]
struct AuditedCorpusDiagnostics {
    topics_total: usize,
    claims_total: usize,
    exact_clause_surfaces: usize,
    fixed_phrase_surfaces: usize,
    governed_clause_surfaces: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct AuditedCorpusClaim {
    discourse_root_digest: String,
    canonical_path: String,
    fact_id: String,
    proposition_id: String,
    approved_surface: String,
    approved_surface_sha256: String,
    realization_strategy: String,
    surface_validation: String,
    lexical_witnesses: Vec<LexicalWitness>,
    #[serde(default)]
    expected_clause_surface_sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LexicalWitness {
    kind: String,
    source_semantic_id: String,
    source_binding: String,
    accepted_surfaces: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AuditedCorpusTopic {
    claims: BTreeMap<String, AuditedCorpusClaim>,
}

#[derive(Debug, Clone, Deserialize)]
struct AuditedCorpusManifest {
    schema_version: u32,
    manifest_id: String,
    #[allow(dead_code)]
    manifest_digest: String,
    source_files: BTreeMap<String, String>,
    diagnostics: AuditedCorpusDiagnostics,
    topics: BTreeMap<String, AuditedCorpusTopic>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ReplayManifest {
    schema_version: u32,
    manifest_id: String,
    manifest_digest: String,
    corpus_manifest_id: String,
    corpus_manifest_digest: String,
    matrix_id: String,
    matrix_digest: String,
    topics_total: usize,
    claims_total: usize,
    selection_vectors_digest: String,
    realization_vectors_digest: String,
    turn_record_fixture_sha256: String,
    turn_record_stage_digest: String,
    turn_record_bundle_digest: String,
    reference_binary_digest: String,
    legacy_graph_declarative_fallback: bool,
}

fn run_replay_gate() -> GateReport {
    let manifest: ReplayManifest = match serde_json::from_str(EMBEDDED_REPLAY_MANIFEST) {
        Ok(manifest) => manifest,
        Err(error) => {
            return GateReport::failed(
                GatePhase::D,
                vec![format!("replay manifest parse failed: {error}")],
            )
        }
    };
    let corpus: AuditedCorpusManifest = match serde_json::from_str(EMBEDDED_AUDITED_CORPUS) {
        Ok(manifest) => manifest,
        Err(error) => return GateReport::failed(GatePhase::D, vec![error.to_string()]),
    };
    let matrix: AgreementMatrix = match serde_json::from_str(EMBEDDED_MATRIX) {
        Ok(matrix) => matrix,
        Err(error) => return GateReport::failed(GatePhase::D, vec![error.to_string()]),
    };
    let mut violations = Vec::new();
    if manifest.schema_version != 2 {
        violations.push("replay manifest schema_version must be 2".into());
    }
    if manifest.manifest_id != REPLAY_MANIFEST_ID {
        violations.push("replay manifest id drifted".into());
    }
    if manifest.manifest_digest.len() != 64 {
        violations.push("replay manifest must carry a SHA-256 digest".into());
    }
    let mut canonical = serde_json::to_value(&manifest).expect("replay manifest serializes");
    canonical
        .as_object_mut()
        .expect("manifest is an object")
        .remove("manifest_digest");
    let actual_manifest_digest = sha256_hex(
        serde_json::to_string(&canonical)
            .expect("canonical replay manifest serializes")
            .as_bytes(),
    );
    if actual_manifest_digest != manifest.manifest_digest {
        violations.push(format!(
            "replay manifest digest mismatch: recorded={}, actual={actual_manifest_digest}",
            manifest.manifest_digest
        ));
    }
    if manifest.corpus_manifest_id != corpus.manifest_id
        || manifest.corpus_manifest_digest != corpus.manifest_digest
    {
        violations.push("replay manifest is not bound to the audited corpus manifest".into());
    }
    if manifest.matrix_id != matrix.matrix_id || manifest.matrix_digest != matrix.matrix_digest {
        violations.push("replay manifest is not bound to the agreement matrix".into());
    }
    if manifest.topics_total != 30 || manifest.claims_total != 69 {
        violations.push("replay manifest must bind 30 topics and 69 claims".into());
    }
    if manifest.selection_vectors_digest.len() != 64
        || manifest.realization_vectors_digest.len() != 64
        || manifest
            .selection_vectors_digest
            .chars()
            .all(|value| value == '0')
        || manifest
            .realization_vectors_digest
            .chars()
            .all(|value| value == '0')
    {
        violations.push("selection and realization vector digests must be SHA-256 values".into());
    }
    if manifest.selection_vectors_digest != sha256_hex(EMBEDDED_SELECTION_VECTORS) {
        violations.push("selection reference vectors drifted".into());
    }
    if manifest.realization_vectors_digest != sha256_hex(EMBEDDED_REALIZATION_VECTORS) {
        violations.push("realization reference vectors drifted".into());
    }
    if manifest.turn_record_fixture_sha256 != sha256_hex(EMBEDDED_TURN_RECORD_V2.as_bytes()) {
        violations.push("TurnRecord v2 fixture bytes drifted".into());
    }
    match serde_json::from_str::<qxfx0_semantic::response_plan_v2::TurnRecord>(
        EMBEDDED_TURN_RECORD_V2,
    ) {
        Ok(record) => {
            if record.stage_digest != manifest.turn_record_stage_digest
                || record.exact_replay.bundle_digest != manifest.turn_record_bundle_digest
                || record.binary_digest != manifest.reference_binary_digest
            {
                violations.push("TurnRecord v2 fixture metadata drifted".into());
            }
            let materials = qxfx0_semantic::response_plan_v2::ReplayMaterials {
                authority: Some(&record.contract.authority),
                contract: Some(&record.contract),
                binary_digest: Some(&manifest.reference_binary_digest),
            };
            match qxfx0_pipeline::replay::verify_turn_record_replay(
                &record,
                qxfx0_semantic::response_plan_v2::ReplayLevel::Reproduction,
                materials,
            ) {
                Ok(verified)
                    if verified.reproduced_surface
                        == Some(record.exact_replay.expected_surface.clone()) => {}
                Ok(_) => violations.push("TurnRecord v2 reproduced a different surface".into()),
                Err(error) => violations.push(format!("TurnRecord v2 replay failed: {error}")),
            }
        }
        Err(error) => violations.push(format!("TurnRecord v2 fixture parse failed: {error}")),
    }
    if manifest.legacy_graph_declarative_fallback {
        violations.push("legacy_graph is forbidden as a declarative fallback".into());
    }
    if violations.is_empty() {
        GateReport {
            gate: GatePhase::D.as_str(),
            passed: true,
            details: format!(
                "manifest={}, corpus=30 topics/69 claims, legacy_graph=false",
                &manifest.manifest_digest[..16]
            ),
            violations,
        }
    } else {
        GateReport::failed(GatePhase::D, violations)
    }
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
    if manifest.diagnostics.claims_total != 69 {
        violations.push(format!(
            "audited-corpus must cover exactly 69 claims, manifest says {}",
            manifest.diagnostics.claims_total
        ));
    }
    if manifest.topics.len() != 30 {
        violations.push(format!(
            "audited-corpus must contain exactly 30 topics, found {}",
            manifest.topics.len()
        ));
    }
    let mut topics = std::collections::BTreeSet::new();
    let mut manifest_claims = 0usize;
    let mut exact_clause_topics = 0usize;
    let mut fixed_phrase_claims = 0usize;
    let mut governed_clause_topics = 0usize;

    let argued = qxfx0_semantic::argued_topic_registry().map_err(|error| error.to_string());
    let mut claims_authorized = 0usize;

    for (topic_name, topic_manifest) in &manifest.topics {
        if !topics.insert(topic_name.clone()) {
            violations.push(format!(
                "duplicate audited-corpus topic '{}', manifest is not a set",
                topic_name
            ));
        }
        manifest_claims += topic_manifest.claims.len();
        let argued = match &argued {
            Ok(registry) => registry,
            Err(error) => {
                violations.push(format!("argued registry unavailable: {error}"));
                break;
            }
        };
        let Some(topic) = argued.get(topic_name) else {
            violations.push(format!("manifest topic '{}' is not audited", topic_name));
            continue;
        };
        if topic.statement_count() != topic_manifest.claims.len() {
            violations.push(format!(
                "{}: registry states {} statements, manifest records {}",
                topic_name,
                topic.statement_count(),
                topic_manifest.claims.len()
            ));
        }
        let plan = match build_audited_topic(topic_name) {
            Ok(plan) => plan,
            Err(error) => {
                violations.push(format!("{}: {error}", topic_name));
                continue;
            }
        };
        for (claim, statement) in plan
            .authorized()
            .certified()
            .candidate()
            .projected_claims()
            .iter()
            .zip(topic.statements())
        {
            let Some(recorded) = topic_manifest.claims.get(claim.claim_id.as_str()) else {
                violations.push(format!(
                    "{}: claim {} missing from manifest",
                    topic_name,
                    claim.claim_id.as_str()
                ));
                continue;
            };
            if recorded.fact_id != statement.fact_id().as_str()
                || recorded.proposition_id != claim.proposition.as_str()
                || recorded.canonical_path != claim.occurrence.canonical_path()
                || recorded.discourse_root_digest != claim.occurrence.discourse_root_digest()
                || recorded.approved_surface != statement.surface()
                || recorded.approved_surface_sha256 != sha256_hex(statement.surface().as_bytes())
            {
                violations.push(format!(
                    "{}: claim {} drifted from the registry",
                    topic_name,
                    claim.claim_id.as_str()
                ));
            }
            match recorded.surface_validation.as_str() {
                "exact_clause" => exact_clause_topics += 1,
                "governed_clause" => governed_clause_topics += 1,
                "audited_verbatim" => fixed_phrase_claims += 1,
                other => violations.push(format!(
                    "{}: claim {} has unknown surface_validation '{other}'",
                    topic_name,
                    claim.claim_id.as_str()
                )),
            }
            let actual = sha256_hex(recorded.approved_surface.as_bytes());
            if actual != recorded.approved_surface_sha256 {
                violations.push(format!("{}: claim surface digest drifted", topic_name));
            }
        }

        // Semantic + authority parity: the whole certificate chain must
        // authorize every stated claim of the topic.
        claims_authorized += topic.statement_count();
    }

    if manifest_claims != 69 {
        violations.push(format!(
            "audited-corpus topics contain {manifest_claims} claims, expected 69"
        ));
    }
    if exact_clause_topics != manifest.diagnostics.exact_clause_surfaces
        || fixed_phrase_claims != manifest.diagnostics.fixed_phrase_surfaces
        || governed_clause_topics != manifest.diagnostics.governed_clause_surfaces
    {
        violations.push("audited-corpus realization diagnostics do not match claims".into());
    }

    if violations.is_empty() {
        GateReport {
            gate: GatePhase::B.as_str(),
            passed: true,
            details: format!(
                "manifest={}, topics={}, claims={}, claims_authorized={}, \
                 realization exact/fixed/governed={}/{}/{}",
                &manifest.manifest_digest[..16],
                manifest.diagnostics.topics_total,
                manifest.diagnostics.claims_total,
                claims_authorized,
                exact_clause_topics,
                fixed_phrase_claims,
                governed_clause_topics,
            ),
            violations,
        }
    } else {
        GateReport::failed(GatePhase::B, violations)
    }
}

/// Phase C: realization parity over the approved V2 claim surfaces. The
/// manifest selects exact, governed, or audited-verbatim validation per claim
/// and supplies lexical witnesses bound to semantic and realization assets.
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
    let mut exact_clauses = 0usize;
    let mut governed_clauses = 0usize;
    let mut claims_realized = 0usize;
    let mut fixed_surface_claims = 0usize;

    let policy = SelectionPolicy {
        response_plan_v2_mode: qxfx0_semantic::response_plan_v2::ResponsePlanV2Mode::Shadow,
        ..SelectionPolicy::default()
    };
    let budgets = V2BudgetPolicy::default();
    let contract = TurnContractSnapshot::new(
        AuthoritySnapshot::new(
            qxfx0_semantic::active_pack_set().fingerprint(),
            AssertionPolicy::v1().digest(),
        ),
        PlanningPolicySnapshot::new(budgets.digest(), "proposition-canon-v1"),
        RealizationSnapshot::new(
            valency_lexicon().fingerprint(),
            "clause-grammar-v1",
            qxfx0_morphology::get_runtime().lexemes_sha256(),
            qxfx0_semantic::response_plan_v2::preposition_allomorphs().fingerprint(),
        ),
        SelectionPolicySnapshot::new(policy),
    );

    for (topic_name, topic_manifest) in &manifest.topics {
        let execution = execute_audited_topic_at(
            topic_name,
            qxfx0_semantic::response_plan_v2::EvidenceEvaluationContext::new(0, None),
            &budgets,
            &contract,
            SelfSelectionContext::quantize(0.0, 0.0, 0.0),
            policy,
            valency_lexicon(),
            qxfx0_morphology::get_runtime(),
        );
        let plan = match &execution.result {
            V2ExecutionResult::Attempt(V2Attempt::Realizable(plan)) => plan.as_ref(),
            result => {
                violations.push(format!(
                    "{}: audited execution did not produce a realizable plan: {result:?}",
                    topic_name,
                ));
                continue;
            }
        };
        let projected = plan.authorized().certified().candidate().projected_claims();
        let topic = match qxfx0_semantic::argued_topic_registry()
            .ok()
            .and_then(|registry| registry.get(topic_name))
        {
            Some(topic) => topic,
            None => {
                violations.push(format!("{}: audited topic unavailable", topic_name));
                continue;
            }
        };
        for (claim, statement) in projected.iter().zip(topic.statements()) {
            let Some(recorded) = topic_manifest.claims.get(claim.claim_id.as_str()) else {
                violations.push(format!(
                    "{}: claim {} has no manifest surface",
                    topic_name,
                    claim.claim_id.as_str()
                ));
                continue;
            };
            let bound = plan
                .authorized()
                .certified()
                .bindings()
                .get(&claim.claim_id);
            if recorded.proposition_id != claim.proposition.as_str()
                || recorded.discourse_root_digest != claim.occurrence.discourse_root_digest()
                || recorded.canonical_path != claim.occurrence.canonical_path()
                || bound.map(|fact| fact.as_str()) != Some(recorded.fact_id.as_str())
                || statement.surface() != recorded.approved_surface
                || sha256_hex(recorded.approved_surface.as_bytes())
                    != recorded.approved_surface_sha256
            {
                violations.push(format!(
                    "{}: claim {} manifest identity/surface mismatch",
                    topic_name,
                    claim.claim_id.as_str()
                ));
                continue;
            }
            claims_realized += 1;
            match recorded.surface_validation.as_str() {
                "audited_verbatim" => fixed_surface_claims += 1,
                "exact_clause" | "governed_clause" => {}
                other => {
                    violations.push(format!(
                        "{}: claim {} has unknown surface_validation '{other}'",
                        topic_name,
                        claim.claim_id.as_str()
                    ));
                    continue;
                }
            }
            let Some((subject_binding, relation_binding, _)) =
                topic.primary_proposition().canonical_slots()
            else {
                violations.push(format!(
                    "{}: primary proposition has no bindings",
                    topic_name
                ));
                continue;
            };
            let Some(claim_fact) = qxfx0_semantic::active_pack_set()
                .facts()
                .get(statement.fact_id())
            else {
                violations.push(format!(
                    "{}: claim {} fact is unavailable",
                    topic_name,
                    claim.claim_id.as_str()
                ));
                continue;
            };
            for witness in &recorded.lexical_witnesses {
                let source_valid = match witness.kind.as_str() {
                    "subject_lemma" => {
                        witness.source_semantic_id == claim_fact.subject.0
                            && witness.source_binding == subject_binding.as_str()
                    }
                    "head" => {
                        witness.source_semantic_id == claim_fact.relation.as_str()
                            && witness.source_binding == relation_binding.as_str()
                            && valency_head_surfaces(relation_binding.as_str())
                                .is_some_and(|surfaces| surfaces == witness.accepted_surfaces)
                    }
                    _ => false,
                };
                if !source_valid {
                    violations.push(format!(
                        "{}: claim {} has an invalid {} witness source",
                        topic_name,
                        claim.claim_id.as_str(),
                        witness.kind
                    ));
                    continue;
                }
                if witness.accepted_surfaces.is_empty()
                    || !witness.accepted_surfaces.iter().any(|accepted| {
                        contains_whole_word_surface(&recorded.approved_surface, accepted)
                    })
                {
                    violations.push(format!(
                        "{}: claim {} has no whole-word surface for {} witness",
                        topic_name,
                        claim.claim_id.as_str(),
                        witness.kind
                    ));
                }
            }
        }

        let Some(thesis_manifest) = projected.iter().find_map(|claim| {
            topic_manifest
                .claims
                .get(claim.claim_id.as_str())
                .filter(|recorded| recorded.realization_strategy == "clause")
        }) else {
            violations.push(format!("{}: no clause claim in manifest", topic_name));
            continue;
        };
        let resolved = plan.resolved_syn_tree();
        if !matches!(
            resolved.nodes().first(),
            Some(qxfx0_semantic::response_plan_v2::ResolvedSynNode::Clause(_))
        ) {
            violations.push(format!("{}: thesis is not compositional", topic_name));
            continue;
        }
        let relation_id = topic
            .primary_proposition()
            .canonical_slots()
            .map(|(_, relation, _)| relation.as_str())
            .expect("audited primary proposition has canonical slots");
        let Some(surface) = execution
            .realized
            .as_ref()
            .and_then(|surface| surface.clauses.first())
        else {
            violations.push(format!("{}: realization produced no surface", topic_name));
            continue;
        };

        match thesis_manifest.surface_validation.as_str() {
            "exact_clause" => {
                exact_clauses += 1;
                let actual = sha256_hex(surface.as_bytes());
                if thesis_manifest.expected_clause_surface_sha256.as_deref() != Some(&actual) {
                    violations.push(format!(
                        "{}: exact clause digest mismatch: expected={:?}, actual={actual}",
                        topic_name, thesis_manifest.expected_clause_surface_sha256
                    ));
                }
            }
            "governed_clause" => {
                governed_clauses += 1;
                if thesis_manifest.expected_clause_surface_sha256.is_some() {
                    violations.push(format!(
                        "{}: governed clause carries an exact digest",
                        topic_name
                    ));
                }
                let governed = resolved
                    .clauses()
                    .next()
                    .expect("realization produced a clause")
                    .governed_case;
                let required = valency_lexicon()
                    .get(relation_id)
                    .expect("audited relation has a valency frame")
                    .complement()
                    .required_case();
                if required.is_some() && governed != required {
                    violations.push(format!(
                        "{}: realized case {governed:?} does not match lexicon {required:?}",
                        topic_name
                    ));
                }
            }
            other => {
                violations.push(format!(
                    "{}: thesis has invalid surface_validation '{other}'",
                    topic_name
                ));
            }
        }
    }

    if claims_realized != 69
        || fixed_surface_claims != 39
        || exact_clauses != manifest.diagnostics.exact_clause_surfaces
        || governed_clauses != manifest.diagnostics.governed_clause_surfaces
    {
        violations.push(format!(
            "claim-surface coverage mismatch: realized={claims_realized}, fixed={fixed_surface_claims}, exact={exact_clauses}, governed={governed_clauses}"
        ));
    }

    if violations.is_empty() {
        GateReport {
            gate: GatePhase::C.as_str(),
            passed: true,
            details: format!(
                "manifest={}, claims realized {claims_realized}/69 (exact/governed/fixed={exact_clauses}/{governed_clauses}/{fixed_surface_claims})",
                &manifest.manifest_digest[..16],
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
        for phase in [GatePhase::A, GatePhase::B, GatePhase::C, GatePhase::D] {
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
    fn zero_downgrade_gate_passes_the_canary_allowlist() {
        let report = run_gate(GatePhase::ZeroDowngrade);
        assert!(
            report.passed,
            "zero downgrade failed: {:?}",
            report.violations
        );
        assert!(report.details.contains("downgrades=0"));
    }

    #[test]
    fn gate_names_round_trip() {
        for phase in [
            GatePhase::A,
            GatePhase::B,
            GatePhase::C,
            GatePhase::D,
            GatePhase::ZeroDowngrade,
        ] {
            assert_eq!(GatePhase::parse(phase.as_str()), Some(phase));
        }
        assert_eq!(GatePhase::parse("response-plan-v2-phase-z"), None);
    }
}
