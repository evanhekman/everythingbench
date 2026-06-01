//! Action and Tool definitions for the Seven Wonders engine.
//!
//! This module defines the full set of things an agent (LLM) can do on its turn.
//!
//! Design principles:
//! - There are two kinds of actions:
//!   1. Terminal actions: These end the player's turn by consuming one card from hand.
//!   2. Observation / Tool actions: These provide information and do not end the turn.
//!
//! The agent is allowed to issue multiple observation actions before finally
//! committing to one terminal action.

use serde::{Deserialize, Serialize};

/// Terminal actions that consume a card from the player's hand and end their turn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerminalAction {
    /// Play the card for its normal effect (pay cost or chain).
    PlayCard { card_id: String },

    /// Use the chosen card to build the next stage of the player's wonder.
    /// The card is tucked under the wonder and is no longer available.
    BuildWonder {
        card_id: String,
        /// Which stage of the wonder they are building (1-indexed).
        stage: u8,
    },

    /// Discard the card for coins (usually 2, sometimes modified by wonders).
    BurnCard { card_id: String },
}

/// Observation / Tool calls that the agent can make to gather information.
/// These do not consume the turn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObservationAction {
    /// See only the cards the current player has already played.
    CheckMyCards,

    /// See all played cards on the table (self + left + right neighbors).
    CheckAllCards,

    /// See the resources the current player currently produces + coins.
    CheckMyResources,

    /// See resources + coins for self and both neighbors.
    CheckAllResources,

    /// See military strength of self and both neighbors.
    CheckAllMilitary,

    /// See science/civilization symbols the player has (green cards).
    CheckCivilizations,

    /// See the current state of the player's own wonder (built stages + effects).
    CheckMyWonder,

    /// See the wonder boards of self and neighbors (what stages are built).
    CheckWonders,

    // Future candidates (examples):
    // CheckLeftNeighbor,
    // CheckRightNeighbor,
    // CheckAvailableTrades,   // when trying to play a card they can't currently afford
    // CheckVictoryPoints,     // current VP estimate (if we want to expose it)
}

/// The full set of things an agent can request on its turn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SevenWondersAction {
    /// A terminal action that ends the current turn.
    Terminal(TerminalAction),

    /// An information-gathering action that does not end the turn.
    Observe(ObservationAction),
}

impl SevenWondersAction {
    pub fn is_terminal(&self) -> bool {
        matches!(self, SevenWondersAction::Terminal(_))
    }

    pub fn is_observation(&self) -> bool {
        matches!(self, SevenWondersAction::Observe(_))
    }
}

/// Result of attempting to apply an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionResult {
    /// The action was accepted and applied.
    Success { message: Option<String> },

    /// The action was invalid. The agent should try something else.
    Invalid {
        reason: String,
        suggested_actions: Vec<String>, // Human-readable hints
    },

    /// The agent has exceeded allowed tool calls or invalid attempts for this turn.
    TurnLimitExceeded {
        reason: String,
    },
}