use crate::config::validate_model;
use crate::games::bullshit_dict::BullshitDict;
use crate::models::xai::XaiClient;
use crate::results::{RunConfig, RunResult, Summary, TrialResult};
use anyhow::{bail, Result};
use chrono::Utc;
use std::fs;

/// Run Seven Wonders with per-seat player types.
///
/// Each spec is one of:
/// - `human-agent` — full agent context (agent.txt + cards + user.txt + log), terminal input
/// - `human` — minimal context (user.txt + log only), terminal input
/// - `auto` — first playable card in hand (no trades), else burn
/// - `<model-name>` — LLM agent via API (e.g. grok-4.3)
pub fn run_seven_wonders(
    player_count: u8,
    player_specs: &[String],
    max_rounds: Option<u32>,
) -> Result<()> {
    if player_specs.len() != player_count as usize {
        bail!(
            "Expected {} player specs (one per seat), got {}. \
             Example: seven-wonders 3 auto human grok-4.3",
            player_count,
            player_specs.len()
        );
    }

    use crate::games::seven_wonders::{
        controller::PlayerController, FirstPurchaseableController, HumanLogController, LLMController,
        run_game, run_limited_rounds_game,
    };

    let mut controllers: Vec<Box<dyn PlayerController>> = Vec::with_capacity(player_count as usize);

    for (i, spec) in player_specs.iter().enumerate() {
        let kind = spec.to_lowercase();
        let controller: Box<dyn PlayerController> = match kind.as_str() {
            "auto" => Box::new(FirstPurchaseableController),
            "human-agent" => Box::new(HumanLogController::as_agent(format!("player {}", i))),
            "human" => Box::new(HumanLogController::as_human(format!("player {}", i))),
            model => {
                validate_model(model)?;
                Box::new(LLMController::new(model.to_string()))
            }
        };
        controllers.push(controller);
    }

    println!("Starting Seven Wonders ({} players):", player_count);
    for (i, spec) in player_specs.iter().enumerate() {
        println!("  Player {}: {}", i, spec);
    }

    match max_rounds {
        Some(n) => run_limited_rounds_game(controllers, n),
        None => run_game(controllers),
    }

    Ok(())
}

pub fn run_benchmark(model: &str, benchmark: &str, publish: bool) -> Result<()> {
    validate_model(model)?;

    if benchmark.starts_with("seven-wonders") {
        bail!(
            "Use the seven-wonders subcommand instead.\n\
             Example: cargo run -- seven-wonders 3 auto human {}\n\
             Or:      just run seven-wonders 3 auto human {}",
            model, model
        );
    }

    if benchmark != "bullshit-dict" {
        anyhow::bail!(
            "Unknown benchmark '{}'. Implemented: bullshit-dict. \
             For Seven Wonders use: cargo run -- seven-wonders <N> <player-specs...>",
            benchmark
        );
    }

    println!("Loading bullshit-dict game...");
    let game = BullshitDict::load()?;
    let instructions = game.get_prompt()?;

    println!("Loaded {} trials", game.trials.len());

    let client = XaiClient::new()?;

    // Determine next run number from the *raw* directory
    let raw_dir = format!("results/raw/grok/{}/{}", model, benchmark);
    let next_run = get_next_run_number(&raw_dir)?;

    println!("Starting run #{} for model {} on {}", next_run, model, benchmark);

    let mut trial_results = Vec::new();

    for trial in &game.trials {
        let words_list = trial.words.join("\n");
        let user_prompt = format!(
            "{}\n\nHere are the 10 words:\n{}",
            instructions.trim(),
            words_list
        );

        println!("  Running trial {}...", trial.id);

        let (raw_response, latency_ms) = client.complete(model, &user_prompt, None, None)?;

        // Very simple parsing: look for yes or no (case insensitive)
        let cleaned = raw_response.trim().to_lowercase();
        let parsed_answer = if cleaned.contains("yes") {
            "yes".to_string()
        } else if cleaned.contains("no") {
            "no".to_string()
        } else {
            // Fallback: take first word
            cleaned.split_whitespace().next().unwrap_or("unknown").to_string()
        };

        let expected = if trial.has_fake { "yes" } else { "no" };
        let correct = parsed_answer == expected;

        trial_results.push(TrialResult {
            trial_id: trial.id.clone(),
            words: trial.words.clone(),
            prompt: user_prompt,
            raw_response,
            parsed_answer,
            correct,
            latency_ms,
        });
    }

    let correct_count = trial_results.iter().filter(|t| t.correct).count();
    let total = trial_results.len();
    let accuracy = correct_count as f64 / total as f64;

    let result = RunResult {
        schema_version: 1,
        run_number: next_run,
        model: model.to_string(),
        benchmark: benchmark.to_string(),
        provider: "grok".to_string(),
        timestamp: Utc::now(),
        config: RunConfig {
            temperature: 0.0,
            max_tokens: 32,
        },
        trials: trial_results,
        summary: Summary {
            total,
            correct: correct_count,
            accuracy,
        },
    };

    // Always write raw + update latest.json
    result.write_raw()?;

    if publish {
        result.publish_to_site()?;
    }

    println!(
        "\nRun complete. Accuracy: {:.1}% ({}/{})",
        accuracy * 100.0,
        correct_count,
        total
    );

    Ok(())
}

pub fn publish_latest(model: &str, benchmark: &str) -> Result<()> {
    crate::results::publish_latest_raw(model, benchmark)
}

fn get_next_run_number(raw_dir: &str) -> Result<u32> {
    fs::create_dir_all(raw_dir)?;

    let mut max_num = 0u32;

    for entry in fs::read_dir(raw_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if let Some(num_str) = name.strip_suffix(".json") {
            if let Ok(num) = num_str.parse::<u32>() {
                if num > max_num {
                    max_num = num;
                }
            }
        }
    }

    Ok(max_num + 1)
}