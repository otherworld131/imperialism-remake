//! Thin JSON wrappers exposing the `frontend-api` crate to the web build.
//!
//! Every `wasm_*` export keeps its original name, signature, and JSON
//! contract (pinned by `tests/golden_contract.rs`); the bodies live in
//! `crates/frontend-api`, shared with the native Bevy frontend. Each
//! wrapper only (de)serializes the game state and delegates.

use domain::game_state::GameState;
use domain::hex::HexCoord;
use frontend_api::{ApiError, game_to_json, game_to_value};
use wasm_bindgen::prelude::*;

#[cfg(test)]
mod tests;

// ── Wrapper plumbing ─────────────────────────────────────────────────────

fn run_query(
    game_json: &str,
    f: impl FnOnce(&GameState) -> Result<serde_json::Value, ApiError>,
) -> String {
    match frontend_api::game_from_json(game_json) {
        Ok(game) => match f(&game) {
            Ok(v) => v.to_string(),
            Err(e) => e.0,
        },
        Err(e) => e.0,
    }
}

fn run_command(game_json: &str, f: impl FnOnce(&mut GameState) -> Result<(), ApiError>) -> String {
    match frontend_api::game_from_json(game_json) {
        Ok(mut game) => match f(&mut game) {
            Ok(()) => game_to_json(&game),
            Err(e) => e.0,
        },
        Err(e) => e.0,
    }
}

// Legacy helper names used throughout the behavior tests.
#[cfg(test)]
fn serialize_game(game: &GameState) -> String {
    game_to_json(game)
}

#[cfg(test)]
fn game_from_json(json: &str) -> Result<GameState, String> {
    frontend_api::game_from_json(json).map_err(|e| e.message())
}

#[cfg(test)]
fn deserialize_game(json: &str) -> Result<GameState, String> {
    frontend_api::game_from_json(json).map_err(|e| e.0)
}

// ── Lifecycle / setup ────────────────────────────────────────────────────

#[wasm_bindgen]
pub fn wasm_debug_marker() -> String {
    frontend_api::setup::debug_marker()
}

#[wasm_bindgen]
pub fn wasm_max_workers_supportable(grain: u32, fruit: u32, meat: u32) -> u32 {
    frontend_api::setup::max_workers_supportable(grain, fruit, meat)
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn wasm_new_game(
    map_key: &str,
    difficulty: u8,
    nation_index: usize,
    map_width: i32,
    map_height: i32,
    num_great_powers: u32,
    num_minor_nations: u32,
    flavor_key: &str,
    terrain_json: &str,
    has_capital_override: bool,
    capital_q: i32,
    capital_r: i32,
) -> String {
    let capital_override = has_capital_override.then_some(HexCoord::new(capital_q, capital_r));
    let game = frontend_api::setup::new_game(
        map_key,
        difficulty,
        nation_index,
        map_width,
        map_height,
        num_great_powers,
        num_minor_nations,
        flavor_key,
        terrain_json,
        capital_override,
    );
    game_to_json(&game)
}

#[wasm_bindgen]
pub fn wasm_new_scenario_game(
    scenario_id: &str,
    difficulty: u8,
    nation_index: usize,
    flavor_key: &str,
    has_capital_override: bool,
    capital_q: i32,
    capital_r: i32,
) -> String {
    let capital_override = has_capital_override.then_some(HexCoord::new(capital_q, capital_r));
    match frontend_api::setup::new_scenario_game(
        scenario_id,
        difficulty,
        nation_index,
        flavor_key,
        capital_override,
    ) {
        Ok(game) => game_to_json(&game),
        Err(e) => e.0,
    }
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn wasm_new_observer_game(
    map_key: &str,
    difficulty: u8,
    map_width: i32,
    map_height: i32,
    num_great_powers: u32,
    num_minor_nations: u32,
    flavor_key: &str,
    terrain_json: &str,
) -> String {
    let game = frontend_api::setup::new_observer_game(
        map_key,
        difficulty,
        map_width,
        map_height,
        num_great_powers,
        num_minor_nations,
        flavor_key,
        terrain_json,
    );
    game_to_json(&game)
}

#[wasm_bindgen]
pub fn wasm_new_observer_scenario_game(
    scenario_id: &str,
    difficulty: u8,
    flavor_key: &str,
) -> String {
    match frontend_api::setup::new_observer_scenario_game(scenario_id, difficulty, flavor_key) {
        Ok(game) => game_to_json(&game),
        Err(e) => e.0,
    }
}

#[wasm_bindgen]
pub fn wasm_apply_flavor(game_json: &str, flavor_key: &str) -> String {
    run_command(game_json, |game| {
        frontend_api::flavor::reroll_flavor(game, flavor_key);
        Ok(())
    })
}

#[wasm_bindgen]
pub fn wasm_set_human_player(game_json: &str, nation_index: usize) -> String {
    run_command(game_json, |game| {
        frontend_api::setup::set_human_player(game, nation_index)
    })
}

#[wasm_bindgen]
pub fn wasm_get_scenarios() -> String {
    frontend_api::setup::get_scenarios().to_string()
}

// ── Turn processing ──────────────────────────────────────────────────────

#[wasm_bindgen]
pub fn wasm_process_turn(game_json: &str) -> String {
    match frontend_api::game_from_json(game_json) {
        Ok(mut game) => {
            let mut resp = frontend_api::turn::process_turn(&mut game);
            resp["game"] = game_to_value(&game);
            resp.to_string()
        }
        Err(e) => e.0,
    }
}

#[wasm_bindgen]
pub fn wasm_process_turns(game_json: &str, count: u32) -> String {
    match frontend_api::game_from_json(game_json) {
        Ok(mut game) => {
            let mut resp = frontend_api::turn::process_turns(&mut game, count);
            resp["game"] = game_to_value(&game);
            resp.to_string()
        }
        Err(e) => e.0,
    }
}

// ── Map / overlays ───────────────────────────────────────────────────────

#[wasm_bindgen]
pub fn wasm_get_map_data(game_json: &str, disable_fog: bool) -> String {
    run_query(game_json, |g| {
        frontend_api::map::get_map_data(g, disable_fog)
    })
}

#[wasm_bindgen]
pub fn wasm_get_navy_markers(game_json: &str, disable_fog: bool) -> String {
    run_query(game_json, |g| {
        frontend_api::map::get_navy_markers(g, disable_fog)
    })
}

#[wasm_bindgen]
pub fn wasm_get_sea_zones(game_json: &str) -> String {
    run_query(game_json, frontend_api::map::get_sea_zones)
}

#[wasm_bindgen]
pub fn wasm_get_diplomacy_overlay(game_json: &str, nation_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::map::get_diplomacy_overlay(g, nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_get_military_overlay(game_json: &str) -> String {
    run_query(game_json, frontend_api::map::get_military_overlay)
}

#[wasm_bindgen]
pub fn wasm_get_political_snapshot(game_json: &str, turn: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::map::get_political_snapshot(g, turn)
    })
}

// ── Tech ─────────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub fn wasm_get_available_techs(game_json: &str) -> String {
    run_query(game_json, frontend_api::tech::get_available_techs)
}

#[wasm_bindgen]
pub fn wasm_research_tech(game_json: &str, tech_name: &str) -> String {
    run_command(game_json, |g| {
        frontend_api::tech::research_tech(g, tech_name)
    })
}

#[wasm_bindgen]
pub fn wasm_get_tech_screen_data(game_json: &str) -> String {
    run_query(game_json, frontend_api::tech::get_tech_screen_data)
}

#[wasm_bindgen]
pub fn wasm_queue_tech_research(game_json: &str, tech_name: &str) -> String {
    run_command(game_json, |g| {
        frontend_api::tech::queue_tech_research(g, tech_name)
    })
}

#[wasm_bindgen]
pub fn wasm_cancel_tech_research(game_json: &str) -> String {
    run_command(game_json, frontend_api::tech::cancel_tech_research)
}

// ── Units / civilians / navy ─────────────────────────────────────────────

#[wasm_bindgen]
pub fn wasm_get_units_in_province(game_json: &str, province_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::units::get_units_in_province(g, province_id)
    })
}

#[wasm_bindgen]
pub fn wasm_get_civilians(game_json: &str, nation_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::units::get_civilians(g, nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_get_ships(game_json: &str, nation_id: u32) -> String {
    run_query(game_json, |g| frontend_api::units::get_ships(g, nation_id))
}

#[wasm_bindgen]
pub fn wasm_get_valid_move_targets(game_json: &str, nation_id: u32, unit_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::units::get_valid_move_targets(g, nation_id, unit_id)
    })
}

#[wasm_bindgen]
pub fn wasm_get_buildable_units(game_json: &str, nation_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::units::get_buildable_units(g, nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_queue_unit_move(
    game_json: &str,
    nation_id: u32,
    unit_id: u32,
    dest_province_id: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::units::queue_unit_move(g, nation_id, unit_id, dest_province_id)
    })
}

#[wasm_bindgen]
pub fn wasm_cancel_unit_move(game_json: &str, unit_id: u32) -> String {
    run_command(game_json, |g| {
        frontend_api::units::cancel_unit_move(g, unit_id)
    })
}

#[wasm_bindgen]
pub fn wasm_disband_unit(game_json: &str, unit_id: u32) -> String {
    run_command(game_json, |g| frontend_api::units::disband_unit(g, unit_id))
}

#[wasm_bindgen]
pub fn wasm_deploy_civilian(game_json: &str, civilian_id: u32, hex_q: i32, hex_r: i32) -> String {
    run_command(game_json, |g| {
        frontend_api::units::deploy_civilian(g, civilian_id, hex_q, hex_r)
    })
}

#[wasm_bindgen]
pub fn wasm_recall_civilian(game_json: &str, civilian_id: u32) -> String {
    run_command(game_json, |g| {
        frontend_api::units::recall_civilian(g, civilian_id)
    })
}

#[wasm_bindgen]
pub fn wasm_engineer_build(game_json: &str, civilian_id: u32, build_kind: &str) -> String {
    run_command(game_json, |g| {
        frontend_api::units::engineer_build(g, civilian_id, build_kind)
    })
}

#[wasm_bindgen]
pub fn wasm_recruit_army_unit(game_json: &str, nation_id: u32, unit_type_str: &str) -> String {
    run_command(game_json, |g| {
        frontend_api::units::recruit_army_unit(g, nation_id, unit_type_str)
    })
}

#[wasm_bindgen]
pub fn wasm_set_pending_army_recruits(
    game_json: &str,
    nation_id: u32,
    unit_type_str: &str,
    count: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::units::set_pending_army_recruits(g, nation_id, unit_type_str, count)
    })
}

#[wasm_bindgen]
pub fn wasm_upgrade_unit(game_json: &str, nation_id: u32, unit_id: u32) -> String {
    run_command(game_json, |g| {
        frontend_api::units::upgrade_unit(g, nation_id, unit_id)
    })
}

#[wasm_bindgen]
pub fn wasm_upgrade_units(game_json: &str, nation_id: u32, unit_ids_json: &str) -> String {
    match frontend_api::game_from_json(game_json) {
        Ok(mut game) => {
            match frontend_api::units::upgrade_units(&mut game, nation_id, unit_ids_json) {
                Ok(mut resp) => {
                    resp["game"] = game_to_value(&game);
                    resp.to_string()
                }
                Err(e) => e.0,
            }
        }
        Err(e) => e.0,
    }
}

#[wasm_bindgen]
pub fn wasm_get_upgrade_info(game_json: &str, nation_id: u32, unit_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::units::get_upgrade_info(g, nation_id, unit_id)
    })
}

#[wasm_bindgen]
pub fn wasm_set_pending_civilian_hire(
    game_json: &str,
    nation_id: u32,
    civilian_type_str: &str,
    count: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::units::set_pending_civilian_hire(g, nation_id, civilian_type_str, count)
    })
}

#[wasm_bindgen]
pub fn wasm_set_pending_training(
    game_json: &str,
    nation_id: u32,
    to_trained: u32,
    to_expert: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::units::set_pending_training(g, nation_id, to_trained, to_expert)
    })
}

#[wasm_bindgen]
pub fn wasm_set_pending_immigration(game_json: &str, nation_id: u32, count: u32) -> String {
    run_command(game_json, |g| {
        frontend_api::units::set_pending_immigration(g, nation_id, count)
    })
}

#[wasm_bindgen]
pub fn wasm_build_ship(game_json: &str, nation_id: u32, ship_type_str: &str) -> String {
    run_command(game_json, |g| {
        frontend_api::units::build_ship(g, nation_id, ship_type_str)
    })
}

#[wasm_bindgen]
pub fn wasm_cancel_ship_build(game_json: &str, nation_id: u32, ship_type_str: &str) -> String {
    run_command(game_json, |g| {
        frontend_api::units::cancel_ship_build(g, nation_id, ship_type_str)
    })
}

#[wasm_bindgen]
pub fn wasm_set_pending_ships(
    game_json: &str,
    nation_id: u32,
    ship_type_str: &str,
    count: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::units::set_pending_ships(g, nation_id, ship_type_str, count)
    })
}

#[wasm_bindgen]
pub fn wasm_assign_beachhead(game_json: &str, nation_id: u32, target_province_id: u32) -> String {
    run_command(game_json, |g| {
        frontend_api::units::assign_beachhead(g, nation_id, target_province_id)
    })
}

#[wasm_bindgen]
pub fn wasm_move_fleet(
    game_json: &str,
    nation_id: u32,
    from_zone_id: u32,
    to_zone_id: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::units::move_fleet(g, nation_id, from_zone_id, to_zone_id)
    })
}

#[wasm_bindgen]
pub fn wasm_cancel_fleet_move(game_json: &str, nation_id: u32, from_zone_id: u32) -> String {
    run_command(game_json, |g| {
        frontend_api::units::cancel_fleet_move(g, nation_id, from_zone_id)
    })
}

// ── Transport ────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub fn wasm_get_transport_data(game_json: &str, nation_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::transport::get_transport_data(g, nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_set_pending_freight_cars(game_json: &str, nation_id: u32, count: u32) -> String {
    run_command(game_json, |g| {
        frontend_api::transport::set_pending_freight_cars(g, nation_id, count)
    })
}

#[wasm_bindgen]
pub fn wasm_set_transport_allocation(
    game_json: &str,
    nation_id: u32,
    resource: &str,
    units: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::transport::set_transport_allocation(g, nation_id, resource, units)
    })
}

// ── Industry ─────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub fn wasm_get_industry_data(game_json: &str, nation_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::industry::get_industry_data(g, nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_set_chain_target(
    game_json: &str,
    nation_id: u32,
    chain: &str,
    step: &str,
    target: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::industry::set_chain_target(g, nation_id, chain, step, target)
    })
}

#[wasm_bindgen]
pub fn wasm_expand_building(game_json: &str, nation_id: u32, building_type: &str) -> String {
    run_command(game_json, |g| {
        frontend_api::industry::expand_building(g, nation_id, building_type)
    })
}

// ── Trade ────────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub fn wasm_get_trade_data(game_json: &str, nation_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::trade::get_trade_data(g, nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_set_auto_trade_with_minors(game_json: &str, nation_id: u32, enabled: bool) -> String {
    run_command(game_json, |g| {
        frontend_api::trade::set_auto_trade_with_minors(g, nation_id, enabled)
    })
}

#[wasm_bindgen]
pub fn wasm_set_trade_subsidy(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
    amount: i64,
) -> String {
    run_command(game_json, |g| {
        frontend_api::trade::set_trade_subsidy(g, nation_id, target_nation_id, amount)
    })
}

#[wasm_bindgen]
pub fn wasm_set_player_sell_order(
    game_json: &str,
    nation_id: u32,
    commodity_type: &str,
    commodity_name: &str,
    quantity: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::trade::set_player_sell_order(
            g,
            nation_id,
            commodity_type,
            commodity_name,
            quantity,
        )
    })
}

#[wasm_bindgen]
pub fn wasm_set_player_buy_order(
    game_json: &str,
    nation_id: u32,
    commodity_type: &str,
    commodity_name: &str,
    quantity: u32,
    max_price: i64,
) -> String {
    run_command(game_json, |g| {
        frontend_api::trade::set_player_buy_order(
            g,
            nation_id,
            commodity_type,
            commodity_name,
            quantity,
            max_price,
        )
    })
}

// ── Diplomacy ────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub fn wasm_get_diplomacy_screen_data(game_json: &str, nation_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::diplomacy::get_diplomacy_screen_data(g, nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_diplomacy_build_consulate(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::build_consulate(g, nation_id, target_nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_diplomacy_build_embassy(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::build_embassy(g, nation_id, target_nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_diplomacy_propose_nap(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::propose_nap(g, nation_id, target_nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_diplomacy_propose_alliance(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::propose_alliance(g, nation_id, target_nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_diplomacy_declare_war(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::declare_war(g, nation_id, target_nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_diplomacy_send_grant(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
    amount: i64,
) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::send_grant(g, nation_id, target_nation_id, amount)
    })
}

#[wasm_bindgen]
pub fn wasm_diplomacy_break_treaty(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
    treaty_type: &str,
) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::break_treaty(g, nation_id, target_nation_id, treaty_type)
    })
}

#[wasm_bindgen]
pub fn wasm_diplomacy_propose_peace(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::propose_peace(g, nation_id, target_nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_diplomacy_dismiss_outgoing_proposal(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::dismiss_outgoing_proposal(g, nation_id, target_nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_diplomacy_dismiss_pending_action(
    game_json: &str,
    nation_id: u32,
    target_nation_id: u32,
    action_key: &str,
) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::dismiss_pending_action(g, nation_id, target_nation_id, action_key)
    })
}

#[wasm_bindgen]
pub fn wasm_get_pending_proposals(game_json: &str, nation_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::diplomacy::get_pending_proposals(g, nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_accept_proposal(game_json: &str, nation_id: u32, proposal_index: u32) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::accept_proposal(g, nation_id, proposal_index)
    })
}

#[wasm_bindgen]
pub fn wasm_reject_proposal(game_json: &str, nation_id: u32, proposal_index: u32) -> String {
    run_command(game_json, |g| {
        frontend_api::diplomacy::reject_proposal(g, nation_id, proposal_index)
    })
}

// ── Ledger / newspaper / battles ─────────────────────────────────────────

#[wasm_bindgen]
pub fn wasm_get_ledger_data(game_json: &str, nation_id: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::ledger::get_ledger_data(g, nation_id)
    })
}

#[wasm_bindgen]
pub fn wasm_get_all_gp_ledger_data(game_json: &str) -> String {
    run_query(game_json, frontend_api::ledger::get_all_gp_ledger_data)
}

#[wasm_bindgen]
pub fn wasm_get_newspaper_archive(game_json: &str) -> String {
    run_query(game_json, frontend_api::newspaper::get_newspaper_archive)
}

#[wasm_bindgen]
pub fn wasm_get_newspaper_archive_since(game_json: &str, after_turn: u32) -> String {
    run_query(game_json, |g| {
        frontend_api::newspaper::get_newspaper_archive_since(g, after_turn)
    })
}

#[wasm_bindgen]
pub fn wasm_get_battle_data(game_json: &str) -> String {
    run_query(game_json, frontend_api::battles::get_battle_data)
}
