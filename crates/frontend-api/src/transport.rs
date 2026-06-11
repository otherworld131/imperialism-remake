//! Transport screen — freight cars, allocations, delivery projections.
//!
//! Verbatim moves from `crates/wasm-bridge/src/lib.rs` — bodies must stay
//! byte-identical to the originals (error JSON strings included).

use crate::ApiError;
use crate::parse::parse_freight_target;
use domain::economy::transport::TransportSystem;
use domain::game_state::GameState;
use domain::types::*;

/// Query transport data for a nation.
pub fn get_transport_data(game: &GameState, nation_id: u32) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };

    let transport = &nation.economy.transport;
    let (labor_cost, lumber_cost, steel_cost) = TransportSystem::build_freight_car_cost();
    let available_lumber = nation.material_amount(MaterialType::Lumber);
    let available_steel = nation.material_amount(MaterialType::Steel);
    let available_labor = nation.economy.labor.total_labor_units();

    let can_build = available_lumber >= lumber_cost
        && available_steel >= steel_cost
        && available_labor >= labor_cost;

    // Trello #484: capital-tile yields are no longer a free-delivery slot.
    // The UI's allocation panel reflects the unified pool used by
    // `resolve_transport` — local + remote folded into a single "available"
    // column that the player allocates against.
    let (local_items, remote_items) = domain::economy::current_collectable_resources(game, nid);
    let mut available_map: std::collections::BTreeMap<ResourceType, u32> =
        std::collections::BTreeMap::new();
    for (resource, qty) in local_items.iter().chain(remote_items.iter()) {
        *available_map.entry(*resource).or_insert(0) += *qty;
    }
    let combined_items: Vec<(ResourceType, u32)> = available_map.clone().into_iter().collect();
    let remote_available: Vec<(ResourceType, u32)> = combined_items.clone();

    // Project deliveries against the same combined pool used by turn processing
    // so the UI reflects what will actually be collected this turn.
    let merchant_cargo = nation.total_cargo_capacity(&game.game_data);
    let combined_transport = domain::economy::TransportSystem {
        freight_cars: transport.freight_cars + merchant_cargo,
        allocations: transport.allocations.clone(),
    };
    let has_positive_allocations = transport.allocations.iter().any(|(_, units)| *units > 0);
    let remote_deliveries = if has_positive_allocations {
        combined_transport.calculate_deliveries(&combined_items)
    } else {
        Vec::new()
    };
    let rail_only_deliveries = if has_positive_allocations {
        transport.calculate_deliveries(&combined_items)
    } else {
        Vec::new()
    };
    let mut delivered_map: std::collections::BTreeMap<ResourceType, u32> =
        std::collections::BTreeMap::new();
    for (resource, qty) in &remote_deliveries {
        *delivered_map.entry(*resource).or_insert(0) += *qty;
    }

    let (_local_town_outputs, remote_town_outputs) =
        domain::economy::project_town_outputs(game, nid);
    let freight_unused_after_raw = combined_transport
        .freight_cars
        .saturating_sub(delivered_map.values().copied().sum::<u32>());
    let (town_deliveries, _remaining_unused_after_towns) =
        domain::economy::allocate_town_output_freight(
            &mut delivered_map,
            &remote_town_outputs,
            &transport.allocations,
            freight_unused_after_raw,
        );

    let allocations_json: Vec<serde_json::Value> = transport
        .allocations
        .iter()
        .map(|(target, units)| {
            serde_json::json!({
                "resource": target.label(),
                "units": units,
            })
        })
        .collect();

    let deliveries_json: Vec<serde_json::Value> = remote_available
        .iter()
        .map(|(r, avail)| {
            let delivered = delivered_map.get(r).copied().unwrap_or(0);
            serde_json::json!({
                "resource": format!("{:?}", r),
                "available": avail,
                "delivered": delivered,
            })
        })
        .collect();

    let town_deliveries_json: Vec<serde_json::Value> = remote_town_outputs
        .iter()
        .map(|(stockpile, avail)| {
            let delivered = town_deliveries
                .iter()
                .find(|(delivered_stockpile, _)| delivered_stockpile == stockpile)
                .map(|(_, qty)| *qty)
                .unwrap_or(0);
            serde_json::json!({
                "resource": stockpile.label(),
                "available": avail,
                "delivered": delivered,
            })
        })
        .collect();

    let merchant_ship_count = nation.merchant_ship_count();
    let rail_only_deliveries_json: Vec<serde_json::Value> = remote_available
        .iter()
        .map(|(r, _avail)| {
            let delivered = rail_only_deliveries
                .iter()
                .find(|(dr, _)| *dr == *r)
                .map(|(_, qty)| *qty)
                .unwrap_or(0);
            serde_json::json!({
                "resource": format!("{:?}", r),
                "delivered": delivered,
            })
        })
        .collect();

    // Pre-turn demand forecast: delegated to domain so business logic stays in one place.
    let demand_forecast = domain::economy::compute_demand_forecast(nation, &game.game_data);

    // Build combined deliveries list: union of available resources and demanded resources.
    // Resources with demand but zero stock are included with available=0 so the UI can
    // render the demand indicator even when the player has none in warehouse (F-013).
    let mut deliveries_with_demand = deliveries_json;
    deliveries_with_demand.extend(town_deliveries_json.clone());
    for (r, _qty) in &demand_forecast {
        let already_present = remote_available.iter().any(|(ar, _)| ar == r);
        if !already_present {
            deliveries_with_demand.push(serde_json::json!({
                "resource": format!("{:?}", r),
                "available": 0,
                "delivered": 0,
            }));
        }
    }
    for (r, units) in &transport.allocations {
        if *units == 0
            || deliveries_with_demand
                .iter()
                .any(|entry| entry["resource"] == r.label())
        {
            continue;
        }
        deliveries_with_demand.push(serde_json::json!({
            "resource": r.label(),
            "available": 0,
            "delivered": 0,
        }));
    }

    let demand_json: Vec<serde_json::Value> = demand_forecast
        .into_iter()
        .map(|(r, qty)| {
            serde_json::json!({
                "resource": format!("{:?}", r),
                "demand": qty,
            })
        })
        .collect();

    // Worker food requirement (population × Imperialism-1 ratio). Same gate as
    // compute_demand_forecast so it disappears cleanly if food_per_worker is
    // disabled in the game config.
    let total_workers = nation.economy.labor.total_workers();
    let food_requirement_json =
        if total_workers > 0 && game.game_data.game_config.food_per_worker > 0 {
            let (grain_need, fruit_need, meat_need) =
                domain::economy::labor::worker_food_demand(total_workers);
            serde_json::json!({
                "workers": total_workers,
                "grain": grain_need,
                "fruit": fruit_need,
                "meat": meat_need,
            })
        } else {
            serde_json::Value::Null
        };

    Ok(serde_json::json!({
        "freight_cars": transport.freight_cars,
        "total_capacity": transport.total_capacity(),
        "military_transport_capacity": transport.military_transport_capacity(),
        "merchant_marine_cargo": merchant_cargo,
        "merchant_ship_count": merchant_ship_count,
        "remote_delivery_capacity": transport.total_capacity() + merchant_cargo,
        "allocations": allocations_json,
        "build_cost": {
            "labor": labor_cost,
            "lumber": lumber_cost,
            "steel": steel_cost,
        },
        "can_build": can_build,
        "available_lumber": available_lumber,
        "available_steel": available_steel,
        "available_labor": available_labor,
        "deliveries": deliveries_with_demand,
        "town_deliveries": town_deliveries_json,
        "rail_only_deliveries": rail_only_deliveries_json,
        "demand": demand_json,
        "food_requirement": food_requirement_json,
    }))
}

/// Set the number of freight cars to build at end of turn.
pub fn set_pending_freight_cars(
    game: &mut GameState,
    nation_id: u32,
    count: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let Some(nation) = game.get_nation_mut(nid) else {
        return Err(ApiError::raw("{\"error\":\"nation not found\"}"));
    };
    nation.economy.pending_freight_cars = count;
    Ok(())
}

/// Set transport allocation for a freight target (resource/material/good).
pub fn set_transport_allocation(
    game: &mut GameState,
    nation_id: u32,
    resource: &str,
    units: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target = match parse_freight_target(resource) {
        Some(t) => t,
        None => return Err(ApiError::raw("{\"error\":\"unknown freight target\"}")),
    };

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };

    nation.economy.transport.set_allocation(target, units);
    Ok(())
}
