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
/// Ships operate globally (simplified: no per-zone movement). Operations
/// are resolved automatically at the end of each turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NavalOperation {
    /// Warship patrols — attacks enemies encountered.
    Patrol,
    /// Warship escorts friendly merchant ships, protecting them from blockade/patrol.
    Escort,
    /// Warship participates in a blockade against a target nation.
    Blockade(NationId),
    /// Warships establish a landing zone (beachhead) on hostile coastline.
    /// Landing force size = total arms cost of all ships in the beachhead fleet.
    Beachhead(NationId),
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
pub fn beachhead_force_size(warships: &[Ship]) -> u32 {
    warships
        .iter()
        .filter(|s| s.ship_type.category() == ShipCategory::Warship)
        .map(|s| s.ship_type.stats().arms_cost)
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
            .map(|s| s.ship_type.stats().firepower)
            .sum();
        let def_fp: u32 = def_ships
            .iter()
            .map(|s| s.ship_type.stats().firepower)
            .sum();

        // Attacker deals damage to defender ships (weakest hull first)
        def_ships.sort_by_key(|s| s.hull_remaining);
        if !def_ships.is_empty() {
            let damage_per_ship = atk_fp / def_ships.len() as u32;
            for ship in &mut def_ships {
                let armor = ship.ship_type.stats().armor;
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
            let damage_per_ship = def_fp / atk_ships.len() as u32;
            for ship in &mut atk_ships {
                let armor = ship.ship_type.stats().armor;
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
    use crate::map::UnitId;
    use crate::military::ships::Ship;

    fn make_ship(id: u32, ship_type: ShipType, owner: NationId) -> Ship {
        Ship::new(UnitId(id), ship_type, owner)
    }

    // ── Naval combat tests ──────────────────────────────────────

    #[test]
    fn attacker_with_more_firepower_wins() {
        let atk = vec![
            make_ship(1, ShipType::ShipOfTheLine, NationId(1)),
            make_ship(2, ShipType::ShipOfTheLine, NationId(1)),
        ];
        let def = vec![make_ship(10, ShipType::Frigate, NationId(2))];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2));
        assert!(result.attacker_won);
        assert!(!result.defender_ships_lost.is_empty());
    }

    #[test]
    fn defender_with_more_firepower_wins() {
        let atk = vec![make_ship(1, ShipType::Frigate, NationId(1))];
        let def = vec![
            make_ship(10, ShipType::ShipOfTheLine, NationId(2)),
            make_ship(11, ShipType::ShipOfTheLine, NationId(2)),
        ];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2));
        assert!(!result.attacker_won);
        assert!(!result.attacker_ships_lost.is_empty());
    }

    #[test]
    fn empty_attacker_loses() {
        let atk: Vec<Ship> = vec![];
        let def = vec![make_ship(10, ShipType::Frigate, NationId(2))];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2));
        assert!(!result.attacker_won);
        assert!(result.attacker_survivors.is_empty());
    }

    #[test]
    fn empty_defender_loses() {
        let atk = vec![make_ship(1, ShipType::Frigate, NationId(1))];
        let def: Vec<Ship> = vec![];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2));
        assert!(result.attacker_won);
        assert!(result.defender_survivors.is_empty());
    }

    #[test]
    fn both_empty_defender_wins_by_default() {
        let result = resolve_naval_battle(&[], &[], NationId(1), NationId(2));
        assert!(!result.attacker_won);
    }

    #[test]
    fn casualties_tracked() {
        let atk = vec![
            make_ship(1, ShipType::ShipOfTheLine, NationId(1)),
            make_ship(2, ShipType::ShipOfTheLine, NationId(1)),
            make_ship(3, ShipType::ShipOfTheLine, NationId(1)),
        ];
        let def = vec![make_ship(10, ShipType::Frigate, NationId(2))];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2));
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
        let atk = vec![
            make_ship(1, ShipType::Frigate, NationId(1)),
            make_ship(2, ShipType::Frigate, NationId(1)),
        ];
        let def = vec![
            make_ship(10, ShipType::Frigate, NationId(2)),
            make_ship(11, ShipType::Frigate, NationId(2)),
        ];

        let result = resolve_naval_battle(&atk, &def, NationId(1), NationId(2));
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
        assert_eq!(beachhead_force_size(&[]), 0);
    }

    #[test]
    fn beachhead_force_size_warships_only() {
        let fleet = vec![
            make_ship(1, ShipType::Frigate, NationId(1)), // arms_cost = 2
            make_ship(2, ShipType::Frigate, NationId(1)), // arms_cost = 2
            make_ship(3, ShipType::ShipOfTheLine, NationId(1)), // arms_cost = 5
        ];
        assert_eq!(beachhead_force_size(&fleet), 2 + 2 + 5);
    }

    #[test]
    fn beachhead_force_size_ignores_merchants() {
        let fleet = vec![
            make_ship(1, ShipType::Frigate, NationId(1)), // arms_cost = 2
            make_ship(2, ShipType::Trader, NationId(1)),  // merchant, arms_cost = 0 but filtered
        ];
        assert_eq!(beachhead_force_size(&fleet), 2);
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
