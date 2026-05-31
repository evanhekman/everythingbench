mod cli;
mod config;
mod games;
mod models;
mod results;
mod runner;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use runner::run_benchmark;

fn main() -> Result<()> {
    // Load .env if present (for XAI_API_KEY)
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    match cli.command {
        cli::Commands::Run { model, benchmark } => {
            run_benchmark(&model, &benchmark)?;
        }
    }

    Ok(())
}
