//! Wonder definitions and display formatting (currently Gizah A for all players).

use super::cards::{pad_field, TAB_PAIR};
use super::types::{Cost, Resource, Resources};

pub const GIZAH_A_ID: &str = "gizah_a";
pub const GIZAH_A_TOTAL_STAGES: u8 = 3;

#[derive(Debug, Clone)]
pub struct WonderStageDef {
    pub stage: u8,
    pub vp: i32,
    pub resources: Vec<Resource>,
}

pub fn gizah_a_stages() -> Vec<WonderStageDef> {
    vec![
        WonderStageDef {
            stage: 1,
            vp: 3,
            resources: vec![Resource::Wood, Resource::Wood],
        },
        WonderStageDef {
            stage: 2,
            vp: 5,
            resources: vec![Resource::Clay, Resource::Clay, Resource::Loom],
        },
        WonderStageDef {
            stage: 3,
            vp: 7,
            resources: vec![
                Resource::Stone,
                Resource::Stone,
                Resource::Stone,
                Resource::Stone,
            ],
        },
    ]
}

pub fn stages_for_wonder(wonder_id: &str) -> Vec<WonderStageDef> {
    if wonder_id == GIZAH_A_ID {
        gizah_a_stages()
    } else {
        vec![]
    }
}

pub fn stage_def(wonder_id: &str, stage: u8) -> Option<WonderStageDef> {
    stages_for_wonder(wonder_id)
        .into_iter()
        .find(|s| s.stage == stage)
}

pub fn stage_to_cost(def: &WonderStageDef) -> Cost {
    let mut resources = Resources::default();
    for r in &def.resources {
        resources.add(*r, 1);
    }
    Cost {
        coins: 0,
        resources,
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

pub fn format_resources_bracket(resources: &[Resource]) -> String {
    if resources.is_empty() {
        "[]".to_string()
    } else {
        let parts: Vec<_> = resources.iter().map(|r| friendly_resource(*r)).collect();
        format!("[{}]", parts.join(", "))
    }
}

fn format_benefit(vp: i32) -> String {
    if vp == 1 {
        "+1 point".to_string()
    } else {
        format!("+{vp} points")
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

/// All wonder stages for startup context (before any are built).
pub fn format_wonder_stages_overview(wonder_id: &str) -> String {
    let stages = stages_for_wonder(wonder_id);
    if stages.is_empty() {
        return String::new();
    }
    let rows: Vec<_> = stages
        .iter()
        .map(|s| {
            (
                "wonder".to_string(),
                format!("({}/{})", s.stage, GIZAH_A_TOTAL_STAGES),
                format_resources_bracket(&s.resources),
                format_benefit(s.vp),
            )
        })
        .collect();
    format!("wonder stages (Gizah A):\n{}", format_wonder_rows(&rows))
}

/// Current/next wonder stage for per-turn decision context.
pub fn format_wonder_current_stage(wonder_id: &str, stages_built: u8) -> String {
    let total = GIZAH_A_TOTAL_STAGES;
    if stages_built >= total {
        let vp: i32 = gizah_a_stages().iter().map(|s| s.vp).sum();
        return format!(
            "wonder\t({total}/{total})\t—\tall stages built (+{vp} VP)"
        );
    }
    let next = stages_built + 1;
    let Some(def) = stage_def(wonder_id, next) else {
        return format!("wonder\t({stages_built}/{total})\t—\tunknown wonder");
    };
    format!(
        "wonder\t({stages_built}/{total})\t{}\t{}",
        format_resources_bracket(&def.resources),
        format_benefit(def.vp)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gizah_a_overview_lists_all_three_stages() {
        let overview = format_wonder_stages_overview(GIZAH_A_ID);
        assert!(overview.contains("wonder stages (Gizah A)"));
        assert!(overview.contains("[wood, wood]"));
        assert!(overview.contains("+3 points"));
        assert!(overview.contains("[brick, brick, cloth]"));
        assert!(overview.contains("+5 points"));
        assert!(overview.contains("[stone, stone, stone, stone]"));
        assert!(overview.contains("+7 points"));
    }

    #[test]
    fn current_stage_line_tracks_progress() {
        assert!(format_wonder_current_stage(GIZAH_A_ID, 0).contains("(0/3)"));
        assert!(format_wonder_current_stage(GIZAH_A_ID, 0).contains("[wood, wood]"));
        assert!(format_wonder_current_stage(GIZAH_A_ID, 1).contains("(1/3)"));
        assert!(format_wonder_current_stage(GIZAH_A_ID, 1).contains("[brick, brick, cloth]"));
        assert!(format_wonder_current_stage(GIZAH_A_ID, 3).contains("all stages built"));
        assert!(format_wonder_current_stage(GIZAH_A_ID, 3).contains("+15 VP"));
    }
}