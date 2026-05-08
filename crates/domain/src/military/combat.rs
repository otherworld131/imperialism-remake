use crate::data::GameConfig;
use crate::map::UnitId;
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;
#[cfg(test)]
use std::sync::atomic::{AtomicU32, Ordering};

/// Strategy for choosing which enemy unit to prioritize for damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetingPriority {
    /// Damage weakest units first (lowest effective firepower).
    WeakestFirst,
    /// Damage strongest units first (highest effective firepower / most dangerous).
    StrongestFirst,
}

/// Configuration for battle resolution, including retreat rules (card #18).
///
/// `*_retreat_ratio` fields use `f64::INFINITY` as "disabled": pre-battle
/// retreat never triggers for that side. Similarly `*_postbattle_fp_loss = 1.0`
/// effectively disables mid-battle retreat for that side.
#[derive(Debug, Clone, Copy)]
pub struct BattleConfig {
    pub targeting: TargetingPriority,
    /// Whether the attacker has any eligible retreat destination.
    pub attacker_can_retreat: bool,
    /// Whether the defender has any eligible retreat destination.
    /// False when the defender is the nation's capital or is landlocked with
    /// no owned neighboring province.
    pub defender_can_retreat: bool,
    /// If `defender_fp / attacker_fp > this`, the attacker declines the battle
    /// before any damage is dealt. `INFINITY` disables.
    pub attacker_retreat_ratio: f64,
    /// If `attacker_fp / defender_fp > this`, the defender evacuates the
    /// province before any damage is dealt.
    pub defender_retreat_ratio: f64,
    /// Fraction of attacker starting firepower lost to trigger mid-battle
    /// retreat. Set to `1.0` or greater to disable.
    pub attacker_postbattle_fp_loss: f64,
    /// Fraction of defender starting firepower lost to trigger mid-battle
    /// retreat by the defender.
    pub defender_postbattle_fp_loss: f64,
}

impl BattleConfig {
    pub fn with_targeting(targeting: TargetingPriority, config: &GameConfig) -> Self {
        Self {
            targeting,
            attacker_can_retreat: true,
            defender_can_retreat: false,
            attacker_retreat_ratio: f64::INFINITY,
            defender_retreat_ratio: f64::INFINITY,
            attacker_postbattle_fp_loss: config.battle_attacker_fp_loss_ratio,
            defender_postbattle_fp_loss: config.battle_defender_fp_loss_ratio,
        }
    }
}

/// Calculate defense bonus percentage from terrain type.
///
/// Mountain: +50%, Hills: +30%, Forest: +20%, Swamp: +15%, all others: 0%.
/// Values are read from the provided [`GameConfig`] (Lua-tunable via D-5).
pub fn terrain_defense_bonus(terrain: TerrainType, config: &GameConfig) -> f64 {
    match terrain {
        TerrainType::Mountain => config.terrain_defense_mountain,
        TerrainType::Hills => config.terrain_defense_hills,
        TerrainType::Forest => config.terrain_defense_forest,
        TerrainType::Swamp => config.terrain_defense_swamp,
        _ => 0.0,
    }
}

/// Calculate defense bonus multiplier from fort level.
///
/// Level 0: no bonus, Level 1: +20%, Level 2: +40%, Level 3: +60%.
/// Values are read from the provided [`GameConfig`] (Lua-tunable via D-5).
pub fn fort_defense_bonus(fort_level: u8, config: &GameConfig) -> f64 {
    match fort_level {
        1 => config.fort_defense_level1,
        2 => config.fort_defense_level2,
        3 => config.fort_defense_level3,
        _ => 0.0,
    }
}

/// Spawn a single Militia `ArmyUnit` tagged to the given owner and position.
/// Used both by persistent-garrison seeding and by the garrison regeneration
/// tick. Takes the game's unit-ID counter so that ID allocation is
/// deterministic across two games started from the same map key.
pub fn spawn_militia_unit(id_counter: &mut u32, owner: NationId, position: ProvinceId) -> ArmyUnit {
    use crate::map::UnitId;
    let id = *id_counter;
    *id_counter += 1;
    ArmyUnit::new(UnitId(id), ArmyUnitType::Minutemen, owner, position)
}

/// Spawn a single `GarrisonArtillery` unit tagged to the given owner/position.
/// Only produced for a minor nation's capital at map generation time.
pub fn spawn_garrison_artillery_unit(
    id_counter: &mut u32,
    owner: NationId,
    position: ProvinceId,
) -> ArmyUnit {
    use crate::map::UnitId;
    let id = *id_counter;
    *id_counter += 1;
    ArmyUnit::new(UnitId(id), ArmyUnitType::GarrisonArtillery, owner, position)
}

/// Backfill persistent militia in every province based on each province's
/// cached `garrison_count`. Used by test fixtures that construct `GameState`
/// manually (bypassing `new_game`), and by scenarios that need to refresh
/// garrison units after populating province ownership. Existing Militia
/// units at a province are preserved — this function only tops up to the
/// cached count.
pub fn seed_militia_from_garrison_count(game: &mut crate::game_state::GameState) {
    let snapshots: Vec<(NationId, ProvinceId, u8)> = game
        .world
        .provinces
        .iter()
        .map(|p| (p.owner, p.id, p.garrison_count))
        .collect();
    for (owner, pid, target) in snapshots {
        if target == 0 {
            continue;
        }
        let existing = game
            .get_nation(owner)
            .map(|n| {
                n.military
                    .army
                    .iter()
                    .filter(|u| u.position == pid && u.unit_type == ArmyUnitType::Minutemen)
                    .count()
            })
            .unwrap_or(0);
        if existing >= target as usize {
            continue;
        }
        for _ in existing..(target as usize) {
            let unit = spawn_militia_unit(&mut game.next_unit_id, owner, pid);
            if let Some(nation) = game.get_nation_mut(owner) {
                nation.military.army.push(unit);
            }
        }
    }
}

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
    /// Whether the attacker retreated (pre-battle bailout or >=60% initial
    /// firepower loss mid-combat). Alias: `attacker_retreated`.
    pub retreated: bool,
    /// Whether the defender retreated (evacuated the province), either
    /// pre-battle or mid-combat after heavy losses. Mutually exclusive with
    /// `attacker_won = false`. When true, `attacker_won` is also true: the
    /// attacker takes the province without fully destroying the defender.
    pub defender_retreated: bool,
    /// Placements chosen by the caller for surviving attacker units when
    /// `retreated` is set. Empty otherwise.
    pub attacker_retreated_to: Vec<(UnitId, ProvinceId)>,
    /// Placements chosen by the caller for surviving defender units when
    /// `defender_retreated` is set. Empty otherwise.
    pub defender_retreated_to: Vec<(UnitId, ProvinceId)>,
    /// Whether siege artillery reduced the fort's defense bonus.
    pub siege_reduced_fort: bool,
    /// Medal awards for surviving units on the winning side: (unit_type, new_medal_count).
    pub medal_awards: Vec<(ArmyUnitType, u8)>,
    /// Province IDs where attacking units originated (for battle screen arrows).
    /// Multiple provinces when army units are spread across different locations.
    /// Populated for both land attacks and naval landings (the latter records the
    /// embarkation provinces of the landing force).
    pub attacker_origin_provinces: Vec<ProvinceId>,
    /// True when the battle was an amphibious assault (units arrived via warship
    /// landing). False for land attacks across an adjacent border.
    pub is_naval_landing: bool,
    /// Debug-only: explains what triggered a retreat decision (or that the
    /// battle was fought to conclusion). Surfaced in the UI behind a toggle.
    pub retreat_debug: Option<RetreatDebug>,
}

/// Stage at which a side decided to retreat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetreatStage {
    /// Pre-battle bail: side bailed before any rounds based on FP ratio vs threshold.
    PreBattle,
    /// Mid-battle bail: side bailed after a round based on cumulative FP loss vs threshold.
    MidBattle,
    /// No retreat happened — battle was fought to conclusion.
    None,
}

impl RetreatStage {
    pub fn as_str(self) -> &'static str {
        match self {
            RetreatStage::PreBattle => "pre_battle",
            RetreatStage::MidBattle => "mid_battle",
            RetreatStage::None => "none",
        }
    }
}

/// Debug info about which retreat-decision threshold triggered (or didn't).
#[derive(Debug, Clone)]
pub struct RetreatDebug {
    /// Which side retreated, if any: "attacker" / "defender" / "none".
    pub side: &'static str,
    pub stage: RetreatStage,
    /// For PreBattle: defender_FP / attacker_FP (attacker view) or vice versa.
    /// For MidBattle: fraction of initial FP lost (0.0 .. 1.0).
    pub measured_value: f64,
    /// The threshold the measured value was compared against.
    pub threshold: f64,
    /// Optional second pair so we can show "the other side also wanted to bail".
    pub attacker_prebattle_ratio: f64,
    pub defender_prebattle_ratio: f64,
    pub attacker_prebattle_threshold: f64,
    pub defender_prebattle_threshold: f64,
    /// Round number at which mid-battle retreat triggered (1-based). 0 for
    /// pre-battle / no retreat.
    pub round: u32,
}

/// Calculate the General bonus multiplier for a force.
///
/// A General in the force grants a base +10% firepower to every friendly
/// unit, plus +5% per medal (so 4-medal generals cap at +30%). Returns
/// 1.0 if no General is present.
fn general_bonus(units: &[ArmyUnit]) -> f64 {
    if let Some(general) = units
        .iter()
        .filter(|u| u.unit_type == ArmyUnitType::General)
        .max_by_key(|u| u.medals)
    {
        1.10 + general.medals as f64 * 0.05
    } else {
        1.0
    }
}

/// Calculate total firepower for a list of units, including General bonus.
fn total_firepower(units: &[ArmyUnit]) -> f64 {
    let base: f64 = units.iter().map(|u| u.effective_firepower()).sum();
    base * general_bonus(units)
}

/// Attacker firepower using FPM for cavalry charging into melee (range == 1,
/// FPM > 0). All other units use the standard FPN path.
fn attacker_total_firepower(units: &[ArmyUnit]) -> f64 {
    let base: f64 = units.iter().map(|u| u.effective_firepower_charging()).sum();
    base * general_bonus(units)
}

/// Determine whether defensive terrain (or a fort) qualifies a unit's
/// per-unit `defense_terrain_bonus`.
fn qualifies_for_per_unit_terrain(terrain: Option<TerrainType>, fort_level: u8) -> bool {
    if fort_level > 0 {
        return true;
    }
    matches!(
        terrain,
        Some(TerrainType::Mountain | TerrainType::Hills | TerrainType::Forest | TerrainType::Swamp)
    )
}

/// Defender firepower using per-unit DEF stats.
///
/// Replaces the flat `* 1.2` multiplier with each unit's `defense` value as a
/// raw multiplier (`fp * defense`). Per-unit `defense_terrain_bonus` is added
/// to the terrain bonus for units in qualifying terrain (mountain / hills /
/// forest / swamp) or in any fort.  The global fort bonus is then applied as a
/// final multiplier across all units.
///
/// Does **not** include the Minutemen flat entrenchment bonus (+8 per unit) —
/// the caller adds that separately.
fn defender_total_firepower(
    units: &[ArmyUnit],
    terrain: Option<TerrainType>,
    fort_level: u8,
    attacker_has_siege: bool,
    config: &GameConfig,
) -> f64 {
    let global_terrain = terrain
        .map(|t| terrain_defense_bonus(t, config))
        .unwrap_or(0.0);
    let global_fort = effective_fort_bonus(fort_level, attacker_has_siege, config);
    let per_unit_qualifies = qualifies_for_per_unit_terrain(terrain, fort_level);

    let base: f64 = units
        .iter()
        .map(|u| {
            let fp = u.effective_firepower();
            let def = u.unit_type.stats().defense as f64;
            let per_unit_bonus = if per_unit_qualifies {
                u.unit_type.stats().defense_terrain_bonus as f64
            } else {
                0.0
            };
            fp * def * (1.0 + global_terrain + per_unit_bonus)
        })
        .sum();

    base * (1.0 + global_fort) * general_bonus(units)
}

/// Count Militia units in a force.
fn militia_count(units: &[ArmyUnit]) -> usize {
    units
        .iter()
        .filter(|u| u.unit_type == ArmyUnitType::Minutemen)
        .count()
}

/// Check if a force contains any siege artillery units.
fn has_siege_artillery(units: &[ArmyUnit]) -> bool {
    units.iter().any(|u| {
        u.unit_type == ArmyUnitType::SiegeArtillery || u.unit_type == ArmyUnitType::RailroadGuns
    })
}

/// Calculate the effective fort defense bonus, reduced by 50% if attacker has siege artillery.
pub fn effective_fort_bonus(fort_level: u8, attacker_has_siege: bool, config: &GameConfig) -> f64 {
    let base = fort_defense_bonus(fort_level, config);
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
    let game_cfg = GameConfig::default();
    resolve_battle_with_config(
        attacker,
        defender,
        province,
        terrain,
        fort_level,
        BattleConfig::with_targeting(TargetingPriority::StrongestFirst, &game_cfg),
        &game_cfg,
    )
}

/// Resolve a battle with an explicit targeting priority.
///
/// See [`resolve_battle`] for full combat resolution details.
/// Use [`resolve_battle_with_config`] for full retreat control.
pub fn resolve_battle_with_targeting(
    attacker: &CombatForce,
    defender: &CombatForce,
    province: ProvinceId,
    terrain: Option<TerrainType>,
    fort_level: u8,
    targeting: TargetingPriority,
) -> BattleResult {
    let game_cfg = GameConfig::default();
    resolve_battle_with_config(
        attacker,
        defender,
        province,
        terrain,
        fort_level,
        BattleConfig::with_targeting(targeting, &game_cfg),
        &game_cfg,
    )
}

/// Resolve a battle with a full [`BattleConfig`], including retreat rules.
///
/// Two retreat paths are supported:
///   * Pre-battle retreat: a side whose opponent's firepower exceeds its own
///     by more than the retreat-ratio bails before any damage is dealt,
///     provided `*_can_retreat` is true.
///   * Mid-battle retreat: a side that has lost more than `*_postbattle_fp_loss`
///     fraction of its starting FP retreats, gated by `*_can_retreat`.
///
/// Retreat placements (where survivors go) are the caller's responsibility:
/// this function just sets `retreated` / `defender_retreated` flags and
/// returns the surviving units. The caller consumes those flags and writes
/// into `attacker_retreated_to` / `defender_retreated_to` as part of post-
/// battle state updates.
pub fn resolve_battle_with_config(
    attacker: &CombatForce,
    defender: &CombatForce,
    province: ProvinceId,
    terrain: Option<TerrainType>,
    fort_level: u8,
    config: BattleConfig,
    game_config: &GameConfig,
) -> BattleResult {
    let mut atk_units = attacker.units.clone();
    let mut def_units = defender.units.clone();

    let attacker_initial_count = atk_units.len();
    let defender_initial_count = def_units.len();
    let attacker_initial_fp = attacker_total_firepower(&atk_units);
    let attacker_has_siege = has_siege_artillery(&atk_units);
    let siege_reduced_fort = attacker_has_siege && fort_level > 0;
    let defender_initial_fp = defender_total_firepower(
        &def_units,
        terrain,
        fort_level,
        attacker_has_siege,
        game_config,
    ) + militia_count(&def_units) as f64 * 8.0;

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
            defender_retreated: false,
            attacker_retreated_to: Vec::new(),
            defender_retreated_to: Vec::new(),
            siege_reduced_fort: false,
            medal_awards: Vec::new(),
            attacker_origin_provinces: Vec::new(),
            is_naval_landing: false,
            retreat_debug: None,
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
            defender_retreated: false,
            attacker_retreated_to: Vec::new(),
            defender_retreated_to: Vec::new(),
            siege_reduced_fort: false,
            medal_awards: Vec::new(),
            attacker_origin_provinces: Vec::new(),
            is_naval_landing: false,
            retreat_debug: None,
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
            defender_retreated: false,
            attacker_retreated_to: Vec::new(),
            defender_retreated_to: Vec::new(),
            siege_reduced_fort,
            medal_awards,
            attacker_origin_provinces: Vec::new(),
            is_naval_landing: false,
            retreat_debug: None,
        };
    }

    // ── Pre-battle retreat (card #18) ───────────────────────────────
    // Uses the same FPM-aware attacker firepower and DEF-based defender
    // firepower (without the flat Minutemen bonus) for a consistent
    // signal to pre-battle retreat logic.
    let atk_fp_raw = attacker_total_firepower(&atk_units);
    let def_fp_raw = defender_total_firepower(
        &def_units,
        terrain,
        fort_level,
        attacker_has_siege,
        game_config,
    );
    let attacker_ratio = if atk_fp_raw > 0.0 {
        def_fp_raw / atk_fp_raw
    } else {
        f64::INFINITY
    };
    let defender_ratio = if def_fp_raw > 0.0 {
        atk_fp_raw / def_fp_raw
    } else {
        f64::INFINITY
    };
    let attacker_would_bail =
        config.attacker_can_retreat && attacker_ratio > config.attacker_retreat_ratio;
    let defender_would_bail =
        config.defender_can_retreat && defender_ratio > config.defender_retreat_ratio;

    if attacker_would_bail || defender_would_bail {
        // Decide which side actually bails (the more-dominated one, or
        // attacker by tie-breaker).
        let attacker_bails = if attacker_would_bail && defender_would_bail {
            attacker_ratio >= defender_ratio
        } else {
            attacker_would_bail
        };
        let prebattle_debug = RetreatDebug {
            side: if attacker_bails {
                "attacker"
            } else {
                "defender"
            },
            stage: RetreatStage::PreBattle,
            measured_value: if attacker_bails {
                attacker_ratio
            } else {
                defender_ratio
            },
            threshold: if attacker_bails {
                config.attacker_retreat_ratio
            } else {
                config.defender_retreat_ratio
            },
            attacker_prebattle_ratio: attacker_ratio,
            defender_prebattle_ratio: defender_ratio,
            attacker_prebattle_threshold: config.attacker_retreat_ratio,
            defender_prebattle_threshold: config.defender_retreat_ratio,
            round: 0,
        };
        if attacker_bails {
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
                retreated: true,
                defender_retreated: false,
                attacker_retreated_to: Vec::new(),
                defender_retreated_to: Vec::new(),
                siege_reduced_fort,
                medal_awards: Vec::new(),
                attacker_origin_provinces: Vec::new(),
                is_naval_landing: false,
                retreat_debug: Some(prebattle_debug),
            };
        } else {
            // Defender evacuates; attacker takes the province unopposed.
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
                defender_retreated: true,
                attacker_retreated_to: Vec::new(),
                defender_retreated_to: Vec::new(),
                siege_reduced_fort,
                medal_awards: Vec::new(),
                attacker_origin_provinces: Vec::new(),
                is_naval_landing: false,
                retreat_debug: Some(prebattle_debug),
            };
        }
    }

    // Combat rounds (up to 10)
    let mut retreated = false;
    let mut defender_retreated = false;
    let mut midbattle_debug: Option<RetreatDebug> = None;
    let mut current_round: u32 = 0;
    // Track damage dealt by each unit (keyed by UnitId) for medal eligibility
    let mut atk_damage_dealt: std::collections::HashMap<crate::map::UnitId, f64> =
        std::collections::HashMap::new();
    let mut def_damage_dealt: std::collections::HashMap<crate::map::UnitId, f64> =
        std::collections::HashMap::new();

    for _ in 0..10 {
        if atk_units.is_empty() || def_units.is_empty() {
            break;
        }
        current_round += 1;

        // Calculate firepower for this round
        let atk_fp = attacker_total_firepower(&atk_units);
        let def_fp = defender_total_firepower(
            &def_units,
            terrain,
            fort_level,
            attacker_has_siege,
            game_config,
        ) + militia_count(&def_units) as f64 * 8.0;

        // Attacker deals damage to defender units
        if !def_units.is_empty() {
            let damage_per_unit = atk_fp / def_units.len() as f64;
            // Sort defender units by targeting priority
            match config.targeting {
                TargetingPriority::WeakestFirst => {
                    def_units.sort_by(|a, b| {
                        a.effective_firepower()
                            .partial_cmp(&b.effective_firepower())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                TargetingPriority::StrongestFirst => {
                    def_units.sort_by(|a, b| {
                        b.effective_firepower()
                            .partial_cmp(&a.effective_firepower())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
            }
            for unit in &mut def_units {
                unit.take_damage(damage_per_unit as u8);
            }
            // Track damage dealt by each attacker unit (proportional to their firepower)
            if atk_fp > 0.0 {
                for unit in atk_units.iter() {
                    *atk_damage_dealt.entry(unit.id).or_insert(0.0) += unit.effective_firepower();
                }
            }
        }

        // Defender deals damage to attacker units
        if !atk_units.is_empty() {
            let damage_per_unit = def_fp / atk_units.len() as f64;
            // Sort attacker units by targeting priority
            match config.targeting {
                TargetingPriority::WeakestFirst => {
                    atk_units.sort_by(|a, b| {
                        a.effective_firepower()
                            .partial_cmp(&b.effective_firepower())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
                TargetingPriority::StrongestFirst => {
                    atk_units.sort_by(|a, b| {
                        b.effective_firepower()
                            .partial_cmp(&a.effective_firepower())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
            }
            for unit in &mut atk_units {
                unit.take_damage(damage_per_unit as u8);
            }
            // Track damage dealt by each defender unit (proportional to their firepower)
            if def_fp > 0.0 {
                for unit in def_units.iter() {
                    *def_damage_dealt.entry(unit.id).or_insert(0.0) += unit.effective_firepower();
                }
            }
        }

        // Remove destroyed units and record casualties
        for unit in def_units.iter().filter(|u| !u.is_alive()) {
            defender_casualties.push(unit.unit_type);
        }
        def_units.retain(|u| u.is_alive());

        for unit in atk_units.iter().filter(|u| !u.is_alive()) {
            attacker_casualties.push(unit.unit_type);
        }
        atk_units.retain(|u| u.is_alive());

        // Check for attacker retreat
        if config.attacker_can_retreat && attacker_initial_fp > 0.0 && !atk_units.is_empty() {
            let current_atk_fp = attacker_total_firepower(&atk_units);
            let fp_lost_ratio = 1.0 - (current_atk_fp / attacker_initial_fp);
            if fp_lost_ratio > config.attacker_postbattle_fp_loss {
                retreated = true;
                midbattle_debug = Some(RetreatDebug {
                    side: "attacker",
                    stage: RetreatStage::MidBattle,
                    measured_value: fp_lost_ratio,
                    threshold: config.attacker_postbattle_fp_loss,
                    attacker_prebattle_ratio: attacker_ratio,
                    defender_prebattle_ratio: defender_ratio,
                    attacker_prebattle_threshold: config.attacker_retreat_ratio,
                    defender_prebattle_threshold: config.defender_retreat_ratio,
                    round: current_round,
                });
                // Retreating units suffer 10% additional damage on remaining health
                for unit in &mut atk_units {
                    let retreat_damage = (unit.health as f64 * 0.10) as u8;
                    if retreat_damage > 0 {
                        unit.take_damage(retreat_damage);
                    }
                }
                for unit in atk_units.iter().filter(|u| !u.is_alive()) {
                    attacker_casualties.push(unit.unit_type);
                }
                atk_units.retain(|u| u.is_alive());
                break;
            }
        }

        // Check for defender retreat (card #18): symmetric to attacker.
        if config.defender_can_retreat && defender_initial_fp > 0.0 && !def_units.is_empty() {
            // Compare raw unit firepower for loss-ratio — terrain/fort bonuses
            // inflate `defender_initial_fp` artificially, so we use the raw
            // comparison against the un-bonused baseline.
            let raw_def_initial = total_firepower(&defender.units).max(f64::EPSILON);
            let current_def_fp = total_firepower(&def_units);
            let fp_lost_ratio = 1.0 - (current_def_fp / raw_def_initial);
            if fp_lost_ratio > config.defender_postbattle_fp_loss {
                defender_retreated = true;
                midbattle_debug = Some(RetreatDebug {
                    side: "defender",
                    stage: RetreatStage::MidBattle,
                    measured_value: fp_lost_ratio,
                    threshold: config.defender_postbattle_fp_loss,
                    attacker_prebattle_ratio: attacker_ratio,
                    defender_prebattle_ratio: defender_ratio,
                    attacker_prebattle_threshold: config.attacker_retreat_ratio,
                    defender_prebattle_threshold: config.defender_retreat_ratio,
                    round: current_round,
                });
                for unit in &mut def_units {
                    let retreat_damage = (unit.health as f64 * 0.10) as u8;
                    if retreat_damage > 0 {
                        unit.take_damage(retreat_damage);
                    }
                }
                for unit in def_units.iter().filter(|u| !u.is_alive()) {
                    defender_casualties.push(unit.unit_type);
                }
                def_units.retain(|u| u.is_alive());
                break;
            }
        }
    }

    // Determine winner: retreat flags take priority, then eliminations, then
    // surviving firepower.
    let attacker_won = if retreated {
        false
    } else if defender_retreated || (def_units.is_empty() && !atk_units.is_empty()) {
        true
    } else if atk_units.is_empty() {
        false
    } else {
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
        for unit in atk_units.iter_mut() {
            let dealt = atk_damage_dealt.get(&unit.id).copied().unwrap_or(0.0);
            if unit.medals == 0 || dealt > 0.0 {
                unit.award_medal();
                medal_awards.push((unit.unit_type, unit.medals));
            }
        }
    } else {
        for unit in def_units.iter_mut() {
            let dealt = def_damage_dealt.get(&unit.id).copied().unwrap_or(0.0);
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
        defender_retreated,
        attacker_retreated_to: Vec::new(),
        defender_retreated_to: Vec::new(),
        siege_reduced_fort,
        medal_awards,
        attacker_origin_provinces: Vec::new(),
        is_naval_landing: false,
        retreat_debug: midbattle_debug.or(Some(RetreatDebug {
            side: "none",
            stage: RetreatStage::None,
            measured_value: 0.0,
            threshold: 0.0,
            attacker_prebattle_ratio: attacker_ratio,
            defender_prebattle_ratio: defender_ratio,
            attacker_prebattle_threshold: config.attacker_retreat_ratio,
            defender_prebattle_threshold: config.defender_retreat_ratio,
            round: current_round,
        })),
    }
}

/// Test-only counter for `create_garrison`. Production code uses `GameState::alloc_unit_id`.
#[cfg(test)]
static GARRISON_ID_COUNTER: AtomicU32 = AtomicU32::new(5_000_000);

/// Creates the starting garrison for a province. Used in tests only.
///
/// - Great Power: 4 Militia units
/// - Minor Nation: 3 Militia + 1 GarrisonArtillery (defensive artillery behind fortifications)
#[cfg(test)]
pub fn create_garrison(nation_type: NationType) -> Vec<ArmyUnit> {
    use crate::map::UnitId;

    let militia_count = match nation_type {
        NationType::GreatPower => 4,
        NationType::MinorNation => 3,
    };

    let mut units: Vec<ArmyUnit> = (0..militia_count)
        .map(|_| {
            let id = GARRISON_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
            ArmyUnit::new(
                UnitId(id),
                ArmyUnitType::Minutemen,
                NationId(0),   // placeholder — caller should set owner
                ProvinceId(0), // placeholder — caller should set position
            )
        })
        .collect();

    // Minor nations get defensive garrison artillery
    if nation_type == NationType::MinorNation {
        let id = GARRISON_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        units.push(ArmyUnit::new(
            UnitId(id),
            ArmyUnitType::GarrisonArtillery,
            NationId(0),
            ProvinceId(0),
        ));
    }

    units
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
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
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
                make_unit(10, ArmyUnitType::Minutemen, def_nation),
                make_unit(11, ArmyUnitType::Minutemen, def_nation),
                make_unit(12, ArmyUnitType::Minutemen, def_nation),
                make_unit(13, ArmyUnitType::Minutemen, def_nation),
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
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
        );

        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        assert!(result.attacker_won);
        // The militia should be destroyed
        assert!(
            result
                .defender_casualties
                .contains(&ArmyUnitType::Minutemen),
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
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
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
            assert_eq!(unit.unit_type, ArmyUnitType::Minutemen);
            assert_eq!(unit.health, 100);
        }
    }

    #[test]
    fn garrison_minor_nation_creates_3_militia_and_1_garrison_artillery() {
        let garrison = create_garrison(NationType::MinorNation);
        assert_eq!(garrison.len(), 4); // 3 Militia + 1 GarrisonArtillery
        let militia_count = garrison
            .iter()
            .filter(|u| u.unit_type == ArmyUnitType::Minutemen)
            .count();
        let ga_count = garrison
            .iter()
            .filter(|u| u.unit_type == ArmyUnitType::GarrisonArtillery)
            .count();
        assert_eq!(militia_count, 3);
        assert_eq!(ga_count, 1);
        for unit in &garrison {
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
            vec![make_unit(10, ArmyUnitType::Minutemen, NationId(2))],
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
        let cfg = GameConfig::default();

        assert!((terrain_defense_bonus(TerrainType::Mountain, &cfg) - 0.50).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Hills, &cfg) - 0.30).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Forest, &cfg) - 0.20).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Swamp, &cfg) - 0.15).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Grassland, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Desert, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Sea, &cfg) - 0.0).abs() < f64::EPSILON);
    }

    // ── Fort defense bonus ──────────────────────────────────────

    #[test]
    fn fort_defense_bonus_values() {
        use super::fort_defense_bonus;
        let cfg = GameConfig::default();

        assert!((fort_defense_bonus(0, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(1, &cfg) - 0.20).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(2, &cfg) - 0.40).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(3, &cfg) - 0.60).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(4, &cfg) - 0.0).abs() < f64::EPSILON); // out of range
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
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
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
                make_unit(1, ArmyUnitType::Minutemen, atk_nation),
                make_unit(2, ArmyUnitType::Minutemen, atk_nation),
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
        let cfg = GameConfig::default();
        assert!((effective_fort_bonus(1, false, &cfg) - 0.20).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(2, false, &cfg) - 0.40).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(3, false, &cfg) - 0.60).abs() < f64::EPSILON);
    }

    #[test]
    fn effective_fort_bonus_with_siege_reduces_by_half() {
        // With siege, fort bonus is halved
        let cfg = GameConfig::default();
        assert!((effective_fort_bonus(1, true, &cfg) - 0.10).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(2, true, &cfg) - 0.20).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(3, true, &cfg) - 0.30).abs() < f64::EPSILON);
    }

    #[test]
    fn effective_fort_bonus_no_fort_unaffected_by_siege() {
        let cfg = GameConfig::default();
        assert!((effective_fort_bonus(0, false, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(0, true, &cfg) - 0.0).abs() < f64::EPSILON);
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
                make_unit(2, ArmyUnitType::RailroadGuns, atk_nation),
            ],
        );
        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
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
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
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
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
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
                make_unit(11, ArmyUnitType::Minutemen, def_nation),
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
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
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
    /// FP numbers come from the original Imperialism (1997) manual table.
    #[test]
    fn damage_calculation_uses_firepower_and_modifiers() {
        // Base effective firepower: Regulars FPN = 10, no medals => 10.0.
        let unit = make_unit(1, ArmyUnitType::Regulars, NationId(1));
        assert!((unit.effective_firepower() - 10.0).abs() < f64::EPSILON);

        // With 1 medal: FP = 10 * 1.25 = 12.5
        let mut medaled_unit = make_unit(2, ArmyUnitType::Regulars, NationId(1));
        medaled_unit.award_medal();
        assert!((medaled_unit.effective_firepower() - 12.5).abs() < f64::EPSILON);

        // Guards FPN = 17, 2 medals (1.5×) => 25.5.
        let mut guards = make_unit(3, ArmyUnitType::Guards, NationId(1));
        guards.award_medal();
        guards.award_medal();
        assert!((guards.effective_firepower() - 25.5).abs() < f64::EPSILON);

        // Terrain defense bonus applied to defender
        let cfg = GameConfig::default();
        let terrain_bonus = terrain_defense_bonus(TerrainType::Mountain, &cfg);
        assert!((terrain_bonus - 0.50).abs() < f64::EPSILON);

        let terrain_bonus_hills = terrain_defense_bonus(TerrainType::Hills, &cfg);
        assert!((terrain_bonus_hills - 0.30).abs() < f64::EPSILON);

        let terrain_bonus_forest = terrain_defense_bonus(TerrainType::Forest, &cfg);
        assert!((terrain_bonus_forest - 0.20).abs() < f64::EPSILON);

        // Fort defense bonus
        let fort_bonus = fort_defense_bonus(3, &cfg);
        assert!((fort_bonus - 0.60).abs() < f64::EPSILON);

        // Siege artillery reduces fort bonus by 50%
        let effective = effective_fort_bonus(3, true, &cfg);
        assert!((effective - 0.30).abs() < f64::EPSILON);
    }

    // ── Card #18: retreat mechanic tests ─────────────────────────

    #[test]
    fn defender_retreats_pre_battle_when_overmatched() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Guards, atk_nation),
                make_unit(3, ArmyUnitType::Guards, atk_nation),
                make_unit(4, ArmyUnitType::Guards, atk_nation),
                make_unit(5, ArmyUnitType::Guards, atk_nation),
            ],
        );
        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
        );

        let cfg = BattleConfig {
            targeting: TargetingPriority::StrongestFirst,
            attacker_can_retreat: true,
            defender_can_retreat: true,
            attacker_retreat_ratio: 2.0,
            defender_retreat_ratio: 2.0,
            attacker_postbattle_fp_loss: 0.60,
            defender_postbattle_fp_loss: 0.60,
        };
        let result = resolve_battle_with_config(
            &attacker,
            &defender,
            ProvinceId(1),
            None,
            0,
            cfg,
            &GameConfig::default(),
        );
        assert!(
            result.defender_retreated,
            "defender should evacuate before a hopeless battle"
        );
        assert!(
            result.attacker_won,
            "attacker takes the province uncontested"
        );
        assert!(
            result.defender_casualties.is_empty(),
            "pre-battle retreat: no casualties on either side"
        );
        assert_eq!(result.defender_survivors.len(), 1);
    }

    #[test]
    fn defender_fights_to_destruction_when_cannot_retreat() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Guards, atk_nation),
                make_unit(3, ArmyUnitType::Guards, atk_nation),
                make_unit(4, ArmyUnitType::Guards, atk_nation),
                make_unit(5, ArmyUnitType::Guards, atk_nation),
            ],
        );
        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
        );
        // can_retreat=false simulates capital defense or no neighbors.
        let cfg = BattleConfig {
            targeting: TargetingPriority::StrongestFirst,
            attacker_can_retreat: true,
            defender_can_retreat: false,
            attacker_retreat_ratio: 2.0,
            defender_retreat_ratio: 2.0,
            attacker_postbattle_fp_loss: 0.60,
            defender_postbattle_fp_loss: 0.60,
        };
        let result = resolve_battle_with_config(
            &attacker,
            &defender,
            ProvinceId(1),
            None,
            0,
            cfg,
            &GameConfig::default(),
        );
        assert!(
            !result.defender_retreated,
            "defender with no retreat option must fight"
        );
        assert!(result.attacker_won);
        assert!(
            !result.defender_casualties.is_empty(),
            "defender should take casualties fighting to destruction"
        );
    }

    #[test]
    fn attacker_pre_battle_retreats_when_defender_dominates() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);
        // Small attacker, large defender.
        let attacker = make_force(
            atk_nation,
            vec![make_unit(1, ArmyUnitType::Regulars, atk_nation)],
        );
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
        let cfg = BattleConfig {
            targeting: TargetingPriority::StrongestFirst,
            attacker_can_retreat: true,
            defender_can_retreat: true,
            attacker_retreat_ratio: 2.0,
            defender_retreat_ratio: 2.0,
            attacker_postbattle_fp_loss: 0.60,
            defender_postbattle_fp_loss: 0.60,
        };
        let result = resolve_battle_with_config(
            &attacker,
            &defender,
            ProvinceId(1),
            None,
            0,
            cfg,
            &GameConfig::default(),
        );
        assert!(result.retreated, "attacker should bail pre-battle");
        assert!(!result.attacker_won);
        assert!(
            result.attacker_casualties.is_empty(),
            "pre-battle retreat: attacker takes no damage"
        );
    }

    // Card #99: pre-battle retreat must use raw strength (NOT the in-combat
    // Minutemen +8 flat bonus). With 15 Regulars (atk_fp = 150) vs 4 Minutemen
    // (raw def_fp = 100 using DEF=5 multiplier), the ratio is 0.67 — well under
    // 2.0 — so the attacker engages. The overwhelmingly larger force destroys all
    // Minutemen within 3 rounds before losing 60% FP.
    #[test]
    fn attacker_presses_eight_regulars_against_four_militia() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);
        let attacker = make_force(
            atk_nation,
            (0..15)
                .map(|i| make_unit(i, ArmyUnitType::Regulars, atk_nation))
                .collect(),
        );
        let defender = make_force(
            def_nation,
            (10..14)
                .map(|i| make_unit(i, ArmyUnitType::Minutemen, def_nation))
                .collect(),
        );
        let cfg = BattleConfig {
            targeting: TargetingPriority::StrongestFirst,
            attacker_can_retreat: true,
            defender_can_retreat: false,
            attacker_retreat_ratio: 2.0,
            defender_retreat_ratio: 2.0,
            attacker_postbattle_fp_loss: 0.60,
            defender_postbattle_fp_loss: 0.60,
        };
        let result = resolve_battle_with_config(
            &attacker,
            &defender,
            ProvinceId(1),
            None,
            0,
            cfg,
            &GameConfig::default(),
        );
        assert!(
            !result.retreated,
            "raw-strength ratio should keep a clearly superior attacker in the fight"
        );
        assert!(
            result.attacker_won,
            "15 Regulars should beat 4 Minutemen once the battle actually runs"
        );
    }

    // Complementary check: a genuinely outmatched attacker still retreats —
    // the fix is not "retreat never fires". 2 Regulars (atk_fp = 20) vs 8
    // Minutemen (raw def_fp = 200 using DEF=5 multiplier) yields a ratio of
    // 10.0 >> 2.0.
    #[test]
    fn attacker_still_retreats_when_genuinely_outmatched() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);
        let attacker = make_force(
            atk_nation,
            (0..2)
                .map(|i| make_unit(i, ArmyUnitType::Regulars, atk_nation))
                .collect(),
        );
        let defender = make_force(
            def_nation,
            (10..18)
                .map(|i| make_unit(i, ArmyUnitType::Minutemen, def_nation))
                .collect(),
        );
        let cfg = BattleConfig {
            targeting: TargetingPriority::StrongestFirst,
            attacker_can_retreat: true,
            defender_can_retreat: false,
            attacker_retreat_ratio: 2.0,
            defender_retreat_ratio: 2.0,
            attacker_postbattle_fp_loss: 0.60,
            defender_postbattle_fp_loss: 0.60,
        };
        let result = resolve_battle_with_config(
            &attacker,
            &defender,
            ProvinceId(1),
            None,
            0,
            cfg,
            &GameConfig::default(),
        );
        assert!(
            result.retreated,
            "outmatched attacker should still bail pre-battle"
        );
    }

    #[test]
    fn outnumbered_defender_does_not_retreat_when_eliminated() {
        // A lone Militia against 5 Guards is eliminated before it can retreat.
        let atk_nation = NationId(1);
        let def_nation = NationId(2);
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Guards, atk_nation),
                make_unit(2, ArmyUnitType::Guards, atk_nation),
                make_unit(3, ArmyUnitType::Guards, atk_nation),
                make_unit(4, ArmyUnitType::Guards, atk_nation),
                make_unit(5, ArmyUnitType::Guards, atk_nation),
            ],
        );
        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Minutemen, def_nation)],
        );
        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        assert!(
            !result.defender_retreated,
            "eliminated defender should not be flagged as retreated"
        );
    }

    // ── #422: FPM — cavalry charge uses firepower_mounted ──────────

    #[test]
    fn hussars_use_fpm_when_charging() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // One Hussar: FPN=7, FPM=10, range=1, Cavalry → should charge using FPM.
        // One Skirmisher for comparison (FPN=5, range=1, not Cavalry → FPM unused).
        let hussar_fp = ArmyUnit::new(
            crate::map::UnitId(1),
            ArmyUnitType::Hussars,
            atk_nation,
            ProvinceId(1),
        )
        .effective_firepower_charging();

        let skirmisher_fp = ArmyUnit::new(
            crate::map::UnitId(2),
            ArmyUnitType::Skirmishers,
            atk_nation,
            ProvinceId(1),
        )
        .effective_firepower_charging();

        // Hussar charging FP (FPM=10) should exceed its FPN (7).
        let hussar_regular_fp = ArmyUnit::new(
            crate::map::UnitId(1),
            ArmyUnitType::Hussars,
            atk_nation,
            ProvinceId(1),
        )
        .effective_firepower();

        assert!(
            hussar_fp > hussar_regular_fp,
            "charging Hussar should use FPM > FPN"
        );
        // Skirmisher (not cavalry) should be unchanged.
        assert!(
            (skirmisher_fp
                - ArmyUnit::new(
                    crate::map::UnitId(2),
                    ArmyUnitType::Skirmishers,
                    atk_nation,
                    ProvinceId(1)
                )
                .effective_firepower())
            .abs()
                < f64::EPSILON,
            "non-cavalry should use regular FPN when charging"
        );

        // In an equal-unit battle, Hussars as attacker deal more damage than Skirmishers
        // because attacker firepower uses FPM. This is just a sanity check that the
        // battle resolves (not a strict outcome check since DEF changes balance).
        let attacker = make_force(
            atk_nation,
            vec![make_unit(1, ArmyUnitType::Hussars, atk_nation)],
        );
        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::Regulars, def_nation)],
        );
        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        // Confirm battle runs without panic.
        let _ = result;
    }

    #[test]
    fn non_cavalry_charging_fp_unchanged() {
        let nation = NationId(1);
        let types = [
            ArmyUnitType::Regulars,
            ArmyUnitType::Artillery,
            ArmyUnitType::Grenadiers,
        ];
        for unit_type in types {
            let unit = ArmyUnit::new(crate::map::UnitId(99), unit_type, nation, ProvinceId(1));
            assert!(
                (unit.effective_firepower_charging() - unit.effective_firepower()).abs()
                    < f64::EPSILON,
                "{unit_type} should have identical FPN and charging FP"
            );
        }
    }

    // ── #423: DEF — per-unit defense replaces flat 1.2 multiplier ──

    #[test]
    fn high_def_unit_survives_longer_than_low_def() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // Two separate defender compositions, same attacker.
        // RifleInfantry DEF=7 should fare better than Skirmishers DEF=5 under the same assault.
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::RifleInfantry, atk_nation),
                make_unit(2, ArmyUnitType::RifleInfantry, atk_nation),
                make_unit(3, ArmyUnitType::RifleInfantry, atk_nation),
            ],
        );

        let defender_high_def = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::RifleInfantry, def_nation)], // DEF=7
        );
        let defender_low_def = make_force(
            def_nation,
            vec![make_unit(20, ArmyUnitType::Skirmishers, def_nation)], // DEF=5
        );

        let result_high = resolve_battle(&attacker, &defender_high_def, ProvinceId(1), None, 0);
        let result_low = resolve_battle(&attacker, &defender_low_def, ProvinceId(1), None, 0);

        // Higher-FPN defender produces higher initial defensive FP under DEF formula.
        assert!(
            result_high.defender_initial_fp > result_low.defender_initial_fp,
            "RifleInfantry (FPN=15) should have higher initial defensive FP than Skirmishers (FPN=5): high={}, low={}",
            result_high.defender_initial_fp,
            result_low.defender_initial_fp
        );
    }

    #[test]
    fn siege_artillery_in_fort_is_much_harder_to_kill() {
        let atk_nation = NationId(1);
        let def_nation = NationId(2);

        // SiegeArtillery: DEF=9, defense_terrain_bonus=11 → in fort it contributes huge DEF.
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Regulars, atk_nation),
                make_unit(2, ArmyUnitType::Regulars, atk_nation),
            ],
        );
        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::SiegeArtillery, def_nation)],
        );

        // No fort: attacker has a fighting chance.
        let no_fort = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        // With fort level 3: SiegeArtillery gets its full DEF + terrain bonus.
        let with_fort = resolve_battle(&attacker, &defender, ProvinceId(1), None, 3);

        // Fortified SiegeArtillery gains its terrain bonus, so defender initial FP must be higher.
        assert!(
            with_fort.defender_initial_fp > no_fort.defender_initial_fp,
            "SiegeArtillery in fort should have higher initial FP than in open field: fort={}, open={}",
            with_fort.defender_initial_fp,
            no_fort.defender_initial_fp
        );
    }

    #[test]
    fn per_unit_terrain_bonus_does_not_apply_on_plains() {
        // On grassland (no terrain bonus), per-unit defense_terrain_bonus should be ignored.
        let cfg = GameConfig::default();
        let units = vec![ArmyUnit::new(
            crate::map::UnitId(1),
            ArmyUnitType::SiegeArtillery, // defense_terrain_bonus = 11
            NationId(1),
            ProvinceId(1),
        )];

        // Grassland: per-unit bonus should NOT apply (qualifies_for_per_unit_terrain = false)
        let fp_plains =
            defender_total_firepower(&units, Some(TerrainType::Grassland), 0, false, &cfg);
        // Forest: per-unit bonus SHOULD apply
        let fp_forest = defender_total_firepower(&units, Some(TerrainType::Forest), 0, false, &cfg);

        assert!(
            fp_forest > fp_plains,
            "forest should give more defender FP than plains due to terrain bonus"
        );
    }
}
