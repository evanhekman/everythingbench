//! Core game state for Seven Wonders (base game).
//!
//! Current focus: supporting the incremental validation plan.

use super::cards::{CardDatabase, CardData};
use std::collections::HashMap;

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
    pub turn_in_age: u8,             // 1..=7 for most of the age
    pub players: Vec<PlayerState>,
    pub card_db: CardDatabase,
    // TODO: discard pile, battle resolution state, etc.
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
            turn_in_age: 1,
            players,
            card_db,
        };

        state.deal_starting_hands();
        state
    }

    /// Deals 7 cards to each player for the start of Age 1.
    /// For now we just take the first N*7 cards from the Age 1 pool for determinism in tests.
    fn deal_starting_hands(&mut self) {
        let age1_ids: Vec<String> = self
            .card_db
            .by_age
            .get(&1)
            .cloned()
            .unwrap_or_default();

        // Simple deterministic deal for now (proper dealing + guild selection comes later)
        let cards_per_player = 7;
        let total_cards_needed = self.player_count as usize * cards_per_player;

        let mut available = age1_ids.into_iter().take(total_cards_needed).collect::<Vec<_>>();

        for player in &mut self.players {
            player.current_hand = available.drain(..cards_per_player).collect();
        }
    }

    /// Returns the current hand of the given player (card ids).
    pub fn get_hand(&self, player: usize) -> &[String] {
        &self.players[player].current_hand
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

    /// Discards a card from hand for coins.
    pub fn burn_card(&mut self, player: usize, card_id: &str) -> Result<(), String> {
        let hand = &mut self.players[player].current_hand;
        if let Some(pos) = hand.iter().position(|c| c == card_id) {
            hand.remove(pos);
            self.players[player].board.coins = self.players[player].board.coins.saturating_add(2);
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
}

#[cfg(test)]
mod tests {
    use super::GameState;

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
}
