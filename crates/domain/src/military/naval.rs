use crate::military::ships::{Ship, ShipType};
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
}
