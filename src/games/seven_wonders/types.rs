//! Domain types for Seven Wonders.
//!
//! These provide clean, typed representations for resources, costs, effects, etc.
//! instead of raw JSON or strings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The six raw and manufactured resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Resource {
    Wood,
    Stone,
    Ore,
    Clay,
    Glass,
    Loom,
    Papyrus,
}

impl Resource {
    pub fn all() -> &'static [Resource] {
        &[
            Resource::Wood,
            Resource::Stone,
            Resource::Ore,
            Resource::Clay,
            Resource::Glass,
            Resource::Loom,
            Resource::Papyrus,
        ]
    }
}

/// A collection of resources (with counts).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resources {
    pub counts: HashMap<Resource, u8>,
}

impl Resources {
    pub fn add(&mut self, resource: Resource, amount: u8) {
        *self.counts.entry(resource).or_insert(0) += amount;
    }

    pub fn get(&self, resource: Resource) -> u8 {
        *self.counts.get(&resource).unwrap_or(&0)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.counts.values().all(|&c| c == 0)
    }
}

/// Science symbols (green cards).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScienceSymbol {
    Tablet,
    Compass,
    Gear, // cog
}

/// Left or right neighbor for trading / direction specific effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Neighbor {
    Left,
    Right,
}

/// Represents a single resource purchase from a neighbor as part of playing a card or wonder stage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Trade {
    pub from: Neighbor,
    pub resource: Resource,
}

/// Type of goods affected by trade discounts (yellow cards like trading posts, marketplace, forum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiscountType {
    RawMaterials,
    ManufacturedGoods,
}

/// The effect of playing a card or building a wonder stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Effect {
    /// Immediate victory points.
    VictoryPoints(i32),
    /// Coins gained.
    Coins(i32),
    /// Resources this card produces (fixed or choice).
    Production {
        fixed: Resources,
        choice: Option<Vec<Resource>>, // for "or" productions like Timber Yard
    },
    /// Military strength (red cards).
    Military(i32),
    /// Science symbol.
    Science(ScienceSymbol),
    /// Coins per certain condition (e.g. per neighbor brown card).
    CoinsPerNeighbor {
        color: String, // "brown", "grey", etc. or "wonder_stage"
        amount: i32,
    },
    /// VP per certain condition (used by many guilds and some cards).
    PointsPerNeighbor {
        color: String,
        amount: i32,
    },
    /// Trade cost reduction (e.g. from East/West Trading Post, Marketplace, Forum).
    /// direction=None means both neighbors.
    TradeDiscount {
        direction: Option<Neighbor>,
        kind: DiscountType,
        cost: u8,
    },
    /// Other / future effects (e.g. free build, etc.)
    Other(serde_json::Value),
}

impl Default for Effect {
    fn default() -> Self {
        Effect::Other(serde_json::Value::Null)
    }
}

/// Cost to play a card or build a stage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cost {
    pub coins: u8,
    pub resources: Resources,
    // TODO: later support "or" costs if needed
}

/// Enhanced card data with typed cost and effect.
#[derive(Debug, Clone, Deserialize)]
pub struct Card {
    pub id: String,
    pub age: u8,
    pub color: String,
    #[serde(default)]
    pub player_count: Vec<u8>,
    #[serde(default)]
    pub cost: Cost,
    #[serde(default)]
    pub effect: Effect,
    #[serde(default)]
    pub chain_from: Option<serde_json::Value>, // can be string or array for "or"
}
