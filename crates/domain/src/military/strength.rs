//! Role-aware strength estimator the AI uses to decide whether a force is
//! strong enough to attack, or weak enough to retreat. Card #478.
//!
//! The estimator is *separate* from `combat::resolve_battle` — the resolver
//! is the source of truth for what actually happens; this module is what
//! the AI consults *before* committing to a fight. The two consume the same
//! `GameConfig` knobs (charge bonus, artillery melee penalty, …) so the
//! estimator's "I think we'll win" matches the resolver's "we won".
//!
//! Per-unit effective strength:
//!     s(u) = fp_phase(u, role, distance)
//!          * sqrt(def_eff(u, role, terrain, fort))     (Lanchester square)
//!          * range_factor(u, role, enemy_max_range)
//!          * health(u)
//! Force strength sums per-unit contributions and applies the General
//! bonus, exactly like the resolver does.
//!
//! `sqrt(def_eff)` is the load-bearing trick: under Lanchester square-law
//! exchange, a unit's contribution to expected outcome scales as
//! √(fp · durability). Linearly summed `sqrt`-strengths therefore
//! approximate the side's combined chance of winning, which makes ratios
//! between sides directly comparable. See game.lua for the linear
//! alternative we considered (more intuitive in absolute numbers but
//! inflates the apparent gap between Era-3 elite and Era-1 garrisons).

use crate::data::GameConfig;
use crate::military::combat::fort_defense_bonus;
use crate::military::units::{ArmyUnit, ArmyUnitType, UnitCategory};
use crate::types::TerrainType;

/// Which side of the engagement the unit is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleRole {
    Attacker,
    Defender,
    /// Used for nation-wide roll-ups where neither side's terrain/fort is
    /// known (e.g. macro nation strength). Treated like a defender on plain
    /// terrain — durability counts, no bonuses.
    Neutral,
}

/// Battlefield context the per-unit strength formula needs.
#[derive(Debug, Clone, Copy)]
pub struct StrengthCtx {
    /// Carried purely for ABI compat with callers that still pass a
    /// terrain value; the estimator no longer reads it (card #478 zeroed
    /// terrain bonuses).
    pub terrain: Option<TerrainType>,
    pub fort_level: u8,
    /// Whether the attacker has siege artillery (applies the same fort
    /// reduction the resolver uses).
    pub attacker_has_siege: bool,
    /// Max range across opponent's living units. Drives the range-advantage
    /// term so a long-ranged stack can credit its first-strike volley.
    pub opponent_max_range: u32,
    /// Closest hex distance between sides at the start of the engagement.
    /// 1 = melee; ≥2 = bombardment (only relevant when the stack actually
    /// has range≥2 units). For the AI's "should I attack" question this is
    /// almost always 1.
    pub distance: u32,
    /// Current game turn — used to gate Garrison entrenchment, mirroring
    /// the resolver. `0` means "no turn context" (estimator falls back to
    /// "all garrisons entrenched" so legacy callers stay stable).
    pub current_turn: u32,
}

impl StrengthCtx {
    /// Plain-terrain context with no fort and no range information. Used
    /// for nation-level roll-ups where details aren't known.
    pub fn neutral() -> Self {
        Self {
            terrain: None,
            fort_level: 0,
            attacker_has_siege: false,
            opponent_max_range: 0,
            distance: 1,
            current_turn: 0,
        }
    }
}

/// Per-unit firepower the estimator assumes will be used at this distance.
/// Mirrors the resolver's actual fp_phase decisions: cavalry charges with
/// FPM at melee, everyone else uses FPN. Bombardment shooters need range
/// ≥ distance to contribute.
fn fp_phase(unit: &ArmyUnit, role: BattleRole, distance: u32, cfg: &GameConfig) -> f64 {
    let stats = unit.unit_type.stats();
    let base = unit.effective_firepower();
    if distance >= 2 {
        // Bombardment: only units that can actually reach contribute.
        if stats.range >= distance {
            base
        } else {
            0.0
        }
    } else {
        // Melee at distance 1.
        let mut fp = if stats.category == UnitCategory::Cavalry
            && stats.range == 1
            && stats.firepower_mounted > 0
        {
            // FPM-weighted firepower (mirror `effective_firepower_charging`).
            let medal_modifier = 1.0 + unit.medals as f64 * 0.25;
            let health_scale = unit.health as f64 / 100.0;
            stats.firepower_mounted as f64 * medal_modifier * health_scale
        } else {
            base
        };
        // Charge bonus only on the first round; here we only see it as the
        // "we're about to attack" estimator, so the attacker side gets it.
        if role == BattleRole::Attacker && stats.category == UnitCategory::Cavalry {
            fp *= 1.0 + cfg.combat_cavalry_charge_bonus;
        }
        fp
    }
}

/// Side-level durability multiplier (card #478). The per-unit `defense`
/// stat and terrain bonuses were dropped; fort is the only multiplier left
/// and only for defenders. Attackers and neutral roll-ups have no
/// durability multiplier.
fn defender_durability(ctx: &StrengthCtx, cfg: &GameConfig) -> f64 {
    let global_fort = if ctx.attacker_has_siege && ctx.fort_level > 0 {
        fort_defense_bonus(ctx.fort_level, cfg) * 0.5
    } else {
        fort_defense_bonus(ctx.fort_level, cfg)
    };
    1.0 + global_fort
}

fn durability_for(role: BattleRole, ctx: &StrengthCtx, cfg: &GameConfig) -> f64 {
    match role {
        BattleRole::Defender => defender_durability(ctx, cfg),
        BattleRole::Attacker | BattleRole::Neutral => 1.0,
    }
}

/// Multiplier for "I out-range you" — mirrors the first-strike volley's
/// effect on expected outcome but in a continuous form so the AI doesn't
/// see a knife-edge between range==X and range==X+1.
fn range_factor(unit: &ArmyUnit, role: BattleRole, ctx: &StrengthCtx, cfg: &GameConfig) -> f64 {
    if role == BattleRole::Defender || role == BattleRole::Neutral {
        return 1.0;
    }
    let r = unit.unit_type.stats().range as i64;
    let opp = ctx.opponent_max_range as i64;
    let advantage = (r - opp).max(0) as f64;
    let raw = 1.0 + cfg.combat_ai_strength_range_advantage_coeff * advantage;
    raw.min(1.0 + cfg.combat_ai_strength_range_advantage_cap)
}

/// Per-unit effective strength. See module docs for the formula. Card
/// #478: per-unit durability is now uniform (no defense stat), so the
/// Lanchester lift comes purely from the side-level fort multiplier on
/// defenders. Attackers' strength is just `fp × range_factor × health`.
pub fn unit_effective_strength(
    unit: &ArmyUnit,
    role: BattleRole,
    ctx: &StrengthCtx,
    cfg: &GameConfig,
) -> f64 {
    if !unit.is_alive() {
        return 0.0;
    }
    let fp = fp_phase(unit, role, ctx.distance, cfg);
    if fp <= 0.0 {
        return 0.0;
    }
    let dur = durability_for(role, ctx, cfg);
    let durability_lift = if cfg.combat_ai_strength_lanchester {
        dur.max(0.0).sqrt()
    } else {
        dur
    };
    fp * durability_lift * range_factor(unit, role, ctx, cfg)
}

/// Aggregate force strength: sum per-unit + General bonus + Garrison
/// entrenchment (defender role only, mirroring the resolver's per-unit
/// `garrison_entrenchment_fp` raw add gated on `arrived_turn`).
pub fn force_strength(
    units: &[ArmyUnit],
    role: BattleRole,
    ctx: &StrengthCtx,
    cfg: &GameConfig,
) -> f64 {
    if units.is_empty() {
        return 0.0;
    }
    let base: f64 = units
        .iter()
        .map(|u| unit_effective_strength(u, role, ctx, cfg))
        .sum();
    let gen_bonus = units
        .iter()
        .filter(|u| u.unit_type == ArmyUnitType::General)
        .max_by_key(|u| u.medals)
        .map(|g| 1.10 + g.medals as f64 * 0.05)
        .unwrap_or(1.0);
    let mut total = base * gen_bonus;
    if role == BattleRole::Defender {
        let entrenched_count = units
            .iter()
            .filter(|u| {
                u.is_alive()
                    && u.unit_type.category() == UnitCategory::Garrison
                    && (ctx.current_turn == 0 || u.arrived_turn < ctx.current_turn)
            })
            .count();
        // Match the resolver: +`garrison_entrenchment_fp` raw FP per
        // entrenched garrison defender. Run through the same √fort lift
        // so it sits on the same scale as the rest of the defender force.
        let dur = defender_durability(ctx, cfg);
        let durability_lift = if cfg.combat_ai_strength_lanchester {
            dur.max(0.0).sqrt()
        } else {
            dur
        };
        total += entrenched_count as f64 * cfg.garrison_entrenchment_fp * durability_lift;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::UnitId;
    use crate::types::{NationId, ProvinceId};

    fn unit(id: u32, t: ArmyUnitType) -> ArmyUnit {
        ArmyUnit::new(UnitId(id), t, NationId(1), ProvinceId(1))
    }

    #[test]
    fn fort_durability_lift_applies_only_to_defenders() {
        let cfg = GameConfig::default();
        let regulars = unit(1, ArmyUnitType::Regulars);

        let no_fort = StrengthCtx::neutral();
        let mut l3_fort = StrengthCtx::neutral();
        l3_fort.fort_level = 3;

        let def_no_fort = unit_effective_strength(&regulars, BattleRole::Defender, &no_fort, &cfg);
        let def_l3 = unit_effective_strength(&regulars, BattleRole::Defender, &l3_fort, &cfg);
        let atk_no_fort = unit_effective_strength(&regulars, BattleRole::Attacker, &no_fort, &cfg);
        let atk_l3 = unit_effective_strength(&regulars, BattleRole::Attacker, &l3_fort, &cfg);

        // Defender at L3 fort gets √1.75 ≈ 1.32 lift.
        assert!(
            (def_l3 / def_no_fort - (1.75_f64).sqrt()).abs() < 1e-6,
            "L3 fort defender lift = {}, expected {}",
            def_l3 / def_no_fort,
            (1.75_f64).sqrt()
        );
        // Attackers don't see fort durability — the value is unchanged.
        assert!((atk_l3 - atk_no_fort).abs() < 1e-9);
    }

    #[test]
    fn range_advantage_caps() {
        let cfg = GameConfig::default();
        let mut ctx = StrengthCtx::neutral();
        ctx.opponent_max_range = 1;
        // RailroadGuns range = 17. Cap = +50%.
        let rrg = unit(1, ArmyUnitType::RailroadGuns);
        let f = range_factor(&rrg, BattleRole::Attacker, &ctx, &cfg);
        assert!(
            (f - (1.0 + cfg.combat_ai_strength_range_advantage_cap)).abs() < 1e-9,
            "expected capped range_factor, got {}",
            f
        );
    }

    #[test]
    fn cavalry_charge_bonus_applies_only_to_attacker() {
        let cfg = GameConfig::default();
        let ctx = StrengthCtx::neutral();
        let cav = unit(1, ArmyUnitType::Hussars);
        let s_atk = unit_effective_strength(&cav, BattleRole::Attacker, &ctx, &cfg);
        let s_def = unit_effective_strength(&cav, BattleRole::Defender, &ctx, &cfg);
        // Attacker should be strictly higher (charge bonus + FPM swap).
        assert!(s_atk > s_def, "atk={} def={}", s_atk, s_def);
    }

    #[test]
    fn empty_force_is_zero_strength() {
        let cfg = GameConfig::default();
        let ctx = StrengthCtx::neutral();
        assert_eq!(
            force_strength(&[], BattleRole::Attacker, &ctx, &cfg),
            0.0
        );
    }
}
