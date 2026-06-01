//! Seven Wonders engine module.
//!
//! This lives under src/games/ so it can be cleanly imported by the benchmark runner.
//! Data files (JSON card definitions, etc.) live in the root-level `games/seven_wonders/data/` folder.

pub mod actions;

pub use actions::{
    ActionResult, ObservationAction, SevenWondersAction, TerminalAction,
};

/// Top-level game state for Seven Wonders.
/// This will grow significantly as we implement the full rules.
#[derive(Debug, Clone)]
pub struct GameState {
    pub player_count: u8,
}

impl GameState {
    pub fn new(player_count: u8) -> Self {
        assert!((2..=7).contains(&player_count), "Seven Wonders supports 2-7 players");
        Self { player_count }
    }
}

pub use self::actions::SevenWondersAction as Action;