use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;

use std::sync::atomic::{AtomicU32, Ordering};

/// Global counter for generating unique UnitIds in garrison creation.
static GARRISON_ID_COUNTER: AtomicU32 = AtomicU32::new(1_000_000);

/// Represents a force in combat.
#[derive(Debug, Clone)]
pub struct CombatForce {
    pub nation: NationId,
    pub units: Vec<ArmyUnit>,
}

/// Result of a battle.
#[derive(Debug, Clone)]
pub struct BattleResult {
    pub attacker: NationId,
    pub defender: NationId,
    pub province: ProvinceId,
    pub attacker_won: bool,
    pub attacker_casualties: Vec<ArmyUnitType>,
    pub defender_casualties: Vec<ArmyUnitType>,
    pub attacker_survivors: Vec<ArmyUnit>,
    pub defender_survivors: Vec<ArmyUnit>,
}

/// Calculate total firepower for a list of units.
fn total_firepower(units: &[ArmyUnit]) -> f64 {
    units.iter().map(|u| u.effective_firepower()).sum()
}

/// Count Militia units in a force.
fn militia_count(units: &[ArmyUnit]) -> usize {
    units
        .iter()
        .filter(|u| u.unit_type == ArmyUnitType::Militia)
        .count()
}

/// Resolve a battle between an attacker and a defender in a province.
///
/// Simplified combat resolution:
/// 1. Calculate total attacker firepower: sum of effective_firepower() for all units
/// 2. Calculate total defender firepower: sum of effective_firepower() x 1.2 (defensive bonus)
/// 3. Add garrison bonus: if defender has Militia, each Militia adds 8 firepower
/// 4. Run combat rounds (up to 10 rounds):
///    a. Attacker deals damage proportional to their firepower (damage = total_fp / defender_units.len())
///    b. Defender deals damage proportional to their firepower
///    c. Apply damage to units (weakest units take damage first)
///    d. Remove destroyed units (health <= 0)
///    e. If one side is eliminated, combat ends
/// 5. After rounds: side with more remaining firepower wins
/// 6. Surviving units earn 1 medal each (award_medal())
/// 7. Build BattleResult
pub fn resolve_battle(
    attacker: &CombatForce,
    defender: &CombatForce,
    province: ProvinceId,
) -> BattleResult {
    let mut atk_units = attacker.units.clone();
    let mut def_units = defender.units.clone();

    let mut attacker_casualties: Vec<ArmyUnitType> = Vec::new();
    let mut defender_casualties: Vec<ArmyUnitType> = Vec::new();

    // Handle edge case: both sides empty
    if atk_units.is_empty() && def_units.is_empty() {
        return BattleResult {
            attacker: attacker.nation,
            defender: defender.nation,
            province,
            attacker_won: false,
            attacker_casualties,
            defender_casualties,
            attacker_survivors: atk_units,
            defender_survivors: def_units,
        };
    }

    // Handle edge case: attacker empty
    if atk_units.is_empty() {
        return BattleResult {
            attacker: attacker.nation,
            defender: defender.nation,
            province,
            attacker_won: false,
            attacker_casualties,
            defender_casualties,
            attacker_survivors: atk_units,
            defender_survivors: def_units,
        };
    }

    // Handle edge case: defender empty
    if def_units.is_empty() {
        for unit in &mut atk_units {
            unit.award_medal();
        }
        return BattleResult {
            attacker: attacker.nation,
            defender: defender.nation,
            province,
            attacker_won: true,
            attacker_casualties,
            defender_casualties,
            attacker_survivors: atk_units,
            defender_survivors: def_units,
        };
    }

    // Combat rounds (up to 10)
    for _ in 0..10 {
        if atk_units.is_empty() || def_units.is_empty() {
            break;
        }

        // Calculate firepower for this round
        let atk_fp = total_firepower(&atk_units);
        let def_fp = total_firepower(&def_units) * 1.2 + militia_count(&def_units) as f64 * 8.0;

        // Attacker deals damage to defender units (weakest first)
        if !def_units.is_empty() {
            let damage_per_unit = atk_fp / def_units.len() as f64;
            // Sort defender units by firepower ascending (weakest first)
            def_units.sort_by(|a, b| {
                a.effective_firepower()
                    .partial_cmp(&b.effective_firepower())
                    .unwrap()
            });
            for unit in &mut def_units {
                unit.take_damage(damage_per_unit as u8);
            }
        }

        // Defender deals damage to attacker units (weakest first)
        if !atk_units.is_empty() {
            let damage_per_unit = def_fp / atk_units.len() as f64;
            // Sort attacker units by firepower ascending (weakest first)
            atk_units.sort_by(|a, b| {
                a.effective_firepower()
                    .partial_cmp(&b.effective_firepower())
                    .unwrap()
            });
            for unit in &mut atk_units {
                unit.take_damage(damage_per_unit as u8);
            }
        }

        // Remove destroyed units and record casualties
        let mut i = 0;
        while i < def_units.len() {
            if !def_units[i].is_alive() {
                defender_casualties.push(def_units[i].unit_type);
                def_units.remove(i);
            } else {
                i += 1;
            }
        }

        let mut i = 0;
        while i < atk_units.len() {
            if !atk_units[i].is_alive() {
                attacker_casualties.push(atk_units[i].unit_type);
                atk_units.remove(i);
            } else {
                i += 1;
            }
        }
    }

    // Determine winner: if one side eliminated, the other wins.
    // If both survive, the side with more remaining firepower wins.
    let attacker_won = if def_units.is_empty() && !atk_units.is_empty() {
        true
    } else if atk_units.is_empty() {
        false
    } else {
        // Both sides survive: compare remaining firepower
        let atk_remaining = total_firepower(&atk_units);
        let def_remaining = total_firepower(&def_units);
        atk_remaining > def_remaining
    };

    // Award medals to survivors on the winning side
    if attacker_won {
        for unit in &mut atk_units {
            unit.award_medal();
        }
    } else {
        for unit in &mut def_units {
            unit.award_medal();
        }
    }

    BattleResult {
        attacker: attacker.nation,
        defender: defender.nation,
        province,
        attacker_won,
        attacker_casualties,
        defender_casualties,
        attacker_survivors: atk_units,
        defender_survivors: def_units,
    }
}

/// Creates the starting garrison for a province.
///
/// - Great Power: 4 Militia units
/// - Minor Nation: 3 Militia units
///
/// Each unit gets a unique UnitId from an atomic counter.
pub fn create_garrison(nation_type: NationType) -> Vec<ArmyUnit> {
    use crate::map::UnitId;

    let count = match nation_type {
        NationType::GreatPower => 4,
        NationType::MinorNation => 3,
    };

    (0..count)
        .map(|_| {
            let id = GARRISON_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
            ArmyUnit::new(
                UnitId(id),
                ArmyUnitType::Militia,
                NationId(0),   // placeholder — caller should set owner
                ProvinceId(0), // placeholder — caller should set position
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::UnitId;

    /// Helper: create units for a force.
    fn make_unit(id: u32, unit_type: ArmyUnitType, nation: NationId) -> ArmyUnit {
        ArmyUnit::new(UnitId(id), unit_type, nation, ProvinceId(1))
    }

    fn make_force(nation: NationId, units: Vec<ArmyUnit>) -> CombatForce {
        CombatForce { nation, units }
    }

    // ── Large attacker force beats small defender ────────────────

    #[test]
    fn large_attacker_beats_small_defender() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Guards, atk_nation),
                make_unit(3, ArmyUnitType::Guards, atk_nation),
                make_unit(4, ArmyUnitType::SiegeArtillery, atk_nation),
                make_unit(5, ArmyUnitType::SiegeArtillery, atk_nation),
            ],
        );

        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Militia, def_nation)],
        );

        let result = resolve_battle(&attacker, &defender, ProvinceId(1));
        assert!(result.attacker_won);
        assert!(!result.defender_casualties.is_empty());
        assert!(result.defender_survivors.is_empty());
    }

    // ── Defender with garrison bonus is stronger ─────────────────

    #[test]
    fn defender_garrison_bonus_makes_defender_stronger() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // Small attacker force
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Regulars, atk_nation),
                make_unit(2, ArmyUnitType::Regulars, atk_nation),
            ],
        );

        // Defender with militia garrison
        let defender = make_force(
            def_nation,
            vec![
                make_unit(10, ArmyUnitType::Militia, def_nation),
                make_unit(11, ArmyUnitType::Militia, def_nation),
                make_unit(12, ArmyUnitType::Militia, def_nation),
                make_unit(13, ArmyUnitType::Militia, def_nation),
            ],
        );

        let result = resolve_battle(&attacker, &defender, ProvinceId(1));
        // 4 militia with garrison bonus (each adds 8fp) + 1.2x defensive modifier
        // should overwhelm 2 regulars
        assert!(
            !result.attacker_won,
            "Defender with garrison bonus should win against small attacker"
        );
    }

    // ── Defensive 1.2x modifier makes close fights favor defender ─

    #[test]
    fn defensive_modifier_favors_defender_in_close_fight() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // Equal-strength forces without garrison
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Regulars, atk_nation),
                make_unit(2, ArmyUnitType::Regulars, atk_nation),
                make_unit(3, ArmyUnitType::Regulars, atk_nation),
            ],
        );

        let defender = make_force(
            def_nation,
            vec![
                make_unit(10, ArmyUnitType::Regulars, def_nation),
                make_unit(11, ArmyUnitType::Regulars, def_nation),
                make_unit(12, ArmyUnitType::Regulars, def_nation),
            ],
        );

        let result = resolve_battle(&attacker, &defender, ProvinceId(1));
        // Identical forces, but defender gets 1.2x modifier
        assert!(
            !result.attacker_won,
            "Equal forces should favor defender due to 1.2x modifier"
        );
    }

    // ── Casualties are tracked correctly ─────────────────────────

    #[test]
    fn casualties_tracked_correctly() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::SiegeArtillery, atk_nation),
                make_unit(2, ArmyUnitType::SiegeArtillery, atk_nation),
                make_unit(3, ArmyUnitType::SiegeArtillery, atk_nation),
            ],
        );

        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Militia, def_nation)],
        );

        let result = resolve_battle(&attacker, &defender, ProvinceId(1));
        assert!(result.attacker_won);
        // The militia should be destroyed
        assert!(
            result.defender_casualties.contains(&ArmyUnitType::Militia),
            "Defender militia should appear in casualties"
        );
        // Total casualties + survivors should equal original force size
        assert_eq!(
            result.defender_casualties.len() + result.defender_survivors.len(),
            1
        );
        assert_eq!(
            result.attacker_casualties.len() + result.attacker_survivors.len(),
            3
        );
    }

    // ── Survivors earn medals ────────────────────────────────────

    #[test]
    fn survivors_earn_medals() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Guards, atk_nation),
                make_unit(3, ArmyUnitType::Guards, atk_nation),
                make_unit(4, ArmyUnitType::SiegeArtillery, atk_nation),
            ],
        );

        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Militia, def_nation)],
        );

        let result = resolve_battle(&attacker, &defender, ProvinceId(1));
        assert!(result.attacker_won);

        // All surviving attackers should have at least 1 medal
        for unit in &result.attacker_survivors {
            assert!(
                unit.medals >= 1,
                "Surviving attacker unit {:?} should have at least 1 medal, got {}",
                unit.unit_type,
                unit.medals
            );
        }
    }

    // ── Garrison creation ────────────────────────────────────────

    #[test]
    fn garrison_great_power_creates_4_militia() {
        let garrison = create_garrison(NationType::GreatPower);
        assert_eq!(garrison.len(), 4);
        for unit in &garrison {
            assert_eq!(unit.unit_type, ArmyUnitType::Militia);
            assert_eq!(unit.health, 100);
        }
    }

    #[test]
    fn garrison_minor_nation_creates_3_militia() {
        let garrison = create_garrison(NationType::MinorNation);
        assert_eq!(garrison.len(), 3);
        for unit in &garrison {
            assert_eq!(unit.unit_type, ArmyUnitType::Militia);
            assert_eq!(unit.health, 100);
        }
    }

    #[test]
    fn garrison_units_have_unique_ids() {
        let garrison = create_garrison(NationType::GreatPower);
        let ids: Vec<_> = garrison.iter().map(|u| u.id).collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "Garrison units must have unique IDs");
            }
        }
    }

    // ── Empty forces edge case ──────────────────────────────────

    #[test]
    fn empty_forces_defender_wins_by_default() {
        let attacker = make_force(NationId(1), vec![]);
        let defender = make_force(NationId(2), vec![]);

        let result = resolve_battle(&attacker, &defender, ProvinceId(1));
        assert!(!result.attacker_won);
        assert!(result.attacker_casualties.is_empty());
        assert!(result.defender_casualties.is_empty());
    }

    #[test]
    fn empty_attacker_loses() {
        let attacker = make_force(NationId(1), vec![]);
        let defender = make_force(
            NationId(2),
            vec![make_unit(10, ArmyUnitType::Militia, NationId(2))],
        );

        let result = resolve_battle(&attacker, &defender, ProvinceId(1));
        assert!(!result.attacker_won);
        assert!(result.attacker_survivors.is_empty());
    }

    #[test]
    fn empty_defender_loses() {
        let attacker = make_force(
            NationId(1),
            vec![make_unit(1, ArmyUnitType::Regulars, NationId(1))],
        );
        let defender = make_force(NationId(2), vec![]);

        let result = resolve_battle(&attacker, &defender, ProvinceId(1));
        assert!(result.attacker_won);
        assert!(result.defender_survivors.is_empty());
        // Attacker survivors should earn medals
        assert_eq!(result.attacker_survivors.len(), 1);
        assert!(result.attacker_survivors[0].medals >= 1);
    }
}
