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
            age: raw.age,
            color: raw.color,
            player_count: raw.player_count,
            cost,
            effect,
            chain_from: raw.chain_from,
        }
    }

    pub fn get(&self, id: &str) -> Option<&Card> {
        self.cards.get(id)
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

    /// Build tabular hand rows (id, color, cost, benefit) for the given card ids.
    pub fn hand_display_rows(&self, hand: &[String]) -> Vec<HandDisplayRow> {
        hand.iter()
            .map(|id| {
                if let Some(card) = self.get(id) {
                    HandDisplayRow {
                        id: id.clone(),
                        color: normalize_card_color(&card.color).to_string(),
                        cost: format_cost_brackets(&card.cost),
                        benefit: format_effect_benefit(&card.effect),
                    }
                } else {
                    HandDisplayRow {
                        id: id.clone(),
                        color: "?".to_string(),
                        cost: "[]".to_string(),
                        benefit: "?".to_string(),
                    }
                }
            })
            .collect()
    }

    /// Multiline hand block for logs and prompts.
    pub fn format_hand_block(&self, hand: &[String]) -> String {
        let rows = self.hand_display_rows(hand);
        self.format_hand_rows(&rows)
    }

    /// Format rows with tab-separated, width-padded columns.
    pub fn format_hand_rows(&self, rows: &[HandDisplayRow]) -> String {
        if rows.is_empty() {
            return "[]".to_string();
        }
        let w_id = rows.iter().map(|r| r.id.len()).max().unwrap_or(0);
        let w_color = rows.iter().map(|r| r.color.len()).max().unwrap_or(0);
        let w_cost = rows.iter().map(|r| r.cost.len()).max().unwrap_or(0);
        let lines: Vec<String> = rows
            .iter()
            .map(|r| format_hand_row_line(r, w_id, w_color, w_cost))
            .collect();
        format!("[\n{}\n]", lines.join("\n"))
    }
}

/// One row of the tabular hand display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandDisplayRow {
    pub id: String,
    pub color: String,
    pub cost: String,
    pub benefit: String,
}

pub(crate) fn pad_field(value: &str, width: usize) -> String {
    if value.len() >= width {
        value.to_string()
    } else {
        format!("{value}{}", " ".repeat(width - value.len()))
    }
}

pub(crate) const TAB_PAIR: &str = "\t\t";

pub(crate) fn format_hand_row_line(
    row: &HandDisplayRow,
    w_id: usize,
    w_color: usize,
    w_cost: usize,
) -> String {
    format!(
        "{}\t{}{}{}{}{}",
        pad_field(&row.id, w_id),
        pad_field(&row.color, w_color),
        TAB_PAIR,
        pad_field(&row.cost, w_cost),
        TAB_PAIR,
        row.benefit
    )
}

fn friendly_resource(r: Resource) -> &'static str {
    match r {
        Resource::Clay => "brick",
        Resource::Papyrus => "paper",
        Resource::Loom => "cloth",
        Resource::Wood => "wood",
        Resource::Stone => "stone",
        Resource::Ore => "ore",
        Resource::Glass => "glass",
    }
}

fn normalize_card_color(color: &str) -> &str {
    match color {
        "grey" => "gray",
        other => other,
    }
}

fn format_cost_brackets(cost: &Cost) -> String {
    let mut parts = Vec::new();
    if cost.coins > 0 {
        if cost.coins == 1 {
            parts.push("1 coin".to_string());
        } else {
            parts.push(format!("{} coins", cost.coins));
        }
    }
    for res in Resource::all() {
        let amount = cost.resources.get(*res);
        for _ in 0..amount {
            parts.push(friendly_resource(*res).to_string());
        }
    }
    if parts.is_empty() {
        "[]".to_string()
    } else {
        format!("[{}]", parts.join(", "))
    }
}

fn format_production_benefit(fixed: &Resources, choice: &Option<Vec<Resource>>) -> String {
    if let Some(choices) = choice {
        if choices.is_empty() {
            return String::new();
        }
        return choices
            .iter()
            .map(|r| format!("+1 {}", friendly_resource(*r)))
            .collect::<Vec<_>>()
            .join(" or ");
    }
    let mut parts = Vec::new();
    for res in Resource::all() {
        let amount = fixed.get(*res);
        if amount > 0 {
            parts.push(format!("+{amount} {}", friendly_resource(*res)));
        }
    }
    parts.join(", ")
}

fn format_effect_benefit(effect: &Effect) -> String {
    match effect {
        Effect::VictoryPoints(n) => format!("+{n} points"),
        Effect::Coins(n) => format!("+{n} coins"),
        Effect::Military(n) => format!("+{n} military"),
        Effect::Science(sym) => match sym {
            ScienceSymbol::Tablet => "+1 tablet".to_string(),
            ScienceSymbol::Compass => "+1 compass".to_string(),
            ScienceSymbol::Gear => "+1 cog".to_string(),
        },
        Effect::Production { fixed, choice } => format_production_benefit(fixed, choice),
        Effect::TradeDiscount {
            direction,
            kind,
            cost,
        } => {
            let resources = match kind {
                DiscountType::RawMaterials => "[wood, stone, brick, ore]",
                DiscountType::ManufacturedGoods => "[paper, glass, cloth]",
            };
            let neighbor = match direction {
                Some(Neighbor::Left) => "left_neighbor",
                Some(Neighbor::Right) => "right_neighbor",
                None => "either neighbor",
            };
            format!("buy {resources} from {neighbor} for {cost} coin instead of 2")
        }
        Effect::CoinsPerNeighbor { color, amount } => {
            format!("+{amount} coins per neighbor {color}")
        }
        Effect::PointsPerNeighbor { color, amount } => {
            format!("+{amount} points per neighbor {color}")
        }
        Effect::Other(v) => {
            if v.is_null() {
                return "?".to_string();
            }
            if let Some(obj) = v.as_object() {
                if let Some(pts) = obj.get("points").and_then(|x| x.as_i64()) {
                    return format!("+{pts} points");
                }
                if let Some(coins) = obj.get("coins").and_then(|x| x.as_i64()) {
                    return format!("+{coins} coins");
                }
            }
            "?".to_string()
        }
    }
}

#[cfg(test)]
mod hand_format_tests {
    use super::*;

    fn hand_row_columns(line: &str) -> Vec<&str> {
        line.split('\t').filter(|s| !s.is_empty()).collect()
    }

    #[test]
    fn tab_rules_single_after_id_double_between_other_columns() {
        let db = CardDatabase::load();
        let rows = db.hand_display_rows(&["loom".to_string(), "glassworks".to_string()]);
        let w_id = rows.iter().map(|r| r.id.len()).max().unwrap();
        let w_color = rows.iter().map(|r| r.color.len()).max().unwrap();
        let w_cost = rows.iter().map(|r| r.cost.len()).max().unwrap();
        let loom_line = format_hand_row_line(&rows[0], w_id, w_color, w_cost);
        let glass_line = format_hand_row_line(&rows[1], w_id, w_color, w_cost);
        let cols_loom = hand_row_columns(&loom_line);
        let cols_glass = hand_row_columns(&glass_line);
        assert_eq!(cols_loom[0].trim_end(), "loom");
        assert_eq!(cols_glass[0].trim_end(), "glassworks");
        assert!(!loom_line.contains("loom\t\t"), "loom must not double-tab after id");
        assert!(loom_line.contains(&format!("gray{TAB_PAIR}")), "line: {loom_line}");
    }

    #[test]
    fn hand_rows_align_columns_with_tabs() {
        let db = CardDatabase::load();
        let hand = vec![
            "baths".to_string(),
            "loom".to_string(),
            "theater".to_string(),
        ];
        let rows = db.hand_display_rows(&hand);
        let block = db.format_hand_rows(&rows);
        let lines: Vec<&str> = block
            .lines()
            .filter(|l| !l.trim().is_empty() && *l != "[" && *l != "]")
            .collect();
        assert_eq!(lines.len(), 3, "block:\n{block}");
        for line in &lines {
            let cols = hand_row_columns(line);
            assert_eq!(cols.len(), 4, "line: {line}");
        }
        // id column width = 7 ("theater"); shorter ids are space-padded before the tab
        let cols0 = hand_row_columns(lines[0]);
        assert_eq!(cols0[0].trim_end(), "baths");
        assert_eq!(cols0[1].trim_end(), "blue");
        assert_eq!(cols0[2].trim_end(), "[stone]");
        assert_eq!(cols0[3], "+3 points");

        let cols1 = hand_row_columns(lines[1]);
        assert_eq!(cols1[0].trim_end(), "loom");
        assert_eq!(cols1[1].trim_end(), "gray");
        assert_eq!(cols1[2].trim_end(), "[]");
        assert_eq!(cols1[3], "+1 cloth");

        let cols2 = hand_row_columns(lines[2]);
        assert_eq!(cols2[0], "theater");
        assert_eq!(cols2[1].trim_end(), "blue");
        assert_eq!(cols2[2].trim_end(), "[]");
        assert_eq!(cols2[3], "+3 points");
    }

    #[test]
    fn hand_block_is_multiline() {
        let db = CardDatabase::load();
        let hand = vec!["baths".to_string(), "loom".to_string()];
        let block = db.format_hand_block(&hand);
        assert!(block.starts_with("[\n"));
        assert!(block.contains("baths"));
        assert!(block.contains("loom"));
        assert!(block.contains('\t'));
        assert!(block.ends_with("\n]"));
    }
}
