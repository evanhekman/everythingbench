//! Card definitions and loading for Seven Wonders.
//!
//! This module is responsible for loading the JSON data from
//! `games/seven_wonders/data/cards/` and turning it into usable Rust types.

pub use super::types::{Card, Cost, DiscountType, Effect, Neighbor, Resource, Resources, ScienceSymbol};
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

/// Parse a resource name from JSON keys.
fn parse_resource(s: &str) -> Option<Resource> {
    match s {
        "wood" => Some(Resource::Wood),
        "stone" => Some(Resource::Stone),
        "ore" => Some(Resource::Ore),
        "clay" => Some(Resource::Clay),
        "glass" => Some(Resource::Glass),
        "loom" => Some(Resource::Loom),
        "papyrus" => Some(Resource::Papyrus),
        _ => None,
    }
}

/// Parse cost object from the raw JSON (supports flat "coins" + resource keys like "glass": 1).
fn parse_cost(v: &serde_json::Value) -> Cost {
    let mut cost = Cost::default();
    if let Some(obj) = v.as_object() {
        if let Some(c) = obj.get("coins").and_then(|x| x.as_u64()) {
            cost.coins = c as u8;
        }
        for (k, val) in obj {
            if k == "coins" {
                continue;
            }
            if let Some(amt) = val.as_u64() {
                if let Some(res) = parse_resource(k) {
                    cost.resources.add(res, amt as u8);
                }
            }
        }
    }
    cost
}

/// Parse effect from raw JSON. Supports "production" (fixed or {"or": [...] } for combos),
/// "science", "trade_discount", "points", "coins".
fn parse_effect(v: &serde_json::Value) -> Effect {
    if let Some(obj) = v.as_object() {
        if let Some(prod_v) = obj.get("production") {
            if let Some(pobj) = prod_v.as_object() {
                if let Some(or_v) = pobj.get("or").and_then(|x| x.as_array()) {
                    let choices: Vec<Resource> = or_v
                        .iter()
                        .filter_map(|x| x.as_str().and_then(parse_resource))
                        .collect();
                    return Effect::Production {
                        fixed: Resources::default(),
                        choice: Some(choices),
                    };
                } else {
                    let mut fixed = Resources::default();
                    for (k, val) in pobj {
                        if let Some(amt) = val.as_u64() {
                            if let Some(res) = parse_resource(k) {
                                fixed.add(res, amt as u8);
                            }
                        }
                    }
                    return Effect::Production {
                        fixed,
                        choice: None,
                    };
                }
            }
        }
        if let Some(sci) = obj.get("science").and_then(|x| x.as_str()) {
            let sym = match sci {
                "compass" => ScienceSymbol::Compass,
                "tablet" => ScienceSymbol::Tablet,
                "gear" | "cog" => ScienceSymbol::Gear,
                _ => return Effect::Other(v.clone()),
            };
            return Effect::Science(sym);
        }
        if let Some(dv) = obj.get("trade_discount") {
            if let Some(dobj) = dv.as_object() {
                let direction = match dobj.get("direction").and_then(|x| x.as_str()) {
                    Some("left") => Some(Neighbor::Left),
                    Some("right") => Some(Neighbor::Right),
                    _ => None,
                };
                let kind = match dobj.get("kind").and_then(|x| x.as_str()) {
                    Some("raw") => DiscountType::RawMaterials,
                    Some("manufactured") => DiscountType::ManufacturedGoods,
                    _ => return Effect::Other(v.clone()),
                };
                let cost = dobj.get("cost").and_then(|x| x.as_u64()).unwrap_or(1) as u8;
                return Effect::TradeDiscount {
                    direction,
                    kind,
                    cost,
                };
            }
        }
        if let Some(pts) = obj.get("points").and_then(|x| x.as_i64()) {
            return Effect::VictoryPoints(pts as i32);
        }
        if let Some(coins) = obj.get("coins").and_then(|x| x.as_i64()) {
            return Effect::Coins(coins as i32);
        }
        if let Some(mil) = obj.get("military").and_then(|x| x.as_i64()) {
            return Effect::Military(mil as i32);
        }
        if !obj.is_empty() {
            return Effect::Other(v.clone());
        }
    }
    Effect::default()
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
        let cost = parse_cost(&raw.cost);
        let effect = parse_effect(&raw.effect);

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

    /// Returns the list of card ids for the age, with multiplicity based on player_count for the given N players.
    /// For guilds (color purple), this returns all qualifying, the selection of N+2 is done at deal time.
    pub fn build_age_pool(&self, age: u8, players: u8) -> Vec<String> {
        let mut pool = Vec::new();
        if let Some(ids) = self.by_age.get(&age) {
            for id in ids {
                if let Some(card) = self.cards.get(id) {
                    let copies = card.player_count.iter().filter(|&&p| p <= players).count();
                    for _ in 0..copies {
                        pool.push(id.clone());
                    }
                }
            }
        }
        pool
    }
}
