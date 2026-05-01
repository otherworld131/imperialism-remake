use crate::game_state::GameState;
use crate::map::sea_zones::SeaZoneId;
use crate::military::ships::{Ship, ShipCategory, ShipType};
use crate::types::*;

#[derive(Debug, Clone)]
pub struct NavalBattleResult {
    pub attacker: NationId,
    pub defender: NationId,
    pub attacker_won: bool,
    pub attacker_ships_lost: Vec<ShipType>,
    pub defender_ships_lost: Vec<ShipType>,
    pub attacker_survivors: Vec<Ship>,
    pub defender_survivors: Vec<Ship>,
}

// ── Naval Operations ────────────────────────────────────────────

/// The type of naval operation a warship can be ordered to perform.
///
/// Ships occupy sea zones and may be moved through the sea-zone graph during
/// the turn. Operations are resolved automatically at the end of each turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavalOperation {
    /// Warship patrols — attacks enemies encountered.
    Patrol,
    /// Warship escorts friendly merchant ships, protecting them from blockade/patrol.
    Escort,
    /// Warship participates in a blockade against a target nation.
    Blockade(NationId),
    /// Warships establish a landing zone (beachhead) on a specific hostile coastal province.
    /// Landing force size = total arms cost of all ships in the beachhead fleet.
    /// The province must be coastal and owned by a nation the attacker is at war with.
    Beachhead(ProvinceId),
    /// Warship performs reconnaissance on an enemy nation: estimates ground forces.
    Reconnaissance(NationId),
}

/// Result of a naval reconnaissance operation.
#[derive(Debug, Clone)]
pub struct ReconResult {
    /// The nation performing reconnaissance.
    pub observer: NationId,
    /// The target nation being observed.
    pub target: NationId,
    /// Estimated enemy army strength (provinces * garrison_estimate + known army size).
    pub estimated_strength: u32,
    /// Number of coastal provinces the enemy has.
    pub coastal_provinces: u32,
}

/// Calculate the beachhead landing force size for a fleet of warships.
///
/// Landing force size = total arms_cost used to build all ships in the fleet.
/// This matches the design: "Landing force size = total arms used to build all ships."
pub fn beachhead_force_size(warships: &[Ship], data: &crate::data::GameData) -> u32 {
    warships
        .iter()
        .filter(|s| s.ship_type.category() == ShipCategory::Warship)
        .map(|s| data.ship_stats(s.ship_type).arms_cost)
        .sum()
}

/// Calculate how many escort warships are needed to fully protect a merchant fleet.
///
/// Each escort warship protects 2 cargo capacity from blockade. Returns the number
/// of escort warships whose firepower counteracts the blockade effect.
pub fn escort_protection(escort_count: u32, enemy_warship_count: u32) -> u32 {
    // Each escort neutralizes one enemy warship's blockade effect
    escort_count.min(enemy_warship_count)
}

/// Calculate effective blockade impact with escorts.
///
/// Escorts reduce the effective number of enemy warships blocking trade.
/// Each escorting warship neutralizes one enemy blockading warship.
pub fn blockade_with_escorts(
    merchant_cargo: u32,
    enemy_warship_count: u32,
    escort_count: u32,
) -> u32 {
    let effective_enemy = enemy_warship_count.saturating_sub(escort_count);
    calculate_blockade_effect(merchant_cargo, effective_enemy)
}

/// Find a nation's default ocean sea zone for newly-deployed ships.
///
/// Prefers the capital's adjacent ocean zone, then falls back to any owned
/// ocean-coastal province. Returns `None` for landlocked nations.
pub(crate) fn find_nation_home_sea_zone(
    game: &GameState,
    nation_id: NationId,
) -> Option<SeaZoneId> {
    let nation = game.get_nation(nation_id)?;
    let capital_pid = nation.capital_province_id;
    let pids: Vec<_> = std::iter::once(capital_pid)
        .chain(nation.province_ids.iter().copied())
        .collect();
    for pid in pids {
        let province = game.get_province(pid)?;
        if !province.ocean_coastal {
            continue;
        }
        for zone in &game.world.sea_zones {
            if !zone.is_lake && zone.coastal_provinces.contains(&pid) {
                return Some(zone.id);
            }
        }
    }
    None
}

/// Move all warships in a sea zone one adjacent sea zone while preserving the
/// per-zone fleet movement budget semantics used by the frontend.
///
/// Returns `true` if at least one ship moved.
pub(crate) fn move_warship_group_one_zone(
    game: &mut GameState,
    nation_id: NationId,
    from_z: SeaZoneId,
    to_z: SeaZoneId,
) -> bool {
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
        return false;
    }
    let adjacent = game
        .world
        .sea_zones
        .iter()
        .find(|z| z.id == from_z)
        .map(|z| z.is_adjacent_to(to_z))
        .unwrap_or(false);
    if !adjacent {
        return false;
    }

    let Some(nation) = game.get_nation(nation_id) else {
        return false;
    };
    let moving_ship_count = nation
        .military
        .warships
        .iter()
        .filter(|ship| ship.sea_zone == Some(from_z))
        .count();
    if moving_ship_count == 0 {
        return false;
    }

    let budget = if let Some(&rem) = nation.military.fleet_moves_remaining.get(&from_z) {
        rem
    } else {
        nation
            .military
            .warships
            .iter()
            .filter(|ship| ship.sea_zone == Some(from_z))
            .map(|ship| game.game_data.ship_stats(ship.ship_type).speed)
            .filter(|&speed| speed > 0)
            .min()
            .unwrap_or(0)
    };
    if budget == 0 {
        return false;
    }

    let remaining = budget - 1;
    let dest_budget = nation
        .military
        .fleet_moves_remaining
        .get(&to_z)
        .copied()
        .unwrap_or_else(|| {
            nation
                .military
                .warships
                .iter()
                .filter(|ship| ship.sea_zone == Some(to_z))
                .map(|ship| game.game_data.ship_stats(ship.ship_type).speed)
                .filter(|&speed| speed > 0)
                .min()
                .unwrap_or(u32::MAX)
        });

    let Some(nation) = game.get_nation_mut(nation_id) else {
        return false;
    };
    for ship in &mut nation.military.warships {
        if ship.sea_zone == Some(from_z) {
            ship.sea_zone = Some(to_z);
        }
    }

    let source_has_leftovers = nation
        .military
        .warships
        .iter()
        .any(|ship| ship.sea_zone == Some(from_z));
    if source_has_leftovers {
        nation.military.fleet_moves_remaining.insert(from_z, remaining);
    } else {
        nation.military.fleet_moves_remaining.remove(&from_z);
    }
    nation
        .military
        .fleet_moves_remaining
        .insert(to_z, remaining.min(dest_budget));
    true
}

/// Card #408: compute the set of port tiles owned by `nation_id` that are
/// blockaded — i.e. adjacent to a sea zone where a hostile fleet is present
/// AND the owner has zero warships. Country-capital tiles that act as
/// implicit ports (card #419) are included when sea-adjacent.
///
/// "Hostile fleet" = any warship of a nation currently at war with `nation_id`,
/// excluding warships of anarchic nations (consistent with the trade-blockade
/// rule). Ships with no sea zone assigned (`sea_zone == None`) are ignored,
/// since unzoned ships represent transient state and do not exert blockade.
pub fn compute_blockaded_ports(
    game: &GameState,
    nation_id: NationId,
) -> std::collections::HashSet<crate::hex::HexCoord> {
    use std::collections::{HashMap, HashSet};

    let mut out: HashSet<crate::hex::HexCoord> = HashSet::new();
    if game.world.sea_zones.is_empty() {
        return out;
    }
    let Some(nation) = game.get_nation(nation_id) else {
        return out;
    };
    if nation.diplomacy.is_in_anarchy {
        return out;
    }

    // Per-zone enemy warship counts (only zones that hold a hostile ship).
    let mut hostile_per_zone: HashMap<SeaZoneId, u32> = HashMap::new();
    for other in &game.world.nations {
        if other.id == nation_id || other.diplomacy.is_in_anarchy {
            continue;
        }
        let hostile = game
            .world
            .diplomacy
            .get_relation(nation_id, other.id)
            .is_some_and(|r| r.hostilities_active_on(game.turn));
        if !hostile {
            continue;
        }
        for ship in &other.military.warships {
            if let Some(zid) = ship.sea_zone {
                *hostile_per_zone.entry(zid).or_insert(0) += 1;
            }
        }
    }
    if hostile_per_zone.is_empty() {
        return out;
    }

    // Per-zone friendly warship counts.
    let mut friendly_per_zone: HashMap<SeaZoneId, u32> = HashMap::new();
    for ship in &nation.military.warships {
        if let Some(zid) = ship.sea_zone {
            *friendly_per_zone.entry(zid).or_insert(0) += 1;
        }
    }

    // For each owned province, walk every tile that acts as a port (built
    // port, or country-capital adjacent to sea). Mark blockaded if any
    // adjacent ocean zone has hostile ships and zero friendlies.
    for &pid in &nation.province_ids {
        let Some(province) = game.get_province(pid) else {
            continue;
        };
        for &tile_coord in &province.tiles {
            let Some(tile) = game.world.hex_map.get_tile(tile_coord) else {
                continue;
            };
            let acts_as_port = tile.infrastructure.has_port
                || (tile.is_country_capital
                    && tile_coord.neighbors().iter().any(|n| {
                        game.world
                            .hex_map
                            .get_tile(*n)
                            .is_some_and(|t| !t.terrain().is_land())
                    }));
            if !acts_as_port {
                continue;
            }
            let zones =
                crate::map::sea_zones::ocean_zones_adjacent_to_hex(&game.world.sea_zones, tile_coord);
            if zones.is_empty() {
                continue;
            }
            // F-002 review fix: a port is blockaded only when it has no open
            // ocean approach — i.e. EVERY adjacent zone is hostile-undefended
            // and at least one zone actually has hostile fleet presence. A
            // port touching two zones with one open lane stays connected.
            let any_hostile_present = zones.iter().any(|zid| {
                hostile_per_zone.get(zid).copied().unwrap_or(0) > 0
            });
            if !any_hostile_present {
                continue;
            }
            let all_zones_blockaded = zones.iter().all(|zid| {
                let hostile = hostile_per_zone.get(zid).copied().unwrap_or(0);
                let friendly = friendly_per_zone.get(zid).copied().unwrap_or(0);
                hostile > 0 && friendly == 0
            });
            if all_zones_blockaded {
                out.insert(tile_coord);
            }
        }
    }

    out
}

/// Perform naval reconnaissance: estimate enemy ground forces.
///
/// Returns an estimated strength based on provinces owned and army size.
/// Each province is estimated to have ~4 garrison strength, plus known army units.
pub fn naval_reconnaissance(
    observer: NationId,
    target: NationId,
    target_province_count: usize,
    target_army_size: usize,
    target_coastal_provinces: usize,
) -> ReconResult {
    let estimated_strength = (target_province_count * 4 + target_army_size) as u32;
    ReconResult {
        observer,
        target,
        estimated_strength,
        coastal_provinces: target_coastal_provinces as u32,
    }
}

/// Resolve a naval battle (always AI-controlled, never player tactical).
///
/// Combat resolution:
/// 1. Calculate total firepower for each side (armor reduces damage).
/// 2. Run up to 5 rounds of combat.
/// 3. Each round, damage is distributed to ships (weakest hull first).
/// 4. Armor reduces incoming damage per ship.
/// 5. Ships with hull <= 0 are sunk and recorded as losses.
/// 6. Winner = side with more remaining hull points after rounds.
pub fn resolve_naval_battle(
    attacker_ships: &[Ship],
    defender_ships: &[Ship],
    attacker_id: NationId,
    defender_id: NationId,
    data: &crate::data::GameData,
) -> NavalBattleResult {
    let mut atk_ships: Vec<Ship> = attacker_ships.to_vec();
    let mut def_ships: Vec<Ship> = defender_ships.to_vec();
    let mut attacker_ships_lost: Vec<ShipType> = Vec::new();
    let mut defender_ships_lost: Vec<ShipType> = Vec::new();

    // Edge cases
    if atk_ships.is_empty() && def_ships.is_empty() {
        return NavalBattleResult {
            attacker: attacker_id,
            defender: defender_id,
            attacker_won: false,
            attacker_ships_lost,
            defender_ships_lost,
            attacker_survivors: atk_ships,
            defender_survivors: def_ships,
        };
    }
    if atk_ships.is_empty() {
        return NavalBattleResult {
            attacker: attacker_id,
            defender: defender_id,
            attacker_won: false,
            attacker_ships_lost,
            defender_ships_lost,
            attacker_survivors: atk_ships,
            defender_survivors: def_ships,
        };
    }
    if def_ships.is_empty() {
        return NavalBattleResult {
            attacker: attacker_id,
            defender: defender_id,
            attacker_won: true,
            attacker_ships_lost,
            defender_ships_lost,
            attacker_survivors: atk_ships,
            defender_survivors: def_ships,
        };
    }

    // Combat rounds (up to 5)
    for _ in 0..5 {
        if atk_ships.is_empty() || def_ships.is_empty() {
            break;
        }

        // Calculate total firepower for each side
        let atk_fp: u32 = atk_ships
            .iter()
            .map(|s| data.ship_stats(s.ship_type).firepower)
            .sum();
        let def_fp: u32 = def_ships
            .iter()
            .map(|s| data.ship_stats(s.ship_type).firepower)
            .sum();

        // Attacker deals damage to defender ships (weakest hull first)
        // Distribute remainder to first N ships to avoid truncation loss
        def_ships.sort_by_key(|s| s.hull_remaining);
        if !def_ships.is_empty() {
            let n = def_ships.len() as u32;
            let base_damage = atk_fp / n;
            let remainder = atk_fp % n;
            for (i, ship) in def_ships.iter_mut().enumerate() {
                let damage_per_ship = base_damage + if (i as u32) < remainder { 1 } else { 0 };
                let armor = data.ship_stats(ship.ship_type).armor;
                let effective_damage = damage_per_ship.saturating_sub(armor / 5);
                // Always deal at least 1 damage if there is any firepower
                let actual_damage = if atk_fp > 0 && effective_damage == 0 {
                    1
                } else {
                    effective_damage
                };
                ship.take_damage(actual_damage);
            }
        }

        // Defender deals damage to attacker ships (weakest hull first)
        atk_ships.sort_by_key(|s| s.hull_remaining);
        if !atk_ships.is_empty() {
            let n = atk_ships.len() as u32;
            let base_damage = def_fp / n;
            let remainder = def_fp % n;
            for (i, ship) in atk_ships.iter_mut().enumerate() {
                let damage_per_ship = base_damage + if (i as u32) < remainder { 1 } else { 0 };
                let armor = data.ship_stats(ship.ship_type).armor;
                let effective_damage = damage_per_ship.saturating_sub(armor / 5);
                let actual_damage = if def_fp > 0 && effective_damage == 0 {
                    1
                } else {
                    effective_damage
                };
                ship.take_damage(actual_damage);
            }
        }

        // Remove sunk ships
        let mut i = 0;
        while i < def_ships.len() {
            if def_ships[i].is_sunk() {
                defender_ships_lost.push(def_ships[i].ship_type);
                def_ships.remove(i);
            } else {
                i += 1;
            }
        }

        let mut i = 0;
        while i < atk_ships.len() {
            if atk_ships[i].is_sunk() {
                attacker_ships_lost.push(atk_ships[i].ship_type);
                atk_ships.remove(i);
            } else {
                i += 1;
            }
        }
    }

    // Determine winner: side with more remaining hull points
    let attacker_won = if def_ships.is_empty() && !atk_ships.is_empty() {
        true
    } else if atk_ships.is_empty() {
        false
    } else {
        let atk_hull: u32 = atk_ships.iter().map(|s| s.hull_remaining).sum();
        let def_hull: u32 = def_ships.iter().map(|s| s.hull_remaining).sum();
        atk_hull > def_hull
    };

    NavalBattleResult {
        attacker: attacker_id,
        defender: defender_id,
        attacker_won,
        attacker_ships_lost,
        defender_ships_lost,
        attacker_survivors: atk_ships,
        defender_survivors: def_ships,
    }
}

/// Check if a nation's merchant ships are blockaded.
/// Blockade: if an enemy warship is in the same sea zone as merchant ships.
/// Simplified: if at war and enemy has warships, some merchant cargo is lost.
///
/// Each enemy warship blocks 2 cargo capacity.
/// Returns reduced effective cargo capacity.
pub fn calculate_blockade_effect(merchant_cargo: u32, enemy_warship_count: u32) -> u32 {
    let blocked = enemy_warship_count * 2;
    merchant_cargo.saturating_sub(blocked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::map::UnitId;
    use crate::military::ships::Ship;

    fn test_data() -> GameData {
        GameData::default()
    }

    fn make_ship(id: u32, ship_type: ShipType, owner: NationId) -> Ship {
        let data = test_data();
        Ship::with_data(UnitId(id), ship_type, owner, &data)
    }

    // ── Naval combat tests ──────────────────────────────────────

    #[test]
    fn attacker_with_more_firepower_wins() {
        let data = test_data();
        let atk = vec![
            make_ship(1, ShipType::ShipOfTheLine, NationId(1)),
            make_ship(2, ShipType::ShipOfTheLine, NationId(1)),
        ];
        let def = vec![make_ship(10, ShipType::Frigate, NationId(2))];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2), &data);
        assert!(result.attacker_won);
        assert!(!result.defender_ships_lost.is_empty());
    }

    #[test]
    fn defender_with_more_firepower_wins() {
        let data = test_data();
        let atk = vec![make_ship(1, ShipType::Frigate, NationId(1))];
        let def = vec![
            make_ship(10, ShipType::ShipOfTheLine, NationId(2)),
            make_ship(11, ShipType::ShipOfTheLine, NationId(2)),
        ];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2), &data);
        assert!(!result.attacker_won);
        assert!(!result.attacker_ships_lost.is_empty());
    }

    #[test]
    fn empty_attacker_loses() {
        let data = test_data();
        let atk: Vec<Ship> = vec![];
        let def = vec![make_ship(10, ShipType::Frigate, NationId(2))];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2), &data);
        assert!(!result.attacker_won);
        assert!(result.attacker_survivors.is_empty());
    }

    #[test]
    fn empty_defender_loses() {
        let data = test_data();
        let atk = vec![make_ship(1, ShipType::Frigate, NationId(1))];
        let def: Vec<Ship> = vec![];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2), &data);
        assert!(result.attacker_won);
        assert!(result.defender_survivors.is_empty());
    }

    #[test]
    fn both_empty_defender_wins_by_default() {
        let data = test_data();
        let result = resolve_naval_battle(&[], &[], NationId(1), NationId(2), &data);
        assert!(!result.attacker_won);
    }

    #[test]
    fn casualties_tracked() {
        let data = test_data();
        let atk = vec![
            make_ship(1, ShipType::ShipOfTheLine, NationId(1)),
            make_ship(2, ShipType::ShipOfTheLine, NationId(1)),
            make_ship(3, ShipType::ShipOfTheLine, NationId(1)),
        ];
        let def = vec![make_ship(10, ShipType::Frigate, NationId(2))];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2), &data);
        assert!(result.attacker_won);
        // The single frigate should be destroyed
        assert!(
            result.defender_ships_lost.contains(&ShipType::Frigate),
            "Defender frigate should be in casualties"
        );
        assert_eq!(
            result.defender_ships_lost.len() + result.defender_survivors.len(),
            1
        );
        assert_eq!(
            result.attacker_ships_lost.len() + result.attacker_survivors.len(),
            3
        );
    }

    #[test]
    fn equal_forces_battle_resolves() {
        let data = test_data();
        let atk = vec![
            make_ship(1, ShipType::Frigate, NationId(1)),
            make_ship(2, ShipType::Frigate, NationId(1)),
        ];
        let def = vec![
            make_ship(10, ShipType::Frigate, NationId(2)),
            make_ship(11, ShipType::Frigate, NationId(2)),
        ];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2), &data);
        // Both sides equally matched - result is deterministic but either side could win
        // Just verify the battle resolved without panic
        let total_atk = result.attacker_ships_lost.len() + result.attacker_survivors.len();
        let total_def = result.defender_ships_lost.len() + result.defender_survivors.len();
        assert_eq!(total_atk, 2);
        assert_eq!(total_def, 2);
    }

    // ── Blockade tests ──────────────────────────────────────────

    #[test]
    fn no_blockade_when_no_enemy_warships() {
        assert_eq!(calculate_blockade_effect(10, 0), 10);
    }

    #[test]
    fn each_warship_blocks_two_cargo() {
        assert_eq!(calculate_blockade_effect(10, 1), 8);
        assert_eq!(calculate_blockade_effect(10, 2), 6);
        assert_eq!(calculate_blockade_effect(10, 3), 4);
    }

    #[test]
    fn blockade_cannot_go_negative() {
        assert_eq!(calculate_blockade_effect(4, 5), 0);
        assert_eq!(calculate_blockade_effect(0, 3), 0);
    }

    #[test]
    fn full_blockade() {
        assert_eq!(calculate_blockade_effect(6, 3), 0);
    }

    // ── Beachhead force size ────────────────────────────────────

    #[test]
    fn beachhead_force_size_empty() {
        let data = test_data();
        assert_eq!(beachhead_force_size(&[], &data), 0);
    }

    #[test]
    fn beachhead_force_size_warships_only() {
        let data = test_data();
        let fleet = vec![
            make_ship(1, ShipType::Frigate, NationId(1)), // arms_cost = 2
            make_ship(2, ShipType::Frigate, NationId(1)), // arms_cost = 2
            make_ship(3, ShipType::ShipOfTheLine, NationId(1)), // arms_cost = 5
        ];
        assert_eq!(beachhead_force_size(&fleet, &data), 2 + 2 + 5);
    }

    #[test]
    fn beachhead_force_size_ignores_merchants() {
        let data = test_data();
        let fleet = vec![
            make_ship(1, ShipType::Frigate, NationId(1)), // arms_cost = 2
            make_ship(2, ShipType::Trader, NationId(1)),  // merchant, arms_cost = 0 but filtered
        ];
        assert_eq!(beachhead_force_size(&fleet, &data), 2);
    }

    // ── Escort protection ──────────────────────────────────────

    #[test]
    fn escort_neutralizes_enemy() {
        assert_eq!(escort_protection(2, 3), 2);
        assert_eq!(escort_protection(5, 3), 3);
        assert_eq!(escort_protection(0, 3), 0);
    }

    // ── Blockade with escorts ──────────────────────────────────

    #[test]
    fn blockade_reduced_by_escorts() {
        // 10 cargo, 3 enemy warships (block 6), 2 escorts neutralize 2 enemies
        // Effective enemy = 1, blocks 2 cargo => 8
        assert_eq!(blockade_with_escorts(10, 3, 2), 8);
    }

    #[test]
    fn full_escort_negates_blockade() {
        assert_eq!(blockade_with_escorts(10, 3, 3), 10);
        assert_eq!(blockade_with_escorts(10, 3, 5), 10);
    }

    #[test]
    fn no_escorts_same_as_regular_blockade() {
        assert_eq!(
            blockade_with_escorts(10, 3, 0),
            calculate_blockade_effect(10, 3)
        );
    }

    // ── Reconnaissance ─────────────────────────────────────────

    #[test]
    fn reconnaissance_estimates_strength() {
        let result = naval_reconnaissance(NationId(1), NationId(2), 5, 3, 2);
        assert_eq!(result.observer, NationId(1));
        assert_eq!(result.target, NationId(2));
        assert_eq!(result.estimated_strength, 5 * 4 + 3);
        assert_eq!(result.coastal_provinces, 2);
    }

    #[test]
    fn reconnaissance_zero_forces() {
        let result = naval_reconnaissance(NationId(1), NationId(2), 0, 0, 0);
        assert_eq!(result.estimated_strength, 0);
        assert_eq!(result.coastal_provinces, 0);
    }

    // ── Naval operation enum ───────────────────────────────────

    #[test]
    fn naval_operation_equality() {
        assert_eq!(NavalOperation::Patrol, NavalOperation::Patrol);
        assert_eq!(NavalOperation::Escort, NavalOperation::Escort);
        assert_eq!(
            NavalOperation::Blockade(NationId(1)),
            NavalOperation::Blockade(NationId(1))
        );
        assert_ne!(
            NavalOperation::Blockade(NationId(1)),
            NavalOperation::Blockade(NationId(2))
        );
        assert_ne!(NavalOperation::Patrol, NavalOperation::Escort);
    }
}
