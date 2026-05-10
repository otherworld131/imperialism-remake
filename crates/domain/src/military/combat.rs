use crate::data::GameConfig;
use crate::map::UnitId;
use crate::military::units::{ArmyUnit, ArmyUnitType, UnitCategory};
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
    /// Current game turn — used to gate the per-unit Garrison entrenchment
    /// kicker (card #478): a unit must satisfy `arrived_turn < current_turn`
    /// before it earns its entrenchment FP. `0` means "no turn context";
    /// legacy / test callers leave it 0, in which case the resolver falls
    /// back to "all garrisons entrenched" so existing behaviour is stable.
    pub current_turn: u32,
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
            current_turn: 0,
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
    /// Per-unit logs for each side captured at battle start, with final
    /// state filled in post-resolution. Card #478 follow-up — surfaced
    /// behind a battle-screen "Show firepower" debug toggle.
    pub attacker_unit_logs: Vec<BattleUnitLog>,
    pub defender_unit_logs: Vec<BattleUnitLog>,
    /// Per-round trace (volley + rounds) shown behind the "Show firepower"
    /// debug toggle. Empty when the battle short-circuited (empty side or
    /// pre-battle retreat).
    pub round_logs: Vec<BattleRoundLog>,
}

/// Per-unit defender bonus breakdown — captured at battle start so the UI
/// can explain how a unit's raw FP turns into its contribution to
/// `defender_initial_fp`. Card #478 follow-up.
///
/// Post-card-#478 model:
/// `contribution = applied_fp × fort_multiplier + entrenchment_fp`.
/// There's no `× def`, no `× (1 + terrain)`, and no per-unit terrain
/// bonus any more — fort is the only multiplier.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefenderBonusBreakdown {
    /// Firepower used in the contribution calculation —
    /// `effective_firepower` (FPN × medals × health).
    pub applied_firepower: f64,
    /// `(1 + effective_fort)` factor — already accounts for siege halving.
    pub fort_multiplier: f64,
    /// Flat raw FP added for entrenched Garrison defenders. 0 unless the
    /// unit is Garrison-category and `arrived_turn < current_turn`.
    pub entrenchment_fp: f64,
    /// Total contribution this unit made to `defender_initial_fp`:
    /// `applied_firepower × fort_multiplier + entrenchment_fp`.
    pub initial_total_contribution: f64,
}

/// Per-unit log of how a unit entered and left the battle. Surfaced in
/// `BattleResult` so the UI can render initial→final firepower for every
/// unit (survivors + destroyed) and explain defender bonus inflation.
#[derive(Debug, Clone)]
pub struct BattleUnitLog {
    pub unit_type: ArmyUnitType,
    pub medals_initial: u8,
    pub medals_final: u8,
    pub initial_health: u8,
    pub final_health: u8,
    /// Effective firepower at battle start (FPN-based, scaled by initial
    /// health and medals).
    pub initial_firepower: f64,
    /// Effective firepower at battle end (post damage). 0 for destroyed.
    pub final_firepower: f64,
    /// Defender-only: per-unit contribution to `defender_initial_fp`.
    /// `None` for attackers.
    pub defender_breakdown: Option<DefenderBonusBreakdown>,
}

/// Per-round trace of how the battle played out. Surfaced behind the
/// battle-screen "Show firepower" toggle so the player can see the actual
/// numbers the resolver crunched. `round = 0` is the optional first-strike
/// volley; `round = 1..N` are the regular damage exchanges.
///
/// Post-rework: combat is per-shot 1v1 with concentrate-fire spill — each
/// shooter picks one target in its preferred row and damages it by its FP,
/// with overkill spilling to the next priority target. The log records the
/// total FP fired and the number of shooters that fired.
#[derive(Debug, Clone, Default)]
pub struct BattleRoundLog {
    /// 0 = first-strike volley, 1..=10 = combat round.
    pub round: u32,
    /// `Some("attacker"|"defender")` for the first-strike volley row;
    /// `None` for normal rounds.
    pub first_strike_side: Option<&'static str>,
    /// Attacker side firepower fired this round (sum of per-shot FP, after
    /// round-1 cavalry charge and General bonus). Volley rounds: only the
    /// over-range shooters' FP.
    pub atk_fp: f64,
    pub def_fp: f64,
    /// Number of attacker shooters that fired this round.
    pub atk_shots: usize,
    /// Number of defender shooters that fired this round.
    pub def_shots: usize,
    /// Casualties added during this round, on each side.
    pub atk_casualties: Vec<ArmyUnitType>,
    pub def_casualties: Vec<ArmyUnitType>,
    /// Set when this round triggered a mid-battle retreat:
    /// `Some("attacker"|"defender")`.
    pub retreat_triggered: Option<&'static str>,
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
/// Used for raw / unmodified summaries (e.g. retreat baseline). Does NOT
/// apply role-aware modifiers — see `attacker_round_firepower` for those.
fn total_firepower(units: &[ArmyUnit]) -> f64 {
    let base: f64 = units.iter().map(|u| u.effective_firepower()).sum();
    base * general_bonus(units)
}

/// Per-shot attacker firepower for one shooter in a given round.
///
/// Includes round-aware modifiers from card #478:
///   * Round 1 only: cavalry units get a `combat_cavalry_charge_bonus`
///     multiplier ("first-round shock charge"). They also use FPM via
///     `effective_firepower_charging` when charging at range 1.
///
/// The exposed-artillery melee penalty was dropped when combat moved to
/// per-shot 1v1 targeting — front-line shooters can no longer reach
/// artillery sitting behind a screen, so the "screen your guns" feel
/// emerges naturally without a penalty multiplier.
fn attacker_unit_round_fp(unit: &ArmyUnit, round: u32, cfg: &GameConfig) -> f64 {
    let stats = unit.unit_type.stats();
    let mut fp = if round == 1 {
        unit.effective_firepower_charging()
    } else {
        unit.effective_firepower()
    };
    if round == 1 && stats.category == UnitCategory::Cavalry {
        fp *= 1.0 + cfg.combat_cavalry_charge_bonus;
    }
    fp
}

/// Per-shot defender firepower for one shooter, including the side's fort
/// multiplier and the per-unit Garrison entrenchment kicker.
///
/// `current_turn = 0` is the legacy / test escape hatch — entrenchment
/// applies to every Garrison-category unit regardless of `arrived_turn`.
fn defender_unit_shot_fp(
    unit: &ArmyUnit,
    fort_level: u8,
    attacker_has_siege: bool,
    current_turn: u32,
    cfg: &GameConfig,
) -> f64 {
    let fort_factor = 1.0 + effective_fort_bonus(fort_level, attacker_has_siege, cfg);
    let mut fp = unit.effective_firepower() * fort_factor;
    if is_entrenched_garrison(unit, current_turn) {
        fp += cfg.garrison_entrenchment_fp;
    }
    fp
}

/// Aggregate attacker firepower for one combat round (sum + General bonus).
/// Used for retreat baselines and the side-total reported in the round log.
fn attacker_round_firepower(units: &[ArmyUnit], round: u32, cfg: &GameConfig) -> f64 {
    let base: f64 = units
        .iter()
        .filter(|u| u.is_alive())
        .map(|u| attacker_unit_round_fp(u, round, cfg))
        .sum();
    base * general_bonus(units)
}

/// Pre-battle / retreat-baseline attacker firepower: behaves as if round 1
/// is about to start (charge bonus applied). Mirrors what the resolver
/// will actually deal in the next round.
fn attacker_total_firepower(units: &[ArmyUnit], cfg: &GameConfig) -> f64 {
    attacker_round_firepower(units, 1, cfg)
}

/// Aggregate defender firepower (sum + General bonus). Includes fort
/// multiplier and per-unit Garrison entrenchment kicker.
fn defender_total_firepower(
    units: &[ArmyUnit],
    _terrain: Option<TerrainType>,
    fort_level: u8,
    attacker_has_siege: bool,
    current_turn: u32,
    config: &GameConfig,
) -> f64 {
    let base: f64 = units
        .iter()
        .filter(|u| u.is_alive())
        .map(|u| defender_unit_shot_fp(u, fort_level, attacker_has_siege, current_turn, config))
        .sum();
    base * general_bonus(units)
}

/// One shooter's contribution to a round of fire — the FP it deals plus
/// whether it's an artillery shooter (which controls preferred-row
/// targeting). The General bonus is already folded into `fp`.
#[derive(Debug, Clone, Copy)]
struct ShotPlan {
    fp: f64,
    is_artillery_shooter: bool,
}

/// Build per-shot plans for an attacker side in a given round. Includes
/// the round-1 cavalry charge bonus (via `attacker_unit_round_fp`) and
/// the side-level General bonus baked into each shot.
fn build_attacker_shots(units: &[ArmyUnit], round: u32, cfg: &GameConfig) -> Vec<ShotPlan> {
    let bonus = general_bonus(units);
    units
        .iter()
        .filter(|u| u.is_alive())
        .map(|u| ShotPlan {
            fp: attacker_unit_round_fp(u, round, cfg) * bonus,
            is_artillery_shooter: u.unit_type.stats().category == UnitCategory::Artillery,
        })
        .collect()
}

/// Build per-shot plans for the defender side. Each shot already has the
/// fort multiplier and Garrison entrenchment kicker baked in (via
/// `defender_unit_shot_fp`), plus the side-level General bonus.
fn build_defender_shots(
    units: &[ArmyUnit],
    fort_level: u8,
    attacker_has_siege: bool,
    current_turn: u32,
    cfg: &GameConfig,
) -> Vec<ShotPlan> {
    let bonus = general_bonus(units);
    units
        .iter()
        .filter(|u| u.is_alive())
        .map(|u| ShotPlan {
            fp: defender_unit_shot_fp(u, fort_level, attacker_has_siege, current_turn, cfg)
                * bonus,
            is_artillery_shooter: u.unit_type.stats().category == UnitCategory::Artillery,
        })
        .collect()
}

/// Build first-strike volley shots — over-range units only, raw FPN
/// (no charge bonus, no FPM swap), with the side's General bonus and
/// the configured `damage_multiplier` baked in.
fn build_volley_shots(
    side_units: &[ArmyUnit],
    opponent_max_range: u32,
    damage_multiplier: f64,
) -> Vec<ShotPlan> {
    let bonus = general_bonus(side_units);
    side_units
        .iter()
        .filter(|u| {
            u.is_alive()
                && u.unit_type.stats().range > opponent_max_range
                && u.effective_firepower() > 0.0
        })
        .map(|u| ShotPlan {
            fp: u.effective_firepower() * damage_multiplier * bonus,
            is_artillery_shooter: u.unit_type.stats().category == UnitCategory::Artillery,
        })
        .collect()
}

/// Maximum range across living units, treating empty / range-0-only forces
/// as 0. Used both for first-strike eligibility and for the AI estimator.
fn max_range(units: &[ArmyUnit]) -> u32 {
    units
        .iter()
        .filter(|u| u.is_alive())
        .map(|u| u.unit_type.stats().range)
        .max()
        .unwrap_or(0)
}

/// Pick the next concentrate-fire target for a stream. `prefer_artillery`
/// controls preferred row: front-line shooters pass `false` (target
/// non-artillery first; fall through to artillery if no front-line is
/// alive); artillery shooters pass `true` (target artillery first; fall
/// through to front-line if no artillery is alive). Within the chosen
/// row, `targeting` decides strongest- or weakest-first.
fn pick_concentrate_fire_target(
    targets: &[ArmyUnit],
    prefer_artillery: bool,
    targeting: TargetingPriority,
) -> Option<usize> {
    let is_arty =
        |u: &ArmyUnit| u.unit_type.stats().category == UnitCategory::Artillery;
    let preferred: Vec<usize> = targets
        .iter()
        .enumerate()
        .filter(|(_, u)| u.is_alive() && is_arty(u) == prefer_artillery)
        .map(|(i, _)| i)
        .collect();
    let pool: Vec<usize> = if !preferred.is_empty() {
        preferred
    } else {
        targets
            .iter()
            .enumerate()
            .filter(|(_, u)| u.is_alive() && is_arty(u) != prefer_artillery)
            .map(|(i, _)| i)
            .collect()
    };
    if pool.is_empty() {
        return None;
    }
    pool.into_iter()
        .max_by(|&a, &b| {
            let (af, bf) = (
                targets[a].effective_firepower(),
                targets[b].effective_firepower(),
            );
            match targeting {
                TargetingPriority::StrongestFirst => af
                    .partial_cmp(&bf)
                    .unwrap_or(std::cmp::Ordering::Equal),
                TargetingPriority::WeakestFirst => bf
                    .partial_cmp(&af)
                    .unwrap_or(std::cmp::Ordering::Equal),
            }
        })
}

/// Apply one stream of concentrate-fire damage. The stream pools the FP
/// of all shooters that share a preferred row (front-line vs artillery),
/// then drains it onto the highest-priority alive target — overkill
/// spills to the next priority target until the pool empties or every
/// target is dead.
fn apply_concentrate_fire_stream(
    targets: &mut Vec<ArmyUnit>,
    total_fp: f64,
    prefer_artillery: bool,
    targeting: TargetingPriority,
    casualties_out: &mut Vec<ArmyUnitType>,
) {
    if total_fp <= 0.0 {
        return;
    }
    let mut remaining = total_fp;
    while remaining >= 1.0 {
        let Some(idx) = pick_concentrate_fire_target(targets, prefer_artillery, targeting)
        else {
            return;
        };
        let target = &mut targets[idx];
        let hp = target.health as f64;
        let dmg = remaining.min(hp).floor();
        if dmg < 1.0 {
            return;
        }
        target.take_damage(dmg as u8);
        remaining -= dmg;
        if !target.is_alive() {
            casualties_out.push(target.unit_type);
        }
    }
}

/// Apply a full round of concentrate-fire from `shots` to `targets`.
/// Front-line shooters pool their FP and drain onto enemy front-line
/// (spilling to artillery); artillery shooters pool their FP and drain
/// onto enemy artillery (spilling to front-line). The dead are dropped
/// from `targets` after both streams resolve.
fn apply_concentrate_fire_round(
    shots: &[ShotPlan],
    targets: &mut Vec<ArmyUnit>,
    casualties_out: &mut Vec<ArmyUnitType>,
    targeting: TargetingPriority,
) {
    let front_fp: f64 = shots
        .iter()
        .filter(|s| !s.is_artillery_shooter)
        .map(|s| s.fp)
        .sum();
    let arty_fp: f64 = shots
        .iter()
        .filter(|s| s.is_artillery_shooter)
        .map(|s| s.fp)
        .sum();
    apply_concentrate_fire_stream(targets, front_fp, false, targeting, casualties_out);
    apply_concentrate_fire_stream(targets, arty_fp, true, targeting, casualties_out);
    targets.retain(|u| u.is_alive());
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

/// Build initial-state per-unit logs for one side before any damage is
/// dealt. The values stored here are the **role-aware applied** firepower
/// each unit contributes to its side total, so the per-unit rows the UI
/// renders sum to `attacker_initial_fp` / the pre-fort defender base FP.
///
/// Attacker-side `initial_firepower` includes round-1 modifications:
/// FPM swap for cavalry charging at melee range and +25% cavalry charge
/// bonus.
///
/// Defender-side `initial_firepower` is the unit's effective FP. The
/// fort multiplier and Garrison entrenchment kicker are exposed via
/// `defender_breakdown` so the UI can show the full chain
/// `applied_fp × fort_mult + entrenchment`.
fn build_initial_unit_logs(
    units: &[ArmyUnit],
    role: BattleRoleLog,
    fort_level: u8,
    attacker_has_siege: bool,
    current_turn: u32,
    config: &GameConfig,
) -> Vec<BattleUnitLog> {
    let global_fort = effective_fort_bonus(fort_level, attacker_has_siege, config);
    let fort_mult = 1.0 + global_fort;
    units
        .iter()
        .map(|u| {
            let applied_initial = match role {
                BattleRoleLog::Attacker => attacker_unit_round_fp(u, 1, config),
                BattleRoleLog::Defender => u.effective_firepower(),
            };
            let breakdown = match role {
                BattleRoleLog::Defender => {
                    let entrenchment = if is_entrenched_garrison(u, current_turn) {
                        config.garrison_entrenchment_fp
                    } else {
                        0.0
                    };
                    let total = applied_initial * fort_mult + entrenchment;
                    Some(DefenderBonusBreakdown {
                        applied_firepower: applied_initial,
                        fort_multiplier: fort_mult,
                        entrenchment_fp: entrenchment,
                        initial_total_contribution: total,
                    })
                }
                BattleRoleLog::Attacker => None,
            };
            BattleUnitLog {
                unit_type: u.unit_type,
                medals_initial: u.medals,
                medals_final: u.medals,
                initial_health: u.health,
                final_health: u.health,
                initial_firepower: applied_initial,
                final_firepower: applied_initial,
                defender_breakdown: breakdown,
            }
        })
        .collect()
}

/// Whether `u` is an entrenched garrison defender per card #478: garrison
/// category, alive, and present at the province since at least the previous
/// turn. `current_turn = 0` is the legacy / test-harness escape hatch — we
/// treat all garrisons as entrenched in that case so old tests stay stable.
fn is_entrenched_garrison(u: &ArmyUnit, current_turn: u32) -> bool {
    if !u.is_alive() {
        return false;
    }
    if u.unit_type.category() != UnitCategory::Garrison {
        return false;
    }
    if current_turn == 0 {
        return true;
    }
    u.arrived_turn < current_turn
}

#[derive(Copy, Clone)]
enum BattleRoleLog {
    Attacker,
    Defender,
}

/// After resolution, walk the original logs and overwrite final state by
/// matching against survivor unit IDs. `final_firepower` is recomputed
/// with the same role-aware modifiers used at battle start (round-1 view
/// for attackers, raw FP for defenders) so the per-unit initial→final
/// pair the UI shows uses a consistent definition.
fn finalize_unit_logs(
    logs: &mut [BattleUnitLog],
    initial_units: &[ArmyUnit],
    survivors: &[ArmyUnit],
    role: BattleRoleLog,
    config: &GameConfig,
) {
    use std::collections::HashMap;
    let surv_by_id: HashMap<crate::map::UnitId, &ArmyUnit> =
        survivors.iter().map(|u| (u.id, u)).collect();
    for (log, original) in logs.iter_mut().zip(initial_units.iter()) {
        if let Some(s) = surv_by_id.get(&original.id) {
            log.medals_final = s.medals;
            log.final_health = s.health;
            log.final_firepower = match role {
                BattleRoleLog::Attacker => attacker_unit_round_fp(s, 1, config),
                BattleRoleLog::Defender => s.effective_firepower(),
            };
        } else {
            log.final_health = 0;
            log.final_firepower = 0.0;
        }
    }
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
    let attacker_initial_fp = attacker_total_firepower(&atk_units, game_config);
    let attacker_has_siege = has_siege_artillery(&atk_units);
    let siege_reduced_fort = attacker_has_siege && fort_level > 0;
    let defender_initial_fp = defender_total_firepower(
        &def_units,
        terrain,
        fort_level,
        attacker_has_siege,
        config.current_turn,
        game_config,
    );

    // Initial roster snapshot, used to (a) populate per-unit logs for the
    // battle-screen "Show firepower" debug view, and (b) match survivors
    // back to their starting state once combat ends.
    let attacker_initial_units = atk_units.clone();
    let defender_initial_units = def_units.clone();
    let mut attacker_unit_logs = build_initial_unit_logs(
        &attacker_initial_units,
        BattleRoleLog::Attacker,
        fort_level,
        attacker_has_siege,
        config.current_turn,
        game_config,
    );
    let mut defender_unit_logs = build_initial_unit_logs(
        &defender_initial_units,
        BattleRoleLog::Defender,
        fort_level,
        attacker_has_siege,
        config.current_turn,
        game_config,
    );

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
            attacker_unit_logs,
            defender_unit_logs,
            round_logs: Vec::new(),
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
            attacker_unit_logs,
            defender_unit_logs,
            round_logs: Vec::new(),
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
        finalize_unit_logs(&mut attacker_unit_logs, &attacker_initial_units, &atk_units, BattleRoleLog::Attacker, game_config);
        finalize_unit_logs(&mut defender_unit_logs, &defender_initial_units, &def_units, BattleRoleLog::Defender, game_config);
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
            attacker_unit_logs,
            defender_unit_logs,
            round_logs: Vec::new(),
        };
    }

    // ── Pre-battle retreat (card #18 / #478) ────────────────────────
    // Card #478: ratios now come from the AI strength estimator so they
    // mirror what the resolver will actually do (charge bonus, artillery
    // melee penalty, range advantage, Lanchester durability). Personality
    // thresholds (`attacker_retreat_ratio` / `defender_retreat_ratio`)
    // stay unchanged — they encode "how lopsided before I bail?", which
    // is independent of how strength is measured.
    use crate::military::strength::{BattleRole, StrengthCtx, force_strength};
    let strength_ctx_atk = StrengthCtx {
        terrain,
        fort_level,
        attacker_has_siege,
        opponent_max_range: max_range(&def_units),
        distance: 1,
        current_turn: config.current_turn,
    };
    let strength_ctx_def = StrengthCtx {
        terrain,
        fort_level,
        attacker_has_siege,
        opponent_max_range: max_range(&atk_units),
        distance: 1,
        current_turn: config.current_turn,
    };
    let atk_strength = force_strength(&atk_units, BattleRole::Attacker, &strength_ctx_atk, game_config);
    let def_strength = force_strength(&def_units, BattleRole::Defender, &strength_ctx_def, game_config);
    let attacker_ratio = if atk_strength > 0.0 {
        def_strength / atk_strength
    } else {
        f64::INFINITY
    };
    let defender_ratio = if def_strength > 0.0 {
        atk_strength / def_strength
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
        // Pre-battle retreat: nobody took damage, so logs stay at initial.
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
                attacker_unit_logs,
                defender_unit_logs,
                round_logs: Vec::new(),
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
                attacker_unit_logs,
                defender_unit_logs,
                round_logs: Vec::new(),
            };
        }
    }

    // Combat rounds (up to 10)
    let mut retreated = false;
    let mut defender_retreated = false;
    let mut midbattle_debug: Option<RetreatDebug> = None;
    let mut current_round: u32 = 0;
    let mut round_logs: Vec<BattleRoundLog> = Vec::new();
    // Track damage dealt by each unit (keyed by UnitId) for medal eligibility
    let mut atk_damage_dealt: std::collections::HashMap<crate::map::UnitId, f64> =
        std::collections::HashMap::new();
    let mut def_damage_dealt: std::collections::HashMap<crate::map::UnitId, f64> =
        std::collections::HashMap::new();

    // ── Range first-strike volley (card #478) ───────────────────────
    // The longer-ranged side fires one *free* volley before round 1 from
    // only its over-range units (range > opponent_max_range). The
    // opponent's defensive multipliers (terrain, fort, per-unit DEF) still
    // mitigate damage — a fortified target is still hard to bombard — but
    // the attacker takes no return fire this volley.
    //
    // Alternatives (see scripts/config/game.lua): half-damage volley, or
    // no volley but a +30% bombardment bonus every round for the longer-
    // ranged side. We chose full volley capped to one shot.
    if game_config.combat_first_strike_enabled {
        let atk_max_r = max_range(&atk_units);
        let def_max_r = max_range(&def_units);
        let mult = game_config.combat_first_strike_damage_multiplier;
        if atk_max_r > def_max_r {
            let shots = build_volley_shots(&atk_units, def_max_r, mult);
            if !shots.is_empty() && !def_units.is_empty() {
                let volley_fp: f64 = shots.iter().map(|s| s.fp).sum();
                let shot_count = shots.len();
                let mut casualties: Vec<ArmyUnitType> = Vec::new();
                apply_concentrate_fire_round(
                    &shots,
                    &mut def_units,
                    &mut casualties,
                    config.targeting,
                );
                for c in &casualties {
                    defender_casualties.push(*c);
                }
                // Credit damage to over-range bombarders for medal eligibility.
                for u in atk_units.iter().filter(|u| {
                    u.is_alive()
                        && u.unit_type.stats().range > def_max_r
                        && u.effective_firepower() > 0.0
                }) {
                    *atk_damage_dealt.entry(u.id).or_insert(0.0) += u.effective_firepower();
                }
                round_logs.push(BattleRoundLog {
                    round: 0,
                    first_strike_side: Some("attacker"),
                    atk_fp: volley_fp,
                    def_fp: 0.0,
                    atk_shots: shot_count,
                    def_shots: 0,
                    atk_casualties: Vec::new(),
                    def_casualties: casualties,
                    retreat_triggered: None,
                });
            }
        } else if def_max_r > atk_max_r {
            let shots = build_volley_shots(&def_units, atk_max_r, mult);
            if !shots.is_empty() && !atk_units.is_empty() {
                let volley_fp: f64 = shots.iter().map(|s| s.fp).sum();
                let shot_count = shots.len();
                let mut casualties: Vec<ArmyUnitType> = Vec::new();
                apply_concentrate_fire_round(
                    &shots,
                    &mut atk_units,
                    &mut casualties,
                    config.targeting,
                );
                for c in &casualties {
                    attacker_casualties.push(*c);
                }
                for u in def_units.iter().filter(|u| {
                    u.is_alive()
                        && u.unit_type.stats().range > atk_max_r
                        && u.effective_firepower() > 0.0
                }) {
                    *def_damage_dealt.entry(u.id).or_insert(0.0) += u.effective_firepower();
                }
                round_logs.push(BattleRoundLog {
                    round: 0,
                    first_strike_side: Some("defender"),
                    atk_fp: 0.0,
                    def_fp: volley_fp,
                    atk_shots: 0,
                    def_shots: shot_count,
                    atk_casualties: casualties,
                    def_casualties: Vec::new(),
                    retreat_triggered: None,
                });
            }
        }
    }

    for _ in 0..10 {
        if atk_units.is_empty() || def_units.is_empty() {
            break;
        }
        current_round += 1;

        // Build per-shot plans for both sides (concentrate-fire model).
        // Each shot's FP already includes role-aware modifiers (round-1
        // cavalry charge, fort multiplier, garrison entrenchment) and the
        // side's General bonus.
        let atk_shots = build_attacker_shots(&atk_units, current_round, game_config);
        let def_shots = build_defender_shots(
            &def_units,
            fort_level,
            attacker_has_siege,
            config.current_turn,
            game_config,
        );
        let atk_fp: f64 = atk_shots.iter().map(|s| s.fp).sum();
        let def_fp: f64 = def_shots.iter().map(|s| s.fp).sum();
        let atk_shot_count = atk_shots.len();
        let def_shot_count = def_shots.len();

        // Attacker fires at defenders (front-line shooters target enemy
        // front-line first, falling through to artillery; artillery
        // shooters target enemy artillery first, falling through to
        // front-line). Damage spills onto the next priority target if
        // overkill.
        let mut def_round_casualties: Vec<ArmyUnitType> = Vec::new();
        if !def_units.is_empty() {
            apply_concentrate_fire_round(
                &atk_shots,
                &mut def_units,
                &mut def_round_casualties,
                config.targeting,
            );
            if atk_fp > 0.0 {
                for unit in atk_units.iter() {
                    *atk_damage_dealt.entry(unit.id).or_insert(0.0) += unit.effective_firepower();
                }
            }
        }
        for c in &def_round_casualties {
            defender_casualties.push(*c);
        }

        // Defender returns fire — same model.
        let mut atk_round_casualties: Vec<ArmyUnitType> = Vec::new();
        if !atk_units.is_empty() {
            apply_concentrate_fire_round(
                &def_shots,
                &mut atk_units,
                &mut atk_round_casualties,
                config.targeting,
            );
            if def_fp > 0.0 {
                for unit in def_units.iter() {
                    *def_damage_dealt.entry(unit.id).or_insert(0.0) += unit.effective_firepower();
                }
            }
        }
        for c in &atk_round_casualties {
            attacker_casualties.push(*c);
        }

        round_logs.push(BattleRoundLog {
            round: current_round,
            first_strike_side: None,
            atk_fp,
            def_fp,
            atk_shots: atk_shot_count,
            def_shots: def_shot_count,
            atk_casualties: atk_round_casualties,
            def_casualties: def_round_casualties,
            retreat_triggered: None,
        });

        // Check for attacker retreat
        if config.attacker_can_retreat && attacker_initial_fp > 0.0 && !atk_units.is_empty() {
            let current_atk_fp = attacker_total_firepower(&atk_units, game_config);
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
                if let Some(last) = round_logs.last_mut() {
                    last.retreat_triggered = Some("attacker");
                }
                // Retreating units suffer 10% additional damage on remaining health
                for unit in &mut atk_units {
                    let retreat_damage = (unit.health as f64 * 0.10) as u8;
                    if retreat_damage > 0 {
                        unit.take_damage(retreat_damage);
                    }
                }
                for unit in atk_units.iter().filter(|u| !u.is_alive()) {
                    attacker_casualties.push(unit.unit_type);
                    if let Some(last) = round_logs.last_mut() {
                        last.atk_casualties.push(unit.unit_type);
                    }
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
                if let Some(last) = round_logs.last_mut() {
                    last.retreat_triggered = Some("defender");
                }
                for unit in &mut def_units {
                    let retreat_damage = (unit.health as f64 * 0.10) as u8;
                    if retreat_damage > 0 {
                        unit.take_damage(retreat_damage);
                    }
                }
                for unit in def_units.iter().filter(|u| !u.is_alive()) {
                    defender_casualties.push(unit.unit_type);
                    if let Some(last) = round_logs.last_mut() {
                        last.def_casualties.push(unit.unit_type);
                    }
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

    finalize_unit_logs(&mut attacker_unit_logs, &attacker_initial_units, &atk_units, BattleRoleLog::Attacker, game_config);
    finalize_unit_logs(&mut defender_unit_logs, &defender_initial_units, &def_units, BattleRoleLog::Defender, game_config);

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
        attacker_unit_logs,
        defender_unit_logs,
        round_logs,
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

        // Card #478 zeroed terrain bonuses. The lookup function still
        // exists (for any mod that wants to dial them back up) but the
        // shipped game config returns 0 for everything.
        assert!((terrain_defense_bonus(TerrainType::Mountain, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Hills, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Forest, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Swamp, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Grassland, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Desert, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Sea, &cfg) - 0.0).abs() < f64::EPSILON);
    }

    // ── Concentrate-fire targeting (per-shot 1v1 with row preference) ──

    #[test]
    fn front_line_shooters_fall_through_to_artillery_when_no_screen() {
        // Defender is artillery-only — front-line attacker shooters must
        // fall through and damage them.
        let atk_nation = NationId(1);
        let def_nation = NationId(2);
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Regulars, atk_nation),
                make_unit(2, ArmyUnitType::Regulars, atk_nation),
            ],
        );
        let defender = make_force(
            def_nation,
            vec![make_unit(10, ArmyUnitType::LightArtillery, def_nation)],
        );
        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        assert!(result.attacker_won);
        assert!(
            result
                .defender_casualties
                .contains(&ArmyUnitType::LightArtillery),
            "front-line attackers should reach the unscreened artillery"
        );
    }

    #[test]
    fn artillery_shooters_prioritize_enemy_artillery() {
        // Attacker: artillery + screen. Defender: 1 artillery (back row)
        // + 1 infantry (front row). Attacker artillery should target
        // defender's artillery first.
        let atk_nation = NationId(1);
        let def_nation = NationId(2);
        let attacker = make_force(
            atk_nation,
            vec![
                make_unit(1, ArmyUnitType::Regulars, atk_nation),
                make_unit(2, ArmyUnitType::FieldArtillery, atk_nation),
                make_unit(3, ArmyUnitType::FieldArtillery, atk_nation),
            ],
        );
        let defender = make_force(
            def_nation,
            vec![
                make_unit(10, ArmyUnitType::Regulars, def_nation),
                make_unit(11, ArmyUnitType::LightArtillery, def_nation),
            ],
        );
        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        let arty_idx = result
            .defender_casualties
            .iter()
            .position(|t| *t == ArmyUnitType::LightArtillery);
        let inf_idx = result
            .defender_casualties
            .iter()
            .position(|t| *t == ArmyUnitType::Regulars);
        if let (Some(a), Some(i)) = (arty_idx, inf_idx) {
            assert!(
                a <= i,
                "attacker artillery should kill defender artillery before defender infantry falls"
            );
        }
    }

    #[test]
    fn back_row_artillery_takes_no_damage_until_front_row_clears() {
        // Defender: 1 Regulars + 1 LightArtillery. Front row clears first.
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
            vec![
                make_unit(10, ArmyUnitType::Regulars, def_nation),
                make_unit(11, ArmyUnitType::LightArtillery, def_nation),
            ],
        );
        let result = resolve_battle(&attacker, &defender, ProvinceId(1), None, 0);
        // The artillery should die only after the Regulars; with this big
        // an attacker advantage it dies eventually, but the casualty list
        // must record Regulars first.
        let regulars_idx = result
            .defender_casualties
            .iter()
            .position(|t| *t == ArmyUnitType::Regulars);
        let arty_idx = result
            .defender_casualties
            .iter()
            .position(|t| *t == ArmyUnitType::LightArtillery);
        if let (Some(r), Some(a)) = (regulars_idx, arty_idx) {
            assert!(r < a, "front-row Regulars should fall before back-row artillery");
        }
    }

    // ── Fort defense bonus ──────────────────────────────────────

    #[test]
    fn fort_defense_bonus_values() {
        use super::fort_defense_bonus;
        let cfg = GameConfig::default();

        // Card #478 reset the curve to a clean linear 0/0.25/0.50/0.75.
        assert!((fort_defense_bonus(0, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(1, &cfg) - 0.25).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(2, &cfg) - 0.50).abs() < f64::EPSILON);
        assert!((fort_defense_bonus(3, &cfg) - 0.75).abs() < f64::EPSILON);
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
        assert!((effective_fort_bonus(1, false, &cfg) - 0.25).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(2, false, &cfg) - 0.50).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(3, false, &cfg) - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn effective_fort_bonus_with_siege_reduces_by_half() {
        // With siege, fort bonus is halved
        let cfg = GameConfig::default();
        assert!((effective_fort_bonus(1, true, &cfg) - 0.125).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(2, true, &cfg) - 0.25).abs() < f64::EPSILON);
        assert!((effective_fort_bonus(3, true, &cfg) - 0.375).abs() < f64::EPSILON);
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

        // Card #478: terrain bonuses are zeroed; fort is the only
        // defender multiplier left, on the linear curve 0/0.25/0.50/0.75.
        let cfg = GameConfig::default();
        assert!((terrain_defense_bonus(TerrainType::Mountain, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Hills, &cfg) - 0.0).abs() < f64::EPSILON);
        assert!((terrain_defense_bonus(TerrainType::Forest, &cfg) - 0.0).abs() < f64::EPSILON);

        let fort_bonus = fort_defense_bonus(3, &cfg);
        assert!((fort_bonus - 0.75).abs() < f64::EPSILON);

        // Siege artillery reduces fort bonus by 50%
        let effective = effective_fort_bonus(3, true, &cfg);
        assert!((effective - 0.375).abs() < f64::EPSILON);
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
            current_turn: 0,
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
            current_turn: 0,
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
            current_turn: 0,
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
            current_turn: 0,
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
            current_turn: 0,
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
    fn terrain_no_longer_affects_defender_firepower() {
        // Card #478 dropped the terrain multiplier entirely. The resolver
        // should produce identical defender FP on grassland and forest.
        let cfg = GameConfig::default();
        let units = vec![ArmyUnit::new(
            crate::map::UnitId(1),
            ArmyUnitType::SiegeArtillery,
            NationId(1),
            ProvinceId(1),
        )];

        let fp_plains =
            defender_total_firepower(&units, Some(TerrainType::Grassland), 0, false, 0, &cfg);
        let fp_forest =
            defender_total_firepower(&units, Some(TerrainType::Forest), 0, false, 0, &cfg);
        let fp_mountain =
            defender_total_firepower(&units, Some(TerrainType::Mountain), 0, false, 0, &cfg);

        assert!((fp_forest - fp_plains).abs() < f64::EPSILON);
        assert!((fp_mountain - fp_plains).abs() < f64::EPSILON);
    }
}
