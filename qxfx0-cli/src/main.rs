use clap::{Parser, Subcommand};
use qxfx0_cli::{load_or_create_state, run_doctor, run_operational_metrics, run_turn};
use std::sync::atomic::{AtomicBool, Ordering};
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

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ask a single question
    Turn { text: String },
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
    },
    /// Create a verified online SQLite backup
    Backup {
        /// New destination file; existing files are never overwritten
        destination: String,
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
    /// List sessions
    Sessions,
    /// Show version
    Version,
    /// Code orchestration — find functions by natural language description
    Code {
        /// Natural language description of what you want to do
        query: String,
    },
    /// Code orchestration — show registry statistics
    CodeStats,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    ctrlc::set_handler(|| {
        SHUTDOWN.store(true, Ordering::SeqCst);
    })
    .map_err(|e| anyhow::anyhow!("Failed to set Ctrl+C handler: {}", e))?;

    match cli.command {
        Commands::Turn { text } => {
            debug!("Executing Turn command for session: {}", cli.session_id);
            let db = qxfx0_persistence::Persistence::open(&cli.db)?;

            info!(
                "Processing turn for session '{}' ({} chars)",
                cli.session_id,
                text.chars().count()
            );
            let response = run_turn(&db, &cli.session_id, &text)?;

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
                    db.save_state(&cli.session_id, &state)?;
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
                    db.save_state(&cli.session_id, &state)?;
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
                let output = qxfx0_pipeline::process_turn(&input, &mut state);

                debug!("Saving state for session: {}", cli.session_id);
                db.save_state(&cli.session_id, &state)?;
                println!("{}\n", output.response);
            }

            debug!("Final state persistence for session: {}", cli.session_id);
            db.save_state(&cli.session_id, &state)?;
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
                    db.save_state(&cli.session_id, &state)?;
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
                let output = qxfx0_pipeline::process_turn(&input, &mut state);
                db.save_state(&cli.session_id, &state)?;
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
        Commands::Doctor { json } => {
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
    }
}
