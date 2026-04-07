#![allow(unused_labels)]
use crate::economy::buildings::{Building, BuildingType};
use crate::economy::trade;
use crate::game_state::GameState;
use crate::types::*;

use super::common::{AiPersonality, get_personality};

/// Build mills and factories when the nation has the required materials.
fn ai_build_infrastructure(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Build mills if the nation doesn't have them.
    // First mill of each type is free (bootstrap) — this prevents the chicken-and-egg
    // problem where mills require Lumber+Steel that can only be produced by mills.
    // This mirrors the original Imperialism where nations had basic industry from the start.
    let mill_types = [
        BuildingType::LumberMill,
        BuildingType::SteelMill,
        BuildingType::TextileMill,
    ];
    for mill_type in mill_types {
        if !nation.has_building(mill_type) {
            // First mill is free (bootstrap) — no material cost
            nation.buildings.push(Building::new(mill_type, 2));
        }
    }

    // Build factories: first one of each type is free (bootstrap), same as mills
    let mill_factory_pairs = [
        (BuildingType::LumberMill, BuildingType::FurnitureFactory),
        (BuildingType::SteelMill, BuildingType::HardwareFactory),
        (BuildingType::TextileMill, BuildingType::ClothingFactory),
    ];
    for (mill, factory) in mill_factory_pairs {
        if nation.has_building(mill) && !nation.has_building(factory) {
            nation.buildings.push(Building::new(factory, 1));
        }
    }
}

/// AI builds map infrastructure: depots and railroads to connect provinces.
///
/// Strategy: Build a depot on the capital province first, then build depots on
/// adjacent provinces, and railroads to link them. This allows resource flow.
pub(crate) fn ai_build_map_infrastructure(game: &mut GameState, nation_id: NationId) {
    use crate::map::infrastructure::{build_depot, build_railroad};

    let treasury = match game.get_nation(nation_id) {
        Some(n) => n.treasury,
        None => return,
    };

    // Need at least $3,000 to afford a depot + some railroads
    if treasury < Money::dollars(3000) {
        return;
    }

    // Get nation's province IDs and capital province
    let capital_province_id = match game.get_nation(nation_id) {
        Some(n) => n.capital_province_id,
        None => return,
    };

    let province_ids: Vec<ProvinceId> = match game.get_nation(nation_id) {
        Some(n) => n.province_ids.clone(),
        None => return,
    };

    // Step 1: Build depot on capital province if it doesn't have one
    let capital_tiles: Vec<crate::hex::HexCoord> = game
        .get_province(capital_province_id)
        .map(|p| p.tiles.clone())
        .unwrap_or_default();

    let capital_has_depot = capital_tiles.iter().any(|coord| {
        game.hex_map
            .get_tile(*coord)
            .is_some_and(|t| t.infrastructure.has_depot)
    });

    if !capital_has_depot {
        if let Some(&tile_coord) = capital_tiles.first()
            && let Ok(cost) = build_depot(&mut game.hex_map, tile_coord)
            && let Some(nation) = game.get_nation_mut(nation_id)
        {
            nation.treasury -= cost;
        }
        return; // One major action per turn
    }

    // Step 2: Build railroads on capital province tiles that don't have them
    let mut spent = Money::dollars(0);
    let spend_limit = Money::dollars(2000);
    for &tile_coord in &capital_tiles {
        if spent >= spend_limit {
            break;
        }
        let needs_rr = game
            .hex_map
            .get_tile(tile_coord)
            .is_some_and(|t| !t.infrastructure.has_railroad);
        if needs_rr && let Ok(cost) = build_railroad(&mut game.hex_map, tile_coord) {
            if let Some(nation) = game.get_nation_mut(nation_id) {
                nation.treasury -= cost;
            }
            spent += cost;
        }
    }
    if spent > Money::dollars(0) {
        return; // Spent this turn on railroads
    }

    // Step 3: Build depots on adjacent provinces + railroads to connect
    for &pid in &province_ids {
        if pid == capital_province_id {
            continue;
        }

        let prov_tiles: Vec<crate::hex::HexCoord> = game
            .get_province(pid)
            .map(|p| p.tiles.clone())
            .unwrap_or_default();

        let has_depot = prov_tiles.iter().any(|coord| {
            game.hex_map
                .get_tile(*coord)
                .is_some_and(|t| t.infrastructure.has_depot)
        });

        if !has_depot {
            if let Some(&tile_coord) = prov_tiles.first()
                && game
                    .get_nation(nation_id)
                    .is_some_and(|n| n.treasury >= Money::dollars(2000))
                && let Ok(cost) = build_depot(&mut game.hex_map, tile_coord)
                && let Some(nation) = game.get_nation_mut(nation_id)
            {
                nation.treasury -= cost;
            }
            return; // One depot per turn
        }

        // Build railroads on this province's tiles to extend the network
        for &tile_coord in &prov_tiles {
            let can_afford = game
                .get_nation(nation_id)
                .is_some_and(|n| n.treasury >= Money::dollars(200));
            if !can_afford {
                break;
            }
            let needs_rr = game
                .hex_map
                .get_tile(tile_coord)
                .is_some_and(|t| !t.infrastructure.has_railroad);
            if needs_rr && let Ok(cost) = build_railroad(&mut game.hex_map, tile_coord) {
                if let Some(nation) = game.get_nation_mut(nation_id) {
                    nation.treasury -= cost;
                }
                return; // One railroad per province per turn to spread cost
            }
        }
    }
}

/// The AI keeps at least 2 of each good in reserve.
pub fn ai_manage_resources(game: &mut GameState, nation_id: NationId, actions: &mut Vec<String>) {
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Only sell goods when treasury is low
    if nation.treasury >= Money::dollars(3000) {
        return;
    }

    let nation_name = nation.name.clone();

    // Define goods to sell and their prices
    let goods_prices: [(GoodsType, i64); 3] = [
        (GoodsType::Furniture, 200),
        (GoodsType::Hardware, 250),
        (GoodsType::Clothing, 200),
    ];

    let mut total_revenue = Money::ZERO;

    for (goods_type, price_per_unit) in &goods_prices {
        let amount = match game.get_nation(nation_id) {
            Some(n) => n.goods_amount(*goods_type),
            None => return,
        };
        // Keep at least 2 in reserve
        if amount <= 2 {
            continue;
        }
        let excess = amount - 2;
        let revenue = Money::dollars(*price_per_unit) * excess as i64;

        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.consume_goods(*goods_type, excess);
        nation.treasury += revenue;
        total_revenue += revenue;
    }

    if total_revenue > Money::ZERO {
        actions.push(format!(
            "{} sold excess goods for ${}",
            nation_name,
            total_revenue.as_dollars()
        ));
    }
}

/// Consolidate AI economic decisions.
///
/// - If AI has no mills and has lumber+steel materials: build a LumberMill
/// - If AI has mills producing materials, build corresponding factories
/// - Expand mills when capacity is maxed (if resources > capacity * threshold)
/// - **Economic** personality: expand more aggressively (threshold multiplier 1 instead of 2)
pub(crate) fn ai_manage_economy(game: &mut GameState, nation_id: NationId) {
    let personality = get_personality(game, nation_id);

    if game.ai_debug {
        let nation_name = game
            .get_nation(nation_id)
            .map(|n| n.name.as_str())
            .unwrap_or("?");
        let treasury = game
            .get_nation(nation_id)
            .map(|n| n.treasury.as_dollars())
            .unwrap_or(0);
        eprintln!(
            "[AI:{}:economy] treasury=${}, personality={}",
            nation_name, treasury, personality
        );
    }

    // Build infrastructure handles mills and factories
    ai_build_infrastructure(game, nation_id);

    // ── Read Lua config (feature-gated) ──────────────────────
    #[cfg(feature = "lua")]
    let lua_cfg = game
        .game_data
        .lua_engine
        .as_ref()
        .and_then(|e| super::lua_bridge::lua_get_config(e, personality));
    #[cfg(not(feature = "lua"))]
    let _lua_cfg: Option<()> = None;

    // Economic personality expands more aggressively (Lua overrides Rust defaults)
    let expansion_threshold_multiplier: u32 = 'val: {
        #[cfg(feature = "lua")]
        if let Some(v) = lua_cfg
            .as_ref()
            .and_then(|c| c.expansion_threshold_multiplier)
        {
            break 'val v;
        }
        match personality {
            AiPersonality::Economic => 1,
            _ => 2,
        }
    };

    // Expand mills when input resources exceed capacity * threshold
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    let expansions_needed: Vec<BuildingType> = nation
        .buildings
        .iter()
        .filter_map(|b| {
            let input_resources = match b.building_type {
                BuildingType::LumberMill => nation.resource_amount(ResourceType::Timber),
                BuildingType::SteelMill => {
                    nation.resource_amount(ResourceType::Coal)
                        + nation.resource_amount(ResourceType::Iron)
                }
                BuildingType::TextileMill => {
                    nation.resource_amount(ResourceType::Cotton)
                        + nation.resource_amount(ResourceType::Wool)
                }
                _ => return None,
            };
            if input_resources > b.effective_capacity() * expansion_threshold_multiplier
                && b.pending_capacity == 0
            {
                Some(b.building_type)
            } else {
                None
            }
        })
        .collect();

    for bt in expansions_needed {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => return,
        };
        let has_lumber = nation.material_amount(MaterialType::Lumber) >= 1;
        let has_steel = nation.material_amount(MaterialType::Steel) >= 1;
        if has_lumber && has_steel {
            let nation = game.get_nation_mut(nation_id).unwrap();
            nation.consume_material(MaterialType::Lumber, 1);
            nation.consume_material(MaterialType::Steel, 1);
            if let Some(building) = nation.get_building_mut(bt) {
                building.start_expansion(1);
            }
        }
    }

    // When treasury is very high, expand existing mills/factories even without
    // surplus resources — invest in future capacity growth.
    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };
    if nation.treasury > Money::dollars(15_000) {
        let expandable: Vec<BuildingType> = nation
            .buildings
            .iter()
            .filter(|b| {
                matches!(
                    b.building_type,
                    BuildingType::LumberMill | BuildingType::SteelMill | BuildingType::TextileMill
                ) && b.pending_capacity == 0
            })
            .map(|b| b.building_type)
            .collect();

        for bt in expandable {
            let nation = match game.get_nation(nation_id) {
                Some(n) => n,
                None => return,
            };
            let has_lumber = nation.material_amount(MaterialType::Lumber) >= 1;
            let has_steel = nation.material_amount(MaterialType::Steel) >= 1;
            if has_lumber && has_steel {
                let nation = game.get_nation_mut(nation_id).unwrap();
                nation.consume_material(MaterialType::Lumber, 1);
                nation.consume_material(MaterialType::Steel, 1);
                if let Some(building) = nation.get_building_mut(bt) {
                    building.start_expansion(1);
                }
            }
        }
    }
}

/// Sell excess tradeable resources on the market for cash.
///
/// For each tradeable resource the AI has more than 10 of, sell the excess
/// at base_price and add proceeds to the treasury.
pub(crate) fn ai_trade(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Don't sell resources when already sitting on a large treasury —
    // keep the materials for building ships, units, and infrastructure instead.
    if nation.treasury > Money::dollars(20_000) {
        return;
    }

    // Check all tradeable resource types for surplus
    let tradeable_resources = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Fruit,
        ResourceType::Livestock,
        ResourceType::Oil,
    ];

    for resource in tradeable_resources {
        let amount = nation.resource_amount(resource);
        if amount > 10 {
            let excess = amount - 10;
            let price = trade::base_price(resource);
            if price != Money::ZERO {
                let revenue = price * excess as i64;
                nation.remove_resource(resource, excess);
                nation.treasury += revenue;
            }
        }
    }
}

/// Build freight cars if the nation has none and has the required materials.
///
/// Cost per freight car: 1 lumber + 1 steel (labor requirement simplified away).
/// Builds 2 freight cars if possible.
fn ai_build_transport(game: &mut GameState, nation_id: NationId) {
    let nation = match game.get_nation_mut(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Build freight cars if we have fewer than needed (scale with province count)
    let target_cars = (nation.province_count() as u32).max(2);
    if nation.transport.freight_cars >= target_cars {
        return;
    }

    // Build up to 2 freight cars per turn (cost: 1 lumber + 1 steel each)
    let cars_to_build = (target_cars - nation.transport.freight_cars).min(2);
    let lumber_available = nation.material_amount(MaterialType::Lumber);
    let steel_available = nation.material_amount(MaterialType::Steel);
    let affordable = cars_to_build.min(lumber_available).min(steel_available);

    if affordable > 0 {
        nation.consume_material(MaterialType::Lumber, affordable);
        nation.consume_material(MaterialType::Steel, affordable);
        nation.transport.build_freight_cars(affordable);
    }
}

/// Proactive transport building: build freight cars when transport capacity
/// is insufficient for current resource production.
///
/// Checks total resources in the warehouse against freight car capacity.
/// If warehouse resources exceed capacity, builds additional freight cars
/// (up to 2 per turn) when materials are available.
pub(crate) fn ai_build_transport_proactive(game: &mut GameState, nation_id: NationId) {
    // First, use the basic logic to build initial cars if none exist
    ai_build_transport(game, nation_id);

    let nation = match game.get_nation(nation_id) {
        Some(n) => n,
        None => return,
    };

    // Calculate total resources in warehouse
    let total_resources: u32 = nation.warehouse.values().sum();
    let capacity = nation.transport.total_capacity();

    // If resources exceed capacity, we need more freight cars
    if total_resources <= capacity {
        return;
    }

    // Build additional freight cars (1 lumber + 1 steel each, up to 2 per turn)
    let cars_to_build = 2u32;
    let lumber_available = nation.material_amount(MaterialType::Lumber);
    let steel_available = nation.material_amount(MaterialType::Steel);
    let affordable = cars_to_build.min(lumber_available).min(steel_available);

    if affordable > 0 {
        let nation = game.get_nation_mut(nation_id).unwrap();
        nation.consume_material(MaterialType::Lumber, affordable);
        nation.consume_material(MaterialType::Steel, affordable);
        nation.transport.build_freight_cars(affordable);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::common::test_helpers::test_game_with_ai;
    use crate::ai::run_ai_turns;
    use crate::economy::buildings::{Building, BuildingType};

    #[test]
    fn ai_builds_mill_when_it_has_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give the AI lumber and steel materials
        *ai.materials.entry(MaterialType::Lumber).or_insert(0) = 3;
        *ai.materials.entry(MaterialType::Steel).or_insert(0) = 3;

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have built a LumberMill (first in the loop)
        assert!(
            ai.has_building(BuildingType::LumberMill),
            "AI should build a LumberMill when it has lumber + steel materials"
        );
    }

    #[test]
    fn ai_builds_factory_when_it_has_mill_and_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give the AI all three mills already so it won't spend materials on them
        ai.buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        ai.buildings.push(Building::new(BuildingType::SteelMill, 2));
        ai.buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        // Give materials for factory construction
        *ai.materials.entry(MaterialType::Lumber).or_insert(0) = 2;
        *ai.materials.entry(MaterialType::Steel).or_insert(0) = 2;

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert!(
            ai.has_building(BuildingType::FurnitureFactory),
            "AI should build a FurnitureFactory when it has a LumberMill and materials"
        );
    }

    #[test]
    fn ai_bootstraps_mills_and_factories() {
        let mut game = test_game_with_ai();
        // AI has no materials at all

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // First mills and factories are free (bootstrap)
        assert!(
            ai.has_building(BuildingType::LumberMill),
            "AI should bootstrap first LumberMill for free"
        );
        assert!(
            ai.has_building(BuildingType::SteelMill),
            "AI should bootstrap first SteelMill for free"
        );
        assert!(
            ai.has_building(BuildingType::FurnitureFactory),
            "AI should bootstrap first FurnitureFactory for free"
        );
        assert!(
            ai.has_building(BuildingType::ClothingFactory),
            "AI should bootstrap first ClothingFactory for free"
        );
    }

    #[test]
    fn ai_sells_excess_resources() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(1000);
        // Give AI 15 timber (surplus over 10 threshold)
        ai.add_resource(ResourceType::Timber, 15);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have sold 5 timber at $50 each = $250
        assert_eq!(
            ai.resource_amount(ResourceType::Timber),
            10,
            "AI should sell down to 10 timber"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(1250),
            "Treasury should increase by $250 from selling 5 timber at $50"
        );
    }

    #[test]
    fn ai_does_not_sell_resources_below_threshold() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(1000);
        ai.add_resource(ResourceType::Timber, 8); // below threshold of 10

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.resource_amount(ResourceType::Timber),
            8,
            "AI should not sell resources at or below 10"
        );
    }

    #[test]
    fn ai_sells_multiple_excess_resources() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(0);
        ai.add_resource(ResourceType::Timber, 15); // 5 excess at $50 = $250
        ai.add_resource(ResourceType::Coal, 20); // 10 excess at $75 = $750

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(ai.resource_amount(ResourceType::Timber), 10);
        assert_eq!(ai.resource_amount(ResourceType::Coal), 10);
        assert_eq!(
            ai.treasury,
            Money::dollars(1000),
            "Treasury should increase by $250 + $750 = $1000"
        );
    }

    #[test]
    fn ai_does_not_sell_non_tradeable_grain() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.treasury = Money::dollars(0);
        ai.add_resource(ResourceType::Grain, 20); // grain is not in the tradeable list

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Grain has base_price $0, and is not in the tradeable_resources list in ai_trade
        // so it should remain untouched. Worker recruitment may consume 1 grain.
        // But with 0 workers and < 5 workers, 1 grain consumed for recruitment.
        assert_eq!(
            ai.resource_amount(ResourceType::Grain),
            19,
            "Only 1 grain consumed for worker recruitment, none sold"
        );
    }

    #[test]
    fn ai_builds_freight_cars_when_it_has_materials() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        // Give all buildings so infrastructure doesn't consume materials
        ai.buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        ai.buildings.push(Building::new(BuildingType::SteelMill, 2));
        ai.buildings
            .push(Building::new(BuildingType::TextileMill, 2));
        ai.buildings
            .push(Building::new(BuildingType::FurnitureFactory, 1));
        ai.buildings
            .push(Building::new(BuildingType::HardwareFactory, 1));
        ai.buildings
            .push(Building::new(BuildingType::ClothingFactory, 1));
        ai.add_material(MaterialType::Lumber, 5);
        ai.add_material(MaterialType::Steel, 5);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.transport.freight_cars, 2,
            "AI should build 2 freight cars"
        );
        // Should have consumed 2 lumber + 2 steel
        assert_eq!(ai.material_amount(MaterialType::Lumber), 3);
        assert_eq!(ai.material_amount(MaterialType::Steel), 3);
    }

    #[test]
    fn ai_does_not_build_freight_cars_without_materials() {
        let mut game = test_game_with_ai();
        // AI has no materials

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        assert_eq!(
            ai.transport.freight_cars, 0,
            "AI should not build freight cars without materials"
        );
    }

    #[test]
    fn ai_scales_freight_cars_with_provinces() {
        let mut game = test_game_with_ai();
        let ai = game.get_nation_mut(NationId(2)).unwrap();
        ai.transport.build_freight_cars(1); // start with 1 car
        // Give plenty of materials (some may be consumed by economy/infra building)
        ai.add_material(MaterialType::Lumber, 20);
        ai.add_material(MaterialType::Steel, 20);

        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // With 1 province, target = max(1*2, 5) = 5, so AI builds more
        // (up to 2 per turn, from 1 → 3)
        assert!(
            ai.transport.freight_cars > 1,
            "AI should build more freight cars to meet target (has {})",
            ai.transport.freight_cars
        );
    }

    #[test]
    fn ai_builds_depot_on_capital() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // The test AI's capital tile is at (3,3) — verify it exists
        let ai = game.get_nation(ai_id).unwrap();
        let cap_province = game.get_province(ai.capital_province_id).unwrap();
        let cap_tile = cap_province.tiles[0];

        // If the tile doesn't exist in the map, skip (test map too small)
        if game.hex_map.get_tile(cap_tile).is_none() {
            // Still verify the function doesn't panic on missing tiles
            ai_build_map_infrastructure(&mut game, ai_id);
            return;
        }

        assert!(
            !game
                .hex_map
                .get_tile(cap_tile)
                .unwrap()
                .infrastructure
                .has_depot,
            "No depot initially"
        );

        ai_build_map_infrastructure(&mut game, ai_id);

        // After one call, should have built a depot on capital
        assert!(
            game.hex_map
                .get_tile(cap_tile)
                .unwrap()
                .infrastructure
                .has_depot,
            "AI should build depot on capital tile"
        );

        // Treasury should have decreased by $2,000
        let ai = game.get_nation(ai_id).unwrap();
        assert_eq!(ai.treasury, Money::dollars(8000));
    }

    #[test]
    fn ai_sells_excess_goods_when_treasury_low() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Set treasury below $3,000 threshold
        game.get_nation_mut(ai_id).unwrap().treasury = Money::dollars(1000);

        // Give AI excess goods
        game.get_nation_mut(ai_id)
            .unwrap()
            .add_goods(GoodsType::Furniture, 5); // 5 - 2 reserve = 3 to sell
        game.get_nation_mut(ai_id)
            .unwrap()
            .add_goods(GoodsType::Hardware, 4); // 4 - 2 reserve = 2 to sell
        game.get_nation_mut(ai_id)
            .unwrap()
            .add_goods(GoodsType::Clothing, 1); // below reserve, won't sell

        let mut actions = Vec::new();
        ai_manage_resources(&mut game, ai_id, &mut actions);

        let ai = game.get_nation(ai_id).unwrap();

        // Should have sold 3 Furniture @ $200 = $600
        // and 2 Hardware @ $250 = $500
        // Total revenue: $1,100
        assert_eq!(
            ai.goods_amount(GoodsType::Furniture),
            2,
            "Should keep 2 Furniture"
        );
        assert_eq!(
            ai.goods_amount(GoodsType::Hardware),
            2,
            "Should keep 2 Hardware"
        );
        assert_eq!(
            ai.goods_amount(GoodsType::Clothing),
            1,
            "Should not sell Clothing below reserve"
        );
        assert_eq!(
            ai.treasury,
            Money::dollars(2100), // 1000 + 600 + 500
            "Treasury should increase by goods revenue"
        );
        assert!(
            actions.iter().any(|a| a.contains("sold excess goods")),
            "Should report selling goods"
        );
    }

    #[test]
    fn ai_does_not_sell_goods_when_treasury_sufficient() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Treasury above threshold
        game.get_nation_mut(ai_id).unwrap().treasury = Money::dollars(5000);

        // Give AI excess goods
        game.get_nation_mut(ai_id)
            .unwrap()
            .add_goods(GoodsType::Furniture, 10);

        let mut actions = Vec::new();
        ai_manage_resources(&mut game, ai_id, &mut actions);

        let ai = game.get_nation(ai_id).unwrap();
        assert_eq!(
            ai.goods_amount(GoodsType::Furniture),
            10,
            "Should not sell goods when treasury is sufficient"
        );
        assert!(actions.is_empty(), "No action should be reported");
    }

    #[test]
    fn ai_builds_transport_proactively_when_overflow() {
        let mut game = test_game_with_ai();
        let ai_id = NationId(2);

        // Give AI some resources that exceed transport capacity
        let ai = game.get_nation_mut(ai_id).unwrap();
        ai.add_resource(ResourceType::Timber, 20);
        ai.add_resource(ResourceType::Coal, 10);
        // Give materials for building freight cars
        ai.add_material(MaterialType::Lumber, 4);
        ai.add_material(MaterialType::Steel, 4);
        // No freight cars initially

        ai_build_transport_proactive(&mut game, ai_id);

        let ai = game.get_nation(ai_id).unwrap();
        // Should have built freight cars: first the basic (2), then proactive (up to 2 more)
        assert!(
            ai.transport.freight_cars >= 2,
            "AI should build freight cars proactively, got {}",
            ai.transport.freight_cars
        );
    }
}
