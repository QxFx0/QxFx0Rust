use clap::{Parser, Subcommand, ValueEnum};
use qxfx0_cli::measurement::{run_renderer_diversity_audit, run_runtime_benchmark};
use qxfx0_cli::{
    append_turn_diagnostics, authority_report, create_anomaly_shadow_trace_sink,
    create_authority_trace_sink, create_cognitive_pilot_trace_sink, create_doubt_shadow_trace_sink,
    create_response_plan_v2_shadow_trace_sink, load_or_create_state, run_doctor,
    run_operational_metrics, run_turn_with_renderer_and_stance_provenance,
    run_turn_with_renderer_anomaly_shadow_trace, run_turn_with_renderer_cognitive_pilot,
    run_turn_with_renderer_diagnostics,
    run_turn_with_renderer_diagnostics_and_anomaly_shadow_trace,
    run_turn_with_renderer_diagnostics_and_cognitive_pilot,
    run_turn_with_renderer_diagnostics_and_doubt_shadow_trace,
    run_turn_with_renderer_doubt_shadow_trace, run_turn_with_v2_shadow_trace,
    verify_authority_trace, write_anomaly_shadow_trace_jsonl, write_authority_trace_jsonl,
    write_cognitive_pilot_trace_jsonl, write_doubt_shadow_trace_jsonl,
    write_response_plan_v2_shadow_trace_jsonl, AuthorityReportScope, DiagnosedTurn,
};
use qxfx0_pipeline::{
    process_turn_with_options, ClarificationMode, RendererAuthority, ResponsePlanV2Authority,
    SameTopicSuppressionMode, TurnOptions,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tracing::{debug, error, info, warn};

/// Shared flag set by the Ctrl+C handler so that long-running commands can
/// save state and exit gracefully.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

#[derive(Parser)]
#[command(name = "qxfx0")]
#[command(about = "Deterministic philosophical dialogue runtime")]
struct Cli {
    #[arg(long, default_value = "default", global = true)]
    session_id: String,

    #[arg(long, default_value = "qxfx0.db", global = true)]
    db: String,

    /// Render admitted audited content plans instead of the legacy graph path.
    #[arg(long, global = true)]
    render_audited_plan: bool,

    /// Force the legacy shadow renderer (explicit escape hatch; the default
    /// renderer is `audited_plan`). Mutually exclusive with `--render-audited-plan`.
    #[arg(long, global = true, conflicts_with = "render_audited_plan")]
    render_legacy: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, ValueEnum)]
enum ReportScope {
    All,
    Positive,
    Negative,
}

/// CLI surface of the B2 ablation arm (ADR-0043 U2). Mirrors
/// `qxfx0_pipeline::EssenceAblation` without leaking the crate type into
/// clap.
#[derive(Clone, Copy, ValueEnum)]
enum EssenceV2AblationArg {
    Enabled,
    CommitDisabled,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // clap owns the one-shot command payload
enum Commands {
    /// Ask a single question
    Turn {
        text: String,
        /// Append opt-in read-only per-turn timing evidence as JSONL.
        #[arg(long, value_name = "PATH")]
        diagnostics_jsonl: Option<PathBuf>,
        /// Write deterministic observation-only doubt evidence to a new JSONL file.
        /// This never changes routing, rendering, or persisted session state.
        #[arg(long, value_name = "PATH")]
        doubt_shadow_trace_jsonl: Option<PathBuf>,
        /// Write deterministic observation-only anomaly recovery evidence to a new JSONL file.
        /// This never applies a recovery strategy or changes persisted session state.
        #[arg(long, value_name = "PATH")]
        anomaly_shadow_trace_jsonl: Option<PathBuf>,
        #[arg(long, value_name = "PATH")]
        cognitive_pilot_trace_jsonl: Option<PathBuf>,
        /// Explicit V2 authority canary for the three-topic allowlist.
        #[arg(long, conflicts_with = "render_audited_plan")]
        response_plan_v2_authority: bool,
        /// Write the V2 authority receipt and pipeline trace to a new JSONL file.
        #[arg(long, requires = "response_plan_v2_authority", value_name = "PATH")]
        response_plan_v2_trace_jsonl: Option<PathBuf>,
        #[arg(long, requires = "response_plan_v2_authority")]
        authority_case_id: Option<String>,
        #[arg(long, requires = "response_plan_v2_authority")]
        authority_input_class: Option<String>,
        #[arg(long, requires = "response_plan_v2_authority")]
        authority_expected_result: Option<String>,
        #[arg(long, requires = "response_plan_v2_authority")]
        authority_expected_guard: Option<String>,
        /// Write observation-only V2 shadow evidence; V1 remains authoritative.
        #[arg(
            long,
            conflicts_with = "response_plan_v2_authority",
            value_name = "PATH"
        )]
        response_plan_v2_shadow_trace_jsonl: Option<PathBuf>,
        /// Default-off typed provenance recording; it never enables recovery.
        #[arg(long)]
        record_stance_provenance: bool,
        /// B2 Control-A ablation arm for the V2 subject core (ADR-0043 U2):
        /// `commit-disabled` suppresses the essence commitment while
        /// witnessing continues. Experiment surface only; the default is the
        /// law (`enabled`).
        #[arg(long, value_enum, default_value_t = EssenceV2AblationArg::Enabled)]
        essence_v2_ablation: EssenceV2AblationArg,
        #[arg(long, requires = "cognitive_pilot_trace_jsonl")]
        enable_clarification: bool,
        #[arg(long, requires_all = ["cognitive_pilot_trace_jsonl", "enable_clarification"])]
        enable_same_topic_suppression: bool,
    },
    /// Interactive dialogue session
    Chat,
    /// Run self-play enrichment
    Selfplay {
        #[arg(default_value = "10")]
        iterations: usize,
    },
    /// Discover relations for a concept
    Discover { concept: String },
    /// Health check
    Doctor {
        /// Emit a machine-readable JSON report
        #[arg(long)]
        json: bool,
        /// Run a named version-contract gate instead of the health check
        /// (response-plan-v2-phase-a | -b | -c | response-plan-v2-replay |
        /// response-plan-v2-zero-downgrade | response-plan-v2-canary-report),
        /// see ADR-0034
        #[arg(long)]
        gate: Option<String>,
    },
    /// Create a verified online SQLite backup
    Backup {
        /// New destination file; existing files are never overwritten
        destination: String,
    },
    /// Serve turns over a unix socket as a long-lived process (ADR-0043 U1):
    /// one JSON request per line, same journal sequence as `turn`, blobs
    /// initialized once per process instead of once per turn.
    Serve {
        /// Unix socket path to listen on
        #[arg(long, value_name = "PATH", default_value = "/tmp/qxfx0.sock")]
        socket: PathBuf,
    },
    /// Health, database-size and response-latency metrics
    Metrics {
        /// Emit JSON instead of Prometheus text format
        #[arg(long)]
        json: bool,
        /// Fail if DB + WAL + SHM exceed this many bytes
        #[arg(long, default_value_t = 1_073_741_824)]
        max_db_bytes: u64,
        /// Fail if the in-memory response probe exceeds this duration
        #[arg(long, default_value_t = 2_000)]
        max_response_ms: u64,
    },
    /// Measure first-turn and steady-state in-memory runtime latency
    Benchmark {
        #[arg(long, default_value_t = 100)]
        samples: usize,
        #[arg(long, default_value_t = 10)]
        warmup: usize,
        /// Measure the audited-plan renderer instead of the default legacy shadow.
        #[arg(long)]
        audited_plan: bool,
        #[arg(long)]
        json: bool,
    },
    /// Audit diversity of the authoritative audited-plan renderer
    RendererAudit {
        #[arg(long, default_value_t = 3)]
        opening_words: usize,
        #[arg(long)]
        json: bool,
    },
    /// List sessions
    Sessions,
    /// Кодекс: deterministic reflection prompt — topic of the day (or a
    /// given audited topic) with thesis, counterpoint and questions
    Reflect {
        /// Optional explicit audited topic; defaults to the topic of the day
        topic: Option<String>,
    },
    /// Кодекс: reflection protocol report over the session state
    Report {
        /// Emit Markdown (the journal artifact) instead of console text
        #[arg(long)]
        markdown: bool,
        /// Write the report to a new file; existing files are never overwritten
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },
    /// Кодекс: export the session as a verifiable diary — human Markdown
    /// with an embedded replay-verifiable manifest
    Export {
        /// Write the diary to a new file; existing files are never overwritten
        #[arg(long, value_name = "PATH")]
        out: PathBuf,
        /// Sign the manifest with HMAC-SHA256 over this passphrase
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Кодекс: verify a diary export by deterministic replay
    VerifyDiary {
        /// Path to the exported diary file
        path: PathBuf,
        /// Passphrase of the export's HMAC signature, when it carries one
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Show version
    Version,
    /// Code orchestration — find functions by natural language description
    Code {
        /// Natural language description of what you want to do
        query: String,
    },
    /// Code orchestration — show registry statistics
    CodeStats,
    /// Check structural consistency of one external authority trace JSONL artifact (not authenticity)
    VerifyAuthorityTrace { path: PathBuf },
    /// Aggregate structurally checked authority trace JSONL artifacts (not authenticated)
    AuthorityReport {
        #[arg(required = true, num_args = 1..)]
        paths: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = ReportScope::All)]
        scope: ReportScope,
    },
}

fn finish_diagnostics(
    mut diagnosed: DiagnosedTurn,
    path: &PathBuf,
    db_open_ms: u64,
    process_started: Instant,
) -> String {
    diagnosed.diagnostics.db_open_ms = db_open_ms;
    diagnosed.diagnostics.cli_process_ms = process_started
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    if let Err(error) = append_turn_diagnostics(path, &diagnosed.diagnostics) {
        warn!(
            "turn completed but diagnostic record could not be appended to {}: {error}",
            path.display()
        );
    }
    diagnosed.response
}

fn main() -> anyhow::Result<()> {
    let process_started = Instant::now();
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    // Default authority is `audited_plan` (ADR-0034/0041). It is ~7x faster at
    // p99 than legacy_shadow and produces the curated, fail-closed surface for
    // admitted topics. `--render-legacy` restores the legacy shadow renderer for
    // A/B comparison; `--render-audited-plan` is kept as an explicit no-op alias.
    let renderer_authority = if cli.render_legacy {
        RendererAuthority::LegacyShadow
    } else {
        RendererAuthority::AuditedPlan
    };

    ctrlc::set_handler(|| {
        SHUTDOWN.store(true, Ordering::SeqCst);
    })
    .map_err(|e| anyhow::anyhow!("Failed to set Ctrl+C handler: {}", e))?;

    match cli.command {
        Commands::Turn {
            text,
            diagnostics_jsonl,
            doubt_shadow_trace_jsonl,
            anomaly_shadow_trace_jsonl,
            cognitive_pilot_trace_jsonl,
            response_plan_v2_authority,
            response_plan_v2_trace_jsonl,
            authority_case_id,
            authority_input_class,
            authority_expected_result,
            authority_expected_guard,
            response_plan_v2_shadow_trace_jsonl,
            record_stance_provenance,
            enable_clarification,
            enable_same_topic_suppression,
            essence_v2_ablation,
        } => {
            debug!("Executing Turn command for session: {}", cli.session_id);
            let essence_v2_ablation = match essence_v2_ablation {
                EssenceV2AblationArg::Enabled => qxfx0_pipeline::EssenceAblation::Enabled,
                EssenceV2AblationArg::CommitDisabled => {
                    qxfx0_pipeline::EssenceAblation::CommitDisabled
                }
            };
            if let Some(path) = response_plan_v2_shadow_trace_jsonl {
                if diagnostics_jsonl.is_some()
                    || doubt_shadow_trace_jsonl.is_some()
                    || anomaly_shadow_trace_jsonl.is_some()
                    || cognitive_pilot_trace_jsonl.is_some()
                    || record_stance_provenance
                {
                    anyhow::bail!("response plan V2 shadow evidence requires a standalone turn");
                }
                let mut sink = create_response_plan_v2_shadow_trace_sink(&path)?;
                let db = qxfx0_persistence::Persistence::open(&cli.db)?;
                let traced = run_turn_with_v2_shadow_trace(&db, &cli.session_id, &text)?;
                write_response_plan_v2_shadow_trace_jsonl(&mut sink, &traced.trace)?;
                println!("{}", traced.response);
                return Ok(());
            }
            if record_stance_provenance
                && (diagnostics_jsonl.is_some()
                    || doubt_shadow_trace_jsonl.is_some()
                    || anomaly_shadow_trace_jsonl.is_some()
                    || cognitive_pilot_trace_jsonl.is_some())
            {
                anyhow::bail!(
                    "stance provenance recording currently requires a standalone ordinary turn"
                );
            }
            if response_plan_v2_authority {
                let mut sink = response_plan_v2_trace_jsonl
                    .as_ref()
                    .map(create_authority_trace_sink)
                    .transpose()?;
                let db = qxfx0_persistence::Persistence::open(&cli.db)?;
                let traced = qxfx0_cli::run_turn_with_v2_authority_trace(
                    &db,
                    &cli.session_id,
                    &text,
                    ResponsePlanV2Authority::Canary,
                )?;
                let mut traced = traced;
                traced.trace.set_authority_case_metadata(
                    authority_case_id.as_deref(),
                    authority_input_class.as_deref(),
                    authority_expected_result.as_deref(),
                    authority_expected_guard.as_deref(),
                );
                if let Some(sink) = sink.as_mut() {
                    write_authority_trace_jsonl(sink, &traced.trace)?;
                }
                println!("{}", traced.response);
                return Ok(());
            }
            if let Some(path) = cognitive_pilot_trace_jsonl {
                if doubt_shadow_trace_jsonl.is_some() || anomaly_shadow_trace_jsonl.is_some() {
                    anyhow::bail!(
                        "cognitive pilot, doubt shadow, and anomaly shadow traces require separate turns"
                    );
                }
                let mut sink = create_cognitive_pilot_trace_sink(&path)?;
                let clarification = if enable_clarification {
                    ClarificationMode::LimitedEnabled
                } else {
                    ClarificationMode::TraceOnly
                };
                let suppression = if enable_same_topic_suppression {
                    SameTopicSuppressionMode::LimitedEnabled
                } else {
                    SameTopicSuppressionMode::TraceOnly
                };
                let db_open_started = diagnostics_jsonl.as_ref().map(|_| Instant::now());
                let db = qxfx0_persistence::Persistence::open(&cli.db)?;
                let db_open_ms = db_open_started
                    .map(|started| started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
                let response = if let Some(diagnostics_path) = diagnostics_jsonl {
                    let (diagnosed, trace) =
                        run_turn_with_renderer_diagnostics_and_cognitive_pilot(
                            &db,
                            &cli.session_id,
                            &text,
                            renderer_authority,
                            clarification,
                            suppression,
                        )?;
                    write_cognitive_pilot_trace_jsonl(&mut sink, &trace)?;
                    finish_diagnostics(
                        diagnosed,
                        &diagnostics_path,
                        db_open_ms.expect("diagnostics path requires an open timer"),
                        process_started,
                    )
                } else {
                    let traced = run_turn_with_renderer_cognitive_pilot(
                        &db,
                        &cli.session_id,
                        &text,
                        renderer_authority,
                        clarification,
                        suppression,
                    )?;
                    write_cognitive_pilot_trace_jsonl(&mut sink, &traced.trace)?;
                    traced.response
                };
                println!("{}", response);
                return Ok(());
            }
            if let Some(path) = anomaly_shadow_trace_jsonl {
                if doubt_shadow_trace_jsonl.is_some() {
                    anyhow::bail!("doubt shadow and anomaly shadow traces require separate turns");
                }
                let mut sink = create_anomaly_shadow_trace_sink(&path)?;
                let db_open_started = diagnostics_jsonl.as_ref().map(|_| Instant::now());
                let db = qxfx0_persistence::Persistence::open(&cli.db)?;
                let db_open_ms = db_open_started
                    .map(|started| started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
                let response = if let Some(diagnostics_path) = diagnostics_jsonl {
                    let (diagnosed, trace) =
                        run_turn_with_renderer_diagnostics_and_anomaly_shadow_trace(
                            &db,
                            &cli.session_id,
                            &text,
                            renderer_authority,
                        )?;
                    write_anomaly_shadow_trace_jsonl(&mut sink, &trace)?;
                    finish_diagnostics(
                        diagnosed,
                        &diagnostics_path,
                        db_open_ms.expect("diagnostics path requires an open timer"),
                        process_started,
                    )
                } else {
                    let traced = run_turn_with_renderer_anomaly_shadow_trace(
                        &db,
                        &cli.session_id,
                        &text,
                        renderer_authority,
                    )?;
                    write_anomaly_shadow_trace_jsonl(&mut sink, &traced.trace)?;
                    traced.response
                };
                println!("{}", response);
                return Ok(());
            }
            // Open the trace artifact before the DB. An invalid or existing sink
            // therefore fails fast without processing or persisting a turn.
            let mut doubt_trace_sink = doubt_shadow_trace_jsonl
                .as_ref()
                .map(create_doubt_shadow_trace_sink)
                .transpose()?;
            let db_open_started = diagnostics_jsonl.as_ref().map(|_| Instant::now());
            let db = qxfx0_persistence::Persistence::open(&cli.db)?;
            let db_open_ms = db_open_started
                .map(|started| started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));

            info!(
                "Processing turn for session '{}' ({} chars)",
                cli.session_id,
                text.chars().count()
            );
            let response = match (diagnostics_jsonl, doubt_trace_sink.as_mut()) {
                (Some(path), Some(sink)) => {
                    let (diagnosed, trace) =
                        run_turn_with_renderer_diagnostics_and_doubt_shadow_trace(
                            &db,
                            &cli.session_id,
                            &text,
                            renderer_authority,
                            essence_v2_ablation,
                        )?;
                    write_doubt_shadow_trace_jsonl(sink, &trace)?;
                    finish_diagnostics(
                        diagnosed,
                        &path,
                        db_open_ms.expect("diagnostics path requires an open timer"),
                        process_started,
                    )
                }
                (Some(path), None) => {
                    let diagnosed = run_turn_with_renderer_diagnostics(
                        &db,
                        &cli.session_id,
                        &text,
                        renderer_authority,
                    )?;
                    finish_diagnostics(
                        diagnosed,
                        &path,
                        db_open_ms.expect("diagnostics path requires an open timer"),
                        process_started,
                    )
                }
                (None, Some(sink)) => {
                    let traced = run_turn_with_renderer_doubt_shadow_trace(
                        &db,
                        &cli.session_id,
                        &text,
                        renderer_authority,
                        essence_v2_ablation,
                    )?;
                    write_doubt_shadow_trace_jsonl(sink, &traced.trace)?;
                    traced.response
                }
                (None, None) => {
                    let unix_seconds = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_secs())
                        .unwrap_or(0);
                    let day = qxfx0_cli::codex::epoch_day(unix_seconds);
                    if record_stance_provenance {
                        run_turn_with_renderer_and_stance_provenance(
                            &db,
                            &cli.session_id,
                            &text,
                            renderer_authority,
                        )?
                    } else if essence_v2_ablation == qxfx0_pipeline::EssenceAblation::CommitDisabled
                    {
                        qxfx0_cli::run_journal_turn_with_essence_ablation(
                            &db,
                            &cli.session_id,
                            &text,
                            day,
                            renderer_authority,
                            essence_v2_ablation,
                        )?
                    } else {
                        qxfx0_cli::run_journal_turn(
                            &db,
                            &cli.session_id,
                            &text,
                            day,
                            renderer_authority,
                        )?
                    }
                }
            };

            println!("{}", response);
            debug!(
                "Response generated successfully for session: {}",
                cli.session_id
            );
            Ok(())
        }
        Commands::Chat => {
            debug!(
                "Entering interactive chat mode for session: {}",
                cli.session_id
            );
            let db = qxfx0_persistence::Persistence::open(&cli.db)?;
            let mut state = load_or_create_state(&db, &cli.session_id)?;

            println!(
                "QxFx0 Rust v{} — интерактивный режим",
                env!("CARGO_PKG_VERSION")
            );
            println!("Session: {}", cli.session_id);
            println!("Введите :quit для выхода\n");

            use std::io::{self, BufRead, Write};
            let stdin = io::stdin();
            let mut stdout = io::stdout();

            loop {
                if SHUTDOWN.load(Ordering::SeqCst) {
                    info!("Shutdown signal received, saving state and exiting chat");
                    qxfx0_cli::save_journal_state(&db, &cli.session_id, &mut state)?;
                    println!("\nState saved. Bye.");
                    break;
                }

                print!("> ");
                stdout.flush()?;
                let mut line = String::new();
                if stdin.lock().read_line(&mut line)? == 0 {
                    debug!("EOF detected, exiting chat loop");
                    break;
                }
                let line = line.trim();
                if line == ":quit" || line == ":q" {
                    debug!("Quit command received");
                    qxfx0_cli::save_journal_state(&db, &cli.session_id, &mut state)?;
                    println!("State saved. Bye.");
                    break;
                }
                if line.is_empty() {
                    continue;
                }

                info!("Processing chat turn ({} chars)", line.chars().count());
                let input = qxfx0_pipeline::TurnInput {
                    raw_text: line.to_string(),
                    session_id: cli.session_id.clone(),
                };
                let output = process_turn_with_options(
                    &input,
                    &mut state,
                    TurnOptions::new().with_renderer(renderer_authority),
                );

                debug!("Saving state for session: {}", cli.session_id);
                qxfx0_cli::save_journal_state(&db, &cli.session_id, &mut state)?;
                println!("{}\n", output.response);
            }

            debug!("Final state persistence for session: {}", cli.session_id);
            qxfx0_cli::save_journal_state(&db, &cli.session_id, &mut state)?;
            Ok(())
        }
        Commands::Selfplay { iterations } => {
            info!(
                "Starting self-play: {} iterations on session '{}'",
                iterations, cli.session_id
            );
            let db = qxfx0_persistence::Persistence::open(&cli.db)?;
            let mut state = load_or_create_state(&db, &cli.session_id)?;

            println!(
                "Self-play: {} iterations on session '{}'",
                iterations, cli.session_id
            );

            let seed_topics = [
                "что такое свобода?",
                "что ты думаешь об ответственности?",
                "как истина связана с красотой?",
                "что такое память?",
                "что ты думаешь о сознании?",
                "как свобода связана с волей?",
                "что такое справедливость?",
                "что ты думаешь о смерти?",
                "как язык связан с мышлением?",
                "что такое время?",
            ];

            for i in 0..iterations {
                if SHUTDOWN.load(Ordering::SeqCst) {
                    info!(
                        "Shutdown signal received during self-play at iteration {}, saving state",
                        i
                    );
                    qxfx0_cli::save_journal_state(&db, &cli.session_id, &mut state)?;
                    println!("\nSelf-play interrupted. State saved.");
                    break;
                }

                let topic = seed_topics[i % seed_topics.len()];
                debug!(
                    "Self-play iteration {}/{}: topic '{}'",
                    i + 1,
                    iterations,
                    topic
                );

                let input = qxfx0_pipeline::TurnInput {
                    raw_text: topic.to_string(),
                    session_id: cli.session_id.clone(),
                };
                let output = process_turn_with_options(
                    &input,
                    &mut state,
                    TurnOptions::new().with_renderer(renderer_authority),
                );
                qxfx0_cli::save_journal_state(&db, &cli.session_id, &mut state)?;
                println!("[{}/{}] {} → {}", i + 1, iterations, topic, output.response);
                println!();
            }

            info!(
                "Self-play complete for session '{}'. Turns: {}, Edges: {}",
                cli.session_id,
                state.dialogue.turn_count,
                state.semantic.runtime_graph.edges.len()
            );

            println!(
                "Self-play complete. Session '{}' now has {} turns, {} graph edges.",
                cli.session_id,
                state.dialogue.turn_count,
                state.semantic.runtime_graph.edges.len()
            );
            Ok(())
        }
        Commands::Discover { concept } => {
            info!("Discovering relations for concept: {}", concept);
            let graph = qxfx0_semantic::seed_graph();
            let atom_id = qxfx0_types::atom::AtomId::new(concept.to_lowercase());
            let rels = graph.relations_from(&atom_id);
            if rels.is_empty() {
                warn!("No relations found for concept: {}", concept);
                println!("No relations found for '{}' in the seed graph.", concept);
            } else {
                debug!("Found {} relations for concept: {}", rels.len(), concept);
                println!("Relations for '{}' ({}):", concept, rels.len());
                for rel in &rels {
                    println!(
                        "  {} → {} ({:?})",
                        rel.from.as_str(),
                        rel.to.as_str(),
                        rel.rel_type
                    );
                    println!("    {}", rel.ru_original);
                }
            }
            Ok(())
        }
        Commands::Doctor { json, gate } => {
            if let Some(name) = gate {
                // A named gate is a version contract, not a health check, so it
                // never opens the database and never mixes with health output.
                let Some(phase) = qxfx0_cli::response_plan_v2_gate::GatePhase::parse(&name) else {
                    return Err(anyhow::anyhow!(
                        "unknown gate '{name}'; expected response-plan-v2-phase-{{a,b,c}}, response-plan-v2-replay, response-plan-v2-zero-downgrade, or response-plan-v2-canary-report"
                    ));
                };
                info!(gate = %name, "Running version-contract gate");
                let report = qxfx0_cli::response_plan_v2_gate::run_gate(phase);
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!(
                        "[{}] {}: {}",
                        if report.passed { "OK" } else { "FAIL" },
                        report.gate,
                        report.details
                    );
                    for violation in &report.violations {
                        println!("  - {violation}");
                    }
                }
                return if report.passed {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("gate {} failed", report.gate))
                };
            }
            info!("Performing system health check");
            let report = run_doctor(&cli.db);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "version": env!("CARGO_PKG_VERSION"),
                        "healthy": report.is_healthy(),
                        "checks": &report.checks,
                    }))?
                );
            } else {
                println!("QxFx0 Rust v{} health check:", env!("CARGO_PKG_VERSION"));
                for check in &report.checks {
                    println!(
                        "  [{}] {}: {}",
                        if check.passed { "OK" } else { "FAIL" },
                        check.name,
                        check.details
                    );
                }
            }
            if report.is_healthy() {
                if !json {
                    println!("  Status: OK");
                }
                Ok(())
            } else {
                if !json {
                    println!("  Status: FAILED");
                }
                Err(anyhow::anyhow!("one or more health checks failed"))
            }
        }
        Commands::Backup { destination } => {
            info!("Creating online database backup");
            qxfx0_persistence::Persistence::backup_database(&cli.db, &destination)?;
            println!("Backup verified: {}", destination);
            Ok(())
        }
        Commands::Serve { socket } => {
            let listener = qxfx0_serve::bind(&socket)?;
            info!("serving turns on {}", socket.display());
            println!("listening: {}", socket.display());
            qxfx0_serve::serve(listener, &cli.db)?;
            Ok(())
        }
        Commands::Metrics {
            json,
            max_db_bytes,
            max_response_ms,
        } => {
            let metrics = run_operational_metrics(&cli.db);
            let violations = metrics.threshold_violations(max_db_bytes, max_response_ms);
            if json {
                println!("{}", serde_json::to_string_pretty(&metrics)?);
            } else {
                print!("{}", metrics.to_prometheus());
            }
            if violations.is_empty() {
                Ok(())
            } else {
                Err(anyhow::anyhow!(violations.join("; ")))
            }
        }
        Commands::Benchmark {
            samples,
            warmup,
            audited_plan,
            json,
        } => {
            let renderer = if audited_plan {
                RendererAuthority::AuditedPlan
            } else {
                RendererAuthority::LegacyShadow
            };
            let report =
                run_runtime_benchmark(samples, warmup, renderer).map_err(anyhow::Error::msg)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "first_turn={}us steady_p50={}us steady_p95={}us samples={}",
                    report.first_turn_micros,
                    report.steady_state_micros.p50,
                    report.steady_state_micros.p95,
                    report.steady_state_micros.samples
                );
            }
            Ok(())
        }
        Commands::RendererAudit {
            opening_words,
            json,
        } => {
            let report = run_renderer_diversity_audit(opening_words).map_err(anyhow::Error::msg)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "topics={} ready={} blocked={} unique_responses={} unique_openings={}",
                    report.audited_topics,
                    report.ready_plans,
                    report.blocked_topics.len(),
                    report.unique_responses,
                    report.unique_normalized_openings
                );
            }
            Ok(())
        }
        Commands::Sessions => {
            debug!("Listing all sessions from database: {}", cli.db);
            let db = qxfx0_persistence::Persistence::open(&cli.db)?;
            let sessions = db.list_sessions()?;
            if sessions.is_empty() {
                println!("No sessions found.");
            } else {
                for s in &sessions {
                    println!("  {}", s);
                }
                println!("\n{} session(s)", sessions.len());
            }
            Ok(())
        }
        Commands::Reflect { topic } => {
            // Reflection degrades gracefully: with a database and a session
            // it remembers the journal; without them it stays stateless and
            // never creates a file on a typo.
            let unix_seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            let day = qxfx0_cli::codex::epoch_day(unix_seconds);
            let journal_state = if std::path::Path::new(&cli.db).exists() {
                qxfx0_persistence::Persistence::open(&cli.db)
                    .ok()
                    .and_then(|db| db.load_state(&cli.session_id).ok().flatten())
            } else {
                None
            };
            let first_run = journal_state.is_none();
            let topic_name = match topic {
                Some(name) => name,
                None => qxfx0_cli::codex::select_topic_of_day(day, journal_state.as_ref()),
            };
            let card = match qxfx0_cli::codex::build_reflection_card(&topic_name, day) {
                Some(card) => card,
                None => {
                    return Err(anyhow::anyhow!(
                        "тема «{topic_name}» не входит в аудированные темы; вызовите `qxfx0 reflect` без аргумента для темы дня"
                    ))
                }
            };
            match journal_state {
                Some(state) => {
                    let memory = qxfx0_cli::codex::build_memory_card(card, &state, day);
                    print!("{}", qxfx0_cli::codex::render_memory_card(&memory));
                }
                None => {
                    print!("{}", qxfx0_cli::codex::render_reflection_card(&card));
                    if first_run {
                        print!(
                            "\nЭто может быть первая запись твоего дневника. Ответь одним предложением:\n  qxfx0 turn \"я думал о {}: ...\"\nЗавтра тема вернётся другой — а через неделю эта же, с памятью о твоём ответе.\n",
                            topic_name
                        );
                    }
                }
            }
            Ok(())
        }
        Commands::Report { markdown, out } => {
            // Read-only product surface: refuse to create a database on a
            // mistyped path instead of reporting on an empty fresh one.
            if !std::path::Path::new(&cli.db).exists() {
                return Err(anyhow::anyhow!(
                    "база данных не найдена: {} (report не создаёт новую базу)",
                    cli.db
                ));
            }
            let db = qxfx0_persistence::Persistence::open(&cli.db)?;
            let state = match db.load_state(&cli.session_id)? {
                Some(state) => state,
                None => {
                    return Err(anyhow::anyhow!(
                    "сессия «{}» не найдена в {}; начните с `qxfx0 --session-id {} turn \"...\"`",
                    cli.session_id,
                    cli.db,
                    cli.session_id
                ))
                }
            };
            let report = qxfx0_cli::codex::build_reflection_report(&state);
            let content = if markdown {
                qxfx0_cli::codex::render_report_markdown(&report)
            } else {
                qxfx0_cli::codex::render_report_console(&report)
            };
            match out {
                Some(path) => {
                    // Journal artifacts are never silently overwritten.
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .map_err(|error| {
                            anyhow::anyhow!(
                                "не удалось создать {}: {error} (существующие файлы не перезаписываются)",
                                path.display()
                            )
                        })?;
                    use std::io::Write;
                    file.write_all(content.as_bytes())?;
                    println!("Отчёт записан: {}", path.display());
                }
                None => print!("{content}"),
            }
            Ok(())
        }
        Commands::Export { out, passphrase } => {
            // Like `report`: an export never creates a database on a
            // mistyped path — the diary must come from a real session.
            if !std::path::Path::new(&cli.db).exists() {
                return Err(anyhow::anyhow!(
                    "база данных не найдена: {} (export не создаёт новую базу)",
                    cli.db
                ));
            }
            let db = qxfx0_persistence::Persistence::open(&cli.db)?;
            let state = match db.load_state(&cli.session_id)? {
                Some(state) => state,
                None => {
                    return Err(anyhow::anyhow!(
                        "сессия «{}» не найдена в {}",
                        cli.session_id,
                        cli.db
                    ))
                }
            };
            if state.dialogue.journal.is_empty() {
                return Err(anyhow::anyhow!(
                    "у сессии «{}» нет записанных ходов дневника: записи ведутся с этой версии, начните с `qxfx0 turn`",
                    cli.session_id
                ));
            }
            let mut export = qxfx0_cli::codex::build_diary_export(&state, renderer_authority);
            let signed = passphrase.is_some();
            if let Some(passphrase) = passphrase {
                qxfx0_cli::codex::append_diary_signature(
                    &mut export.markdown,
                    &export.manifest_json,
                    &passphrase,
                );
            }
            // Journal artifacts are never silently overwritten.
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&out)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "не удалось создать {}: {error} (существующие файлы не перезаписываются)",
                        out.display()
                    )
                })?;
            use std::io::Write;
            file.write_all(export.markdown.as_bytes())?;
            println!("Дневник записан: {}", out.display());
            println!(
                "Проверка: qxfx0 verify-diary {}{}",
                out.display(),
                if signed {
                    " --passphrase <фраза>"
                } else {
                    ""
                }
            );
            Ok(())
        }
        Commands::VerifyDiary { path, passphrase } => {
            let markdown = std::fs::read_to_string(&path).map_err(|error| {
                anyhow::anyhow!("не удалось прочитать {}: {error}", path.display())
            })?;
            let verification = qxfx0_cli::codex::verify_diary(&markdown, passphrase.as_deref());
            if verification.verified() {
                println!(
                    "Дневник подтверждён: сессия «{}», ходов {}, подпись {}",
                    verification.session_id,
                    verification.turns,
                    if verification.signature_checked {
                        "проверена"
                    } else {
                        "отсутствует"
                    }
                );
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "дневник НЕ прошёл проверку: {}",
                    verification
                        .failure
                        .unwrap_or_else(|| "неизвестная причина".into())
                ))
            }
        }
        Commands::Version => {
            println!("QxFx0 Rust v{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Commands::Code { query } => {
            info!("Orchestrating code query ({} chars)", query.chars().count());
            let graph = qxfx0_code::build_full_registry();
            let orch = qxfx0_code::CodeOrchestrator::new(graph);
            let result = match orch.orchestrate(&query) {
                Ok(result) => result,
                Err(e) => {
                    error!("Code orchestration failed: {}", e);
                    return Err(anyhow::Error::from(e).context("code orchestration failed"));
                }
            };
            debug!(
                "Orchestration successful: {} chains found",
                result.chain_count
            );
            println!("Query: {}\n", query);
            println!("{}", result.rendered);
            if !result.alternatives.is_empty() {
                println!("\nAlternatives:");
                for alt in &result.alternatives {
                    println!("  {}", alt);
                }
            }
            println!("\n({} chains found)", result.chain_count);
            Ok(())
        }
        Commands::CodeStats => {
            debug!("Generating code registry statistics");
            let graph = qxfx0_code::build_full_registry();
            let type_edges = graph
                .edges
                .iter()
                .filter(|e| e.rel_type == qxfx0_code::CodeRelationType::RelComposes)
                .count();
            println!("QxFx0 Code Registry stats:");
            println!("  Atoms: {}", graph.atoms.len());
            println!("  Relations: {}", graph.edges.len());
            println!("  Type-directed edges: {}", type_edges);
            let by_kind: std::collections::BTreeMap<_, _> =
                graph
                    .atoms
                    .values()
                    .fold(std::collections::BTreeMap::new(), |mut acc, a| {
                        *acc.entry(format!("{:?}", a.kind)).or_insert(0) += 1;
                        acc
                    });
            for (kind, count) in &by_kind {
                println!("    {}: {}", kind, count);
            }
            Ok(())
        }
        Commands::VerifyAuthorityTrace { path } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&verify_authority_trace(path)?)?
            );
            Ok(())
        }
        Commands::AuthorityReport { paths, scope } => {
            let scope = match scope {
                ReportScope::All => AuthorityReportScope::All,
                ReportScope::Positive => AuthorityReportScope::Positive,
                ReportScope::Negative => AuthorityReportScope::Negative,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&authority_report(paths, false, scope)?)?
            );
            Ok(())
        }
    }
}
