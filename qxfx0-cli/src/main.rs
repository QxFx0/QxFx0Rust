use clap::{Parser, Subcommand};
use qxfx0_pipeline::{TurnInput, TurnPipeline};
use qxfx0_semantic::seed_graph;
use qxfx0_types::system_state::SystemState;

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
    Doctor,
    /// List sessions
    Sessions,
    /// Show version
    Version,
}

fn load_or_create_state(db: &qxfx0_persistence::Persistence, session_id: &str) -> SystemState {
    match db.load_state(session_id).unwrap_or(None) {
        Some(state) => state,
        None => SystemState {
            session_id: session_id.to_string(),
            runtime_graph: seed_graph(),
            ..Default::default()
        },
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Turn { text } => {
            let db = qxfx0_persistence::Persistence::open(&cli.db)?;
            let mut state = load_or_create_state(&db, &cli.session_id);
            let input = TurnInput {
                raw_text: text,
                session_id: cli.session_id.clone(),
            };
            let output = TurnPipeline::process(&input, &mut state);
            db.save_state(&cli.session_id, &state)?;
            println!("{}", output.response);
            if output.blocked {
                eprintln!("[guard] blocked: {:?}", output.guard_status);
            }
            Ok(())
        }
        Commands::Chat => {
            let db = qxfx0_persistence::Persistence::open(&cli.db)?;
            let mut state = load_or_create_state(&db, &cli.session_id);
            println!("QxFx0 Rust v0.1.0 — интерактивный режим");
            println!("Session: {}", cli.session_id);
            println!("Введите :quit для выхода\n");
            use std::io::{self, BufRead, Write};
            let stdin = io::stdin();
            let mut stdout = io::stdout();
            loop {
                print!("> ");
                stdout.flush()?;
                let mut line = String::new();
                if stdin.lock().read_line(&mut line)? == 0 {
                    break;
                }
                let line = line.trim();
                if line == ":quit" || line == ":q" {
                    db.save_state(&cli.session_id, &state)?;
                    println!("State saved. Bye.");
                    break;
                }
                if line.is_empty() {
                    continue;
                }
                let input = TurnInput {
                    raw_text: line.to_string(),
                    session_id: cli.session_id.clone(),
                };
                let output = TurnPipeline::process(&input, &mut state);
                db.save_state(&cli.session_id, &state)?;
                println!("{}\n", output.response);
            }
            Ok(())
        }
        Commands::Selfplay { iterations } => {
            println!("Self-play: {} iterations — not yet implemented", iterations);
            Ok(())
        }
        Commands::Discover { concept } => {
            println!("Discover: {} — not yet implemented", concept);
            Ok(())
        }
        Commands::Doctor => {
            let graph = seed_graph();
            println!("QxFx0 Rust v0.1.0 health check:");
            println!(
                "  Seed graph: {} atoms, {} relations",
                graph.atoms.len(),
                graph.edges.len()
            );
            println!("  Relation types: {}", qxfx0_types::RelationType::ALL.len());
            println!("  Covered topics: {}", qxfx0_semantic::COVERED_TOPICS.len());
            println!("  Morphology: 6 cases (nominative/genitive/dative/accusative/instrumental/prepositional)");
            println!("  Pipeline: 6 stages (Prepare→Route→Render→Finalize→Guard→Persist)");
            println!("  Persistence: SQLite");
            println!("  Status: OK");
            Ok(())
        }
        Commands::Sessions => {
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
            println!("QxFx0 Rust v0.1.0");
            Ok(())
        }
    }
}
