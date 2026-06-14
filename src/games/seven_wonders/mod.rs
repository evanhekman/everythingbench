//! Seven Wonders engine module.
//!
//! This lives under src/games/ so it can be cleanly imported by the benchmark runner.
//! Data files (JSON card definitions, etc.) live in the root-level `games/seven_wonders/data/` folder.

pub mod actions;
pub mod cards;
pub mod controller;
pub mod log;
pub mod scoring;
pub mod state;
pub mod term;
pub mod types;
pub mod wonders;

pub use self::controller::{FirstPurchaseableController, HumanLogController, LLMController};
pub use self::state::{
    GameState, SevenWondersGameOutcome, run_game, run_limited_rounds_game,
    run_limited_rounds_game_with_boards,
};
pub use self::wonders::WonderDatabase;