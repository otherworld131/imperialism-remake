use crate::DomainError;
use crate::map::UnitId;
use crate::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitCategory {
    Infantry,
    Cavalry,
    Artillery,
    Special,
    Garrison,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Era {
    One = 1,
    Two = 2,
    Three = 3,
}

/// All army unit types from the original Imperialism (1997).
///
/// Three eras of progression per role; Garrison/Infantry/Cavalry/Artillery/
/// Engineer roles each have an Era I → II → III chain. `General` is special
/// (earned, not built).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArmyUnitType {
    // ── Garrison (immovable, defend only) ─────
    Minutemen, // Era I
    Militia,   // Era II
    Conscript, // Era III

    // ── Skirmisher infantry (light scouts) ────
    Skirmishers,   // Era I
    Sharpshooters, // Era II
    Rangers,       // Era III

    // ── Line infantry (general purpose) ───────
    Regulars,      // Era I
    RifleInfantry, // Era II
    Infantry,      // Era III

    // ── Elite/heavy infantry ──────────────────
    Grenadiers,     // Era I
    Guards,         // Era II
    MachineGunners, // Era III

    // ── Light cavalry (fast, scouting) ────────
    Hussars,    // Era I
    Carbineers, // Era II
    Mechanised, // Era III

    // ── Heavy cavalry ─────────────────────────
    Cuirassiers, // Era I  (no Era II in original)
    Armour,      // Era III

    // ── Light artillery ───────────────────────
    LightArtillery,  // Era I
    FieldArtillery,  // Era II
    MobileArtillery, // Era III

    // ── Heavy artillery ───────────────────────
    Artillery,      // Era I
    SiegeArtillery, // Era II
    RailroadGuns,   // Era III

    // ── Engineer ──────────────────────────────
    Sapper,         // Era I
    CombatEngineer, // Era II
    Saboteur,       // Era III

    // ── Special ───────────────────────────────
    General,

    // ── Project-specific (not in original manual) ─
    /// Defensive artillery auto-spawned at minor-nation capitals. Immovable,
    /// no upkeep, never built by the player. Kept as a project extension to
    /// preserve the existing minor-nation defense balance.
    GarrisonArtillery,
}

impl std::str::FromStr for ArmyUnitType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Minutemen" => Ok(Self::Minutemen),
            "Militia" => Ok(Self::Militia),
            "Conscript" => Ok(Self::Conscript),
            "Skirmishers" => Ok(Self::Skirmishers),
            "Sharpshooters" => Ok(Self::Sharpshooters),
            "Rangers" => Ok(Self::Rangers),
            "Regulars" => Ok(Self::Regulars),
            "RifleInfantry" => Ok(Self::RifleInfantry),
            "Infantry" => Ok(Self::Infantry),
            "Grenadiers" => Ok(Self::Grenadiers),
            "Guards" => Ok(Self::Guards),
            "MachineGunners" => Ok(Self::MachineGunners),
            "Hussars" => Ok(Self::Hussars),
            "Carbineers" => Ok(Self::Carbineers),
            "Mechanised" => Ok(Self::Mechanised),
            "Cuirassiers" => Ok(Self::Cuirassiers),
            "Armour" => Ok(Self::Armour),
            "LightArtillery" => Ok(Self::LightArtillery),
            "FieldArtillery" => Ok(Self::FieldArtillery),
            "MobileArtillery" => Ok(Self::MobileArtillery),
            "Artillery" => Ok(Self::Artillery),
            "SiegeArtillery" => Ok(Self::SiegeArtillery),
            "RailroadGuns" => Ok(Self::RailroadGuns),
            "Sapper" => Ok(Self::Sapper),
            "CombatEngineer" => Ok(Self::CombatEngineer),
            "Saboteur" => Ok(Self::Saboteur),
            "General" => Ok(Self::General),
            "GarrisonArtillery" => Ok(Self::GarrisonArtillery),
            _ => Err(format!("unknown ArmyUnitType: {}", s)),
        }
    }
}

/// Stats for a unit type. Fields mirror the original Imperialism manual:
///
/// - `firepower` (FPN) — base attack vs infantry/garrison.
/// - `firepower_mounted` (FPM) — bonus attack vs cavalry / when charging.
///   Currently unused by combat resolution (file: Trello "wire up FPM").
/// - `defense` (DEF) — base defensive value.
///   Currently unused by combat resolution (file: Trello "wire up DEF & terrain bonus").
/// - `defense_terrain_bonus` — extra DEF when defending in favorable terrain
///   (forest/hills/fort). Original game bracketed this as e.g. `5(6)`; the
///   bonus is the bracketed delta.
///   Currently unused by combat resolution.
/// - `range` — attack range in hexes.
/// - `movement` — base movement points per turn.
/// - `arms_required` — Arms unit consumed at recruitment (and per upgrade diff).
/// - `cost` — dollar cost to recruit. Resource costs are resolved separately.
/// - `maintenance_per_turn` — upkeep paid each turn (Garrison units pay 0).
/// - `prerequisite_tech` — tech name required to unlock this unit, or `None`.
/// - `era` — historical era bucket (1/2/3) for UI grouping and obsoletion.
#[derive(Debug, Clone)]
pub struct UnitStats {
    pub firepower: u32,
    pub firepower_mounted: u32,
    pub defense: u32,
    pub defense_terrain_bonus: u32,
    pub movement: u32,
    pub range: u32,
    pub cost: Money,
    pub arms_required: u32,
    pub requires_horse: bool,
    pub category: UnitCategory,
    pub maintenance_per_turn: Money,
    pub prerequisite_tech: Option<String>,
    pub era: Era,
}

#[derive(Debug, Clone)]
pub struct ArmyUnit {
    pub id: UnitId,
    pub unit_type: ArmyUnitType,
    pub owner: NationId,
    pub position: ProvinceId,
    pub health: u8, // 0-100 in 5% increments
    pub medals: u8, // 0-4+
    pub movement_remaining: u32,
}

/// Process-global registry populated by the Lua loader at GameData init
/// (see [`install_unit_stats`]). When set, `ArmyUnitType::stats()` reads
/// from this map; the hardcoded [`ArmyUnitType::stats_baseline`] table
/// is the fallback for tests that bypass GameData and for the brief
/// window during snapshot restore before GameData rehydrates.
///
/// Resolves F-002 from the round-1 review: making Lua actually authoritative
/// at runtime instead of leaving `data.unit_stats` shelf-bound.
static UNIT_STATS_REGISTRY: std::sync::OnceLock<
    std::sync::RwLock<std::collections::HashMap<ArmyUnitType, UnitStats>>,
> = std::sync::OnceLock::new();

/// Install (or replace) the process-wide unit stats registry. Called by
/// GameData when Lua data loads successfully. Idempotent — subsequent
/// calls swap the contents under a write lock.
pub fn install_unit_stats(map: std::collections::HashMap<ArmyUnitType, UnitStats>) {
    let cell = UNIT_STATS_REGISTRY
        .get_or_init(|| std::sync::RwLock::new(std::collections::HashMap::new()));
    if let Ok(mut guard) = cell.write() {
        *guard = map;
    }
}

impl ArmyUnitType {
    /// Returns the base stats for this unit type.
    ///
    /// Combat numbers (FPN/FPM/DEF/terrain bonus) come from the original
    /// Imperialism (1997) manual unit table. Dollar `cost`/`maintenance` are
    /// project-specific.
    ///
    /// The values are sourced from `scripts/config/units.lua` at runtime
    /// when GameData has installed the registry; the hardcoded
    /// [`Self::stats_baseline`] table is the fallback for tests and the
    /// snapshot-restore window. Both shapes are kept in sync by the
    /// `lua_baseline_unit_stats_match` test in `data::tests`.
    pub fn stats(&self) -> UnitStats {
        if let Some(cell) = UNIT_STATS_REGISTRY.get() {
            if let Ok(guard) = cell.read() {
                if let Some(s) = guard.get(self) {
                    return s.clone();
                }
            }
        }
        self.stats_baseline()
    }

    /// Hardcoded baseline stats — the source of truth in the absence of
    /// `scripts/config/units.lua`. Same values as the Lua file; the
    /// `lua_baseline_unit_stats_match` test asserts they agree.
    pub fn stats_baseline(&self) -> UnitStats {
        use ArmyUnitType::*;
        // Helper to keep the table readable.
        let s = |firepower,
                 firepower_mounted,
                 defense,
                 defense_terrain_bonus,
                 range,
                 movement,
                 arms_required,
                 requires_horse,
                 category,
                 cost: i64,
                 maintenance: i64,
                 era,
                 prerequisite_tech: Option<&str>| UnitStats {
            firepower,
            firepower_mounted,
            defense,
            defense_terrain_bonus,
            range,
            movement,
            arms_required,
            requires_horse,
            category,
            cost: Money::dollars(cost),
            maintenance_per_turn: Money::dollars(maintenance),
            era,
            prerequisite_tech: prerequisite_tech.map(String::from),
        };
        match self {
            // ── Garrison ──────────────────────────────────────────
            // FPN FPM DEF DEFT RNG MVR ARMS HORSE  CAT       COST  MTN  ERA
            Minutemen => s(
                5,
                0,
                5,
                0,
                1,
                0,
                0,
                false,
                UnitCategory::Garrison,
                0,
                0,
                Era::One,
                None,
            ),
            Militia => s(
                10,
                0,
                7,
                0,
                2,
                0,
                1,
                false,
                UnitCategory::Garrison,
                0,
                0,
                Era::Two,
                None,
            ),
            Conscript => s(
                17,
                0,
                5,
                0,
                2,
                4,
                1,
                false,
                UnitCategory::Garrison,
                100,
                25,
                Era::Three,
                Some("Modern Warfare"),
            ),

            // ── Skirmisher infantry ─────────────────────────────
            Skirmishers => s(
                5,
                0,
                5,
                0,
                1,
                4,
                1,
                false,
                UnitCategory::Infantry,
                100,
                25,
                Era::One,
                None,
            ),
            Sharpshooters => s(
                11,
                0,
                5,
                0,
                3,
                4,
                1,
                false,
                UnitCategory::Infantry,
                200,
                50,
                Era::Two,
                Some("Sharpshooter Training"),
            ),
            Rangers => s(
                15,
                0,
                10,
                0,
                5,
                4,
                1,
                false,
                UnitCategory::Infantry,
                250,
                50,
                Era::Three,
                Some("Ranger Training"),
            ),

            // ── Line infantry ───────────────────────────────────
            Regulars => s(
                10,
                5,
                5,
                0,
                1,
                4,
                1,
                false,
                UnitCategory::Infantry,
                100,
                25,
                Era::One,
                None,
            ),
            RifleInfantry => s(
                15,
                0,
                7,
                1,
                2,
                4,
                1,
                false,
                UnitCategory::Infantry,
                200,
                50,
                Era::Two,
                Some("Rifling"),
            ),
            Infantry => s(
                22,
                0,
                10,
                0,
                2,
                4,
                2,
                false,
                UnitCategory::Infantry,
                300,
                75,
                Era::Three,
                Some("Modern Warfare"),
            ),

            // ── Elite/heavy infantry ────────────────────────────
            Grenadiers => s(
                12,
                0,
                5,
                1,
                1,
                4,
                1,
                false,
                UnitCategory::Infantry,
                150,
                50,
                Era::One,
                Some("Grenadier Tactics"),
            ),
            Guards => s(
                17,
                0,
                9,
                0,
                2,
                4,
                1,
                false,
                UnitCategory::Infantry,
                250,
                75,
                Era::Two,
                Some("Professional Army"),
            ),
            MachineGunners => s(
                28,
                0,
                12,
                0,
                2,
                4,
                2,
                false,
                UnitCategory::Infantry,
                400,
                100,
                Era::Three,
                Some("Machine Guns"),
            ),

            // ── Light cavalry ───────────────────────────────────
            Hussars => s(
                7,
                10,
                4,
                0,
                1,
                7,
                1,
                true,
                UnitCategory::Cavalry,
                150,
                25,
                Era::One,
                None,
            ),
            Carbineers => s(
                11,
                13,
                7,
                0,
                2,
                7,
                1,
                true,
                UnitCategory::Cavalry,
                250,
                50,
                Era::Two,
                Some("Carbines"),
            ),
            Mechanised => s(
                30,
                0,
                10,
                2,
                4,
                6,
                2,
                false,
                UnitCategory::Cavalry,
                400,
                75,
                Era::Three,
                Some("Mechanisation"),
            ),

            // ── Heavy cavalry ───────────────────────────────────
            Cuirassiers => s(
                15,
                0,
                9,
                0,
                1,
                7,
                1,
                true,
                UnitCategory::Cavalry,
                200,
                50,
                Era::One,
                None,
            ),
            Armour => s(
                30,
                0,
                16,
                0,
                6,
                6,
                4,
                false,
                UnitCategory::Cavalry,
                500,
                100,
                Era::Three,
                Some("Armoured Vehicles"),
            ),

            // ── Light artillery ─────────────────────────────────
            LightArtillery => s(
                10,
                0,
                9,
                0,
                3,
                3,
                2,
                false,
                UnitCategory::Artillery,
                200,
                50,
                Era::One,
                None,
            ),
            FieldArtillery => s(
                17,
                0,
                12,
                1,
                5,
                3,
                2,
                false,
                UnitCategory::Artillery,
                350,
                75,
                Era::Two,
                Some("Field Artillery"),
            ),
            MobileArtillery => s(
                22,
                0,
                12,
                1,
                5,
                4,
                2,
                false,
                UnitCategory::Artillery,
                450,
                100,
                Era::Three,
                Some("Mobile Artillery"),
            ),

            // ── Heavy artillery ─────────────────────────────────
            Artillery => s(
                16,
                0,
                11,
                1,
                4,
                2,
                2,
                false,
                UnitCategory::Artillery,
                300,
                75,
                Era::One,
                Some("Improved Artillery"),
            ),
            SiegeArtillery => s(
                21,
                0,
                9,
                11,
                6,
                2,
                3,
                false,
                UnitCategory::Artillery,
                500,
                100,
                Era::Two,
                Some("Siege Warfare"),
            ),
            RailroadGuns => s(
                50,
                0,
                20,
                5,
                17,
                0,
                4,
                false,
                UnitCategory::Artillery,
                600,
                125,
                Era::Three,
                Some("Railroad Artillery"),
            ),

            // ── Engineer ────────────────────────────────────────
            Sapper => s(
                5,
                0,
                4,
                0,
                1,
                4,
                1,
                false,
                UnitCategory::Special,
                150,
                25,
                Era::One,
                Some("Engineering"),
            ),
            CombatEngineer => s(
                7,
                0,
                7,
                0,
                2,
                4,
                1,
                false,
                UnitCategory::Special,
                200,
                50,
                Era::Two,
                Some("Engineering"),
            ),
            Saboteur => s(
                9,
                0,
                10,
                2,
                1,
                4,
                1,
                false,
                UnitCategory::Special,
                250,
                50,
                Era::Three,
                Some("Modern Warfare"),
            ),

            // ── Special ─────────────────────────────────────────
            General => s(
                0,
                0,
                0,
                0,
                0,
                8,
                0,
                false,
                UnitCategory::Special,
                0,
                0,
                Era::One,
                None,
            ),

            // ── Project-specific: minor-nation capital defense ──
            GarrisonArtillery => s(
                4,
                0,
                0,
                0,
                3,
                0,
                0,
                false,
                UnitCategory::Garrison,
                0,
                0,
                Era::One,
                None,
            ),
        }
    }

    pub fn category(&self) -> UnitCategory {
        self.stats().category
    }

    /// Garrison units cannot move; everything else can.
    pub fn can_move(&self) -> bool {
        !matches!(
            self,
            ArmyUnitType::Minutemen | ArmyUnitType::Militia | ArmyUnitType::GarrisonArtillery
        )
        // Note: Conscript is Garrison-category but has movement (per manual).
    }

    /// `General` is earned via combat reward, not built.
    /// `Minutemen` and `Militia` (Era I/II garrison) spawn automatically when
    /// a province is captured — they aren't placed by the recruit menu either.
    /// `GarrisonArtillery` is a minor-nation capital defense extension.
    pub fn can_build(&self) -> bool {
        !matches!(
            self,
            ArmyUnitType::General
                | ArmyUnitType::Minutemen
                | ArmyUnitType::Militia
                | ArmyUnitType::GarrisonArtillery
        )
    }

    /// Returns the tech tree name required to build/unlock this unit type,
    /// if any. Era I units are available from game start. Names match entries
    /// in the tech tree (`TechTree::get_by_name`).
    pub fn required_tech(&self) -> Option<&str> {
        use ArmyUnitType::*;
        match self {
            // Era I — base units, no tech needed
            Minutemen | Militia | Skirmishers | Regulars | Grenadiers | Hussars | Cuirassiers
            | LightArtillery | Artillery | Sapper | General | GarrisonArtillery => None,
            // Era II
            Sharpshooters => Some("Bessemer Converter"),
            RifleInfantry => Some("Breech-Loading Rifles"),
            Guards => Some("Breech-Loading Rifles"),
            Carbineers => Some("Breech-Loading Rifles"),
            FieldArtillery => Some("Rifled Artillery"),
            SiegeArtillery => Some("Large Artillery"),
            CombatEngineer => Some("Bessemer Converter"),
            // Era III
            Conscript => Some("Modern Warfare"),
            Rangers => Some("Machine Guns"),
            Infantry => Some("Modern Warfare"),
            MachineGunners => Some("Machine Guns"),
            Mechanised => Some("Internal Combustion"),
            Armour => Some("Internal Combustion"),
            MobileArtillery => Some("Internal Combustion"),
            RailroadGuns => Some("Large Artillery"),
            Saboteur => Some("Modern Warfare"),
        }
    }

    /// Returns the next-era unit in the same role chain, if any.
    ///
    /// Chains (per original-game manual roster):
    /// - Garrison: Minutemen → Militia → Conscript
    /// - Skirmisher: Skirmishers → Sharpshooters → Rangers
    /// - Line infantry: Regulars → RifleInfantry → Infantry
    /// - Elite infantry: Grenadiers → Guards → MachineGunners
    /// - Light cavalry: Hussars → Carbineers → Mechanised
    /// - Heavy cavalry: Cuirassiers → Armour (no Era II)
    /// - Light artillery: LightArtillery → FieldArtillery → MobileArtillery
    /// - Heavy artillery: Artillery → SiegeArtillery → RailroadGuns
    /// - Engineer: Sapper → CombatEngineer → Saboteur
    pub fn upgrade_to(&self) -> Option<ArmyUnitType> {
        use ArmyUnitType::*;
        match self {
            // Garrison
            Minutemen => Some(Militia),
            Militia => Some(Conscript),
            // Skirmisher
            Skirmishers => Some(Sharpshooters),
            Sharpshooters => Some(Rangers),
            // Line infantry
            Regulars => Some(RifleInfantry),
            RifleInfantry => Some(Infantry),
            // Elite infantry
            Grenadiers => Some(Guards),
            Guards => Some(MachineGunners),
            // Light cavalry
            Hussars => Some(Carbineers),
            Carbineers => Some(Mechanised),
            // Heavy cavalry (no Era II — jumps straight to Armour)
            Cuirassiers => Some(Armour),
            // Light artillery
            LightArtillery => Some(FieldArtillery),
            FieldArtillery => Some(MobileArtillery),
            // Heavy artillery
            Artillery => Some(SiegeArtillery),
            SiegeArtillery => Some(RailroadGuns),
            // Engineer
            Sapper => Some(CombatEngineer),
            CombatEngineer => Some(Saboteur),
            // End-of-line
            Conscript | Rangers | Infantry | MachineGunners | Mechanised | Armour
            | MobileArtillery | RailroadGuns | Saboteur | General | GarrisonArtillery => None,
        }
    }

    /// `obsoleted_by` is the same chain as `upgrade_to`, but expresses the
    /// recruit-menu filter: once the upgrade target is unlocked, the older
    /// variant disappears from the build menu (existing units stay in play
    /// and can be upgraded).
    pub fn obsoleted_by(&self) -> Option<ArmyUnitType> {
        self.upgrade_to()
    }

    /// True when ANY newer variant in this unit's role chain has been
    /// unlocked for the caller's nation, i.e. the recruit menu should hide
    /// this unit. Existing units of this type stay on the board and can
    /// still be upgraded.
    ///
    /// Walks the entire chain (Era I → II → III) so that if Era III is
    /// unlocked while Era II is not, the Era I variant is still hidden
    /// (resolves F-005 from the round-1 review).
    ///
    /// `has_tech` is a closure that answers "has the nation researched the
    /// tech with this name?". Caller-supplied so this method stays decoupled
    /// from `Nation` / `GameData`.
    pub fn is_recruit_obsoleted<F: Fn(&str) -> bool>(&self, has_tech: F) -> bool {
        let mut cur = *self;
        while let Some(next) = cur.upgrade_to() {
            let unlocked = match next.required_tech() {
                Some(tech) => has_tech(tech),
                None => true,
            };
            if unlocked {
                return true;
            }
            cur = next;
        }
        false
    }

    /// Walk this unit's role chain to the latest variant the nation has
    /// unlocked. Used by the AI so its picks don't stick to Era I when
    /// later-era variants are available.
    ///
    /// `has_tech` mirrors [`Self::is_recruit_obsoleted`].
    pub fn latest_unlocked_in_chain<F: Fn(&str) -> bool>(&self, has_tech: F) -> ArmyUnitType {
        let mut cur = *self;
        while let Some(next) = cur.upgrade_to() {
            let unlocked = match next.required_tech() {
                Some(tech) => has_tech(tech),
                None => true,
            };
            if !unlocked {
                break;
            }
            cur = next;
        }
        cur
    }

    /// Era bucket (I/II/III) per the original-game grouping.
    pub fn era(&self) -> Era {
        self.stats().era
    }
}

impl ArmyUnit {
    pub fn new(id: UnitId, unit_type: ArmyUnitType, owner: NationId, position: ProvinceId) -> Self {
        let stats = unit_type.stats();
        Self {
            id,
            unit_type,
            owner,
            position,
            health: 100,
            medals: 0,
            movement_remaining: stats.movement,
        }
    }

    /// Calculate effective firepower with medal modifier, scaled by health.
    /// Medal modifier: (1.0 + medals * 0.25), so 4 medals = 2.0x.
    /// Health scaling: firepower degrades linearly with damage.
    pub fn effective_firepower(&self) -> f64 {
        let base_fp = self.unit_type.stats().firepower as f64;
        let medal_modifier = 1.0 + self.medals as f64 * 0.25;
        let health_scale = self.health as f64 / 100.0;
        base_fp * medal_modifier * health_scale
    }

    pub fn take_damage(&mut self, amount: u8) {
        let effective = if amount == 0 {
            0
        } else {
            ((amount as u16 + 2) / 5 * 5).max(5) as u8
        };
        self.health = self.health.saturating_sub(effective);
    }

    pub fn heal(&mut self, amount: u8) {
        let multiplier = 1 + self.medals / 2;
        let effective = (amount as u16) * (multiplier as u16);
        let new_health = (self.health as u16) + effective;
        self.health = if new_health > 100 {
            100
        } else {
            new_health as u8
        };
    }

    pub fn award_medal(&mut self) {
        if self.medals < 4 {
            self.medals += 1;
        }
    }

    pub fn is_alive(&self) -> bool {
        self.health > 0
    }

    /// Per-turn maintenance. Garrison units pay no upkeep (tied to
    /// provinces, can't be disbanded). All others pay
    /// `cents_per_arm * arms_required`.
    pub fn maintenance_cost(&self, cents_per_arm: i64) -> Money {
        if self.unit_type.category() == UnitCategory::Garrison {
            return Money::ZERO;
        }
        let arms = self.unit_type.stats().arms_required;
        Money::from_cents(cents_per_arm * arms as i64)
    }
}

/// Cost to upgrade a unit from `from` to `to`, per the original-game manual:
/// the production-cost difference (clamped to zero — upgrading "downhill"
/// is free rather than a refund).
///
/// Resource arithmetic (additional Arms required for the bigger unit) is
/// handled separately in [`upgrade_player_unit`] — this function only
/// returns the dollar delta.
pub fn upgrade_cost(from: ArmyUnitType, to: ArmyUnitType) -> Money {
    let from_cost = from.stats().cost;
    let to_cost = to.stats().cost;
    if to_cost > from_cost {
        Money::from_cents(to_cost.cents() - from_cost.cents())
    } else {
        Money::ZERO
    }
}

/// Upgrade a single player-owned army unit in place, preserving health and
/// medals.
///
/// Validation order:
/// 1. Unit exists in the nation's army.
/// 2. Unit type has an `upgrade_to()` target.
/// 3. The target's `required_tech` is researched (when any).
/// 4. Treasury covers `upgrade_cost(from, to)`.
/// 5. Arms stockpile covers any positive delta in `arms_required`.
///
/// On success, deducts the cost + extra arms and mutates the unit type.
/// `movement_remaining` is reset to the target type's stat so the upgraded
/// unit can still act this turn at its new movement rate.
///
/// Caller (the wasm bridge or the CLI) is responsible for sending events
/// or UI feedback.
pub fn upgrade_player_unit(
    game: &mut crate::game_state::GameState,
    nation_id: NationId,
    unit_id: UnitId,
) -> Result<(ArmyUnitType, ArmyUnitType, Money), DomainError> {
    use crate::types::MaterialType;

    let nation = game
        .get_nation(nation_id)
        .ok_or(DomainError::NationNotFound(nation_id))?;
    let pos = nation
        .military
        .army
        .iter()
        .position(|u| u.id == unit_id)
        .ok_or_else(|| DomainError::illegal("unit not found"))?;
    let from_type = nation.military.army[pos].unit_type;
    let to_type = from_type
        .upgrade_to()
        .ok_or_else(|| DomainError::illegal("no upgrade path for this unit"))?;

    // Tech gate.
    if let Some(req_tech) = to_type.required_tech() {
        let has_tech = nation.researched_techs.iter().any(|tid| {
            game.game_data
                .tech_tree
                .get(*tid)
                .map(|t| t.name == req_tech)
                .unwrap_or(false)
        });
        if !has_tech {
            return Err(DomainError::illegal("upgrade target tech not researched"));
        }
    }

    // Cost gates.
    let cost = upgrade_cost(from_type, to_type);
    if nation.economy.treasury < cost {
        return Err(DomainError::illegal("insufficient treasury for upgrade"));
    }
    let arms_delta = to_type
        .stats()
        .arms_required
        .saturating_sub(from_type.stats().arms_required);
    if arms_delta > 0 && nation.material_amount(MaterialType::Arms) < arms_delta {
        return Err(DomainError::illegal("insufficient arms for upgrade"));
    }

    let nation = game
        .get_nation_mut(nation_id)
        .ok_or(DomainError::NationNotFound(nation_id))?;
    nation.economy.treasury -= cost;
    if arms_delta > 0 {
        nation.consume_material(MaterialType::Arms, arms_delta);
    }
    let unit = &mut nation.military.army[pos];
    unit.unit_type = to_type;
    unit.movement_remaining = to_type.stats().movement;
    Ok((from_type, to_type, cost))
}

/// Disband (dismiss) a player's army unit.
///
/// Errors if the unit isn't found, or if it is a Garrison unit (Minutemen /
/// Militia / Conscript — tied to province defense and not dismissable).
pub fn disband_unit(
    game: &mut crate::game_state::GameState,
    nation_id: NationId,
    unit_id: UnitId,
) -> Result<(), DomainError> {
    let nation = game
        .get_nation_mut(nation_id)
        .ok_or(DomainError::NationNotFound(nation_id))?;
    let pos = nation
        .military
        .army
        .iter()
        .position(|u| u.id == unit_id)
        .ok_or_else(|| DomainError::illegal("unit not found"))?;
    if nation.military.army[pos].unit_type.category() == UnitCategory::Garrison {
        return Err(DomainError::illegal("garrison units cannot be dismissed"));
    }
    nation.military.army.remove(pos);
    game.transient
        .pending_moves
        .retain(|(nid, id, _)| *nid != nation_id || *id != unit_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Roster invariants ───────────────────────────────────────

    /// All 26 unit types from the original Imperialism (1997) manual table
    /// (heavy cavalry skips Era II, hence 26 not 27), plus `General` (earned,
    /// not built) and `GarrisonArtillery` (project extension for minor-nation
    /// capital defense). Used by the data tests below.
    const ALL_TYPES: [ArmyUnitType; 28] = [
        ArmyUnitType::Minutemen,
        ArmyUnitType::Militia,
        ArmyUnitType::Conscript,
        ArmyUnitType::Skirmishers,
        ArmyUnitType::Sharpshooters,
        ArmyUnitType::Rangers,
        ArmyUnitType::Regulars,
        ArmyUnitType::RifleInfantry,
        ArmyUnitType::Infantry,
        ArmyUnitType::Grenadiers,
        ArmyUnitType::Guards,
        ArmyUnitType::MachineGunners,
        ArmyUnitType::Hussars,
        ArmyUnitType::Carbineers,
        ArmyUnitType::Mechanised,
        ArmyUnitType::Cuirassiers,
        ArmyUnitType::Armour,
        ArmyUnitType::LightArtillery,
        ArmyUnitType::FieldArtillery,
        ArmyUnitType::MobileArtillery,
        ArmyUnitType::Artillery,
        ArmyUnitType::SiegeArtillery,
        ArmyUnitType::RailroadGuns,
        ArmyUnitType::Sapper,
        ArmyUnitType::CombatEngineer,
        ArmyUnitType::Saboteur,
        ArmyUnitType::General,
        ArmyUnitType::GarrisonArtillery,
    ];

    #[test]
    fn all_types_round_trip_via_from_str() {
        for t in ALL_TYPES {
            let name = format!("{:?}", t);
            let parsed: ArmyUnitType = name.parse().expect("FromStr should round-trip Debug");
            assert_eq!(parsed, t);
        }
    }

    #[test]
    fn upgrade_chains_match_original_eras() {
        use ArmyUnitType::*;
        let chains = [
            (Minutemen, Militia, Conscript),
            (Skirmishers, Sharpshooters, Rangers),
            (Regulars, RifleInfantry, Infantry),
            (Grenadiers, Guards, MachineGunners),
            (Hussars, Carbineers, Mechanised),
            (LightArtillery, FieldArtillery, MobileArtillery),
            (Artillery, SiegeArtillery, RailroadGuns),
            (Sapper, CombatEngineer, Saboteur),
        ];
        for (era1, era2, era3) in chains {
            assert_eq!(era1.upgrade_to(), Some(era2));
            assert_eq!(era2.upgrade_to(), Some(era3));
            assert_eq!(era3.upgrade_to(), None);
        }
    }

    #[test]
    fn cuirassiers_skip_to_armour() {
        // Heavy cavalry has no Era II in the original.
        assert_eq!(
            ArmyUnitType::Cuirassiers.upgrade_to(),
            Some(ArmyUnitType::Armour)
        );
        assert_eq!(ArmyUnitType::Armour.upgrade_to(), None);
    }

    #[test]
    fn obsoleted_by_matches_upgrade_to() {
        for t in ALL_TYPES {
            assert_eq!(t.obsoleted_by(), t.upgrade_to());
        }
    }

    #[test]
    fn era_buckets_match_chain_position() {
        use ArmyUnitType::*;
        for (era, units) in [
            (
                Era::One,
                &[
                    Minutemen,
                    Skirmishers,
                    Regulars,
                    Grenadiers,
                    Hussars,
                    Cuirassiers,
                    LightArtillery,
                    Artillery,
                    Sapper,
                ] as &[_],
            ),
            (
                Era::Two,
                &[
                    Militia,
                    Sharpshooters,
                    RifleInfantry,
                    Guards,
                    Carbineers,
                    FieldArtillery,
                    SiegeArtillery,
                    CombatEngineer,
                ],
            ),
            (
                Era::Three,
                &[
                    Conscript,
                    Rangers,
                    Infantry,
                    MachineGunners,
                    Mechanised,
                    Armour,
                    MobileArtillery,
                    RailroadGuns,
                    Saboteur,
                ],
            ),
        ] {
            for u in units {
                assert_eq!(u.era(), era, "{:?} should be {:?}", u, era);
            }
        }
    }

    // ── New stat fields exist for every type ────────────────────

    #[test]
    fn every_unit_type_has_stats() {
        for t in ALL_TYPES {
            let _ = t.stats(); // panics if match is non-exhaustive
        }
    }

    #[test]
    fn carbineers_have_mounted_firepower() {
        assert_eq!(ArmyUnitType::Carbineers.stats().firepower_mounted, 13);
    }

    #[test]
    fn rifle_infantry_has_terrain_bonus() {
        let s = ArmyUnitType::RifleInfantry.stats();
        assert_eq!(s.defense, 7);
        assert_eq!(s.defense_terrain_bonus, 1);
    }

    #[test]
    fn siege_artillery_has_large_terrain_bonus() {
        // Manual: 9(20) — base 9, +11 in defensive terrain.
        let s = ArmyUnitType::SiegeArtillery.stats();
        assert_eq!(s.defense, 9);
        assert_eq!(s.defense_terrain_bonus, 11);
    }

    // ── Original-game stat spot-checks ──────────────────────────

    #[test]
    fn regulars_match_manual() {
        let s = ArmyUnitType::Regulars.stats();
        assert_eq!(s.firepower, 10);
        assert_eq!(s.firepower_mounted, 5);
        assert_eq!(s.range, 1);
        assert_eq!(s.defense, 5);
        assert_eq!(s.movement, 4);
        assert_eq!(s.arms_required, 1);
    }

    #[test]
    fn machine_gunners_match_manual() {
        let s = ArmyUnitType::MachineGunners.stats();
        assert_eq!(s.firepower, 28);
        assert_eq!(s.defense, 12);
        assert_eq!(s.arms_required, 2);
    }

    #[test]
    fn railroad_guns_match_manual() {
        let s = ArmyUnitType::RailroadGuns.stats();
        assert_eq!(s.firepower, 50);
        assert_eq!(s.range, 17);
        assert_eq!(s.movement, 0); // rail-bound
    }

    // ── can_move / can_build ────────────────────────────────────

    #[test]
    fn era1_and_era2_garrison_cannot_move() {
        assert!(!ArmyUnitType::Minutemen.can_move());
        assert!(!ArmyUnitType::Militia.can_move());
    }

    #[test]
    fn auto_spawned_garrison_cannot_be_built_manually() {
        assert!(!ArmyUnitType::Minutemen.can_build());
        assert!(!ArmyUnitType::Militia.can_build());
        assert!(!ArmyUnitType::General.can_build());
    }

    #[test]
    fn buildable_units_include_all_combat_types() {
        for t in [
            ArmyUnitType::Regulars,
            ArmyUnitType::RifleInfantry,
            ArmyUnitType::Infantry,
            ArmyUnitType::Sapper,
            ArmyUnitType::CombatEngineer,
            ArmyUnitType::Saboteur,
            ArmyUnitType::Conscript,
        ] {
            assert!(t.can_build(), "{:?} should be buildable", t);
        }
    }

    // ── Health / damage / heal ──────────────────────────────────

    #[test]
    fn take_damage_rounds_to_5_percent() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.take_damage(7);
        assert_eq!(unit.health, 95);
        unit.take_damage(13);
        assert_eq!(unit.health, 80);
    }

    #[test]
    fn medal_firepower_scales_correctly() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        // Regulars FPN = 10, 0 medals = 1.0× → 10
        assert!((unit.effective_firepower() - 10.0).abs() < f64::EPSILON);
        unit.award_medal();
        // 10 × 1.25 = 12.5
        assert!((unit.effective_firepower() - 12.5).abs() < f64::EPSILON);
    }

    #[test]
    fn maintenance_cost_zero_for_garrison() {
        let cents_per_arm = 250;
        for t in [
            ArmyUnitType::Minutemen,
            ArmyUnitType::Militia,
            ArmyUnitType::Conscript,
        ] {
            let unit = ArmyUnit::new(UnitId(1), t, NationId(1), ProvinceId(1));
            assert_eq!(unit.maintenance_cost(cents_per_arm), Money::ZERO);
        }
    }

    #[test]
    fn maintenance_cost_scales_with_arms() {
        // Regulars: 1 arm × 250¢ = $2.50
        let unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        assert_eq!(unit.maintenance_cost(250), Money::from_cents(250));
        // Armour: 4 arms × 250¢ = $10
        let unit = ArmyUnit::new(UnitId(1), ArmyUnitType::Armour, NationId(1), ProvinceId(1));
        assert_eq!(unit.maintenance_cost(250), Money::from_cents(1000));
    }

    // ── Obsoletion helpers (Card #420) ──────────────────────────

    // ── Upgrade cost (Card #417) ───────────────────────────────

    #[test]
    fn upgrade_cost_is_production_difference() {
        // Regulars cost $100, RifleInfantry $200 — delta $100.
        assert_eq!(
            upgrade_cost(ArmyUnitType::Regulars, ArmyUnitType::RifleInfantry),
            Money::dollars(100)
        );
        // Heavy cavalry: Cuirassiers $200 -> Armour $500 — delta $300.
        assert_eq!(
            upgrade_cost(ArmyUnitType::Cuirassiers, ArmyUnitType::Armour),
            Money::dollars(300)
        );
    }

    #[test]
    fn upgrade_cost_clamps_to_zero_when_target_is_cheaper() {
        // Same-cost or cheaper upgrade is free, never a refund.
        assert_eq!(
            upgrade_cost(ArmyUnitType::Regulars, ArmyUnitType::Regulars),
            Money::ZERO
        );
    }

    #[test]
    fn era1_units_obsoleted_when_era2_tech_researched() {
        // Regulars (Era I line) is obsoleted once "Breech-Loading Rifles"
        // (the tech for RifleInfantry per `required_tech()`) is researched.
        let has_blr = |t: &str| t == "Breech-Loading Rifles";
        assert!(ArmyUnitType::Regulars.is_recruit_obsoleted(has_blr));
        // RifleInfantry itself is not obsoleted at this point — Infantry's
        // tech ("Modern Warfare") isn't researched.
        assert!(!ArmyUnitType::RifleInfantry.is_recruit_obsoleted(has_blr));
    }

    #[test]
    fn era3_units_never_obsoleted() {
        // End-of-line variants have no upgrade target.
        let always_true = |_: &str| true;
        for t in [
            ArmyUnitType::Conscript,
            ArmyUnitType::Rangers,
            ArmyUnitType::Infantry,
            ArmyUnitType::MachineGunners,
            ArmyUnitType::Mechanised,
            ArmyUnitType::Armour,
            ArmyUnitType::MobileArtillery,
            ArmyUnitType::RailroadGuns,
            ArmyUnitType::Saboteur,
        ] {
            assert!(
                !t.is_recruit_obsoleted(always_true),
                "{:?} should never be obsoleted",
                t
            );
        }
    }

    #[test]
    fn latest_unlocked_in_chain_walks_all_eras() {
        // With Breech-Loading Rifles + Modern Warfare researched, picking
        // Regulars walks to Infantry (Era III line).
        let has_blr_and_modern =
            |t: &str| matches!(t, "Breech-Loading Rifles" | "Modern Warfare");
        let latest = ArmyUnitType::Regulars.latest_unlocked_in_chain(has_blr_and_modern);
        assert_eq!(latest, ArmyUnitType::Infantry);
    }

    #[test]
    fn latest_unlocked_in_chain_stops_at_first_locked_link() {
        // Only Breech-Loading Rifles researched → Regulars upgrades to
        // RifleInfantry but stops there because "Modern Warfare" (Infantry's
        // tech) isn't met.
        let only_blr = |t: &str| t == "Breech-Loading Rifles";
        let latest = ArmyUnitType::Regulars.latest_unlocked_in_chain(only_blr);
        assert_eq!(latest, ArmyUnitType::RifleInfantry);
    }

    #[test]
    fn latest_unlocked_in_chain_is_identity_when_nothing_unlocked() {
        let nothing = |_: &str| false;
        assert_eq!(
            ArmyUnitType::Regulars.latest_unlocked_in_chain(nothing),
            ArmyUnitType::Regulars
        );
    }

    #[test]
    fn era1_units_need_no_tech() {
        for t in [
            ArmyUnitType::Minutemen,
            ArmyUnitType::Skirmishers,
            ArmyUnitType::Regulars,
            ArmyUnitType::Hussars,
            ArmyUnitType::Cuirassiers,
            ArmyUnitType::LightArtillery,
            ArmyUnitType::Sapper,
            ArmyUnitType::General,
        ] {
            assert!(t.required_tech().is_none(), "{:?} should not need tech", t);
        }
    }
}
