//! Card definitions and loading for Seven Wonders.
//!
//! This module is responsible for loading the JSON data from
//! `games/seven_wonders/data/cards/` and turning it into usable Rust types.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct CardData {
    pub id: String,
    pub name: String,
    pub age: u8,
    pub color: String, // brown, grey, yellow, blue, green, red, purple
    #[serde(default)]
    pub player_count: Vec<u8>,
    #[serde(default)]
    pub cost: serde_json::Value,   // TODO: model properly later
    #[serde(default)]
    pub effect: serde_json::Value, // TODO: model properly later
    #[serde(default)]
    pub chain_from: Option<serde_json::Value>,
    #[serde(default)]
    pub chain_to: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CardDatabase {
    pub cards: HashMap<String, CardData>,
    pub by_age: HashMap<u8, Vec<String>>,
}

impl CardDatabase {
    pub fn load() -> Self {
        let mut cards = HashMap::new();
        let mut by_age: HashMap<u8, Vec<String>> = HashMap::new();

        // Load all three ages for now
        for age in [1, 2, 3] {
            let path = format!("games/seven_wonders/data/cards/age{}.json", age);
            let content = fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("Failed to read {}", path));

            let age_cards: Vec<CardData> = serde_json::from_str(&content)
                .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e));

            for card in age_cards {
                by_age.entry(age).or_default().push(card.id.clone());
                cards.insert(card.id.clone(), card);
            }
        }

        Self { cards, by_age }
    }

    pub fn get(&self, id: &str) -> Option<&CardData> {
        self.cards.get(id)
    }
}
