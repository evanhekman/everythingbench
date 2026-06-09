use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "everythingbench", version, about = "LLM board game benchmark runner")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a benchmark for a specific model
    Run {
        /// Model name (must be in the known allowlist)
        model: String,

        /// Benchmark / game to run (e.g. bullshit-dict)
        benchmark: String,

        /// Publish this run to the website after completion
        #[arg(long)]
        publish: bool,
    },

    /// Run a Seven Wonders game with configurable players.
    /// Each player is: human, human-agent (full LLM context), auto, or an LLM model name.
    ///
    /// Example: seven-wonders 3 auto human-agent grok-4.3
    SevenWonders {
        /// Number of players (3–7)
        #[arg(value_parser = clap::value_parser!(u8).range(3..=7))]
        player_count: u8,

        /// One spec per player: human | human-agent | auto | <model-name>
        #[arg(num_args = 1..)]
        players: Vec<String>,

        /// Stop after this many rounds (default: full game)
        #[arg(long)]
        rounds: Option<u32>,

        /// Publish this run to the website after completion
        #[arg(long)]
        publish: bool,
    },

    /// Publish the latest raw run for a model + benchmark to the website
    Publish {
        /// Model name
        model: String,

        /// Benchmark / game
        benchmark: String,
    },
}