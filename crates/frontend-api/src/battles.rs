//! Battle archive queries and battle-result serializers.
//!
//! Verbatim moves from `crates/wasm-bridge/src/lib.rs` — bodies must stay
//! byte-identical to the originals (error JSON strings included). The
//! serializers are `pub` because `crate::turn` also uses them for turn
//! reports.

use crate::ApiError;
use domain::game_state::GameState;
use domain::military::combat::BattleResult;
use domain::military::naval::NavalBattleResult;
use domain::military::units::ArmyUnit;

/// Serialize a land battle result to JSON, resolving nation/province names from game state.
pub fn serialize_battle(b: &BattleResult, game: &GameState) -> serde_json::Value {
    let attacker_name = game
        .get_nation(b.attacker)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let defender_name = game
        .get_nation(b.defender)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let province_name = game
        .get_province(b.province)
        .map(|p| p.name.as_str())
        .unwrap_or("Unknown");
    let capital_tile = game
        .get_province(b.province)
        .map(|p| serde_json::json!({"q": p.capital_tile.q, "r": p.capital_tile.r}));
    let province_tiles: Vec<serde_json::Value> = game
        .get_province(b.province)
        .map(|p| {
            p.tiles
                .iter()
                .map(|t| serde_json::json!({"q": t.q, "r": t.r}))
                .collect()
        })
        .unwrap_or_default();
    let origin_tiles: Vec<serde_json::Value> = b
        .attacker_origin_provinces
        .iter()
        .filter_map(|pid| {
            game.get_province(*pid)
                .map(|p| serde_json::json!({"q": p.capital_tile.q, "r": p.capital_tile.r}))
        })
        .collect();
    let origin_province_names: Vec<String> = b
        .attacker_origin_provinces
        .iter()
        .filter_map(|pid| game.get_province(*pid).map(|p| p.name.clone()))
        .collect();

    let serialize_units = |units: &[ArmyUnit]| -> Vec<serde_json::Value> {
        units
            .iter()
            .map(|u| {
                serde_json::json!({
                    "unit_type": format!("{:?}", u.unit_type),
                    "health": u.health,
                    "medals": u.medals,
                    "effective_firepower": u.effective_firepower(),
                })
            })
            .collect()
    };

    let serialize_unit_logs =
        |logs: &[domain::military::combat::BattleUnitLog]| -> Vec<serde_json::Value> {
            logs.iter()
                .map(|log| {
                    let breakdown = log.defender_breakdown.as_ref().map(|b| {
                        serde_json::json!({
                            "applied_firepower": b.applied_firepower,
                            "fort_multiplier": b.fort_multiplier,
                            "entrenchment_fp": b.entrenchment_fp,
                            "initial_total_contribution": b.initial_total_contribution,
                        })
                    });
                    serde_json::json!({
                        "unit_type": format!("{:?}", log.unit_type),
                        "medals_initial": log.medals_initial,
                        "medals_final": log.medals_final,
                        "initial_health": log.initial_health,
                        "final_health": log.final_health,
                        "initial_firepower": log.initial_firepower,
                        "final_firepower": log.final_firepower,
                        "defender_breakdown": breakdown,
                    })
                })
                .collect()
        };

    let round_logs: Vec<serde_json::Value> = b
        .round_logs
        .iter()
        .map(|r| {
            serde_json::json!({
                "round": r.round,
                "first_strike_side": r.first_strike_side,
                "atk_fp": r.atk_fp,
                "def_fp": r.def_fp,
                "atk_shots": r.atk_shots,
                "def_shots": r.def_shots,
                "atk_casualties": r.atk_casualties.iter().map(|c| format!("{:?}", c)).collect::<Vec<_>>(),
                "def_casualties": r.def_casualties.iter().map(|c| format!("{:?}", c)).collect::<Vec<_>>(),
                "retreat_triggered": r.retreat_triggered,
            })
        })
        .collect();

    let retreat_debug = b.retreat_debug.as_ref().map(|d| {
        serde_json::json!({
            "side": d.side,
            "stage": d.stage.as_str(),
            "measured_value": d.measured_value,
            "threshold": d.threshold,
            "attacker_prebattle_ratio": d.attacker_prebattle_ratio,
            "defender_prebattle_ratio": d.defender_prebattle_ratio,
            "attacker_prebattle_threshold": d.attacker_prebattle_threshold,
            "defender_prebattle_threshold": d.defender_prebattle_threshold,
            "round": d.round,
        })
    });

    serde_json::json!({
        "type": "land",
        "attacker": attacker_name,
        "attacker_id": b.attacker.0,
        "defender": defender_name,
        "defender_id": b.defender.0,
        "province": province_name,
        "province_id": b.province.0,
        "attacker_won": b.attacker_won,
        "retreated": b.retreated,
        "defender_retreated": b.defender_retreated,
        "attacker_casualties": b.attacker_casualties.iter()
            .map(|c| format!("{:?}", c)).collect::<Vec<_>>(),
        "defender_casualties": b.defender_casualties.iter()
            .map(|c| format!("{:?}", c)).collect::<Vec<_>>(),
        "attacker_survivors": serialize_units(&b.attacker_survivors),
        "defender_survivors": serialize_units(&b.defender_survivors),
        "terrain": b.terrain.map(|t| format!("{:?}", t)),
        "fort_level": b.fort_level,
        "siege_reduced_fort": b.siege_reduced_fort,
        "attacker_initial_count": b.attacker_initial_count,
        "defender_initial_count": b.defender_initial_count,
        "attacker_initial_fp": b.attacker_initial_fp,
        "defender_initial_fp": b.defender_initial_fp,
        "attacker_survivors_count": b.attacker_initial_count.saturating_sub(b.attacker_casualties.len()),
        "defender_survivors_count": b.defender_initial_count.saturating_sub(b.defender_casualties.len()),
        "medal_awards": b.medal_awards.iter()
            .map(|(t, c)| serde_json::json!({"unit_type": format!("{:?}", t), "medals": c}))
            .collect::<Vec<_>>(),
        "capital_tile": capital_tile,
        "province_tiles": province_tiles,
        "origin_tiles": origin_tiles,
        "origin_province_names": origin_province_names,
        "is_naval_landing": b.is_naval_landing,
        "retreat_debug": retreat_debug,
        // Card #478 follow-up: per-unit logs for the battle-screen
        // "Show firepower" debug toggle.
        "attacker_unit_logs": serialize_unit_logs(&b.attacker_unit_logs),
        "defender_unit_logs": serialize_unit_logs(&b.defender_unit_logs),
        "round_logs": round_logs,
    })
}

/// Serialize a naval battle result to JSON, resolving nation names from game state.
pub fn serialize_naval_battle(nb: &NavalBattleResult, game: &GameState) -> serde_json::Value {
    let attacker_name = game
        .get_nation(nb.attacker)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let defender_name = game
        .get_nation(nb.defender)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");

    serde_json::json!({
        "type": "naval",
        "attacker": attacker_name,
        "attacker_id": nb.attacker.0,
        "defender": defender_name,
        "defender_id": nb.defender.0,
        "attacker_won": nb.attacker_won,
        "attacker_ships_lost": nb.attacker_ships_lost.iter()
            .map(|s| format!("{:?}", s)).collect::<Vec<_>>(),
        "defender_ships_lost": nb.defender_ships_lost.iter()
            .map(|s| format!("{:?}", s)).collect::<Vec<_>>(),
        "attacker_survivors_count": nb.attacker_survivors.len(),
        "defender_survivors_count": nb.defender_survivors.len(),
    })
}

/// Return the battle archive for all past turns.
pub fn get_battle_data(game: &GameState) -> Result<serde_json::Value, ApiError> {
    let archive: Vec<serde_json::Value> = game
        .archive
        .battle_archive
        .iter()
        .map(|(turn, battles, naval_battles)| {
            let land: Vec<serde_json::Value> =
                battles.iter().map(|b| serialize_battle(b, game)).collect();
            let naval: Vec<serde_json::Value> = naval_battles
                .iter()
                .map(|nb| serialize_naval_battle(nb, game))
                .collect();
            serde_json::json!({
                "turn": turn.0,
                "year": turn.year(),
                "quarter": turn.quarter(),
                "battles": land,
                "naval_battles": naval,
            })
        })
        .collect();

    Ok(serde_json::Value::Array(archive))
}
