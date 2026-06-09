//! Player controllers for driving the game (human via terminal or LLM via API).

use super::actions::{SevenWondersAction, TerminalAction};
use super::state::GameState;
use super::term;
use crate::results::LlmSeatStats;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;

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

fn parse_neighbor(s: &str) -> Option<super::types::Neighbor> {
    use super::types::Neighbor;
    match s {
        "left" | "l" => Some(Neighbor::Left),
        "right" | "r" => Some(Neighbor::Right),
        _ => None,
    }
}

/// Parse `left:wood` / `right:stone` (preferred) or `wood:left` / `stone:right`.
fn parse_trade_token(tok: &str) -> Option<super::types::Trade> {
    use super::types::Trade;
    let t = tok.to_lowercase();
    let (a, b) = t.split_once(':')?;
    if b.contains(':') {
        return None;
    }
    if let Some(neigh) = parse_neighbor(a) {
        if let Some(res) = parse_resource(b) {
            return Some(Trade { from: neigh, resource: res });
        }
    }
    if let Some(res) = parse_resource(a) {
        if let Some(neigh) = parse_neighbor(b) {
            return Some(Trade { from: neigh, resource: res });
        }
    }
    None
}

fn parse_trades(tail: &str) -> Vec<super::types::Trade> {
    tail.split_whitespace()
        .filter_map(|tok| parse_trade_token(tok))
        .collect()
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

    /// Run the decision loop for the player.
    /// `log_view`: when Some, this is the (personalized) log prefix + open decision block for this player.
    ///   Only LLM impls use it as their prompt content. Others may ignore it.
    /// Returns the chosen terminal action (or observe for humans).
    fn decide_action(&mut self, game: &GameState, player: usize, log_view: Option<&str>) -> SevenWondersAction;

    /// Print static prompt files once before the first round (LLM / `human-agent`).
    fn print_startup_context(&mut self, _game: &GameState, _player: usize) {}
}

/// LLM controller using the xAI API.
/// Now uses the rich personalized plain-text log (when the runner supplies log_view) as its entire context.
/// The log contains history + the open decision block with this player's private hand/coins/neighbors.
/// Always outputs exactly one of the 3 terminating actions (play/wonder/burn, with trades specified via repeated dir:res tokens).
pub struct LLMController {
    pub model: String,
    client: Option<crate::models::xai::XaiClient>,
    stats: Option<Rc<RefCell<LlmSeatStats>>>,
    /// Conversation history: system + static user body seeded at startup; each turn appends log + assistant reply.
    messages: Vec<(String, String)>,
    /// True when the latest user message is for the current decision and has no assistant reply yet (retry updates it).
    pending_turn_user: bool,
}

impl LLMController {
    pub fn with_stats(model: String, stats: Option<Rc<RefCell<LlmSeatStats>>>) -> Self {
        let client = crate::models::xai::XaiClient::new().ok();
        Self {
            model,
            client,
            stats,
            messages: Vec::new(),
            pending_turn_user: false,
        }
    }

    fn seed_conversation(&mut self, game: &GameState, player: usize) {
        self.messages = vec![
            (
                String::from("system"),
                system_prompt_text(),
            ),
            (
                String::from("user"),
                human_agent_static_user_body(game, player),
            ),
        ];
        self.pending_turn_user = false;
    }

    fn push_or_update_turn_user(&mut self, content: String) {
        if self.pending_turn_user {
            if let Some(last) = self.messages.last_mut() {
                if last.0 == "user" {
                    last.1 = content;
                    return;
                }
            }
        }
        self.messages.push((String::from("user"), content));
        self.pending_turn_user = true;
    }
}

impl PlayerController for LLMController {
    fn decision_mode(&self) -> ControllerDecisionMode {
        ControllerDecisionMode::LlmAgent
    }

    fn print_startup_context(&mut self, game: &GameState, player: usize) {
        self.seed_conversation(game, player);
        term::print_human_agent_startup_context(
            game,
            player,
            &system_prompt_text(),
            &human_agent_static_user_body(game, player),
        );
    }

    fn decide_action(&mut self, game: &GameState, player: usize, log_view: Option<&str>) -> SevenWondersAction {
        let view = game.view_for_player(player);
        let hand = &view.hand;

        if hand.is_empty() {
            let fallback = SevenWondersAction::Terminal(TerminalAction::BurnCard { card_id: "none".to_string() });
            println!("[LLM p{}] fallback empty hand", player);
            return fallback;
        }

        if self.messages.is_empty() {
            self.seed_conversation(game, player);
        }

        let turn_prompt = if let Some(view) = log_view {
            build_turn_user_message(player, view)
        } else {
            format!(
                "You are playing 7 Wonders as player {}. Current hand: {:?}. Coins: {}. Choose one action: play <card>, wonder <card>, or burn <card>. Output only the action like 'play workshop' or 'burn theater'.",
                player, hand, view.coins
            )
        };

        if log_view.is_some() {
            term::print_llm_turn_context(game, player, log_view.unwrap());
        }

        let mut decided: Option<SevenWondersAction> = None;
        let max_toks = if log_view.is_some() { Some(80) } else { None };

        if self.client.is_some() {
            self.push_or_update_turn_user(turn_prompt);
        }

        if let Some(client) = self.client.as_ref() {
            match client.complete_with_messages(&self.model, &self.messages, max_toks) {
                Ok((response, latency_ms)) => {
                    self.messages
                        .push((String::from("assistant"), response.clone()));
                    self.pending_turn_user = false;
                    if let Some(stats) = &self.stats {
                        let mut s = stats.borrow_mut();
                        s.decisions += 1;
                        s.total_latency_ms += latency_ms;
                    }
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
                        if let Some(stats) = &self.stats {
                            stats.borrow_mut().parse_failures += 1;
                        }
                        println!("[LLM p{}] unparsed '{}', fallback", player, response.trim());
                    }
                }
                Err(e) => {
                    if self.pending_turn_user {
                        self.messages.pop();
                        self.pending_turn_user = false;
                    }
                    if let Some(stats) = &self.stats {
                        stats.borrow_mut().api_errors += 1;
                    }
                    println!("[LLM p{}] API error: {}. fallback", player, e);
                }
            }
        } else {
            if self.pending_turn_user {
                self.messages.pop();
                self.pending_turn_user = false;
            }
            if let Some(stats) = &self.stats {
                stats.borrow_mut().api_errors += 1;
            }
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

/// Static portion of the LLM user message (agent + cards + user.txt + wonder overview), without log.
pub(crate) fn human_agent_static_user_body(game: &GameState, player: usize) -> String {
    let agent_txt = load_prompt_file("agent.txt");
    let cards_txt = load_cards_for_players(game.player_count);
    let user_txt = load_prompt_file("user.txt");
    let wonder = game.format_wonder_stages_overview(player);
    format!("{agent_txt}\n\n{cards_txt}\n\n{user_txt}\n\n{wonder}")
}

fn print_wonder_startup(game: &GameState, player: usize) {
    term::print_wonder_startup_context(game, player);
}

pub(crate) fn system_prompt_text() -> String {
    load_prompt_file("system_prompt.txt")
}

/// Per-turn user message: log slice only (static prompts were sent at startup).
pub(crate) fn build_turn_user_message(player: usize, log_view: &str) -> String {
    format!(
        "Current log context for your decision (player {player}):\n{log_view}\n\nOutput exactly one action line:"
    )
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

    fn print_startup_context(&mut self, game: &GameState, player: usize) {
        if self.full_agent_context {
            term::print_human_agent_startup_context(
                game,
                player,
                &system_prompt_text(),
                &human_agent_static_user_body(game, player),
            );
        } else {
            print_wonder_startup(game, player);
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

    #[test]
    fn parse_trades_accepts_dir_resource_and_resource_dir() {
        use crate::games::seven_wonders::types::{Neighbor, Resource};

        let dir_first = parse_trades("play archery_range left:wood right:wood left:ore");
        assert_eq!(dir_first.len(), 3);
        assert_eq!(dir_first[0].from, Neighbor::Left);
        assert_eq!(dir_first[0].resource, Resource::Wood);

        let res_first = parse_trades("play archery_range wood:right wood:left ore:left");
        assert_eq!(res_first.len(), 3);
        assert_eq!(res_first[0].from, Neighbor::Right);
        assert_eq!(res_first[0].resource, Resource::Wood);
        assert_eq!(res_first[1].from, Neighbor::Left);
        assert_eq!(res_first[1].resource, Resource::Wood);
        assert_eq!(res_first[2].from, Neighbor::Left);
        assert_eq!(res_first[2].resource, Resource::Ore);
    }
}
