//! End-of-game scoring for Seven Wonders (base game).

use super::cards::CardDatabase;
use super::types::{Effect, ScienceSymbol};
use std::collections::HashMap;

/// Breakdown of a player's final score.
#[derive(Debug, Clone, Default)]
pub struct ScoreBreakdown {
    pub military_victory: i32,
    pub military_defeat: i32,
    pub treasury: i32,
    pub wonders: i32,
    pub civilian: i32,
    pub science: i32,
    pub guilds: i32,
    pub commerce: i32,
    pub total: i32,
}

pub fn compute_final_score(
    card_db: &CardDatabase,
    player: usize,
    players: &[super::state::PlayerState],
    player_count: u8,
) -> ScoreBreakdown {
    let p = &players[player];
    let board = &p.board;
    let n = player_count as usize;

    let mut breakdown = ScoreBreakdown::default();
    breakdown.military_victory = board.military_victory_vp as i32;
    breakdown.military_defeat = -(board.defeat_tokens as i32);
    breakdown.treasury = (board.coins / 3) as i32;
    breakdown.wonders = wonder_vp(&board.wonder_id, board.wonder_stages_built);

    for cid in &board.played_cards {
        let Some(card) = card_db.get(cid) else { continue };
        match &card.effect {
            Effect::VictoryPoints(vp) => {
                if card.color == "blue" {
                    breakdown.civilian += *vp;
                } else {
                    breakdown.commerce += *vp;
                }
            }
            _ => {}
        }
    }

    breakdown.science = science_vp(&board.science_symbols, &board.played_cards);

    for cid in &board.played_cards {
        if card_db.get(cid).map(|c| c.color.as_str()) == Some("purple") {
            breakdown.guilds += guild_vp(card_db, cid, player, players, n);
        }
    }

    breakdown.total = breakdown.military_victory
        + breakdown.military_defeat
        + breakdown.treasury
        + breakdown.wonders
        + breakdown.civilian
        + breakdown.commerce
        + breakdown.science
        + breakdown.guilds;

    breakdown
}

fn wonder_vp(wonder_id: &str, stages: u8) -> i32 {
    if wonder_id == "gizah_a" {
        let per_stage = [3i32, 5, 7];
        (0..stages as usize)
            .map(|i| per_stage.get(i).copied().unwrap_or(0))
            .sum()
    } else {
        0
    }
}

fn science_vp(symbols: &[String], played: &[String]) -> i32 {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for s in symbols {
        *counts.entry(s.as_str()).or_insert(0) += 1;
    }
    if played.iter().any(|c| c == "scientists_guild") {
        for sym in ["compass", "tablet", "gear"] {
            *counts.entry(sym).or_insert(0) += 1;
        }
    }
    let mut score = 0i32;
    for sym in ["compass", "tablet", "gear"] {
        let n = *counts.get(sym).unwrap_or(&0);
        score += (n * (n + 1) / 2) as i32;
    }
    let sets = counts
        .get("compass")
        .copied()
        .unwrap_or(0)
        .min(counts.get("tablet").copied().unwrap_or(0))
        .min(counts.get("gear").copied().unwrap_or(0));
    score + (sets as i32) * 7
}

fn guild_vp(
    card_db: &CardDatabase,
    guild_id: &str,
    player: usize,
    players: &[super::state::PlayerState],
    n: usize,
) -> i32 {
    let left = (player + n - 1) % n;
    let right = (player + 1) % n;
    let count_color = |idx: usize, color: &str| -> i32 {
        players[idx]
            .board
            .played_cards
            .iter()
            .filter(|cid| card_db.get(cid).map(|c| c.color == color).unwrap_or(false))
            .count() as i32
    };
    let wonder_stages = |idx: usize| -> i32 { players[idx].board.wonder_stages_built as i32 };

    match guild_id {
        "workers_guild" => count_color(left, "brown") + count_color(right, "brown"),
        "craftsmens_guild" => 2 * (count_color(left, "grey") + count_color(right, "grey")),
        "magistrates_guild" => count_color(left, "blue") + count_color(right, "blue"),
        "traders_guild" => count_color(left, "yellow") + count_color(right, "yellow"),
        "spies_guild" => count_color(left, "red") + count_color(right, "red"),
        "philosophers_guild" => count_color(left, "green") + count_color(right, "green"),
        "shipowners_guild" => {
            let own = &players[player].board.played_cards;
            own.iter()
                .filter(|cid| {
                    card_db
                        .get(cid)
                        .map(|c| matches!(c.color.as_str(), "brown" | "grey" | "purple"))
                        .unwrap_or(false)
                })
                .count() as i32
        }
        "builders_guild" => {
            wonder_stages(player) + wonder_stages(left) + wonder_stages(right)
        }
        "decorators_guild" => {
            if players[player].board.wonder_stages_built >= 3 {
                7
            } else {
                0
            }
        }
        "scientists_guild" => 0, // handled in science_vp
        _ => 0,
    }
}

pub fn science_symbol_name(sym: ScienceSymbol) -> &'static str {
    match sym {
        ScienceSymbol::Compass => "compass",
        ScienceSymbol::Tablet => "tablet",
        ScienceSymbol::Gear => "gear",
    }
}