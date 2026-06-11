//! Units, civilians, ships and fleets — queries and player commands.
//!
//! Verbatim moves from `crates/wasm-bridge/src/lib.rs` — bodies must stay
//! byte-identical to the originals (error JSON strings included). Queries
//! return the same `serde_json::Value` the wasm exports stringified;
//! commands mutate pending/queued state only (everything resolves at end
//! turn).

use crate::ApiError;
use crate::guards::nation_has_tech;
use crate::parse::{parse_army_unit_type, parse_ship_type};
use domain::economy::civilians::{CivilianType, parse_civilian_type};
use domain::game_state::GameState;
use domain::hex::HexCoord;
use domain::military::ships::{Ship, ShipType};
use domain::military::units::ArmyUnitType;
use domain::types::*;

// ── Query: Units in Province ─────────────────────────────────────────

/// Get all army units in a province. Returns JSON with unit details.
pub fn get_units_in_province(
    game: &GameState,
    province_id: u32,
) -> Result<serde_json::Value, ApiError> {
    let pid = ProvinceId(province_id);
    let province = match game.get_province(pid) {
        Some(p) => p,
        None => return Err(ApiError::raw("{\"error\":\"province not found\"}")),
    };

    let province_name = province.name.clone();
    let garrison_count = province.garrison_count;

    let mut units: Vec<serde_json::Value> = Vec::new();
    for nation in &game.world.nations {
        for unit in &nation.military.army {
            if unit.position == pid {
                let stats = unit.unit_type.stats();
                // Upgrade affordances (Card #417): non-null only when an
                // upgrade target exists AND the owning nation has the
                // required tech. Cost = production-cost difference.
                let (upgrade_to_name, upgrade_cost_dollars, upgrade_arms_delta) =
                    match unit.unit_type.upgrade_to() {
                        Some(to) => {
                            let tech_met = match to.required_tech() {
                                Some(tech) => nation_has_tech(nation, tech, &game.game_data),
                                None => true,
                            };
                            if tech_met {
                                let cost =
                                    domain::military::units::upgrade_cost(unit.unit_type, to);
                                let arms_delta =
                                    to.stats().arms_required.saturating_sub(stats.arms_required);
                                (
                                    Some(format!("{:?}", to)),
                                    Some(cost.as_dollars()),
                                    Some(arms_delta),
                                )
                            } else {
                                (None, None, None)
                            }
                        }
                        None => (None, None, None),
                    };
                units.push(serde_json::json!({
                    "id": unit.id.0,
                    "unit_type": format!("{:?}", unit.unit_type),
                    "category": format!("{:?}", stats.category),
                    "owner_id": nation.id.0,
                    "owner_name": nation.name,
                    "health": unit.health,
                    "medals": unit.medals,
                    "firepower": stats.firepower,
                    "effective_firepower": unit.effective_firepower(),
                    "movement": stats.movement,
                    "movement_remaining": unit.movement_remaining,
                    "upgrade_to": upgrade_to_name,
                    "upgrade_cost": upgrade_cost_dollars,
                    "upgrade_arms_delta": upgrade_arms_delta,
                    "heal_blocked_reason": unit.last_heal_block.map(|b| b.as_str()),
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "army_units": units,
        "garrison_count": garrison_count,
        "province_name": province_name,
    }))
}

// ── Query: Pending Unit Moves ────────────────────────────────────────

/// Queued (not yet resolved) army moves for one nation. The web frontend
/// digs these out of the serialized game state; native frontends get them
/// as a view model: `[{unit_id, source_province_id, dest_province_id,
/// dest_name}]`. Nothing here resolves before end turn.
pub fn get_pending_unit_moves(
    game: &GameState,
    nation_id: u32,
) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    let moves: Vec<serde_json::Value> = game
        .transient
        .pending_moves
        .iter()
        .filter(|(n, _, _)| *n == nid)
        .filter_map(|(_, uid, dest)| {
            // Skip moves whose unit vanished rather than reporting a
            // misleading arrow (mirrors the web's pendingMoveArrows).
            let unit = nation.military.army.iter().find(|u| u.id == *uid)?;
            let dest_name = game
                .get_province(*dest)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "?".to_string());
            Some(serde_json::json!({
                "unit_id": uid.0,
                "source_province_id": unit.position.0,
                "dest_province_id": dest.0,
                "dest_name": dest_name,
            }))
        })
        .collect();
    Ok(serde_json::Value::Array(moves))
}

// ── Query: Civilians ─────────────────────────────────────────────────

/// Get all civilians for a nation. Returns deployed/undeployed groups.
pub fn get_civilians(game: &GameState, nation_id: u32) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };

    let mut deployed: Vec<serde_json::Value> = Vec::new();
    let mut undeployed: Vec<serde_json::Value> = Vec::new();

    for civ in &nation.military.civilians {
        match civ.position {
            Some(pos) => {
                let tile = game.world.hex_map.get_tile(pos);
                let terrain_str = tile
                    .map(|t| format!("{:?}", t.terrain()))
                    .unwrap_or_default();
                // F-005: Only expose resource if visible (not hidden behind prospecting)
                let resource_str = tile
                    .filter(|t| t.has_visible_resource())
                    .and_then(|t| t.resource_deposit())
                    .map(|r| format!("{:?}", r));
                deployed.push(serde_json::json!({
                    "id": civ.id.0,
                    "type": format!("{}", civ.civilian_type),
                    "position": {"q": pos.q, "r": pos.r},
                    "working": civ.working,
                    "turns_remaining": civ.turns_remaining,
                    "build_task": civ.build_task.map(|t| format!("{}", t)),
                    "tile_terrain": terrain_str,
                    "tile_resource": resource_str,
                }));
            }
            None => {
                undeployed.push(serde_json::json!({
                    "id": civ.id.0,
                    "type": format!("{}", civ.civilian_type),
                    "position": null,
                    "working": false,
                    "turns_remaining": 0,
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "deployed": deployed,
        "undeployed": undeployed,
    }))
}

// ── Query: Ships ─────────────────────────────────────────────────────

/// Get all ships for a nation.
pub fn get_ships(game: &GameState, nation_id: u32) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };

    // Card #471: ships start with `sea_zone: None` and only get assigned when
    // the AI runs its naval logic (or when the player issues a fleet command).
    // For the player's first turn we still need to report a zone so the UI
    // can group ships by fleet — fall back to the zone containing the
    // deterministic fleet anchor.
    let fallback_zone_id: Option<u32> = {
        use domain::military::navy_placement::fleet_anchor;
        fleet_anchor(nation, &game.world.hex_map, &game.world.provinces).and_then(|anchor| {
            game.world
                .sea_zones
                .iter()
                .find(|z| z.hexes.iter().any(|h| h.q == anchor.q && h.r == anchor.r))
                .map(|z| z.id.0)
        })
    };
    let resolved_zone = |s: &Ship| -> Option<u32> { s.sea_zone.map(|z| z.0).or(fallback_zone_id) };

    let merchants: Vec<serde_json::Value> = nation
        .military
        .merchant_fleet
        .iter()
        .map(|s| {
            let stats = game.game_data.ship_stats(s.ship_type);
            serde_json::json!({
                "id": s.id.0,
                "type": format!("{:?}", s.ship_type),
                "hull": s.hull_remaining,
                "hull_max": stats.hull,
                "cargo": stats.cargo,
                "sea_zone": resolved_zone(s),
            })
        })
        .collect();

    let warships: Vec<serde_json::Value> = nation
        .military
        .warships
        .iter()
        .map(|s| {
            let stats = game.game_data.ship_stats(s.ship_type);
            serde_json::json!({
                "id": s.id.0,
                "type": format!("{:?}", s.ship_type),
                "hull": s.hull_remaining,
                "hull_max": stats.hull,
                "firepower": stats.firepower,
                "sea_zone": resolved_zone(s),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "merchants": merchants,
        "warships": warships,
        "total_cargo": nation.total_cargo_capacity(&game.game_data),
        "total_naval_fp": nation.total_naval_firepower(&game.game_data),
    }))
}

// ── Query: Valid Move Targets ────────────────────────────────────────

/// Get valid move destinations for an army unit.
pub fn get_valid_move_targets(
    game: &GameState,
    nation_id: u32,
    unit_id: u32,
) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let uid = domain::map::UnitId(unit_id);

    // Find the unit
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    let unit = match nation.military.army.iter().find(|u| u.id == uid) {
        Some(u) => u,
        None => return Err(ApiError::raw("{\"error\":\"unit not found\"}")),
    };
    if !unit.unit_type.can_move() {
        return Ok(serde_json::json!({"friendly": [], "hostile": []}));
    }

    let mut friendly: Vec<serde_json::Value> = Vec::new();
    let mut hostile: Vec<serde_json::Value> = Vec::new();

    for prov in &game.world.provinces {
        if prov.id == unit.position {
            continue; // Skip current province
        }
        if prov.owner == nid {
            // Own province
            friendly.push(serde_json::json!({
                "province_id": prov.id.0,
                "name": prov.name,
            }));
        } else {
            // F-011: Allow attacking provinces at war OR owned by anarchic nations
            let at_war = game.world.diplomacy.is_at_war(nid, prov.owner);
            let target_anarchic = game
                .get_nation(prov.owner)
                .is_some_and(|n| n.diplomacy.is_in_anarchy);
            if at_war || target_anarchic {
                // Adjacency check: nation must own a province adjacent to
                // the target, or have an active landing site (matching backend logic).
                let nation_adjacent = nation.province_ids.iter().any(|&our_pid| {
                    game.get_province(our_pid).is_some_and(|our_prov| {
                        domain::map::provinces_are_adjacent(&game.world.hex_map, our_prov, prov)
                    })
                });
                let has_landing =
                    game.transient
                        .pending_landings
                        .iter()
                        .any(|(lid, pid, established)| {
                            *lid == nid && *pid == prov.id && *established < game.turn
                        });
                if !nation_adjacent && !has_landing {
                    continue;
                }

                let owner_name = game
                    .get_nation(prov.owner)
                    .map(|n| n.name.as_str())
                    .unwrap_or("Unknown");
                hostile.push(serde_json::json!({
                    "province_id": prov.id.0,
                    "name": prov.name,
                    "owner": owner_name,
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "friendly": friendly,
        "hostile": hostile,
    }))
}

// ── Query: Buildable Units ───────────────────────────────────────────

/// Get all buildable unit types for a nation (army, civilian, ship).
pub fn get_buildable_units(
    game: &GameState,
    nation_id: u32,
) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };

    let mut arms_available = nation.goods_amount(GoodsType::Arms);
    let mut treasury = nation.economy.treasury;
    let mut horses_available = nation.resource_amount(domain::types::ResourceType::Horses);
    let mut oil_available = nation.resource_amount(domain::types::ResourceType::Oil);
    let mut untrained_labor = nation.economy.labor.untrained;
    let mut trained_labor = nation.economy.labor.trained;
    let mut expert_labor = nation.economy.labor.expert;

    // Deduct resources already committed by queued army recruits so that
    // max_count and affordability checks reflect truly available amounts.
    for unit_str in &nation.economy.pending_army_recruits {
        if let Ok(ut) = unit_str.parse::<ArmyUnitType>() {
            let s = ut.stats();
            treasury = treasury
                .checked_sub(s.cost)
                .unwrap_or(domain::types::Money::ZERO);
            arms_available = arms_available.saturating_sub(s.arms_required);
            if s.requires_horse {
                horses_available = horses_available.saturating_sub(1);
            }
            if s.fuel_required > 0 {
                oil_available = oil_available.saturating_sub(s.fuel_required);
            }
            match s.recruit_tier {
                domain::economy::labor::WorkerType::Untrained => {
                    untrained_labor = untrained_labor.saturating_sub(1);
                }
                domain::economy::labor::WorkerType::Trained => {
                    trained_labor = trained_labor.saturating_sub(1);
                }
                domain::economy::labor::WorkerType::Expert => {
                    expert_labor = expert_labor.saturating_sub(1);
                }
            }
        }
    }

    // All buildable army units, ordered by category and era so the recruit
    // panel groups roles together.
    let all_army_types = [
        // Skirmisher
        ArmyUnitType::Skirmishers,
        ArmyUnitType::Sharpshooters,
        ArmyUnitType::Rangers,
        // Line infantry
        ArmyUnitType::Regulars,
        ArmyUnitType::RifleInfantry,
        ArmyUnitType::Infantry,
        // Elite infantry
        ArmyUnitType::Grenadiers,
        ArmyUnitType::Guards,
        ArmyUnitType::MachineGunners,
        // Light cavalry
        ArmyUnitType::Hussars,
        ArmyUnitType::Scouts,
        ArmyUnitType::Carbineers,
        ArmyUnitType::Mechanised,
        // Heavy cavalry
        ArmyUnitType::Cuirassiers,
        ArmyUnitType::Armour,
        // Light artillery
        ArmyUnitType::LightArtillery,
        ArmyUnitType::HorseArtillery,
        ArmyUnitType::FieldArtillery,
        ArmyUnitType::MobileArtillery,
        // Heavy artillery
        ArmyUnitType::Artillery,
        ArmyUnitType::SiegeArtillery,
        ArmyUnitType::RailroadGuns,
        // Garrison (only Conscript is recruitable; Minutemen/Militia auto-spawn)
        ArmyUnitType::Conscript,
        // Engineer
        ArmyUnitType::Sapper,
        ArmyUnitType::CombatEngineer,
        ArmyUnitType::Commandos,
        ArmyUnitType::Saboteur,
    ];

    let army: Vec<serde_json::Value> = all_army_types
        .iter()
        .filter(|t| t.can_build())
        // Card #420: drop obsolete variants once the next-era tech lands.
        // Existing units of the obsolete type stay on the board and remain
        // upgradable — only the recruit menu changes.
        .filter(|t| !t.is_recruit_obsoleted(|tech| nation_has_tech(nation, tech, &game.game_data)))
        // Hide units whose required tech is not yet researched. Affordability
        // (cost / arms) stays visible-but-greyed so the player sees what's
        // about to become available, but locked-by-tech is too noisy.
        .filter(|t| match t.required_tech() {
            Some(tech) => nation_has_tech(nation, tech, &game.game_data),
            None => true,
        })
        .map(|t| {
            let stats = t.stats();
            let labor_available = match stats.recruit_tier {
                domain::economy::labor::WorkerType::Untrained => untrained_labor,
                domain::economy::labor::WorkerType::Trained => trained_labor,
                domain::economy::labor::WorkerType::Expert => expert_labor,
            };
            let reason = if treasury < stats.cost {
                Some("Insufficient funds".to_string())
            } else if arms_available < stats.arms_required {
                Some("Not enough arms".to_string())
            } else if stats.requires_horse && horses_available < 1 {
                Some("Not enough horses".to_string())
            } else if stats.fuel_required > 0 && oil_available < stats.fuel_required {
                Some("Not enough fuel".to_string())
            } else if labor_available < 1 {
                Some(format!("Not enough {:?} workers", stats.recruit_tier))
            } else {
                None
            };
            let can_afford = reason.is_none();

            let max_by_treasury = if stats.cost.as_dollars() > 0 {
                (treasury.as_dollars() / stats.cost.as_dollars()) as u32
            } else {
                99
            };
            let max_by_arms = if stats.arms_required > 0 {
                arms_available / stats.arms_required
            } else {
                99
            };
            let max_by_horses = if stats.requires_horse {
                horses_available
            } else {
                99
            };
            let max_by_oil = if stats.fuel_required > 0 {
                oil_available / stats.fuel_required
            } else {
                99
            };
            let max_by_labor = labor_available;
            let max_count = max_by_treasury
                .min(max_by_arms)
                .min(max_by_horses)
                .min(max_by_oil)
                .min(max_by_labor);

            serde_json::json!({
                "type": format!("{:?}", t),
                "category": format!("{:?}", stats.category),
                "cost": stats.cost.as_dollars(),
                "arms_required": stats.arms_required,
                "firepower": stats.firepower,
                "movement": stats.movement,
                "can_afford": can_afford,
                "max_count": max_count,
                // Always true now that locked-by-tech variants are filtered out
                // upstream; kept on the wire so the TS interface doesn't shift.
                "tech_met": true,
                "reason": reason,
                "requires_horse": stats.requires_horse,
            })
        })
        .collect();

    // Civilians
    let all_civilian_types = [
        CivilianType::Farmer,
        CivilianType::Rancher,
        CivilianType::Forester,
        CivilianType::Engineer,
        CivilianType::Miner,
        CivilianType::Driller,
        CivilianType::Prospector,
    ];

    let cfg = &game.game_data.game_config;
    let expert_workers = nation.economy.labor.expert;
    let civilians: Vec<serde_json::Value> = all_civilian_types
        .iter()
        .filter(|ct| ct.is_unlocked(&nation.researched_techs, &game.game_data, cfg))
        .map(|ct| {
            let cost = ct.creation_cost(cfg);
            let cash_ok = treasury >= cost;
            let expert_ok = !cfg.civilian_costs_expert || expert_workers > 0;
            let can_afford = cash_ok && expert_ok;
            let reason = if !cash_ok {
                Some("Insufficient funds".to_string())
            } else if !expert_ok {
                Some("No expert workers available".to_string())
            } else {
                None
            };
            // Max hirable this turn: limited by cash and expert workers
            let max_by_cash = if cost.cents() > 0 {
                (treasury.cents() / cost.cents()) as u32
            } else {
                u32::MAX
            };
            let max_by_expert = if cfg.civilian_costs_expert {
                expert_workers
            } else {
                u32::MAX
            };
            let max_count = max_by_cash.min(max_by_expert);
            serde_json::json!({
                "type": format!("{}", ct),
                "cost": cost.as_dollars(),
                "can_afford": can_afford,
                "tech_met": true,
                "reason": reason,
                "max_count": max_count,
                "expert_required": cfg.civilian_costs_expert,
            })
        })
        .collect();

    // Ships
    let all_ship_types = [
        ShipType::Trader,
        ShipType::Indiaman,
        ShipType::Clipper,
        ShipType::Paddlewheeler,
        ShipType::Freighter,
        ShipType::Frigate,
        ShipType::ShipOfTheLine,
        ShipType::Raider,
        ShipType::Ironclad,
        ShipType::AdvancedIronclad,
        ShipType::ArmouredCruiser,
        ShipType::Dreadnought,
        ShipType::Battlecruiser,
    ];

    let ships: Vec<serde_json::Value> = all_ship_types
        .iter()
        .map(|st| {
            let stats = game.game_data.ship_stats(*st);
            let tech_met = match &stats.prerequisite_tech {
                Some(tech) => nation_has_tech(nation, tech, &game.game_data),
                None => true,
            };

            let mut resources_needed = serde_json::Map::new();
            if stats.fabric_cost > 0 {
                resources_needed.insert("Fabric".into(), stats.fabric_cost.into());
            }
            if stats.lumber_cost > 0 {
                resources_needed.insert("Lumber".into(), stats.lumber_cost.into());
            }
            if stats.arms_cost > 0 {
                resources_needed.insert("Arms".into(), stats.arms_cost.into());
            }
            if stats.steel_cost > 0 {
                resources_needed.insert("Steel".into(), stats.steel_cost.into());
            }
            if stats.coal_cost > 0 {
                resources_needed.insert("Coal".into(), stats.coal_cost.into());
            }

            let has_fabric = nation.material_amount(MaterialType::Fabric) >= stats.fabric_cost;
            let has_lumber = nation.material_amount(MaterialType::Lumber) >= stats.lumber_cost;
            let has_arms = nation.goods_amount(GoodsType::Arms) >= stats.arms_cost;
            let has_steel = nation.material_amount(MaterialType::Steel) >= stats.steel_cost;
            let has_coal = nation.resource_amount(ResourceType::Coal) >= stats.coal_cost;
            let can_afford = has_fabric && has_lumber && has_arms && has_steel && has_coal;

            let max_by_fabric = if stats.fabric_cost > 0 {
                nation.material_amount(MaterialType::Fabric) / stats.fabric_cost
            } else {
                99
            };
            let max_by_lumber = if stats.lumber_cost > 0 {
                nation.material_amount(MaterialType::Lumber) / stats.lumber_cost
            } else {
                99
            };
            let max_by_arms = if stats.arms_cost > 0 {
                nation.goods_amount(GoodsType::Arms) / stats.arms_cost
            } else {
                99
            };
            let max_by_steel = if stats.steel_cost > 0 {
                nation.material_amount(MaterialType::Steel) / stats.steel_cost
            } else {
                99
            };
            let max_by_coal = if stats.coal_cost > 0 {
                nation.resource_amount(ResourceType::Coal) / stats.coal_cost
            } else {
                99
            };
            let max_count = max_by_fabric
                .min(max_by_lumber)
                .min(max_by_arms)
                .min(max_by_steel)
                .min(max_by_coal);

            let reason = if !tech_met {
                Some(format!(
                    "Requires {}",
                    stats.prerequisite_tech.as_deref().unwrap_or("?")
                ))
            } else if !can_afford {
                Some("Insufficient resources".to_string())
            } else {
                None
            };

            serde_json::json!({
                "type": format!("{:?}", st),
                "category": format!("{:?}", st.category()),
                "resources_needed": serde_json::Value::Object(resources_needed),
                "can_afford": can_afford,
                "max_count": max_count,
                "tech_met": tech_met,
                "reason": reason,
                "firepower": stats.firepower,
                "hull": stats.hull,
                "cargo": stats.cargo,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "army": army,
        "civilians": civilians,
        "ships": ships,
        "treasury": treasury.as_dollars(),
        "arms": arms_available,
    }))
}

// ── Command: Queue Unit Move ─────────────────────────────────────────

/// Queue a unit move for turn resolution.
pub fn queue_unit_move(
    game: &mut GameState,
    nation_id: u32,
    unit_id: u32,
    dest_province_id: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    if nid != game.human_player_nation {
        return Err(ApiError::raw(
            "{\"error\":\"cannot queue moves for another nation\"}",
        ));
    }
    let uid = domain::map::UnitId(unit_id);
    let dest = ProvinceId(dest_province_id);

    // Validate unit exists and belongs to nation
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    let unit = match nation.military.army.iter().find(|u| u.id == uid) {
        Some(u) => u,
        None => return Err(ApiError::raw("{\"error\":\"unit not found\"}")),
    };
    if !unit.unit_type.can_move() {
        return Err(ApiError::raw("{\"error\":\"this unit cannot move\"}"));
    }

    // Validate destination province exists
    let dest_prov = match game.get_province(dest) {
        Some(p) => p,
        None => return Err(ApiError::raw("{\"error\":\"province not found\"}")),
    };

    // F-003+F-011: Validate target legality — own province, at-war, or anarchic target
    let target_is_own = dest_prov.owner == nid;
    let target_at_war = game.world.diplomacy.is_at_war(nid, dest_prov.owner);
    let target_anarchic = game
        .get_nation(dest_prov.owner)
        .is_some_and(|n| n.diplomacy.is_in_anarchy);
    if !target_is_own && !target_at_war && !target_anarchic {
        return Err(ApiError::raw(
            "{\"error\":\"cannot move to that province\"}",
        ));
    }

    // F-003: Replace existing pending move for this unit (prevent duplicates)
    game.transient.pending_moves.retain(|(_, id, _)| *id != uid);
    game.transient.pending_moves.push((nid, uid, dest));
    Ok(())
}

// ── Command: Cancel Unit Move ────────────────────────────────────────

/// Cancel a pending unit move.
pub fn cancel_unit_move(game: &mut GameState, unit_id: u32) -> Result<(), ApiError> {
    let uid = domain::map::UnitId(unit_id);
    let player = game.human_player_nation;
    game.transient
        .pending_moves
        .retain(|(nid, id, _)| !(*nid == player && *id == uid));
    Ok(())
}

// ── Command: Disband Unit ────────────────────────────────────────────

/// Dismiss (disband) one of the player's army units.
///
/// Validates the unit belongs to the human player nation and is not a Garrison
/// (militia / garrison artillery). Removes the unit and any pending move for it.
pub fn disband_unit(game: &mut GameState, unit_id: u32) -> Result<(), ApiError> {
    if game.observer_mode {
        return Err(ApiError::raw(
            "{\"error\":\"disband not allowed in observer mode\"}",
        ));
    }
    let uid = domain::map::UnitId(unit_id);
    let player_nation = game.human_player_nation;
    match domain::military::units::disband_unit(game, player_nation, uid) {
        Ok(()) => Ok(()),
        Err(e) => Err(ApiError::msg(e)),
    }
}

// ── Command: Deploy Civilian ─────────────────────────────────────────

/// Deploy a civilian to a hex tile to start improving it.
pub fn deploy_civilian(
    game: &mut GameState,
    civilian_id: u32,
    hex_q: i32,
    hex_r: i32,
) -> Result<(), ApiError> {
    let cid = domain::map::UnitId(civilian_id);
    let coord = HexCoord::new(hex_q, hex_r);
    let human_nid = game.human_player_nation;

    // Validate tile exists and is owned by the player
    let tile = match game.world.hex_map.get_tile(coord) {
        Some(t) => t,
        None => return Err(ApiError::raw("{\"error\":\"tile not found\"}")),
    };
    let tile_province = match tile.province_id {
        Some(pid) => pid,
        None => return Err(ApiError::raw("{\"error\":\"tile has no province\"}")),
    };
    let prov = match game.get_province(tile_province) {
        Some(p) => p,
        None => return Err(ApiError::raw("{\"error\":\"province not found\"}")),
    };
    if prov.owner != human_nid {
        return Err(ApiError::raw("{\"error\":\"tile not owned by player\"}"));
    }

    // F-006: Check tile doesn't already have an assigned civilian
    if tile.assigned_civilian.is_some() {
        return Err(ApiError::raw(
            "{\"error\":\"tile already has a civilian assigned\"}",
        ));
    }

    let terrain = tile.terrain();
    // F-017: Only use visible resources for can_improve check.
    // Prospectors work on terrain (not resources), so they're unaffected.
    // Other civilians need visible resources — hidden deposits are not valid targets.
    let resource = if tile.has_visible_resource() {
        tile.resource_deposit()
    } else {
        None
    };
    let improvement_level = tile.improvement_level();

    // Find the civilian in the player's nation
    let nation = match game.get_nation_mut(human_nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    let civ = match nation.military.civilians.iter_mut().find(|c| c.id == cid) {
        Some(c) => c,
        None => return Err(ApiError::raw("{\"error\":\"civilian not found\"}")),
    };
    if civ.position.is_some() {
        return Err(ApiError::raw("{\"error\":\"civilian already deployed\"}"));
    }
    if !civ.civilian_type.can_improve(terrain, resource) {
        return Err(ApiError::raw(
            "{\"error\":\"civilian cannot improve this tile\"}",
        ));
    }

    civ.deploy(coord);
    // Engineers are deployed without an auto-start; the player issues a build
    // order via wasm_engineer_build once the engineer is on the right hex.
    // Prospectors reveal in 1 turn (deploy → end turn → reveal).
    if civ.civilian_type != domain::economy::CivilianType::Engineer {
        let turns = if civ.civilian_type == domain::economy::CivilianType::Prospector {
            1
        } else if improvement_level == 0 {
            3
        } else {
            5
        };
        civ.start_work(turns);
    }

    // F-006: Set assigned_civilian on the tile
    if let Some(tile_mut) = game.world.hex_map.get_tile_mut(coord) {
        tile_mut.assigned_civilian = Some(cid);
    }

    Ok(())
}

// ── Command: Recall Civilian ─────────────────────────────────────────

/// Recall a deployed civilian back to the capital.
pub fn recall_civilian(game: &mut GameState, civilian_id: u32) -> Result<(), ApiError> {
    let cid = domain::map::UnitId(civilian_id);
    let human_nid = game.human_player_nation;

    // Extract old position before mutating, to avoid borrow conflicts
    let old_pos = {
        let nation = match game.get_nation(human_nid) {
            Some(n) => n,
            None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
        };
        let civ = match nation.military.civilians.iter().find(|c| c.id == cid) {
            Some(c) => c,
            None => return Err(ApiError::raw("{\"error\":\"civilian not found\"}")),
        };
        civ.position
    };

    // F-006: Clear assigned_civilian on the old tile
    if let Some(pos) = old_pos
        && let Some(tile_mut) = game.world.hex_map.get_tile_mut(pos)
    {
        tile_mut.assigned_civilian = None;
    }

    // Now mutate the civilian
    let nation = match game.get_nation_mut(human_nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    let civ = match nation.military.civilians.iter_mut().find(|c| c.id == cid) {
        Some(c) => c,
        None => return Err(ApiError::raw("{\"error\":\"civilian not found\"}")),
    };
    civ.position = None;
    civ.working = false;
    civ.turns_remaining = 0;
    civ.build_task = None;

    Ok(())
}

// ── Command: Engineer Build (railroad / depot / port) ────────────────

/// Order a deployed Engineer civilian to start a build task on its current hex.
/// The engineer must already be deployed (via `deploy_civilian`) on an
/// owned hex. `build_kind` is one of "railroad", "depot", "port".
pub fn engineer_build(
    game: &mut GameState,
    civilian_id: u32,
    build_kind: &str,
) -> Result<(), ApiError> {
    use domain::economy::BuildTask;

    let cid = domain::map::UnitId(civilian_id);
    let human_nid = game.human_player_nation;

    let task = match build_kind.to_lowercase().as_str() {
        "railroad" | "rail" => BuildTask::Railroad,
        "depot" => BuildTask::Depot,
        "port" => BuildTask::Port,
        other => return Err(ApiError::msg(format!("unknown build kind: {}", other))),
    };

    // Look up the engineer, its position, and the target tile's state.
    let (position, working) = {
        let nation = match game.get_nation(human_nid) {
            Some(n) => n,
            None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
        };
        let civ = match nation.military.civilians.iter().find(|c| c.id == cid) {
            Some(c) => c,
            None => return Err(ApiError::raw("{\"error\":\"civilian not found\"}")),
        };
        if civ.civilian_type != domain::economy::CivilianType::Engineer {
            return Err(ApiError::raw("{\"error\":\"civilian is not an engineer\"}"));
        }
        (civ.position, civ.working)
    };

    let pos = match position {
        Some(p) => p,
        None => return Err(ApiError::raw("{\"error\":\"engineer is not deployed\"}")),
    };
    if working {
        return Err(ApiError::raw("{\"error\":\"engineer is already working\"}"));
    }

    // Validate tile ownership + prerequisites (depot needs railroad or capital,
    // port needs coastal tile). Railroad only needs ownership + land.
    let tile = match game.world.hex_map.get_tile(pos) {
        Some(t) => t,
        None => return Err(ApiError::raw("{\"error\":\"tile not found\"}")),
    };
    let owns_tile = tile
        .province_id
        .and_then(|pid| game.get_province(pid))
        .is_some_and(|p| p.owner == human_nid);
    if !owns_tile {
        return Err(ApiError::raw("{\"error\":\"tile not owned by player\"}"));
    }
    match task {
        BuildTask::Railroad => {
            if !tile.terrain().is_land() {
                return Err(ApiError::raw(
                    "{\"error\":\"cannot build railroad on sea\"}",
                ));
            }
            if tile.infrastructure.has_railroad {
                return Err(ApiError::raw("{\"error\":\"railroad already exists\"}"));
            }
            // Tech pre-flight: some terrains require a researched tech.
            let cfg_for_tech = &game.game_data.game_config;
            let researched = &game.get_nation(human_nid).unwrap().researched_techs;
            if !domain::map::infrastructure::rail_terrain_enabled(
                tile.terrain(),
                researched,
                &game.game_data,
                cfg_for_tech,
            ) {
                let tech = domain::map::infrastructure::railroad_required_tech(
                    tile.terrain(),
                    cfg_for_tech,
                )
                .unwrap_or("?");
                return Err(ApiError::msg(format!(
                    "railroad on {:?} requires tech: {}",
                    tile.terrain(),
                    tech
                )));
            }
        }
        BuildTask::Depot => {
            if !tile.terrain().is_land() {
                return Err(ApiError::raw("{\"error\":\"cannot build depot on sea\"}"));
            }
            if tile.infrastructure.has_depot {
                return Err(ApiError::raw("{\"error\":\"depot already exists\"}"));
            }
            if !tile.infrastructure.has_railroad {
                return Err(ApiError::raw(
                    "{\"error\":\"depot requires a railroad on the tile\"}",
                ));
            }
        }
        BuildTask::Port => {
            if !tile.terrain().is_land() {
                return Err(ApiError::raw("{\"error\":\"cannot build port on sea\"}"));
            }
            if tile.infrastructure.has_port {
                return Err(ApiError::raw("{\"error\":\"port already exists\"}"));
            }
            let is_coastal = pos.neighbors().iter().any(|n| {
                game.world
                    .hex_map
                    .get_tile(*n)
                    .is_some_and(|t| !t.terrain().is_land())
            });
            if !is_coastal {
                return Err(ApiError::raw(
                    "{\"error\":\"port requires a coastal tile\"}",
                ));
            }
        }
    }

    // Affordability gate — treasury is debited on completion, so reject orders
    // the nation cannot pay for up front (matches CLI/AI contract).
    let cfg = game.game_data.game_config.clone();
    let task_cost = match task {
        BuildTask::Railroad => {
            match domain::map::infrastructure::railroad_cost(tile.terrain(), &cfg) {
                Some(c) => c,
                None => {
                    return Err(ApiError::raw(
                        "{\"error\":\"cannot build railroad on this terrain\"}",
                    ));
                }
            }
        }
        BuildTask::Depot => Money::dollars(cfg.depot_cost),
        BuildTask::Port => Money::dollars(cfg.port_cost),
    };
    let nation_treasury = game
        .get_nation(human_nid)
        .map(|n| n.economy.treasury)
        .unwrap_or(Money::ZERO);
    if nation_treasury.checked_sub(task_cost).is_none() {
        return Err(ApiError::raw("{\"error\":\"insufficient funds\"}"));
    }

    // Issue the build order.
    let nation = match game.get_nation_mut(human_nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    if let Some(civ) = nation.military.civilians.iter_mut().find(|c| c.id == cid) {
        civ.start_build(task, &cfg);
    }
    Ok(())
}

// ── Command: Recruit Army Unit ───────────────────────────────────────

/// Queue an army unit for end-of-turn recruitment. Resources are NOT deducted
/// until end-of-turn when the unit is actually created.
pub fn recruit_army_unit(
    game: &mut GameState,
    nation_id: u32,
    unit_type_str: &str,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let unit_type = match parse_army_unit_type(unit_type_str) {
        Some(t) => t,
        None => {
            return Err(ApiError::msg(format!(
                "unknown unit type: {}",
                unit_type_str
            )));
        }
    };

    if !unit_type.can_build() {
        return Err(ApiError::raw(
            "{\"error\":\"this unit type cannot be built\"}",
        ));
    }

    // Tech and obsolescence checks only (no resource check at queue time)
    {
        let nation = match game.get_nation(nid) {
            Some(n) => n,
            None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
        };
        if let Some(tech) = unit_type.required_tech()
            && !nation_has_tech(nation, tech, &game.game_data)
        {
            return Err(ApiError::msg(format!("requires tech: {}", tech)));
        }
        if unit_type.is_recruit_obsoleted(|tech| nation_has_tech(nation, tech, &game.game_data)) {
            return Err(ApiError::msg(format!(
                "{:?} is obsoleted by a researched newer variant; recruit the upgrade instead",
                unit_type
            )));
        }
    }

    if let Some(nation) = game.get_nation_mut(nid) {
        nation
            .economy
            .pending_army_recruits
            .push(unit_type_str.to_string());
    }

    Ok(())
}

/// Set the number of queued recruits of a given unit type (replaces all existing
/// queued recruits of that type with `count` copies). Resources deducted at end-of-turn.
pub fn set_pending_army_recruits(
    game: &mut GameState,
    nation_id: u32,
    unit_type_str: &str,
    count: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let unit_type = match parse_army_unit_type(unit_type_str) {
        Some(t) => t,
        None => {
            return Err(ApiError::msg(format!(
                "unknown unit type: {}",
                unit_type_str
            )));
        }
    };
    if !unit_type.can_build() {
        return Err(ApiError::raw(
            "{\"error\":\"this unit type cannot be built\"}",
        ));
    }
    {
        let nation = match game.get_nation(nid) {
            Some(n) => n,
            None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
        };
        if let Some(tech) = unit_type.required_tech()
            && !nation_has_tech(nation, tech, &game.game_data)
        {
            return Err(ApiError::msg(format!("requires tech: {}", tech)));
        }
        if unit_type.is_recruit_obsoleted(|tech| nation_has_tech(nation, tech, &game.game_data)) {
            return Err(ApiError::msg(format!(
                "{:?} is obsoleted; recruit the upgrade instead",
                unit_type
            )));
        }
    }
    if let Some(nation) = game.get_nation_mut(nid) {
        nation
            .economy
            .pending_army_recruits
            .retain(|s| s != unit_type_str);
        for _ in 0..count {
            nation
                .economy
                .pending_army_recruits
                .push(unit_type_str.to_string());
        }
    }
    Ok(())
}

// ── Command: Upgrade Unit (Card #417) ────────────────────────────────

/// Upgrade a single player-owned unit to its next-era variant.
///
/// Cost = production-cost difference, paid from the treasury. Any extra
/// `arms_required` is consumed from the Arms stockpile. Health and medals
/// are preserved.
pub fn upgrade_unit(game: &mut GameState, nation_id: u32, unit_id: u32) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let uid = domain::map::UnitId(unit_id);
    match domain::military::units::upgrade_player_unit(game, nid, uid) {
        Ok(_) => Ok(()),
        Err(e) => Err(ApiError::msg(e)),
    }
}

/// Bulk-upgrade player units. Returns `{"upgraded": N, "failed": [...]}`.
/// Each id is processed in order; the first failure is reported but the
/// rest are still attempted (so a single under-funded upgrade doesn't
/// silently abort the batch). The game state reflects every successful
/// upgrade.
///
/// The wasm wrapper reattaches the serialized game under a `"game"` key to
/// preserve the legacy `{"upgraded": N, "failed": [...], "game": <state>}`
/// response shape.
pub fn upgrade_units(
    game: &mut GameState,
    nation_id: u32,
    unit_ids_json: &str,
) -> Result<serde_json::Value, ApiError> {
    let ids: Vec<u32> = match serde_json::from_str(unit_ids_json) {
        Ok(v) => v,
        Err(e) => return Err(ApiError::msg(format!("bad unit_ids JSON: {}", e))),
    };
    let nid = NationId(nation_id);
    let mut upgraded = 0usize;
    let mut failures: Vec<serde_json::Value> = Vec::new();
    for id in ids {
        let uid = domain::map::UnitId(id);
        match domain::military::units::upgrade_player_unit(game, nid, uid) {
            Ok(_) => upgraded += 1,
            Err(e) => failures.push(serde_json::json!({ "unit_id": id, "error": e.to_string() })),
        }
    }
    Ok(serde_json::json!({
        "upgraded": upgraded,
        "failed": failures,
    }))
}

/// Inspect a unit's upgrade prospects: { upgrade_to: "...", cost, arms_delta, tech_met }.
/// Returns `{ "upgrade_to": null }` for end-of-line variants.
pub fn get_upgrade_info(
    game: &GameState,
    nation_id: u32,
    unit_id: u32,
) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    let unit = match nation.military.army.iter().find(|u| u.id.0 == unit_id) {
        Some(u) => u,
        None => return Err(ApiError::raw("{\"error\":\"unit not found\"}")),
    };
    let from = unit.unit_type;
    let to = match from.upgrade_to() {
        Some(t) => t,
        None => return Ok(serde_json::json!({ "upgrade_to": null })),
    };
    let cost = domain::military::units::upgrade_cost(from, to);
    let arms_delta = to
        .stats()
        .arms_required
        .saturating_sub(from.stats().arms_required);
    let tech_met = match to.required_tech() {
        Some(tech) => nation_has_tech(nation, tech, &game.game_data),
        None => true,
    };
    Ok(serde_json::json!({
        "upgrade_to": format!("{:?}", to),
        "cost": cost.as_dollars(),
        "arms_delta": arms_delta,
        "tech_met": tech_met,
    }))
}

// ── Command: Hire Civilian ───────────────────────────────────────────

/// Hire a new civilian unit.
pub fn set_pending_civilian_hire(
    game: &mut GameState,
    nation_id: u32,
    civilian_type_str: &str,
    count: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let civ_type = match parse_civilian_type(civilian_type_str) {
        Some(t) => t,
        None => {
            return Err(ApiError::msg(format!(
                "unknown civilian type: {}",
                civilian_type_str
            )));
        }
    };

    // Check tech unlock before taking a mutable borrow
    {
        let nation = match game.get_nation(nid) {
            Some(n) => n,
            None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
        };
        if !civ_type.is_unlocked(
            &nation.researched_techs,
            &game.game_data,
            &game.game_data.game_config,
        ) {
            return Err(ApiError::raw(
                "{\"error\":\"civilian type locked: required technology not researched\"}",
            ));
        }
    }

    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    if count == 0 {
        nation.economy.pending_civilian_hires.remove(&civ_type);
    } else {
        nation
            .economy
            .pending_civilian_hires
            .insert(civ_type, count);
    }

    Ok(())
}

/// Set the pending worker training counts for end-of-turn processing.
pub fn set_pending_training(
    game: &mut GameState,
    nation_id: u32,
    to_trained: u32,
    to_expert: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    nation.economy.pending_train_to_trained = to_trained;
    nation.economy.pending_train_to_expert = to_expert;
    Ok(())
}

/// Set the pending immigration count for end-of-turn processing.
pub fn set_pending_immigration(
    game: &mut GameState,
    nation_id: u32,
    count: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    nation.economy.pending_immigration = count;
    Ok(())
}

// ── Command: Build Ship ──────────────────────────────────────────────

/// Queue a ship for end-of-turn construction. Resources are NOT deducted until end-of-turn.
/// Calling this with the same ship type again replaces the existing order (idempotent for
/// the slider pattern). Call `cancel_ship_build` to remove the order.
pub fn build_ship(
    game: &mut GameState,
    nation_id: u32,
    ship_type_str: &str,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let ship_type = match parse_ship_type(ship_type_str) {
        Some(t) => t,
        None => {
            return Err(ApiError::msg(format!(
                "unknown ship type: {}",
                ship_type_str
            )));
        }
    };

    let stats = game.game_data.ship_stats(ship_type).clone();

    // Check tech prerequisite and affordability (resources must be available at queue time
    // so the player gets immediate feedback, but deduction happens at end-of-turn).
    {
        let nation = match game.get_nation(nid) {
            Some(n) => n,
            None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
        };
        if let Some(ref tech) = stats.prerequisite_tech
            && !nation_has_tech(nation, tech, &game.game_data)
        {
            return Err(ApiError::msg(format!("requires tech: {}", tech)));
        }
        if nation.material_amount(MaterialType::Fabric) < stats.fabric_cost {
            return Err(ApiError::raw("{\"error\":\"not enough fabric\"}"));
        }
        if nation.material_amount(MaterialType::Lumber) < stats.lumber_cost {
            return Err(ApiError::raw("{\"error\":\"not enough lumber\"}"));
        }
        if nation.goods_amount(GoodsType::Arms) < stats.arms_cost {
            return Err(ApiError::raw("{\"error\":\"not enough arms\"}"));
        }
        if nation.material_amount(MaterialType::Steel) < stats.steel_cost {
            return Err(ApiError::raw("{\"error\":\"not enough steel\"}"));
        }
        if nation.resource_amount(ResourceType::Coal) < stats.coal_cost {
            return Err(ApiError::raw("{\"error\":\"not enough coal\"}"));
        }
    }

    // Queue ship for end-of-turn delivery; resources deducted then.
    {
        let nation = match game.get_nation_mut(nid) {
            Some(n) => n,
            None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
        };
        nation.economy.pending_ships.push(ship_type_str.to_string());
    }

    Ok(())
}

/// Cancel a queued ship order (remove the first matching entry from pending_ships).
pub fn cancel_ship_build(
    game: &mut GameState,
    nation_id: u32,
    ship_type_str: &str,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation_mut(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
    };
    if let Some(pos) = nation
        .economy
        .pending_ships
        .iter()
        .position(|s| s == ship_type_str)
    {
        nation.economy.pending_ships.remove(pos);
    }
    Ok(())
}

/// Set the number of queued ships of a given type (replaces all existing queued
/// ships of that type with `count` copies). Resources are deducted at end-of-turn.
pub fn set_pending_ships(
    game: &mut GameState,
    nation_id: u32,
    ship_type_str: &str,
    count: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let ship_type = match parse_ship_type(ship_type_str) {
        Some(t) => t,
        None => {
            return Err(ApiError::msg(format!(
                "unknown ship type: {}",
                ship_type_str
            )));
        }
    };
    {
        let nation = match game.get_nation(nid) {
            Some(n) => n,
            None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
        };
        let stats = game.game_data.ship_stats(ship_type);
        if let Some(ref tech) = stats.prerequisite_tech.clone()
            && !nation_has_tech(nation, tech, &game.game_data)
        {
            return Err(ApiError::msg(format!("requires tech: {}", tech)));
        }
    }
    {
        let nation = match game.get_nation_mut(nid) {
            Some(n) => n,
            None => return Err(ApiError::raw("{\"error\":\"nation not found\"}")),
        };
        nation.economy.pending_ships.retain(|s| s != ship_type_str);
        for _ in 0..count {
            nation.economy.pending_ships.push(ship_type_str.to_string());
        }
    }
    Ok(())
}

// ── Mutation: Assign Beachhead ──────────────────────────────────────

/// Assign a nation's warships to establish a beachhead on a specific coastal enemy province.
pub fn assign_beachhead(
    game: &mut GameState,
    nation_id: u32,
    target_province_id: u32,
) -> Result<(), ApiError> {
    let nid = NationId(nation_id);
    let target_pid = ProvinceId(target_province_id);

    // Validate the target province is coastal and owned by an enemy at war
    let valid = game.get_province(target_pid).is_some_and(|p| {
        p.coastal && {
            let at_war = game.world.diplomacy.is_at_war(nid, p.owner);
            let target_anarchic = game
                .get_nation(p.owner)
                .is_some_and(|n| n.diplomacy.is_in_anarchy);
            at_war || target_anarchic
        }
    });
    if !valid {
        return Err(ApiError::raw(
            "{\"error\":\"target province is not a valid coastal enemy province\"}",
        ));
    }

    // Must have warships
    let has_warships = game
        .get_nation(nid)
        .is_some_and(|n| !n.military.warships.is_empty());
    if !has_warships {
        return Err(ApiError::raw("{\"error\":\"no warships available\"}"));
    }

    // Sea-zone adjacency: attacker must own at least one coastal province (embarkation point)
    let has_coast = game.get_nation(nid).is_some_and(|n| {
        n.province_ids
            .iter()
            .any(|&pid| game.get_province(pid).is_some_and(|p| p.coastal))
    });
    if !has_coast {
        return Err(ApiError::raw(
            "{\"error\":\"you have no coastal provinces to embark from\"}",
        ));
    }

    // Assign all warships to beachhead targeting the specific province
    if let Some(nation) = game.get_nation_mut(nid) {
        for ship in &mut nation.military.warships {
            ship.operation = Some(domain::military::naval::NavalOperation::Beachhead(
                target_pid,
            ));
        }
    }

    Ok(())
}

/// Queue a fleet move (card #471). Validates that both sea zones exist, are
/// non-lake and adjacent, and that the nation has at least one warship in
/// `from_zone_id` (with the same fallback-zone back-fill that
/// `wasm_get_navy_markers` and `get_ships` apply, so turn-1 moves work
/// before the AI naval pass runs). On success the move is appended to
/// `pending_fleet_moves`. The actual ship repositioning happens at
/// end-of-turn in `resolve_pending_fleet_moves`, mirroring how army
/// `pending_moves` are drained.
///
/// If a pending move for the same `(nation, from_zone)` already exists it is
/// replaced — re-clicking a destination just retargets the queued move.
pub fn move_fleet(
    game: &mut GameState,
    nation_id: u32,
    from_zone_id: u32,
    to_zone_id: u32,
) -> Result<(), ApiError> {
    use domain::map::sea_zones::SeaZoneId;

    let nid = NationId(nation_id);
    let from_z = SeaZoneId(from_zone_id);
    let to_z = SeaZoneId(to_zone_id);

    let from_zone_ok = game
        .world
        .sea_zones
        .iter()
        .any(|z| z.id == from_z && !z.is_lake);
    let to_zone_ok = game
        .world
        .sea_zones
        .iter()
        .any(|z| z.id == to_z && !z.is_lake);
    if !from_zone_ok || !to_zone_ok {
        return Err(ApiError::raw("{\"error\":\"invalid sea zone\"}"));
    }
    let adjacent = game
        .world
        .sea_zones
        .iter()
        .find(|z| z.id == from_z)
        .is_some_and(|z| z.is_adjacent_to(to_z));
    if !adjacent {
        return Err(ApiError::raw("{\"error\":\"sea zones are not adjacent\"}"));
    }

    // Same back-fill the read-side queries do: if `from_z` matches the fleet
    // anchor's containing zone, treat ships with `sea_zone: None` as living
    // there. Persist the assignment so end-of-turn movement can find them.
    let fallback_zone_id: Option<SeaZoneId> = {
        use domain::military::navy_placement::fleet_anchor;
        game.get_nation(nid).and_then(|n| {
            fleet_anchor(n, &game.world.hex_map, &game.world.provinces).and_then(|anchor| {
                game.world
                    .sea_zones
                    .iter()
                    .find(|z| z.hexes.iter().any(|h| h.q == anchor.q && h.r == anchor.r))
                    .map(|z| z.id)
            })
        })
    };
    if fallback_zone_id == Some(from_z)
        && let Some(nation) = game.get_nation_mut(nid)
    {
        for ship in &mut nation.military.warships {
            if ship.sea_zone.is_none() {
                ship.sea_zone = Some(from_z);
            }
        }
    }

    let has_ships = game.get_nation(nid).is_some_and(|n| {
        n.military
            .warships
            .iter()
            .any(|s| s.sea_zone == Some(from_z))
    });
    if !has_ships {
        return Err(ApiError::raw(
            "{\"error\":\"no warships in that sea zone\"}",
        ));
    }

    // Replace any existing queued move from the same source zone so the
    // player can retarget without piling up stale entries.
    game.transient
        .pending_fleet_moves
        .retain(|(n, fz, _)| *n != nid || *fz != from_z);
    game.transient.pending_fleet_moves.push((nid, from_z, to_z));

    Ok(())
}

/// Cancel a queued fleet move for `(nation, from_zone_id)` (card #471). No-op
/// if no such pending move exists.
pub fn cancel_fleet_move(
    game: &mut GameState,
    nation_id: u32,
    from_zone_id: u32,
) -> Result<(), ApiError> {
    use domain::map::sea_zones::SeaZoneId;

    let nid = NationId(nation_id);
    let from_z = SeaZoneId(from_zone_id);
    game.transient
        .pending_fleet_moves
        .retain(|(n, fz, _)| *n != nid || *fz != from_z);
    Ok(())
}
