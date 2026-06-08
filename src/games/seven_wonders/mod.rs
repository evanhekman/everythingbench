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
pub mod types;

pub use self::actions::{
    ActionResult, ObservationAction, SevenWondersAction, TerminalAction,
};
pub use self::cards::{CardDatabase, Card};
pub use self::controller::{
    FirstPurchaseableController, HumanController, HumanLogController, LLMController, PlayerController,
};
pub use self::log::GameLog;
pub use self::state::{GameState, PlayerBoard, PlayerState, PlayerView, run_game, run_limited_rounds_game, run_smoke_game};
pub use self::types::{Cost, DiscountType, Effect, Neighbor, Resource, Resources, ScienceSymbol, Trade};