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

    /// Publish the latest raw run for a model + benchmark to the website
    Publish {
        /// Model name
        model: String,

        /// Benchmark / game
        benchmark: String,
    },
}