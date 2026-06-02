//! Player controllers for driving the game (human via terminal or LLM via API).

use super::state::GameState;
use super::actions::{TerminalAction, SevenWondersAction};
use std::io::{self, Write};

pub trait PlayerController {
    /// Run the decision loop for the player: can do multiple observations, then one terminal action.
    /// Returns the terminal action chosen.
    fn decide_action(&mut self, game: &GameState, player: usize) -> SevenWondersAction;
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
    fn decide_action(&mut self, game: &GameState, player: usize) -> SevenWondersAction {
        println!("\n=== {}'s turn (player {}) ===", self.name, player);
        let view = game.view_for_player(player);
        println!("Hand: {:?}", view.hand);
        println!("Coins: {}", view.coins);
        println!("Wonder stages: {}", view.wonder_stages_built);

        loop {
            println!("Choose: (o)bserve <tool>, (p)lay <card> [trades], (w)onder <card> <stage>, (b)urn <card>, (h)elp");
            print!("> ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let input = input.trim().to_lowercase();

            if input == "h" || input == "help" {
                println!("Tools: mycards, allcards, myresources, allresources, allmilitary, civilizations, mywonder, wonders, ageturn");
                println!("Actions: play <cardid> [e.g. left:wood], wonder <cardid> <stage>, burn <cardid>");
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
                if parts.len() < 3 {
                    println!("wonder <cardid> <stage>");
                    continue;
                }
                let card = parts[1].to_string();
                let stage: u8 = parts[2].parse().unwrap_or(1);
                return SevenWondersAction::Terminal(TerminalAction::BuildWonder { card_id: card, stage, trades: vec![] });
            }
            if cmd == "burn" || cmd == "b" {
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
/// For smoke test, uses a simple prompt with current hand and asks for action.
/// In full, would support the full observation loop by multiple calls.
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
    fn decide_action(&mut self, game: &GameState, player: usize) -> SevenWondersAction {
        let view = game.view_for_player(player);
        let hand = &view.hand;

        println!("\n=== LLM {}'s turn (player {}) ===", self.model, player);
        println!("Hand: {:?}", hand);
        println!("Coins: {}", view.coins);
        println!("Wonder stages: {}", view.wonder_stages_built);

        if hand.is_empty() {
            let fallback = SevenWondersAction::Terminal(TerminalAction::BurnCard { card_id: "none".to_string() });
            println!("LLM fallback (empty hand): {:?}", fallback);
            return fallback;
        }

        // Basic: one shot prompt. For full agentic, would loop with tool results.
        let prompt = format!(
            "You are playing 7 Wonders as player {}. Current hand: {:?}. Coins: {}. Choose one action: play <card>, wonder <card> <stage>, or burn <card>. Output only the action like 'play workshop' or 'burn theater'.",
            player, hand, view.coins
        );

        let mut decided: Option<SevenWondersAction> = None;

        if let Some(client) = &self.client {
            match client.complete(&self.model, &prompt, None) {
                Ok((response, latency_ms)) => {
                    println!("LLM raw response ({}ms): {}", latency_ms, response);
                    let cleaned = response.trim().to_lowercase();
                    let chosen = if cleaned.starts_with("play ") {
                        let card = cleaned[5..].trim().to_string();
                        if hand.contains(&card) {
                            Some(SevenWondersAction::Terminal(TerminalAction::PlayCard { card_id: card, trades: vec![] }))
                        } else { None }
                    } else if cleaned.starts_with("wonder ") {
                        let parts: Vec<&str> = cleaned[7..].trim().split_whitespace().collect();
                        if parts.len() >= 2 {
                            let card = parts[0].to_string();
                            let stage: u8 = parts[1].parse().unwrap_or(1);
                            if hand.contains(&card) {
                                Some(SevenWondersAction::Terminal(TerminalAction::BuildWonder { card_id: card, stage, trades: vec![] }))
                            } else { None }
                        } else { None }
                    } else if cleaned.starts_with("burn ") {
                        let card = cleaned[5..].trim().to_string();
                        if hand.contains(&card) {
                            Some(SevenWondersAction::Terminal(TerminalAction::BurnCard { card_id: card }))
                        } else { None }
                    } else if let Some(card) = hand.iter().find(|c| cleaned.contains(*c)) {
                        // Lenient fallback: if the response mentions any card id from hand, play it.
                        Some(SevenWondersAction::Terminal(TerminalAction::PlayCard { card_id: card.clone(), trades: vec![] }))
                    } else {
                        None
                    };

                    if let Some(action) = chosen {
                        println!("LLM parsed: {:?}", action);
                        decided = Some(action);
                    } else {
                        println!("LLM response not understood (cleaned: '{}'), will fallback.", cleaned);
                    }
                }
                Err(e) => {
                    println!("LLM API error: {}. Will use fallback.", e);
                }
            }
        } else {
            println!("LLM has no API client (missing XAI_API_KEY?), using fallback.");
        }

        if let Some(action) = decided {
            action
        } else {
            // Fallback: burn first card in hand
            let fallback = SevenWondersAction::Terminal(TerminalAction::BurnCard { card_id: hand[0].clone() });
            println!("LLM fallback chose: {:?}", fallback);
            fallback
        }
    }
}
