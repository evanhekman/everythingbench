use crate::config::validate_model;
use crate::games::bullshit_dict::BullshitDict;
use crate::models::xai::XaiClient;
use crate::games::seven_wonders::SevenWondersGameOutcome;
use crate::results::{
    build_seven_wonders_run_result, next_run_number, raw_dir, seven_wonders_run_model,
    LlmSeatStats, PersistableRun, RunConfig, RunResult, Summary, TrialResult,
    SEVEN_WONDERS_BENCHMARK,
};
use anyhow::{bail, Result};
use chrono::Utc;
use std::cell::RefCell;
use std::rc::Rc;

/// Run Seven Wonders with per-seat player types.
///
/// Each spec is one of:
/// - `human` — trimmed terminal player (user.txt + log); re-prompt until legal action
/// - `human-agent` — full LLM-equivalent context (system + agent + cards + log); terminal input
/// - `auto` — first playable card in hand (no trades), else burn
/// - `<model-name>` — LLM agent via API (e.g. grok-4.3)
pub fn run_seven_wonders(
    player_count: u8,
    player_specs: &[String],
    max_rounds: Option<u32>,
    publish: bool,
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
        controller::PlayerController, FirstPurchaseableController, GameState, HumanLogController,
        LLMController, run_limited_rounds_game_with_boards, term, WonderDatabase,
    };
    let mut llm_stats: Vec<Option<Rc<RefCell<LlmSeatStats>>>> =
        vec![None; player_count as usize];
    let mut controllers: Vec<Box<dyn PlayerController>> = Vec::with_capacity(player_count as usize);

    for (i, spec) in player_specs.iter().enumerate() {
        let kind = spec.to_lowercase();
        let controller: Box<dyn PlayerController> = match kind.as_str() {
            "auto" => Box::new(FirstPurchaseableController),
            "human-agent" => Box::new(HumanLogController::as_agent(format!("player {}", i))),
            "human" => Box::new(HumanLogController::as_human(format!("player {}", i))),
            model => {
                validate_model(model)?;
                let stats = Rc::new(RefCell::new(LlmSeatStats::default()));
                llm_stats[i] = Some(stats.clone());
                Box::new(LLMController::with_stats(model.to_string(), Some(stats)))
            }
        };
        controllers.push(controller);
    }

    let has_interactive_human = player_specs
        .iter()
        .any(|s| matches!(s.to_lowercase().as_str(), "human" | "human-agent"));
    if has_interactive_human {
        term::clear_screen();
    }

    println!("Starting Seven Wonders ({} players):", player_count);
    for (i, spec) in player_specs.iter().enumerate() {
        println!("  Player {}: {}", i, spec);
    }

    let wonder_db = WonderDatabase::load();
    let wonder_board_ids = wonder_db.assign_unique_random(player_count);
    let game = GameState::new_with_assignment(player_count, wonder_board_ids.clone());
    for (p, controller) in controllers.iter_mut().enumerate() {
        controller.print_startup_context(&game, p);
    }
    for (p, board_id) in wonder_board_ids.iter().enumerate() {
        println!(
            "  Player {} civilization: {}",
            p,
            wonder_db.display_name(board_id)
        );
    }

    let outcome = match max_rounds {
        Some(n) => run_limited_rounds_game_with_boards(controllers, n, Some(wonder_board_ids)),
        None => run_limited_rounds_game_with_boards(controllers, u32::MAX, Some(wonder_board_ids)),
    };

    write_seven_wonders_results(
        &seven_wonders_run_model(player_specs),
        player_specs,
        max_rounds,
        &outcome,
        &llm_stats,
        publish,
    )?;

    Ok(())
}

fn write_seven_wonders_results(
    model: &str,
    player_specs: &[String],
    max_rounds: Option<u32>,
    outcome: &SevenWondersGameOutcome,
    llm_stats: &[Option<Rc<RefCell<LlmSeatStats>>>],
    publish: bool,
) -> Result<()> {
    let scores: Vec<_> = if let Some(scores) = &outcome.final_scores {
        scores.clone()
    } else {
        vec![Default::default(); player_specs.len()]
    };

    let stats_snapshot: Vec<Option<LlmSeatStats>> = llm_stats
        .iter()
        .map(|opt| opt.as_ref().map(|s| s.borrow().clone()))
        .collect();

    let result = build_seven_wonders_run_result(
        model.to_string(),
        player_specs,
        max_rounds,
        outcome.rounds_played,
        outcome.game_complete,
        outcome.player_count,
        &scores,
        &stats_snapshot,
    )?;

    result.write_raw()?;

    if publish {
        result.publish_to_site()?;
    }

    println!(
        "\nLogged Seven Wonders run #{} for {} → results/raw/grok/{}/{}/",
        result.run_number,
        model,
        model,
        SEVEN_WONDERS_BENCHMARK
    );

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

    let raw_dir = raw_dir(crate::results::DEFAULT_PROVIDER, model, benchmark);
    let next_run = next_run_number(&raw_dir)?;

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

        let cleaned = raw_response.trim().to_lowercase();
        let parsed_answer = if cleaned.contains("yes") {
            "yes".to_string()
        } else if cleaned.contains("no") {
            "no".to_string()
        } else {
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
        provider: crate::results::DEFAULT_PROVIDER.to_string(),
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