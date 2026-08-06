//! QxFx0 cli crate — generated from Haskell specification.
//!
//! Exposes the same entry point used by `main.rs` so integration tests can
//! drive the turn / chat flow without spawning a subprocess.

pub mod measurement;
pub mod response_plan_v2_gate;

use qxfx0_code::{build_full_registry, CodeOrchestrator};
use qxfx0_persistence::SaveStateTimings;
use qxfx0_pipeline::fact_grounded::FactGroundedRollout;
use qxfx0_pipeline::{
    process_turn, process_turn_with_renderer, process_turn_with_renderer_and_stance_provenance,
    process_turn_with_timing_and_renderer,
    process_turn_with_timing_trace_and_features_and_suppression,
    process_turn_with_timing_trace_and_renderer_and_anomaly_shadow,
    process_turn_with_timing_trace_and_renderer_and_doubt_shadow,
    process_turn_with_trace_and_renderer_and_anomaly_shadow,
    process_turn_with_trace_and_renderer_and_doubt_shadow,
    process_turn_with_trace_and_renderer_and_features_and_suppression, AnomalyShadowMode,
    ClarificationMode, DoubtShadowMode, PipelineStageTimings, RendererAuthority,
    SameTopicSuppressionMode, TurnInput,
};
use qxfx0_semantic::{argued_topic_registry, seed_graph};
use qxfx0_types::system_state::{SemanticState, SystemState};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::Write;
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

/// Run an ordinary persisted turn while collecting Debate Core evidence that
/// remains external to session state and has no response authority.
pub fn run_turn_with_debate_core_trace(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
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
            .with_renderer(renderer_authority)
            .with_debate_core(qxfx0_pipeline::DebateCoreMode::TraceOnly),
    );
    db.save_state(session_id, &state)?;
    Ok(AuthorityTracedTurn {
        response: output.response,
        trace,
    })
}

/// Run an ordinary persisted turn while collecting User Argument Parsing v1
/// evidence that remains external to session state and has no response authority.
pub fn run_turn_with_user_argument_trace(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
    renderer_authority: RendererAuthority,
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
            .with_renderer(renderer_authority)
            .with_user_argument_parser(qxfx0_pipeline::UserArgumentParserMode::TraceOnly),
    );
    db.save_state(session_id, &state)?;
    Ok(AuthorityTracedTurn {
        response: output.response,
        trace,
    })
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
    db.save_state(session_id, &state)?;
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

pub fn create_debate_core_trace_sink(path: impl AsRef<Path>) -> anyhow::Result<File> {
    create_trace_sink(path, "debate core")
}

pub fn create_user_argument_trace_sink(path: impl AsRef<Path>) -> anyhow::Result<File> {
    create_trace_sink(path, "user argument")
}

pub fn write_debate_core_trace_jsonl(
    sink: &mut File,
    trace: &qxfx0_pipeline::execution_trace::PipelineTrace,
) -> anyhow::Result<()> {
    let receipt = trace
        .debate_receipt
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("debate core trace has no observation receipt"))?;
    let mut record = serde_json::to_vec(&DebateCoreTraceRecord {
        schema: "qxfx0.debate-core-trace.v1",
        receipt,
    })?;
    record.push(b'\n');
    sink.write_all(&record)?;
    Ok(())
}

pub fn write_user_argument_trace_jsonl(
    sink: &mut File,
    trace: &qxfx0_pipeline::execution_trace::PipelineTrace,
) -> anyhow::Result<()> {
    let receipt = trace
        .user_argument_receipt
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("user argument trace has no parse receipt"))?;
    receipt
        .validate()
        .map_err(|error| anyhow::anyhow!("invalid user argument receipt: {error}"))?;
    let mut record = serde_json::to_vec(&UserArgumentTraceRecord {
        schema: "qxfx0.user-argument-parse-trace.v1",
        receipt,
    })?;
    record.push(b'\n');
    sink.write_all(&record)?;
    Ok(())
}

#[derive(Debug, Serialize)]
struct DebateCoreTraceRecord<'a> {
    schema: &'a str,
    receipt: &'a qxfx0_types::DebateObservationReceipt,
}

#[derive(Debug, Serialize)]
struct UserArgumentTraceRecord<'a> {
    schema: &'a str,
    receipt: &'a qxfx0_types::UserArgumentParseReceipt,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedDebateCoreTraceRecord {
    schema: String,
    receipt: qxfx0_types::DebateObservationReceipt,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedUserArgumentTraceRecord {
    schema: String,
    receipt: qxfx0_types::UserArgumentParseReceipt,
}

/// Verify one receipt-only Debate Core artifact and return its validated receipt.
pub fn verify_debate_core_trace(
    path: impl AsRef<Path>,
) -> anyhow::Result<qxfx0_types::DebateObservationReceipt> {
    let receipt = verify_receipt_artifact::<OwnedDebateCoreTraceRecord, _, _>(
        path.as_ref(),
        "debate core",
        "qxfx0.debate-core-trace.v1",
        |record| (record.schema, record.receipt),
    )?;
    receipt
        .validate()
        .map_err(|error| anyhow::anyhow!("invalid debate core receipt: {error}"))?;
    Ok(receipt)
}

/// Verify one receipt-only User Argument Parsing artifact.
pub fn verify_user_argument_trace(
    path: impl AsRef<Path>,
) -> anyhow::Result<qxfx0_types::UserArgumentParseReceipt> {
    let receipt = verify_receipt_artifact::<OwnedUserArgumentTraceRecord, _, _>(
        path.as_ref(),
        "user argument",
        "qxfx0.user-argument-parse-trace.v1",
        |record| (record.schema, record.receipt),
    )?;
    receipt
        .validate()
        .map_err(|error| anyhow::anyhow!("invalid user argument receipt: {error}"))?;
    Ok(receipt)
}

fn verify_receipt_artifact<T, R, F>(
    path: &Path,
    label: &str,
    expected_schema: &str,
    into_receipt: F,
) -> anyhow::Result<R>
where
    T: serde::de::DeserializeOwned,
    F: FnOnce(T) -> (String, R),
{
    const MAX_TRACE_BYTES: u64 = 1_048_576;
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_TRACE_BYTES {
        anyhow::bail!("{label} trace exceeds {MAX_TRACE_BYTES} bytes");
    }
    let source = std::fs::read_to_string(path)?;
    let mut records = source.lines().filter(|line| !line.trim().is_empty());
    let line = records
        .next()
        .ok_or_else(|| anyhow::anyhow!("{label} trace contains no records"))?;
    if records.next().is_some() {
        anyhow::bail!("{label} trace must contain exactly one record");
    }
    let record: T = serde_json::from_str(line)
        .map_err(|error| anyhow::anyhow!("invalid {label} trace: {error}"))?;
    let (schema, receipt) = into_receipt(record);
    if schema != expected_schema {
        anyhow::bail!("unsupported {label} trace schema '{schema}'");
    }
    Ok(receipt)
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
    #[serde(default, rename = "debate_receipt")]
    _debate_receipt: Option<serde_json::Value>,
    #[serde(default, rename = "user_argument_receipt")]
    _user_argument_receipt: Option<serde_json::Value>,
    authority_guard_classification: Option<String>,
    authority_case_id: Option<String>,
    authority_input_class: Option<String>,
    authority_expected_result: Option<String>,
    authority_expected_guard: Option<String>,
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
    let mut case_ids = std::collections::BTreeSet::new();
    for path in paths {
        artifact_count += 1;
        let source = std::fs::read_to_string(path.as_ref())?;
        for (index, line) in source.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: OwnedAuthorityTraceRecord = serde_json::from_str(line)
                .map_err(|error| anyhow::anyhow!("authority trace line {}: {error}", index + 1))?;
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
    let probe_output = process_turn(&probe_input, &mut probe_state);
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

    report
}

/// Build a freshly seeded `SystemState` for a given session id.
pub fn fresh_state(session_id: &str) -> SystemState {
    SystemState {
        session_id: session_id.to_string(),
        semantic: SemanticState {
            runtime_graph: seed_graph(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Load existing state for the session, or create a fresh one seeded with the
/// knowledge graph. Any persistence error other than "no such session row"
/// (which `Persistence::load_state` already maps to `Ok(None)`) is propagated
/// via `?` so the caller can surface it to the user.
pub fn load_or_create_state(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
) -> anyhow::Result<SystemState> {
    match db.load_state(session_id) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => Ok(fresh_state(session_id)),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
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
        let output = process_turn(&input, &mut self.state);

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

        self.db.save_state(&self.state.session_id, &self.state)?;
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

/// Run one turn with explicit authority for admitted content-plan rendering.
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
    let output = process_turn_with_renderer(&input, &mut state, renderer_authority);
    db.save_state(session_id, &state)?;
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
    db.save_state(session_id, &state)?;
    Ok(output.response)
}

/// Run one normal persisted turn while returning observation-only doubt
/// evidence for an external sink. The trace never enters `SystemState`.
pub fn run_turn_with_renderer_doubt_shadow_trace(
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
    let (output, trace) = process_turn_with_trace_and_renderer_and_doubt_shadow(
        &input,
        &mut state,
        renderer_authority,
        DoubtShadowMode::TraceOnly,
    );
    db.save_state(session_id, &state)?;
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
    let (output, trace) = process_turn_with_trace_and_renderer_and_anomaly_shadow(
        &input,
        &mut state,
        renderer_authority,
        AnomalyShadowMode::TraceOnly,
    );
    db.save_state(session_id, &state)?;
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
    let (output, trace) = process_turn_with_trace_and_renderer_and_features_and_suppression(
        &input,
        &mut state,
        renderer_authority,
        DoubtShadowMode::Disabled,
        clarification,
        suppression,
    );
    db.save_state(session_id, &state)?;
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
    let (output, pipeline) =
        process_turn_with_timing_and_renderer(&input, &mut state, renderer_authority);
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
pub fn run_turn_with_renderer_diagnostics_and_doubt_shadow_trace(
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
    let (output, pipeline, trace) = process_turn_with_timing_trace_and_renderer_and_doubt_shadow(
        &input,
        &mut state,
        renderer_authority,
        DoubtShadowMode::TraceOnly,
    );
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
    let (output, pipeline, trace) = process_turn_with_timing_trace_and_renderer_and_anomaly_shadow(
        &input,
        &mut state,
        renderer_authority,
        AnomalyShadowMode::TraceOnly,
    );
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
    let (output, pipeline, trace) = process_turn_with_timing_trace_and_features_and_suppression(
        &input,
        &mut state,
        renderer_authority,
        DoubtShadowMode::Disabled,
        clarification,
        suppression,
    );
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

    fn debate_trace_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("qxfx0-debate-{label}-{}.jsonl", std::process::id()))
    }

    #[test]
    fn debate_trace_verification_is_strict_and_tamper_evident() {
        let db = qxfx0_persistence::Persistence::open_memory().expect("open memory db");
        let traced = run_turn_with_debate_core_trace(
            &db,
            "debate-verification",
            "что такое свобода?",
            RendererAuthority::LegacyShadow,
        )
        .expect("debate turn");
        let path = debate_trace_path("verification");
        let _ = std::fs::remove_file(&path);
        let mut sink = create_debate_core_trace_sink(&path).expect("new debate sink");
        write_debate_core_trace_jsonl(&mut sink, &traced.trace).expect("write debate trace");
        drop(sink);
        let receipt = verify_debate_core_trace(&path).expect("valid trace verifies");
        assert_eq!(receipt.topic_id, "свобода");

        let original = std::fs::read_to_string(&path).unwrap();
        let mut tampered: serde_json::Value = serde_json::from_str(&original).unwrap();
        tampered["receipt"]["topic_id"] = serde_json::json!("произвол");
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&tampered).unwrap()),
        )
        .unwrap();
        assert!(verify_debate_core_trace(&path).is_err());

        let mut extended: serde_json::Value = serde_json::from_str(&original).unwrap();
        extended["receipt"]["raw_input"] = serde_json::json!("hidden");
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&extended).unwrap()),
        )
        .unwrap();
        let error = verify_debate_core_trace(&path).unwrap_err().to_string();
        assert!(
            error.contains("unknown field") && error.contains("raw_input"),
            "top-level unknown field must be rejected by name, got: {error}"
        );

        let mut nested: serde_json::Value = serde_json::from_str(&original).unwrap();
        nested["receipt"]["nodes"][0]["raw_text"] = serde_json::json!("hidden");
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&nested).unwrap()),
        )
        .unwrap();
        let error = verify_debate_core_trace(&path).unwrap_err().to_string();
        assert!(
            error.contains("unknown field") && error.contains("raw_text"),
            "nested unknown field must be rejected by name, got: {error}"
        );
        let _ = std::fs::remove_file(path);
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
    fn authority_report_counts_denial_before_render() {
        let db = qxfx0_persistence::Persistence::open_memory().expect("open memory db");
        let traced = run_turn_with_v2_authority_trace(
            &db,
            "authority-denial",
            "что такое истина?",
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
        assert_eq!(report.checks.len(), 11);
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "Performance diagnostics" && check.passed));
        let content_assets = report
            .checks
            .iter()
            .find(|check| check.name == "Content plan assets")
            .expect("content plan assets check");
        assert!(content_assets.details.contains("argued_topics_admitted=30"));
        assert!(content_assets
            .details
            .contains("content_predicates_total=69"));
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
}
