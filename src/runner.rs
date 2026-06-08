use crate::config::validate_model;
use crate::games::bullshit_dict::BullshitDict;
use crate::models::xai::XaiClient;
use crate::results::{RunConfig, RunResult, Summary, TrialResult};
use anyhow::Result;
use chrono::Utc;
use std::fs;

pub fn run_benchmark(model: &str, benchmark: &str, publish: bool) -> Result<()> {
    validate_model(model)?;

    if benchmark == "seven-wonders" {
        // Normal / interactive mode: 2 humans + 1 LLM with the given model, full game.
        println!("Starting Seven Wonders with 2 humans + 1 AI ({})", model);
        use crate::games::seven_wonders::{controller::PlayerController, HumanController, LLMController, run_game};
        let controllers: Vec<Box<dyn PlayerController>> = vec![
            Box::new(HumanController::new("Human1".to_string())),
            Box::new(HumanController::new("Human2".to_string())),
            Box::new(LLMController::new(model.to_string())),
        ];
        run_game(controllers);
        return Ok(());
    }

    if benchmark == "seven-wonders-smoke" {
        // Dedicated smoke test (per spec):
        // - 4 rounds total (via run_limited_rounds_game)
        // - Only the middle controller (player 1) is LLM using the given model; it receives the personalized log view each turn.
        // - Players 0 and 2 are FirstPurchaseableController (auto: first valid play with 0 trades). Their behavior is unchanged.
        // - The engine builds a single log.txt (full, with private decision blocks) + prints the exact views sent to the agent.
        // - Up to 3 retries + ERROR injection for the LLM on invalid actions.
        println!("Starting dedicated Seven Wonders smoke test (4 rounds): log-based LLM only for player 1; autos for 0+2 (unchanged)");
        use crate::games::seven_wonders::{controller::PlayerController, LLMController, FirstPurchaseableController, run_limited_rounds_game};
        let controllers: Vec<Box<dyn PlayerController>> = vec![
            Box::new(FirstPurchaseableController),           // player 0 auto
            Box::new(LLMController::new(model.to_string())), // player 1 = grok (gets log)
            Box::new(FirstPurchaseableController),           // player 2 auto
        ];
        run_limited_rounds_game(controllers, 4);
        return Ok(());
    }

    if benchmark != "bullshit-dict" {
        anyhow::bail!("Only 'bullshit-dict' and 'seven-wonders' benchmarks are implemented");
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