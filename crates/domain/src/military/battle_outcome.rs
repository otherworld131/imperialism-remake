//! Pure battle-outcome computation — no `GameState` mutation.
//!
//! [`compute_battle_outcome`] wraps [`resolve_battle_with_config`] and
//! converts its [`BattleResult`] into a [`BattleOutcome`] that describes
//! everything that _should_ happen without actually mutating anything.
//! The intended caller, [`crate::turn::processor`], is responsible for
//! applying the outcome to the game state.

use crate::data::GameConfig;
use crate::events::HistoryEvent;
use crate::map::UnitId;
use crate::military::combat::{
    BattleConfig, BattleResult, CombatForce, TargetingPriority, resolve_battle_with_config,
};
use crate::military::units::ArmyUnit;
use crate::types::{NationId, ProvinceId, TerrainType};
use std::collections::HashMap;

/// The result of [`compute_battle_outcome`]: a pure description of what
/// happened in a battle, with no mutation applied to game state.
#[derive(Debug, Clone)]
pub struct BattleOutcome {
    /// The nation that won the battle.
    pub winner: NationId,
    /// The attacking nation.
    pub attacker_id: NationId,
    /// The defending nation.
    pub defender_id: NationId,
    /// The province that was fought over.
    pub target_province: ProvinceId,
    /// Health damage dealt to each unit (initial_health − surviving_health).
    /// Units that were destroyed appear with their full initial health as damage.
    pub casualties: HashMap<UnitId, u32>,
    /// Medal awards for surviving units on the winning side: `(unit_id, new_medal_count)`.
    pub medals_awarded: Vec<(UnitId, u32)>,
    /// If the attacker won, the province conquest that should be applied.
    pub province_change: Option<ProvinceConquest>,
    /// Morale changes per unit — always empty in the current engine (no morale system yet).
    pub morale_changes: HashMap<UnitId, i32>,
    /// History events that should be recorded when the outcome is applied.
    pub history_events: Vec<HistoryEvent>,
    /// Whether the attacker retreated (pre-battle bailout or mid-combat losses).
    pub attacker_retreated: bool,
    /// Whether the defender retreated (evacuated the province).
    pub defender_retreated: bool,
    /// Whether siege artillery was present and reduced the fort's defense bonus.
    /// When `true`, the caller should decrement `fort_level` on the battle province.
    pub siege_reduced_fort: bool,
    /// Surviving attacker units with updated health and medals.
    pub attacker_survivors: Vec<ArmyUnit>,
    /// Surviving defender units with updated health.
    pub defender_survivors: Vec<ArmyUnit>,
    /// The raw [`BattleResult`] from `resolve_battle_with_config`.
    /// Carried here so callers can push it into `TurnReport::battles` for the
    /// frontend without re-running the simulation.
    pub raw_result: BattleResult,
}

/// Describes a province ownership change that should be applied when
/// the battle outcome is committed to game state.
#[derive(Debug, Clone)]
pub struct ProvinceConquest {
    pub province_id: ProvinceId,
    pub new_owner: NationId,
    pub old_owner: NationId,
}

/// Geographic context of a battle site (terrain and fortification level).
///
/// Used as a single parameter to [`BattleParams::with_default_config`] to
/// stay within the project's 7-argument function limit.
#[derive(Debug, Clone, Copy, Default)]
pub struct BattleSite {
    /// Terrain at the battle site (`None` for open ground).
    pub terrain: Option<TerrainType>,
    /// Fort level at the battle site (`0` = no fort).
    pub fort_level: u8,
}

impl BattleSite {
    /// Open ground with no fortification.
    pub fn open() -> Self {
        Self { terrain: None, fort_level: 0 }
    }

    /// Specific terrain with no fortification.
    pub fn terrain(terrain: TerrainType) -> Self {
        Self { terrain: Some(terrain), fort_level: 0 }
    }

    /// Open ground with a fort.
    pub fn fort(level: u8) -> Self {
        Self { terrain: None, fort_level: level }
    }
}

/// All inputs needed to compute a single battle outcome.
///
/// Passed to [`compute_battle_outcome`] as a bundle to stay within
/// the project's 7-argument function limit.
pub struct BattleParams<'a> {
    /// Nation that initiated the attack.
    pub attacker_id: NationId,
    /// Nation that owns `target_province`.
    pub defender_id: NationId,
    /// The province being contested.
    pub target_province: ProvinceId,
    /// Units participating in the attack (pre-battle state).
    pub attacker_units: &'a [ArmyUnit],
    /// Units defending the province (pre-battle state).
    pub defender_units: &'a [ArmyUnit],
    /// Terrain at the battle site (`None` if unknown or open ground).
    pub terrain: Option<TerrainType>,
    /// Fort level at the battle site (`0` = no fort).
    pub fort_level: u8,
    /// Retreat rules and targeting priority for this battle.
    pub battle_config: BattleConfig,
    /// Global game constants (terrain/fort bonus values, etc.).
    pub game_config: &'a GameConfig,
}

impl<'a> BattleParams<'a> {
    /// Construct params with the default [`BattleConfig`] (strongest-first
    /// targeting, standard retreat thresholds from `game_config`).
    ///
    /// `site` bundles terrain and fort level into a single argument so the
    /// function stays within the project's 7-argument limit.
    pub fn with_default_config(
        attacker_id: NationId,
        defender_id: NationId,
        target_province: ProvinceId,
        attacker_units: &'a [ArmyUnit],
        defender_units: &'a [ArmyUnit],
        site: BattleSite,
        game_config: &'a GameConfig,
    ) -> Self {
        Self {
            attacker_id,
            defender_id,
            target_province,
            attacker_units,
            defender_units,
            terrain: site.terrain,
            fort_level: site.fort_level,
            battle_config: BattleConfig::with_targeting(
                TargetingPriority::StrongestFirst,
                game_config,
            ),
            game_config,
        }
    }
}

/// Compute a battle outcome **without** mutating any game state.
///
/// Takes all the inputs bundled in [`BattleParams`] and returns a
/// [`BattleOutcome`] describing the result.  The caller is responsible for
/// applying the outcome (moving units, updating province ownership,
/// recording history, etc.).
pub fn compute_battle_outcome(params: BattleParams<'_>) -> BattleOutcome {
    let BattleParams {
        attacker_id,
        defender_id,
        target_province,
        attacker_units,
        defender_units,
        terrain,
        fort_level,
        battle_config,
        game_config,
    } = params;

    debug_assert!(
        attacker_id != defender_id,
        "attacker and defender must be different nations"
    );
    debug_assert!(
        attacker_units
            .iter()
            .all(|u| u.owner == attacker_id),
        "attacker_units contain units not owned by the attacker"
    );
    debug_assert!(
        defender_units
            .iter()
            .all(|u| u.owner == defender_id),
        "defender_units contain units not owned by the defender"
    );

    // Snapshot initial unit health and medals before the battle so we can
    // compute per-unit damage and medal awards afterwards.
    let initial_health: HashMap<UnitId, u8> = attacker_units
        .iter()
        .chain(defender_units.iter())
        .map(|u| (u.id, u.health))
        .collect();
    let initial_medals: HashMap<UnitId, u8> = attacker_units
        .iter()
        .chain(defender_units.iter())
        .map(|u| (u.id, u.medals))
        .collect();

    let attacker_force = CombatForce {
        nation: attacker_id,
        units: attacker_units.to_vec(),
    };
    let defender_force = CombatForce {
        nation: defender_id,
        units: defender_units.to_vec(),
    };

    let result = resolve_battle_with_config(
        &attacker_force,
        &defender_force,
        target_province,
        terrain,
        fort_level,
        battle_config,
        game_config,
    );

    // ── Casualties ─────────────────────────────────────────────────────────
    // For every unit that participated, compute how much health was lost.
    // Destroyed units (not in either survivor list) count as full initial health.
    let mut casualties: HashMap<UnitId, u32> = HashMap::new();

    let survivor_health: HashMap<UnitId, u8> = result
        .attacker_survivors
        .iter()
        .chain(result.defender_survivors.iter())
        .map(|u| (u.id, u.health))
        .collect();

    for u in attacker_units.iter().chain(defender_units.iter()) {
        let before = *initial_health.get(&u.id).unwrap_or(&u.health) as u32;
        let after = *survivor_health.get(&u.id).unwrap_or(&0) as u32;
        let damage = before.saturating_sub(after);
        if damage > 0 {
            casualties.insert(u.id, damage);
        }
    }

    // ── Medal awards ────────────────────────────────────────────────────────
    // Find survivors on the winning side whose medal count increased.
    let winning_survivors = if result.attacker_won {
        result.attacker_survivors.as_slice()
    } else {
        result.defender_survivors.as_slice()
    };
    let medals_awarded: Vec<(UnitId, u32)> = winning_survivors
        .iter()
        .filter_map(|u| {
            let before = *initial_medals.get(&u.id).unwrap_or(&u.medals);
            if u.medals > before {
                Some((u.id, u.medals as u32))
            } else {
                None
            }
        })
        .collect();

    // ── Province conquest ───────────────────────────────────────────────────
    let province_change = if result.attacker_won {
        Some(ProvinceConquest {
            province_id: target_province,
            new_owner: attacker_id,
            old_owner: defender_id,
        })
    } else {
        None
    };

    // ── History events ──────────────────────────────────────────────────────
    let history_events = if result.attacker_won {
        vec![HistoryEvent::ProvinceConquered {
            conqueror: attacker_id,
            loser: defender_id,
            province: target_province,
        }]
    } else {
        Vec::new()
    };

    // ── Winner ──────────────────────────────────────────────────────────────
    let winner = if result.attacker_won {
        attacker_id
    } else {
        defender_id
    };

    BattleOutcome {
        winner,
        attacker_id,
        defender_id,
        target_province,
        casualties,
        medals_awarded,
        province_change,
        morale_changes: HashMap::new(),
        history_events,
        attacker_retreated: result.retreated,
        defender_retreated: result.defender_retreated,
        siege_reduced_fort: result.siege_reduced_fort,
        attacker_survivors: result.attacker_survivors.clone(),
        defender_survivors: result.defender_survivors.clone(),
        raw_result: result,
    }
}

// Tests live in `tests/military.rs` (integration tests) to avoid the
// pre-existing domain-crate test-infrastructure issue that prevents inline
// `#[cfg(test)]` modules from compiling in isolation.
