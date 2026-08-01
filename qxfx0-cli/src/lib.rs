//! QxFx0 cli crate — generated from Haskell specification.
//!
//! Exposes the same entry point used by `main.rs` so integration tests can
//! drive the turn / chat flow without spawning a subprocess.

pub mod measurement;

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
use serde::Serialize;
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

#[derive(Debug, Serialize)]
struct TraceRecord<'a> {
    schema: &'a str,
    trace: &'a qxfx0_pipeline::execution_trace::PipelineTrace,
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
    use std::process::Stdio;

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
