use clap::{Parser, Subcommand};
use qxfx0_semantic::{seed_graph, ContextualComposer, PropositionParser, GraphEngagement, PropositionMode};
use qxfx0_types::atom::AtomId;
use qxfx0_types::field::FieldProfile;

#[derive(Parser)]
#[command(name = "qxfx0")]
#[command(about = "Deterministic philosophical dialogue runtime")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, default_value = "default")]
    session_id: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Ask a single question
    Turn { text: String },
    /// Interactive dialogue session
    Chat,
    /// Run self-play enrichment
    Selfplay { #[arg(default_value = "10")] iterations: usize },
    /// Discover relations for a concept
    Discover { concept: String },
    /// Health check
    Doctor,
    /// Show version
    Version,
}

fn process_turn(input: &str) -> String {
    let graph = seed_graph();
    let fp = FieldProfile::default();

    // Parse proposition
    let prop = PropositionParser::parse(input);

    // Engage with graph
    let engagement = GraphEngagement::engage(&graph, &prop);

    // Compose response
    let surface = ContextualComposer::compose(&graph, &fp, &prop, &engagement);

    if surface.text.is_empty() {
        format!("Я не нахожу достаточного материала по теме «{}».", prop.subject)
    } else {
        // Add authority prefix based on mode
        match prop.mode {
            PropositionMode::Define => {
                format!("Известно, что {}", surface.text)
            }
            _ => surface.text,
        }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Turn { text } => {
            let response = process_turn(&text);
            println!("{}", response);
            Ok(())
        }
        Commands::Chat => {
            println!("QxFx0 Rust v0.1.0 — интерактивный режим");
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
                    println!("State saved. Bye.");
                    break;
                }
                if line.is_empty() {
                    continue;
                }
                let response = process_turn(line);
                println!("{}\n", response);
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
            println!("  Seed graph: {} atoms, {} relations", graph.atoms.len(), graph.edges.len());
            println!("  Relation types: {}", qxfx0_types::RelationType::ALL.len());
            println!("  Covered topics: {}", qxfx0_semantic::COVERED_TOPICS.len());
            println!("  Status: OK");
            Ok(())
        }
        Commands::Version => {
            println!("QxFx0 Rust v0.1.0");
            Ok(())
        }
    }
}
