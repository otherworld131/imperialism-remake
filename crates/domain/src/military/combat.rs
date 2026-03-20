use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;

use std::sync::atomic::{AtomicU32, Ordering};

/// Calculate defense bonus percentage from terrain type.
///
/// Mountain: +50%, Hills (FertileHills/BarrenHills): +30%, Forest: +20%,
/// Swamp: +15%, all others: 0%.
pub fn terrain_defense_bonus(terrain: TerrainType) -> f64 {
    match terrain {
        TerrainType::Mountain => 0.50,
        TerrainType::FertileHills | TerrainType::BarrenHills => 0.30,
        TerrainType::HardwoodForest | TerrainType::ScrubForest => 0.20,
        TerrainType::Swamp => 0.15,
        _ => 0.0,
    }
}

/// Calculate defense bonus multiplier from fort level.
///
/// Level 0: no bonus, Level 1: +20%, Level 2: +40%, Level 3: +60%.
pub fn fort_defense_bonus(fort_level: u8) -> f64 {
    match fort_level {
        1 => 0.20,
        2 => 0.40,
        3 => 0.60,
        _ => 0.0,
    }
}

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
    /// The terrain at the battle site, if known.
    pub terrain: Option<TerrainType>,
    /// The fort level at the battle site (0 = no fort).
    pub fort_level: u8,
    /// Total attacker firepower at the start of battle.
    pub attacker_initial_fp: f64,
    /// Total defender firepower at the start of battle (including bonuses).
    pub defender_initial_fp: f64,
    /// Number of attacker units at start.
    pub attacker_initial_count: usize,
    /// Number of defender units at start.
    pub defender_initial_count: usize,
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
/// Combat resolution:
/// 1. Calculate total attacker firepower: sum of effective_firepower() for all units
/// 2. Calculate total defender firepower: sum of effective_firepower() x 1.2 (defensive bonus)
/// 3. Apply terrain bonus: multiply defender FP by (1.0 + terrain_defense_bonus)
/// 4. Apply fort bonus: multiply defender FP by (1.0 + fort_defense_bonus)
/// 5. Add garrison bonus: if defender has Militia, each Militia adds 8 firepower
/// 6. Run combat rounds (up to 10 rounds):
///    a. Attacker deals damage proportional to their firepower (damage = total_fp / defender_units.len())
///    b. Defender deals damage proportional to their firepower
///    c. Apply damage to units (weakest units take damage first)
///    d. Remove destroyed units (health <= 0)
///    e. If one side is eliminated, combat ends
/// 7. After rounds: side with more remaining firepower wins
/// 8. Surviving units earn 1 medal each (award_medal())
/// 9. Build BattleResult
pub fn resolve_battle(
    attacker: &CombatForce,
    defender: &CombatForce,
    province: ProvinceId,
    terrain: Option<TerrainType>,
    fort_level: u8,
) -> BattleResult {
    let mut atk_units = attacker.units.clone();
    let mut def_units = defender.units.clone();

    let attacker_initial_count = atk_units.len();
    let defender_initial_count = def_units.len();
    let attacker_initial_fp = total_firepower(&atk_units);
    let terrain_bonus_init = terrain.map(terrain_defense_bonus).unwrap_or(0.0);
    let fort_bonus_init = fort_defense_bonus(fort_level);
    let defender_initial_fp =
        total_firepower(&def_units) * 1.2 * (1.0 + terrain_bonus_init) * (1.0 + fort_bonus_init)
            + militia_count(&def_units) as f64 * 8.0;

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
            terrain,
            fort_level,
            attacker_initial_fp,
            defender_initial_fp,
            attacker_initial_count,
            defender_initial_count,
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
            terrain,
            fort_level,
            attacker_initial_fp,
            defender_initial_fp,
            attacker_initial_count,
            defender_initial_count,
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
            terrain,
            fort_level,
            attacker_initial_fp,
            defender_initial_fp,
            attacker_initial_count,
            defender_initial_count,
        };
    }

    // Combat rounds (up to 10)
    for _ in 0..10 {
        if atk_units.is_empty() || def_units.is_empty() {
            break;
        }

        // Calculate firepower for this round
        let atk_fp = total_firepower(&atk_units);
        let terrain_bonus = terrain.map(terrain_defense_bonus).unwrap_or(0.0);
        let fort_bonus = fort_defense_bonus(fort_level);
        let def_fp = total_firepower(&def_units) * 1.2 * (1.0 + terrain_bonus) * (1.0 + fort_bonus)
            + militia_count(&def_units) as f64 * 8.0;

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
        terrain,
        fort_level,
        attacker_initial_fp,
        defender_initial_fp,
        attacker_initial_count,
        defender_initial_count,
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

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
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

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
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

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
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

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
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

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
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

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
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

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
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

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        assert!(result.attacker_won);
        assert!(result.defender_survivors.is_empty());
        // Attacker survivors should earn medals
        assert_eq!(result.attacker_survivors.len(), 1);
        assert!(result.attacker_survivors[0].medals >= 1);
    }

    // ── Terrain defense bonus ───────────────────────────────────

    #[test]
    fn terrain_defense_bonus_values() {
        use super::terrain_defense_bonus;

        assert!((terrain_defense_bonus(TerrainType::Mountain) - 0.50).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::FertileHills) - 0.30).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::BarrenHills) - 0.30).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::HardwoodForest) - 0.20).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::ScrubForest) - 0.20).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Swamp) - 0.15).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Farm) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::DryPlains) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Sea) - 0.0).abs() < f64::EPSILON);
    }

    // ── Fort defense bonus ──────────────────────────────────────

    #[test]
    fn fort_defense_bonus_values() {
        use super::fort_defense_bonus;

        assert!((fort_defense_bonus(0) - 0.0).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(1) - 0.20).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(2) - 0.40).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(3) - 0.60).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(4) - 0.0).abs() < f64::EPSILON); // out of range
    }

    // ── Mountain terrain helps defender win ──────────────────────

    #[test]
    fn mountain_terrain_helps_defender() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // Moderately larger attacker force
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Regulars, atk_nation),
                make_unit(2, ArmyUnitType::Regulars, atk_nation),
                make_unit(3, ArmyUnitType::Regulars, atk_nation),
                make_unit(4, ArmyUnitType::Regulars, atk_nation),
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

        // Without terrain: 4 vs 3 regulars, attacker might win or it's close
        let result_no_terrain = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);

        // With mountain: defender gets +50% on top of 1.2x base = 1.8x
        let result_mountain = resolve_battle(
            &attacker,
            &defender,
            ProvinceId(1),
            Some(TerrainType::Mountain),
            0,
        );

        // Mountain should make the defender relatively stronger
        // (fewer defender casualties or more attacker casualties)
        let mountain_def_casualties = result_mountain.defender_casualties.len();
        let no_terrain_def_casualties = result_no_terrain.defender_casualties.len();
        assert!(
            mountain_def_casualties <= no_terrain_def_casualties,
            "Mountain terrain should reduce defender casualties (mountain: {}, none: {})",
            mountain_def_casualties,
            no_terrain_def_casualties
        );
    }

    // ── Fort helps defender ─────────────────────────────────────

    #[test]
    fn fort_helps_defender() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Regulars, atk_nation),
                make_unit(2, ArmyUnitType::Regulars, atk_nation),
                make_unit(3, ArmyUnitType::Regulars, atk_nation),
                make_unit(4, ArmyUnitType::Regulars, atk_nation),
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

        let result_no_fort = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        let result_fort3 = resolve_battle(&attacker, &defender, ProvinceId(1), None, 3);

        // Level 3 fort (+60%) should reduce defender casualties
        let fort_def_casualties = result_fort3.defender_casualties.len();
        let no_fort_def_casualties = result_no_fort.defender_casualties.len();
        assert!(
            fort_def_casualties <= no_fort_def_casualties,
            "Fort level 3 should reduce defender casualties (fort: {}, none: {})",
            fort_def_casualties,
            no_fort_def_casualties
        );
    }

    // ── Combined terrain + fort makes defender very strong ───────

    #[test]
    fn mountain_plus_fort_makes_defender_very_strong() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // Equal forces
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

        // Mountain + level 3 fort: defender FP multiplied by 1.2 * 1.5 * 1.6 = 2.88x
        let result = resolve_battle(
            &attacker,
            &defender,
            ProvinceId(1),
            Some(TerrainType::Mountain),
            3,
        );
        assert!(
            !result.attacker_won,
            "Equal forces with mountain + fort level 3 should heavily favor defender"
        );
    }
}
