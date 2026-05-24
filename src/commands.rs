use std::io::{self, Write};

use domain::economy::buildings::{Building, BuildingType};
use domain::economy::civilians::{BuildTask, CivilianType, parse_civilian_type};
use domain::game_state::GameState;
use domain::hex::HexCoord;
use domain::map::infrastructure;
use domain::military::ships::{Ship, ShipType};
use domain::military::units::{ArmyUnit, ArmyUnitType};
use domain::nation::Nation;
use domain::types::*;

// ── Helper functions ─────────────────────────────────────────────

fn human_player(game: &GameState) -> Option<&Nation> {
    match game.get_nation(game.human_player_nation) {
        Some(player) => Some(player),
        None => {
            println!("  Internal error: human player nation is missing from game state.");
            None
        }
    }
}

fn human_player_mut(game: &mut GameState) -> Option<&mut Nation> {
    let player_id = game.human_player_nation;
    match game.get_nation_mut(player_id) {
        Some(player) => Some(player),
        None => {
            println!("  Internal error: human player nation is missing from game state.");
            None
        }
    }
}

/// Check if the human player's nation is bankrupt. If so, print a message and return true.
pub(crate) fn check_bankrupt(game: &GameState) -> bool {
    let Some(player) = human_player(game) else {
        return true;
    };
    if player.is_bankrupt() {
        println!(
            "  FINANCIAL CRISIS: Your nation is bankrupt (treasury: {}). No spending allowed until treasury recovers.",
            player.economy.treasury
        );
        true
    } else {
        false
    }
}

/// Parse a building name string into a BuildingType.
/// Only mills and factories can be built by the player.
pub(crate) fn parse_buildable(name: &str) -> Option<BuildingType> {
    match name.to_lowercase().as_str() {
        "lumbermill" | "lumber mill" | "lumber_mill" => Some(BuildingType::LumberMill),
        "steelmill" | "steel mill" | "steel_mill" => Some(BuildingType::SteelMill),
        "textilemill" | "textile mill" | "textile_mill" => Some(BuildingType::TextileMill),
        "furniturefactory" | "furniture factory" | "furniture_factory" => {
            Some(BuildingType::FurnitureFactory)
        }
        "hardwarefactory" | "hardware factory" | "hardware_factory" => {
            Some(BuildingType::HardwareFactory)
        }
        "clothingfactory" | "clothing factory" | "clothing_factory" => {
            Some(BuildingType::ClothingFactory)
        }
        _ => None,
    }
}

/// Parse a unit type name string.
pub(crate) fn parse_unit_type(name: &str) -> Option<ArmyUnitType> {
    match name.to_lowercase().as_str() {
        "militia" => Some(ArmyUnitType::Minutemen),
        "regulars" => Some(ArmyUnitType::Regulars),
        "grenadiers" => Some(ArmyUnitType::Grenadiers),
        "cuirassiers" => Some(ArmyUnitType::Cuirassiers),
        "light artillery" | "lightartillery" | "light_artillery" => {
            Some(ArmyUnitType::LightArtillery)
        }
        _ => None,
    }
}

/// Simplified money cost for supported unit types.
pub(crate) fn unit_build_cost(unit_type: ArmyUnitType) -> Money {
    match unit_type {
        ArmyUnitType::Regulars => Money::dollars(500),
        ArmyUnitType::Grenadiers => Money::dollars(1000),
        ArmyUnitType::Cuirassiers => Money::dollars(500),
        ArmyUnitType::LightArtillery => Money::dollars(2000),
        _ => Money::dollars(0),
    }
}

/// Parse a resource type from a string.
pub(crate) fn parse_resource_type(s: &str) -> Option<ResourceType> {
    match s.to_lowercase().as_str() {
        "timber" => Some(ResourceType::Timber),
        "coal" => Some(ResourceType::Coal),
        "iron" => Some(ResourceType::Iron),
        "cotton" => Some(ResourceType::Cotton),
        "wool" => Some(ResourceType::Wool),
        "fruit" => Some(ResourceType::Fruit),
        "livestock" => Some(ResourceType::Livestock),
        "oil" => Some(ResourceType::Oil),
        "gold" => Some(ResourceType::Gold),
        "gems" => Some(ResourceType::Gems),
        "grain" => Some(ResourceType::Grain),
        "horses" => Some(ResourceType::Horses),
        _ => None,
    }
}

pub(crate) fn read_line() -> Option<String> {
    let mut input = String::new();
    io::stdout().flush().ok();
    match io::stdin().read_line(&mut input) {
        Ok(0) => None,
        Ok(_) => Some(input),
        Err(_) => None,
    }
}

// ── Command handlers ─────────────────────────────────────────────

pub(crate) fn build_building(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let bt = match parse_buildable(query) {
        Some(bt) => bt,
        None => {
            println!("  Unknown building: '{}'", query);
            println!(
                "  Available: lumbermill, steelmill, textilemill, furniturefactory, hardwarefactory, clothingfactory"
            );
            return;
        }
    };

    let Some(player) = human_player(game) else {
        return;
    };

    // Check if the player already has this building
    if player.has_building(bt) {
        println!(
            "  You already have a {:?}. Use 'expand {:?}' to increase its capacity.",
            bt, bt
        );
        return;
    }

    // Cost: 1 lumber + 1 steel per capacity unit; initial capacity is 2
    let initial_capacity: u32 = 2;
    let (lumber_needed, steel_needed) = Building::expansion_cost(initial_capacity);

    let lumber_have = player.material_amount(MaterialType::Lumber);
    let steel_have = player.material_amount(MaterialType::Steel);

    if lumber_have < lumber_needed || steel_have < steel_needed {
        println!(
            "  Insufficient materials to build {:?} (need {} lumber, {} steel; have {} lumber, {} steel).",
            bt, lumber_needed, steel_needed, lumber_have, steel_have
        );
        return;
    }

    let Some(player) = human_player_mut(game) else {
        return;
    };
    player.consume_material(MaterialType::Lumber, lumber_needed);
    player.consume_material(MaterialType::Steel, steel_needed);
    player
        .economy
        .buildings
        .push(Building::new(bt, initial_capacity));

    println!(
        "  Built {:?} with capacity {} (consumed {} lumber, {} steel).",
        bt, initial_capacity, lumber_needed, steel_needed
    );
}

pub(crate) fn expand_building(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let bt = match parse_buildable(query) {
        Some(bt) => bt,
        None => {
            println!("  Unknown building: '{}'", query);
            println!(
                "  Available: lumbermill, steelmill, textilemill, furniturefactory, hardwarefactory, clothingfactory"
            );
            return;
        }
    };

    let Some(player) = human_player(game) else {
        return;
    };

    if !player.has_building(bt) {
        println!(
            "  You don't have a {:?} yet. Use 'build {:?}' first.",
            bt, bt
        );
        return;
    }

    // Calculate the expansion amount using capacity progression (2 -> 4 -> 8 -> 12 -> 16 -> ...)
    let current_capacity = player
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == bt)
        .map(|b| b.capacity)
        .unwrap_or(0);
    let next = match current_capacity {
        c if c < 2 => 2,
        2 => 4,
        4 => 8,
        8 => 12,
        12 => 16,
        16 => 20,
        _ => current_capacity + 4,
    };
    let expand_amount = next - current_capacity;

    // Cost: 1 lumber + 1 steel per new capacity unit
    let (lumber_needed, steel_needed) = Building::expansion_cost(expand_amount);

    let lumber_have = player.material_amount(MaterialType::Lumber);
    let steel_have = player.material_amount(MaterialType::Steel);

    if lumber_have < lumber_needed || steel_have < steel_needed {
        println!(
            "  Insufficient materials to expand {:?} (need {} lumber, {} steel; have {} lumber, {} steel).",
            bt, lumber_needed, steel_needed, lumber_have, steel_have
        );
        return;
    }

    let Some(player) = human_player_mut(game) else {
        return;
    };
    player.consume_material(MaterialType::Lumber, lumber_needed);
    player.consume_material(MaterialType::Steel, steel_needed);

    let Some(building) = player.get_building_mut(bt) else {
        println!(
            "  Internal error: {:?} vanished before expansion could start.",
            bt
        );
        return;
    };
    building.start_expansion(expand_amount);

    println!(
        "  Expanding {:?} from {} to {} capacity (consumed {} lumber, {} steel). Will be ready in 2 turns.",
        bt, current_capacity, next, lumber_needed, steel_needed
    );
}

pub(crate) fn recruit_worker(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let Some(player) = human_player(game) else {
        return;
    };

    // Check province-based recruitment limit
    let max_recruits = crate::display::max_recruitment_capacity(player);
    let current_workers = player.economy.labor.total_workers();
    if current_workers >= max_recruits {
        println!(
            "  Cannot recruit: at capacity ({}/{} workers for {} provinces).",
            current_workers,
            max_recruits,
            player.province_count()
        );
        println!("  Conquer more provinces or expand your Capitol to increase capacity.");
        return;
    }

    let canned_food = player.material_amount(MaterialType::CannedFood);
    let clothing = player.goods_amount(GoodsType::Clothing);
    let furniture = player.goods_amount(GoodsType::Furniture);

    let mut missing = Vec::new();
    if canned_food < 1 {
        missing.push(format!("1 canned food (have {})", canned_food));
    }
    if clothing < 1 {
        missing.push(format!("1 clothing (have {})", clothing));
    }
    if furniture < 1 {
        missing.push(format!("1 furniture (have {})", furniture));
    }

    if !missing.is_empty() {
        println!("  Cannot recruit: missing {}", missing.join(", "));
        return;
    }

    let Some(player) = human_player_mut(game) else {
        return;
    };
    player.consume_material(MaterialType::CannedFood, 1);
    player.consume_goods(GoodsType::Clothing, 1);
    player.consume_goods(GoodsType::Furniture, 1);
    player.economy.labor.recruit_immigrant();

    let max_recruits = crate::display::max_recruitment_capacity(player);
    println!(
        "  Recruited 1 untrained worker (now: {} untrained, {} trained, {} expert, capacity {}).",
        player.economy.labor.untrained,
        player.economy.labor.trained,
        player.economy.labor.expert,
        max_recruits
    );
}

pub(crate) fn train_worker(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let Some(player) = human_player(game) else {
        return;
    };

    if player.economy.labor.untrained == 0 {
        println!("  No untrained workers available to train.");
        return;
    }

    // Simplified: consume 1 paper if available, but allow training regardless
    let has_paper = player.material_amount(MaterialType::Paper) >= 1;

    let Some(player) = human_player_mut(game) else {
        return;
    };
    if has_paper {
        player.consume_material(MaterialType::Paper, 1);
    }
    player.economy.labor.train_worker();

    let paper_note = if has_paper {
        " (consumed 1 paper)"
    } else {
        " (no paper available, training proceeds anyway)"
    };
    println!(
        "  Trained 1 worker{} (now: {} untrained, {} trained, {} expert).",
        paper_note,
        player.economy.labor.untrained,
        player.economy.labor.trained,
        player.economy.labor.expert
    );
}

pub(crate) fn build_unit(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let unit_type = match parse_unit_type(query) {
        Some(ut) => ut,
        None => {
            println!("  Unknown unit type: '{}'", query);
            println!(
                "  Available: regulars ($500), grenadiers ($1000), cuirassiers ($500), light artillery ($2000)"
            );
            return;
        }
    };

    let cost = unit_build_cost(unit_type);
    let player_id = game.human_player_nation;
    let Some(player) = human_player(game) else {
        return;
    };

    if player.economy.treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford {:?} (cost: {}, treasury: {}).",
            unit_type, cost, player.economy.treasury
        );
        return;
    }

    let capital_province = player.capital_province_id;
    let uid = game.alloc_unit_id();
    let unit = ArmyUnit::new(uid, unit_type, player_id, capital_province);

    let Some(player) = human_player_mut(game) else {
        return;
    };
    player.economy.treasury -= cost;
    player.military.army.push(unit);

    println!(
        "  Unit built! {:?} stationed at capital (cost: {}, treasury now: {}). Army size: {}",
        unit_type,
        cost,
        player.economy.treasury,
        player.military.army.len()
    );
}

/// Upgrade a unit at the given army index.
/// Costs $500. Preserves medals and health.
pub(crate) fn cmd_upgrade_unit(game: &mut GameState, index_str: &str) {
    if check_bankrupt(game) {
        return;
    }

    let idx: usize = match index_str.parse() {
        Ok(i) => i,
        Err(_) => {
            println!("  Usage: upgrade <unit_index> (e.g. upgrade 0)");
            return;
        }
    };

    let Some(player) = human_player(game) else {
        return;
    };

    if idx >= player.military.army.len() {
        println!(
            "  Invalid unit index {}. You have {} units (0-{}).",
            idx,
            player.military.army.len(),
            player.military.army.len().saturating_sub(1)
        );
        return;
    }

    let current_type = player.military.army[idx].unit_type;
    let target_type = match current_type.upgrade_to() {
        Some(t) => t,
        None => {
            println!("  {:?} has no available upgrade path.", current_type);
            return;
        }
    };

    // Check if prerequisite tech is researched
    if let Some(required_tech_name) = target_type.required_tech() {
        let has_tech = player.researched_techs.iter().any(|tid| {
            game.game_data
                .tech_tree
                .get(*tid)
                .map(|t| t.name == required_tech_name)
                .unwrap_or(false)
        });
        if !has_tech {
            println!(
                "  Cannot upgrade {:?} to {:?}: requires '{}' technology.",
                current_type, target_type, required_tech_name
            );
            return;
        }
    }

    // Check cost ($500 flat)
    let cost = Money::dollars(500);
    if player.economy.treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford upgrade (cost: {}, treasury: {}).",
            cost, player.economy.treasury
        );
        return;
    }

    // Apply upgrade
    let Some(player) = human_player_mut(game) else {
        return;
    };
    player.economy.treasury -= cost;
    let old_medals = player.military.army[idx].medals;
    let old_health = player.military.army[idx].health;
    player.military.army[idx].unit_type = target_type;
    player.military.army[idx].movement_remaining = target_type.stats().movement;
    // Preserve medals and health
    player.military.army[idx].medals = old_medals;
    player.military.army[idx].health = old_health;

    println!(
        "  Upgraded {:?} -> {:?} (medals: {}, health: {}, cost: {}, treasury: {}).",
        current_type, target_type, old_medals, old_health, cost, player.economy.treasury
    );
}

/// Build a merchant ship (trader or indiaman).
pub(crate) fn cmd_build_ship(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let ship_type = match query.to_lowercase().as_str() {
        "trader" => ShipType::Trader,
        "indiaman" => ShipType::Indiaman,
        _ => {
            println!("  Unknown ship type: '{}'", query);
            println!(
                "  Available: trader (2 fabric + 4 lumber, cargo: 2), indiaman (3 fabric + 7 lumber, cargo: 4)"
            );
            return;
        }
    };

    let stats = game.game_data.ship_stats(ship_type).clone();
    let fabric_needed = stats.fabric_cost;
    let lumber_needed = stats.lumber_cost;

    let Some(player) = human_player(game) else {
        return;
    };

    let fabric_have = player.material_amount(MaterialType::Fabric);
    let lumber_have = player.material_amount(MaterialType::Lumber);

    if fabric_have < fabric_needed || lumber_have < lumber_needed {
        println!(
            "  Insufficient materials to build {:?} (need {} fabric + {} lumber; have {} fabric + {} lumber).",
            ship_type, fabric_needed, lumber_needed, fabric_have, lumber_have
        );
        return;
    }

    let player_id = game.human_player_nation;
    let uid = game.alloc_unit_id();
    let ship = Ship::with_data(uid, ship_type, player_id, &game.game_data);

    {
        let Some(player) = human_player_mut(game) else {
            return;
        };
        player.consume_material(MaterialType::Fabric, fabric_needed);
        player.consume_material(MaterialType::Lumber, lumber_needed);
        player.military.merchant_fleet.push(ship);
    }

    let player = match human_player(game) {
        Some(p) => p,
        None => return,
    };
    println!(
        "  Ship built! {:?} added to merchant fleet (cargo: {}). Fleet size: {}, total cargo: {}",
        ship_type,
        stats.cargo,
        player.military.merchant_fleet.len(),
        player.total_cargo_capacity(&game.game_data),
    );
}

/// Build a warship (frigate or ship-of-the-line).
pub(crate) fn cmd_build_warship(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let ship_type = match query.to_lowercase().as_str() {
        "frigate" => ShipType::Frigate,
        "ship-of-the-line" | "ship of the line" | "shipoftheline" => ShipType::ShipOfTheLine,
        _ => {
            println!("  Unknown warship type: '{}'", query);
            println!(
                "  Available: frigate (2 fabric + 5 lumber + 2 arms), ship-of-the-line (3 fabric + 8 lumber + 5 arms)"
            );
            return;
        }
    };

    let stats = game.game_data.ship_stats(ship_type).clone();
    let fabric_needed = stats.fabric_cost;
    let lumber_needed = stats.lumber_cost;
    let arms_needed = stats.arms_cost;

    let Some(player) = human_player(game) else {
        return;
    };

    let fabric_have = player.material_amount(MaterialType::Fabric);
    let lumber_have = player.material_amount(MaterialType::Lumber);
    let arms_have = player.goods_amount(GoodsType::Arms);

    if fabric_have < fabric_needed || lumber_have < lumber_needed || arms_have < arms_needed {
        println!(
            "  Insufficient materials to build {:?} (need {} fabric + {} lumber + {} arms; have {} fabric + {} lumber + {} arms).",
            ship_type,
            fabric_needed,
            lumber_needed,
            arms_needed,
            fabric_have,
            lumber_have,
            arms_have
        );
        return;
    }

    let player_id = game.human_player_nation;
    let uid = game.alloc_unit_id();
    let ship = Ship::with_data(uid, ship_type, player_id, &game.game_data);

    {
        let Some(player) = human_player_mut(game) else {
            return;
        };
        player.consume_material(MaterialType::Fabric, fabric_needed);
        player.consume_material(MaterialType::Lumber, lumber_needed);
        player.consume_goods(GoodsType::Arms, arms_needed);
        player.military.warships.push(ship);
    }

    let player = match human_player(game) {
        Some(p) => p,
        None => return,
    };
    println!(
        "  Warship built! {:?} added to navy (FP: {}, Armor: {}, Hull: {}). Navy size: {}, total naval firepower: {}",
        ship_type,
        stats.firepower,
        stats.armor,
        stats.hull,
        player.military.warships.len(),
        player.total_naval_firepower(&game.game_data),
    );
}

/// Produce arms: convert 1 steel into 1 arms.
pub(crate) fn cmd_produce_arms(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let Some(player) = human_player(game) else {
        return;
    };

    let steel_have = player.material_amount(MaterialType::Steel);
    if steel_have < 1 {
        println!("  Cannot produce arms: need 1 steel (have {}).", steel_have);
        return;
    }

    let Some(player) = human_player_mut(game) else {
        return;
    };
    player.consume_material(MaterialType::Steel, 1);
    player.add_goods(GoodsType::Arms, 1);

    println!(
        "  Produced 1 arms from 1 steel. Arms: {}, Steel: {}",
        player.goods_amount(GoodsType::Arms),
        player.material_amount(MaterialType::Steel)
    );
}

/// Show blockade status against a target nation.
pub(crate) fn cmd_blockade(game: &GameState, query: &str) {
    let player_id = game.human_player_nation;
    let player = match game.get_nation(player_id) {
        Some(n) => n,
        None => return,
    };

    // Find the target nation by name
    let target = game
        .world
        .nations
        .iter()
        .find(|n| n.name.to_lowercase().contains(&query.to_lowercase()) && n.id != player_id);

    let target = match target {
        Some(t) => t,
        None => {
            println!(
                "  Unknown nation: '{}'. Use 'nations' to see all nations.",
                query
            );
            return;
        }
    };

    // Check if at war
    let at_war = game
        .world
        .diplomacy
        .get_relation(player_id, target.id)
        .is_some_and(|r| r.at_war);

    if !at_war {
        println!(
            "  You are not at war with {}. Blockade requires being at war.",
            target.name
        );
        return;
    }

    let our_warships = player.warship_count();
    if our_warships == 0 {
        println!("  You have no warships. Build warships first with 'build warship <type>'.");
        return;
    }

    let enemy_cargo = target.total_cargo_capacity(&game.game_data);
    let blocked = domain::military::calculate_blockade_effect(enemy_cargo, our_warships as u32);
    let cargo_blocked = enemy_cargo.saturating_sub(blocked);

    println!("  NAVAL BLOCKADE vs {}:", target.name);
    println!("    Your warships: {}", our_warships);
    println!(
        "    Your naval firepower: {}",
        player.total_naval_firepower(&game.game_data)
    );
    println!("    Enemy merchant cargo: {} holds", enemy_cargo);
    println!(
        "    Cargo blocked: {} (each warship blocks 2 holds)",
        cargo_blocked
    );
    println!("    Enemy effective cargo: {} holds", blocked);
    println!();
    println!("  Note: Blockade is applied automatically each turn while at war.");
    println!(
        "  Enemy warships ({}) will engage yours in naval combat.",
        target.warship_count()
    );
}

/// Sell resources at base price, adding revenue to treasury.
pub(crate) fn cmd_sell(game: &mut GameState, args: &str) {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() != 2 {
        println!("  Usage: sell <resource> <quantity>");
        println!("  Example: sell timber 5");
        return;
    }

    let resource = match parse_resource_type(parts[0]) {
        Some(r) => r,
        None => {
            println!("  Unknown resource: '{}'", parts[0]);
            println!(
                "  Tradeable: timber, coal, iron, cotton, wool, fruit, livestock, oil, gold, gems"
            );
            return;
        }
    };

    let quantity: u32 = match parts[1].parse() {
        Ok(q) if q > 0 => q,
        _ => {
            println!("  Quantity must be a positive number.");
            return;
        }
    };

    if !resource.is_tradeable() {
        println!("  {:?} is not tradeable.", resource);
        return;
    }

    let price = game
        .world
        .market_state
        .current_price(domain::economy::trade::Commodity::Resource(resource));
    if price == Money::ZERO {
        println!("  {:?} has no trade value.", resource);
        return;
    }

    let Some(player) = human_player(game) else {
        return;
    };

    if player.resource_amount(resource) < quantity {
        println!(
            "  Insufficient {:?} (have {}, want to sell {}).",
            resource,
            player.resource_amount(resource),
            quantity
        );
        return;
    }

    let revenue = price * quantity as i64;

    let Some(player) = human_player_mut(game) else {
        return;
    };
    player.remove_resource(resource, quantity);
    player.economy.treasury += revenue;

    println!(
        "  Sold {} {:?} at {} each for {} total. Treasury: {}",
        quantity, resource, price, revenue, player.economy.treasury
    );
}

pub(crate) fn research_tech(game: &mut GameState, query: &str) {
    let year = game.turn.year();

    let query_lower = query.to_lowercase();

    // Get available techs and find a case-insensitive partial match
    let Some(player) = human_player(game) else {
        return;
    };
    let available = game
        .game_data
        .tech_tree
        .available_techs(&player.researched_techs, year);

    let matched: Vec<_> = available
        .iter()
        .filter(|t| t.name.to_lowercase().contains(&query_lower))
        .collect();

    match matched.len() {
        0 => {
            println!("  No available technology matches '{}'.", query);
            println!("  Use 'tech' to see what is available.");
        }
        1 => {
            let tech = matched[0];
            let Some(player) = human_player(game) else {
                return;
            };

            // Check if player can afford it
            if player.economy.treasury.checked_sub(tech.cost).is_none() {
                println!(
                    "  Cannot afford {} (cost: {}, treasury: {}).",
                    tech.name, tech.cost, player.economy.treasury
                );
                return;
            }

            let tech_id = tech.id;
            let tech_name = tech.name.clone();
            let tech_cost = tech.cost;

            // Deduct cost and add tech
            let Some(player) = human_player_mut(game) else {
                return;
            };
            player.economy.treasury -= tech_cost;
            player.research_tech(tech_id);

            println!(
                "  {}",
                crate::display::color_green(&format!("Researched: {}!", tech_name))
            );
            println!(
                "  Cost: {} (treasury now: {})",
                tech_cost, player.economy.treasury
            );

            // Record history event (deduplicate: skip if same event already exists for this turn)
            let turn = game.turn;
            let entry = domain::events::HistoryEvent::TechnologyResearched {
                researcher: game.human_player_nation,
                tech_name: tech_name.clone(),
            };
            if !game
                .archive
                .history
                .iter()
                .any(|(t, ev)| *t == turn && *ev == entry)
            {
                game.archive.history.push((turn, entry));
            }
        }
        _ => {
            println!(
                "  Multiple technologies match '{}'. Be more specific:",
                query
            );
            for tech in &matched {
                println!("    - {} (cost: {})", tech.name, tech.cost);
            }
        }
    }
}

pub(crate) fn cmd_hire_civilian(game: &mut GameState, type_name: &str) {
    if check_bankrupt(game) {
        return;
    }

    let civ_type = match parse_civilian_type(type_name) {
        Some(ct) => ct,
        None => {
            println!("  Unknown civilian type: '{}'", type_name);
            println!("  Available: prospector ($100), miner ($1500), engineer ($500),");
            println!(
                "             farmer ($100), rancher ($100), forester ($100), driller ($2000)"
            );
            return;
        }
    };

    let cost = civ_type.creation_cost(&game.game_data.game_config);
    let player_id = game.human_player_nation;
    let civilian_costs_expert = game.game_data.game_config.civilian_costs_expert;
    let Some(player) = human_player(game) else {
        return;
    };

    if !civ_type.is_unlocked(
        &player.researched_techs,
        &game.game_data,
        &game.game_data.game_config,
    ) {
        let req = civ_type
            .required_tech(&game.game_data.game_config)
            .unwrap_or("required technology");
        println!(
            "  Cannot hire {}: requires technology '{}' (not yet researched).",
            civ_type, req
        );
        return;
    }

    if civilian_costs_expert && player.economy.labor.expert == 0 {
        println!("  Cannot hire civilian: requires an expert worker (you have none).");
        println!("  Train workers at the Trade School first.");
        return;
    }

    if player.economy.treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford {} (cost: {}, treasury: {}).",
            civ_type, cost, player.economy.treasury
        );
        return;
    }

    let id = game.alloc_unit_id();
    let civilian = domain::economy::civilians::Civilian::new(id, civ_type, player_id);

    let Some(player) = human_player_mut(game) else {
        return;
    };
    player.economy.treasury -= cost;
    if civilian_costs_expert {
        player.economy.labor.expert -= 1;
    }
    player.military.civilians.push(civilian);

    println!(
        "  Hired {} (cost: {}, treasury now: {}). Use 'deploy <index> <province>' to deploy.",
        civ_type, cost, player.economy.treasury
    );
    if civilian_costs_expert {
        println!(
            "  (Lost 1 expert worker — {} expert workers remain)",
            player.economy.labor.expert
        );
    }
}

pub(crate) fn cmd_deploy_civilian(game: &mut GameState, args: &str) {
    // Parse: "<index> <province_name>"
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        println!("  Usage: deploy <civilian_index> <province_name>");
        println!("  Example: deploy 0 France City");
        return;
    }
    let index: usize = match parts[0].parse() {
        Ok(i) => i,
        Err(_) => {
            println!(
                "  Invalid index: '{}'. Use 'civilians' to see indices.",
                parts[0]
            );
            return;
        }
    };
    let province_name = parts[1].trim();

    let player_id = game.human_player_nation;

    // Phase 1: gather info with immutable borrows, extract owned data
    let (civ_type, coord, prov_name) = {
        let Some(player) = human_player(game) else {
            return;
        };

        if index >= player.military.civilians.len() {
            println!(
                "  Invalid index {}. You have {} civilians.",
                index,
                player.military.civilians.len()
            );
            return;
        }

        let civ_type = player.military.civilians[index].civilian_type;

        // Find the province by name (case-insensitive partial match)
        let lower_name = province_name.to_lowercase();
        let matching_provinces: Vec<_> = game
            .world
            .provinces
            .iter()
            .filter(|p| p.owner == player_id && p.name.to_lowercase().contains(&lower_name))
            .collect();

        let province = match matching_provinces.len() {
            0 => {
                println!("  No owned province matches '{}'.", province_name);
                return;
            }
            1 => matching_provinces[0],
            _ => {
                println!(
                    "  Multiple provinces match '{}'. Be more specific.",
                    province_name
                );
                return;
            }
        };

        let prov_name = province.name.clone();

        // Find the first workable tile in the province for this civilian type
        let mut target_coord = None;
        for tile_coord in &province.tiles {
            if let Some(tile) = game.world.hex_map.get_tile(*tile_coord)
                && civ_type.can_improve(tile.terrain(), tile.resource_deposit())
                && tile.assigned_civilian.is_none()
            {
                // Prospectors target tiles with no resource yet (unprospected)
                // Engineers target any land tile (infrastructure)
                // Other civilians need a resource that can still be improved
                let eligible = match civ_type {
                    domain::economy::civilians::CivilianType::Prospector => {
                        tile.terrain().can_have_deposits() && tile.resource_deposit().is_none()
                    }
                    domain::economy::civilians::CivilianType::Engineer => true,
                    _ => tile
                        .resource_deposit()
                        .map(|r| tile.improvement_level() < r.max_improvement_level())
                        .unwrap_or(false),
                };
                if eligible {
                    target_coord = Some(*tile_coord);
                    break;
                }
            }
        }

        let coord = match target_coord {
            Some(c) => c,
            None => {
                println!(
                    "  No improvable tile for {} in province '{}'.",
                    civ_type, prov_name
                );
                return;
            }
        };

        (civ_type, coord, prov_name)
    };

    // Phase 2: apply mutations
    let Some(player) = human_player_mut(game) else {
        return;
    };
    let civilian = &mut player.military.civilians[index];
    civilian.deploy(coord);
    civilian.start_work(1); // 1 turn for improvements
    let civ_id = civilian.id;

    // Assign civilian to tile
    if let Some(tile) = game.world.hex_map.get_tile_mut(coord) {
        tile.assigned_civilian = Some(civ_id);
    }

    println!(
        "  Deployed {} to ({}, {}) in province '{}'. Work will complete next turn.",
        civ_type, coord.q, coord.r, prov_name
    );
}

pub(crate) fn cmd_consulate(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if target.is_great_power() {
        println!(
            "  {} is a Great Power. Consulates are for Minor Nations only.",
            target.name
        );
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    let Some(player) = human_player(game) else {
        return;
    };
    let cost = Money::dollars(500);
    if player.economy.treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford consulate (cost: {}, treasury: {}).",
            cost, player.economy.treasury
        );
        return;
    }

    match game.queue_direct_diplomacy_action(
        domain::game_state::PendingDiplomacyAction::BuildConsulate {
            player: player_id,
            target: target_id,
        },
    ) {
        Ok(_) => {
            println!(
                "  {}",
                crate::display::color_green(&format!(
                    "Trade consulate queued with {}. It will take effect at end of turn (cost: {}).",
                    target_name, cost
                ))
            );
        }
        Err(e) => {
            println!("  Cannot build consulate: {}", e);
        }
    }
}

pub(crate) fn cmd_embassy(game: &mut GameState, query: &str) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if target.is_great_power() {
        println!(
            "  {} is a Great Power. Use diplomacy with Great Powers through treaties.",
            target.name
        );
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    let Some(player) = human_player(game) else {
        return;
    };
    let cost = Money::dollars(5000);
    if player.economy.treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford embassy (cost: {}, treasury: {}).",
            cost, player.economy.treasury
        );
        return;
    }

    match game.queue_direct_diplomacy_action(
        domain::game_state::PendingDiplomacyAction::BuildEmbassy {
            player: player_id,
            target: target_id,
        },
    ) {
        Ok(_) => {
            println!(
                "  {}",
                crate::display::color_green(&format!(
                    "Embassy queued with {}. It will take effect at end of turn (cost: {}).",
                    target_name, cost
                ))
            );
        }
        Err(e) => {
            println!("  Cannot build embassy: {}", e);
        }
    }
}

pub(crate) fn cmd_war(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if target.id == player_id {
        println!("  You cannot declare war on yourself.");
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    // Check if already at war with anyone
    if game.world.diplomacy.is_at_war_with_anyone(player_id) {
        println!("  You are already at war. You cannot declare another war while fighting.");
        return;
    }

    // Check if already at war with this target
    if let Some(rel) = game.world.diplomacy.get_relation(player_id, target_id)
        && rel.at_war
    {
        println!("  You are already at war with {}.", target_name);
        return;
    }
    if !game.can_project_war_against(player_id, target_id) {
        println!(
            "  {} cannot be reached by land or ocean from your current territory.",
            target_name
        );
        return;
    }

    match game.queue_direct_diplomacy_action(
        domain::game_state::PendingDiplomacyAction::DeclareWar {
            from: player_id,
            to: target_id,
        },
    ) {
        Ok(_) => {
            println!();
            println!("  ╔════════════════════════════════════════╗");
            println!("  ║  DECLARATION OF WAR                    ║");
            println!("  ╚════════════════════════════════════════╝");
            println!(
                "  {}",
                crate::display::color_red(&format!(
                    "War against {} has been queued for end of turn.",
                    target_name
                ))
            );
            println!("  The declaration takes effect when the turn resolves.");
            println!();
        }
        Err(e) => {
            println!("  Cannot declare war: {}", e);
        }
    }
}

pub(crate) fn cmd_peace(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    let target_id = target.id;
    let target_name = target.name.clone();

    // Check if actually at war
    match game.world.diplomacy.get_relation(player_id, target_id) {
        Some(rel) if rel.at_war => {}
        _ => {
            println!("  You are not at war with {}.", target_name);
            return;
        }
    }

    let _ = game.world.diplomacy.make_peace(player_id, target_id);
    println!(
        "  {}",
        crate::display::color_green(&format!("Peace has been established with {}.", target_name))
    );
    println!("  The cannons fall silent.");

    // Record history event
    let turn = game.turn;
    game.archive.history.push((
        turn,
        domain::events::HistoryEvent::PeaceSigned {
            a: player_id,
            b: target_id,
        },
    ));
}

pub(crate) fn cmd_pact(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if target.is_great_power() {
        println!(
            "  {} is a Great Power. Non-aggression pacts are for Minor Nations only.",
            target.name
        );
        println!(
            "  Use 'alliance {}' to form an alliance with a Great Power.",
            target.name
        );
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    match game.world.diplomacy.propose_pact(player_id, target_id) {
        Ok(()) => {
            println!(
                "  {}",
                crate::display::color_green(&format!(
                    "Non-aggression pact signed with {}!",
                    target_name
                ))
            );
            let turn = game.turn;
            game.archive.history.push((
                turn,
                domain::events::HistoryEvent::NonAggressionPactSigned {
                    signer: player_id,
                    partner: target_id,
                },
            ));
        }
        Err(e) => {
            println!("  Cannot propose pact: {}", e);
        }
    }
}

pub(crate) fn cmd_alliance(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if !target.is_great_power() {
        println!(
            "  {} is a Minor Nation. Alliances are between Great Powers only.",
            target.name
        );
        println!(
            "  Use 'pact {}' to sign a non-aggression pact.",
            target.name
        );
        return;
    }

    if target.id == player_id {
        println!("  You cannot form an alliance with yourself.");
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    match game.world.diplomacy.propose_alliance(player_id, target_id) {
        Ok(()) => {
            println!(
                "  {}",
                crate::display::color_green(&format!("Alliance formed with {}!", target_name))
            );
            let turn = game.turn;
            game.archive.history.push((
                turn,
                domain::events::HistoryEvent::AllianceFormed {
                    signer: player_id,
                    partner: target_id,
                },
            ));
        }
        Err(e) => {
            println!("  Cannot form alliance: {}", e);
        }
    }
}

pub(crate) fn cmd_grant(game: &mut GameState, args: &str) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;

    // Parse: grant <nation> <amount>
    let parts: Vec<&str> = args.rsplitn(2, ' ').collect();
    if parts.len() < 2 {
        println!("  Usage: grant <nation> <amount>");
        println!("  Example: grant Bavaria 500");
        return;
    }

    let amount_str = parts[0];
    let nation_query = parts[1].trim();

    let amount: i64 = match amount_str.parse() {
        Ok(v) if v > 0 => v,
        _ => {
            println!(
                "  Invalid amount '{}'. Must be a positive number.",
                amount_str
            );
            return;
        }
    };

    let target = match game.find_nation_by_name(nation_query) {
        Some(n) => n,
        None => {
            println!(
                "  No unique nation matches '{}'. Be more specific.",
                nation_query
            );
            return;
        }
    };

    let target_id = target.id;
    let target_name = target.name.clone();

    if target_id == player_id {
        println!("  You cannot send a grant to yourself.");
        return;
    }

    let grant = Money::dollars(amount);
    let Some(player) = human_player(game) else {
        return;
    };
    if player.economy.treasury.checked_sub(grant).is_none() {
        println!(
            "  Cannot afford grant of {} (treasury: {}).",
            grant, player.economy.treasury
        );
        return;
    }

    game.world.diplomacy.send_grant(player_id, target_id, grant);
    let Some(player) = human_player_mut(game) else {
        return;
    };
    player.economy.treasury -= grant;
    let new_treasury = player.economy.treasury;
    let score = game
        .world
        .diplomacy
        .get_relation(player_id, target_id)
        .map(|r| r.score)
        .unwrap_or(0);

    println!(
        "  {}",
        crate::display::color_green(&format!(
            "Sent ${} grant to {}. Relationship score now: {}. Treasury: {}",
            amount, target_name, score, new_treasury
        ))
    );
}

pub(crate) fn cmd_subsidy(game: &mut GameState, args: &str) {
    if check_bankrupt(game) {
        return;
    }

    // Parse: subsidy <nation> <amount>
    let parts: Vec<&str> = args.rsplitn(2, ' ').collect();
    if parts.len() < 2 {
        println!("  Usage: subsidy <nation> <amount>");
        println!("  Example: subsidy Bavaria 200");
        println!("  Set to 0 to remove: subsidy Bavaria 0");
        return;
    }

    let amount_str = parts[0];
    let nation_query = parts[1].trim();

    let amount: i64 = match amount_str.parse() {
        Ok(v) if v >= 0 => v,
        _ => {
            println!(
                "  Invalid amount '{}'. Must be a non-negative number.",
                amount_str
            );
            return;
        }
    };

    let target = match game.find_nation_by_name(nation_query) {
        Some(n) => n,
        None => {
            println!(
                "  No unique nation matches '{}'. Be more specific.",
                nation_query
            );
            return;
        }
    };

    if target.is_great_power() {
        println!(
            "  {} is a Great Power. Subsidies are for Minor Nations only.",
            target.name
        );
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    if amount == 0 {
        // Remove subsidy
        let Some(player) = human_player_mut(game) else {
            return;
        };
        player.diplomacy.trade_subsidies.remove(&target_id);
        println!(
            "  {}",
            crate::display::color_green(&format!("Trade subsidy with {} removed.", target_name))
        );
    } else {
        let subsidy = Money::dollars(amount);
        let Some(player) = human_player_mut(game) else {
            return;
        };
        player.diplomacy.trade_subsidies.insert(target_id, subsidy);
        println!(
            "  {}",
            crate::display::color_green(&format!(
                "Trade subsidy set: {} per turn to {}.",
                subsidy, target_name
            ))
        );
    }
}

pub(crate) fn cmd_attack(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    if target.id == player_id {
        println!("  You cannot attack yourself.");
        return;
    }

    let target_id = target.id;
    let target_name = target.name.clone();

    // Check at war
    let at_war = game
        .world
        .diplomacy
        .get_relation(player_id, target_id)
        .is_some_and(|rel| rel.at_war);
    if !at_war {
        println!(
            "  You are not at war with {}. Declare war first with 'war {}'.",
            target_name, target_name
        );
        return;
    }

    // Check player has army units
    let Some(player) = human_player(game) else {
        return;
    };
    if player.military.army.is_empty() {
        println!("  You have no army units! Build units first with 'build unit <type>'.");
        return;
    }

    // Find first adjacent (or landing-accessible) province owned by target
    let player_province_ids: Vec<domain::types::ProvinceId> = game
        .get_nation(player_id)
        .map(|n| n.province_ids.clone())
        .unwrap_or_default();
    let target_province = game.world.provinces.iter().find(|p| {
        p.owner == target_id
            && (player_province_ids.iter().any(|&our_pid| {
                game.get_province(our_pid).is_some_and(|our_prov| {
                    domain::map::provinces_are_adjacent(&game.world.hex_map, our_prov, p)
                })
            }) || game
                .transient
                .pending_landings
                .iter()
                .any(|(nid, pid, _)| *nid == player_id && *pid == p.id))
    });

    let province_id = match target_province {
        Some(p) => p.id,
        None => {
            println!(
                "  {} has no reachable provinces to attack. You need an adjacent province or a naval landing site.",
                target_name
            );
            return;
        }
    };

    let province_name = game
        .get_province(province_id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    game.transient
        .pending_attacks
        .push((player_id, province_id));
    println!(
        "  Attack ordered! Your army will assault {} (province of {}) at end of turn.",
        province_name, target_name
    );
    println!(
        "  {} pending attack(s) queued. End turn to resolve.",
        game.transient.pending_attacks.len()
    );
}

/// Assign warships to establish a beachhead (naval landing site) against a target nation.
pub(crate) fn cmd_beachhead(game: &mut GameState, query: &str) {
    let player_id = game.human_player_nation;

    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  Nation '{}' not found.", query);
            return;
        }
    };
    let target_id = target.id;
    let target_name = target.name.clone();

    if target_id == player_id {
        println!("  You cannot target yourself.");
        return;
    }

    // Must be at war
    let at_war = game
        .world
        .diplomacy
        .get_relation(player_id, target_id)
        .map(|r| r.at_war)
        .unwrap_or(false);
    let target_anarchic = game
        .get_nation(target_id)
        .is_some_and(|n| n.diplomacy.is_in_anarchy);
    if !at_war && !target_anarchic {
        println!("  You are not at war with {}.", target_name);
        return;
    }

    // Must have warships
    let Some(player) = human_player(game) else {
        return;
    };
    if player.military.warships.is_empty() {
        println!("  You have no warships! Build warships first.");
        return;
    }

    // Sea-zone adjacency: must own a coastal province to embark from
    let has_coast = player
        .province_ids
        .iter()
        .any(|&pid| game.get_province(pid).is_some_and(|p| p.coastal));
    if !has_coast {
        println!("  You have no coastal provinces to embark from!");
        return;
    }

    // Find first coastal province of the target
    let coastal_province = game
        .world
        .provinces
        .iter()
        .find(|p| p.owner == target_id && p.coastal);
    let coastal_pid = match coastal_province {
        Some(p) => p.id,
        None => {
            println!("  {} has no coastal provinces to target.", target_name);
            return;
        }
    };
    let coastal_name = game
        .get_province(coastal_pid)
        .map(|p| p.name.clone())
        .unwrap_or_default();

    // Assign all warships to beachhead targeting the specific province
    if let Some(nation) = game.get_nation_mut(player_id) {
        for ship in &mut nation.military.warships {
            ship.operation = Some(domain::military::naval::NavalOperation::Beachhead(
                coastal_pid,
            ));
        }
    }

    println!(
        "  Fleet assigned to establish a beachhead at {} ({})!",
        coastal_name, target_name
    );
    println!(
        "  Landing site will be established at end of turn. Attack the coastal province next turn."
    );
}

pub(crate) fn build_freight_car(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }

    let Some(player) = human_player(game) else {
        return;
    };

    let (labor_needed, lumber_needed, steel_needed) =
        domain::economy::transport::TransportSystem::build_freight_car_cost();

    let total_labor = player.economy.labor.total_workers();
    let lumber_have = player.material_amount(MaterialType::Lumber);
    let steel_have = player.material_amount(MaterialType::Steel);

    if total_labor < labor_needed {
        println!(
            "  Cannot build freight car: need {} labor (have {} workers).",
            labor_needed, total_labor
        );
        return;
    }
    if lumber_have < lumber_needed {
        println!(
            "  Cannot build freight car: need {} lumber (have {}).",
            lumber_needed, lumber_have
        );
        return;
    }
    if steel_have < steel_needed {
        println!(
            "  Cannot build freight car: need {} steel (have {}).",
            steel_needed, steel_have
        );
        return;
    }

    let Some(player) = human_player_mut(game) else {
        return;
    };
    player.consume_material(MaterialType::Lumber, lumber_needed);
    player.consume_material(MaterialType::Steel, steel_needed);
    // Labor is consumed as a workforce requirement, not permanently removed.
    // (Workers are available each turn; this just requires having enough.)
    player.economy.transport.build_freight_cars(1);

    println!(
        "  Freight car built! (consumed {} lumber, {} steel). Total cars: {}, capacity: {}.",
        lumber_needed,
        steel_needed,
        player.economy.transport.freight_cars,
        player.economy.transport.total_capacity()
    );
}

/// Assign the player's first Engineer civilian to `coord` and start `task`.
/// Prints an actionable message on success/failure. Matches the player-facing
/// engineer contract (no instant builds from the CLI).
fn assign_engineer_task(
    game: &mut GameState,
    player_id: NationId,
    coord: HexCoord,
    task: BuildTask,
    kind_label: &str,
) {
    let cfg = game.game_data.game_config.clone();

    // Pre-flight: the target tile must be owned by the player.
    let owns_target = game
        .world
        .hex_map
        .get_tile(coord)
        .and_then(|t| t.province_id)
        .and_then(|pid| game.get_province(pid))
        .is_some_and(|p| p.owner == player_id);
    if !owns_target {
        println!(
            "  Cannot build {}: ({},{}) is not owned by your nation.",
            kind_label, coord.q, coord.r
        );
        return;
    }

    // Find an Engineer.
    let (engineer_idx, engineer_id, engineer_busy) = {
        let nation = match game.get_nation(player_id) {
            Some(n) => n,
            None => return,
        };
        let idx = nation
            .military
            .civilians
            .iter()
            .position(|c| c.civilian_type == CivilianType::Engineer);
        match idx {
            Some(i) => (
                i,
                nation.military.civilians[i].id,
                nation.military.civilians[i].working,
            ),
            None => {
                println!(
                    "  Cannot build {}: your nation has no Engineer civilian. Hire one first.",
                    kind_label
                );
                return;
            }
        }
    };

    if engineer_busy {
        println!("  Your Engineer is already working. Wait until the current task completes.");
        return;
    }

    // Check treasury against the task's cost so we don't start work we can't pay for.
    let cost = match task {
        BuildTask::Railroad => {
            let terrain = match game.world.hex_map.get_tile(coord) {
                Some(t) => t.terrain(),
                None => {
                    println!("  Invalid tile at ({},{}).", coord.q, coord.r);
                    return;
                }
            };
            // Tech pre-flight: some terrains require a researched tech.
            let Some(researched) = game.get_nation(player_id).map(|n| &n.researched_techs) else {
                println!("  Internal error: player nation is missing from game state.");
                return;
            };
            if !infrastructure::rail_terrain_enabled(terrain, researched, &game.game_data, &cfg) {
                let tech = infrastructure::railroad_required_tech(terrain, &cfg).unwrap_or("?");
                println!(
                    "  Cannot build railroad on {:?}: requires tech \"{}\".",
                    terrain, tech
                );
                return;
            }
            match infrastructure::railroad_cost(terrain, &cfg) {
                Some(c) => c,
                None => {
                    println!("  Cannot build railroad on {:?}.", terrain);
                    return;
                }
            }
        }
        BuildTask::Depot => Money::dollars(cfg.depot_cost),
        BuildTask::Port => Money::dollars(cfg.port_cost),
    };
    let Some(treasury) = game.get_nation(player_id).map(|n| n.economy.treasury) else {
        println!("  Internal error: player nation is missing from game state.");
        return;
    };
    if treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford {} (cost: {}, treasury: {}).",
            kind_label, cost, treasury
        );
        return;
    }

    // Free the engineer's old tile, claim the new one, start the build.
    let old_pos = game
        .get_nation(player_id)
        .and_then(|n| n.military.civilians[engineer_idx].position);
    if let Some(old) = old_pos
        && let Some(tile) = game.world.hex_map.get_tile_mut(old)
        && tile.assigned_civilian == Some(engineer_id)
    {
        tile.assigned_civilian = None;
    }
    if let Some(tile) = game.world.hex_map.get_tile(coord)
        && tile.assigned_civilian.is_some()
        && tile.assigned_civilian != Some(engineer_id)
    {
        println!(
            "  Target tile ({},{}) already has another civilian assigned.",
            coord.q, coord.r
        );
        return;
    }
    if let Some(tile) = game.world.hex_map.get_tile_mut(coord) {
        tile.assigned_civilian = Some(engineer_id);
    }
    if let Some(nation) = game.get_nation_mut(player_id) {
        let civ = &mut nation.military.civilians[engineer_idx];
        civ.deploy(coord);
        civ.start_build(task, &cfg);
    }
    println!(
        "  Engineer assigned to build {} at ({},{}); completes in {} turn(s).",
        kind_label,
        coord.q,
        coord.r,
        task.turns_required(&cfg)
    );
}

/// Queue a railroad-build task for the player's Engineer on the first rail-less
/// land tile in the capital province. The engineer will complete it on turn end.
pub(crate) fn cmd_build_railroad(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }
    let player_id = game.human_player_nation;
    let Some(capital_province_id) = game.get_nation(player_id).map(|n| n.capital_province_id)
    else {
        println!("  Internal error: human player nation is missing from game state.");
        return;
    };
    let tiles: Vec<HexCoord> = game
        .get_province(capital_province_id)
        .map(|p| p.tiles.clone())
        .unwrap_or_default();

    // First rail-less land tile in the capital province.
    let coord = tiles.iter().copied().find(|c| {
        game.world
            .hex_map
            .get_tile(*c)
            .is_some_and(|t| t.terrain().is_land() && !t.infrastructure.has_railroad)
    });
    let coord = match coord {
        Some(c) => c,
        None => {
            println!("  All land tiles in your capital province already have railroads.");
            return;
        }
    };

    assign_engineer_task(game, player_id, coord, BuildTask::Railroad, "railroad");
}

/// Queue a depot-build task for the player's Engineer on the capital tile (or
/// first rail tile in the capital province).
pub(crate) fn cmd_build_depot(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }
    let player_id = game.human_player_nation;
    let Some(capital_province_id) = game.get_nation(player_id).map(|n| n.capital_province_id)
    else {
        println!("  Internal error: human player nation is missing from game state.");
        return;
    };
    let Some(capital_tile_coord) = game
        .get_province(capital_province_id)
        .map(|p| p.capital_tile)
    else {
        println!("  Internal error: capital province is missing from game state.");
        return;
    };

    assign_engineer_task(
        game,
        player_id,
        capital_tile_coord,
        BuildTask::Depot,
        "depot",
    );
}

/// Queue a port-build task for the player's Engineer on the first coastal
/// rail-less port-less tile in the capital province.
pub(crate) fn cmd_build_port(game: &mut GameState) {
    if check_bankrupt(game) {
        return;
    }
    let player_id = game.human_player_nation;
    let Some(capital_province_id) = game.get_nation(player_id).map(|n| n.capital_province_id)
    else {
        println!("  Internal error: human player nation is missing from game state.");
        return;
    };
    let tiles: Vec<HexCoord> = game
        .get_province(capital_province_id)
        .map(|p| p.tiles.clone())
        .unwrap_or_default();

    let coord = tiles.iter().copied().find(|c| {
        let tile = match game.world.hex_map.get_tile(*c) {
            Some(t) => t,
            None => return false,
        };
        if !tile.terrain().is_land() || tile.infrastructure.has_port {
            return false;
        }
        c.neighbors().iter().any(|n| {
            game.world
                .hex_map
                .get_tile(*n)
                .is_some_and(|t| !t.terrain().is_land())
        })
    });
    let coord = match coord {
        Some(c) => c,
        None => {
            println!("  No coastal tile without a port found in your capital province.");
            return;
        }
    };

    assign_engineer_task(game, player_id, coord, BuildTask::Port, "port");
}

pub(crate) fn cmd_build_fort(game: &mut GameState, province_query: Option<&str>) {
    if check_bankrupt(game) {
        return;
    }

    let player_id = game.human_player_nation;
    let Some(player) = human_player(game) else {
        return;
    };

    // Find the target province: use specified province or fall back to capital
    let target_province_id = if let Some(query) = province_query {
        let lower = query.to_lowercase();
        let matches: Vec<_> = game
            .world
            .provinces
            .iter()
            .filter(|p| p.owner == player_id && p.name.to_lowercase().contains(&lower))
            .collect();
        match matches.len() {
            0 => {
                println!("  No owned province matches '{}'.", query);
                return;
            }
            1 => matches[0].id,
            _ => {
                println!("  Multiple provinces match '{}'. Be more specific.", query);
                return;
            }
        }
    } else {
        player.capital_province_id
    };

    let Some(province) = game.get_province(target_province_id) else {
        println!("  Internal error: target province is missing from game state.");
        return;
    };
    let capital_tile_coord = province.capital_tile;
    let province_name = province.name.clone();

    // Check current fort level
    let current_level = game
        .world
        .hex_map
        .get_tile(capital_tile_coord)
        .map(|t| t.infrastructure.fort_level)
        .unwrap_or(0);
    let next_level = current_level + 1;

    if next_level > 3 {
        println!("  Fort in {} already at maximum level (3).", province_name);
        return;
    }

    let cost = match domain::map::fort_cost(next_level, &game.game_data.game_config) {
        Ok(cost) => cost,
        Err(err) => {
            println!(
                "  Cannot determine fort cost for level {}: {}.",
                next_level, err
            );
            return;
        }
    };
    let Some(treasury) = game.get_nation(player_id).map(|n| n.economy.treasury) else {
        println!("  Internal error: human player nation is missing from game state.");
        return;
    };
    if treasury.checked_sub(cost).is_none() {
        println!(
            "  Cannot afford fort level {} in {} (cost: {}, treasury: {}).",
            next_level, province_name, cost, treasury
        );
        return;
    }

    let cfg_snapshot = game.game_data.game_config.clone();
    match domain::map::build_fort(&mut game.world.hex_map, capital_tile_coord, &cfg_snapshot) {
        Ok((level, cost)) => {
            let Some(player) = human_player_mut(game) else {
                return;
            };
            player.economy.treasury -= cost;
            println!(
                "  {}",
                crate::display::color_green(&format!(
                    "Fort in {} upgraded to level {}! Cost: {}, treasury now: {}.",
                    province_name, level, cost, player.economy.treasury
                ))
            );
        }
        Err(e) => {
            println!("  Cannot build fort in {}: {}", province_name, e);
        }
    }
}

/// Move a unit from its current province to another owned province.
///
/// Usage: move <unit_index> <province_name>
/// - Unit must belong to the player
/// - Target province must be owned by the player
/// - Militia units cannot move
pub(crate) fn cmd_move_unit(game: &mut GameState, args: &str) {
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        println!("  Usage: move <unit_index> <province_name>");
        println!("  Example: move 0 France City");
        println!("  Use 'army' to see unit indices.");
        return;
    }

    let index: usize = match parts[0].parse() {
        Ok(i) => i,
        Err(_) => {
            println!(
                "  Invalid unit index: '{}'. Use 'army' to see indices.",
                parts[0]
            );
            return;
        }
    };
    let province_name = parts[1].trim();

    let player_id = game.human_player_nation;
    let Some(player) = human_player(game) else {
        return;
    };

    if index >= player.military.army.len() {
        println!(
            "  Invalid index {}. You have {} army units.",
            index,
            player.military.army.len()
        );
        return;
    }

    let unit_type = player.military.army[index].unit_type;
    let unit_id = player.military.army[index].id;

    // Militia units cannot move
    if !unit_type.can_move() {
        println!("  {:?} units cannot move (garrison only).", unit_type);
        return;
    }

    // Find target province by partial name match (any province, not just owned)
    let lower_name = province_name.to_lowercase();
    let matching_provinces: Vec<_> = game
        .world
        .provinces
        .iter()
        .filter(|p| p.name.to_lowercase().contains(&lower_name))
        .collect();

    let target_province = match matching_provinces.len() {
        0 => {
            println!("  No province matches '{}'.", province_name);
            return;
        }
        1 => matching_provinces[0],
        _ => {
            println!(
                "  Multiple provinces match '{}'. Be more specific.",
                province_name
            );
            return;
        }
    };

    let target_province_id = target_province.id;
    let target_name = target_province.name.clone();
    let target_owner = target_province.owner;

    // Check the unit is not already in the target province
    if player.military.army[index].position == target_province_id {
        println!("  Unit is already in {}.", target_name);
        return;
    }

    if target_owner == player_id {
        // Friendly province: move immediately
        let Some(player) = human_player_mut(game) else {
            return;
        };
        player.military.army[index].position = target_province_id;
        println!("  Moved {:?} to {}.", unit_type, target_name);
    } else {
        // Check if at war with the province owner
        let at_war = game
            .world
            .diplomacy
            .get_relation(player_id, target_owner)
            .is_some_and(|r| r.at_war);
        if at_war {
            // Queue as a pending move (will become an attack at turn resolution)
            game.transient
                .pending_moves
                .push((player_id, unit_id, target_province_id));
            let owner_name = game
                .get_nation(target_owner)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            println!(
                "  {} is owned by {} (enemy). Attack queued on {} for end of turn.",
                target_name, owner_name, target_name
            );
            println!(
                "  {} pending attack(s) queued. End turn to resolve.",
                game.transient.pending_attacks.len() + game.transient.pending_moves.len()
            );
        } else {
            let owner_name = game
                .get_nation(target_owner)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            println!(
                "  Cannot move to {} -- owned by {} and you are not at war.",
                target_name, owner_name
            );
            println!("  Declare war first with 'war {}'.", owner_name);
        }
    }
}
