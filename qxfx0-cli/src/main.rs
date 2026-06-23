use clap::{Parser, Subcommand};

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
    /// Ask a single question (JSON output)
    Turn {
        #[arg(long)]
        json: bool,
        text: String,
    },
    /// Interactive dialogue session
    Chat,
    /// Run self-play enrichment
    Selfplay {
        #[arg(default_value = "10")]
        iterations: usize,
    },
    /// Discover relations for a concept via LLM
    Discover {
        concept: String,
    },
    /// Health check
    Doctor,
    /// Show version
    Version,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Turn { json: _, text } => {
            println!("QxFx0 Rust v0.1.0 — not yet implemented");
            println!("Input: {text}");
            Ok(())
        }
        Commands::Chat => {
            println!("QxFx0 interactive mode — not yet implemented");
            Ok(())
        }
        Commands::Selfplay { iterations } => {
            println!("Self-play: {iterations} iterations — not yet implemented");
            Ok(())
        }
        Commands::Discover { concept } => {
            println!("Discover: {concept} — not yet implemented");
            Ok(())
        }
        Commands::Doctor => {
            println!("Health check — not yet implemented");
            Ok(())
        }
        Commands::Version => {
            println!("QxFx0 Rust v0.1.0");
            Ok(())
        }
    }
}
