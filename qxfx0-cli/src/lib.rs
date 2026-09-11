//! QxFx0 cli crate — generated from Haskell specification.
//!
//! Exposes the same entry point used by `main.rs` so integration tests can
//! drive the turn / chat flow without spawning a subprocess.

pub mod measurement;
/// «Кодекс» and its journal-turn runtime moved to the `qxfx0-codex` crate
/// (ADR-0043 U0.6); re-exported under the historical paths.
pub use qxfx0_codex as codex;
pub use qxfx0_codex::journal::{
    fresh_state, load_or_create_state, run_journal_turn, run_journal_turn_with_essence_ablation,
    save_journal_state, stamp_practice_today, today_epoch_day,
};
/// Extracted to the `qxfx0-gates` crate (ADR-0043 U0.6); re-exported under
/// the historical module path so `main.rs`, tests and external callers are
/// unaffected.
pub use qxfx0_gates as response_plan_v2_gate;

use qxfx0_code::{build_full_registry, CodeOrchestrator};
use qxfx0_persistence::SaveStateTimings;
use qxfx0_pipeline::fact_grounded::FactGroundedRollout;
use qxfx0_pipeline::{
    process_turn_with_options, process_turn_with_options_and_timing,
    process_turn_with_options_and_trace, process_turn_with_options_timing_and_trace,
    process_turn_with_renderer_and_stance_provenance, AnomalyShadowMode, ClarificationMode,
    DoubtShadowMode, EssenceAblation, PipelineStageTimings, RendererAuthority,
    SameTopicSuppressionMode, TurnInput, TurnOptions,
};
use qxfx0_semantic::{argued_topic_registry, seed_graph};
use qxfx0_types::system_state::SystemState;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationalMetrics {
    pub doctor_healthy: bool,
    pub database_bytes: u64,
    pub doctor_duration_ms: u64,
    pub response_probe_ms: u64,
    pub response_probe_healthy: bool,
}

/// Process and host attributes attached to an opt-in diagnostic record.
///
/// The values are collected from the current process and environment only;
/// no host probes or state changes are performed.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticHostMetadata {
    /// Operating-system family compiled into this binary.
    pub os: &'static str,
    /// CPU architecture compiled into this binary.
    pub architecture: &'static str,
    /// Process identifier for correlation with host logs.
    pub process_id: u32,
    /// Logical CPU count when the platform exposes it.
    pub available_parallelism: Option<usize>,
    /// Optional host name supplied by the process environment.
    pub hostname: Option<String>,
}

/// Read-only performance evidence for one completed `turn` command.
///
/// This record is emitted only when the CLI caller opts into a diagnostics
/// JSONL file. It intentionally excludes user text and response text.
#[derive(Debug, Clone, Serialize)]
pub struct TurnDiagnostics {
    /// Stable schema identifier for JSONL consumers.
    pub schema: &'static str,
    /// Persisted turn number after the command completes.
    pub turn: usize,
    /// Renderer authority selected for the turn.
    pub renderer_authority: &'static str,
    /// Typed family selected by routing.
    pub family: String,
    /// Whether the guard blocked the response.
    pub blocked: bool,
    /// Returned response size in UTF-8 bytes, without response content.
    pub response_bytes: usize,
    /// Connection open and migration duration, filled by the CLI command.
    pub db_open_ms: u64,
    /// CLI wall-clock duration from entry into `main` to diagnostic emission.
    ///
    /// A launcher records the full process invocation separately, including
    /// startup before Rust reaches `main`.
    pub cli_process_ms: u64,
    /// State read and deserialization duration.
    pub db_load_ms: u64,
    /// Lightweight timing for the pure pipeline stages.
    pub pipeline: PipelineStageTimings,
    /// SQLite save timing, including lock/commit evidence.
    pub db_save: SaveStateTimings,
    /// Total measured duration from state load through SQLite save.
    pub total_ms: u64,
    /// Current process and host metadata for correlation.
    pub host: DiagnosticHostMetadata,
}

/// Response plus its opt-in diagnostic evidence.
#[derive(Debug, Clone)]
pub struct DiagnosedTurn {
    /// User-visible response, unchanged from the standard turn path.
    pub response: String,
    /// Timing and metadata excluded from persisted session state.
    pub diagnostics: TurnDiagnostics,
}

/// A completed normal turn plus its observation-only pipeline trace.
#[derive(Debug, Clone)]
pub struct DoubtShadowTracedTurn {
    /// User-visible response, unchanged by the trace-only feature.
    pub response: String,
    /// Deterministic execution evidence, kept external to session state.
    pub trace: qxfx0_pipeline::execution_trace::PipelineTrace,
}

#[derive(Debug, Clone)]
pub struct AuthorityTracedTurn {
    pub response: String,
    pub trace: qxfx0_pipeline::execution_trace::PipelineTrace,
}

/// Run ResponsePlan V2 as an observation-only shadow without persisting the
/// in-memory turn. V1 remains authoritative for the returned response.
pub fn run_turn_with_v2_shadow_trace(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
) -> anyhow::Result<AuthorityTracedTurn> {
    let mut state = load_or_create_state(db, session_id)?;
    let input = qxfx0_pipeline::TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let (output, trace) = qxfx0_pipeline::process_turn_with_options_and_trace(
        &input,
        &mut state,
        qxfx0_pipeline::TurnOptions::new()
            .with_response_plan_v2(qxfx0_pipeline::ResponsePlanV2Mode::Shadow),
    );
    Ok(AuthorityTracedTurn {
        response: output.response,
        trace,
    })
}

pub fn run_turn_with_v2_authority_trace(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    authority: qxfx0_pipeline::ResponsePlanV2Authority,
) -> anyhow::Result<AuthorityTracedTurn> {
    let mut state = load_or_create_state(db, session_id)?;
    let input = qxfx0_pipeline::TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let (output, trace) = qxfx0_pipeline::process_turn_with_options_and_trace(
        &input,
        &mut state,
        qxfx0_pipeline::TurnOptions::new().with_response_plan_v2_authority(authority),
    );
    save_journal_state(db, session_id, &mut state)?;
    Ok(AuthorityTracedTurn {
        response: output.response,
        trace,
    })
}

pub fn create_authority_trace_sink(path: impl AsRef<Path>) -> anyhow::Result<File> {
    create_trace_sink(path, "authority")
}

pub fn write_authority_trace_jsonl(
    sink: &mut File,
    trace: &qxfx0_pipeline::execution_trace::PipelineTrace,
) -> anyhow::Result<()> {
    write_trace_jsonl(sink, "qxfx0.authority-trace.v1", trace)
}

pub fn create_response_plan_v2_shadow_trace_sink(path: impl AsRef<Path>) -> anyhow::Result<File> {
    create_trace_sink(path, "response plan V2 shadow")
}

pub fn write_response_plan_v2_shadow_trace_jsonl(
    sink: &mut File,
    trace: &qxfx0_pipeline::execution_trace::PipelineTrace,
) -> anyhow::Result<()> {
    write_trace_jsonl(sink, "qxfx0.response-plan-v2-shadow-trace.v1", trace)
}

#[derive(Debug, Serialize)]
struct TraceRecord<'a> {
    schema: &'a str,
    trace: &'a qxfx0_pipeline::execution_trace::PipelineTrace,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedAuthorityTraceRecord {
    schema: String,
    trace: OwnedAuthorityTrace,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedAuthorityTrace {
    #[serde(rename = "request_id")]
    _request_id: String,
    steps: Vec<OwnedTraceStep>,
    authority_receipt: Option<serde_json::Value>,
    authority_guard_classification: Option<String>,
    authority_case_id: Option<String>,
    authority_input_class: Option<String>,
    authority_expected_result: Option<String>,
    authority_expected_guard: Option<String>,
    /// Observational evidence the serialized trace carries (skip-when-none at
    /// the source). The verifier does not interpret either: the thesis
    /// receipt appears on canary turns that also observe thesis projection,
    /// and `essence_advance` is the ADR-0043 U2 shadow summary present on
    /// every successful turn. Accepted so the strict external schema stays
    /// aligned with what the writer emits.
    #[serde(rename = "thesis_observation_receipt")]
    _thesis_observation_receipt: Option<serde_json::Value>,
    #[serde(rename = "essence_advance")]
    _essence_advance: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedTraceStep {
    stage: String,
    input_digest: String,
    output_digest: String,
    metadata: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AuthorityReport {
    pub turns: usize,
    pub compositional: usize,
    pub audited_verbatim: usize,
    pub typed_non_declarative: usize,
    pub realization_downgrade: usize,
    pub replay_failures: usize,
    pub guard_blocks: usize,
    pub rollback_activations: usize,
    pub positive_turns: usize,
    pub negative_turns: usize,
    pub expectation_failures: usize,
    pub expected_denials: usize,
    pub unexpected_denials: usize,
    pub expected_rollbacks: usize,
    pub unexpected_rollbacks: usize,
    pub case_ids: Vec<String>,
    pub input_classes: std::collections::BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityReportScope {
    All,
    Positive,
    Negative,
}

const MAX_AUTHORITY_TRACE_LINE_BYTES: usize = 8 * 1024 * 1024;
const MAX_AUTHORITY_TRACE_RECORDS: usize = 100_000;
const MAX_AUTHORITY_TRACE_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

pub fn verify_authority_trace(path: impl AsRef<Path>) -> anyhow::Result<AuthorityReport> {
    authority_report([path.as_ref()], true, AuthorityReportScope::All)
}

pub fn authority_report<I, P>(
    paths: I,
    fail_closed: bool,
    scope: AuthorityReportScope,
) -> anyhow::Result<AuthorityReport>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut report = AuthorityReport::default();
    let mut artifact_count = 0;
    let mut record_count = 0usize;
    let mut total_bytes = 0u64;
    let mut case_ids = std::collections::BTreeSet::new();
    for path in paths {
        artifact_count += 1;
        let path = path.as_ref();
        let file = File::open(path).map_err(|error| {
            anyhow::anyhow!(
                "failed to open authority trace '{}': {error}",
                path.display()
            )
        })?;
        let mut reader = BufReader::new(file);
        let mut line = Vec::new();
        let mut index = 0usize;
        loop {
            index += 1;
            line.clear();
            let mut ended_with_newline = false;
            loop {
                let available = reader.fill_buf().map_err(|error| {
                    anyhow::anyhow!(
                        "failed to read authority trace '{}' at line {index}: {error}",
                        path.display()
                    )
                })?;
                if available.is_empty() {
                    break;
                }
                let take = available
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(available.len(), |position| position + 1);
                if line.len().saturating_add(take) > MAX_AUTHORITY_TRACE_LINE_BYTES {
                    anyhow::bail!(
                        "authority trace '{}' line {index} exceeds the {} byte limit",
                        path.display(),
                        MAX_AUTHORITY_TRACE_LINE_BYTES
                    );
                }
                line.extend_from_slice(&available[..take]);
                reader.consume(take);
                if line.last() == Some(&b'\n') {
                    ended_with_newline = true;
                    break;
                }
            }
            if line.is_empty() && !ended_with_newline {
                break;
            }
            total_bytes = total_bytes.saturating_add(line.len() as u64);
            if total_bytes > MAX_AUTHORITY_TRACE_TOTAL_BYTES {
                anyhow::bail!(
                    "authority traces exceed the {} byte aggregate limit",
                    MAX_AUTHORITY_TRACE_TOTAL_BYTES
                );
            }
            if line.last() == Some(&b'\n') {
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
            }
            let line = std::str::from_utf8(&line).map_err(|error| {
                anyhow::anyhow!(
                    "authority trace '{}' line {index} is not UTF-8: {error}",
                    path.display()
                )
            })?;
            if line.trim().is_empty() {
                continue;
            }
            record_count = record_count.saturating_add(1);
            if record_count > MAX_AUTHORITY_TRACE_RECORDS {
                anyhow::bail!(
                    "authority traces exceed the {} record aggregate limit",
                    MAX_AUTHORITY_TRACE_RECORDS
                );
            }
            let record: OwnedAuthorityTraceRecord =
                serde_json::from_str(line).map_err(|error| {
                    anyhow::anyhow!("authority trace '{}' line {index}: {error}", path.display())
                })?;
            for step in &record.trace.steps {
                if !valid_digest(&step.input_digest) || !valid_digest(&step.output_digest) {
                    anyhow::bail!(
                        "authority trace line {} has an invalid stage digest",
                        index + 1
                    );
                }
            }
            if record.trace.authority_guard_classification.is_none()
                && record.trace.authority_receipt.is_none()
            {
                anyhow::bail!(
                    "authority trace line {} has no authority evidence",
                    index + 1
                );
            }
            if record.schema != "qxfx0.authority-trace.v1" {
                anyhow::bail!(
                    "authority trace line {} has schema '{}'",
                    index + 1,
                    record.schema
                );
            }
            let guard = record
                .trace
                .authority_guard_classification
                .as_deref()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "authority trace line {} has no guard classification",
                        index + 1
                    )
                })?;
            let expected_result = record.trace.authority_expected_result.as_deref();
            let positive = matches!(expected_result, Some("compositional" | "audited_verbatim"));
            let negative = expected_result.is_some() && !positive;
            if (scope == AuthorityReportScope::Positive && !positive)
                || (scope == AuthorityReportScope::Negative && !negative)
            {
                continue;
            }
            if expected_result.is_some() || record.trace.authority_expected_guard.is_some() {
                let case_id = record.trace.authority_case_id.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("authority trace line {} has no case_id", index + 1)
                })?;
                let input_class =
                    record
                        .trace
                        .authority_input_class
                        .as_deref()
                        .ok_or_else(|| {
                            anyhow::anyhow!("authority trace line {} has no input_class", index + 1)
                        })?;
                if !case_ids.insert(case_id.to_owned()) {
                    anyhow::bail!("authority trace has duplicate case_id '{case_id}'");
                }
                *report
                    .input_classes
                    .entry(input_class.to_owned())
                    .or_default() += 1;
            }
            if positive {
                report.positive_turns += 1;
            } else if negative {
                report.negative_turns += 1;
            }
            if record
                .trace
                .authority_expected_guard
                .as_deref()
                .is_some_and(|expected| expected != guard)
            {
                report.expectation_failures += 1;
            }
            let Some(receipt) = record
                .trace
                .authority_receipt
                .as_ref()
                .and_then(serde_json::Value::as_object)
            else {
                report.turns += 1;
                if guard == "authority_denied_before_render" {
                    if expected_result == Some("authority_denied") {
                        report.expected_denials += 1;
                        report.expected_rollbacks += 1;
                    } else {
                        report.unexpected_denials += 1;
                        report.unexpected_rollbacks += 1;
                        report.expectation_failures += 1;
                    }
                    report.rollback_activations += 1;
                    if fail_closed {
                        anyhow::bail!("authority trace line {} is not release-eligible", index + 1);
                    }
                    continue;
                }
                anyhow::bail!("authority trace line {} has no receipt", index + 1);
            };
            let string = |field: &str| {
                receipt
                    .get(field)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        anyhow::anyhow!("authority trace line {} missing {field}", index + 1)
                    })
            };
            for digest in [
                "artifact_digest",
                "contract_digest",
                "output_digest",
                "replay_bundle_digest",
            ] {
                let value = string(digest)?;
                if !valid_digest(value) {
                    anyhow::bail!("authority trace line {} has invalid {digest}", index + 1);
                }
            }
            let topic = string("topic")?;
            let requested_mode = string("requested_mode")?;
            let effective_mode = string("effective_mode")?;
            let authority = string("authority")?;
            let receipt_guard = string("guard_classification")?;
            if receipt_guard != guard {
                anyhow::bail!(
                    "authority trace line {} has conflicting guard classifications",
                    index + 1
                );
            }
            let outcome = receipt
                .get("outcome")
                .and_then(serde_json::Value::as_object)
                .and_then(|outcome| outcome.keys().next())
                .map(String::as_str)
                .ok_or_else(|| {
                    anyhow::anyhow!("authority trace line {} has invalid outcome", index + 1)
                })?;
            if expected_result.is_some_and(|expected| expected != outcome) {
                report.expectation_failures += 1;
            }
            report.turns += 1;
            match outcome {
                "compositional" => report.compositional += 1,
                "audited_verbatim" => report.audited_verbatim += 1,
                "typed_non_declarative" => report.typed_non_declarative += 1,
                "realization_downgrade" => report.realization_downgrade += 1,
                other => anyhow::bail!(
                    "authority trace line {} has unknown outcome '{other}'",
                    index + 1
                ),
            }
            if guard == "v2_rendered_guard_blocked" {
                report.guard_blocks += 1;
            }
            if authority.eq_ignore_ascii_case("disabled")
                || guard == "authority_denied_before_render"
            {
                report.rollback_activations += 1;
                if expected_result == Some("authority_denied") {
                    report.expected_rollbacks += 1;
                } else {
                    report.unexpected_rollbacks += 1;
                }
            }
            let replay_ok = record.trace.steps.iter().any(|step| {
                step.stage == "response_plan_v2"
                    && step.metadata.get("replay_parity").map(String::as_str) == Some("true")
            });
            if !replay_ok {
                report.replay_failures += 1;
            }
            let v2_step = record
                .trace
                .steps
                .iter()
                .find(|step| step.stage == "response_plan_v2")
                .ok_or_else(|| {
                    anyhow::anyhow!("authority trace line {} has no V2 step", index + 1)
                })?;
            let metadata_equals = |field: &str, expected: &str| {
                v2_step.metadata.get(field).map(String::as_str) == Some(expected)
            };
            let output = receipt
                .get("outcome")
                .and_then(|value| value.get(outcome))
                .and_then(|value| value.get("output"));
            let output_digest_matches = output
                .and_then(|value| value.get("surface_digest"))
                .and_then(serde_json::Value::as_str)
                == Some(string("output_digest")?);
            let digests_match = v2_step.output_digest == string("artifact_digest")?
                && metadata_equals("contract_digest", string("contract_digest")?)
                && metadata_equals("authority_surface_digest", string("output_digest")?)
                && metadata_equals("replay_bundle_digest", string("replay_bundle_digest")?)
                && output_digest_matches;
            let eligible = qxfx0_pipeline::response_plan_v2_canary_allowlist().contains(&topic);
            if fail_closed
                && (!authority.eq_ignore_ascii_case("canary")
                    || !eligible
                    || requested_mode != "canary"
                    || effective_mode != "canary"
                    || !matches!(outcome, "compositional" | "audited_verbatim")
                    || guard != "v2_successfully_emitted"
                    || !replay_ok
                    || !digests_match
                    || v2_step.metadata.get("downgrade_count").map(String::as_str) != Some("0")
                    || v2_step.metadata.get("v1_fallback_used").map(String::as_str)
                        != Some("false"))
            {
                anyhow::bail!("authority trace line {} is not release-eligible", index + 1);
            }
        }
    }
    if artifact_count == 0 || report.turns == 0 {
        anyhow::bail!("authority trace contains no records");
    }
    report.case_ids = case_ids.into_iter().collect();
    Ok(report)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn diagnostic_host_metadata() -> DiagnosticHostMetadata {
    DiagnosticHostMetadata {
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        process_id: std::process::id(),
        available_parallelism: std::thread::available_parallelism().ok().map(usize::from),
        hostname: std::env::var("HOSTNAME")
            .ok()
            .filter(|value| !value.is_empty()),
    }
}

/// Append one JSONL performance record without changing the session database.
pub fn append_turn_diagnostics(
    path: impl AsRef<Path>,
    diagnostics: &TurnDiagnostics,
) -> anyhow::Result<()> {
    let mut record = serde_json::to_vec(diagnostics)?;
    record.push(b'\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.as_ref())?;
    file.write_all(&record)?;
    Ok(())
}

/// Create a new external JSONL sink for doubt shadow evidence.
///
/// Existing files are rejected. This makes the opt-in artifact explicit and
/// prevents a command from silently appending to a completed pilot trace.
pub fn create_doubt_shadow_trace_sink(path: impl AsRef<Path>) -> anyhow::Result<File> {
    create_trace_sink(path, "doubt shadow")
}

fn create_trace_sink(path: impl AsRef<Path>, trace_name: &str) -> anyhow::Result<File> {
    let path = path.as_ref();
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            anyhow::anyhow!(
                "{trace_name} trace sink must be a new file ({}): {error}",
                path.display()
            )
        })
}

/// Append one deterministic doubt-shadow record to a sink created for this
/// command. The serialized trace deliberately excludes wall-clock durations.
pub fn write_doubt_shadow_trace_jsonl(
    sink: &mut File,
    trace: &qxfx0_pipeline::execution_trace::PipelineTrace,
) -> anyhow::Result<()> {
    write_trace_jsonl(sink, "qxfx0.doubt-shadow-trace.v1", trace)
}

fn write_trace_jsonl(
    sink: &mut File,
    schema: &str,
    trace: &qxfx0_pipeline::execution_trace::PipelineTrace,
) -> anyhow::Result<()> {
    let mut record = serde_json::to_vec(&TraceRecord { schema, trace })?;
    record.push(b'\n');
    sink.write_all(&record)?;
    Ok(())
}

pub fn create_cognitive_pilot_trace_sink(path: impl AsRef<Path>) -> anyhow::Result<File> {
    create_trace_sink(path, "cognitive pilot")
}

pub fn write_cognitive_pilot_trace_jsonl(
    sink: &mut File,
    trace: &qxfx0_pipeline::execution_trace::PipelineTrace,
) -> anyhow::Result<()> {
    write_trace_jsonl(sink, "qxfx0.cognitive-pilot-trace.v1", trace)
}

/// Create a new external JSONL sink for anomaly shadow evidence.
pub fn create_anomaly_shadow_trace_sink(path: impl AsRef<Path>) -> anyhow::Result<File> {
    create_trace_sink(path, "anomaly shadow")
}

/// Append one deterministic anomaly-shadow record to a sink created for this
/// command. The trace is observational and excludes wall-clock durations.
pub fn write_anomaly_shadow_trace_jsonl(
    sink: &mut File,
    trace: &qxfx0_pipeline::execution_trace::PipelineTrace,
) -> anyhow::Result<()> {
    write_trace_jsonl(sink, "qxfx0.anomaly-shadow-trace.v1", trace)
}

impl OperationalMetrics {
    pub fn threshold_violations(
        &self,
        max_database_bytes: u64,
        max_response_ms: u64,
    ) -> Vec<String> {
        let mut violations = Vec::new();
        if !self.doctor_healthy {
            violations.push("doctor reported an unhealthy subsystem".into());
        }
        if self.database_bytes > max_database_bytes {
            violations.push(format!(
                "database storage is {} bytes, limit is {} bytes",
                self.database_bytes, max_database_bytes
            ));
        }
        if !self.response_probe_healthy {
            violations.push("response probe returned an invalid result".into());
        }
        if self.response_probe_ms > max_response_ms {
            violations.push(format!(
                "response probe took {} ms, limit is {} ms",
                self.response_probe_ms, max_response_ms
            ));
        }
        violations
    }

    pub fn to_prometheus(&self) -> String {
        format!(
            concat!(
                "# TYPE qxfx0_doctor_healthy gauge\n",
                "qxfx0_doctor_healthy {}\n",
                "# TYPE qxfx0_database_bytes gauge\n",
                "qxfx0_database_bytes {}\n",
                "# TYPE qxfx0_doctor_duration_seconds gauge\n",
                "qxfx0_doctor_duration_seconds {:.6}\n",
                "# TYPE qxfx0_response_probe_duration_seconds gauge\n",
                "qxfx0_response_probe_duration_seconds {:.6}\n",
                "# TYPE qxfx0_response_probe_healthy gauge\n",
                "qxfx0_response_probe_healthy {}\n"
            ),
            u8::from(self.doctor_healthy),
            self.database_bytes,
            self.doctor_duration_ms as f64 / 1_000.0,
            self.response_probe_ms as f64 / 1_000.0,
            u8::from(self.response_probe_healthy),
        )
    }
}

fn elapsed_millis(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn database_storage_bytes(db_path: &str) -> u64 {
    [
        db_path.to_string(),
        format!("{db_path}-wal"),
        format!("{db_path}-shm"),
    ]
    .iter()
    .filter_map(|path| std::fs::metadata(path).ok())
    .map(|metadata| metadata.len())
    .sum()
}

/// Collect machine-readable health, storage and synthetic response metrics.
/// The response probe runs entirely in memory and never changes the monitored
/// database.
pub fn run_operational_metrics(db_path: &str) -> OperationalMetrics {
    let doctor_started = Instant::now();
    let doctor_report = run_doctor(db_path);
    let doctor_duration_ms = elapsed_millis(doctor_started);

    let mut probe_state = fresh_state("__operational_probe__");
    let probe_input = TurnInput {
        raw_text: "что такое свобода?".into(),
        session_id: probe_state.session_id.clone(),
    };
    let response_started = Instant::now();
    let probe_output =
        process_turn_with_options(&probe_input, &mut probe_state, TurnOptions::new());
    let response_probe_ms = elapsed_millis(response_started);
    let response_probe_healthy =
        !probe_output.response.trim().is_empty() && probe_state.validate().is_empty();

    OperationalMetrics {
        doctor_healthy: doctor_report.is_healthy(),
        database_bytes: database_storage_bytes(db_path),
        doctor_duration_ms,
        response_probe_ms,
        response_probe_healthy,
    }
}

impl DoctorReport {
    pub fn is_healthy(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }
}

/// One session's between-turn bridge maintenance result (ADR-0043 U4): the
/// decay/retire/prune pass the runtime-edge store survived. Retained edges
/// are those the ladder kept after unused-confidence decay and the
/// per-session cap; retired edges are the difference. Sessions with no
/// stored bridge row never appear here — the bridge stayed asleep for them.
#[derive(Debug, Clone, Serialize)]
pub struct BridgeSessionMaintenance {
    pub session_id: String,
    pub edges_before: usize,
    pub edges_after: usize,
    pub runtime_edges_before: usize,
    pub runtime_edges_after: usize,
    pub quarantined: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BridgeMaintenanceReport {
    pub sessions: Vec<BridgeSessionMaintenance>,
    pub sessions_touched: usize,
    pub total_edges_before: usize,
    pub total_edges_after: usize,
    pub total_runtime_edges_after: usize,
}

/// Run the between-turn bridge maintenance pass over every session that
/// carries a stored runtime-edge store (ADR-0043 U4). This is the decay /
/// retire / prune half of the worker cycle — the queue is intentionally
/// empty here: candidate corroboration is enqueued by the long-lived serve
/// worker between turns, while this CLI maintenance command is the
/// scheduler-driven lifecycle that fades edges nothing has re-corroborated
/// and bounds the store. The turn path is never involved; the bridge sleeps
/// for every session whose store is absent. Idempotent for a session whose
/// topic touches every edge (nothing to decay).
pub fn run_bridge_maintenance(db_path: &str) -> anyhow::Result<BridgeMaintenanceReport> {
    let db = qxfx0_persistence::Persistence::open(db_path)?;
    let config = qxfx0_bridge::DecayConfig::default();
    let mut report = BridgeMaintenanceReport::default();
    for session_id in db.list_sessions()? {
        let Some(edges_json) = db.load_bridge_edges(&session_id)? else {
            continue; // the bridge never touched this session: it sleeps.
        };
        let store = qxfx0_bridge::decode_store(&edges_json).map_err(|error| {
            anyhow::anyhow!("session {session_id}: corrupt bridge store: {error}")
        })?;
        let runtime_edges_before = qxfx0_bridge::runtime_edge_count(&store);
        let edges_before = store.len();
        // Decay needs the session's current topic (topic-touching edges are
        // spared) and turn ordinal for the trace; admission is unused for an
        // empty queue but the graph atoms are threaded so a future caller
        // that enqueues candidates here stays correct.
        let (turn, topic, known_atoms) = match db.load_state(&session_id)? {
            Some(state) => {
                let mut atoms: std::collections::BTreeSet<qxfx0_types::atom::AtomId> =
                    state.semantic.runtime_graph.atoms.keys().cloned().collect();
                if let Some(topic) = state.dialogue.last_topic.clone() {
                    atoms.insert(qxfx0_types::atom::AtomId::new(topic));
                }
                (
                    state.dialogue.turn_count as u64,
                    state.dialogue.last_topic.clone().unwrap_or_default(),
                    atoms,
                )
            }
            // A stored edge row with no state row: decay against an empty
            // graph so only the cap/retire rules can act (fail toward
            // retention of topic-untouched evidence, never fabrication).
            None => (0, String::new(), std::collections::BTreeSet::new()),
        };
        let queue = qxfx0_bridge::BoundedCorroborationQueue::default();
        let (decayed, worker_report, _quarantined) =
            qxfx0_bridge::process_turn_boundary(store, queue, turn, &topic, &known_atoms, &config);
        let new_json = qxfx0_bridge::encode_store(&decayed).map_err(|error| {
            anyhow::anyhow!("session {session_id}: bridge store encode: {error}")
        })?;
        // An emptied-to-nothing store clears the row so a drained session is
        // indistinguishable from a never-touched one.
        db.save_bridge_edges(
            &session_id,
            if decayed.is_empty() {
                None
            } else {
                Some(&new_json)
            },
        )?;
        let maintenance = BridgeSessionMaintenance {
            session_id,
            edges_before,
            edges_after: decayed.len(),
            runtime_edges_before,
            runtime_edges_after: worker_report.runtime_edges_after,
            quarantined: worker_report.quarantined.len(),
        };
        report.total_edges_before += maintenance.edges_before;
        report.total_edges_after += maintenance.edges_after;
        report.total_runtime_edges_after += maintenance.runtime_edges_after;
        report.sessions_touched += 1;
        report.sessions.push(maintenance);
    }
    Ok(report)
}

/// The current Unix seconds, never sampled on the turn path — only by CLI
/// commands that stamp a lifecycle row at explicit operator request.
pub fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn promotion_status_name(status: qxfx0_bridge::OverlayStatus) -> &'static str {
    match status {
        qxfx0_bridge::OverlayStatus::Draft => "Draft",
        qxfx0_bridge::OverlayStatus::Activated => "Activated",
        qxfx0_bridge::OverlayStatus::Released => "Released",
    }
}

/// Render a bridge triple as a canonical Russian surface. The candidate
/// keeps only the words involved — inflected rendering is the editorial
/// admission step, not the boundary's.
fn render_bridge_triple(
    subject: &qxfx0_types::atom::AtomId,
    relation: qxfx0_types::RelationType,
    object: &qxfx0_types::atom::AtomId,
) -> String {
    format!(
        "{} {} {}",
        subject.as_str(),
        relation.verb_ru(),
        object.as_str()
    )
}

/// The CLI promotion surface (ADR-0043 U5): the thin layer that enumerates
/// candidates from every session's Promoted bridge tier, gates them against
/// the curated argued-corpus baseline (the same informativeness bar an
/// editor passes), and drives the pure lifecycle machine in
/// `qxfx0_bridge::promotion` through the v14 store. Release is permanent:
/// a Released overlay's row and content address are frozen, and only the
/// active pointer moves on rollback.
pub struct PromotionSurface;

/// The outcome of one draft: the materialized overlay, the runtime
/// exclusions, and the import-quarantine refusals (empty without an
/// import file), so one report shows the operator every refusal.
pub struct DraftReport {
    pub overlay: qxfx0_bridge::Overlay,
    pub exclusions: Vec<(
        qxfx0_bridge::PromotionCandidate,
        qxfx0_bridge::ExclusionReason,
    )>,
    pub import_refusals: Vec<(String, qxfx0_bridge::ExclusionReason)>,
    pub imported: usize,
}

impl PromotionSurface {
    /// The gate policy this build pins, versioned and checksummed so a
    /// re-draft under an older policy stays auditable.
    pub fn policy() -> qxfx0_bridge::GatePolicy {
        qxfx0_bridge::builtin_gate_policy()
    }

    /// A snapshot identifier derived from the promoted content itself, never
    /// the clock: the same evidence scans to the same id, a changed set to a
    /// new one. This pins candidate identity without leaking time into a
    /// deterministic store.
    fn snapshot_id(db: &qxfx0_persistence::Persistence) -> anyhow::Result<String> {
        use qxfx0_bridge::{BridgeEdgeSource, RuntimeEdgeStore};
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        for session_id in db.list_sessions()? {
            let Some(json) = db.load_bridge_edges(&session_id)? else {
                continue;
            };
            let store: RuntimeEdgeStore = qxfx0_bridge::decode_store(&json)
                .map_err(|error| anyhow::anyhow!("session {session_id}: bridge store: {error}"))?;
            for edge in store.values() {
                if edge.source != BridgeEdgeSource::Promoted {
                    continue;
                }
                hasher.update(session_id.as_bytes());
                hasher.update([1]);
                hasher.update(edge.from.as_str().as_bytes());
                hasher.update([1]);
                hasher.update(edge.to.as_str().as_bytes());
                hasher.update([1]);
                hasher.update(qxfx0_bridge::promotion::canonical_slug(edge.rel_type).as_bytes());
                hasher.update([0]);
            }
        }
        Ok(format!("snapshot-{:x}", hasher.finalize()))
    }

    /// Enumerate candidate triples from the Promoted tier across every
    /// session, deduplicated by canonical triple (the snapshot id is part of
    /// the candidate identity, so a retry over unchanged evidence is the
    /// same candidate set — idempotent drafts). The caller passes the
    /// snapshot id so candidate identity and the overlay's own id are always
    /// computed from the same scan.
    pub fn enumerate_candidates(
        db: &qxfx0_persistence::Persistence,
        snapshot_id: &str,
    ) -> anyhow::Result<Vec<qxfx0_bridge::PromotionCandidate>> {
        use qxfx0_bridge::{BridgeEdgeSource, PromotionCandidate, RuntimeEdgeStore};
        let mut seen: std::collections::BTreeMap<(String, String, String), PromotionCandidate> =
            std::collections::BTreeMap::new();
        for session_id in db.list_sessions()? {
            let Some(json) = db.load_bridge_edges(&session_id)? else {
                continue;
            };
            let store: RuntimeEdgeStore = qxfx0_bridge::decode_store(&json)
                .map_err(|error| anyhow::anyhow!("session {session_id}: bridge store: {error}"))?;
            for edge in store.values() {
                if edge.source != BridgeEdgeSource::Promoted {
                    continue;
                }
                let key = (
                    edge.from.as_str().to_string(),
                    qxfx0_bridge::promotion::canonical_slug(edge.rel_type),
                    edge.to.as_str().to_string(),
                );
                seen.entry(key).or_insert_with(|| PromotionCandidate {
                    snapshot_id: snapshot_id.to_string(),
                    topic: edge.topic.clone(),
                    subject: edge.from.clone(),
                    relation: edge.rel_type,
                    object: edge.to.clone(),
                    rendered_ru: render_bridge_triple(&edge.from, edge.rel_type, &edge.to),
                    confidence: edge.confidence,
                    support: edge.co_occurrence,
                });
            }
        }
        Ok(seen.into_values().collect())
    }

    /// The topic's curated baseline from the argued corpus — the thesis,
    /// counterpoint and consequence surfaces the system already renders.
    /// An empty baseline means the topic has no audited content yet, so
    /// novelty is judged against nothing (the Haskell empty-corpus case).
    fn baseline_for_topic(topic: &str) -> Vec<String> {
        match qxfx0_semantic::argued_topic_registry() {
            Ok(registry) => registry
                .get(topic)
                .map(|argued| {
                    let mut surfaces = vec![
                        argued.thesis().surface().to_string(),
                        argued.counterpoint().surface().to_string(),
                    ];
                    if let Some(consequence) = argued.consequence() {
                        surfaces.push(consequence.surface().to_string());
                    }
                    surfaces
                })
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// The union of every session's runtime-graph atoms known to this
    /// database — the seed-atom bar's universe (no minting). Deterministic:
    /// a BTreeSet in atom order.
    fn known_atoms(
        db: &qxfx0_persistence::Persistence,
    ) -> anyhow::Result<std::collections::BTreeSet<qxfx0_types::AtomId>> {
        let mut known = std::collections::BTreeSet::new();
        for session_id in db.list_sessions()? {
            if let Some(state) = db.load_state(&session_id)? {
                known.extend(state.semantic.runtime_graph.atoms.keys().cloned());
            }
        }
        Ok(known)
    }

    /// The topic admission oracle: a topic clears the counterpoint bar iff
    /// the argued registry carries a curated counterpoint surface for it.
    fn topic_admission_for(topic: &str) -> qxfx0_bridge::TopicAdmissionFacts {
        let has_counterpoint = qxfx0_semantic::argued_topic_registry()
            .ok()
            .and_then(|registry| registry.get(topic))
            .is_some_and(|argued| !argued.counterpoint().surface().trim().is_empty());
        qxfx0_bridge::TopicAdmissionFacts { has_counterpoint }
    }

    /// The `relates_to` oracle for the corpus precheck: a canonical triple
    /// collides with curated authority iff the seed graph — the static,
    /// embedded authority — already carries that exact
    /// (subject, relation, object). Deterministic: the seed graph never
    /// moves inside a process.
    fn curated_triple_set() -> std::collections::BTreeSet<(String, String, String)> {
        qxfx0_semantic::seed_graph()
            .edges
            .iter()
            .map(|relation| {
                (
                    relation.from.as_str().to_string(),
                    qxfx0_bridge::promotion::canonical_slug(relation.rel_type),
                    relation.to.as_str().to_string(),
                )
            })
            .collect()
    }

    /// Snapshot id blended over the runtime Promoted tier *and* an optional
    /// import-quarantine file's verbatim bytes: mixed evidence re-drafts to
    /// the same version, either input changing moves it.
    fn snapshot_id_mixed(
        db: &qxfx0_persistence::Persistence,
        import_bytes: Option<&[u8]>,
    ) -> anyhow::Result<String> {
        let runtime = Self::snapshot_id(db)?;
        if import_bytes.is_none() {
            return Ok(runtime);
        }
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(
            runtime
                .strip_prefix("snapshot-")
                .unwrap_or(&runtime)
                .as_bytes(),
        );
        hasher.update([2]);
        hasher.update(import_bytes.expect("checked"));
        hasher.update([2]);
        Ok(format!("snapshot-import-{:x}", hasher.finalize()))
    }

    /// Draft a new overlay from the currently-promoted evidence. Idempotent:
    /// the same evidence yields the same content-addressed version and the
    /// store insert is a no-op. Returns the overlay (its `predicates` may be
    /// empty) plus the exclusion trace, so an all-refused review still shows
    /// the operator why nothing was admitted.
    pub fn draft(
        db: &qxfx0_persistence::Persistence,
        now: i64,
    ) -> anyhow::Result<(
        qxfx0_bridge::Overlay,
        Vec<(
            qxfx0_bridge::PromotionCandidate,
            qxfx0_bridge::ExclusionReason,
        )>,
    )> {
        Self::draft_with_imports(db, now, None).map(|report| (report.overlay, report.exclusions))
    }

    /// Draft with an optional import-quarantine byte stream mixed into the
    /// evidence: runtime Promoted triples and resolved import predicates
    /// share one snapshot digest, one admission ladder, one overlay.
    /// Returns a [`DraftReport`].
    pub fn draft_with_imports(
        db: &qxfx0_persistence::Persistence,
        now: i64,
        import_bytes: Option<&[u8]>,
    ) -> anyhow::Result<DraftReport> {
        let snapshot_id = Self::snapshot_id_mixed(db, import_bytes)?;
        let mut candidates = Self::enumerate_candidates(db, &snapshot_id)?;
        let mut import_refusals: Vec<(String, qxfx0_bridge::ExclusionReason)> = Vec::new();
        let mut imported = 0usize;
        if let Some(bytes) = import_bytes {
            let text = std::str::from_utf8(bytes)
                .map_err(|error| anyhow::anyhow!("import quarantine is not UTF-8: {error}"))?;
            let (records, malformed) = qxfx0_bridge::parse_quarantine_jsonl(text);
            for refusal in malformed {
                import_refusals.push((
                    format!("line {}", refusal.line),
                    qxfx0_bridge::ExclusionReason::UnknownEndpoint,
                ));
            }
            let known = Self::known_atoms(db)?;
            let (mut resolved, refused) =
                qxfx0_bridge::candidates_from_import(&records, &snapshot_id, &known);
            imported = resolved.len();
            import_refusals.extend(refused);
            candidates.append(&mut resolved);
        }
        let known = Self::known_atoms(db)?;
        let (overlay, exclusions) = qxfx0_bridge::create_draft_with_admission(
            &snapshot_id,
            &candidates,
            &Self::policy(),
            &Self::baseline_for_topic,
            &Self::topic_admission_for,
            &known,
            now,
        );
        overlay
            .verify_integrity()
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        if !overlay.predicates.is_empty() {
            let json = serde_json::to_string(&overlay)?;
            db.save_promotion_overlay(
                &overlay.version,
                promotion_status_name(overlay.status),
                &overlay.snapshot_id,
                overlay.parent_version.as_deref(),
                &overlay.checksum,
                &json,
                overlay.created_at,
            )?;
        }
        Ok(DraftReport {
            overlay,
            exclusions,
            import_refusals,
            imported,
        })
    }

    /// Read a stored overlay and re-verify its content address on the way in.
    fn load_overlay(
        db: &qxfx0_persistence::Persistence,
        version: &str,
    ) -> anyhow::Result<qxfx0_bridge::Overlay> {
        let Some((stored_status, json)) = db.load_promotion_overlay(version)? else {
            return Err(anyhow::anyhow!("no promotion overlay {version}"));
        };
        let overlay: qxfx0_bridge::Overlay = serde_json::from_str(&json)?;
        overlay
            .verify_integrity()
            .map_err(|error| anyhow::anyhow!("promotion overlay {version}: {error}"))?;
        if promotion_status_name(overlay.status) != stored_status {
            return Err(anyhow::anyhow!(
                "promotion overlay {version} status mismatch (store {stored_status:?}, blob {:?})",
                overlay.status
            ));
        }
        Ok(overlay)
    }

    /// Advance a Draft to Activated via the store's pinned-CAS transition.
    /// `approve` requires the latest passing corpus-precheck trial bound to
    /// this exact version *and* checksum: an evaluation made against
    /// different content (a re-draft the operator has not re-examined) must
    /// not authorize activation. Run `promotion evaluate <version>` first.
    pub fn approve(
        db: &qxfx0_persistence::Persistence,
        version: &str,
        now: i64,
    ) -> anyhow::Result<qxfx0_bridge::Overlay> {
        let current = Self::load_overlay(db, version)?;
        let bound = db.latest_passing_evaluation(&current.version, &current.checksum)?;
        let Some((evaluation_id, _)) = bound else {
            return Err(anyhow::anyhow!(
                "no passing corpus evaluation is bound to overlay {version} \
                 (run `promotion evaluate {version}` first)"
            ));
        };
        let _ = evaluation_id;
        let current_json = serde_json::to_string(&current)?;
        let activated = current
            .activate(now)
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        let json = serde_json::to_string(&activated)?;
        db.replace_promotion_overlay_if_matches(
            &activated.version,
            &current_json,
            promotion_status_name(activated.status),
            &json,
            &activated.checksum,
        )?;
        Ok(activated)
    }

    /// Run the structural corpus precheck over one overlay and persist the
    /// trial row: topics are the overlay's own predicate topics plus the
    /// fixed 12-topic evaluation set (plus any `--topics` extras), all
    /// normalized and deduplicated so the trial is deterministic. Baseline
    /// surfaces come from the argued registry; the duplicate-with-authority
    /// probe reads the embedded seed graph. Idempotent by content-addressed
    /// evaluation id: re-running the same trial is a store no-op.
    pub fn evaluate(
        db: &qxfx0_persistence::Persistence,
        version: &str,
        extra_topics: &[String],
        now: i64,
    ) -> anyhow::Result<qxfx0_bridge::CorpusTrial> {
        let overlay = Self::load_overlay(db, version)?;
        let mut topics: std::collections::BTreeSet<String> = overlay
            .predicates
            .iter()
            .map(|predicate| predicate.topic.clone())
            .collect();
        topics.extend(
            qxfx0_bridge::EVALUATION_TOPIC_SET
                .iter()
                .map(|topic| topic.to_string()),
        );
        for extra in extra_topics {
            let normalized = extra.trim().to_lowercase();
            if !normalized.is_empty() {
                topics.insert(normalized);
            }
        }
        let trial = qxfx0_bridge::run_corpus_precheck(
            &overlay,
            &topics,
            &Self::baseline_for_topic,
            &Self::relates_to_seed_graph,
            now,
        );
        let details = serde_json::to_string(&trial.topics)?;
        db.save_promotion_evaluation(qxfx0_persistence::PromotionEvaluationRow {
            evaluation_id: &trial.evaluation_id,
            overlay_version: &trial.overlay_version,
            corpus_version: &trial.corpus_version,
            completed_at: trial.completed_at,
            overlay_checksum: &trial.overlay_checksum,
            automated_passed: trial.passed,
            overlay_usage_cases: trial.overlay_usage_cases as i64,
            details_json: &details,
        })?;
        Ok(trial)
    }

    /// Revalidate an overlay under the current gate policy and baseline.
    /// Report-only: the stored row is never touched (release is permanent),
    /// the report is the audit instrument the operator archives.
    pub fn revalidate(
        db: &qxfx0_persistence::Persistence,
        version: &str,
    ) -> anyhow::Result<qxfx0_bridge::Revalidation> {
        let overlay = Self::load_overlay(db, version)?;
        Ok(qxfx0_bridge::revalidate(
            &overlay,
            &Self::policy(),
            &Self::baseline_for_topic,
        ))
    }

    /// The `relates_to` oracle for the corpus precheck: a canonical triple
    /// collides with curated authority iff the embedded seed graph already
    /// carries that exact (subject, relation, object). The seed graph is
    /// process-global and static, so the oracle is deterministic.
    fn relates_to_seed_graph(
        topic: &str,
        subject: &str,
        relation: qxfx0_types::RelationType,
        object: &str,
    ) -> bool {
        let _ = topic;
        Self::curated_triple_set().contains(&(
            subject.to_string(),
            qxfx0_bridge::promotion::canonical_slug(relation),
            object.to_string(),
        ))
    }

    /// Release an Activated overlay (the human decision; permanent) and
    /// point the active singleton at it. The previously-active version is
    /// recorded as this overlay's parent — the rollback target.
    pub fn release(
        db: &qxfx0_persistence::Persistence,
        version: &str,
        now: i64,
    ) -> anyhow::Result<qxfx0_bridge::Overlay> {
        let current = Self::load_overlay(db, version)?;
        let current_json = serde_json::to_string(&current)?;
        let released = current
            .release(now)
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        let released = qxfx0_bridge::Overlay {
            parent_version: db.load_active_promotion_overlay()?,
            ..released
        };
        let json = serde_json::to_string(&released)?;
        db.replace_promotion_overlay_if_matches(
            &released.version,
            &current_json,
            promotion_status_name(released.status),
            &json,
            &released.checksum,
        )?;
        db.set_active_promotion_overlay(Some(version), now)?;
        Ok(released)
    }

    /// Retire the active overlay: move the pointer to its parent (the
    /// previous Released version, or none at the root). Released rows are
    /// never edited — only the singleton moves.
    pub fn rollback(
        db: &qxfx0_persistence::Persistence,
        now: i64,
    ) -> anyhow::Result<Option<String>> {
        let Some(active_version) = db.load_active_promotion_overlay()? else {
            return Ok(None);
        };
        let active = Self::load_overlay(db, &active_version)?;
        let target = qxfx0_bridge::rollback(&active).map_err(|error| anyhow::anyhow!("{error}"))?;
        db.set_active_promotion_overlay(target.as_deref(), now)?;
        Ok(target)
    }

    /// The active overlay version, if any.
    pub fn active(db: &qxfx0_persistence::Persistence) -> anyhow::Result<Option<String>> {
        Ok(db.load_active_promotion_overlay()?)
    }

    /// The overlay journal (newest creation first) for `promotion list`.
    pub fn list(db: &qxfx0_persistence::Persistence) -> anyhow::Result<Vec<(String, String, i64)>> {
        Ok(db.list_promotion_overlays()?)
    }
}

/// Execute production health checks without mutating session state. Opening
/// the database may apply the normal idempotent schema migration.
pub fn run_doctor(db_path: &str) -> DoctorReport {
    let mut report = DoctorReport::default();

    match qxfx0_persistence::Persistence::open(db_path) {
        Ok(db) => match db.health_check() {
            Ok(violations) => report.checks.push(DoctorCheck {
                name: "SQLite",
                passed: violations.is_empty(),
                details: if violations.is_empty() {
                    format!(
                        "schema v{}, quick_check/foreign keys/session states valid",
                        db.schema_version().unwrap_or_default()
                    )
                } else {
                    violations.join("; ")
                },
            }),
            Err(error) => report.checks.push(DoctorCheck {
                name: "SQLite",
                passed: false,
                details: error.to_string(),
            }),
        },
        Err(error) => report.checks.push(DoctorCheck {
            name: "SQLite",
            passed: false,
            details: error.to_string(),
        }),
    }

    report.checks.push(DoctorCheck {
        name: "Performance diagnostics",
        passed: true,
        details: concat!(
            "opt-in qxfx0.turn-diagnostics.v1 records stage timing, ",
            "SQLite write-lock/commit timing, and host metadata outside session state"
        )
        .into(),
    });

    let graph = seed_graph();
    let mut graph_violations = graph.validate().err().unwrap_or_default();
    for topic in qxfx0_semantic::COVERED_TOPICS {
        if !graph
            .atoms
            .contains_key(&qxfx0_types::atom::AtomId::new(*topic))
        {
            graph_violations.push(format!("covered topic '{topic}' is absent from seed graph"));
        }
    }
    report.checks.push(DoctorCheck {
        name: "Seed graph",
        passed: graph_violations.is_empty(),
        details: if graph_violations.is_empty() {
            format!(
                "{} atoms, {} relations, {} covered topics",
                graph.atoms.len(),
                graph.edges.len(),
                qxfx0_semantic::COVERED_TOPICS.len()
            )
        } else {
            graph_violations.join("; ")
        },
    });

    match argued_topic_registry() {
        Ok(registry) => {
            let metrics = registry.metrics();
            let missing_topics = registry
                .topics()
                .filter(|topic| !graph.atoms.contains_key(topic.topic()))
                .map(|topic| topic.topic().as_str())
                .collect::<Vec<_>>();
            report.checks.push(DoctorCheck {
                name: "Content plan assets",
                passed: missing_topics.is_empty(),
                details: if missing_topics.is_empty() {
                    format!(
                        concat!(
                            "recognition_topics_total={}, content_predicates_total={}, ",
                            "argued_topics_admitted={}, argued_predicates_admitted={}, ",
                            "profile_enabled={}"
                        ),
                        metrics.recognition_topics_total,
                        metrics.content_predicates_total,
                        metrics.argued_topics_admitted,
                        metrics.argued_predicates_admitted,
                        metrics.profile_enabled,
                    )
                } else {
                    format!(
                        "admitted topics absent from seed graph: {}",
                        missing_topics.join(", ")
                    )
                },
            });
        }
        Err(error) => report.checks.push(DoctorCheck {
            name: "Content plan assets",
            passed: false,
            details: error.into(),
        }),
    }

    let templates = qxfx0_semantic::TemplateRegistry::load();
    let template_violations = templates.validate();
    let used_relation_types = graph
        .edges
        .iter()
        .map(|relation| relation.rel_type)
        .collect::<std::collections::BTreeSet<_>>();
    let covered_relation_types = used_relation_types
        .iter()
        .filter(|relation_type| !templates.get(**relation_type).is_empty())
        .count();
    report.checks.push(DoctorCheck {
        name: "Templates",
        passed: template_violations.is_empty(),
        details: if template_violations.is_empty() {
            format!(
                "{} templates for {} types; direct coverage {}/{} used relation types",
                templates.template_count(),
                templates.relation_type_count(),
                covered_relation_types,
                used_relation_types.len()
            )
        } else {
            template_violations.join("; ")
        },
    });

    let morphology = qxfx0_morphology::MorphologyData::with_seed();
    let morphology_passed = morphology.lemmatize("свободы") == "свобода"
        && morphology.to_case(qxfx0_morphology::Case::Prepositional, "дом") == "доме";
    report.checks.push(DoctorCheck {
        name: "Morphology",
        passed: morphology_passed,
        details: if morphology_passed {
            "seed dictionary and case conversion operational".into()
        } else {
            "lemmatization or case conversion probe failed".into()
        },
    });

    let verb_count = qxfx0_morphology::verb_lexicon::lemma_count();
    let verb_probe = qxfx0_morphology::verbs::conjugate_present(
        "писать",
        qxfx0_morphology::verbs::VerbPerson::FirstSingular,
    ) == Some("пишу".into())
        && qxfx0_morphology::verbs::past_tense(
            "мочь",
            qxfx0_types::morphology::Gender::Masculine,
            qxfx0_types::morphology::Number::Singular,
        ) == Some("мог".into());
    let verbs_passed = verb_count >= 19_000 && verb_probe;
    report.checks.push(DoctorCheck {
        name: "Verb lexicon",
        passed: verbs_passed,
        details: if verbs_passed {
            format!("{verb_count} digest-pinned verb paradigms; conjugation probes operational")
        } else {
            format!(
                "verb lexicon degraded: {verb_count} lemmas, probes {}",
                if verb_probe { "ok" } else { "failed" }
            )
        },
    });

    let adjective_count = qxfx0_morphology::adjective_lexicon::lemma_count();
    let adjective_probe = qxfx0_morphology::lemmatize_surface("внутреннего") == "внутренний"
        && qxfx0_morphology::adjective_lexicon::lookup("необратимый").and_then(|entry| {
            entry.short_form(
                qxfx0_types::morphology::Gender::Neuter,
                qxfx0_types::morphology::Number::Singular,
            )
        }) == Some("необратимо");
    let adjectives_passed = adjective_count >= 20_000 && adjective_probe;
    report.checks.push(DoctorCheck {
        name: "Adjective lexicon",
        passed: adjectives_passed,
        details: if adjectives_passed {
            format!("{adjective_count} digest-pinned adjective paradigms; probes operational")
        } else {
            format!(
                "adjective lexicon degraded: {adjective_count} lemmas, probes {}",
                if adjective_probe { "ok" } else { "failed" }
            )
        },
    });

    let pronoun_count = qxfx0_morphology::pronoun_lexicon::lemma_count();
    let pronoun_probe = qxfx0_morphology::lemmatize_surface("собой") == "себя"
        && qxfx0_morphology::lemmatize_surface("мной") == "я";
    let pronouns_passed = pronoun_count >= 40 && pronoun_probe;
    report.checks.push(DoctorCheck {
        name: "Pronoun lexicon",
        passed: pronouns_passed,
        details: if pronouns_passed {
            format!("{pronoun_count} digest-pinned closed-class paradigms")
        } else {
            format!(
                "pronoun lexicon degraded: {pronoun_count} lemmas, probes {}",
                if pronoun_probe { "ok" } else { "failed" }
            )
        },
    });

    let code_graph = build_full_registry();
    let mut code_violations = code_graph.validate();
    let type_edges = code_graph
        .edges
        .iter()
        .filter(|edge| edge.rel_type == qxfx0_code::CodeRelationType::RelComposes)
        .count();
    if code_graph.atoms.len() < 80 {
        code_violations.push("production registry contains fewer than 80 real atoms".into());
    }
    if type_edges == 0 {
        code_violations.push("production registry contains no type-directed edges".into());
    }
    report.checks.push(DoctorCheck {
        name: "Code registry",
        passed: code_violations.is_empty(),
        details: if code_violations.is_empty() {
            format!(
                "{} typed atoms, {} relations, {} RelComposes edges",
                code_graph.atoms.len(),
                code_graph.edges.len(),
                type_edges
            )
        } else {
            code_violations.join("; ")
        },
    });

    let packs = qxfx0_semantic::active_pack_set();
    let pack_valid = packs.fingerprint().len() == 64;
    report.checks.push(DoctorCheck {
        name: "Knowledge pack",
        passed: pack_valid,
        details: if pack_valid {
            format!(
                "active immutable pack fingerprint sha256:{}, {} facts",
                packs.fingerprint(),
                packs.facts().len()
            )
        } else {
            "active pack fingerprint is not a SHA-256 identifier".into()
        },
    });

    let fact_registry_valid = packs
        .facts()
        .records()
        .all(|record| packs.facts().select(&record.id).is_ok());
    report.checks.push(DoctorCheck {
        name: "Curated FactRegistry",
        passed: fact_registry_valid,
        details: if fact_registry_valid {
            format!(
                "{} curated FactId records re-resolve successfully",
                packs.facts().len()
            )
        } else {
            "active FactRegistry contains a non-selectable record".into()
        },
    });

    let perspective_valid = qxfx0_types::PerspectiveState::default()
        .validate()
        .is_empty()
        && FactGroundedRollout::default() == FactGroundedRollout::Disabled;
    report.checks.push(DoctorCheck {
        name: "Perspective boundary",
        passed: perspective_valid,
        details: if perspective_valid {
            "bounded PerspectiveState valid; fact-grounded rollout default is Disabled".into()
        } else {
            "PerspectiveState or default-off rollout contract failed".into()
        },
    });

    let stance_contract_valid = qxfx0_types::STANCE_ATTESTATION_VERSION == 1
        && qxfx0_types::STANCE_PROVENANCE_VERSION == 1
        && qxfx0_types::StanceTopic::new("doctor").is_ok()
        && qxfx0_types::BoundedStanceProvenance::default().capacity() > 0;
    report.checks.push(DoctorCheck {
        name: "Stance authority",
        passed: stance_contract_valid,
        details: if stance_contract_valid {
            "signed attestation, bounded provenance, and temporal contract versions valid".into()
        } else {
            "stance authority contract probe failed".into()
        },
    });

    // Self-layer V2 canonical invariants (ADR-0043 U2/U3): the pure port of
    // the Haskell subject core must satisfy its own laws before any
    // pipeline integration is allowed to consume it.
    let essence_v2_violations = qxfx0_self_v2::validate_invariants();
    report.checks.push(DoctorCheck {
        name: "Self layer V2",
        passed: essence_v2_violations.is_empty(),
        details: if essence_v2_violations.is_empty() {
            "conatus builtin weights positive; essence defaults coherent; empty carrier never commits; salience + deliberation builtins coherent"
                .into()
        } else {
            essence_v2_violations.join("; ")
        },
    });

    // Learning bridge invariants (ADR-0043 U4): the between-turn algebra
    // must be sound before any promotion door (U5) can open, and the
    // default build must carry no network surface — privacy as an
    // architectural fact.
    let bridge_violations = qxfx0_bridge::validate_bridge_invariants();
    let bridge_network_free = !cfg!(feature = "llm-candidates");
    report.checks.push(DoctorCheck {
        name: "Learning bridge",
        passed: bridge_violations.is_empty() && bridge_network_free,
        details: if bridge_violations.is_empty() {
            "corroboration ladder bounded; promotion thresholds in range; decay/retire total; no network in the default build".into()
        } else {
            bridge_violations.join("; ")
        },
    });

    // Promotion boundary invariants (ADR-0043 U5): the gate policy, the
    // informativeness floor and the lifecycle transition table (release is
    // permanent, rollback retires only the pointer) must all be coherent
    // before the human-release door means anything.
    let promotion_violations = qxfx0_bridge::promotion::validate_promotion_invariants();
    report.checks.push(DoctorCheck {
        name: "Promotion boundary",
        passed: promotion_violations.is_empty(),
        details: if promotion_violations.is_empty() {
            "gate policy versioned+checksummed; semantic-gain floor in range; lifecycle transitions and release immutability enforceable".into()
        } else {
            promotion_violations.join("; ")
        },
    });

    // FELT evidence invariants (ADR-0043 U6): the six-gate thresholds are
    // the law — a future edit that lowers the ten-turn floor or empties
    // the recovery literal must fail loudly here instead of silently
    // passing thinner sessions.
    let felt_violations = qxfx0_codex::felt::validate_felt_invariants();
    report.checks.push(DoctorCheck {
        name: "Felt evidence",
        passed: felt_violations.is_empty(),
        details: if felt_violations.is_empty() {
            "six gates intact; ten-turn floor, two-turn definition, two-topic distinction; recovery literal non-empty".into()
        } else {
            felt_violations.join("; ")
        },
    });

    report
}

/// DialogueSession encapsulates the state and tools needed for a conversation.
pub struct DialogueSession {
    pub state: SystemState,
    pub db: qxfx0_persistence::Persistence,
    pub orchestrator: CodeOrchestrator,
}

impl DialogueSession {
    pub fn new(db: qxfx0_persistence::Persistence, session_id: &str) -> anyhow::Result<Self> {
        let state = load_or_create_state(&db, session_id)?;
        let graph = build_full_registry();
        let orchestrator = CodeOrchestrator::new(graph);
        Ok(Self {
            state,
            db,
            orchestrator,
        })
    }

    /// Process a turn, integrating both the semantic pipeline and code orchestration.
    pub fn process_turn(&mut self, text: &str) -> anyhow::Result<String> {
        let input = TurnInput {
            raw_text: text.to_string(),
            session_id: self.state.session_id.clone(),
        };
        // 1. Try the standard semantic pipeline
        let output = process_turn_with_options(&input, &mut self.state, TurnOptions::new());

        // 2. If the response is empty or looks like a request for code/action,
        // we can integrate the CodeOrchestrator here.
        // For now, we prioritize the pipeline but allow the orchestrator to supplement
        // if the pipeline output is a specific trigger or empty.
        let final_response = if output.response.is_empty() || output.response.contains("код") {
            match self.orchestrator.orchestrate(text) {
                Ok(res) => format!(
                    "{} \n\n[Code Orchestration]: {}\n",
                    output.response, res.rendered
                ),
                Err(_) => output.response,
            }
        } else {
            output.response
        };

        let session_id = self.state.session_id.clone();
        save_journal_state(&self.db, &session_id, &mut self.state)?;
        Ok(final_response)
    }
}

/// Run a single turn through the pipeline (mirrors the `Turn` CLI branch).
/// Persists before returning the response text — see H4 in the audit.
pub fn run_turn(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
) -> anyhow::Result<String> {
    run_turn_with_renderer(db, session_id, text, RendererAuthority::LegacyShadow)
}

pub fn run_turn_with_renderer(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
) -> anyhow::Result<String> {
    let mut state = load_or_create_state(db, session_id)?;
    let input = TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let output = process_turn_with_options(
        &input,
        &mut state,
        TurnOptions::new().with_renderer(renderer_authority),
    );
    save_journal_state(db, session_id, &mut state)?;
    Ok(output.response)
}

/// Run a standalone, explicit provenance-recording turn.
pub fn run_turn_with_renderer_and_stance_provenance(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
) -> anyhow::Result<String> {
    let mut state = load_or_create_state(db, session_id)?;
    let input = TurnInput {
        raw_text: text.into(),
        session_id: session_id.into(),
    };
    let output = process_turn_with_renderer_and_stance_provenance(
        &input,
        &mut state,
        renderer_authority,
        qxfx0_pipeline::StanceProvenanceMode::RecordAffirmedSystemDecision,
    );
    save_journal_state(db, session_id, &mut state)?;
    Ok(output.response)
}

/// Run one normal persisted turn while returning observation-only doubt
/// evidence for an external sink. The trace never enters `SystemState`.
/// `essence_v2_ablation` selects the B2 control arm (ADR-0043 U2); the CLI
/// passes `Enabled` unless `--essence-v2-ablation` says otherwise.
pub fn run_turn_with_renderer_doubt_shadow_trace(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
    essence_v2_ablation: EssenceAblation,
) -> anyhow::Result<DoubtShadowTracedTurn> {
    let mut state = load_or_create_state(db, session_id)?;
    let input = TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let (output, trace) = process_turn_with_options_and_trace(
        &input,
        &mut state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_doubt_shadow(DoubtShadowMode::TraceOnly)
            .with_essence_v2_ablation(essence_v2_ablation),
    );
    save_journal_state(db, session_id, &mut state)?;
    Ok(DoubtShadowTracedTurn {
        response: output.response,
        trace,
    })
}

/// Run one normal persisted turn while returning observation-only typed anomaly
/// evidence for an external sink. The trace never enters `SystemState`.
pub fn run_turn_with_renderer_anomaly_shadow_trace(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
) -> anyhow::Result<DoubtShadowTracedTurn> {
    let mut state = load_or_create_state(db, session_id)?;
    let input = TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let (output, trace) = process_turn_with_options_and_trace(
        &input,
        &mut state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_anomaly_shadow(AnomalyShadowMode::TraceOnly),
    );
    save_journal_state(db, session_id, &mut state)?;
    Ok(DoubtShadowTracedTurn {
        response: output.response,
        trace,
    })
}

pub fn run_turn_with_renderer_cognitive_pilot(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
    clarification: ClarificationMode,
    suppression: SameTopicSuppressionMode,
) -> anyhow::Result<DoubtShadowTracedTurn> {
    let mut state = load_or_create_state(db, session_id)?;
    let input = TurnInput {
        raw_text: text.into(),
        session_id: session_id.into(),
    };
    let (output, trace) = process_turn_with_options_and_trace(
        &input,
        &mut state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_doubt_shadow(DoubtShadowMode::Disabled)
            .with_clarification(clarification)
            .with_suppression(suppression),
    );
    save_journal_state(db, session_id, &mut state)?;
    Ok(DoubtShadowTracedTurn {
        response: output.response,
        trace,
    })
}

/// Run one turn with lightweight timing and SQLite diagnostic evidence.
///
/// The standard [`run_turn_with_renderer`] path remains timing-free. This
/// opt-in function executes the same state load, pipeline, and save sequence,
/// but returns observational timing that callers may write outside the
/// production database.
pub fn run_turn_with_renderer_diagnostics(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
) -> anyhow::Result<DiagnosedTurn> {
    let total_started = Instant::now();
    let load_started = Instant::now();
    let mut state = load_or_create_state(db, session_id)?;
    let db_load_ms = elapsed_millis(load_started);
    let input = TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let (output, pipeline) = process_turn_with_options_and_timing(
        &input,
        &mut state,
        TurnOptions::new().with_renderer(renderer_authority),
    );
    stamp_practice_today(&mut state);
    let db_save = db.save_state_with_timings(session_id, &state)?;
    Ok(build_diagnosed_turn(
        &state,
        renderer_authority,
        output,
        pipeline,
        db_save,
        db_load_ms,
        total_started,
    ))
}

/// Run one turn with both existing timing diagnostics and doubt shadow trace
/// evidence. This preserves the normal single processing/persistence path.
/// `essence_v2_ablation` selects the B2 control arm (ADR-0043 U2).
pub fn run_turn_with_renderer_diagnostics_and_doubt_shadow_trace(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
    essence_v2_ablation: EssenceAblation,
) -> anyhow::Result<(
    DiagnosedTurn,
    qxfx0_pipeline::execution_trace::PipelineTrace,
)> {
    let total_started = Instant::now();
    let load_started = Instant::now();
    let mut state = load_or_create_state(db, session_id)?;
    let db_load_ms = elapsed_millis(load_started);
    let input = TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let (output, pipeline, trace) = process_turn_with_options_timing_and_trace(
        &input,
        &mut state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_doubt_shadow(DoubtShadowMode::TraceOnly)
            .with_essence_v2_ablation(essence_v2_ablation),
    );
    stamp_practice_today(&mut state);
    let db_save = db.save_state_with_timings(session_id, &state)?;
    Ok((
        build_diagnosed_turn(
            &state,
            renderer_authority,
            output,
            pipeline,
            db_save,
            db_load_ms,
            total_started,
        ),
        trace,
    ))
}

/// Run one turn with existing timing diagnostics and anomaly shadow trace
/// evidence without processing or persisting a second turn.
pub fn run_turn_with_renderer_diagnostics_and_anomaly_shadow_trace(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
) -> anyhow::Result<(
    DiagnosedTurn,
    qxfx0_pipeline::execution_trace::PipelineTrace,
)> {
    let total_started = Instant::now();
    let load_started = Instant::now();
    let mut state = load_or_create_state(db, session_id)?;
    let db_load_ms = elapsed_millis(load_started);
    let input = TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let (output, pipeline, trace) = process_turn_with_options_timing_and_trace(
        &input,
        &mut state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_anomaly_shadow(AnomalyShadowMode::TraceOnly),
    );
    stamp_practice_today(&mut state);
    let db_save = db.save_state_with_timings(session_id, &state)?;
    Ok((
        build_diagnosed_turn(
            &state,
            renderer_authority,
            output,
            pipeline,
            db_save,
            db_load_ms,
            total_started,
        ),
        trace,
    ))
}

pub fn run_turn_with_renderer_diagnostics_and_cognitive_pilot(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
    clarification: ClarificationMode,
    suppression: SameTopicSuppressionMode,
) -> anyhow::Result<(
    DiagnosedTurn,
    qxfx0_pipeline::execution_trace::PipelineTrace,
)> {
    let total_started = Instant::now();
    let load_started = Instant::now();
    let mut state = load_or_create_state(db, session_id)?;
    let db_load_ms = elapsed_millis(load_started);
    let input = TurnInput {
        raw_text: text.into(),
        session_id: session_id.into(),
    };
    let (output, pipeline, trace) = process_turn_with_options_timing_and_trace(
        &input,
        &mut state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_doubt_shadow(DoubtShadowMode::Disabled)
            .with_clarification(clarification)
            .with_suppression(suppression),
    );
    stamp_practice_today(&mut state);
    let db_save = db.save_state_with_timings(session_id, &state)?;
    Ok((
        build_diagnosed_turn(
            &state,
            renderer_authority,
            output,
            pipeline,
            db_save,
            db_load_ms,
            total_started,
        ),
        trace,
    ))
}

fn build_diagnosed_turn(
    state: &SystemState,
    renderer_authority: RendererAuthority,
    output: qxfx0_pipeline::TurnOutput,
    pipeline: PipelineStageTimings,
    db_save: SaveStateTimings,
    db_load_ms: u64,
    total_started: Instant,
) -> DiagnosedTurn {
    let response = output.response;
    DiagnosedTurn {
        diagnostics: TurnDiagnostics {
            schema: "qxfx0.turn-diagnostics.v1",
            turn: state.dialogue.turn_count,
            renderer_authority: renderer_authority.as_str(),
            family: format!("{:?}", output.family),
            blocked: output.blocked,
            response_bytes: response.len(),
            db_open_ms: 0,
            cli_process_ms: 0,
            db_load_ms,
            pipeline,
            db_save,
            total_ms: elapsed_millis(total_started),
            host: diagnostic_host_metadata(),
        },
        response,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::process::Stdio;

    fn authority_trace_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "qxfx0-authority-{label}-{}.jsonl",
            std::process::id()
        ))
    }

    #[test]
    fn authority_trace_verification_and_report_are_fail_closed() {
        let db = qxfx0_persistence::Persistence::open_memory().expect("open memory db");
        let traced = run_turn_with_v2_authority_trace(
            &db,
            "authority-verification",
            "что такое свобода?",
            qxfx0_pipeline::ResponsePlanV2Authority::Canary,
        )
        .expect("authority turn");
        assert_eq!(
            traced.trace.authority_guard_classification.as_deref(),
            Some("v2_successfully_emitted")
        );

        let path = authority_trace_path("valid");
        let _ = std::fs::remove_file(&path);
        let mut sink = create_authority_trace_sink(&path).expect("new authority sink");
        write_authority_trace_jsonl(&mut sink, &traced.trace).expect("write authority trace");
        drop(sink);
        let report = verify_authority_trace(&path).expect("valid trace verifies");
        assert_eq!(report.turns, 1);
        assert_eq!(report.compositional + report.audited_verbatim, 1);
        assert_eq!(report.replay_failures, 0);

        let mut tampered: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        tampered["trace"]["authority_receipt"]["output_digest"] =
            serde_json::Value::String("0".repeat(64));
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&tampered).unwrap()),
        )
        .unwrap();
        assert!(verify_authority_trace(&path).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn authority_report_rejects_an_oversized_jsonl_line() {
        let path = authority_trace_path("oversized-line");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, vec![b'x'; MAX_AUTHORITY_TRACE_LINE_BYTES + 1])
            .expect("write oversized trace line");

        let error = authority_report([&path], false, AuthorityReportScope::All)
            .expect_err("oversized line must be rejected");
        let message = error.to_string();
        assert!(message.contains("line 1"), "{message}");
        assert!(message.contains("byte limit"), "{message}");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn authority_report_rejects_non_utf8_with_path_and_line_context() {
        let path = authority_trace_path("non-utf8");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, [0xff, b'\n']).expect("write non-UTF-8 trace line");

        let error = authority_report([&path], false, AuthorityReportScope::All)
            .expect_err("non-UTF-8 line must be rejected");
        let message = error.to_string();
        assert!(message.contains("line 1"), "{message}");
        assert!(message.contains("not UTF-8"), "{message}");
        assert!(
            message.contains(path.to_string_lossy().as_ref()),
            "{message}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn authority_report_counts_denial_before_render() {
        let db = qxfx0_persistence::Persistence::open_memory().expect("open memory db");
        let traced = run_turn_with_v2_authority_trace(
            &db,
            "authority-denial",
            "что такое время?",
            qxfx0_pipeline::ResponsePlanV2Authority::Canary,
        )
        .expect("denied authority turn");
        assert_eq!(
            traced.trace.authority_guard_classification.as_deref(),
            Some("authority_denied_before_render")
        );

        let path = authority_trace_path("denied");
        let _ = std::fs::remove_file(&path);
        let mut sink = create_authority_trace_sink(&path).expect("new authority sink");
        write_authority_trace_jsonl(&mut sink, &traced.trace).expect("write authority trace");
        drop(sink);
        let report = authority_report([&path], false, AuthorityReportScope::All)
            .expect("denial remains reportable");
        assert_eq!(report.turns, 1);
        assert_eq!(report.rollback_activations, 1);
        assert!(verify_authority_trace(&path).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn response_plan_v2_shadow_trace_is_observational() {
        let db = qxfx0_persistence::Persistence::open_memory().expect("open memory db");
        let traced =
            run_turn_with_v2_shadow_trace(&db, "cohort-shadow", "что такое ответственность?")
                .expect("shadow turn");
        let step = traced
            .trace
            .steps
            .iter()
            .find(|step| step.stage == "response_plan_v2")
            .expect("V2 shadow step");
        assert_eq!(step.metadata.get("requested_mode"), Some(&"Shadow".into()));
        assert_eq!(step.metadata.get("effective_mode"), Some(&"Shadow".into()));
        assert_eq!(step.metadata.get("attempted"), Some(&"true".into()));
        assert_eq!(step.metadata.get("completed"), Some(&"true".into()));
        assert_eq!(step.metadata.get("downgrade_count"), Some(&"0".into()));
        assert_eq!(step.metadata.get("semantic_parity"), Some(&"true".into()));
        assert_eq!(step.metadata.get("authority_parity"), Some(&"true".into()));
        assert_eq!(
            step.metadata.get("realization_parity"),
            Some(&"true".into())
        );
        assert_eq!(step.metadata.get("replay_parity"), Some(&"true".into()));
        assert_eq!(step.metadata.get("v1_authoritative"), Some(&"true".into()));
        assert_eq!(step.metadata.get("v1_fallback_used"), Some(&"false".into()));
        assert!(traced.trace.authority_receipt.is_some());
        assert!(
            db.load_state("cohort-shadow")
                .expect("load state")
                .is_none(),
            "shadow evidence must not persist its in-memory turn"
        );
    }

    /// M7.1 — smoke test: run a `Turn` against an in-memory DB and assert the
    /// pipeline returns a non-empty response. Mirrors the `Turn` CLI branch.
    #[test]
    fn test_turn_smoke() {
        let db = qxfx0_persistence::Persistence::open_memory().expect("open in-memory db");
        let response =
            run_turn(&db, "smoke-session", "что такое свобода?").expect("turn should succeed");
        assert!(
            !response.is_empty(),
            "pipeline produced empty response for seeded topic"
        );

        // State should have been persisted by run_turn.
        let loaded = db
            .load_state("smoke-session")
            .expect("load after save")
            .expect("session row must exist");
        assert_eq!(loaded.session_id, "smoke-session");
        assert!(loaded.dialogue.turn_count >= 1);
    }

    #[test]
    fn test_audited_plan_renderer_flag_is_available_to_the_cli_library() {
        let db = qxfx0_persistence::Persistence::open_memory().expect("open in-memory db");
        let response = run_turn_with_renderer(
            &db,
            "audited-plan-session",
            "что такое свобода?",
            RendererAuthority::AuditedPlan,
        )
        .expect("turn should succeed");

        assert!(response.starts_with("Тезис: свобода предполагает возможность выбора."));
        assert!(response.ends_with("Проверка: верно ли это?"));
    }

    /// M7.2 — chat-mode EOF must still persist state. We spawn the actual
    /// binary with piped stdin that ends in EOF (no `:quit`) so the chat
    /// loop reaches the unconditional save at the end.
    #[test]
    fn test_chat_eof_saves() {
        let tmp = std::env::temp_dir().join(format!("qxfx0-cli-eof-{}.db", std::process::id()));
        let db_path = tmp.to_string_lossy().to_string();
        let session_id = format!("eof-session-{}", std::process::id());

        let stdin_payload = "что такое истина?\n".to_string();

        // Resolve the binary path. `CARGO_BIN_EXE_qxfx0` is set when this
        // crate's own integration tests build (or when running via
        // `cargo test --bin qxfx0`); fall back to the conventional
        // `target/debug/qxfx0` path for plain `cargo test --workspace`.
        let bin = match std::env::var("CARGO_BIN_EXE_qxfx0") {
            Ok(p) => p,
            Err(_) => {
                let target_dir =
                    std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
                let workspace = std::env::var("CARGO_MANIFEST_DIR")
                    .map(|m| {
                        std::path::PathBuf::from(m)
                            .join(&target_dir)
                            .join("debug/qxfx0")
                    })
                    .unwrap_or_else(|_| std::path::PathBuf::from(&target_dir).join("debug/qxfx0"));
                workspace.to_string_lossy().to_string()
            }
        };

        if !std::path::Path::new(&bin).exists() {
            eprintln!(
                "skipping test_chat_eof_saves: qxfx0 binary not found at {}",
                bin
            );
            return;
        }

        let mut child = std::process::Command::new(&bin)
            .args(["chat", "--session-id", &session_id, "--db", &db_path])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn qxfx0 chat");

        {
            let mut stdin = child.stdin.take().expect("stdin pipe");
            stdin
                .write_all(stdin_payload.as_bytes())
                .expect("write stdin");
            // Dropping `stdin` here closes the pipe → EOF on the child side.
        }

        let output = child.wait_with_output().expect("wait for qxfx0");
        assert!(
            output.status.success(),
            "qxfx0 chat exited non-zero: {:?}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        let db =
            qxfx0_persistence::Persistence::open(&db_path).expect("reopen db written by child");
        let loaded = db
            .load_state(&session_id)
            .expect("load after child exit")
            .expect("session row must exist after EOF exit");
        assert_eq!(loaded.session_id, session_id);
        assert!(
            loaded.dialogue.turn_count >= 1,
            "expected at least one processed turn before EOF"
        );

        let _ = std::fs::remove_file(&db_path);
    }

    /// M7.3 — a corrupted session row must surface as an `Err` rather than
    /// silently being treated as "no state, create a fresh one".
    #[test]
    fn test_corrupted_db() {
        let tmp = std::env::temp_dir().join(format!("qxfx0-cli-corrupt-{}.db", std::process::id()));
        let db_path = tmp.to_string_lossy().to_string();

        // Create the schema by opening the DB once.
        {
            let db = qxfx0_persistence::Persistence::open(&db_path).expect("open");
            // Use the persistence API to insert a valid row first (so the
            // table exists), then overwrite the JSON column with garbage
            // through a side-channel `rusqlite::Connection`.
            db.save_state("corrupt-session", &fresh_state("corrupt-session"))
                .expect("initial save");
        }

        // Corrupt the JSON column directly.
        {
            use rusqlite::{params, Connection};
            let conn = Connection::open(&db_path).expect("open conn");
            conn.execute(
                "UPDATE runtime_sessions SET state_json = ?1 WHERE id = ?2",
                params!["{not valid json", "corrupt-session"],
            )
            .expect("corrupt row");
        }

        // Re-open via the typed Persistence and try load_or_create_state.
        let db = qxfx0_persistence::Persistence::open(&db_path).expect("reopen");
        let result = load_or_create_state(&db, "corrupt-session");
        assert!(
            result.is_err(),
            "load_or_create_state must propagate serialization errors, got Ok"
        );

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn diagnostic_turn_writes_timing_outside_the_session_database() {
        let db = qxfx0_persistence::Persistence::open_memory().expect("open memory db");
        let diagnosed = run_turn_with_renderer_diagnostics(
            &db,
            "diagnostic-session",
            "что такое свобода?",
            RendererAuthority::LegacyShadow,
        )
        .expect("diagnostic turn should succeed");

        assert!(!diagnosed.response.is_empty());
        assert_eq!(diagnosed.diagnostics.turn, 1);
        assert_eq!(diagnosed.diagnostics.schema, "qxfx0.turn-diagnostics.v1");
        assert_eq!(
            db.load_state("diagnostic-session")
                .expect("load state")
                .expect("state saved")
                .dialogue
                .turn_count,
            1
        );

        let path = std::env::temp_dir().join(format!(
            "qxfx0-turn-diagnostics-{}-{}.jsonl",
            std::process::id(),
            diagnosed.diagnostics.turn
        ));
        let _ = std::fs::remove_file(&path);
        append_turn_diagnostics(&path, &diagnosed.diagnostics).expect("append diagnostic");
        let line = std::fs::read_to_string(&path).expect("read diagnostic");
        let record: serde_json::Value = serde_json::from_str(line.trim()).expect("valid JSONL");
        assert_eq!(record["schema"], "qxfx0.turn-diagnostics.v1");
        assert!(record["pipeline"]["plan_render_ms"].is_number());
        assert!(record["db_save"]["sqlite_write_lock_ms"].is_number());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn doubt_shadow_trace_is_external_and_preserves_normal_persistence() {
        let standard_db = qxfx0_persistence::Persistence::open_memory().expect("open standard");
        let shadow_db = qxfx0_persistence::Persistence::open_memory().expect("open shadow");
        let session_id = "doubt-shadow-cli";
        let text = "что такое свобода?";

        let standard = run_turn_with_renderer(
            &standard_db,
            session_id,
            text,
            RendererAuthority::LegacyShadow,
        )
        .expect("normal turn");
        let traced = run_turn_with_renderer_doubt_shadow_trace(
            &shadow_db,
            session_id,
            text,
            RendererAuthority::LegacyShadow,
            EssenceAblation::Enabled,
        )
        .expect("trace-only turn");
        assert_eq!(traced.response, standard);
        let standard_state = standard_db.load_state(session_id).unwrap().unwrap();
        let shadow_state = shadow_db.load_state(session_id).unwrap().unwrap();
        assert_eq!(
            qxfx0_pipeline::execution_trace::calculate_stable_digest(&standard_state).unwrap(),
            qxfx0_pipeline::execution_trace::calculate_stable_digest(&shadow_state).unwrap()
        );

        let path = std::env::temp_dir().join(format!(
            "qxfx0-doubt-shadow-{}-{}.jsonl",
            std::process::id(),
            shadow_state.dialogue.turn_count
        ));
        let _ = std::fs::remove_file(&path);
        let mut sink = create_doubt_shadow_trace_sink(&path).expect("new trace sink");
        write_doubt_shadow_trace_jsonl(&mut sink, &traced.trace).expect("write trace");
        drop(sink);
        let record: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(record["schema"], "qxfx0.doubt-shadow-trace.v1");
        assert!(record["trace"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step["stage"] == "doubt_shadow"));
        assert!(
            record["trace"].get("total_duration").is_none(),
            "external replay evidence must exclude wall-clock duration"
        );
        assert!(create_doubt_shadow_trace_sink(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_doctor_checks_real_subsystems() {
        let path = std::env::temp_dir().join(format!("qxfx0-doctor-{}.db", std::process::id()));
        let report = run_doctor(path.to_str().unwrap());
        assert!(
            report.is_healthy(),
            "doctor failures: {:?}",
            report
                .checks
                .iter()
                .filter(|check| !check.passed)
                .collect::<Vec<_>>()
        );
        assert_eq!(report.checks.len(), 18);
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "Performance diagnostics" && check.passed));
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "Learning bridge" && check.passed));
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "Promotion boundary" && check.passed));
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "Felt evidence" && check.passed));
        let content_assets = report
            .checks
            .iter()
            .find(|check| check.name == "Content plan assets")
            .expect("content plan assets check");
        assert!(content_assets
            .details
            .contains("argued_topics_admitted=141"));
        assert!(content_assets
            .details
            .contains("content_predicates_total=291"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_operational_metrics_cover_health_storage_and_latency() {
        let path = std::env::temp_dir().join(format!("qxfx0-metrics-{}.db", std::process::id()));
        let metrics = run_operational_metrics(path.to_str().unwrap());
        assert!(metrics.doctor_healthy);
        assert!(metrics.database_bytes > 0);
        assert!(metrics.response_probe_healthy);
        assert!(metrics.threshold_violations(u64::MAX, u64::MAX).is_empty());
        assert!(metrics.to_prometheus().contains("qxfx0_database_bytes"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn test_bridge_maintenance_spares_topic_touching_edges() {
        use qxfx0_bridge::{encode_store, BridgeEdge, RuntimeEdgeStore};
        use qxfx0_types::atom::AtomId;
        use qxfx0_types::RelationType;

        let path =
            std::env::temp_dir().join(format!("qxfx0-bridge-maintain-{}.db", std::process::id()));
        let db_path = path.to_string_lossy().to_string();
        let session_id = format!("bridge-maintain-{}", std::process::id());

        let db = qxfx0_persistence::Persistence::open(&db_path).unwrap();
        let mut state = SystemState {
            session_id: session_id.clone(),
            ..SystemState::default()
        };
        state.dialogue.turn_count = 3;
        state.dialogue.last_topic = Some("свобода".into());
        db.save_state(&session_id, &state).unwrap();

        // Two edges. `свобода -> выбор` touches the session topic and is
        // spared; the `разум -> мысль` edge is off-topic and decays from
        // 0.31 (×0.95 = 0.2945 < 0.3) into retirement.
        let mut store = RuntimeEdgeStore::new();
        let on_topic = BridgeEdge::new(
            AtomId::new("свобода"),
            AtomId::new("выбор"),
            RelationType::RelRelatedTo,
            "свобода",
            0.31,
        );
        let off_topic = BridgeEdge::new(
            AtomId::new("разум"),
            AtomId::new("мысль"),
            RelationType::RelRelatedTo,
            "разум",
            0.31,
        );
        store.insert((on_topic.from.clone(), on_topic.to.clone()), on_topic);
        store.insert((off_topic.from.clone(), off_topic.to.clone()), off_topic);
        db.save_bridge_edges(&session_id, Some(&encode_store(&store).unwrap()))
            .unwrap();

        // A second session carries no bridge store: the maintenance pass
        // must skip it entirely (the bridge sleeps).
        let sleeper = format!("bridge-sleeper-{}", std::process::id());
        let mut sleeper_state = SystemState {
            session_id: sleeper.clone(),
            ..SystemState::default()
        };
        sleeper_state.dialogue.turn_count = 1;
        db.save_state(&sleeper, &sleeper_state).unwrap();
        assert!(db.load_bridge_edges(&sleeper).unwrap().is_none());

        let report = run_bridge_maintenance(&db_path).unwrap();
        assert_eq!(report.sessions_touched, 1, "only the seeded session ran");
        assert_eq!(report.sessions[0].session_id, session_id);
        assert_eq!(report.sessions[0].edges_before, 2);
        assert_eq!(
            report.sessions[0].edges_after, 1,
            "the off-topic edge retired"
        );
        assert_eq!(report.sessions[0].runtime_edges_after, 1);
        assert_eq!(report.total_edges_before, 2);
        assert_eq!(report.total_edges_after, 1);

        // The decayed store persisted: the surviving edge is exactly the
        // on-topic pair, and the sleeper still carries no row.
        let remaining = db.load_bridge_edges(&session_id).unwrap().unwrap();
        let decoded: RuntimeEdgeStore = qxfx0_bridge::decode_store(&remaining).unwrap();
        assert!(decoded.contains_key(&(AtomId::new("свобода"), AtomId::new("выбор"))));
        assert!(!decoded.contains_key(&(AtomId::new("разум"), AtomId::new("мысль"))));
        assert!(db.load_bridge_edges(&sleeper).unwrap().is_none());

        // A second run is idempotent: the surviving edge touches the topic
        // and has no decay to apply.
        let second = run_bridge_maintenance(&db_path).unwrap();
        assert_eq!(second.sessions[0].edges_before, 1);
        assert_eq!(second.sessions[0].edges_after, 1);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn test_promotion_lifecycle_draft_approve_release_rollback() {
        use qxfx0_bridge::{BridgeEdge, BridgeEdgeSource, RuntimeEdgeStore};
        use qxfx0_types::atom::{Atom, AtomCategory, AtomId};
        use qxfx0_types::RelationType;

        let path = std::env::temp_dir().join(format!("qxfx0-promotion-{}.db", std::process::id()));
        let db_path = path.to_string_lossy().to_string();
        let session_id = format!("promo-{}", std::process::id());
        let db = qxfx0_persistence::Persistence::open(&db_path).unwrap();
        let mut state = SystemState {
            session_id: session_id.clone(),
            ..SystemState::default()
        };
        state.dialogue.turn_count = 5;
        // The admission bar needs real grounded atoms: seed them into the
        // runtime graph (the same bar the draft will check them against).
        for (atom, category) in [
            ("свобода", AtomCategory::CatTopic),
            ("предприимчивость", AtomCategory::CatConcept),
        ] {
            state.semantic.runtime_graph.atoms.insert(
                AtomId::new(atom),
                Atom {
                    id: AtomId::new(atom),
                    display: atom.into(),
                    category,
                },
            );
        }
        db.save_state(&session_id, &state).unwrap();

        // Seed one Promoted bridge edge on the curated topic "свобода":
        // counterpoint exists in the registry, both endpoints are known,
        // and the object is novel against the baseline (so the gate has a
        // deterministic pass).
        let edge = BridgeEdge {
            from: AtomId::new("свобода"),
            to: AtomId::new("предприимчивость"),
            rel_type: RelationType::RelRequires,
            topic: "свобода".into(),
            confidence: 0.9,
            co_occurrence: 5,
            weight: 0.9,
            source: BridgeEdgeSource::Promoted,
        };
        let mut store = RuntimeEdgeStore::new();
        store.insert((edge.from.clone(), edge.to.clone()), edge);
        db.save_bridge_edges(
            &session_id,
            Some(&qxfx0_bridge::encode_store(&store).unwrap()),
        )
        .unwrap();

        // Draft: a fresh overlay with exactly one admitted predicate.
        let (overlay, exclusions) = PromotionSurface::draft(&db, 100).unwrap();
        assert!(exclusions.is_empty(), "{exclusions:?}");
        assert_eq!(overlay.predicates.len(), 1);
        assert!(overlay.version.starts_with("overlay-"));
        assert_eq!(PromotionSurface::active(&db).unwrap(), None);

        // A Draft cannot release before activation.
        assert!(PromotionSurface::release(&db, &overlay.version, 101).is_err());

        // approve requires a passing evaluation bound to this exact content.
        assert!(
            PromotionSurface::approve(&db, &overlay.version, 102).is_err(),
            "activation without a prior precheck must fail"
        );

        // Draft -> evaluate -> approve -> release.
        let trial = PromotionSurface::evaluate(&db, &overlay.version, &[], 102).unwrap();
        assert!(
            trial.passed,
            "the admitted triple must survive its own precheck"
        );
        let activated = PromotionSurface::approve(&db, &overlay.version, 103).unwrap();
        assert_eq!(activated.status, qxfx0_bridge::OverlayStatus::Activated);
        let released = PromotionSurface::release(&db, &overlay.version, 104).unwrap();
        assert_eq!(released.status, qxfx0_bridge::OverlayStatus::Released);
        assert_eq!(
            PromotionSurface::active(&db).unwrap().as_deref(),
            Some(overlay.version.as_str())
        );

        // Release is permanent: a second release of the same version is
        // refused, and a second draft over unchanged evidence is a no-op
        // (content-addressed, no new row).
        assert!(PromotionSurface::release(&db, &overlay.version, 105).is_err());
        let (redraft, _) = PromotionSurface::draft(&db, 106).unwrap();
        assert_eq!(redraft.version, overlay.version);

        // Rollback retires the pointer (parent is none at the root) without
        // editing the released row.
        let target = PromotionSurface::rollback(&db, 107).unwrap();
        assert_eq!(target, None);
        assert_eq!(PromotionSurface::active(&db).unwrap(), None);
        // The released overlay remains in the journal, immutable.
        let journal = PromotionSurface::list(&db).unwrap();
        assert!(journal
            .iter()
            .any(|(version, status, _)| version == &overlay.version && status == "Released"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn test_promotion_draft_with_imports_mixes_evidence_idempotently() {
        use qxfx0_types::atom::{Atom, AtomCategory, AtomId};

        let path =
            std::env::temp_dir().join(format!("qxfx0-promotion-import-{}.db", std::process::id()));
        let db_path = path.to_string_lossy().to_string();
        let session_id = format!("promo-import-{}", std::process::id());
        let db = qxfx0_persistence::Persistence::open(&db_path).unwrap();
        let mut state = SystemState {
            session_id: session_id.clone(),
            ..SystemState::default()
        };
        state.dialogue.turn_count = 2;
        // The session graph grounds "свобода" and "выбор": both the runtime
        // edge (below) and the import row resolve against the same atoms.
        for (atom, category) in [
            ("свобода", AtomCategory::CatTopic),
            ("выбор", AtomCategory::CatConcept),
        ] {
            state.semantic.runtime_graph.atoms.insert(
                AtomId::new(atom),
                Atom {
                    id: AtomId::new(atom),
                    display: atom.into(),
                    category,
                },
            );
        }
        db.save_state(&session_id, &state).unwrap();

        let line_resolvable = r#"{"topic":"свобода","graph_atom_id":"свобода","predicates":[{"en":"freedom requires choice","kind":"rel","ru":"свобода требует выбор"}],"reasons":[]}"#;
        let fixture = format!("{line_resolvable}\nnot json at all\n");
        let report = PromotionSurface::draft_with_imports(&db, 200, Some(fixture.as_bytes()))
            .expect("mixed draft must not fail on one malformed line");
        assert_eq!(report.imported, 1, "the rel row must resolve");
        assert_eq!(
            report.import_refusals.len(),
            1,
            "the malformed line is refused"
        );
        assert_eq!(
            report.overlay.predicates.len(),
            1,
            "imported novel triple must clear an empty-topic baseline: {:?}",
            report.exclusions
        );
        assert!(report.overlay.version.starts_with("overlay-"));

        // Idempotent: the same bytes re-draft to the same version, with no
        // duplicate row and no duplicate predicate.
        let retry =
            PromotionSurface::draft_with_imports(&db, 201, Some(fixture.as_bytes())).unwrap();
        assert_eq!(retry.overlay.version, report.overlay.version);
        let listed = PromotionSurface::list(&db).unwrap();
        assert_eq!(
            listed
                .iter()
                .filter(|(version, _, _)| version == &report.overlay.version)
                .count(),
            1
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }
}
