//! Card definitions and loading for Seven Wonders.
//!
//! This module is responsible for loading the JSON data from
//! `games/seven_wonders/data/cards/` and turning it into usable Rust types.

pub use super::types::{Card, Cost, Effect};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

/// Legacy/raw card data as it appears in the JSON files.
/// We keep this for loading, then convert to the richer `Card` type.
#[derive(Debug, Clone, Deserialize)]
struct RawCardData {
    pub id: String,
    pub name: String,
    pub age: u8,
    pub color: String,
    #[serde(default)]
    pub player_count: Vec<u8>,
    #[serde(default)]
    pub cost: serde_json::Value,
    #[serde(default)]
    pub effect: serde_json::Value,
    #[serde(default)]
    pub chain_from: Option<serde_json::Value>,
    #[serde(default)]
    pub chain_to: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CardDatabase {
    cards: HashMap<String, Card>,
    by_age: HashMap<u8, Vec<String>>,
}

impl CardDatabase {
    pub fn load() -> Self {
        let mut cards = HashMap::new();
        let mut by_age: HashMap<u8, Vec<String>> = HashMap::new();

        for age in [1, 2, 3] {
            let path = format!("games/seven_wonders/data/cards/age{}.json", age);
            let content = fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("Failed to read {}", path));

            let raw_cards: Vec<RawCardData> = serde_json::from_str(&content)
                .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e));

            for raw in raw_cards {
                let card = Self::from_raw(raw);
                by_age.entry(age).or_default().push(card.id.clone());
                cards.insert(card.id.clone(), card);
            }
        }

        Self { cards, by_age }
    }

    fn from_raw(raw: RawCardData) -> Card {
        // For now we do a very light conversion.
        // Cost and Effect will be improved as we populate the JSON data.
        let cost = if raw.cost.is_object() && raw.cost.as_object().map_or(false, |o| !o.is_empty()) {
            // Very rough for now; we'll refine when we model costs properly.
            Cost { coins: 0, resources: Default::default() }
        } else {
            Cost::default()
        };

        let effect = if raw.effect.is_object() && raw.effect.as_object().map_or(false, |o| !o.is_empty()) {
            Effect::Other(raw.effect)
        } else {
            Effect::default()
        };

        Card {
            id: raw.id,
            name: raw.name,
            age: raw.age,
            color: raw.color,
            player_count: raw.player_count,
            cost,
            effect,
            chain_from: raw.chain_from,
            chain_to: raw.chain_to,
        }
    }

    pub fn get(&self, id: &str) -> Option<&Card> {
        self.cards.get(id)
    }

    /// Returns all card IDs for a given age (1, 2, or 3).
    pub fn cards_for_age(&self, age: u8) -> &[String] {
        self.by_age.get(&age).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Internal for now: access to by_age map (used by GameState setup).
    pub(crate) fn by_age(&self) -> &HashMap<u8, Vec<String>> {
        &self.by_age
    }
}
