//! Seven Wonders engine module.
//!
//! This lives under src/games/ so it can be cleanly imported by the benchmark runner.
//! Data files (JSON card definitions, etc.) live in the root-level `games/seven_wonders/data/` folder.

pub mod actions;
pub mod cards;
pub mod state;
pub mod types;

pub use self::actions::{
    ActionResult, ObservationAction, SevenWondersAction, TerminalAction,
};
pub use self::cards::{CardDatabase, Card};
pub use self::state::{GameState, PlayerBoard, PlayerState, PlayerView};
pub use self::types::{Cost, Effect, Resource, Resources, ScienceSymbol};