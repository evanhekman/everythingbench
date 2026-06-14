//! Wonder board definitions loaded from games/seven_wonders/data/wonders.json.

use super::cards::{pad_field, TAB_PAIR};
use super::types::{Cost, Resource, Resources};

/// Starting resource from a civilization's wonder board token (not a played card).
pub fn token_resource(token: &str) -> Option<Resource> {
    match token {
        "wood" => Some(Resource::Wood),
        "stone" => Some(Resource::Stone),
        "brick" => Some(Resource::Clay),
        "ore" => Some(Resource::Ore),
        "glass" => Some(Resource::Glass),
        "paper" => Some(Resource::Papyrus),
        "cloth" => Some(Resource::Loom),
        _ => None,
    }
}
use serde::Deserialize;
use std::collections::HashMap;

pub const GIZAH_DAY_ID: &str = "gizah_day";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WonderStageEffect {
    ProduceRawChoice,
    ProduceManufacturedChoice,
    ScienceChoice,
    PlayFromDiscard,
    SixthRoundExtraPlay,
    FirstPerColorFree,
    FirstPerAgeFree,
    LastPerAgeFree,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WonderStage {
    pub stage: u8,
    pub cost: Vec<String>,
    pub benefit_text: String,
    pub vp: i32,
    pub coins: i32,
    pub military: i32,
    pub effect: Option<WonderStageEffect>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WonderBoard {
    pub id: String,
    pub wonder: String,
    pub name: String,
    pub side: String,
    pub token: String,
    pub display_name: String,
    pub stages: Vec<WonderStage>,
}

#[derive(Debug, Clone)]
pub struct WonderDatabase {
    boards: HashMap<String, WonderBoard>,
    all_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WonderStageView {
    pub stage: u8,
    pub resources: Vec<Resource>,
    pub benefit_text: String,
    pub vp: i32,
}

impl WonderDatabase {
    pub fn load() -> Self {
        let path = "games/seven_wonders/data/wonders.json";
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Failed to read {}", path));
        let root: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e));
        let boards_vec: Vec<WonderBoard> = serde_json::from_value(
            root.get("boards")
                .cloned()
                .unwrap_or_else(|| panic!("wonders.json missing boards")),
        )
        .unwrap_or_else(|e| panic!("Failed to parse wonder boards: {}", e));

        let mut boards = HashMap::new();
        let mut all_ids = Vec::new();
        for board in boards_vec {
            all_ids.push(board.id.clone());
            boards.insert(board.id.clone(), board);
        }
        all_ids.sort();
        Self { boards, all_ids }
    }

    pub fn all_board_ids(&self) -> &[String] {
        &self.all_ids
    }

    pub fn get(&self, board_id: &str) -> Option<&WonderBoard> {
        self.boards.get(board_id)
    }

    pub fn token_resource_for(&self, board_id: &str) -> Option<Resource> {
        self.get(board_id)
            .and_then(|b| token_resource(&b.token))
    }

    pub fn display_name<'a>(&'a self, board_id: &'a str) -> &'a str {
        if let Some(board) = self.boards.get(board_id) {
            board.display_name.as_str()
        } else {
            board_id
        }
    }

    pub fn total_stages(&self, board_id: &str) -> u8 {
        self.boards
            .get(board_id)
            .map(|b| b.stages.len() as u8)
            .unwrap_or(0)
    }

    pub fn stage(&self, board_id: &str, stage: u8) -> Option<&WonderStage> {
        self.boards.get(board_id)?.stages.iter().find(|s| s.stage == stage)
    }

    pub fn stages_for(&self, board_id: &str) -> Vec<WonderStageView> {
        let Some(board) = self.boards.get(board_id) else {
            return vec![];
        };
        board
            .stages
            .iter()
            .map(|s| WonderStageView {
                stage: s.stage,
                resources: parse_cost_resources(&s.cost),
                benefit_text: s.benefit_text.clone(),
                vp: s.vp,
            })
            .collect()
    }

    pub fn stage_cost(&self, board_id: &str, stage: u8) -> Cost {
        let Some(stage_def) = self.stage(board_id, stage) else {
            return Cost::default();
        };
        resources_to_cost(&parse_cost_resources(&stage_def.cost))
    }

    pub fn wonder_vp(&self, board_id: &str, stages_built: u8) -> i32 {
        let Some(board) = self.boards.get(board_id) else {
            return 0;
        };
        board
            .stages
            .iter()
            .take(stages_built as usize)
            .map(|s| s.vp)
            .sum()
    }

    pub fn assign_unique_random(&self, player_count: u8) -> Vec<String> {
        let n = player_count as usize;
        assert!(n <= self.all_ids.len(), "not enough wonder boards for {} players", n);
        let mut ids = self.all_ids.clone();
        shuffle_strings(&mut ids);
        ids.into_iter().take(n).collect()
    }

    #[cfg(test)]
    pub fn assign_fixed(&self, board_id: &str, player_count: u8) -> Vec<String> {
        vec![board_id.to_string(); player_count as usize]
    }
}

fn shuffle_strings(ids: &mut [String]) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for i in (1..ids.len()).rev() {
        let mut hash = seed.wrapping_add(i as u128);
        hash ^= hash << 13;
        hash ^= hash >> 7;
        hash ^= hash << 17;
        let j = (hash as usize) % (i + 1);
        ids.swap(i, j);
    }
}

fn parse_resource_key(key: &str) -> Option<Resource> {
    match key {
        "wood" => Some(Resource::Wood),
        "stone" => Some(Resource::Stone),
        "clay" => Some(Resource::Clay),
        "ore" => Some(Resource::Ore),
        "glass" => Some(Resource::Glass),
        "papyrus" => Some(Resource::Papyrus),
        "loom" => Some(Resource::Loom),
        _ => None,
    }
}

fn parse_cost_resources(cost: &[String]) -> Vec<Resource> {
    cost.iter()
        .filter_map(|k| parse_resource_key(k))
        .collect()
}

fn resources_to_cost(resources: &[Resource]) -> Cost {
    let mut counts = Resources::default();
    for r in resources {
        counts.add(*r, 1);
    }
    Cost {
        coins: 0,
        resources: counts,
    }
}

pub fn format_resources_bracket(resources: &[Resource]) -> String {
    if resources.is_empty() {
        "[]".to_string()
    } else {
        let parts: Vec<_> = resources.iter().map(|r| friendly_resource(*r)).collect();
        format!("[{}]", parts.join(", "))
    }
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

fn format_benefit_text(text: &str) -> String {
    if text.contains("point") {
        text.replace("points", "points").replace("point", "point")
    } else {
        text.to_string()
    }
}

fn format_wonder_rows(rows: &[(String, String, String, String)]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let w_label = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
    let w_progress = rows.iter().map(|r| r.1.len()).max().unwrap_or(0);
    let w_cost = rows.iter().map(|r| r.2.len()).max().unwrap_or(0);
    rows.iter()
        .map(|(label, progress, cost, benefit)| {
            format!(
                "{}\t{}{}{}{}{}",
                pad_field(label, w_label),
                pad_field(progress, w_progress),
                TAB_PAIR,
                pad_field(cost, w_cost),
                TAB_PAIR,
                benefit
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_wonder_stages_overview(db: &WonderDatabase, board_id: &str) -> String {
    let Some(board) = db.get(board_id) else {
        return String::new();
    };
    let total = board.stages.len();
    let rows: Vec<_> = db
        .stages_for(board_id)
        .iter()
        .map(|s| {
            (
                "wonder".to_string(),
                format!("({}/{})", s.stage, total),
                format_resources_bracket(&s.resources),
                format_benefit_text(&s.benefit_text),
            )
        })
        .collect();
    format!(
        "wonder stages ({}):\n{}",
        board.display_name,
        format_wonder_rows(&rows)
    )
}

pub fn format_wonder_current_stage(db: &WonderDatabase, board_id: &str, stages_built: u8) -> String {
    let total = db.total_stages(board_id);
    if total == 0 {
        return format!("wonder\t(?/?)\t—\tunknown board");
    }
    if stages_built >= total {
        let vp = db.wonder_vp(board_id, stages_built);
        return format!("wonder\t({total}/{total})\t—\tall stages built (+{vp} VP)");
    }
    let next = stages_built + 1;
    let Some(stage) = db.stages_for(board_id).into_iter().find(|s| s.stage == next) else {
        return format!("wonder\t({stages_built}/{total})\t—\tunknown stage");
    };
    format!(
        "wonder\t({stages_built}/{total})\t{}\t{}",
        format_resources_bracket(&stage.resources),
        format_benefit_text(&stage.benefit_text)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_loads_gizah_day() {
        let db = WonderDatabase::load();
        let board = db.get(GIZAH_DAY_ID).expect("gizah_day");
        assert_eq!(board.stages.len(), 3);
        assert_eq!(board.stages[0].vp, 3);
        assert_eq!(db.wonder_vp(GIZAH_DAY_ID, 3), 15);
    }

    #[test]
    fn gizah_day_overview_lists_all_three_stages() {
        let db = WonderDatabase::load();
        let overview = format_wonder_stages_overview(&db, GIZAH_DAY_ID);
        assert!(overview.contains("Gizah (day)"));
        assert!(overview.contains("[wood, wood]"));
        assert!(overview.contains("+3 points"));
        assert!(overview.contains("[brick, brick, cloth]"));
        assert!(overview.contains("+5 points"));
        assert!(overview.contains("[stone, stone, stone, stone]"));
        assert!(overview.contains("+7 points"));
    }

    #[test]
    fn current_stage_line_tracks_progress() {
        let db = WonderDatabase::load();
        assert!(format_wonder_current_stage(&db, GIZAH_DAY_ID, 0).contains("(0/3)"));
        assert!(format_wonder_current_stage(&db, GIZAH_DAY_ID, 0).contains("[wood, wood]"));
        assert!(format_wonder_current_stage(&db, GIZAH_DAY_ID, 1).contains("(1/3)"));
        assert!(format_wonder_current_stage(&db, GIZAH_DAY_ID, 1).contains("[brick, brick, cloth]"));
        assert!(format_wonder_current_stage(&db, GIZAH_DAY_ID, 3).contains("all stages built"));
        assert!(format_wonder_current_stage(&db, GIZAH_DAY_ID, 3).contains("+15 VP"));
    }

    #[test]
    fn token_resource_maps_civilization_tokens() {
        assert_eq!(token_resource("paper"), Some(Resource::Papyrus));
        assert_eq!(token_resource("cloth"), Some(Resource::Loom));
        let db = WonderDatabase::load();
        assert_eq!(
            db.token_resource_for("ephesos_day"),
            Some(Resource::Papyrus)
        );
    }

    #[test]
    fn assigns_unique_boards_for_seven_players() {
        let db = WonderDatabase::load();
        let ids = db.assign_unique_random(7);
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), 7);
    }
}