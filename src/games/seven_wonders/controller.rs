//! Player controllers for driving the game (human via terminal or LLM via API).

use super::actions::{SevenWondersAction, TerminalAction};
use super::state::GameState;
use super::term;
use std::io::{self, Write};

const PROMPTS_DIR: &str = "games/seven_wonders/prompts";

fn load_prompt_file(name: &str) -> String {
    let path = format!("{}/{}", PROMPTS_DIR, name);
    match std::fs::read_to_string(&path) {
        Ok(s) => s.trim().to_string(),
        Err(_) => format!("[MISSING PROMPT FILE: {}]", name),
    }
}

/// Load the player-count-specific card list (cards3.txt … cards7.txt).
fn load_cards_for_players(player_count: u8) -> String {
    let specific = format!("cards{}.txt", player_count);
    let content = load_prompt_file(&specific);
    if !content.starts_with("[MISSING") {
        return content;
    }
    load_prompt_file("cards.txt")
}

fn parse_resource(s: &str) -> Option<super::types::Resource> {
    match s {
        "wood" => Some(super::types::Resource::Wood),
        "stone" => Some(super::types::Resource::Stone),
        "ore" => Some(super::types::Resource::Ore),
        "clay" | "brick" => Some(super::types::Resource::Clay),
        "glass" => Some(super::types::Resource::Glass),
        "loom" | "cloth" => Some(super::types::Resource::Loom),
        "papyrus" | "paper" => Some(super::types::Resource::Papyrus),
        _ => None,
    }
}

fn parse_trades(tail: &str) -> Vec<super::types::Trade> {
    use super::types::{Neighbor, Trade};
    let mut out = vec![];
    for tok in tail.split_whitespace() {
        let t = tok.to_lowercase();
        let (side_part, res_part) = if let Some(p) = t.split_once(':') { p } else { continue };
        let neigh = match side_part {
            "left" | "l" => Neighbor::Left,
            "right" | "r" => Neighbor::Right,
            _ => continue,
        };
        if res_part.contains(':') {
            continue;
        }
        if let Some(res) = parse_resource(res_part) {
            out.push(Trade { from: neigh, resource: res });
        }
    }
    out
}

fn expand_action_shorthand(s: &str) -> String {
    let mut parts = s.split_whitespace();
    let Some(cmd) = parts.next() else {
        return s.to_string();
    };
    let rest: String = parts.collect::<Vec<_>>().join(" ");
    match cmd {
        "p" => format!("play {}", rest),
        "w" => format!("wonder {}", rest),
        "b" => format!("burn {}", rest),
        _ => s.to_string(),
    }
}

/// Parse one agent action line: `play baths`, `play baths left:stone`, `wonder altar`, `burn card`.
pub fn parse_agent_action_line(
    line: &str,
    hand: &[String],
    wonder_stages_built: u8,
) -> Option<SevenWondersAction> {
    let cleaned = line.trim().to_lowercase();
    if cleaned.is_empty() {
        return None;
    }
    let trades = if cleaned.contains(':') {
        parse_trades(&cleaned)
    } else {
        vec![]
    };
    let without_trades: String = expand_action_shorthand(
        &cleaned
            .split_whitespace()
            .filter(|t| !t.contains(':'))
            .collect::<Vec<_>>()
            .join(" "),
    );

    if without_trades.starts_with("play ") {
        let card = without_trades[5..].split_whitespace().next()?.to_string();
        if hand.contains(&card) {
            return Some(SevenWondersAction::Terminal(TerminalAction::PlayCard {
                card_id: card,
                trades,
            }));
        }
    } else if without_trades.starts_with("wonder ") {
        let card = without_trades[7..].split_whitespace().next()?.to_string();
        if hand.contains(&card) {
            return Some(SevenWondersAction::Terminal(TerminalAction::BuildWonder {
                card_id: card,
                stage: wonder_stages_built + 1,
                trades,
            }));
        }
    } else if without_trades.starts_with("burn ") {
        let card = without_trades[5..].split_whitespace().next()?.to_string();
        if hand.contains(&card) {
            return Some(SevenWondersAction::Terminal(TerminalAction::BurnCard { card_id: card }));
        }
    }
    None
}

/// How the game runner handles invalid or unparsed actions for this controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerDecisionMode {
    /// Picks only engine-valid actions (no trades). Invalid submissions are retried silently.
    Auto,
    /// Terminal human: re-prompt until the player submits a legal action. No auto-burn fallbacks.
    InteractiveHuman,
    /// LLM agent: limited retries with error injection, then forced burn fallback.
    LlmAgent,
}

pub trait PlayerController {
    fn decision_mode(&self) -> ControllerDecisionMode {
        ControllerDecisionMode::Auto
    }

    /// Whether this controller wants a rich log context string (instead of / in addition to raw GameState).
    /// LLM controllers return true so the runner supplies the personalized plain-text log view.
    /// Autos and humans return false and continue to drive directly from GameState (autos unchanged per spec).
    fn prefers_log_context(&self) -> bool {
        false
    }

    /// Run the decision loop for the player.
    /// `log_view`: when Some, this is the (personalized) log prefix + open decision block for this player.
    ///   Only LLM impls use it as their prompt content. Others may ignore it.
    /// Returns the chosen terminal action (or observe for humans).
    fn decide_action(&mut self, game: &GameState, player: usize, log_view: Option<&str>) -> SevenWondersAction;

    /// Print static prompt files once before the first round (`human-agent` only).
    fn print_startup_context(&self, _game: &GameState, _player: usize) {}
}

/// Simple human controller via terminal.
/// Uses the same tool/action loop.
pub struct HumanController {
    pub name: String,
}

impl HumanController {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl PlayerController for HumanController {
    fn decide_action(&mut self, game: &GameState, player: usize, _log_view: Option<&str>) -> SevenWondersAction {
        println!("\n=== {}'s turn (player {}) ===", self.name, player);
        let view = game.view_for_player(player);
        println!("Hand: {:?}", view.hand);
        println!("Coins: {}", view.coins);
        println!("Wonder stages: {}", view.wonder_stages_built);

        loop {
            println!("Choose: (o)bserve <tool>, (p)lay <card> [trades], (w)onder <card>, (b)urn <card>, (h)elp");
            print!("> ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let input = input.trim().to_lowercase();

            if input == "h" || input == "help" {
                println!("Tools: mycards, allcards, myresources, allresources, allmilitary, civilizations, mywonder, wonders, ageturn");
                println!("Actions: play <cardid> [e.g. left:wood], wonder <cardid>, burn <cardid>");
                continue;
            }

            if input.starts_with("o ") || input.starts_with("observe ") {
                let tool = input.split_whitespace().nth(1).unwrap_or("").to_lowercase();
                match tool.as_str() {
                    "mycards" => {
                        println!("Your played: {:?}", game.check_my_cards(player));
                    }
                    "allcards" => {
                        println!("All played (simplified view)");
                    }
                    "myc oins" | "myresources" => {
                        println!("Your coins: {}", game.check_my_coins(player));
                    }
                    "ageturn" => {
                        println!("Age: {}, Round: {}", game.current_age, game.round_in_age);
                    }
                    _ => println!("Unknown tool or not implemented: {}", tool),
                }
                continue;
            }

            // Terminal actions
            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            let cmd = parts[0];
            if cmd == "play" || cmd == "p" {
                if parts.len() < 2 {
                    println!("play <cardid>");
                    continue;
                }
                let card = parts[1].to_string();
                return SevenWondersAction::Terminal(TerminalAction::PlayCard { card_id: card, trades: vec![] });
            }
            if cmd == "wonder" || cmd == "w" {
                if parts.len() != 2 {
                    println!("wonder <cardid>   (exactly one card id; stage is always the next one)");
                    continue;
                }
                let card = parts[1].to_string();
                let stage: u8 = game.view_for_player(player).wonder_stages_built + 1;
                return SevenWondersAction::Terminal(TerminalAction::BuildWonder { card_id: card, stage, trades: vec![] });
            }
            if cmd == "burn" {
                if parts.len() < 2 {
                    println!("burn <cardid>");
                    continue;
                }
                let card = parts[1].to_string();
                return SevenWondersAction::Terminal(TerminalAction::BurnCard { card_id: card });
            }

            println!("Unknown");
        }
    }
}

/// LLM controller using the xAI API.
/// Now uses the rich personalized plain-text log (when the runner supplies log_view) as its entire context.
/// The log contains history + the open decision block with this player's private hand/coins/neighbors.
/// Always outputs exactly one of the 3 terminating actions (play/wonder/burn, with trades specified via repeated dir:res tokens).
pub struct LLMController {
    pub model: String,
    client: Option<crate::models::xai::XaiClient>,
}

impl LLMController {
    pub fn new(model: String) -> Self {
        let client = crate::models::xai::XaiClient::new().ok();
        Self { model, client }
    }
}

impl PlayerController for LLMController {
    fn decision_mode(&self) -> ControllerDecisionMode {
        ControllerDecisionMode::LlmAgent
    }

    fn prefers_log_context(&self) -> bool {
        true
    }

    fn decide_action(&mut self, game: &GameState, player: usize, log_view: Option<&str>) -> SevenWondersAction {
        let view = game.view_for_player(player);
        let hand = &view.hand;

        if hand.is_empty() {
            let fallback = SevenWondersAction::Terminal(TerminalAction::BurnCard { card_id: "none".to_string() });
            println!("[LLM p{}] fallback empty hand", player);
            return fallback;
        }

        // Build the prompt from the external prompt files + the dynamic log view.
        // system_prompt.txt is passed as the system message.
        // The user message combines agent.txt + cards.txt + user.txt (placeholder) + current log context.
        let system_prompt = load_prompt_file("system_prompt.txt");

        let user_prompt = if let Some(view) = log_view {
            format!(
                "{}\n\nOutput exactly one action line:",
                build_decision_context(game, player, view, true)
            )
        } else {
            // Legacy fallback (no log view supplied)
            format!(
                "You are playing 7 Wonders as player {}. Current hand: {:?}. Coins: {}. Choose one action: play <card>, wonder <card>, or burn <card>. Output only the action like 'play workshop' or 'burn theater'.",
                player, hand, view.coins
            )
        };

        // For visibility of exactly what the agent receives (as requested).
        // We show the composed user prompt + note that system_prompt.txt was also sent.
        if log_view.is_some() {
            println!("=== SYSTEM PROMPT (from system_prompt.txt) ===\n{}\n=== END SYSTEM ===", system_prompt);
            println!("=== FULL USER PROMPT SENT TO PLAYER {} (agent.txt + cards.txt + user.txt + log view) ===\n{}\n=== END USER PROMPT ===", player, user_prompt);
        }

        let mut decided: Option<SevenWondersAction> = None;

        if let Some(client) = &self.client {
            // Use higher max_tokens when we have a log view (response is still short, but safer).
            let max_toks = if log_view.is_some() { Some(80) } else { None };
            match client.complete(&self.model, &user_prompt, Some(&system_prompt), max_toks) {
                Ok((response, latency_ms)) => {
                    println!("[LLM p{}] raw ({}ms): {}", player, latency_ms, response);
                    let chosen = parse_agent_action_line(&response, hand, view.wonder_stages_built)
                        .or_else(|| {
                            hand.iter()
                                .find(|c| response.to_lowercase().contains(c.as_str()))
                                .map(|card| {
                                    SevenWondersAction::Terminal(TerminalAction::PlayCard {
                                        card_id: card.clone(),
                                        trades: vec![],
                                    })
                                })
                        });

                    if let Some(action) = chosen {
                        println!("[LLM p{}] parsed: {:?}", player, action);
                        decided = Some(action);
                    } else {
                        println!("[LLM p{}] unparsed '{}', fallback", player, response.trim());
                    }
                }
                Err(e) => {
                    println!("[LLM p{}] API error: {}. fallback", player, e);
                }
            }
        } else {
            println!("[LLM p{}] no API client, fallback", player);
        }

        if let Some(action) = decided {
            action
        } else {
            // Fallback: burn first card in hand
            let fallback = SevenWondersAction::Terminal(TerminalAction::BurnCard { card_id: hand[0].clone() });
            println!("[LLM p{}] fallback burn {}", player, hand[0]);
            fallback
        }
    }
}

/// Static portion of the LLM user message (agent + cards + user.txt), without log.
pub(crate) fn human_agent_static_user_body(game: &GameState) -> String {
    let agent_txt = load_prompt_file("agent.txt");
    let cards_txt = load_cards_for_players(game.player_count);
    let user_txt = load_prompt_file("user.txt");
    format!("{agent_txt}\n\n{cards_txt}\n\n{user_txt}")
}

pub(crate) fn system_prompt_text() -> String {
    load_prompt_file("system_prompt.txt")
}

/// Build the user-message body shown to a deciding player.
/// `full_agent_context`: include agent.txt + cards list (same as LLM user message).
fn build_decision_context(game: &GameState, player: usize, log_view: &str, full_agent_context: bool) -> String {
    let user_txt = load_prompt_file("user.txt");
    if full_agent_context {
        format!(
            "{}\n\nCurrent log context for your decision (player {}):\n{}\n",
            human_agent_static_user_body(game),
            player,
            log_view
        )
    } else {
        format!(
            "{}\n\nCurrent log context for your decision (player {}):\n{}\n",
            user_txt, player, log_view
        )
    }
}

/// Human at the terminal with log-based play.
pub struct HumanLogController {
    pub player_label: String,
    /// When true (`human-agent`), show system_prompt + full agent user message like an LLM.
    pub full_agent_context: bool,
}

impl HumanLogController {
    pub fn as_human(player_label: String) -> Self {
        Self {
            player_label,
            full_agent_context: false,
        }
    }

    pub fn as_agent(player_label: String) -> Self {
        Self {
            player_label,
            full_agent_context: true,
        }
    }
}

impl PlayerController for HumanLogController {
    fn decision_mode(&self) -> ControllerDecisionMode {
        ControllerDecisionMode::InteractiveHuman
    }

    fn prefers_log_context(&self) -> bool {
        true
    }

    fn print_startup_context(&self, game: &GameState, player: usize) {
        if self.full_agent_context {
            term::print_human_agent_startup_context(
                game,
                player,
                &system_prompt_text(),
                &human_agent_static_user_body(game),
            );
        }
    }

    fn decide_action(&mut self, game: &GameState, player: usize, log_view: Option<&str>) -> SevenWondersAction {
        let view = game.view_for_player(player);
        let hand = &view.hand;

        loop {
            if let Some(v) = log_view {
                if self.full_agent_context {
                    term::print_human_agent_turn_context(game, player, v);
                } else {
                    let user_txt = load_prompt_file("user.txt");
                    term::print_decision_screen(game, player, &self.player_label, &user_txt, v);
                }
            } else {
                println!("\n{}", term::bold(&format!(
                    "YOUR TURN — {} (player {})",
                    self.player_label, player
                )));
                println!("hand: {}", term::format_hand_list(game, hand));
            }

            print!("> ");
            term::flush_stdout();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            if let Some(action) = parse_agent_action_line(&input, hand, view.wonder_stages_built) {
                term::print_parse_ok(&format!("{action:?}"));
                return action;
            }
            term::print_parse_fail(input.trim());
        }
    }
}

/// Auto player: walks the hand in order, plays the first card the engine accepts
/// (no neighbor trades). Falls back to burning the first card in hand.
pub struct FirstPurchaseableController;

impl PlayerController for FirstPurchaseableController {
    fn decide_action(&mut self, game: &GameState, player: usize, _log_view: Option<&str>) -> SevenWondersAction {
        let hand = game.view_for_player(player).hand;
        for card in &hand {
            let play = TerminalAction::PlayCard {
                card_id: card.clone(),
                trades: vec![],
            };
            if game.is_valid_terminal_action(player, &play) {
                println!("[Auto p{}] play {}", player, card);
                return SevenWondersAction::Terminal(play);
            }
        }
        // fallback
        if let Some(card) = hand.first() {
            println!("[Auto p{}] fallback burn {}", player, card);
            return SevenWondersAction::Terminal(TerminalAction::BurnCard {
                card_id: card.clone(),
            });
        }
        SevenWondersAction::Terminal(TerminalAction::BurnCard {
            card_id: "none".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::seven_wonders::actions::TerminalAction;

    #[test]
    fn parse_accepts_single_letter_action_shorthand() {
        let hand = vec!["guard_tower".to_string(), "lumber_yard".to_string()];
        let play = parse_agent_action_line("p guard_tower", &hand, 0).unwrap();
        assert!(matches!(
            play,
            SevenWondersAction::Terminal(TerminalAction::PlayCard { ref card_id, .. })
                if card_id == "guard_tower"
        ));
        let wonder = parse_agent_action_line("w lumber_yard", &hand, 1).unwrap();
        assert!(matches!(
            wonder,
            SevenWondersAction::Terminal(TerminalAction::BuildWonder { stage: 2, .. })
        ));
        let burn = parse_agent_action_line("b lumber_yard", &hand, 0).unwrap();
        assert!(matches!(
            burn,
            SevenWondersAction::Terminal(TerminalAction::BurnCard { ref card_id })
                if card_id == "lumber_yard"
        ));
    }
}
