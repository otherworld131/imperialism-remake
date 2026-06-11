//! Industry screen: production data query and chain-target / building
//! expansion commands. Bodies moved verbatim from `wasm-bridge`.

use crate::ApiError;
use crate::parse::parse_building_type;
use domain::economy::buildings::BuildingType;
use domain::economy::production::{
    ProductionChain, calculate_armory_production, calculate_canned_food_production,
    calculate_factory_production, calculate_mill_production, calculate_paper_production,
};
use domain::game_state::GameState;
use domain::military::units::ArmyUnitType;
use domain::types::*;

/// Industry-panel buildings: expandable production sites whose tier
/// progression actually changes what they output. Fixed-capacity
/// infrastructure (Armory, Capitol, FoodProcessing, Railyard, Shipyard,
/// TradeSchool, University, Warehouse) is excluded — they don't gain
/// throughput from being "expanded" and shouldn't carry an Expand button.
fn is_expandable_industry_building(bt: BuildingType) -> bool {
    matches!(
        bt,
        BuildingType::LumberMill
            | BuildingType::SteelMill
            | BuildingType::TextileMill
            | BuildingType::FurnitureFactory
            | BuildingType::HardwareFactory
            | BuildingType::ClothingFactory
            | BuildingType::PaperFactory
            | BuildingType::OilRefinery
            | BuildingType::PowerPlant
            | BuildingType::AdvancedTextileMill
            | BuildingType::ChemicalPlant
    )
}

/// Query industry/production data for a nation.
pub fn get_industry_data(game: &GameState, nation_id: u32) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };

    // Buildings — Industry panel only lists the expandable production
    // buildings (mills, factories, late-game plants). Fixed-capacity
    // infrastructure (Armory, Capitol, FoodProcessing, Railyard, Shipyard,
    // TradeSchool, University, Warehouse) is omitted because the UI's
    // Expand button has no meaningful effect on what they produce.
    let buildings_json: Vec<serde_json::Value> = nation
        .economy
        .buildings
        .iter()
        .filter(|b| is_expandable_industry_building(b.building_type))
        .map(|b| {
            let next_cap = b.next_capacity();
            let (exp_lumber, exp_steel) =
                domain::economy::buildings::Building::expansion_cost(next_cap - b.capacity);
            serde_json::json!({
                "type": format!("{:?}", b.building_type),
                "display_name": format!("{}", b.building_type),
                "capacity": b.capacity,
                "next_capacity": next_cap,
                "is_expanding": b.turns_until_upgrade > 0,
                "turns_remaining": b.turns_until_upgrade,
                "pending_capacity": b.pending_capacity,
                "expansion_cost": { "lumber": exp_lumber, "steel": exp_steel },
            })
        })
        .collect();

    // Warehouse
    let resources_json: serde_json::Value = nation
        .economy
        .warehouse
        .iter()
        .map(|(r, qty)| (format!("{:?}", r), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    let materials_json: serde_json::Value = nation
        .economy
        .materials
        .iter()
        .map(|(m, qty)| (format!("{:?}", m), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    let goods_json: serde_json::Value = nation
        .economy
        .goods
        .iter()
        .map(|(g, qty)| (format!("{:?}", g), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    // Warehouse stock targets — the AI's per-commodity aim (buy-side
    // stockpile target for resources, sell-side reserve for materials and
    // goods). Surfaced for the debug overlay in the Industry panel.
    let wh_targets = domain::ai::warehouse_targets::compute_warehouse_targets(game, nid);
    let warehouse_target_resources_json: serde_json::Value = wh_targets
        .resources
        .iter()
        .map(|(r, qty)| (format!("{:?}", r), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();
    let warehouse_target_materials_json: serde_json::Value = wh_targets
        .materials
        .iter()
        .map(|(m, qty)| (format!("{:?}", m), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();
    let warehouse_target_goods_json: serde_json::Value = wh_targets
        .goods
        .iter()
        .map(|(g, qty)| (format!("{:?}", g), serde_json::json!(qty)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    // Labor
    let labor = &nation.economy.labor;

    // Production forecast for each chain
    let available_lumber_mat = nation.material_amount(MaterialType::Lumber);
    let available_steel_mat = nation.material_amount(MaterialType::Steel);

    // Can-expand map — matches the filtered Industry buildings list.
    let can_expand: serde_json::Value = nation
        .economy
        .buildings
        .iter()
        .filter(|b| is_expandable_industry_building(b.building_type))
        .map(|b| {
            let next_cap = b.next_capacity();
            let (exp_lumber, exp_steel) =
                domain::economy::buildings::Building::expansion_cost(next_cap - b.capacity);
            let expandable = b.turns_until_upgrade == 0
                && available_lumber_mat >= exp_lumber
                && available_steel_mat >= exp_steel;
            (
                format!("{:?}", b.building_type),
                serde_json::json!(expandable),
            )
        })
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    // Building capacities for production forecast
    let lumber_mill_cap = nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::LumberMill)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let steel_mill_cap = nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::SteelMill)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let textile_mill_cap = nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::TextileMill)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let furniture_cap = nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::FurnitureFactory)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let hardware_cap = nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::HardwareFactory)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let clothing_cap = nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::ClothingFactory)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let armory_cap = nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::Armory)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let paper_cap = nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::PaperFactory)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let canned_food_cap = nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == BuildingType::FoodProcessing)
        .map(|b| b.capacity)
        .unwrap_or(0);

    let labor_units = labor.total_labor_units();
    let targets = &nation.economy.chain_targets;

    // Compute labor budgets using the same Hamilton allocator as process_turn.
    let labor_budgets = domain::turn::economy_phase::allocate_labor(
        labor_units,
        targets,
        domain::turn::economy_phase::BuildingCapacities {
            timber: lumber_mill_cap,
            metal: steel_mill_cap,
            textile: textile_mill_cap,
            furniture: furniture_cap,
            hardware: hardware_cap,
            clothing: clothing_cap,
            armory: armory_cap,
            paper: paper_cap,
            canned_food: canned_food_cap,
        },
    );

    // Resources committed to each mill (capped by output targets).
    let all_res: Vec<(ResourceType, u32)> = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Grain,
        ResourceType::Fruit,
        ResourceType::Fish,
        ResourceType::Livestock,
    ]
    .iter()
    .map(|&r| (r, nation.resource_amount(r)))
    .collect();

    let fed_res = domain::turn::economy_phase::apply_feed_to_resources(&all_res, targets);

    // Max-feed resources (unlimited target = full warehouse), for computing max outputs.
    let max_res = all_res.clone();
    // Unlimited labor (resource-bound max) and all-labor-to-one-step (labor-bound max).
    let unlimited_labor = labor_units * 2 + 1;

    let timber_mill = calculate_mill_production(
        ProductionChain::Timber,
        &fed_res,
        lumber_mill_cap,
        labor_budgets.timber_mill,
    );
    let metal_mill = calculate_mill_production(
        ProductionChain::Metal,
        &fed_res,
        steel_mill_cap,
        labor_budgets.metal_mill,
    );
    let textile_mill = calculate_mill_production(
        ProductionChain::Textile,
        &fed_res,
        textile_mill_cap,
        labor_budgets.textile_mill,
    );

    // Resource-bound max (unlimited labor, 100% feed): shows capacity/resource ceiling.
    let timber_res_max = calculate_mill_production(
        ProductionChain::Timber,
        &max_res,
        lumber_mill_cap,
        unlimited_labor,
    );
    let metal_res_max = calculate_mill_production(
        ProductionChain::Metal,
        &max_res,
        steel_mill_cap,
        unlimited_labor,
    );
    let textile_res_max = calculate_mill_production(
        ProductionChain::Textile,
        &max_res,
        textile_mill_cap,
        unlimited_labor,
    );
    // Labor-bound max (all available labor to this step, 100% feed): shows labor ceiling.
    let timber_labor_max = calculate_mill_production(
        ProductionChain::Timber,
        &max_res,
        lumber_mill_cap,
        labor_units,
    );
    let metal_labor_max = calculate_mill_production(
        ProductionChain::Metal,
        &max_res,
        steel_mill_cap,
        labor_units,
    );
    let textile_labor_max = calculate_mill_production(
        ProductionChain::Textile,
        &max_res,
        textile_mill_cap,
        labor_units,
    );

    // Combine warehouse materials + this turn's mill output for factory inputs.
    let mat_lumber = nation.material_amount(MaterialType::Lumber)
        + timber_mill
            .materials_produced
            .first()
            .map(|x| x.1)
            .unwrap_or(0);
    let mat_steel = nation.material_amount(MaterialType::Steel)
        + metal_mill
            .materials_produced
            .first()
            .map(|x| x.1)
            .unwrap_or(0);
    let mat_fabric = nation.material_amount(MaterialType::Fabric)
        + textile_mill
            .materials_produced
            .first()
            .map(|x| x.1)
            .unwrap_or(0);

    let available_mats: Vec<(MaterialType, u32)> = vec![
        (MaterialType::Lumber, mat_lumber),
        (MaterialType::Steel, mat_steel),
        (MaterialType::Fabric, mat_fabric),
    ];
    let fed_mats = domain::turn::economy_phase::apply_feed_to_materials(&available_mats, targets);

    let furniture_prod = calculate_factory_production(
        ProductionChain::Timber,
        &fed_mats,
        furniture_cap,
        labor_budgets.lumber_factory,
    );
    let hardware_prod = calculate_factory_production(
        ProductionChain::Metal,
        &fed_mats,
        hardware_cap,
        labor_budgets.steel_factory,
    );
    let clothing_prod = calculate_factory_production(
        ProductionChain::Textile,
        &fed_mats,
        clothing_cap,
        labor_budgets.garment_factory,
    );

    let steel_consumed_by_hardware = hardware_prod
        .materials_consumed
        .iter()
        .find(|(m, _)| *m == MaterialType::Steel)
        .map(|(_, q)| *q)
        .unwrap_or(0);
    let steel_for_armory = mat_steel
        .saturating_sub(steel_consumed_by_hardware)
        .min(targets.armory);
    let armory_cfg = &game.game_data.game_config;
    let armory_prod = calculate_armory_production(
        steel_for_armory,
        armory_cap,
        labor_budgets.armory,
        armory_cfg.armory_steel_per_arm,
        armory_cfg.armory_labor_per_arm,
    );
    // armory_max uses total available steel (ignoring hardware's share) so the
    // slider cap reflects what the armory *could* produce at full allocation,
    // even when hardware is also configured. This lets the player see the
    // trade-off and set a non-zero armory target.
    let armory_max = calculate_armory_production(
        mat_steel,
        armory_cap,
        labor_units,
        armory_cfg.armory_steel_per_arm,
        armory_cfg.armory_labor_per_arm,
    );

    // Paper chain (Lumber → Paper): uses current lumber in warehouse + this turn's mill output.
    let lumber_for_paper = nation.material_amount(MaterialType::Lumber)
        + timber_mill
            .materials_produced
            .first()
            .map(|x| x.1)
            .unwrap_or(0);
    let paper_lumber_slice: Vec<(domain::types::MaterialType, u32)> =
        vec![(MaterialType::Lumber, lumber_for_paper)];
    let paper_prod =
        calculate_paper_production(&paper_lumber_slice, paper_cap, labor_budgets.paper_factory);
    let paper_max = calculate_paper_production(&paper_lumber_slice, paper_cap, labor_units);
    let paper_committed_lumber = paper_prod
        .materials_consumed
        .iter()
        .find(|(m, _)| *m == MaterialType::Lumber)
        .map(|(_, q)| *q)
        .unwrap_or(0);

    // Cannery: 1 Grain + 1 Fruit + 1 (Fish OR Livestock) → 1 CannedFood.
    let canned_prod = calculate_canned_food_production(
        &fed_res,
        canned_food_cap,
        labor_budgets.canned_food_factory,
    );
    let canned_res_max =
        calculate_canned_food_production(&max_res, canned_food_cap, unlimited_labor);
    let canned_labor_max = calculate_canned_food_production(&max_res, canned_food_cap, labor_units);
    let canned_committed_grain = canned_prod
        .resources_consumed
        .iter()
        .find(|(r, _)| *r == ResourceType::Grain)
        .map(|(_, q)| *q)
        .unwrap_or(0);
    let canned_committed_fruit = canned_prod
        .resources_consumed
        .iter()
        .find(|(r, _)| *r == ResourceType::Fruit)
        .map(|(_, q)| *q)
        .unwrap_or(0);
    let canned_committed_fish = canned_prod
        .resources_consumed
        .iter()
        .find(|(r, _)| *r == ResourceType::Fish)
        .map(|(_, q)| *q)
        .unwrap_or(0);
    let canned_committed_livestock = canned_prod
        .resources_consumed
        .iter()
        .find(|(r, _)| *r == ResourceType::Livestock)
        .map(|(_, q)| *q)
        .unwrap_or(0);

    // Max materials for factory max: warehouse + max mill output at 100% feed.
    let max_mat_lumber = nation.material_amount(MaterialType::Lumber)
        + timber_res_max
            .materials_produced
            .first()
            .map(|x| x.1)
            .unwrap_or(0);
    let max_mat_steel = nation.material_amount(MaterialType::Steel)
        + metal_res_max
            .materials_produced
            .first()
            .map(|x| x.1)
            .unwrap_or(0);
    let max_mat_fabric = nation.material_amount(MaterialType::Fabric)
        + textile_res_max
            .materials_produced
            .first()
            .map(|x| x.1)
            .unwrap_or(0);
    let max_mats: Vec<(MaterialType, u32)> = [
        (MaterialType::Lumber, max_mat_lumber),
        (MaterialType::Steel, max_mat_steel),
        (MaterialType::Fabric, max_mat_fabric),
    ]
    .to_vec();

    let furniture_res_max = calculate_factory_production(
        ProductionChain::Timber,
        &max_mats,
        furniture_cap,
        unlimited_labor,
    );
    let hardware_res_max = calculate_factory_production(
        ProductionChain::Metal,
        &max_mats,
        hardware_cap,
        unlimited_labor,
    );
    let clothing_res_max = calculate_factory_production(
        ProductionChain::Textile,
        &max_mats,
        clothing_cap,
        unlimited_labor,
    );
    let furniture_labor_max = calculate_factory_production(
        ProductionChain::Timber,
        &max_mats,
        furniture_cap,
        labor_units,
    );
    let hardware_labor_max =
        calculate_factory_production(ProductionChain::Metal, &max_mats, hardware_cap, labor_units);
    let clothing_labor_max = calculate_factory_production(
        ProductionChain::Textile,
        &max_mats,
        clothing_cap,
        labor_units,
    );

    let freight_car_cost = game.game_data.game_config.freight_car_cost;

    let committed_expert_civilian = nation.economy.pending_civilian_hires.values().sum::<u32>();
    let committed_untrained_training = nation.economy.pending_train_to_trained;
    let committed_trained_training = nation.economy.pending_train_to_expert;
    let cfg = &game.game_data.game_config;
    let max_pending_immigration = domain::turn::projected_immigration_queue_capacity(game, nid);

    // Committed resources from pending army recruits
    let mut army_committed_arms = 0u32;
    let mut army_committed_horses = 0u32;
    let mut army_committed_untrained = 0u32;
    let mut army_committed_trained = 0u32;
    let mut army_committed_expert = 0u32;
    for unit_str in &nation.economy.pending_army_recruits {
        if let Ok(ut) = unit_str.parse::<ArmyUnitType>() {
            let s = ut.stats();
            army_committed_arms += s.arms_required;
            if s.requires_horse {
                army_committed_horses += 1;
            }
            match s.recruit_tier {
                domain::economy::labor::WorkerType::Untrained => army_committed_untrained += 1,
                domain::economy::labor::WorkerType::Trained => army_committed_trained += 1,
                domain::economy::labor::WorkerType::Expert => army_committed_expert += 1,
            }
        }
    }

    let (fc_labor, fc_lumber, fc_steel) =
        domain::economy::transport::TransportSystem::build_freight_car_cost();
    let committed_expert = committed_expert_civilian + army_committed_expert;
    let committed_untrained = committed_untrained_training + army_committed_untrained;
    let committed_trained = committed_trained_training + army_committed_trained;
    let committed_freight_labor = nation.economy.pending_freight_cars.saturating_mul(fc_labor);
    let committed_labor_units = committed_untrained * cfg.untrained_labor
        + committed_trained * cfg.trained_labor
        + committed_expert * cfg.expert_labor
        + committed_freight_labor;

    let max_fc = if fc_lumber > 0 && fc_steel > 0 && fc_labor > 0 {
        (nation.material_amount(MaterialType::Lumber) / fc_lumber)
            .min(nation.material_amount(MaterialType::Steel) / fc_steel)
            .min(labor_units / fc_labor)
    } else {
        0
    };

    Ok(serde_json::json!({
        "buildings": buildings_json,
        "freight_car_cost": freight_car_cost,
        "pending_freight_cars": nation.economy.pending_freight_cars,
        "max_freight_cars": max_fc,
        "warehouse": {
            "resources": resources_json,
            "materials": materials_json,
            "goods": goods_json,
        },
        "warehouse_targets": {
            "resources": warehouse_target_resources_json,
            "materials": warehouse_target_materials_json,
            "goods": warehouse_target_goods_json,
        },
        "labor": {
            "untrained": labor.untrained,
            "trained": labor.trained,
            "expert": labor.expert,
            "total_workers": labor.total_workers(),
            "total_labor_units": labor.total_labor_units(),
            "committed_expert": committed_expert,
            "committed_untrained": committed_untrained,
            "committed_trained": committed_trained,
            "committed_labor_units": committed_labor_units,
        },
        "chain_targets": {
            "timber_mill": targets.timber_mill,
            "metal_mill": targets.metal_mill,
            "textile_mill": targets.textile_mill,
            "lumber_factory": targets.lumber_factory,
            "steel_factory": targets.steel_factory,
            "garment_factory": targets.garment_factory,
            "armory": targets.armory,
            "paper_factory": targets.paper_factory,
            "canned_food_factory": targets.canned_food_factory,
        },
        "production_forecast": {
            "timber_chain": {
                "mill_target": targets.timber_mill,
                "mill_cap": lumber_mill_cap,
                "mill_output": timber_mill.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "mill_labor": timber_mill.labor_used,
                "mill_max_output": timber_res_max.materials_produced.first().map(|x| x.1).unwrap_or(0).min(timber_labor_max.materials_produced.first().map(|x| x.1).unwrap_or(0)),
                "mill_committed_timber": timber_mill.resources_consumed.iter().find(|(r,_)| *r == ResourceType::Timber).map(|x| x.1).unwrap_or(0),
                "factory_target": targets.lumber_factory,
                "factory_cap": furniture_cap,
                "factory_output": furniture_prod.goods_produced.first().map(|x| x.1).unwrap_or(0),
                "factory_labor": furniture_prod.labor_used,
                "factory_max_output": furniture_res_max.goods_produced.first().map(|x| x.1).unwrap_or(0).min(furniture_labor_max.goods_produced.first().map(|x| x.1).unwrap_or(0)),
                "factory_committed_lumber": furniture_prod.materials_consumed.iter().find(|(m,_)| *m == MaterialType::Lumber).map(|(_,q)| *q).unwrap_or(0),
            },
            "metal_chain": {
                "mill_target": targets.metal_mill,
                "mill_cap": steel_mill_cap,
                "mill_output": metal_mill.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "mill_labor": metal_mill.labor_used,
                "mill_max_output": metal_res_max.materials_produced.first().map(|x| x.1).unwrap_or(0).min(metal_labor_max.materials_produced.first().map(|x| x.1).unwrap_or(0)),
                "mill_committed_coal": metal_mill.resources_consumed.iter().find(|(r,_)| *r == ResourceType::Coal).map(|x| x.1).unwrap_or(0),
                "mill_committed_iron": metal_mill.resources_consumed.iter().find(|(r,_)| *r == ResourceType::Iron).map(|x| x.1).unwrap_or(0),
                "factory_target": targets.steel_factory,
                "factory_cap": hardware_cap,
                "factory_output": hardware_prod.goods_produced.first().map(|x| x.1).unwrap_or(0),
                "factory_labor": hardware_prod.labor_used,
                "factory_max_output": hardware_res_max.goods_produced.first().map(|x| x.1).unwrap_or(0).min(hardware_labor_max.goods_produced.first().map(|x| x.1).unwrap_or(0)),
                "factory_committed_steel": hardware_prod.materials_consumed.iter().find(|(m,_)| *m == MaterialType::Steel).map(|(_,q)| *q).unwrap_or(0),
            },
            "textile_chain": {
                "mill_target": targets.textile_mill,
                "mill_cap": textile_mill_cap,
                "mill_output": textile_mill.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "mill_labor": textile_mill.labor_used,
                "mill_max_output": textile_res_max.materials_produced.first().map(|x| x.1).unwrap_or(0).min(textile_labor_max.materials_produced.first().map(|x| x.1).unwrap_or(0)),
                "mill_committed_cotton": textile_mill.resources_consumed.iter().find(|(r,_)| *r == ResourceType::Cotton).map(|x| x.1).unwrap_or(0),
                "mill_committed_wool": textile_mill.resources_consumed.iter().find(|(r,_)| *r == ResourceType::Wool).map(|x| x.1).unwrap_or(0),
                "factory_target": targets.garment_factory,
                "factory_cap": clothing_cap,
                "factory_output": clothing_prod.goods_produced.first().map(|x| x.1).unwrap_or(0),
                "factory_labor": clothing_prod.labor_used,
                "factory_max_output": clothing_res_max.goods_produced.first().map(|x| x.1).unwrap_or(0).min(clothing_labor_max.goods_produced.first().map(|x| x.1).unwrap_or(0)),
                "factory_committed_fabric": clothing_prod.materials_consumed.iter().find(|(m,_)| *m == MaterialType::Fabric).map(|(_,q)| *q).unwrap_or(0),
            },
            "arms_chain": {
                "armory_cap": armory_cap,
                "armory_target": targets.armory,
                "armory_output": armory_prod.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "armory_labor": armory_prod.labor_used,
                "armory_max_output": armory_max.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "armory_committed_steel": steel_for_armory,
            },
            "paper_chain": {
                "factory_cap": paper_cap,
                "factory_target": targets.paper_factory,
                "factory_output": paper_prod.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "factory_labor": paper_prod.labor_used,
                "factory_max_output": paper_max.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "factory_committed_lumber": paper_committed_lumber,
            },
            "food_chain": {
                "factory_cap": canned_food_cap,
                "factory_target": targets.canned_food_factory,
                "factory_output": canned_prod.materials_produced.first().map(|x| x.1).unwrap_or(0),
                "factory_labor": canned_prod.labor_used,
                "factory_max_output": canned_res_max.materials_produced.first().map(|x| x.1).unwrap_or(0).min(canned_labor_max.materials_produced.first().map(|x| x.1).unwrap_or(0)),
                "factory_committed_grain": canned_committed_grain,
                "factory_committed_fruit": canned_committed_fruit,
                "factory_committed_fish": canned_committed_fish,
                "factory_committed_livestock": canned_committed_livestock,
            },
        },
        "pending_ships": nation.economy.pending_ships,
        "pending_army_recruits": nation.economy.pending_army_recruits,
        "army_committed_arms": army_committed_arms,
        "army_committed_horses": army_committed_horses,
        "auto_trade_with_minors": nation.economy.auto_trade_with_minors,
        "can_expand": can_expand,
        "pending_civilian_hires": nation.economy.pending_civilian_hires
            .iter()
            .map(|(k, v)| (format!("{}", k), serde_json::json!(v)))
            .collect::<serde_json::Map<String, serde_json::Value>>(),
        "pending_immigration": nation.economy.pending_immigration,
        "max_pending_immigration": max_pending_immigration,
        "pending_training": {
            "to_trained": nation.economy.pending_train_to_trained,
            "to_expert": nation.economy.pending_train_to_expert,
        },
        "immigration_costs": {
            "canned_food": cfg.immigration_canned_food,
            "clothing": cfg.immigration_clothing,
        },
        "training_costs": {
            "to_trained_paper": game.game_data.game_config.train_to_trained_paper_cost,
            "to_trained_labor": game.game_data.game_config.train_to_trained_labor_cost,
            "to_expert_paper": game.game_data.game_config.train_to_expert_paper_cost,
            "to_expert_labor": game.game_data.game_config.train_to_expert_labor_cost,
        },
    }))
}

/// Set the output target (units) for a production chain step.
/// Pass u32::MAX (4294967295) for "unlimited" (use all available inputs).
pub fn set_chain_target(
    game: &mut GameState,
    nation_id: u32,
    chain: &str,
    step: &str,
    target: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    match (chain, step) {
        ("timber", "mill") => nation.economy.chain_targets.timber_mill = target,
        ("timber", "factory") => nation.economy.chain_targets.lumber_factory = target,
        ("timber", "paper") => nation.economy.chain_targets.paper_factory = target,
        ("metal", "mill") => nation.economy.chain_targets.metal_mill = target,
        ("metal", "factory") => nation.economy.chain_targets.steel_factory = target,
        ("textile", "mill") => nation.economy.chain_targets.textile_mill = target,
        ("textile", "factory") => nation.economy.chain_targets.garment_factory = target,
        ("arms", "armory") => nation.economy.chain_targets.armory = target,
        ("food", "factory") => nation.economy.chain_targets.canned_food_factory = target,
        _ => return Err(ApiError::raw("{\"error\":\"unknown chain/step\"}")),
    }
    Ok(())
}

/// Expand a building to its next capacity tier.
pub fn expand_building(
    game: &mut GameState,
    nation_id: u32,
    building_type: &str,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let bt = match parse_building_type(building_type) {
        Some(b) => b,
        None => return Err(ApiError::raw("{\"error\":\"unknown building type\"}")),
    };

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };

    let building = match nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == bt)
    {
        Some(b) => b,
        None => return Err(ApiError::raw("{\"error\":\"building not found\"}")),
    };

    if building.turns_until_upgrade > 0 {
        return Err(ApiError::raw(
            "{\"error\":\"building is already expanding\"}",
        ));
    }

    let next_cap = building.next_capacity();
    let amount = next_cap - building.capacity;
    let (exp_lumber, exp_steel) = domain::economy::buildings::Building::expansion_cost(amount);

    if nation.material_amount(MaterialType::Lumber) < exp_lumber {
        return Err(ApiError::raw("{\"error\":\"not enough lumber\"}"));
    }
    if nation.material_amount(MaterialType::Steel) < exp_steel {
        return Err(ApiError::raw("{\"error\":\"not enough steel\"}"));
    }

    nation.consume_material(MaterialType::Lumber, exp_lumber);
    nation.consume_material(MaterialType::Steel, exp_steel);

    // Find the building again mutably and start expansion
    if let Some(b) = nation
        .economy
        .buildings
        .iter_mut()
        .find(|b| b.building_type == bt)
    {
        b.start_expansion(amount);
    }

    Ok(())
}
