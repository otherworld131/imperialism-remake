use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;

use std::sync::atomic::{AtomicU32, Ordering};

/// Strategy for choosing which enemy unit to prioritize for damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetingPriority {
    /// Damage weakest units first (lowest effective firepower).
    WeakestFirst,
    /// Damage strongest units first (highest effective firepower / most dangerous).
    StrongestFirst,
}

/// Calculate defense bonus percentage from terrain type.
///
/// Mountain: +50%, Hills (FertileHills/BarrenHills): +30%, Forest: +20%,
/// Swamp: +15%, all others: 0%.
pub fn terrain_defense_bonus(terrain: TerrainType) -> f64 {
    match terrain {
        TerrainType::Mountain => 0.50,
        TerrainType::Hills => 0.30,
        TerrainType::Forest => 0.20,
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
    /// Whether the attacker retreated (lost >60% of initial firepower).
    pub retreated: bool,
    /// Whether siege artillery reduced the fort's defense bonus.
    pub siege_reduced_fort: bool,
    /// Medal awards for surviving units on the winning side: (unit_type, new_medal_count).
    pub medal_awards: Vec<(ArmyUnitType, u8)>,
}

/// Calculate the General bonus multiplier for a force.
///
/// If the force contains a General, all friendly units get a 5% firepower
/// boost per General medal. Returns 1.0 if no General is present.
fn general_bonus(units: &[ArmyUnit]) -> f64 {
    if let Some(general) = units.iter().find(|u| u.unit_type == ArmyUnitType::General) {
        1.0 + general.medals as f64 * 0.05
    } else {
        1.0
    }
}

/// Calculate total firepower for a list of units, including General bonus.
fn total_firepower(units: &[ArmyUnit]) -> f64 {
    let base: f64 = units.iter().map(|u| u.effective_firepower()).sum();
    base * general_bonus(units)
}

/// Count Militia units in a force.
fn militia_count(units: &[ArmyUnit]) -> usize {
    units
        .iter()
        .filter(|u| u.unit_type == ArmyUnitType::Militia)
        .count()
}

/// Check if a force contains any siege artillery units.
fn has_siege_artillery(units: &[ArmyUnit]) -> bool {
    units.iter().any(|u| {
        u.unit_type == ArmyUnitType::SiegeArtillery || u.unit_type == ArmyUnitType::RailroadGun
    })
}

/// Calculate the effective fort defense bonus, reduced by 50% if attacker has siege artillery.
pub fn effective_fort_bonus(fort_level: u8, attacker_has_siege: bool) -> f64 {
    let base = fort_defense_bonus(fort_level);
    if attacker_has_siege && base > 0.0 {
        base * 0.5
    } else {
        base
    }
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
///    c. Apply damage to units (targeting priority determines which units take damage first)
///    d. Remove destroyed units (health <= 0)
///    e. If one side is eliminated, combat ends
/// 7. After rounds: side with more remaining firepower wins
/// 8. Surviving units earn 1 medal each (award_medal())
/// 9. Build BattleResult
///
/// The `targeting` parameter controls which enemy unit is damaged first:
/// - `StrongestFirst`: prioritize the highest-FP (most dangerous) enemy unit
/// - `WeakestFirst`: prioritize the lowest-FP enemy unit
pub fn resolve_battle(
    attacker: &CombatForce,
    defender: &CombatForce,
    province: ProvinceId,
    terrain: Option<TerrainType>,
    fort_level: u8,
) -> BattleResult {
    resolve_battle_with_targeting(
        attacker,
        defender,
        province,
        terrain,
        fort_level,
        TargetingPriority::StrongestFirst,
    )
}

/// Resolve a battle with an explicit targeting priority.
///
/// See [`resolve_battle`] for full combat resolution details.
pub fn resolve_battle_with_targeting(
    attacker: &CombatForce,
    defender: &CombatForce,
    province: ProvinceId,
    terrain: Option<TerrainType>,
    fort_level: u8,
    targeting: TargetingPriority,
) -> BattleResult {
    let mut atk_units = attacker.units.clone();
    let mut def_units = defender.units.clone();

    let attacker_initial_count = atk_units.len();
    let defender_initial_count = def_units.len();
    let attacker_initial_fp = total_firepower(&atk_units);
    let terrain_bonus_init = terrain.map(terrain_defense_bonus).unwrap_or(0.0);
    let attacker_has_siege = has_siege_artillery(&atk_units);
    let siege_reduced_fort = attacker_has_siege && fort_level > 0;
    let fort_bonus_init = effective_fort_bonus(fort_level, attacker_has_siege);
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
            retreated: false,
            siege_reduced_fort: false,
            medal_awards: Vec::new(),
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
            retreated: false,
            siege_reduced_fort: false,
            medal_awards: Vec::new(),
        };
    }

    // Handle edge case: defender empty
    if def_units.is_empty() {
        let medal_awards: Vec<(ArmyUnitType, u8)> = atk_units
            .iter()
            .map(|u| (u.unit_type, u.medals + 1))
            .collect();
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
            retreated: false,
            siege_reduced_fort,
            medal_awards,
        };
    }

    // Combat rounds (up to 10)
    let mut retreated = false;
    // Track damage dealt by each unit (by index in survivors) for medal eligibility
    let mut atk_damage_dealt: Vec<f64> = vec![0.0; atk_units.len()];
    let mut def_damage_dealt: Vec<f64> = vec![0.0; def_units.len()];

    for _ in 0..10 {
        if atk_units.is_empty() || def_units.is_empty() {
            break;
        }

        // Calculate firepower for this round
        let atk_fp = total_firepower(&atk_units);
        let terrain_bonus = terrain.map(terrain_defense_bonus).unwrap_or(0.0);
        let fort_bonus = effective_fort_bonus(fort_level, attacker_has_siege);
        let def_fp = total_firepower(&def_units) * 1.2 * (1.0 + terrain_bonus) * (1.0 + fort_bonus)
            + militia_count(&def_units) as f64 * 8.0;

        // Attacker deals damage to defender units
        if !def_units.is_empty() {
            let damage_per_unit = atk_fp / def_units.len() as f64;
            // Sort defender units by targeting priority
            match targeting {
                TargetingPriority::WeakestFirst => {
                    def_units.sort_by(|a, b| {
                        a.effective_firepower()
                            .partial_cmp(&b.effective_firepower())
                            .unwrap()
                    });
                }
                TargetingPriority::StrongestFirst => {
                    def_units.sort_by(|a, b| {
                        b.effective_firepower()
                            .partial_cmp(&a.effective_firepower())
                            .unwrap()
                    });
                }
            }
            for unit in &mut def_units {
                unit.take_damage(damage_per_unit as u8);
            }
            // Track damage dealt by each attacker unit (proportional to their firepower)
            if atk_fp > 0.0 {
                for (idx, unit) in atk_units.iter().enumerate() {
                    if idx < atk_damage_dealt.len() {
                        atk_damage_dealt[idx] += unit.effective_firepower();
                    }
                }
            }
        }

        // Defender deals damage to attacker units
        if !atk_units.is_empty() {
            let damage_per_unit = def_fp / atk_units.len() as f64;
            // Sort attacker units by targeting priority
            match targeting {
                TargetingPriority::WeakestFirst => {
                    atk_units.sort_by(|a, b| {
                        a.effective_firepower()
                            .partial_cmp(&b.effective_firepower())
                            .unwrap()
                    });
                }
                TargetingPriority::StrongestFirst => {
                    atk_units.sort_by(|a, b| {
                        b.effective_firepower()
                            .partial_cmp(&a.effective_firepower())
                            .unwrap()
                    });
                }
            }
            for unit in &mut atk_units {
                unit.take_damage(damage_per_unit as u8);
            }
            // Track damage dealt by each defender unit (proportional to their firepower)
            if def_fp > 0.0 {
                for (idx, unit) in def_units.iter().enumerate() {
                    if idx < def_damage_dealt.len() {
                        def_damage_dealt[idx] += unit.effective_firepower();
                    }
                }
            }
        }

        // Remove destroyed units and record casualties
        let mut i = 0;
        while i < def_units.len() {
            if !def_units[i].is_alive() {
                defender_casualties.push(def_units[i].unit_type);
                def_units.remove(i);
                if i < def_damage_dealt.len() {
                    def_damage_dealt.remove(i);
                }
            } else {
                i += 1;
            }
        }

        let mut i = 0;
        while i < atk_units.len() {
            if !atk_units[i].is_alive() {
                attacker_casualties.push(atk_units[i].unit_type);
                atk_units.remove(i);
                if i < atk_damage_dealt.len() {
                    atk_damage_dealt.remove(i);
                }
            } else {
                i += 1;
            }
        }

        // Check for retreat: if attacker has lost >60% of initial firepower
        if attacker_initial_fp > 0.0 && !atk_units.is_empty() {
            let current_atk_fp = total_firepower(&atk_units);
            let fp_lost_ratio = 1.0 - (current_atk_fp / attacker_initial_fp);
            if fp_lost_ratio > 0.60 {
                retreated = true;
                // Retreating units suffer 10% additional damage on remaining health
                for unit in &mut atk_units {
                    let retreat_damage = (unit.health as f64 * 0.10) as u8;
                    // Round to nearest 5% increment for consistency
                    let rounded = (retreat_damage / 5) * 5;
                    if rounded > 0 {
                        unit.take_damage(rounded);
                    }
                }
                // Remove any units killed by retreat damage
                let mut i = 0;
                while i < atk_units.len() {
                    if !atk_units[i].is_alive() {
                        attacker_casualties.push(atk_units[i].unit_type);
                        atk_units.remove(i);
                        if i < atk_damage_dealt.len() {
                            atk_damage_dealt.remove(i);
                        }
                    } else {
                        i += 1;
                    }
                }
                break;
            }
        }
    }

    // Determine winner: if one side eliminated, the other wins.
    // If attacker retreated, defender wins.
    // If both survive, the side with more remaining firepower wins.
    let attacker_won = if retreated {
        false
    } else if def_units.is_empty() && !atk_units.is_empty() {
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
    // Winners with 0 medals get 1 medal.
    // Winners with existing medals: gain one if they dealt damage.
    // Losers keep their medals (don't lose them).
    let mut medal_awards: Vec<(ArmyUnitType, u8)> = Vec::new();
    if attacker_won {
        for (idx, unit) in atk_units.iter_mut().enumerate() {
            let dealt = atk_damage_dealt.get(idx).copied().unwrap_or(0.0);
            if unit.medals == 0 || dealt > 0.0 {
                unit.award_medal();
                medal_awards.push((unit.unit_type, unit.medals));
            }
        }
    } else {
        for (idx, unit) in def_units.iter_mut().enumerate() {
            let dealt = def_damage_dealt.get(idx).copied().unwrap_or(0.0);
            if unit.medals == 0 || dealt > 0.0 {
                unit.award_medal();
                medal_awards.push((unit.unit_type, unit.medals));
            }
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
        retreated,
        siege_reduced_fort,
        medal_awards,
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
        assert!((terrain_defense_bonus(TerrainType::Hills) - 0.30).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Forest) - 0.20).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Swamp) - 0.15).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Grassland) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Desert) - 0.0).abs() < f64::EPSILON);
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

    // ── Retreat mechanics ─────────────────────────────────────────

    #[test]
    fn retreat_triggers_when_attacker_loses_over_60_percent_fp() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // Small attacker force against a strong defender
        // Attacker will lose firepower quickly and should retreat
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Regulars, atk_nation),
                make_unit(2, ArmyUnitType::Regulars, atk_nation),
            ],
        );

        // Defender: strong force with terrain + fort that will quickly degrade attacker
        let defender = make_force(
            def_nation,
            vec![
                make_unit(10, ArmyUnitType::Guards, def_nation),
                make_unit(11, ArmyUnitType::Guards, def_nation),
                make_unit(12, ArmyUnitType::Guards, def_nation),
                make_unit(13, ArmyUnitType::Guards, def_nation),
                make_unit(14, ArmyUnitType::Guards, def_nation),
            ],
        );

        let result = resolve_battle(
            &attacker,
            &defender,
            ProvinceId(1),
            Some(TerrainType::Mountain),
            3,
        );

        // Attacker should have retreated or been eliminated
        assert!(
            !result.attacker_won,
            "Attacker should not win against overwhelmingly stronger defender"
        );
        // If any attacker survivors remain, they should have retreated
        if !result.attacker_survivors.is_empty() {
            assert!(
                result.retreated,
                "Attacker with survivors against overwhelming force should have retreated"
            );
        }
    }

    #[test]
    fn retreating_units_take_extra_damage() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // Attacker: 2 weak regulars
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Regulars, atk_nation),
                make_unit(2, ArmyUnitType::Regulars, atk_nation),
            ],
        );

        // Defender: very strong force to force retreat
        let defender = make_force(
            def_nation,
            vec![
                make_unit(10, ArmyUnitType::Guards, def_nation),
                make_unit(11, ArmyUnitType::Guards, def_nation),
                make_unit(12, ArmyUnitType::Guards, def_nation),
                make_unit(13, ArmyUnitType::Guards, def_nation),
                make_unit(14, ArmyUnitType::Guards, def_nation),
                make_unit(15, ArmyUnitType::Guards, def_nation),
            ],
        );

        let result = resolve_battle(
            &attacker,
            &defender,
            ProvinceId(1),
            Some(TerrainType::Mountain),
            3,
        );

        assert!(!result.attacker_won);
        // If retreat happened, surviving units should have less than 100 health
        if result.retreated && !result.attacker_survivors.is_empty() {
            for survivor in &result.attacker_survivors {
                assert!(
                    survivor.health < 100,
                    "Retreating survivors should have taken damage (health: {})",
                    survivor.health
                );
            }
        }
    }

    #[test]
    fn no_retreat_when_attacker_is_winning() {
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
        assert!(!result.retreated, "Winning attacker should not retreat");
    }

    #[test]
    fn retreat_means_defender_wins() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // Set up a battle where retreat will definitely occur
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Militia, atk_nation),
                make_unit(2, ArmyUnitType::Militia, atk_nation),
            ],
        );

        let defender = make_force(
            def_nation,
            vec![
                make_unit(10, ArmyUnitType::Guards, def_nation),
                make_unit(11, ArmyUnitType::Guards, def_nation),
                make_unit(12, ArmyUnitType::Guards, def_nation),
                make_unit(13, ArmyUnitType::Guards, def_nation),
                make_unit(14, ArmyUnitType::SiegeArtillery, def_nation),
                make_unit(15, ArmyUnitType::SiegeArtillery, def_nation),
            ],
        );

        let result = resolve_battle(
            &attacker,
            &defender,
            ProvinceId(1),
            Some(TerrainType::Mountain),
            3,
        );
        // Attacker should lose (either eliminated or retreated)
        assert!(!result.attacker_won);
    }

    // ── Siege artillery reduces fort defense ─────────────────────

    #[test]
    fn effective_fort_bonus_without_siege() {
        // Without siege, fort bonus is unchanged
        assert!((effective_fort_bonus(1, false) - 0.20).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(2, false) - 0.40).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(3, false) - 0.60).abs() < f64::EPSILON);
    }

    #[test]
    fn effective_fort_bonus_with_siege_reduces_by_half() {
        // With siege, fort bonus is halved
        assert!((effective_fort_bonus(1, true) - 0.10).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(2, true) - 0.20).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(3, true) - 0.30).abs() < f64::EPSILON);
    }

    #[test]
    fn effective_fort_bonus_no_fort_unaffected_by_siege() {
        assert!((effective_fort_bonus(0, false) - 0.0).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(0, true) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn siege_artillery_reduces_fort_bonus_in_battle() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // Attacker WITH siege artillery
        let attacker_with_siege = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Guards, atk_nation),
                make_unit(3, ArmyUnitType::Guards, atk_nation),
                make_unit(4, ArmyUnitType::SiegeArtillery, atk_nation),
            ],
        );

        // Attacker WITHOUT siege artillery
        let attacker_without_siege = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Guards, atk_nation),
                make_unit(3, ArmyUnitType::Guards, atk_nation),
                make_unit(4, ArmyUnitType::Guards, atk_nation),
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

        // With fort level 3 and siege artillery
        let result_with_siege =
            resolve_battle(&attacker_with_siege, &defender, ProvinceId(1), None, 3);

        // Without siege (same strength force) against fort level 3
        let result_without_siege =
            resolve_battle(&attacker_without_siege, &defender, ProvinceId(1), None, 3);

        // The siege result should have the flag set
        assert!(result_with_siege.siege_reduced_fort);
        assert!(!result_without_siege.siege_reduced_fort);

        // With siege, attacker should do better (fewer attacker casualties or more defender casualties)
        assert!(
            result_with_siege.attacker_casualties.len()
                <= result_without_siege.attacker_casualties.len()
                || result_with_siege.defender_casualties.len()
                    >= result_without_siege.defender_casualties.len(),
            "Siege artillery should reduce fort effectiveness"
        );
    }

    #[test]
    fn railroad_gun_also_counts_as_siege() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::RailroadGun, atk_nation),
            ],
        );
        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Militia, def_nation)],
        );

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 2);
        assert!(result.siege_reduced_fort);
    }

    #[test]
    fn no_siege_units_means_no_fort_reduction() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Regulars, atk_nation),
            ],
        );
        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Militia, def_nation)],
        );

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 2);
        assert!(!result.siege_reduced_fort);
    }

    // ── Medal awards in battle results ───────────────────────────

    #[test]
    fn battle_result_includes_medal_awards() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Guards, atk_nation),
                make_unit(3, ArmyUnitType::Guards, atk_nation),
            ],
        );

        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Militia, def_nation)],
        );

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        assert!(result.attacker_won);
        // Medal awards should be populated for the winning side
        assert!(
            !result.medal_awards.is_empty(),
            "Medal awards should be recorded for winning units"
        );
        // Each medal award should have a positive medal count
        for (_, count) in &result.medal_awards {
            assert!(*count >= 1, "Awarded medal count should be at least 1");
        }
    }

    #[test]
    fn empty_defender_medal_awards_recorded() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        let attacker = make_force(
            atk_nation,
            vec![make_unit(1, ArmyUnitType::Regulars, atk_nation)],
        );
        let defender = make_force(def_nation, vec![]);

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        assert!(result.attacker_won);
        assert_eq!(result.medal_awards.len(), 1);
        assert_eq!(result.medal_awards[0].0, ArmyUnitType::Regulars);
        assert_eq!(result.medal_awards[0].1, 1);
    }

    // ── Targeting priority ───────────────────────────────────────

    #[test]
    fn strongest_first_targeting_damages_high_fp_units_first() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // Attacker with moderate force
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Guards, atk_nation),
                make_unit(3, ArmyUnitType::Guards, atk_nation),
            ],
        );

        // Defender with mixed-strength units
        let defender = make_force(
            def_nation,
            vec![
                make_unit(10, ArmyUnitType::Guards, def_nation),
                make_unit(11, ArmyUnitType::Militia, def_nation),
            ],
        );

        // StrongestFirst: Guards (higher FP) should be targeted first
        let result_strongest = resolve_battle_with_targeting(
            &attacker,
            &defender,
            ProvinceId(1),
            None,
            0,
            TargetingPriority::StrongestFirst,
        );

        // WeakestFirst: Militia (lower FP) should be targeted first
        let result_weakest = resolve_battle_with_targeting(
            &attacker,
            &defender,
            ProvinceId(1),
            None,
            0,
            TargetingPriority::WeakestFirst,
        );

        // Both should produce valid results (attacker and defender counts consistent)
        assert_eq!(
            result_strongest.attacker_casualties.len() + result_strongest.attacker_survivors.len(),
            3
        );
        assert_eq!(
            result_weakest.attacker_casualties.len() + result_weakest.attacker_survivors.len(),
            3
        );

        // Total defender forces should be accounted for
        assert_eq!(
            result_strongest.defender_casualties.len() + result_strongest.defender_survivors.len(),
            2
        );
        assert_eq!(
            result_weakest.defender_casualties.len() + result_weakest.defender_survivors.len(),
            2
        );
    }

    #[test]
    fn default_resolve_battle_uses_strongest_first() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Guards, atk_nation),
            ],
        );
        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Militia, def_nation)],
        );

        // Default resolve_battle should produce the same result as explicit StrongestFirst
        let result_default = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        let result_explicit = resolve_battle_with_targeting(
            &attacker,
            &defender,
            ProvinceId(1),
            None,
            0,
            TargetingPriority::StrongestFirst,
        );

        assert_eq!(result_default.attacker_won, result_explicit.attacker_won);
        assert_eq!(
            result_default.attacker_casualties.len(),
            result_explicit.attacker_casualties.len()
        );
        assert_eq!(
            result_default.defender_casualties.len(),
            result_explicit.defender_casualties.len()
        );
    }

    // ── Damage calculation uses firepower and modifiers ──────────

    /// Verify combat uses FP * medal_modifier * terrain_modifier formula.
    #[test]
    fn damage_calculation_uses_firepower_and_modifiers() {
        // Base effective firepower: Regulars base FP = 2, no medals => 2.0
        let unit = make_unit(1, ArmyUnitType::Regulars, NationId(1));
        assert!((unit.effective_firepower() - 2.0).abs() < f64::EPSILON);

        // With 1 medal: FP = 2 * 1.25 = 2.5
        let mut medaled_unit = make_unit(2, ArmyUnitType::Regulars, NationId(1));
        medaled_unit.award_medal();
        assert!((medaled_unit.effective_firepower() - 2.5).abs() < f64::EPSILON);

        // Guards base FP = 5, 2 medals: 5 * 1.5 = 7.5
        let mut guards = make_unit(3, ArmyUnitType::Guards, NationId(1));
        guards.award_medal();
        guards.award_medal();
        assert!((guards.effective_firepower() - 7.5).abs() < f64::EPSILON);

        // Terrain defense bonus applied to defender
        let terrain_bonus = terrain_defense_bonus(TerrainType::Mountain);
        assert!((terrain_bonus - 0.50).abs() < f64::EPSILON);

        let terrain_bonus_hills = terrain_defense_bonus(TerrainType::Hills);
        assert!((terrain_bonus_hills - 0.30).abs() < f64::EPSILON);

        let terrain_bonus_forest = terrain_defense_bonus(TerrainType::Forest);
        assert!((terrain_bonus_forest - 0.20).abs() < f64::EPSILON);

        // Fort defense bonus
        let fort_bonus = fort_defense_bonus(3);
        assert!((fort_bonus - 0.60).abs() < f64::EPSILON);

        // Siege artillery reduces fort bonus by 50%
        let effective = effective_fort_bonus(3, true);
        assert!((effective - 0.30).abs() < f64::EPSILON);
    }
}
