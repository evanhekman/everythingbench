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
        cli::Commands::Run { model, benchmark, publish } => {
            run_benchmark(&model, &benchmark, publish)?;
        }
        cli::Commands::SevenWonders {
            player_count,
            players,
            rounds,
        } => {
            runner::run_seven_wonders(player_count, &players, rounds)?;
        }
        cli::Commands::Publish { model, benchmark } => {
            runner::publish_latest(&model, &benchmark)?;
        }
    }

    Ok(())
}
