//! Player controllers for driving the game (human via terminal or LLM via API).

use super::state::GameState;
use super::actions::{TerminalAction, SevenWondersAction};
use std::io::{self, Write};

const PROMPTS_DIR: &str = "games/seven_wonders/prompts";

fn load_prompt_file(name: &str) -> String {
    let path = format!("{}/{}", PROMPTS_DIR, name);
    match std::fs::read_to_string(&path) {
        Ok(s) => s.trim().to_string(),
        Err(_) => format!("[MISSING PROMPT FILE: {}]", name),
    }
}

pub trait PlayerController {
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

    /// Parse a resource name (lowercased id like "wood", "papyrus").
    fn parse_res(s: &str) -> Option<super::types::Resource> {
        match s {
            "wood" => Some(super::types::Resource::Wood),
            "stone" => Some(super::types::Resource::Stone),
            "ore" => Some(super::types::Resource::Ore),
            "clay" => Some(super::types::Resource::Clay),
            "glass" => Some(super::types::Resource::Glass),
            "loom" => Some(super::types::Resource::Loom),
            "papyrus" => Some(super::types::Resource::Papyrus),
            _ => None,
        }
    }

    /// Parse trades from the tail of a model response using dir:resource notation.
    /// Repeat the token for multiple units, e.g. "left:wood right:stone left:stone".
    /// Only simple "dir:res" form is supported (no :n count suffix).
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
            // Only support simple dir:res; if res_part contains ":", it's old :n form -> skip to fail fast
            if res_part.contains(':') {
                continue;
            }
            if let Some(res) = Self::parse_res(res_part) {
                out.push(Trade { from: neigh, resource: res });
            }
        }
        out
    }
}

impl PlayerController for LLMController {
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
        let agent_txt = load_prompt_file("agent.txt");
        let cards_txt = load_prompt_file("cards.txt");
        let user_txt = load_prompt_file("user.txt");

        let user_prompt = if let Some(view) = log_view {
            // The view contains the open decision block with private info + any retry errors.
            // Prepend the static agent instructions, cards reference, and user placeholder.
            format!(
                "{}\n\n{}\n\n{}\n\nCurrent log context for your decision (player {}):\n{}\n\nOutput exactly one action line:",
                agent_txt, cards_txt, user_txt, player, view
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
                    let cleaned = response.trim().to_lowercase();

                    // Parse action + optional trailing trades.
                    let (cmd, tail) = if let Some(sp) = cleaned.find(' ') {
                        (&cleaned[..sp], cleaned[sp..].trim())
                    } else {
                        (cleaned.as_str(), "")
                    };
                    let trades = Self::parse_trades(tail);

                    let chosen = if cmd == "play" || cleaned.starts_with("play ") {
                        // card is the token after "play "
                        let after = if cleaned.starts_with("play ") { &cleaned[5..] } else { &cleaned[4..] };
                        let card = after.split_whitespace().next().unwrap_or("").to_string();
                        if hand.contains(&card) {
                            Some(SevenWondersAction::Terminal(TerminalAction::PlayCard { card_id: card, trades }))
                        } else { None }
                    } else if cmd == "wonder" || cleaned.starts_with("wonder ") {
                        let parts: Vec<&str> = if cleaned.starts_with("wonder ") { cleaned[7..].trim().split_whitespace().collect() } else { cleaned[6..].trim().split_whitespace().collect() };
                        if parts.len() == 1 {
                            let card = parts[0].to_string();
                            let stage: u8 = view.wonder_stages_built + 1;
                            if hand.contains(&card) {
                                Some(SevenWondersAction::Terminal(TerminalAction::BuildWonder { card_id: card, stage, trades }))
                            } else { None }
                        } else {
                            // old mode with stage or invalid -> fail to produce action (will be unparsed)
                            None
                        }
                    } else if cmd == "burn" || cleaned.starts_with("burn ") {
                        let after = if cleaned.starts_with("burn ") { &cleaned[5..] } else { &cleaned[4..] };
                        let card = after.split_whitespace().next().unwrap_or("").to_string();
                        if hand.contains(&card) {
                            Some(SevenWondersAction::Terminal(TerminalAction::BurnCard { card_id: card }))
                        } else { None }
                    } else if let Some(card) = hand.iter().find(|c| cleaned.contains(*c)) {
                        // Lenient: mention of a card id -> build it (no trades in fallback)
                        Some(SevenWondersAction::Terminal(TerminalAction::PlayCard { card_id: card.clone(), trades: vec![] }))
                    } else {
                        None
                    };

                    if let Some(action) = chosen {
                        println!("[LLM p{}] parsed: {:?}", player, action);
                        decided = Some(action);
                    } else {
                        println!("[LLM p{}] unparsed '{}', fallback", player, cleaned);
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

/// Controller for smoke tests: always picks the first card in hand that is
/// playable (i.e. the engine accepts the PlayCard action with no trades).
/// Falls back to burning the first card.
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
