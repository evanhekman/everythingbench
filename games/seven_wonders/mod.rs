//! Seven Wonders Game Engine
//!
//! This is a strict, data-driven implementation of the base game of 7 Wonders
//! designed for LLM agent benchmarking.
//!
//! Core design goals:
//! - No external dependencies for the game logic
//! - Strong validation of all actions
//! - Clear separation between observation and decision actions
//! - Support for 2-7 players
//! - Full base game mechanics (all 3 ages, guilds, wonders, trading, etc.)
//!
//! Interaction model (agentic):
//! The controlling agent (LLM) can issue multiple `ObservationAction`s in a loop.
//! The turn only ends when the agent issues one of the three terminal actions:
//! - Play a card
//! - Build a wonder stage using a card
//! - Burn a card for coins
//!
//! Invalid actions are rejected with feedback. Excessive invalid actions or
//! tool call loops are tracked and reported in the benchmark results.

pub mod actions;

pub use actions::{
    ActionResult, ObservationAction, SevenWondersAction, TerminalAction,
};

// Placeholder for the core game state. We will expand this significantly
// once we have the data model and card definitions.

/// Represents the full internal state of a Seven Wonders game.
/// This is the "god view" — the benchmark runner and engine use this.
/// Individual players/agents only ever see filtered views.
#[derive(Debug, Clone)]
pub struct GameState {
    // TODO: players, current_age, hands, played cards, wonders, coins, etc.
    pub player_count: u8,
    // ... much more to come
}

impl GameState {
    /// Creates a new game. This will eventually handle dealing cards,
    /// assigning wonders, giving starting coins, etc.
    pub fn new(player_count: u8) -> Self {
        assert!((2..=7).contains(&player_count), "Seven Wonders supports 2-7 players");

        Self {
            player_count,
        }
    }
}

// Re-export the main public types for convenience
pub use self::actions::SevenWondersAction as Action;