//! Core game state for Seven Wonders (base game).
//!
//! Current focus: supporting the incremental validation plan.

use super::actions::{ActionResult, SevenWondersAction, TerminalAction, Trade, Neighbor};
use super::cards::CardDatabase;
use super::types::{Cost, DiscountType, Effect, Resource, Resources};

/// Represents one player's board state.
#[derive(Debug, Clone)]
pub struct PlayerBoard {
    pub wonder_id: String,           // Currently always "gizah_a"
    pub wonder_stages_built: u8,     // 0-3 for Gizah A
    pub played_cards: Vec<String>,   // card ids
    pub coins: u8,
    pub military_tokens: i8,         // can be negative
    pub science_symbols: Vec<String>,// "cog", "tablet", "compass"
}

/// Full state for one player from the engine's perspective.
#[derive(Debug, Clone)]
pub struct PlayerState {
    pub id: usize,
    pub board: PlayerBoard,
    pub current_hand: Vec<String>,   // card ids currently in hand
}

/// The main game state.
#[derive(Debug, Clone)]
pub struct GameState {
    pub player_count: u8,
    pub current_age: u8,
    pub round_in_age: u8,            // 1..=6
    pub players: Vec<PlayerState>,
    pub card_db: CardDatabase,
    pub current_round_actions: Vec<Option<TerminalAction>>,
    // direction: true for left (player i passes to i+1), false for right
    pub pass_left: bool,
    // TODO: battle tokens per player, etc.
}

impl GameState {
    /// Creates a new game with all players using Gizah A.
    /// Deals starting hands for Age 1.
    pub fn new(player_count: u8) -> Self {
        assert!((2..=7).contains(&player_count));

        let card_db = CardDatabase::load();

        let mut players = Vec::with_capacity(player_count as usize);
        for i in 0..player_count {
            players.push(PlayerState {
                id: i as usize,
                board: PlayerBoard {
                    wonder_id: "gizah_a".to_string(),
                    wonder_stages_built: 0,
                    played_cards: vec![],
                    coins: 3, // standard for Gizah A? (actually varies, but fine for now)
                    military_tokens: 0,
                    science_symbols: vec![],
                },
                current_hand: vec![],
            });
        }

        let mut state = Self {
            player_count,
            current_age: 1,
            round_in_age: 1,
            players,
            card_db,
            current_round_actions: vec![None; player_count as usize],
            pass_left: true, // age 1 and 3 left, age 2 right
        };

        state.start_age();
        state
    }

    /// Starts a new age: builds the deck for the age (with proper guild selection for age 3),
    /// deals 7 cards to each player.
    /// For determinism in tests, uses sorted order instead of shuffle.
    pub fn start_age(&mut self) {
        let age = self.current_age;
        let n = self.player_count as usize;

        // Build the pool with correct copies
        let mut pool = self.card_db.build_age_pool(age, self.player_count);

        // For age 3, select N+2 guilds
        if age == 3 {
            let mut guilds: Vec<String> = pool.iter().filter(|id| {
                if let Some(c) = self.card_db.get(id) {
                    c.color == "purple"
                } else { false }
            }).cloned().collect();
            guilds.sort(); // determinism
            let num_guilds = std::cmp::min(guilds.len(), n + 2);
            let selected_guilds: std::collections::HashSet<_> = guilds.into_iter().take(num_guilds).collect();

            pool.retain(|id| {
                if let Some(c) = self.card_db.get(id) {
                    if c.color == "purple" {
                        selected_guilds.contains(id)
                    } else {
                        true
                    }
                } else { true }
            });
        }

        // Sort for determinism
        pool.sort();

        // Deal 7 to each
        let mut idx = 0;
        for player in &mut self.players {
            let mut hand = vec![];
            for _ in 0..7 {
                if idx < pool.len() {
                    hand.push(pool[idx].clone());
                    idx += 1;
                }
            }
            player.current_hand = hand;
        }

        self.round_in_age = 1;
        self.current_round_actions = vec![None; n];
        self.pass_left = age != 2; // age 1,3 left (to +1), age 2 right (to -1)
    }

    /// Returns the current hand of the given player (card ids).
    pub fn get_hand(&self, player: usize) -> &[String] {
        &self.players[player].current_hand
    }

    /// Returns a view of the game from a specific player's perspective.
    /// This is what an agent (LLM or human) should primarily work with.
    pub fn view_for_player(&self, player: usize) -> PlayerView {
        let p = &self.players[player];
        PlayerView {
            player_id: player,
            hand: p.current_hand.clone(),
            played_cards: p.board.played_cards.clone(),
            coins: p.board.coins,
            wonder_stages_built: p.board.wonder_stages_built,
            wonder_id: p.board.wonder_id.clone(),
            military_tokens: p.board.military_tokens,
            // For now, minimal. We will expand observation tools to return richer views.
        }
    }

    /// Attempts to play a card from the player's hand.
    /// Currently very minimal — just removes the card from hand and records it as played.
    /// Full cost, chaining, and trading logic comes later.
    pub fn play_card(&mut self, player: usize, card_id: &str) -> Result<(), String> {
        let hand = &mut self.players[player].current_hand;
        if let Some(pos) = hand.iter().position(|c| c == card_id) {
            hand.remove(pos);
            self.players[player].board.played_cards.push(card_id.to_string());
            Ok(())
        } else {
            Err(format!("Player {} does not have card {} in hand", player, card_id))
        }
    }

    /// Attempts to use a card from hand to build a wonder stage.
    /// Currently just tucks the card (no wonder stage tracking yet).
    pub fn build_wonder(&mut self, player: usize, card_id: &str) -> Result<(), String> {
        let hand = &mut self.players[player].current_hand;
        if let Some(pos) = hand.iter().position(|c| c == card_id) {
            hand.remove(pos);
            self.players[player].board.wonder_stages_built += 1;
            // In reality the card is tucked, but we don't track tucked cards yet.
            Ok(())
        } else {
            Err(format!("Player {} does not have card {} in hand", player, card_id))
        }
    }

    /// Discards a card from hand for coins (base +3 coins).
    pub fn burn_card(&mut self, player: usize, card_id: &str) -> Result<(), String> {
        let hand = &mut self.players[player].current_hand;
        if let Some(pos) = hand.iter().position(|c| c == card_id) {
            hand.remove(pos);
            self.players[player].board.coins = self.players[player].board.coins.saturating_add(3);
            Ok(())
        } else {
            Err(format!("Player {} does not have card {} in hand", player, card_id))
        }
    }

    // ==================== Observation Tools (very basic versions) ====================

    pub fn check_my_cards(&self, player: usize) -> Vec<String> {
        self.players[player].board.played_cards.clone()
    }

    pub fn check_my_coins(&self, player: usize) -> u8 {
        self.players[player].board.coins
    }

    /// Returns whether the given terminal action is valid for the player right now.
    /// Used by auto controllers in smoke tests to pick a purchaseable card.
    pub fn is_valid_terminal_action(&self, player: usize, action: &TerminalAction) -> bool {
        self.validate_afford(player, action).is_ok()
    }

    /// Build the private info text that goes inside a decision block for this player.
    /// Includes current hand (ids), coins, wonder progress, own production, and what the two
    /// neighbors can currently supply (fixed + their choice slots). Used by the log + LLM prompt.
    pub(crate) fn friendly_resource(r: Resource) -> &'static str {
        match r {
            Resource::Clay => "brick",
            Resource::Papyrus => "paper",
            Resource::Loom => "cloth",
            Resource::Wood => "wood",
            Resource::Stone => "stone",
            Resource::Ore => "ore",
            Resource::Glass => "glass",
        }
    }

    fn format_hand_for_log(hand: &[String]) -> String {
        if hand.is_empty() {
            return "[]".to_string();
        }
        format!("[{}]", hand.join(", "))
    }

    pub fn get_private_decision_info(&self, player: usize) -> String {
        let hand = &self.players[player].current_hand;
        let coins = self.players[player].board.coins;
        let stages = self.players[player].board.wonder_stages_built;

        let my_fixed = self.compute_fixed_production(player);
        let my_choices = self.collect_choice_options(player);
        let your_prod = Self::format_production_list(&my_fixed, &my_choices);

        let left_p = self.neighbor_player(player, Neighbor::Left);
        let right_p = self.neighbor_player(player, Neighbor::Right);
        let left_fixed = self.compute_fixed_production(left_p);
        let right_fixed = self.compute_fixed_production(right_p);
        let left_choices = self.collect_choice_options(left_p);
        let right_choices = self.collect_choice_options(right_p);
        let left_prod = Self::format_production_list(&left_fixed, &left_choices);
        let right_prod = Self::format_production_list(&right_fixed, &right_choices);

        format!(
            "hand: {}\ncoins: {}\nwonder_stages_built: {}\n\
your_production: [{}]\n\
left (Player {}) production: [{}]\n\
right (Player {}) production: [{}]\n",
            Self::format_hand_for_log(hand),
            coins,
            stages,
            your_prod.join(", "),
            left_p,
            left_prod.join(", "),
            right_p,
            right_prod.join(", ")
        )
    }

    pub(crate) fn format_production_list(fixed: &Resources, choices: &[Vec<Resource>]) -> Vec<String> {
        let mut list = vec![];
        for (r, &amt) in &fixed.counts {
            let name = Self::friendly_resource(*r);
            for _ in 0..amt {
                list.push(name.to_string());
            }
        }
        for ch in choices {
            if ch.is_empty() { continue; }
            if ch.len() == 1 {
                list.push(Self::friendly_resource(ch[0]).to_string());
            } else {
                let joined: Vec<String> = ch.iter().map(|&r| Self::friendly_resource(r).to_string()).collect();
                list.push(joined.join("/"));
            }
        }
        list
    }

    /// End-of-age summary: wonder stages built and battle results (stubbed battles for now).
    pub fn get_age_summary(&self) -> String {
        let mut lines = vec![];
        for (i, p) in self.players.iter().enumerate() {
            lines.push(format!(
                "Player {} built {} wonder stages",
                i, p.board.wonder_stages_built
            ));
        }
        lines.push(String::new());
        for i in 0..self.players.len() {
            let left_delta = 0i8; // stub until military resolution is implemented
            let right_delta = 0i8;
            lines.push(format!(
                "Player {} gets {:+}, {:+} from battles",
                i, left_delta, right_delta
            ));
        }
        lines.join("\n")
    }

    // ==================== Resource / Cost / Trade helpers (for validation & apply) ====================

    fn compute_fixed_production(&self, player: usize) -> Resources {
        let mut prod = Resources::default();
        for cid in &self.players[player].board.played_cards {
            if let Some(card) = self.card_db.get(cid) {
                if let Effect::Production { fixed, .. } = &card.effect {
                    for (r, &amt) in &fixed.counts {
                        prod.add(*r, amt);
                    }
                }
            }
        }
        prod
    }

    fn collect_choice_options(&self, player: usize) -> Vec<Vec<Resource>> {
        let mut choices = vec![];
        for cid in &self.players[player].board.played_cards {
            if let Some(card) = self.card_db.get(cid) {
                if let Effect::Production { choice: Some(opts), .. } = &card.effect {
                    if !opts.is_empty() {
                        choices.push(opts.clone());
                    }
                }
            }
        }
        choices
    }

    /// Can the player cover the needed resources using own fixed production + choice slots + any extra fixed (e.g. bought via trades)?
    fn player_can_cover(&self, player: usize, needed: &Resources, extra_fixed: &Resources) -> bool {
        let fixed = self.compute_fixed_production(player);
        let mut avail_fixed = fixed;
        for (r, &amt) in &extra_fixed.counts {
            avail_fixed.add(*r, amt);
        }
        let mut rem_counts = needed.counts.clone();
        for (r, &fa) in &avail_fixed.counts {
            if let Some(need) = rem_counts.get_mut(r) {
                *need = need.saturating_sub(fa);
            }
        }
        rem_counts.retain(|_, &mut v| v > 0);
        let mut still_needed: Vec<Resource> = vec![];
        for (r, &a) in &rem_counts {
            for _ in 0..a {
                still_needed.push(*r);
            }
        }
        let mut slots = self.collect_choice_options(player);
        for nr in still_needed {
            if let Some(idx) = slots.iter().position(|opts| opts.contains(&nr)) {
                slots.remove(idx);
            } else {
                return false;
            }
        }
        true
    }

    fn compute_bought(&self, trades: &[Trade]) -> Resources {
        let mut b = Resources::default();
        for t in trades {
            b.add(t.resource, 1);
        }
        b
    }

    fn effective_buy_price(&self, player: usize, from: Neighbor, resource: Resource) -> u8 {
        let is_raw = matches!(resource, Resource::Wood | Resource::Stone | Resource::Ore | Resource::Clay);
        let is_manuf = matches!(resource, Resource::Glass | Resource::Loom | Resource::Papyrus);
        for cid in &self.players[player].board.played_cards {
            if let Some(card) = self.card_db.get(cid) {
                if let Effect::TradeDiscount { direction, kind, cost } = &card.effect {
                    let dir_match = match direction {
                        None => true,
                        Some(Neighbor::Left) => matches!(from, Neighbor::Left),
                        Some(Neighbor::Right) => matches!(from, Neighbor::Right),
                    };
                    let kind_match = match kind {
                        DiscountType::RawMaterials => is_raw,
                        DiscountType::ManufacturedGoods => is_manuf,
                    };
                    if dir_match && kind_match {
                        return *cost;
                    }
                }
            }
        }
        2
    }

    fn compute_trade_coins(&self, player: usize, trades: &[Trade]) -> u32 {
        trades.iter().map(|t| self.effective_buy_price(player, t.from, t.resource) as u32).sum()
    }

    pub(crate) fn neighbor_player(&self, player: usize, from: Neighbor) -> usize {
        let n = self.player_count as usize;
        match from {
            Neighbor::Left => (player + n - 1) % n,
            Neighbor::Right => (player + 1) % n,
        }
    }

    /// Check if the specified trades can be fulfilled by the neighbors' production (respecting fixed + exclusive combo slots per card).
    fn trades_are_valid(&self, player: usize, trades: &[Trade]) -> bool {
        use std::collections::HashMap;
        let mut by_neigh: HashMap<Neighbor, Resources> = HashMap::new();
        for t in trades {
            by_neigh.entry(t.from).or_default().add(t.resource, 1);
        }
        for (from, req) in by_neigh {
            let neigh_p = self.neighbor_player(player, from);
            if !self.neighbor_can_supply(neigh_p, &req) {
                return false;
            }
        }
        true
    }

    fn neighbor_can_supply(&self, neigh_player: usize, req: &Resources) -> bool {
        if req.counts.is_empty() {
            return true;
        }
        let fixed = self.compute_fixed_production(neigh_player);
        let mut rem_counts = req.counts.clone();
        for (r, &f) in &fixed.counts {
            if let Some(n) = rem_counts.get_mut(r) {
                *n = n.saturating_sub(f);
            }
        }
        rem_counts.retain(|_, &mut v| v > 0);
        let mut still: Vec<Resource> = vec![];
        for (r, &a) in &rem_counts {
            for _ in 0..a {
                still.push(*r);
            }
        }
        let mut slots = self.collect_choice_options(neigh_player);
        for nr in still {
            if let Some(idx) = slots.iter().position(|opts| opts.contains(&nr)) {
                slots.remove(idx);
            } else {
                return false;
            }
        }
        true
    }

    fn wonder_stage_cost(&self, wonder_id: &str, stage: u8) -> Cost {
        if wonder_id != "gizah_a" {
            return Cost::default();
        }
        // Giza A (standard base game costs)
        let pairs: Vec<(Resource, u8)> = match stage {
            1 => vec![(Resource::Stone, 2)],
            2 => vec![(Resource::Wood, 3)],
            3 => vec![(Resource::Stone, 4)],
            _ => vec![],
        };
        let mut res = Resources::default();
        for (r, a) in pairs {
            res.add(r, a);
        }
        Cost { coins: 0, resources: res }
    }

    fn validate_card_play(&self, player: usize, card_id: &str, trades: &[Trade]) -> Result<(), String> {
        let card = self.card_db.get(card_id).ok_or_else(|| "Unknown card".to_string())?;
        let cost = &card.cost;
        let bought = self.compute_bought(trades);
        if !self.player_can_cover(player, &cost.resources, &bought) {
            return Err(format!("Insufficient resources to play {} (not available from self or neighbors via specified trades)", card_id));
        }
        let card_coins = cost.coins as u32;
        let trade_coins = self.compute_trade_coins(player, trades);
        if (self.players[player].board.coins as u32) < card_coins + trade_coins {
            return Err(format!(
                "Not enough coins to play {} (have {}, need {} + {} from trades)",
                card_id, self.players[player].board.coins, card_coins, trade_coins
            ));
        }
        if !self.trades_are_valid(player, trades) {
            return Err("Invalid trades: requested resources not supplied by the chosen neighbor(s) or violating combo resource rules (cannot buy both options from one choice card in a turn, or same resource more times than available)".to_string());
        }
        Ok(())
    }

    fn validate_wonder_stage(&self, player: usize, stage: u8, trades: &[Trade]) -> Result<(), String> {
        let current = self.players[player].board.wonder_stages_built;
        if stage != current + 1 {
            return Err("Can only build the next wonder stage in sequence".to_string());
        }
        let wcost = self.wonder_stage_cost(&self.players[player].board.wonder_id, stage);
        let bought = self.compute_bought(trades);
        if !self.player_can_cover(player, &wcost.resources, &bought) {
            return Err(format!("Insufficient resources to build wonder stage {} (not available from self or neighbors via specified trades)", stage));
        }
        let wcoins = wcost.coins as u32;
        let trade_coins = self.compute_trade_coins(player, trades);
        if (self.players[player].board.coins as u32) < wcoins + trade_coins {
            return Err(format!(
                "Not enough coins to build wonder stage {} (have {}, need {} + {} from trades)",
                stage, self.players[player].board.coins, wcoins, trade_coins
            ));
        }
        if !self.trades_are_valid(player, trades) {
            return Err("Invalid trades for wonder: requested resources not supplied by the chosen neighbor(s) or violating combo rules".to_string());
        }
        Ok(())
    }

    fn validate_afford(&self, player: usize, action: &TerminalAction) -> Result<(), String> {
        match action {
            TerminalAction::PlayCard { card_id, trades } => self.validate_card_play(player, card_id, trades),
            TerminalAction::BuildWonder { stage, trades, .. } => self.validate_wonder_stage(player, *stage, trades),
            TerminalAction::BurnCard { .. } => Ok(()),
        }
    }

    /// Submit a terminal action for the current round.
    /// If all players have submitted, resolves the round (applies actions, passes hands, etc.).
    pub fn submit_terminal_action(&mut self, player: usize, action: TerminalAction) -> ActionResult {
        if self.current_round_actions[player].is_some() {
            return ActionResult::Invalid {
                reason: "Already submitted action for this round".to_string(),
                suggested_actions: vec![],
            };
        }
        // Basic validation: card must be in hand
        let card_id = match &action {
            TerminalAction::PlayCard { card_id, .. } => card_id,
            TerminalAction::BuildWonder { card_id, .. } => card_id,
            TerminalAction::BurnCard { card_id } => card_id,
        };
        if !self.players[player].current_hand.contains(card_id) {
            return ActionResult::Invalid {
                reason: format!("Card {} not in hand", card_id),
                suggested_actions: vec![],
            };
        }
        // Full afford / trade validation (resources from self+neighbors, coin costs incl. trades, supply limits)
        if let Err(reason) = self.validate_afford(player, &action) {
            return ActionResult::Invalid {
                reason,
                suggested_actions: vec![],
            };
        }
        self.current_round_actions[player] = Some(action);
        if self.current_round_actions.iter().all(|a| a.is_some()) {
            self.resolve_round();
        }
        ActionResult::Success { message: Some("Action submitted for round".to_string()) }
    }

    fn resolve_round(&mut self) {
        // Apply actions (remove card from hand, pay costs incl. trades for play/wonder, add production implicitly via played list, +3 for burn)
        for (i, opt_action) in self.current_round_actions.iter().enumerate() {
            if let Some(action) = opt_action {
                let card_id = match action {
                    TerminalAction::PlayCard { card_id, .. } => card_id,
                    TerminalAction::BuildWonder { card_id, .. } => card_id,
                    TerminalAction::BurnCard { card_id } => card_id,
                };
                // Remove from hand
                if let Some(pos) = self.players[i].current_hand.iter().position(|c| c == card_id) {
                    self.players[i].current_hand.remove(pos);
                }
                match action {
                    TerminalAction::PlayCard { card_id, trades } => {
                        self.players[i].board.played_cards.push(card_id.clone());
                        // pay coin cost of card + trades
                        let card_c = self.card_db.get(card_id).map(|c| c.cost.coins as u32).unwrap_or(0);
                        let t_c = self.compute_trade_coins(i, trades);
                        let pay = (card_c + t_c) as u8;
                        self.players[i].board.coins = self.players[i].board.coins.saturating_sub(pay);
                    }
                    TerminalAction::BuildWonder { stage, trades, .. } => {
                        self.players[i].board.wonder_stages_built += 1;
                        let w_c = self.wonder_stage_cost(&self.players[i].board.wonder_id, *stage).coins as u32;
                        let t_c = self.compute_trade_coins(i, trades);
                        let pay = (w_c + t_c) as u8;
                        self.players[i].board.coins = self.players[i].board.coins.saturating_sub(pay);
                    }
                    TerminalAction::BurnCard { .. } => {
                        self.players[i].board.coins = self.players[i].board.coins.saturating_add(3);
                    }
                }
            }
        }

        // Pass hands
        self.pass_hands();

        self.round_in_age += 1;
        self.current_round_actions = vec![None; self.player_count as usize];

        if self.round_in_age > 6 {
            self.resolve_battles();
            if self.current_age < 3 {
                self.current_age += 1;
                self.start_age();
            } else {
                // TODO: final scoring
            }
        }
    }

    fn pass_hands(&mut self) {
        let n = self.player_count as usize;
        let mut new_hands = vec![vec![]; n];
        for i in 0..n {
            let target = if self.pass_left {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            };
            new_hands[target] = self.players[i].current_hand.clone();
        }
        for i in 0..n {
            self.players[i].current_hand = new_hands[i].clone();
        }
    }

    fn resolve_battles(&mut self) {
        // Stub: for Gizah A, no military, so no change. Real implementation would compute strength from red cards + wonders.
        // For now, just placeholder.
    }
}

/// Human-readable round-summary lines for a terminal action.
fn format_action_summary_lines(game: &GameState, player: usize, action: &TerminalAction) -> Vec<String> {
    let mut lines = vec![];
    let trades: &[Trade] = match action {
        TerminalAction::PlayCard { trades, .. } | TerminalAction::BuildWonder { trades, .. } => trades,
        TerminalAction::BurnCard { .. } => &[],
    };
    for t in trades {
        let supplier = game.neighbor_player(player, t.from);
        lines.push(format!(
            "Player {} bought {} from player {}",
            player,
            GameState::friendly_resource(t.resource),
            supplier
        ));
    }
    match action {
        TerminalAction::PlayCard { card_id, .. } => {
            lines.push(format!("Player {} played {}", player, card_id));
        }
        TerminalAction::BuildWonder { stage, .. } => {
            lines.push(format!("Player {} built wonder stage {}", player, stage));
        }
        TerminalAction::BurnCard { card_id } => {
            lines.push(format!("Player {} burned {}", player, card_id));
        }
    }
    lines
}

fn format_error_for_log(reason: &str) -> String {
    let r = reason.to_lowercase();
    if r.contains("insufficient resources") || r.contains("not available from self or neighbors") {
        "ERROR: Insufficient resources to perform this action. Please select a different action or specify trades using left:resource / right:resource.\n".to_string()
    } else if r.contains("not enough coins") {
        format!("ERROR: {}\n", reason)
    } else if r.contains("invalid trades") {
        "ERROR: Invalid trades. Check neighbor production and use left:resource / right:resource notation (repeat for multiples).\n".to_string()
    } else if r.contains("not in hand") {
        format!("ERROR: {}\n", reason)
    } else {
        format!("ERROR: {}\n", reason)
    }
}

/// Run a full (stub) game with the given controllers.
/// Each round, for each player, the controller's decide_action is called (which can loop observations),
/// then the terminal action is submitted.
/// After 6 rounds, battles (stub), next age, etc.
pub fn run_game(controllers: Vec<Box<dyn super::controller::PlayerController>>) {
    run_limited_rounds_game(controllers, u32::MAX);
}

/// Quick smoke test: runs only the first 2 rounds of age 1.
/// Useful for fast iteration when testing agents without burning through a full 18-round game.
pub fn run_smoke_game(controllers: Vec<Box<dyn super::controller::PlayerController>>) {
    run_limited_rounds_game(controllers, 2);
}

/// Dedicated limited-rounds game runner (used by smoke tests and normal runs).
/// max_rounds: total number of rounds to play before stopping (across ages).
///
/// This is where the single shared log.txt + personalized per-agent views are built:
/// - One GameLog accumulates the canonical full plain-text log (with private decision blocks for whoever was deciding).
/// - For controllers that prefer_log_context (i.e. LLM), we supply get_decision_view_for which only contains
///   completed prior round summaries + the current round header + open decision block.
/// - Same-round actions (even from earlier players in sequential execution) are hidden from the view.
/// - Autos (FirstPurchaseable) behavior is untouched: they call is_valid_terminal_action directly.
/// - Up to 3 attempts for log players on nonsensical/illegal; ERROR lines injected into the decision text on retry.
/// - Full log is written to log.txt after every decision close and every round (for live `cat log.txt` or `tail -f`).
/// - Console also prints the FULL LOG and the exact PERSONALIZED VIEW SENT each time for the agent.
pub fn run_limited_rounds_game(mut controllers: Vec<Box<dyn super::controller::PlayerController>>, max_rounds: u32) {
    let n = controllers.len() as u8;
    let mut game = GameState::new(n);
    let mut game_log = super::GameLog::new();

    let is_smoke = max_rounds <= 4; // treat 2 or 4 as the dedicated smoke style
    let label = if max_rounds == 4 { " (SMOKE: 4 rounds, LLM only for player 1)" } else if max_rounds == 2 { " (SMOKE: 2 rounds)" } else { "" };
    println!("Starting Seven Wonders game{} with {} players (all Gizah A).", label, n);

    let mut total_rounds_played = 0u32;

    for age in 1..=3 {
        if total_rounds_played >= max_rounds {
            break;
        }
        game.current_age = age;
        game.start_age();
        game_log.start_age(age as u8);
        println!("\n=== Age {} ===", age);

        let mut rounds_this_age = 0u32;

        for round in 1..=6 {
            if total_rounds_played >= max_rounds {
                break;
            }
            game.round_in_age = round;
            game.current_round_actions = vec![None; n as usize];
            println!("\n-- Round {} --", round);

            game_log.start_round(round as u8);

            for p in 0..n as usize {
                let prefers_log = controllers[p].prefers_log_context();

                if prefers_log {
                    // === Log-using controller (LLM): rich context, 3 attempts, special afford path, errors to model ===
                    let private = game.get_private_decision_info(p);
                    game_log.begin_player_decision(p, private);

                    let mut attempts = 0u32;
                    let mut got_success = false;

                    loop {
                        attempts += 1;
                        if attempts > 3 {
                            println!("[LLM p{}] 3 attempts exhausted for this decision - forcing fallback burn", p);
                            break;
                        }

                        let view = game_log.get_decision_view_for(p);

                        // Show user the full built log + exactly what this agent is being sent (as requested).
                        println!("=== FULL LOG (as being built, also written to log.txt) ===\n{}\n=== END FULL LOG ===", game_log.full_as_str());
                        println!("=== PERSONALIZED VIEW SENT TO AGENT (player {}) attempt {} ===\n{}\n=== END SENT VIEW ===", p, attempts, view);

                        let action = controllers[p].decide_action(&game, p, Some(&view));

                        if let SevenWondersAction::Terminal(term) = action {
                            let res = game.submit_terminal_action(p, term.clone());
                            println!("Player {} result: {:?}", p, res);

                            if matches!(res, ActionResult::Success { .. }) {
                                let summary = format_action_summary_lines(&game, p, &term);
                                game_log.close_current_decision(&summary);
                                got_success = true;
                                break;
                            } else if let ActionResult::Invalid { reason, .. } = res {
                                game_log.append_to_current_decision(&format_error_for_log(&reason));
                            }
                        } else {
                            game_log.append_to_current_decision(
                                "ERROR: Only play, wonder, and burn actions are allowed.\n",
                            );
                        }
                    }

                    if !got_success {
                        // Fallback burn after exhausting attempts (still record in log)
                        if let Some(card) = game.players[p].current_hand.first().cloned() {
                            let fb = TerminalAction::BurnCard { card_id: card.clone() };
                            let _ = game.submit_terminal_action(p, fb.clone());
                            let summary = format_action_summary_lines(&game, p, &fb);
                            game_log.close_current_decision(&summary);
                        }
                    }
                } else {
                    // === Non-log controllers (Auto FirstPurchaseable + Human): behavior unchanged ===
                    // Autos still use direct engine is_valid + pick first playable. We just record their action as simple line.
                    loop {
                        let action = controllers[p].decide_action(&game, p, None);
                        if let SevenWondersAction::Terminal(term) = action {
                            let res = game.submit_terminal_action(p, term.clone());
                            println!("Player {} result: {:?}", p, res);
                            if matches!(res, ActionResult::Success { .. }) {
                                let summary = format_action_summary_lines(&game, p, &term);
                                game_log.append_simple_player_action(&summary);
                                break;
                            } else {
                                // Re-ask (humans re-enter their menu loop on next decide call; autos should not hit invalid)
                            }
                        }
                    }
                }
            }

            total_rounds_played += 1;
            rounds_this_age += 1;

            // Round fully resolved inside the last player's submit.
            // Now emit the public summary (visible to all subsequent decisions) and commit the round.
            game_log.add_round_summary(round as u8);

            // Extra visibility of the built log after the round (in addition to the per-decision prints).
            println!("=== FULL LOG AFTER ROUND {} (committed for next round; also in log.txt) ===\n{}\n=== END ===", round, game_log.full_as_str());
        }

        if rounds_this_age == 6 {
            game.resolve_battles();
            game_log.close_age(&game.get_age_summary());
        }
    }
    game_log.write_to_disk();

    if max_rounds == 4 {
        println!("\nDedicated smoke finished after 4 rounds. Full log in log.txt. Check console for the views that were sent to the LLM.");
    } else if is_smoke {
        println!("\nSmoke game finished after {} rounds (full scoring/battles stubbed for now).", max_rounds);
    } else {
        println!("\nGame finished (full scoring/battles stubbed for now).");
    }
}

/// A view of the game from one player's perspective.
/// Observation tools should return data in (or populate) structures like this.
#[derive(Debug, Clone)]
pub struct PlayerView {
    pub player_id: usize,
    pub hand: Vec<String>,           // card ids in current hand
    pub played_cards: Vec<String>,   // own played cards (ids)
    pub coins: u8,
    pub wonder_id: String,
    pub wonder_stages_built: u8,
    pub military_tokens: i8,
    // TODO: neighbor info, science, resources summary, etc. will be added via specific tools.
}

#[cfg(test)]
mod tests {
    use super::GameState;
    use crate::games::seven_wonders::actions::{ActionResult, TerminalAction};
    use crate::games::seven_wonders::types::{Neighbor, Resource, Trade};

    #[test]
    fn player_receives_starting_hand() {
        let game = GameState::new(3);

        for player in 0..3 {
            let hand = game.get_hand(player);
            assert_eq!(
                hand.len(),
                7,
                "Player {} should receive 7 cards in starting hand for a 3-player game",
                player
            );
        }
    }

    #[test]
    fn three_player_game_deals_21_cards_total() {
        let game = GameState::new(3);
        let total: usize = (0..3).map(|p| game.get_hand(p).len()).sum();
        assert_eq!(total, 21);
    }

    #[test]
    fn player_can_perform_all_three_terminating_actions() {
        let mut game = GameState::new(3);
        let player = 0;

        // Give the player three distinct cards in hand for the test
        game.players[player].current_hand = vec![
            "lumber_yard".to_string(),
            "stone_pit".to_string(),
            "clay_pool".to_string(),
        ];

        // Burn one
        assert!(game.burn_card(player, "lumber_yard").is_ok());
        assert_eq!(game.players[player].current_hand.len(), 2);

        // Play one
        assert!(game.play_card(player, "stone_pit").is_ok());
        assert_eq!(game.players[player].board.played_cards.len(), 1);

        // Use one to build a wonder stage
        assert!(game.build_wonder(player, "clay_pool").is_ok());
        assert_eq!(game.players[player].board.wonder_stages_built, 1);
        assert!(game.players[player].current_hand.is_empty());
    }

    #[test]
    fn player_can_use_basic_observation_tools() {
        let mut game = GameState::new(2);
        let player = 0;

        // Manually give the player a played card and some coins for the test
        game.players[player].board.played_cards.push("lumber_yard".to_string());
        game.players[player].board.coins = 5;

        let my_cards = game.check_my_cards(player);
        assert_eq!(my_cards, vec!["lumber_yard".to_string()]);

        let coins = game.check_my_coins(player);
        assert_eq!(coins, 5);
    }

    // ==================== New unit tests for cost, resource, trade, burn, combo rules ====================

    #[test]
    fn play_fails_if_required_resources_not_available_from_self_or_neighbors() {
        let mut game = GameState::new(3);
        // player 0 has workshop (costs 1 glass) in hand
        game.players[0].current_hand = vec!["workshop".to_string()];
        // no one has played glassworks or any glass producer
        // even with empty trades, should fail resource check
        let action = TerminalAction::PlayCard {
            card_id: "workshop".to_string(),
            trades: vec![],
        };
        let res = game.submit_terminal_action(0, action);
        if let ActionResult::Invalid { reason, .. } = res {
            assert!(reason.contains("resource") || reason.contains("glass") || reason.contains("Insufficient"), "unexpected reason: {}", reason);
        } else {
            panic!("expected Invalid for missing resources");
        }
        // hand unchanged
        assert!(game.players[0].current_hand.contains(&"workshop".to_string()));
    }

    #[test]
    fn play_fails_if_coins_insufficient_even_if_resources_available_via_trade() {
        let mut game = GameState::new(3);
        // player 0 will buy glass from left (player 2)
        game.players[0].current_hand = vec!["workshop".to_string()];
        game.players[0].board.coins = 1; // less than trade price 2
        // left neighbor (p2) has played glassworks
        game.players[2].board.played_cards = vec!["glassworks".to_string()];
        let action = TerminalAction::PlayCard {
            card_id: "workshop".to_string(),
            trades: vec![Trade { from: Neighbor::Left, resource: Resource::Glass }],
        };
        let res = game.submit_terminal_action(0, action);
        if let ActionResult::Invalid { reason, .. } = res {
            assert!(reason.contains("coin") || reason.contains("Not enough coins"), "unexpected: {}", reason);
        } else {
            panic!("expected coin failure");
        }
    }

    #[test]
    fn wonder_stage_fails_if_resources_not_available() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["lumber_yard".to_string()];
        // stage 1 gizah requires 2 stone, no stone producers
        let action = TerminalAction::BuildWonder {
            card_id: "lumber_yard".to_string(),
            stage: 1,
            trades: vec![],
        };
        // to reach validate we submit dummies for others? For fail we can submit direct, it will validate even if not all
        let res = game.submit_terminal_action(0, action);
        if let ActionResult::Invalid { reason, .. } = res {
            assert!(reason.contains("resource") || reason.contains("stone") || reason.contains("Insufficient"), "got: {}", reason);
        } else {
            panic!("expected resource fail for wonder");
        }
    }

    #[test]
    fn wonder_stage_fails_if_coins_insufficient_for_trades() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["lumber_yard".to_string()];
        game.players[0].board.coins = 1; // low, will need 4 for 2 stone @2 each
        // neighbor (left of 0 = p2) supplies 2 stone via two producers (for test)
        game.players[2].board.played_cards = vec!["stone_pit".to_string(), "stone_pit".to_string()];
        let action = TerminalAction::BuildWonder {
            card_id: "lumber_yard".to_string(),
            stage: 1,
            trades: vec![
                Trade { from: Neighbor::Left, resource: Resource::Stone },
                Trade { from: Neighbor::Left, resource: Resource::Stone },
            ],
        };
        let res = game.submit_terminal_action(0, action);
        if let ActionResult::Invalid { reason, .. } = res {
            assert!(reason.contains("coin") || reason.contains("Not enough coins"), "got: {}", reason);
        } else {
            panic!("expected coin fail for wonder trade");
        }
    }

    #[test]
    fn burn_increases_coins_by_3() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["lumber_yard".to_string()];
        game.players[0].board.coins = 0;
        // use direct for simplicity (tests the +3)
        assert!(game.burn_card(0, "lumber_yard").is_ok());
        assert_eq!(game.players[0].board.coins, 3);
        assert!(game.players[0].current_hand.is_empty());
    }

    #[test]
    fn trading_post_and_forum_decrease_buy_cost_by_1() {
        let mut game = GameState::new(3);
        // player 0 has workshop (needs glass), has east trading post? for manuf we use forum
        game.players[0].current_hand = vec!["workshop".to_string()];
        game.players[0].board.coins = 1;
        game.players[0].board.played_cards = vec!["forum".to_string()]; // manuf discount to 1 from both
        // left neighbor of p0 is p2, so put glass production on p2
        game.players[2].board.played_cards = vec!["glassworks".to_string()];
        // without forum would need 2 coins, with it 1 coin -> should succeed
        let action = TerminalAction::PlayCard {
            card_id: "workshop".to_string(),
            trades: vec![Trade { from: Neighbor::Left, resource: Resource::Glass }],
        };
        // make others submit something so resolve happens
        game.players[1].current_hand = vec!["lumber_yard".to_string()];
        game.players[2].current_hand = vec!["lumber_yard".to_string()];
        let _ = game.submit_terminal_action(1, TerminalAction::BurnCard { card_id: "lumber_yard".to_string() });
        let _ = game.submit_terminal_action(2, TerminalAction::BurnCard { card_id: "lumber_yard".to_string() });
        let res = game.submit_terminal_action(0, action);
        assert!(matches!(res, ActionResult::Success { .. }));
        // coins should be 1 - 1 = 0
        assert_eq!(game.players[0].board.coins, 0);
        assert!(game.players[0].board.played_cards.contains(&"workshop".to_string()));
    }

    #[test]
    fn cannot_purchase_same_resource_twice_in_one_turn() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["baths".to_string()]; // costs 1 stone
        game.players[0].board.coins = 10;
        // neighbor has only 1 stone producer
        game.players[1].board.played_cards = vec!["stone_pit".to_string()];
        // try to buy stone twice (over request)
        let action = TerminalAction::PlayCard {
            card_id: "baths".to_string(),
            trades: vec![
                Trade { from: Neighbor::Left, resource: Resource::Stone },
                Trade { from: Neighbor::Left, resource: Resource::Stone },
            ],
        };
        let res = game.submit_terminal_action(0, action);
        if let ActionResult::Invalid { reason, .. } = res {
            assert!(reason.contains("Invalid trades") || reason.contains("combo") || reason.contains("over"), "got: {}", reason);
        } else {
            panic!("expected invalid for duplicate purchase");
        }
    }

    #[test]
    fn cannot_purchase_both_options_from_combo_resource_card_in_one_turn() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["workshop".to_string()]; // needs glass, but we'll use a stone/wood card for the combo test by using baths? use a play needing wood and stone? baths needs stone.
        // instead, use a card that needs 1 wood + setup? for simplicity, use baths needing stone, but request wood and stone from a combo that has wood/stone.
        // to hit the both options rule, request wood and stone when only 1 slot.
        game.players[0].current_hand = vec!["baths".to_string()]; // stone
        game.players[0].board.coins = 10;
        // neighbor p2 played forest_cave (wood or stone)
        game.players[2].board.played_cards = vec!["forest_cave".to_string()];
        let action = TerminalAction::PlayCard {
            card_id: "baths".to_string(),
            trades: vec![
                Trade { from: Neighbor::Right, resource: Resource::Wood },
                Trade { from: Neighbor::Right, resource: Resource::Stone },
            ],
        };
        let res = game.submit_terminal_action(0, action);
        if let ActionResult::Invalid { reason, .. } = res {
            assert!(reason.contains("Invalid trades") || reason.contains("combo") || reason.contains("both"), "got: {}", reason);
        } else {
            panic!("expected invalid for buying both from combo");
        }
    }

    #[test]
    fn smoke_scenario_green_cards_require_commodities_even_after_pass() {
        // Repro the user's smoke run situation (after r1 plays that succeeded, hands passed left)
        // p1 played loom (grey producer), p2 played west_trading_post
        // Then in "r2", p0 has scriptorium (needs papyrus) in hand, no papyrus produced yet -> should fail
        // p1 has apothecary (needs loom), and p1 owns loom from r1 play -> should succeed
        let mut game = GameState::new(3);

        // Simulate post r1 state (no resolve needed for this, we set hands and played directly)
        game.players[1].board.played_cards = vec!["loom".to_string()];
        game.players[2].board.played_cards = vec!["west_trading_post".to_string()];
        game.players[2].board.coins = 2; // after paying for the post

        // The hands as seen in user's r2 (6 cards each)
        game.players[0].current_hand = vec![
            "scriptorium".to_string(), "stockade".to_string(), "stone_pit".to_string(),
            "theater".to_string(), "timber_yard".to_string(), "workshop".to_string(),
        ];
        game.players[1].current_hand = vec![
            "apothecary".to_string(), "barracks".to_string(), "baths".to_string(),
            "clay_pit".to_string(), "clay_pool".to_string(), "east_trading_post".to_string(),
        ];
        game.players[2].current_hand = vec![
            "glassworks".to_string(), "guard_tower".to_string(), "lumber_yard".to_string(),
            "marketplace".to_string(), "ore_vein".to_string(), "press".to_string(),
        ];

        // p0 tries scriptorium (papyrus req, but press not played by anyone; p2 has it in the hand but not played)
        let action = TerminalAction::PlayCard {
            card_id: "scriptorium".to_string(),
            trades: vec![],
        };
        let res = game.submit_terminal_action(0, action);
        assert!(
            matches!(res, ActionResult::Invalid { ref reason, .. } if reason.contains("papyrus") || reason.contains("resource") || reason.contains("Insufficient")),
            "scriptorium should be invalid without papyrus producer: {:?}", res
        );
        // hand unchanged
        assert!(game.players[0].current_hand.contains(&"scriptorium".to_string()));

        // Now p1 tries apothecary (loom req); p1 has loom in played from "r1", so own production covers
        let action2 = TerminalAction::PlayCard {
            card_id: "apothecary".to_string(),
            trades: vec![],
        };
        let res2 = game.submit_terminal_action(1, action2);
        // This should succeed because loom is in p1's own played_cards
        assert!(
            matches!(res2, ActionResult::Success { .. }),
            "apothecary should succeed because player owns loom production: {:?}", res2
        );
    }
}
