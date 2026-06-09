//! Action and Tool definitions for the Seven Wonders engine (base game).
//!
//! See the documentation in `games/seven_wonders/` for the high-level interaction model.

use serde::{Deserialize, Serialize};

pub use super::types::{Neighbor, Trade};

/// Terminal actions. These consume one card from the player's hand and end their turn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerminalAction {
    /// Play the card normally (pay its cost or chain for free). `trades` lists explicit purchases
    /// from neighbors to cover resource deficits (engine validates availability, prices, and rules).
    PlayCard { card_id: String, trades: Vec<Trade> },

    /// Use the card to build the specified stage of the player's wonder.
    /// The card is tucked and removed from the player's available cards.
    BuildWonder { card_id: String, stage: u8, trades: Vec<Trade> },

    /// Discard the card for coins.
    BurnCard { card_id: String },
}

/// Observation / Tool actions. These provide information and do **not** end the turn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObservationAction {
    CheckMyCards,
    CheckAllCards,
    CheckMyResources,
    CheckAllResources,
    CheckAllMilitary,
    CheckCivilizations,
    CheckMyWonder,
    CheckWonders,

    // More will be added (e.g. trading-related queries, specific neighbor views, etc.)
}

/// The complete set of actions an agent can take during its decision loop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SevenWondersAction {
    Terminal(TerminalAction),
    Observe(ObservationAction),
}

/// Result returned to the agent after attempting an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionResult {
    Success { message: Option<String> },
    Invalid {
        reason: String,
        suggested_actions: Vec<String>,
    },
    TurnLimitExceeded { reason: String },
}