//! Core game state for Seven Wonders (base game).
//!
//! Current focus: supporting the incremental validation plan.

use super::actions::{ActionResult, SevenWondersAction, TerminalAction, Trade, Neighbor};
use super::cards::CardDatabase;
use super::log::GameLog;
use super::wonders::{
    format_wonder_current_stage, format_wonder_stages_overview, WonderDatabase, WonderStageEffect,
};
use super::scoring::{self, ScoreBreakdown};
use super::types::{Cost, DiscountType, Effect, Resource, Resources};
use serde_json::Value;

/// Represents one player's board state.
#[derive(Debug, Clone)]
pub struct PlayerBoard {
    pub wonder_id: String,
    pub wonder_stages_built: u8,
    pub played_cards: Vec<String>,   // card ids
    pub coins: u8,
    pub military_victory_vp: u8,     // accumulated 1/3/5 VP from winning battles each age
    pub defeat_tokens: u8,           // each worth -1 VP at end of game
    pub science_symbols: Vec<String>,// "gear", "tablet", "compass"
    pub wonder_military: i32,
    pub wonder_choice_slots: Vec<Vec<Resource>>,
}

/// Full state for one player from the engine's perspective.
#[derive(Debug, Clone)]
pub struct PlayerState {
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
    pub wonder_db: WonderDatabase,
    pub current_round_actions: Vec<Vec<TerminalAction>>,
    /// Face-up discard pile (burns and end-of-age discards); most recent at end.
    pub discard_pile: Vec<String>,
    // direction: true for left (player i passes to i+1), false for right
    pub pass_left: bool,
    pub game_over: bool,
    /// Per-player (left_delta, right_delta) from the most recent age's battles.
    pub last_age_battle_deltas: Vec<(i8, i8)>,
    pub final_scores: Option<Vec<ScoreBreakdown>>,
}

impl GameState {
    /// Creates a new game with a unique random civilization (wonder board) per player.
    /// Deals starting hands for Age 1.
    pub fn new(player_count: u8) -> Self {
        Self::new_with_assignment(player_count, Self::default_board_assignment(player_count))
    }

    fn default_board_assignment(player_count: u8) -> Vec<String> {
        let wonder_db = WonderDatabase::load();
        #[cfg(test)]
        {
            wonder_db.assign_fixed(super::wonders::GIZAH_DAY_ID, player_count)
        }
        #[cfg(not(test))]
        {
            wonder_db.assign_unique_random(player_count)
        }
    }

    pub fn new_with_assignment(player_count: u8, wonder_ids: Vec<String>) -> Self {
        assert!((2..=7).contains(&player_count));
        assert_eq!(wonder_ids.len(), player_count as usize);

        let card_db = CardDatabase::load();
        let wonder_db = WonderDatabase::load();

        let mut players = Vec::with_capacity(player_count as usize);
        for wonder_id in wonder_ids {
            players.push(PlayerState {
                board: PlayerBoard {
                    wonder_id,
                    wonder_stages_built: 0,
                    played_cards: vec![],
                    coins: 3,
                    military_victory_vp: 0,
                    defeat_tokens: 0,
                    science_symbols: vec![],
                    wonder_military: 0,
                    wonder_choice_slots: vec![],
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
            wonder_db,
            current_round_actions: vec![vec![]; player_count as usize],
            discard_pile: vec![],
            pass_left: true, // age 1 and 3 left, age 2 right
            game_over: false,
            last_age_battle_deltas: vec![(0, 0); player_count as usize],
            final_scores: None,
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
        self.current_round_actions = vec![vec![]; n];
        self.pass_left = age != 2; // age 1,3 left (to +1), age 2 right (to -1)
    }

    /// Returns the current hand of the given player (card ids).
    #[cfg(test)]
    pub fn get_hand(&self, player: usize) -> &[String] {
        &self.players[player].current_hand
    }

    /// Returns a view of the game from a specific player's perspective.
    /// This is what an agent (LLM or human) should primarily work with.
    pub fn view_for_player(&self, player: usize) -> PlayerView {
        let p = &self.players[player];
        PlayerView {
            hand: p.current_hand.clone(),
            coins: p.board.coins,
            wonder_stages_built: p.board.wonder_stages_built,
        }
    }

    /// Attempts to play a card from the player's hand.
    /// Currently very minimal — just removes the card from hand and records it as played.
    /// Full cost, chaining, and trading logic comes later.
    #[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
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

    #[cfg(test)]
    pub fn check_my_cards(&self, player: usize) -> Vec<String> {
        self.players[player].board.played_cards.clone()
    }

    #[cfg(test)]
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

        let wonder_id = &self.players[player].board.wonder_id;
        let discard_line = if self.discard_pile.is_empty() {
            "discard: []".to_string()
        } else {
            format!("discard: [{}]", self.discard_pile.join(", "))
        };
        format!(
            "hand: {}\ncoins: {}\n{}\n{discard_line}\n\
your_production: [{}]\n\
left (Player {}) production: [{}]\n\
right (Player {}) production: [{}]\n",
            self.card_db.format_hand_block(hand),
            coins,
            format_wonder_current_stage(&self.wonder_db, wonder_id, stages),
            your_prod.join(", "),
            left_p,
            left_prod.join(", "),
            right_p,
            right_prod.join(", ")
        )
    }

    /// All wonder stages for startup prompts (Gizah A).
    pub fn format_wonder_stages_overview(&self, player: usize) -> String {
        format_wonder_stages_overview(
            &self.wonder_db,
            &self.players[player].board.wonder_id,
        )
    }

    pub fn civilization_name(&self, player: usize) -> &str {
        self.wonder_db
            .display_name(&self.players[player].board.wonder_id)
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
        for (i, &(left_delta, right_delta)) in self.last_age_battle_deltas.iter().enumerate() {
            lines.push(format!(
                "Player {} gets {:+}, {:+} from battles",
                i, left_delta, right_delta
            ));
        }
        lines.join("\n")
    }

    pub fn is_game_over(&self) -> bool {
        self.game_over
    }

    fn satisfies_chain(&self, player: usize, chain_from: &Option<Value>) -> bool {
        let Some(val) = chain_from else { return false };
        let played = &self.players[player].board.played_cards;
        match val {
            Value::String(id) => played.iter().any(|c| c == id),
            Value::Array(ids) => ids.iter().any(|v| {
                v.as_str()
                    .map(|id| played.iter().any(|c| c == id))
                    .unwrap_or(false)
            }),
            _ => false,
        }
    }

    fn card_military_strength(&self, card_id: &str) -> i32 {
        let Some(card) = self.card_db.get(card_id) else { return 0 };
        if let Effect::Military(n) = &card.effect {
            return *n;
        }
        if card.color == "red" {
            return match card.age {
                1 => 1,
                2 => 2,
                3 => 3,
                _ => 0,
            };
        }
        0
    }

    pub fn military_strength(&self, player: usize) -> i32 {
        let card_mil: i32 = self.players[player]
            .board
            .played_cards
            .iter()
            .map(|cid| self.card_military_strength(cid))
            .sum();
        card_mil + self.players[player].board.wonder_military
    }

    fn apply_card_effect(&mut self, player: usize, card_id: &str) {
        let Some(card) = self.card_db.get(card_id).cloned() else { return };
        match &card.effect {
            Effect::Coins(n) => {
                if *n > 0 {
                    self.players[player].board.coins =
                        self.players[player].board.coins.saturating_add(*n as u8);
                }
            }
            Effect::Science(sym) => {
                self.players[player]
                    .board
                    .science_symbols
                    .push(scoring::science_symbol_name(*sym).to_string());
            }
            Effect::VictoryPoints(_) | Effect::Military(_) => {}
            _ => {}
        }
    }

    pub(crate) fn discard_end_of_age_cards(&mut self) {
        for player in &mut self.players {
            if player.current_hand.len() == 1 {
                let card = player.current_hand.pop().expect("single card");
                self.discard_pile.push(card);
            }
        }
    }

    fn hand_card_id(action: &TerminalAction) -> &str {
        match action {
            TerminalAction::PlayCard { card_id, .. }
            | TerminalAction::BuildWonder { card_id, .. }
            | TerminalAction::BurnCard { card_id } => card_id,
        }
    }

    fn has_built_effect(&self, player: usize, effect: WonderStageEffect) -> bool {
        self.built_wonder_effects(player).contains(&effect)
    }

    pub fn actions_required_for_player(&self, player: usize) -> usize {
        if self.round_in_age == 6 && self.has_built_effect(player, WonderStageEffect::SixthRoundExtraPlay) {
            if self.players[player].current_hand.len() >= 2 {
                return 2;
            }
        }
        1
    }

    pub fn player_round_complete(&self, player: usize) -> bool {
        self.current_round_actions[player].len() >= self.actions_required_for_player(player)
    }

    fn all_players_round_complete(&self) -> bool {
        (0..self.player_count as usize).all(|p| self.player_round_complete(p))
    }

    fn validate_discard_play(&self, player: usize, card_id: &str) -> Result<(), String> {
        if !self.discard_pile.iter().any(|c| c == card_id) {
            return Err(format!("Card {} is not in the discard pile", card_id));
        }
        if self.players[player].board.played_cards.iter().any(|c| c == card_id) {
            return Err(format!(
                "You have already built {}; each card can only be played once",
                card_id
            ));
        }
        Ok(())
    }

    fn apply_discard_play(&mut self, player: usize, card_id: &str) {
        if let Some(pos) = self.discard_pile.iter().position(|c| c == card_id) {
            self.discard_pile.remove(pos);
        }
        self.players[player].board.played_cards.push(card_id.to_string());
        self.apply_card_effect(player, card_id);
    }

    fn battle_reward_for_age(age: u8) -> u8 {
        match age {
            1 => 1,
            2 => 3,
            3 => 5,
            _ => 0,
        }
    }

    fn finalize_game(&mut self) {
        let n = self.player_count as usize;
        let mut scores = Vec::with_capacity(n);
        for i in 0..n {
            scores.push(scoring::compute_final_score(
                &self.card_db,
                &self.wonder_db,
                i,
                &self.players,
                self.player_count,
            ));
        }
        self.final_scores = Some(scores);
        self.game_over = true;
    }

    // ==================== Resource / Cost / Trade helpers (for validation & apply) ====================

    fn compute_fixed_production(&self, player: usize) -> Resources {
        let mut prod = Resources::default();
        if let Some(r) = self
            .wonder_db
            .token_resource_for(&self.players[player].board.wonder_id)
        {
            prod.add(r, 1);
        }
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

    /// Choice-production cards whose output is for the owner's use only (not purchasable by neighbors).
    const NON_TRADEABLE_PRODUCTION_CARDS: &'static [&'static str] = &["caravansery", "forum"];

    fn collect_choice_options(&self, player: usize) -> Vec<Vec<Resource>> {
        self.collect_choice_options_inner(player, false)
    }

    fn collect_tradeable_choice_options(&self, player: usize) -> Vec<Vec<Resource>> {
        self.collect_choice_options_inner(player, true)
    }

    fn collect_choice_options_inner(&self, player: usize, exclude_non_tradeable: bool) -> Vec<Vec<Resource>> {
        let mut choices = self.players[player].board.wonder_choice_slots.clone();
        for cid in &self.players[player].board.played_cards {
            if exclude_non_tradeable && Self::NON_TRADEABLE_PRODUCTION_CARDS.contains(&cid.as_str()) {
                continue;
            }
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
        let mut slots = self.collect_tradeable_choice_options(neigh_player);
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
        self.wonder_db.stage_cost(wonder_id, stage)
    }

    fn built_wonder_effects(&self, player: usize) -> Vec<WonderStageEffect> {
        let board_id = &self.players[player].board.wonder_id;
        let built = self.players[player].board.wonder_stages_built;
        let mut effects = Vec::new();
        for stage in 1..=built {
            if let Some(def) = self.wonder_db.stage(board_id, stage) {
                if let Some(effect) = &def.effect {
                    effects.push(*effect);
                }
            }
        }
        effects
    }

    fn wonder_free_play_applies(&self, player: usize, card_id: &str) -> bool {
        let Some(card) = self.card_db.get(card_id) else {
            return false;
        };
        let effects = self.built_wonder_effects(player);
        if effects.contains(&WonderStageEffect::FirstPerAgeFree) && self.round_in_age == 1 {
            return true;
        }
        if effects.contains(&WonderStageEffect::LastPerAgeFree) && self.round_in_age == 6 {
            return true;
        }
        if effects.contains(&WonderStageEffect::FirstPerColorFree) {
            let already_built_color = self.players[player].board.played_cards.iter().any(|cid| {
                self.card_db
                    .get(cid)
                    .map(|c| c.color == card.color)
                    .unwrap_or(false)
            });
            if !already_built_color {
                return true;
            }
        }
        false
    }

    fn apply_wonder_stage_rewards(&mut self, player: usize, stage: u8) {
        let board_id = self.players[player].board.wonder_id.clone();
        let Some(stage_def) = self.wonder_db.stage(&board_id, stage).cloned() else {
            return;
        };
        if stage_def.coins > 0 {
            self.players[player].board.coins = self.players[player]
                .board
                .coins
                .saturating_add(stage_def.coins as u8);
        }
        if stage_def.military > 0 {
            self.players[player].board.wonder_military += stage_def.military;
        }
        if let Some(effect) = stage_def.effect {
            match effect {
                WonderStageEffect::ScienceChoice => {
                    self.players[player]
                        .board
                        .science_symbols
                        .push("compass".to_string());
                }
                WonderStageEffect::ProduceRawChoice => {
                    self.players[player].board.wonder_choice_slots.push(vec![
                        Resource::Wood,
                        Resource::Stone,
                        Resource::Ore,
                        Resource::Clay,
                    ]);
                }
                WonderStageEffect::ProduceManufacturedChoice => {
                    self.players[player].board.wonder_choice_slots.push(vec![
                        Resource::Glass,
                        Resource::Loom,
                        Resource::Papyrus,
                    ]);
                }
                WonderStageEffect::PlayFromDiscard
                | WonderStageEffect::SixthRoundExtraPlay
                | WonderStageEffect::FirstPerColorFree
                | WonderStageEffect::FirstPerAgeFree
                | WonderStageEffect::LastPerAgeFree => {}
            }
        }
    }

    fn wonder_stage_grants_discard_play(&self, board_id: &str, stage: u8) -> bool {
        self.wonder_db
            .stage(board_id, stage)
            .and_then(|s| s.effect)
            == Some(WonderStageEffect::PlayFromDiscard)
    }

    fn validate_card_play(&self, player: usize, card_id: &str, trades: &[Trade]) -> Result<(), String> {
        let card = self.card_db.get(card_id).ok_or_else(|| "Unknown card".to_string())?;
        if self.players[player].board.played_cards.iter().any(|c| c == card_id) {
            return Err(format!(
                "You have already built {}; each card can only be played once",
                card_id
            ));
        }
        if self.satisfies_chain(player, &card.chain_from) || self.wonder_free_play_applies(player, card_id) {
            return Ok(());
        }
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

    fn validate_wonder_stage(
        &self,
        player: usize,
        stage: u8,
        trades: &[Trade],
        discard_play: Option<&str>,
    ) -> Result<(), String> {
        let current = self.players[player].board.wonder_stages_built;
        if stage != current + 1 {
            return Err("Can only build the next wonder stage in sequence".to_string());
        }
        let board_id = self.players[player].board.wonder_id.clone();
        let wcost = self.wonder_stage_cost(&board_id, stage);
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

        let grants_discard = self.wonder_stage_grants_discard_play(&board_id, stage);
        match (grants_discard, discard_play) {
            (true, None) if !self.discard_pile.is_empty() => {
                return Err(
                    "This wonder stage grants a free play from discard; add 'discard <card_id>' to your wonder action"
                        .to_string(),
                );
            }
            (false, Some(_)) => {
                return Err(
                    "discard play is only valid when building a wonder stage with that effect".to_string(),
                );
            }
            (_, Some(card_id)) => self.validate_discard_play(player, card_id)?,
            _ => {}
        }
        Ok(())
    }

    fn validate_afford(&self, player: usize, action: &TerminalAction) -> Result<(), String> {
        match action {
            TerminalAction::PlayCard { card_id, trades } => self.validate_card_play(player, card_id, trades),
            TerminalAction::BuildWonder {
                stage,
                trades,
                discard_play,
                ..
            } => self.validate_wonder_stage(player, *stage, trades, discard_play.as_deref()),
            TerminalAction::BurnCard { .. } => Ok(()),
        }
    }

    /// Submit a terminal action for the current round.
    /// If all players have submitted, resolves the round (applies actions, passes hands, etc.).
    pub fn submit_terminal_action(&mut self, player: usize, action: TerminalAction) -> ActionResult {
        if self.player_round_complete(player) {
            return ActionResult::Invalid {
                reason: "Already submitted all actions for this round".to_string(),
                suggested_actions: vec![],
            };
        }
        let card_id = Self::hand_card_id(&action);
        if !self.players[player].current_hand.iter().any(|c| c == card_id) {
            return ActionResult::Invalid {
                reason: format!("Card {} not in hand", card_id),
                suggested_actions: vec![],
            };
        }
        if self
            .current_round_actions[player]
            .iter()
            .any(|a| Self::hand_card_id(a) == card_id)
        {
            return ActionResult::Invalid {
                reason: format!("Card {} already used for an action this round", card_id),
                suggested_actions: vec![],
            };
        }
        if let Err(reason) = self.validate_afford(player, &action) {
            return ActionResult::Invalid {
                reason,
                suggested_actions: vec![],
            };
        }
        self.current_round_actions[player].push(action);
        if self.all_players_round_complete() {
            self.resolve_round();
        }
        ActionResult::Success { message: Some("Action submitted for round".to_string()) }
    }

    fn resolve_round(&mut self) {
        let actions: Vec<Vec<TerminalAction>> = self.current_round_actions.clone();
        for (i, player_actions) in actions.iter().enumerate() {
            for action in player_actions {
                let card_id = Self::hand_card_id(action);
                if let Some(pos) = self.players[i].current_hand.iter().position(|c| c == card_id) {
                    self.players[i].current_hand.remove(pos);
                }
                match action {
                    TerminalAction::PlayCard { card_id, trades } => {
                        self.players[i].board.played_cards.push(card_id.clone());
                        let chained = self
                            .card_db
                            .get(card_id)
                            .map(|c| self.satisfies_chain(i, &c.chain_from))
                            .unwrap_or(false);
                        let wonder_free = self.wonder_free_play_applies(i, card_id);
                        if !chained && !wonder_free {
                            let card_c =
                                self.card_db.get(card_id).map(|c| c.cost.coins as u32).unwrap_or(0);
                            let t_c = self.compute_trade_coins(i, trades);
                            let pay = (card_c + t_c) as u8;
                            self.players[i].board.coins =
                                self.players[i].board.coins.saturating_sub(pay);
                        }
                        self.apply_card_effect(i, card_id);
                    }
                    TerminalAction::BuildWonder {
                        stage,
                        trades,
                        discard_play,
                        ..
                    } => {
                        self.players[i].board.wonder_stages_built += 1;
                        let w_c =
                            self.wonder_stage_cost(&self.players[i].board.wonder_id, *stage).coins
                                as u32;
                        let t_c = self.compute_trade_coins(i, trades);
                        let pay = (w_c + t_c) as u8;
                        self.players[i].board.coins =
                            self.players[i].board.coins.saturating_sub(pay);
                        self.apply_wonder_stage_rewards(i, *stage);
                        if let Some(discard_card) = discard_play {
                            self.apply_discard_play(i, discard_card);
                        }
                    }
                    TerminalAction::BurnCard { card_id } => {
                        self.players[i].board.coins =
                            self.players[i].board.coins.saturating_add(3);
                        self.discard_pile.push(card_id.clone());
                    }
                }
            }
        }

        // Pass hands
        self.pass_hands();

        self.round_in_age += 1;
        self.current_round_actions = vec![vec![]; self.player_count as usize];

        if self.round_in_age > 6 {
            self.discard_end_of_age_cards();
            self.resolve_battles();
            if self.current_age < 3 {
                self.current_age += 1;
                self.start_age();
            } else {
                self.finalize_game();
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
        let n = self.player_count as usize;
        let age = self.current_age;
        let win_vp = Self::battle_reward_for_age(age);
        let mut deltas = vec![(0i8, 0i8); n];

        for i in 0..n {
            let str_i = self.military_strength(i);
            let left = self.neighbor_player(i, Neighbor::Left);
            let right = self.neighbor_player(i, Neighbor::Right);
            let str_left = self.military_strength(left);
            let str_right = self.military_strength(right);

            let left_delta = if str_i > str_left {
                self.players[i].board.military_victory_vp += win_vp;
                win_vp as i8
            } else if str_i < str_left {
                self.players[i].board.defeat_tokens += 1;
                -1
            } else {
                0
            };

            let right_delta = if str_i > str_right {
                self.players[i].board.military_victory_vp += win_vp;
                win_vp as i8
            } else if str_i < str_right {
                self.players[i].board.defeat_tokens += 1;
                -1
            } else {
                0
            };

            deltas[i] = (left_delta, right_delta);
        }

        self.last_age_battle_deltas = deltas;
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
        TerminalAction::BuildWonder {
            stage,
            discard_play,
            ..
        } => {
            lines.push(format!("Player {} built wonder stage {}", player, stage));
            if let Some(card) = discard_play {
                lines.push(format!("Player {} played {} from discard", player, card));
            }
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
    } else if r.contains("not in hand") || r.contains("already built") {
        format!("ERROR: {}\n", reason)
    } else {
        format!("ERROR: {}\n", reason)
    }
}

/// Run a full (stub) game with the given controllers.
/// Each round, for each player, the controller's decide_action is called (which can loop observations),
/// then the terminal action is submitted.
/// After 6 rounds, battles (stub), next age, etc.
/// Outcome of a Seven Wonders session (full or limited rounds).
#[derive(Debug, Clone)]
pub struct SevenWondersGameOutcome {
    pub player_count: u8,
    pub rounds_played: u32,
    pub game_complete: bool,
    pub final_scores: Option<Vec<super::scoring::ScoreBreakdown>>,
}

pub fn run_game(controllers: Vec<Box<dyn super::controller::PlayerController>>) -> SevenWondersGameOutcome {
    run_limited_rounds_game(controllers, u32::MAX)
}

/// Dedicated limited-rounds game runner (used by smoke tests and normal runs).
/// max_rounds: total number of rounds to play before stopping (across ages).
///
/// This is where the single shared log.txt + personalized per-agent views are built:
/// - One GameLog accumulates the canonical full plain-text log (with private decision blocks for whoever was deciding).
/// - For LLM and interactive human controllers, we supply get_decision_view_for which only contains
///   completed prior round summaries + the current round header + open decision block.
/// - Same-round actions (even from earlier players in sequential execution) are hidden from the view.
/// - Autos (FirstPurchaseable) behavior is untouched: they call is_valid_terminal_action directly.
/// - Interactive humans re-prompt until a legal action; no auto-burn on parse failure or invalid play.
/// - LLM agents get up to 3 attempts on illegal actions, then a forced burn fallback.
/// - Full log is written to log.txt after every decision close and every round (for live `cat log.txt` or `tail -f`).
/// - Console also prints the FULL LOG and the exact PERSONALIZED VIEW SENT each time for the agent.
pub fn run_limited_rounds_game(
    controllers: Vec<Box<dyn super::controller::PlayerController>>,
    max_rounds: u32,
) -> SevenWondersGameOutcome {
    run_limited_rounds_game_with_boards(controllers, max_rounds, None)
}

pub fn run_limited_rounds_game_with_boards(
    mut controllers: Vec<Box<dyn super::controller::PlayerController>>,
    max_rounds: u32,
    wonder_board_ids: Option<Vec<String>>,
) -> SevenWondersGameOutcome {
    let n = controllers.len() as u8;
    let mut game = match wonder_board_ids {
        Some(ids) => GameState::new_with_assignment(n, ids),
        None => GameState::new(n),
    };
    let mut game_log = GameLog::new();

    let is_smoke = max_rounds <= 4;
    let label = if max_rounds == 4 {
        " (SMOKE: 4 rounds)"
    } else if max_rounds == 2 {
        " (SMOKE: 2 rounds)"
    } else {
        ""
    };
    println!("Starting Seven Wonders game{} with {} players.", label, n);
    for p in 0..n as usize {
        println!("  Player {} civilization: {}", p, game.civilization_name(p));
        println!(
            "    {}",
            game.format_wonder_stages_overview(p)
                .lines()
                .next()
                .unwrap_or("")
        );
    }

    game_log.start_age(game.current_age);
    println!("\n=== Age {} ===", game.current_age);

    let mut total_rounds_played = 0u32;
    while total_rounds_played < max_rounds && !game.is_game_over() {
        let round = game.round_in_age;
        game.current_round_actions = vec![vec![]; n as usize];
        println!("\n-- Round {} (Age {}) --", round, game.current_age);

        game_log.start_round(round);
        let age_before = game.current_age;

        for p in 0..n as usize {
            use super::controller::ControllerDecisionMode;

            match controllers[p].decision_mode() {
                ControllerDecisionMode::InteractiveHuman | ControllerDecisionMode::LlmAgent => {
                    let private = game.get_private_decision_info(p);
                    game_log.begin_player_decision(p, private);

                    let max_attempts = match controllers[p].decision_mode() {
                        ControllerDecisionMode::LlmAgent => 3,
                        _ => u32::MAX,
                    };
                    let mut attempts = 0u32;
                    let mut got_success = false;

                    loop {
                        attempts += 1;
                        if attempts > max_attempts {
                            println!("[p{}] {} attempts exhausted - forcing fallback burn", p, max_attempts);
                            break;
                        }

                        let view = game_log.get_decision_view_for(p);
                        let action = controllers[p].decide_action(&game, p, Some(&view));

                        if let SevenWondersAction::Terminal(term) = action {
                            let res = game.submit_terminal_action(p, term.clone());
                            let is_human = matches!(
                                controllers[p].decision_mode(),
                                super::controller::ControllerDecisionMode::InteractiveHuman
                            );
                            if is_human {
                                super::term::print_action_result(p, &format!("{res:?}"));
                            } else {
                                println!("Player {} result: {:?}", p, res);
                            }

                            if matches!(res, ActionResult::Success { .. }) {
                                if game.player_round_complete(p) {
                                    let summary: Vec<String> = game.current_round_actions[p]
                                        .iter()
                                        .flat_map(|a| format_action_summary_lines(&game, p, a))
                                        .collect();
                                    game_log.close_current_decision(&summary);
                                    got_success = true;
                                    break;
                                }
                                game_log.append_to_current_decision(
                                    "Play your second card this round (Babylon sixth-round bonus).\n",
                                );
                            } else if let ActionResult::Invalid { reason, .. } = res {
                                let err = format_error_for_log(&reason);
                                if is_human {
                                    super::term::print_error_line(err.trim());
                                } else {
                                    println!("{}", err.trim());
                                }
                                game_log.append_to_current_decision(&err);
                            }
                        } else {
                            let err = "ERROR: Only play, wonder, and burn actions are allowed.\n";
                            if matches!(
                                controllers[p].decision_mode(),
                                super::controller::ControllerDecisionMode::InteractiveHuman
                            ) {
                                super::term::print_error_line(err.trim());
                            } else {
                                println!("{}", err.trim());
                            }
                            game_log.append_to_current_decision(err);
                        }
                    }

                    if !got_success {
                        if let Some(card) = game.players[p].current_hand.first().cloned() {
                            let fb = TerminalAction::BurnCard { card_id: card.clone() };
                            let _ = game.submit_terminal_action(p, fb.clone());
                            let summary = format_action_summary_lines(&game, p, &fb);
                            game_log.close_current_decision(&summary);
                        }
                    }
                }
                ControllerDecisionMode::Auto => {
                    loop {
                        let action = controllers[p].decide_action(&game, p, None);
                        if let SevenWondersAction::Terminal(term) = action {
                            let res = game.submit_terminal_action(p, term.clone());
                            println!("Player {} result: {:?}", p, res);
                            if matches!(res, ActionResult::Success { .. }) {
                                let summary = format_action_summary_lines(&game, p, &term);
                                game_log.append_simple_player_action(&summary);
                                if game.player_round_complete(p) {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        total_rounds_played += 1;
        game_log.add_round_summary(round);

        if game.current_age > age_before {
            game_log.close_age(&game.get_age_summary());
            if !game.is_game_over() {
                game_log.start_age(game.current_age);
                println!("\n=== Age {} ===", game.current_age);
            }
        }
    }

    if game.is_game_over() {
        if let Some(scores) = &game.final_scores {
            println!("\n=== FINAL SCORES ===");
            for (i, s) in scores.iter().enumerate() {
                println!(
                    "Player {}: {} VP (military +{}, defeats {}, treasury {}, wonders {}, civilian {}, science {}, guilds {}, commerce {})",
                    i,
                    s.total,
                    s.military_victory,
                    s.military_defeat,
                    s.treasury,
                    s.wonders,
                    s.civilian,
                    s.science,
                    s.guilds,
                    s.commerce
                );
            }
        }
    }

    game_log.write_to_disk();

    if max_rounds == 4 {
        println!("\nFinished after 4 rounds. Full log in log.txt.");
    } else if is_smoke {
        println!("\nSmoke game finished after {} rounds.", max_rounds);
    } else if game.is_game_over() {
        println!("\nGame finished.");
    } else {
        println!("\nStopped after {} rounds.", max_rounds);
    }

    SevenWondersGameOutcome {
        player_count: n,
        rounds_played: total_rounds_played,
        game_complete: game.is_game_over(),
        final_scores: game.final_scores.clone(),
    }
}

/// A view of the game from one player's perspective.
/// Observation tools should return data in (or populate) structures like this.
#[derive(Debug, Clone)]
pub struct PlayerView {
    pub hand: Vec<String>,
    pub coins: u8,
    pub wonder_stages_built: u8,
}

#[cfg(test)]
mod tests {
    use super::GameState;
    use crate::games::seven_wonders::actions::{ActionResult, TerminalAction};
    use crate::games::seven_wonders::types::{Neighbor, Resource, Trade};

    fn expect_invalid(res: ActionResult, hint: &str) {
        match res {
            ActionResult::Invalid { reason, .. } => {
                assert!(
                    reason.to_lowercase().contains(&hint.to_lowercase()),
                    "expected error containing {:?}, got: {}",
                    hint,
                    reason
                );
            }
            other => panic!("expected Invalid containing {:?}, got {:?}", hint, other),
        }
    }

    /// Set cumulative military strength using red cards (+1/+2/+3).
    fn set_military_strength(game: &mut GameState, player: usize, strength: i32) {
        assert!(strength >= 0, "strength must be non-negative");
        let mut cards = Vec::new();
        let mut rem = strength;
        for _ in 0..(rem / 3) {
            cards.push("fortifications".to_string());
        }
        rem %= 3;
        for _ in 0..(rem / 2) {
            cards.push("stables".to_string());
        }
        rem %= 2;
        for _ in 0..rem {
            cards.push("stockade".to_string());
        }
        game.players[player].board.played_cards = cards;
        assert_eq!(
            game.military_strength(player),
            strength,
            "failed to set military strength for player {player}"
        );
    }

    fn resolve_battles_for_age(
        game: &mut GameState,
        age: u8,
        strengths: [i32; 3],
    ) -> Vec<(i8, i8)> {
        game.current_age = age;
        for (p, &s) in strengths.iter().enumerate() {
            set_military_strength(game, p, s);
            game.players[p].board.military_victory_vp = 0;
            game.players[p].board.defeat_tokens = 0;
        }
        game.resolve_battles();
        game.last_age_battle_deltas.clone()
    }

    fn assert_battle_deltas(
        age: u8,
        strengths: [i32; 3],
        expected: [(i8, i8); 3],
        label: &str,
    ) {
        let mut game = GameState::new(3);
        let deltas = resolve_battles_for_age(&mut game, age, strengths);
        assert_eq!(
            deltas, expected,
            "{label}: age {age} strengths {strengths:?}"
        );
    }

    /// Invalid actions must not queue a round action or spend resources/coins early.
    fn assert_no_action_queued(game: &GameState, player: usize, card_id: &str, coins_before: u8) {
        assert!(game.current_round_actions[player].is_empty());
        assert!(game.players[player].current_hand.contains(&card_id.to_string()));
        assert_eq!(game.players[player].board.coins, coins_before);
        assert!(!game.players[player].board.played_cards.contains(&card_id.to_string()));
    }

    /// Default test boards use paper (Ephesos) so civilization tokens do not collide with
    /// stone/wood/clay-focused scenarios.
    fn new_resource_test_game(player_count: u8) -> GameState {
        GameState::new_with_assignment(
            player_count,
            vec!["ephesos_day".to_string(); player_count as usize],
        )
    }

    fn setup_round2_player1_scenario() -> GameState {
        let mut game = new_resource_test_game(3);
        // Mirror log: after age-1 round 1, player 1 chose guard_tower; neighbors played red cards.
        game.players[0].board.played_cards = vec!["barracks".to_string()];
        game.players[1].board.played_cards = vec!["guard_tower".to_string()];
        game.players[2].board.played_cards = vec!["stockade".to_string()];
        game.players[1].current_hand = vec![
            "altar".to_string(),
            "apothecary".to_string(),
            "baths".to_string(),
            "clay_pit".to_string(),
            "clay_pool".to_string(),
            "east_trading_post".to_string(),
        ];
        game.players[1].board.coins = 3;
        game
    }

    /// Test helper: have every player except `focal` burn until the round can resolve.
    fn complete_round_for_tests(game: &mut GameState, focal: usize) {
        for p in 0..game.player_count as usize {
            if p == focal {
                continue;
            }
            while !game.player_round_complete(p) {
                if game.players[p].current_hand.is_empty() {
                    game.players[p].current_hand.push("lumber_yard".to_string());
                }
                let card = game.players[p].current_hand[0].clone();
                let _ = game.submit_terminal_action(
                    p,
                    TerminalAction::BurnCard { card_id: card },
                );
            }
        }
    }

    #[test]
    fn private_decision_info_includes_wonder_current_stage() {
        let game = GameState::new(3);
        let info = game.get_private_decision_info(0);
        assert!(info.contains("wonder\t(0/3)\t[wood, wood]\t+3 points"));

        let mut game2 = GameState::new(3);
        game2.players[0].board.wonder_stages_built = 1;
        let info2 = game2.get_private_decision_info(0);
        assert!(info2.contains("wonder\t(1/3)\t[brick, brick, cloth]\t+5 points"));
    }

    #[test]
    fn wonder_stages_overview_lists_all_gizah_stages() {
        let game = GameState::new(3);
        let overview = game.format_wonder_stages_overview(0);
        assert!(overview.contains("Gizah (day)"));
        assert!(overview.contains("(1/3)"));
        assert!(overview.contains("(2/3)"));
        assert!(overview.contains("(3/3)"));
    }

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
        // stage 1 Gizah A requires 2 wood, no wood producers
        let action = TerminalAction::BuildWonder {
            card_id: "lumber_yard".to_string(),
            stage: 1,
            trades: vec![],
            discard_play: None,
        };
        let res = game.submit_terminal_action(0, action);
        if let ActionResult::Invalid { reason, .. } = res {
            assert!(
                reason.contains("resource") || reason.contains("wood") || reason.contains("Insufficient"),
                "got: {reason}"
            );
        } else {
            panic!("expected resource fail for wonder");
        }
    }

    #[test]
    fn wonder_stage_fails_if_coins_insufficient_for_trades() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["lumber_yard".to_string()];
        game.players[0].board.coins = 1; // low, will need 4 for 2 wood @2 each
        game.players[2].board.played_cards = vec!["lumber_yard".to_string(), "lumber_yard".to_string()];
        let action = TerminalAction::BuildWonder {
            card_id: "lumber_yard".to_string(),
            stage: 1,
            trades: vec![
                Trade { from: Neighbor::Left, resource: Resource::Wood },
                Trade { from: Neighbor::Left, resource: Resource::Wood },
            ],
            discard_play: None,
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
        game.players[0].current_hand = vec!["workshop".to_string()];
        game.players[0].board.coins = 1;
        game.players[0].board.played_cards = vec!["marketplace".to_string()];
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
    fn cannot_buy_resources_from_neighbor_caravansery_or_forum() {
        let mut game = new_resource_test_game(3);
        game.players[0].current_hand = vec!["baths".to_string()];
        game.players[0].board.coins = 10;
        game.players[2].board.played_cards = vec!["caravansery".to_string()];
        let caravansery_trade = TerminalAction::PlayCard {
            card_id: "baths".to_string(),
            trades: vec![Trade {
                from: Neighbor::Left,
                resource: Resource::Stone,
            }],
        };
        expect_invalid(
            game.submit_terminal_action(0, caravansery_trade),
            "invalid trades",
        );

        let mut game2 = GameState::new(3);
        game2.players[0].current_hand = vec!["workshop".to_string()];
        game2.players[0].board.coins = 10;
        game2.players[2].board.played_cards = vec!["forum".to_string()];
        let forum_trade = TerminalAction::PlayCard {
            card_id: "workshop".to_string(),
            trades: vec![Trade {
                from: Neighbor::Left,
                resource: Resource::Glass,
            }],
        };
        expect_invalid(
            game2.submit_terminal_action(0, forum_trade),
            "invalid trades",
        );

        // Other combo producers (e.g. forest_cave) remain tradeable.
        let mut game3 = GameState::new(3);
        game3.players[0].current_hand = vec!["barracks".to_string()];
        game3.players[0].board.coins = 10;
        game3.players[2].board.played_cards = vec!["forest_cave".to_string()];
        let forest_cave_trade = TerminalAction::PlayCard {
            card_id: "barracks".to_string(),
            trades: vec![Trade {
                from: Neighbor::Left,
                resource: Resource::Ore,
            }],
        };
        assert!(
            game3.is_valid_terminal_action(0, &forest_cave_trade),
            "forest_cave should still be tradeable"
        );
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
    fn chain_building_allows_free_play_without_resources() {
        let mut game = GameState::new(3);
        game.players[0].board.played_cards = vec!["well".to_string()];
        game.players[0].current_hand = vec!["statue".to_string()];
        game.players[1].current_hand = vec!["lumber_yard".to_string()];
        game.players[2].current_hand = vec!["ore_vein".to_string()];
        let action = TerminalAction::PlayCard {
            card_id: "statue".to_string(),
            trades: vec![],
        };
        let _ = game.submit_terminal_action(1, TerminalAction::BurnCard {
            card_id: "lumber_yard".to_string(),
        });
        let _ = game.submit_terminal_action(2, TerminalAction::BurnCard {
            card_id: "ore_vein".to_string(),
        });
        let res = game.submit_terminal_action(0, action);
        assert!(matches!(res, ActionResult::Success { .. }), "chained statue should be free: {:?}", res);
    }

    #[test]
    fn end_of_age_discard_removes_last_card() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["lumber_yard".to_string()];
        game.discard_end_of_age_cards();
        assert!(game.players[0].current_hand.is_empty());
        game.players[1].current_hand = vec!["a".to_string(), "b".to_string()];
        game.discard_end_of_age_cards();
        assert_eq!(game.players[1].current_hand.len(), 2);
    }

    #[test]
    fn military_battle_assigns_victory_and_defeat() {
        let mut game = GameState::new(3);
        game.players[0].board.played_cards = vec!["stockade".to_string()];
        game.players[1].board.played_cards = vec![];
        game.players[2].board.played_cards = vec!["barracks".to_string(), "guard_tower".to_string()];
        game.resolve_battles();
        assert!(game.players[0].board.military_victory_vp > 0);
        assert!(game.players[1].board.defeat_tokens > 0);
        assert_eq!(game.last_age_battle_deltas[0].0, -1); // lost to left neighbor's stronger army
        assert_eq!(game.last_age_battle_deltas[0].1, 1);  // beat right neighbor
    }

    #[test]
    fn military_battle_deltas_age1() {
        assert_battle_deltas(
            1,
            [1, 2, 3],
            [(-1, -1), (1, -1), (1, 1)],
            "age1 [1,2,3]",
        );
        assert_battle_deltas(1, [2, 2, 2], [(0, 0), (0, 0), (0, 0)], "age1 [2,2,2]");
        assert_battle_deltas(
            1,
            [0, 10, 0],
            [(0, -1), (1, 1), (-1, 0)],
            "age1 [0,10,0]",
        );
    }

    #[test]
    fn military_battle_deltas_age2() {
        assert_battle_deltas(
            2,
            [2, 2, 5],
            [(-1, 0), (0, -1), (3, 3)],
            "age2 [2,2,5]",
        );
        assert_battle_deltas(
            2,
            [3, 4, 5],
            [(-1, -1), (3, -1), (3, 3)],
            "age2 [3,4,5]",
        );
        assert_battle_deltas(2, [2, 2, 2], [(0, 0), (0, 0), (0, 0)], "age2 [2,2,2]");
    }

    #[test]
    fn military_battle_deltas_age3() {
        assert_battle_deltas(
            3,
            [4, 5, 6],
            [(-1, -1), (5, -1), (5, 5)],
            "age3 [4,5,6]",
        );
        assert_battle_deltas(3, [2, 2, 2], [(0, 0), (0, 0), (0, 0)], "age3 [2,2,2]");
        assert_battle_deltas(
            3,
            [7, 5, 7],
            [(0, 5), (-1, -1), (5, 0)],
            "age3 [7,5,7]",
        );
    }

    #[test]
    fn science_card_adds_symbol_on_play() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["workshop".to_string()];
        game.players[0].board.played_cards = vec!["glassworks".to_string()];
        game.players[1].current_hand = vec!["lumber_yard".to_string()];
        game.players[2].current_hand = vec!["ore_vein".to_string()];
        let _ = game.submit_terminal_action(1, TerminalAction::BurnCard {
            card_id: "lumber_yard".to_string(),
        });
        let _ = game.submit_terminal_action(2, TerminalAction::BurnCard {
            card_id: "ore_vein".to_string(),
        });
        let _ = game.submit_terminal_action(0, TerminalAction::PlayCard {
            card_id: "workshop".to_string(),
            trades: vec![],
        });
        assert!(game.players[0].board.science_symbols.contains(&"gear".to_string()));
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

    // ==================== Validation tests requested ====================

    #[test]
    fn card_database_loads_resource_and_coin_costs() {
        let game = GameState::new(3);
        let apothecary = game.card_db.get("apothecary").expect("apothecary");
        assert_eq!(apothecary.cost.resources.get(Resource::Loom), 1);
        assert_eq!(apothecary.cost.coins, 0);

        let altar = game.card_db.get("altar").expect("altar");
        assert!(altar.cost.resources.is_empty());
        assert_eq!(altar.cost.coins, 0);

        let clay_pit = game.card_db.get("clay_pit").expect("clay_pit");
        assert_eq!(clay_pit.cost.coins, 1);
        assert!(clay_pit.cost.resources.is_empty());

        let east_post = game.card_db.get("east_trading_post").expect("east_trading_post");
        assert_eq!(east_post.cost.coins, 0);

        let guard_tower = game.card_db.get("guard_tower").expect("guard_tower");
        assert_eq!(guard_tower.cost.resources.get(Resource::Clay), 1);
    }

    #[test]
    fn guard_tower_requires_brick_without_production_or_trades() {
        let mut game = GameState::new(3);
        game.players[1].current_hand = vec!["guard_tower".to_string()];
        game.players[1].board.coins = 3;
        let res = game.submit_terminal_action(
            1,
            TerminalAction::PlayCard {
                card_id: "guard_tower".to_string(),
                trades: vec![],
            },
        );
        expect_invalid(res, "insufficient resources");
        assert_no_action_queued(&game, 1, "guard_tower", 3);
    }

    #[test]
    fn round2_log_hand_rejects_resource_cost_cards_without_trades() {
        let mut game = setup_round2_player1_scenario();
        let player = 1;

        for card_id in ["apothecary", "baths"] {
            let res = game.submit_terminal_action(
                player,
                TerminalAction::PlayCard {
                    card_id: card_id.to_string(),
                    trades: vec![],
                },
            );
            expect_invalid(res, "insufficient resources");
            assert_no_action_queued(&game, player, card_id, 3);
        }
    }

    #[test]
    fn round2_log_hand_allows_free_and_coin_only_cards() {
        let mut game = setup_round2_player1_scenario();
        let player = 1;
        complete_round_for_tests(&mut game, player);

        let res_pool = game.submit_terminal_action(
            player,
            TerminalAction::PlayCard {
                card_id: "clay_pool".to_string(),
                trades: vec![],
            },
        );
        assert!(matches!(res_pool, ActionResult::Success { .. }), "{:?}", res_pool);
        assert!(game.players[player].board.played_cards.contains(&"clay_pool".to_string()));
        assert_eq!(game.players[player].board.coins, 3);

        let mut game2 = setup_round2_player1_scenario();
        complete_round_for_tests(&mut game2, player);
        let res_pit = game2.submit_terminal_action(
            player,
            TerminalAction::PlayCard {
                card_id: "clay_pit".to_string(),
                trades: vec![],
            },
        );
        assert!(matches!(res_pit, ActionResult::Success { .. }), "{:?}", res_pit);
        assert_eq!(game2.players[player].board.coins, 2);

        let mut game3 = setup_round2_player1_scenario();
        complete_round_for_tests(&mut game3, player);
        let res_post = game3.submit_terminal_action(
            player,
            TerminalAction::PlayCard {
                card_id: "east_trading_post".to_string(),
                trades: vec![],
            },
        );
        assert!(matches!(res_post, ActionResult::Success { .. }), "{:?}", res_post);
        assert_eq!(game3.players[player].board.coins, 3);
    }

    #[test]
    fn coin_only_cards_fail_when_player_cannot_pay() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["clay_pit".to_string(), "timber_yard".to_string()];
        game.players[0].board.coins = 0;

        for card_id in ["clay_pit", "timber_yard"] {
            let res = game.submit_terminal_action(
                0,
                TerminalAction::PlayCard {
                    card_id: card_id.to_string(),
                    trades: vec![],
                },
            );
            expect_invalid(res, "not enough coins");
            assert_no_action_queued(&game, 0, card_id, 0);
        }
    }

    #[test]
    fn direct_play_card_bypasses_validation() {
        // Document that only submit_terminal_action enforces costs (game loop uses that path).
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["apothecary".to_string()];
        game.players[0].board.coins = 3;
        assert!(!game.is_valid_terminal_action(
            0,
            &TerminalAction::PlayCard {
                card_id: "apothecary".to_string(),
                trades: vec![],
            }
        ));
        assert!(game.play_card(0, "apothecary").is_ok());
        assert!(game.players[0].board.played_cards.contains(&"apothecary".to_string()));
    }

    #[test]
    fn play_fails_when_unaffordable_without_trades() {
        let mut game = new_resource_test_game(3);
        game.players[0].current_hand = vec!["baths".to_string()];
        game.players[0].board.coins = 10;
        // baths costs 1 stone; no producer on board and no trades specified
        let res = game.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "baths".to_string(),
                trades: vec![],
            },
        );
        expect_invalid(res, "insufficient resources");
        assert!(game.players[0].current_hand.contains(&"baths".to_string()));
    }

    #[test]
    fn archery_range_valid_with_trades_from_both_neighbors() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["archery_range".to_string()];
        game.players[0].board.coins = 12;
        // right (P1): glass, wood, ore/brick
        game.players[1].board.played_cards =
            vec!["glassworks".to_string(), "lumber_yard".to_string(), "clay_pit".to_string()];
        // left (P2): stone, cloth, ore, wood/stone
        game.players[2].board.played_cards = vec![
            "stone_pit".to_string(),
            "loom".to_string(),
            "ore_vein".to_string(),
            "timber_yard".to_string(),
        ];
        let res = game.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "archery_range".to_string(),
                trades: vec![
                    Trade {
                        from: Neighbor::Right,
                        resource: Resource::Wood,
                    },
                    Trade {
                        from: Neighbor::Left,
                        resource: Resource::Wood,
                    },
                    Trade {
                        from: Neighbor::Left,
                        resource: Resource::Ore,
                    },
                ],
            },
        );
        assert!(
            matches!(res, ActionResult::Success { .. }),
            "archery_range with neighbor trades: {:?}",
            res
        );
    }

    #[test]
    fn play_fails_when_trading_from_wrong_neighbor() {
        let mut game = new_resource_test_game(3);
        // p0 right neighbor is p1; stone only on p1
        game.players[0].current_hand = vec!["baths".to_string()];
        game.players[0].board.coins = 10;
        game.players[1].board.played_cards = vec!["stone_pit".to_string()];
        let res = game.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "baths".to_string(),
                trades: vec![Trade {
                    from: Neighbor::Left,
                    resource: Resource::Stone,
                }],
            },
        );
        expect_invalid(res, "invalid trades");
    }

    #[test]
    fn wonder_build_fails_when_unaffordable() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["lumber_yard".to_string()];
        game.players[0].board.coins = 10;
        // Gizah A stage 1 costs 2 wood; no wood available
        let res = game.submit_terminal_action(
            0,
            TerminalAction::BuildWonder {
                card_id: "lumber_yard".to_string(),
                stage: 1,
                trades: vec![],
                discard_play: None,
            },
        );
        expect_invalid(res, "insufficient resources");
    }

    #[test]
    fn halikarnassos_wonder_stage_plays_from_discard() {
        let mut game =
            GameState::new_with_assignment(3, vec!["halikarnassos_day".into(); 3]);
        game.discard_pile = vec!["well".to_string()];
        game.players[0].board.wonder_stages_built = 1;
        game.players[0].board.played_cards =
            vec!["glassworks".to_string(), "loom".to_string()];
        game.players[0].current_hand = vec!["stone_pit".to_string()];
        let stage2 = TerminalAction::BuildWonder {
            card_id: "stone_pit".to_string(),
            stage: 2,
            trades: vec![],
            discard_play: Some("well".to_string()),
        };
        assert!(matches!(
            game.submit_terminal_action(0, stage2),
            ActionResult::Success { .. }
        ));
        complete_round_for_tests(&mut game, 0);
        assert!(!game.discard_pile.contains(&"well".to_string()));
        assert!(game.players[0].board.played_cards.contains(&"well".to_string()));
    }

    #[test]
    fn babylon_night_allows_two_plays_on_round_six() {
        let mut game = GameState::new_with_assignment(3, vec!["babylon_night".into(); 3]);
        game.round_in_age = 6;
        game.players[0].current_hand = vec!["lumber_yard".to_string(), "ore_vein".to_string()];
        game.players[0].board.wonder_stages_built = 1; // sixth-round effect unlocked
        assert_eq!(game.actions_required_for_player(0), 2);

        let play1 = TerminalAction::PlayCard {
            card_id: "lumber_yard".to_string(),
            trades: vec![],
        };
        assert!(matches!(
            game.submit_terminal_action(0, play1),
            ActionResult::Success { .. }
        ));
        assert!(!game.player_round_complete(0));

        let play2 = TerminalAction::BurnCard {
            card_id: "ore_vein".to_string(),
        };
        assert!(matches!(
            game.submit_terminal_action(0, play2),
            ActionResult::Success { .. }
        ));
    }

    #[test]
    fn ephesos_starting_paper_does_not_block_press() {
        let mut game = GameState::new_with_assignment(3, vec!["ephesos_day".into(); 3]);
        game.players[0].current_hand = vec!["press".to_string()];
        let action = TerminalAction::PlayCard {
            card_id: "press".to_string(),
            trades: vec![],
        };
        assert!(
            game.is_valid_terminal_action(0, &action),
            "ephesos paper token is production, not a played press card"
        );
    }

    #[test]
    fn civilization_token_appears_in_production() {
        let game = GameState::new_with_assignment(3, vec!["ephesos_day".into(); 3]);
        let prod = game.compute_fixed_production(0);
        assert_eq!(prod.counts.get(&Resource::Papyrus), Some(&1));
    }

    #[test]
    fn olympia_night_first_card_of_each_age_is_free_on_round_one() {
        let mut game = GameState::new_with_assignment(3, vec!["olympia_night".into(); 3]);
        game.current_age = 2;
        game.round_in_age = 1;
        game.players[0].board.wonder_stages_built = 1;
        game.players[0].current_hand = vec!["baths".to_string()];
        game.players[0].board.coins = 0;
        let action = TerminalAction::PlayCard {
            card_id: "baths".to_string(),
            trades: vec![],
        };
        assert!(game.is_valid_terminal_action(0, &action));
        assert!(matches!(
            game.submit_terminal_action(0, action),
            ActionResult::Success { .. }
        ));
        complete_round_for_tests(&mut game, 0);
        assert_eq!(game.players[0].board.coins, 0);
    }

    #[test]
    fn olympia_night_last_card_of_age_is_free() {
        let mut game = GameState::new_with_assignment(3, vec!["olympia_night".into(); 3]);
        game.round_in_age = 6;
        game.players[0].board.wonder_stages_built = 2; // last-per-age-free unlocked
        game.players[0].current_hand = vec!["baths".to_string()];
        game.players[0].board.coins = 0;
        let action = TerminalAction::PlayCard {
            card_id: "baths".to_string(),
            trades: vec![],
        };
        assert!(game.is_valid_terminal_action(0, &action));
        assert!(matches!(
            game.submit_terminal_action(0, action.clone()),
            ActionResult::Success { .. }
        ));
        complete_round_for_tests(&mut game, 0);
        assert_eq!(game.players[0].board.coins, 0);
        assert!(game.players[0].board.played_cards.contains(&"baths".to_string()));
    }

    #[test]
    fn burn_adds_card_to_discard_pile() {
        let mut game = GameState::new(3);
        for p in 0..3 {
            game.players[p].current_hand = vec![format!("lumber_yard")];
        }
        for p in 0..3 {
            let _ = game.submit_terminal_action(
                p,
                TerminalAction::BurnCard {
                    card_id: "lumber_yard".to_string(),
                },
            );
        }
        assert!(game.discard_pile.contains(&"lumber_yard".to_string()));
    }

    #[test]
    fn end_of_age_discard_goes_to_pile() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["lumber_yard".to_string()];
        game.discard_end_of_age_cards();
        assert_eq!(game.discard_pile, vec!["lumber_yard".to_string()]);
        assert!(game.players[0].current_hand.is_empty());
    }

    #[test]
    fn trade_fails_when_insufficient_coins() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["baths".to_string()];
        game.players[0].board.coins = 0;
        game.players[1].board.played_cards = vec!["stone_pit".to_string()];
        let res = game.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "baths".to_string(),
                trades: vec![Trade {
                    from: Neighbor::Right,
                    resource: Resource::Stone,
                }],
            },
        );
        expect_invalid(res, "not enough coins");
    }

    #[test]
    fn marketplace_discounts_manufactured_goods_from_either_neighbor() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["workshop".to_string()];
        game.players[0].board.coins = 1;
        game.players[0].board.played_cards = vec!["marketplace".to_string()];
        game.players[1].board.played_cards = vec!["glassworks".to_string()];
        complete_round_for_tests(&mut game, 0);
        let res = game.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "workshop".to_string(),
                trades: vec![Trade {
                    from: Neighbor::Right,
                    resource: Resource::Glass,
                }],
            },
        );
        assert!(matches!(res, ActionResult::Success { .. }), "marketplace right: {:?}", res);
        assert_eq!(game.players[0].board.coins, 0);

        let mut game2 = GameState::new(3);
        game2.players[0].current_hand = vec!["scriptorium".to_string()];
        game2.players[0].board.coins = 1;
        game2.players[0].board.played_cards = vec!["marketplace".to_string()];
        game2.players[2].board.played_cards = vec!["press".to_string()];
        complete_round_for_tests(&mut game2, 0);
        let res2 = game2.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "scriptorium".to_string(),
                trades: vec![Trade {
                    from: Neighbor::Left,
                    resource: Resource::Papyrus,
                }],
            },
        );
        assert!(matches!(res2, ActionResult::Success { .. }), "marketplace left papyrus: {:?}", res2);
        assert_eq!(game2.players[0].board.coins, 0);
    }

    #[test]
    fn west_trading_post_discounts_raw_from_left_only() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["baths".to_string()];
        game.players[0].board.coins = 1;
        game.players[0].board.played_cards = vec!["west_trading_post".to_string()];
        game.players[2].board.played_cards = vec!["stone_pit".to_string()];
        complete_round_for_tests(&mut game, 0);
        let res = game.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "baths".to_string(),
                trades: vec![Trade {
                    from: Neighbor::Left,
                    resource: Resource::Stone,
                }],
            },
        );
        assert!(matches!(res, ActionResult::Success { .. }), "west post left discount: {:?}", res);
        assert_eq!(game.players[0].board.coins, 0);

        // Same card should still cost 2 coins from the right neighbor.
        let mut game2 = GameState::new(3);
        game2.players[0].current_hand = vec!["baths".to_string()];
        game2.players[0].board.coins = 1;
        game2.players[0].board.played_cards = vec!["west_trading_post".to_string()];
        game2.players[1].board.played_cards = vec!["stone_pit".to_string()];
        let res2 = game2.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "baths".to_string(),
                trades: vec![Trade {
                    from: Neighbor::Right,
                    resource: Resource::Stone,
                }],
            },
        );
        expect_invalid(res2, "not enough coins");
    }

    #[test]
    fn east_trading_post_discounts_raw_from_right_only() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["baths".to_string()];
        game.players[0].board.coins = 1;
        game.players[0].board.played_cards = vec!["east_trading_post".to_string()];
        game.players[1].board.played_cards = vec!["stone_pit".to_string()];
        complete_round_for_tests(&mut game, 0);
        let res = game.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "baths".to_string(),
                trades: vec![Trade {
                    from: Neighbor::Right,
                    resource: Resource::Stone,
                }],
            },
        );
        assert!(matches!(res, ActionResult::Success { .. }), "east post right discount: {:?}", res);
        assert_eq!(game.players[0].board.coins, 0);

        let mut game2 = GameState::new(3);
        game2.players[0].current_hand = vec!["baths".to_string()];
        game2.players[0].board.coins = 1;
        game2.players[0].board.played_cards = vec!["east_trading_post".to_string()];
        game2.players[2].board.played_cards = vec!["stone_pit".to_string()];
        let res2 = game2.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "baths".to_string(),
                trades: vec![Trade {
                    from: Neighbor::Left,
                    resource: Resource::Stone,
                }],
            },
        );
        expect_invalid(res2, "not enough coins");
    }

    #[test]
    fn play_fails_when_duplicate_card_already_built() {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec!["lumber_yard".to_string()];
        game.players[0].board.played_cards = vec!["lumber_yard".to_string()];
        game.players[0].board.coins = 10;
        let res = game.submit_terminal_action(
            0,
            TerminalAction::PlayCard {
                card_id: "lumber_yard".to_string(),
                trades: vec![],
            },
        );
        expect_invalid(res, "already");
        assert!(game.players[0].current_hand.contains(&"lumber_yard".to_string()));
    }

    /// Every documented chain link: parent built => child plays free (no coins, no trades).
    /// Every documented chain link from cards.txt.
    const CHAIN_LINKS: &[(&str, &str)] = &[
        ("altar", "pantheon"),
        ("baths", "aqueduct"),
        ("well", "statue"),
        ("theater", "gardens"),
        ("marketplace", "caravansery"),
        ("caravansery", "lighthouse"),
        ("east_trading_post", "forum"),
        ("west_trading_post", "forum"),
        ("forum", "haven"),
        ("apothecary", "stables"),
        ("apothecary", "dispensary"),
        ("dispensary", "arena"),
        ("dispensary", "lodge"),
        ("workshop", "archery_range"),
        ("workshop", "laboratory"),
        ("laboratory", "siege_workshop"),
        ("laboratory", "observatory"),
        ("scriptorium", "courthouse"),
        ("scriptorium", "library"),
        ("library", "university"),
        ("library", "senate"),
        ("school", "academy"),
        ("school", "study"),
        ("walls", "fortifications"),
        ("training_ground", "circus"),
        ("laboratory", "lodge"),
    ];

    fn assert_card_chains_from(
        db: &crate::games::seven_wonders::cards::CardDatabase,
        child: &str,
        parent: &str,
    ) {
        use serde_json::Value;
        let card = db
            .get(child)
            .unwrap_or_else(|| panic!("unknown card id: {child}"));
        let ok = match &card.chain_from {
            Some(Value::String(id)) => id == parent,
            Some(Value::Array(ids)) => ids
                .iter()
                .any(|v| v.as_str() == Some(parent)),
            _ => false,
        };
        assert!(ok, "{child} should list chain_from {parent}, got {:?}", card.chain_from);
    }

    fn setup_chain_play(child: &str, parents: &[&str]) -> GameState {
        let mut game = GameState::new(3);
        game.players[0].current_hand = vec![child.to_string()];
        game.players[0].board.played_cards = parents.iter().map(|p| (*p).to_string()).collect();
        game.players[0].board.coins = 0;
        game
    }

    #[test]
    fn card_database_chain_from_matches_documented_links() {
        let db = crate::games::seven_wonders::cards::CardDatabase::load();
        for &(parent, child) in CHAIN_LINKS {
            assert_card_chains_from(&db, child, parent);
        }
    }

    #[test]
    fn every_chain_link_builds_free_when_parent_built() {
        for &(parent, child) in CHAIN_LINKS {
            let game = setup_chain_play(child, &[parent]);
            let action = TerminalAction::PlayCard {
                card_id: child.to_string(),
                trades: vec![],
            };
            assert!(
                game.is_valid_terminal_action(0, &action),
                "chain {parent} -> {child} should be free with zero coins and no trades"
            );
        }
    }

    #[test]
    fn every_chain_link_requires_resources_without_parent() {
        let db = crate::games::seven_wonders::cards::CardDatabase::load();
        for &(_parent, child) in CHAIN_LINKS {
            let card = db.get(child).expect("child card");
            if card.cost.coins == 0 && card.cost.resources.counts.is_empty() {
                continue;
            }
            let mut game = GameState::new(3);
            game.players[0].current_hand = vec![child.to_string()];
            game.players[0].board.coins = 0;
            let action = TerminalAction::PlayCard {
                card_id: child.to_string(),
                trades: vec![],
            };
            assert!(
                !game.is_valid_terminal_action(0, &action),
                "{child} should require resources when chain parent is not built"
            );
        }
    }

    #[test]
    fn senate_chains_from_library() {
        let game = setup_chain_play("senate", &["library"]);
        let action = TerminalAction::PlayCard {
            card_id: "senate".to_string(),
            trades: vec![],
        };
        assert!(
            game.is_valid_terminal_action(0, &action),
            "senate should chain free from library per cards.txt"
        );
    }
}
