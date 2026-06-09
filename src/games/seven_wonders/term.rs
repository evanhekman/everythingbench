//! ANSI terminal styling for interactive human players (human / human-agent).
//! No-op when stdout is not a TTY (pipes, logs).

use super::state::GameState;
use std::io::{IsTerminal, Write};

const RESET: &str = "\x1b[0m";
const GRAY: &str = "\x1b[90m";
const RED: &str = "\x1b[1;31m";
const BOLD: &str = "\x1b[1m";

fn stdout_is_tty() -> bool {
    std::io::stdout().is_terminal()
}

fn wrap(code: &str, text: &str) -> String {
    if stdout_is_tty() {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

pub fn gray(text: &str) -> String {
    wrap(GRAY, text)
}

pub fn red(text: &str) -> String {
    wrap(RED, text)
}

pub fn bold(text: &str) -> String {
    wrap(BOLD, text)
}

/// ANSI foreground for Seven Wonders card colors.
fn card_color_code(color: &str) -> &'static str {
    match color {
        "brown" => "\x1b[38;5;130m",
        "grey" | "gray" => "\x1b[90m",
        "yellow" => "\x1b[33m",
        "blue" => "\x1b[34m",
        "red" => "\x1b[31m",
        "purple" => "\x1b[35m",
        "green" => "\x1b[32m",
        _ => "\x1b[37m",
    }
}

pub fn format_card_id(game: &GameState, card_id: &str) -> String {
    if !stdout_is_tty() {
        return card_id.to_string();
    }
    let color = game
        .card_db
        .get(card_id)
        .map(|c| c.color.as_str())
        .unwrap_or("white");
    format!("{}{}{RESET}", card_color_code(color), card_id)
}

fn pad_visible_field(display: &str, visible_len: usize, width: usize) -> String {
    if visible_len >= width {
        display.to_string()
    } else {
        format!("{display}{}", " ".repeat(width - visible_len))
    }
}

pub fn format_hand_list(game: &GameState, hand: &[String]) -> String {
    let rows = game.card_db.hand_display_rows(hand);
    if rows.is_empty() {
        return "[]".to_string();
    }
    if !stdout_is_tty() {
        return game.card_db.format_hand_rows(&rows);
    }
    let w_id = rows.iter().map(|r| r.id.len()).max().unwrap_or(0);
    let w_color = rows.iter().map(|r| r.color.len()).max().unwrap_or(0);
    let w_cost = rows.iter().map(|r| r.cost.len()).max().unwrap_or(0);
    let lines: Vec<String> = rows
        .iter()
        .map(|row| {
            let colored_id = format_card_id(game, &row.id);
            format!(
                "{}\t{}{}{}{}{}",
                pad_visible_field(&colored_id, row.id.len(), w_id),
                super::cards::pad_field(&row.color, w_color),
                super::cards::TAB_PAIR,
                super::cards::pad_field(&row.cost, w_cost),
                super::cards::TAB_PAIR,
                row.benefit
            )
        })
        .collect();
    format!("[\n{}\n]", lines.join("\n"))
}

fn colorize_hand_detail_line(game: &GameState, line: &str) -> Option<String> {
    let tab_start = line.find('\t')?;
    let id_field = &line[..tab_start];
    let card_id = id_field.trim_end();
    if game.card_db.get(card_id).is_none() {
        return None;
    }
    let padding = id_field.len() - card_id.len();
    let colored_id = format_card_id(game, card_id);
    Some(format!(
        "{}{}{}",
        colored_id,
        " ".repeat(padding),
        &line[tab_start..]
    ))
}

fn colorize_log_line(game: &GameState, player: usize, line: &str) -> String {
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("ERROR:") {
        return red(trimmed);
    }
    if trimmed == "hand: []" {
        return "hand: []".to_string();
    }
    if let Some(colored) = colorize_hand_detail_line(game, trimmed) {
        return colored;
    }
    if trimmed.starts_with("your_production:")
        || trimmed.starts_with("left (Player")
        || trimmed.starts_with("right (Player")
        || trimmed.starts_with("wonder stages")
        || trimmed.starts_with("wonder\t")
        || trimmed.starts_with("wonder ")
    {
        return gray(trimmed);
    }
    trimmed.to_string()
}

fn print_action_prompt(game: &GameState, bar: &str) {
    println!("{bar}");
    println!("Enter one action (play / wonder / burn). Examples:");
    println!("  play {}", format_card_id(game, "marketplace"));
    println!(
        "  play {} left:stone",
        format_card_id(game, "baths")
    );
    println!("  burn {}", format_card_id(game, "lumber_yard"));
}

fn print_body_lines(game: &GameState, player: usize, body: &str) {
    for line in body.lines() {
        if line.trim().is_empty() {
            println!();
        } else {
            println!("{}", colorize_log_line(game, player, line));
        }
    }
}

/// Trimmed view for `human`: user.txt + colored log only.
pub fn print_decision_screen(
    game: &GameState,
    player: usize,
    player_label: &str,
    user_txt: &str,
    log_view: &str,
) {
    let bar = "=".repeat(60);
    println!("\n{bar}");
    println!("{}", bold(&format!("YOUR TURN — {player_label} (player {player})")));
    println!("{bar}\n");

    for line in user_txt.lines() {
        if line.trim().is_empty() {
            println!();
        } else {
            println!("{}", gray(line));
        }
    }
    println!();
    println!(
        "{}",
        gray(&format!(
            "Current log context for your decision (player {player}):"
        ))
    );
    print_body_lines(game, player, log_view);
}

/// Wonder stage overview printed once at game start (`human` players).
pub fn print_wonder_startup_context(game: &GameState, player: usize) {
    println!("{}", gray(&game.format_wonder_stages_overview(player)));
    println!();
}

/// Static agent context printed once before the first round (`human-agent`).
pub fn print_human_agent_startup_context(
    game: &GameState,
    player: usize,
    system_prompt: &str,
    static_user_body: &str,
) {
    println!("{}", gray("=== SYSTEM PROMPT (from system_prompt.txt) ==="));
    print_body_lines(game, player, system_prompt);
    println!("{}", gray("=== END SYSTEM ===\n"));

    println!(
        "{}",
        gray("=== USER PROMPT (agent.txt + cards.txt + user.txt) ===")
    );
    print_body_lines(game, player, static_user_body);
    println!("{}", gray("=== END USER PROMPT (static) ===\n"));
}

/// Per-turn log slice for LLM agents (static prompts already shown at startup).
pub fn print_llm_turn_context(game: &GameState, player: usize, log_view: &str) {
    println!(
        "{}",
        gray(&format!(
            "=== LOG CONTEXT SENT TO PLAYER {player} (this turn only) ==="
        ))
    );
    print_body_lines(game, player, log_view);
    println!("{}", gray("=== END LOG CONTEXT ===\n"));
}

/// Per-turn log slice for `human-agent` (static prompts already shown at startup).
pub fn print_human_agent_turn_context(game: &GameState, player: usize, log_view: &str) {
    let bar = "=".repeat(60);
    println!(
        "{}",
        gray(&format!(
            "Current log context for your decision (player {player}):"
        ))
    );
    print_body_lines(game, player, log_view);
    print_action_prompt(game, &bar);
}

pub fn print_parse_ok(action: &str) {
    println!("{}", gray(&format!("Parsed: {action}")));
}

pub fn print_parse_fail(input: &str) {
    println!(
        "{}",
        gray(&format!(
            "Could not parse '{input}'. Try again (play / wonder / burn)."
        ))
    );
}

pub fn print_action_result(player: usize, result: &str) {
    println!("{}", gray(&format!("Player {player} result: {result}")));
}

pub fn print_error_line(message: &str) {
    let trimmed = message.trim();
    if trimmed.starts_with("ERROR:") {
        println!("{}", red(trimmed));
    } else {
        println!("{}", red(&format!("ERROR: {trimmed}")));
    }
}

pub fn flush_stdout() {
    let _ = std::io::stdout().flush();
}

/// Clear the terminal (hides `cargo run` build noise). No-op when stdout is not a TTY.
pub fn clear_screen() {
    if stdout_is_tty() {
        print!("\x1b[2J\x1b[H");
        let _ = std::io::stdout().flush();
    }
}