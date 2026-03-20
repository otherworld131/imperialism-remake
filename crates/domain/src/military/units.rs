use crate::map::UnitId;
use crate::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum UnitCategory {
    Infantry,
    Cavalry,
    Artillery,
    Special,
    Garrison, // Militia/Minutemen - immovable
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ArmyUnitType {
    // Garrison
    Militia,
    // Infantry
    Regulars,
    Grenadiers,
    RifleInfantry,
    Guards,
    Sharpshooters,
    ModernInfantry,
    MachineGunners,
    Rangers,
    // Cavalry
    Cuirassiers,
    Scouts,
    CarbineCavalry,
    Armour,
    Mechanised,
    // Artillery
    LightArtillery,
    StandardArtillery,
    FieldArtillery,
    SiegeArtillery,
    RailroadGun,
    MobileArtillery,
    // Special
    Sapper,
}

#[derive(Debug, Clone)]
pub struct UnitStats {
    pub firepower: u32,
    pub movement: u32,
    pub range: u32,
    pub cost: Money,
    pub arms_required: u32,
    pub requires_horse: bool,
    pub category: UnitCategory,
    pub maintenance_per_turn: Money, // $25 per arm
    pub prerequisite_tech: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArmyUnit {
    pub id: UnitId,
    pub unit_type: ArmyUnitType,
    pub owner: NationId,
    pub position: ProvinceId,
    pub health: u8, // 0-100 in 5% increments
    pub medals: u8, // 0-4+
    pub movement_remaining: u32,
}

impl ArmyUnitType {
    /// Returns the base stats for each unit type.
    pub fn stats(&self) -> UnitStats {
        match self {
            // ── Garrison ──────────────────────────────────────────
            ArmyUnitType::Militia => UnitStats {
                firepower: 1,
                movement: 0,
                range: 1,
                cost: Money::dollars(50),
                arms_required: 1,
                requires_horse: false,
                category: UnitCategory::Garrison,
                maintenance_per_turn: Money::dollars(25),
                prerequisite_tech: None,
            },

            // ── Infantry ─────────────────────────────────────────
            ArmyUnitType::Regulars => UnitStats {
                firepower: 2,
                movement: 3,
                range: 1,
                cost: Money::dollars(100),
                arms_required: 1,
                requires_horse: false,
                category: UnitCategory::Infantry,
                maintenance_per_turn: Money::dollars(25),
                prerequisite_tech: None,
            },
            ArmyUnitType::Grenadiers => UnitStats {
                firepower: 3,
                movement: 3,
                range: 1,
                cost: Money::dollars(150),
                arms_required: 2,
                requires_horse: false,
                category: UnitCategory::Infantry,
                maintenance_per_turn: Money::dollars(50),
                prerequisite_tech: Some("Grenadier Tactics".to_string()),
            },
            ArmyUnitType::RifleInfantry => UnitStats {
                firepower: 4,
                movement: 3,
                range: 2,
                cost: Money::dollars(200),
                arms_required: 2,
                requires_horse: false,
                category: UnitCategory::Infantry,
                maintenance_per_turn: Money::dollars(50),
                prerequisite_tech: Some("Rifling".to_string()),
            },
            ArmyUnitType::Guards => UnitStats {
                firepower: 5,
                movement: 3,
                range: 2,
                cost: Money::dollars(250),
                arms_required: 3,
                requires_horse: false,
                category: UnitCategory::Infantry,
                maintenance_per_turn: Money::dollars(75),
                prerequisite_tech: Some("Professional Army".to_string()),
            },
            ArmyUnitType::Sharpshooters => UnitStats {
                firepower: 4,
                movement: 3,
                range: 3,
                cost: Money::dollars(200),
                arms_required: 2,
                requires_horse: false,
                category: UnitCategory::Infantry,
                maintenance_per_turn: Money::dollars(50),
                prerequisite_tech: Some("Sharpshooter Training".to_string()),
            },
            ArmyUnitType::ModernInfantry => UnitStats {
                firepower: 6,
                movement: 4,
                range: 2,
                cost: Money::dollars(300),
                arms_required: 3,
                requires_horse: false,
                category: UnitCategory::Infantry,
                maintenance_per_turn: Money::dollars(75),
                prerequisite_tech: Some("Modern Warfare".to_string()),
            },
            ArmyUnitType::MachineGunners => UnitStats {
                firepower: 8,
                movement: 2,
                range: 2,
                cost: Money::dollars(400),
                arms_required: 4,
                requires_horse: false,
                category: UnitCategory::Infantry,
                maintenance_per_turn: Money::dollars(100),
                prerequisite_tech: Some("Machine Guns".to_string()),
            },
            ArmyUnitType::Rangers => UnitStats {
                firepower: 5,
                movement: 5,
                range: 2,
                cost: Money::dollars(250),
                arms_required: 2,
                requires_horse: false,
                category: UnitCategory::Infantry,
                maintenance_per_turn: Money::dollars(50),
                prerequisite_tech: Some("Ranger Training".to_string()),
            },

            // ── Cavalry ──────────────────────────────────────────
            ArmyUnitType::Cuirassiers => UnitStats {
                firepower: 3,
                movement: 5,
                range: 1,
                cost: Money::dollars(200),
                arms_required: 2,
                requires_horse: true,
                category: UnitCategory::Cavalry,
                maintenance_per_turn: Money::dollars(50),
                prerequisite_tech: None,
            },
            ArmyUnitType::Scouts => UnitStats {
                firepower: 1,
                movement: 7,
                range: 1,
                cost: Money::dollars(100),
                arms_required: 1,
                requires_horse: true,
                category: UnitCategory::Cavalry,
                maintenance_per_turn: Money::dollars(25),
                prerequisite_tech: None,
            },
            ArmyUnitType::CarbineCavalry => UnitStats {
                firepower: 4,
                movement: 5,
                range: 2,
                cost: Money::dollars(250),
                arms_required: 2,
                requires_horse: true,
                category: UnitCategory::Cavalry,
                maintenance_per_turn: Money::dollars(50),
                prerequisite_tech: Some("Carbines".to_string()),
            },
            ArmyUnitType::Armour => UnitStats {
                firepower: 8,
                movement: 4,
                range: 2,
                cost: Money::dollars(500),
                arms_required: 4,
                requires_horse: false,
                category: UnitCategory::Cavalry,
                maintenance_per_turn: Money::dollars(100),
                prerequisite_tech: Some("Armoured Vehicles".to_string()),
            },
            ArmyUnitType::Mechanised => UnitStats {
                firepower: 6,
                movement: 6,
                range: 2,
                cost: Money::dollars(400),
                arms_required: 3,
                requires_horse: false,
                category: UnitCategory::Cavalry,
                maintenance_per_turn: Money::dollars(75),
                prerequisite_tech: Some("Mechanisation".to_string()),
            },

            // ── Artillery ────────────────────────────────────────
            ArmyUnitType::LightArtillery => UnitStats {
                firepower: 3,
                movement: 3,
                range: 3,
                cost: Money::dollars(200),
                arms_required: 2,
                requires_horse: false,
                category: UnitCategory::Artillery,
                maintenance_per_turn: Money::dollars(50),
                prerequisite_tech: None,
            },
            ArmyUnitType::StandardArtillery => UnitStats {
                firepower: 5,
                movement: 2,
                range: 4,
                cost: Money::dollars(300),
                arms_required: 3,
                requires_horse: false,
                category: UnitCategory::Artillery,
                maintenance_per_turn: Money::dollars(75),
                prerequisite_tech: Some("Improved Artillery".to_string()),
            },
            ArmyUnitType::FieldArtillery => UnitStats {
                firepower: 6,
                movement: 3,
                range: 4,
                cost: Money::dollars(350),
                arms_required: 3,
                requires_horse: false,
                category: UnitCategory::Artillery,
                maintenance_per_turn: Money::dollars(75),
                prerequisite_tech: Some("Field Artillery".to_string()),
            },
            ArmyUnitType::SiegeArtillery => UnitStats {
                firepower: 10,
                movement: 1,
                range: 5,
                cost: Money::dollars(500),
                arms_required: 4,
                requires_horse: false,
                category: UnitCategory::Artillery,
                maintenance_per_turn: Money::dollars(100),
                prerequisite_tech: Some("Siege Warfare".to_string()),
            },
            ArmyUnitType::RailroadGun => UnitStats {
                firepower: 12,
                movement: 1,
                range: 6,
                cost: Money::dollars(600),
                arms_required: 5,
                requires_horse: false,
                category: UnitCategory::Artillery,
                maintenance_per_turn: Money::dollars(125),
                prerequisite_tech: Some("Railroad Artillery".to_string()),
            },
            ArmyUnitType::MobileArtillery => UnitStats {
                firepower: 7,
                movement: 4,
                range: 4,
                cost: Money::dollars(450),
                arms_required: 4,
                requires_horse: false,
                category: UnitCategory::Artillery,
                maintenance_per_turn: Money::dollars(100),
                prerequisite_tech: Some("Mobile Artillery".to_string()),
            },

            // ── Special ──────────────────────────────────────────
            ArmyUnitType::Sapper => UnitStats {
                firepower: 1,
                movement: 3,
                range: 1,
                cost: Money::dollars(150),
                arms_required: 1,
                requires_horse: false,
                category: UnitCategory::Special,
                maintenance_per_turn: Money::dollars(25),
                prerequisite_tech: Some("Engineering".to_string()),
            },
        }
    }

    /// Returns the unit category for this unit type.
    pub fn category(&self) -> UnitCategory {
        self.stats().category
    }

    /// Returns whether this unit type can move. Militia (garrison) cannot.
    pub fn can_move(&self) -> bool {
        !matches!(self, ArmyUnitType::Militia)
    }

    /// Returns the next upgrade for this unit type, if any.
    ///
    /// Upgrade paths:
    /// - Regulars -> RifleInfantry -> ModernInfantry
    /// - Grenadiers -> Guards
    /// - Sharpshooters -> Rangers
    /// - Cuirassiers -> CarbineCavalry
    /// - Scouts -> CarbineCavalry
    /// - LightArtillery -> FieldArtillery -> MobileArtillery
    /// - StandardArtillery -> SiegeArtillery -> RailroadGun
    pub fn upgrade_to(&self) -> Option<ArmyUnitType> {
        match self {
            // Infantry upgrades
            ArmyUnitType::Regulars => Some(ArmyUnitType::RifleInfantry),
            ArmyUnitType::RifleInfantry => Some(ArmyUnitType::ModernInfantry),
            ArmyUnitType::Grenadiers => Some(ArmyUnitType::Guards),
            ArmyUnitType::Sharpshooters => Some(ArmyUnitType::Rangers),

            // Cavalry upgrades
            ArmyUnitType::Cuirassiers => Some(ArmyUnitType::CarbineCavalry),
            ArmyUnitType::Scouts => Some(ArmyUnitType::CarbineCavalry),

            // Artillery upgrades
            ArmyUnitType::LightArtillery => Some(ArmyUnitType::FieldArtillery),
            ArmyUnitType::FieldArtillery => Some(ArmyUnitType::MobileArtillery),
            ArmyUnitType::StandardArtillery => Some(ArmyUnitType::SiegeArtillery),
            ArmyUnitType::SiegeArtillery => Some(ArmyUnitType::RailroadGun),

            // No upgrade available
            _ => None,
        }
    }
}

impl ArmyUnit {
    /// Create a new army unit at 100% health with 0 medals.
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

    /// Calculate effective firepower with medal modifier.
    /// Medal modifier: (1.0 + medals * 0.25), so 4 medals = 2.0x.
    pub fn effective_firepower(&self) -> f64 {
        let base_fp = self.unit_type.stats().firepower as f64;
        let medal_modifier = 1.0 + self.medals as f64 * 0.25;
        base_fp * medal_modifier
    }

    /// Reduce health by the given amount in 5% increments, minimum 0.
    pub fn take_damage(&mut self, amount: u8) {
        // Round to nearest 5% increment
        let rounded = (amount / 5) * 5;
        self.health = self.health.saturating_sub(rounded);
    }

    /// Heal the unit by the given amount. Medal holders heal faster:
    /// effective healing = amount * (1 + medals / 2).
    /// Health is capped at 100.
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

    /// Award a medal to this unit, incrementing the medal count.
    pub fn award_medal(&mut self) {
        self.medals += 1;
    }

    /// Returns true if the unit is still alive (health > 0).
    pub fn is_alive(&self) -> bool {
        self.health > 0
    }

    /// Calculate maintenance cost: $25 per arm in the unit.
    pub fn maintenance_cost(&self) -> Money {
        let arms = self.unit_type.stats().arms_required;
        Money::dollars(25) * arms as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Stats for key units ─────────────────────────────────────

    #[test]
    fn regulars_stats() {
        let stats = ArmyUnitType::Regulars.stats();
        assert_eq!(stats.firepower, 2);
        assert_eq!(stats.movement, 3);
        assert_eq!(stats.range, 1);
        assert_eq!(stats.cost, Money::dollars(100));
        assert_eq!(stats.arms_required, 1);
        assert!(!stats.requires_horse);
        assert_eq!(stats.category, UnitCategory::Infantry);
        assert_eq!(stats.maintenance_per_turn, Money::dollars(25));
        assert!(stats.prerequisite_tech.is_none());
    }

    #[test]
    fn guards_stats() {
        let stats = ArmyUnitType::Guards.stats();
        assert_eq!(stats.firepower, 5);
        assert_eq!(stats.movement, 3);
        assert_eq!(stats.range, 2);
        assert_eq!(stats.cost, Money::dollars(250));
        assert_eq!(stats.arms_required, 3);
        assert!(!stats.requires_horse);
        assert_eq!(stats.category, UnitCategory::Infantry);
        assert_eq!(stats.maintenance_per_turn, Money::dollars(75));
        assert_eq!(
            stats.prerequisite_tech,
            Some("Professional Army".to_string())
        );
    }

    #[test]
    fn siege_artillery_stats() {
        let stats = ArmyUnitType::SiegeArtillery.stats();
        assert_eq!(stats.firepower, 10);
        assert_eq!(stats.movement, 1);
        assert_eq!(stats.range, 5);
        assert_eq!(stats.cost, Money::dollars(500));
        assert_eq!(stats.arms_required, 4);
        assert!(!stats.requires_horse);
        assert_eq!(stats.category, UnitCategory::Artillery);
        assert_eq!(stats.maintenance_per_turn, Money::dollars(100));
        assert_eq!(stats.prerequisite_tech, Some("Siege Warfare".to_string()));
    }

    #[test]
    fn cuirassiers_stats() {
        let stats = ArmyUnitType::Cuirassiers.stats();
        assert_eq!(stats.firepower, 3);
        assert_eq!(stats.movement, 5);
        assert_eq!(stats.range, 1);
        assert_eq!(stats.cost, Money::dollars(200));
        assert_eq!(stats.arms_required, 2);
        assert!(stats.requires_horse);
        assert_eq!(stats.category, UnitCategory::Cavalry);
        assert_eq!(stats.maintenance_per_turn, Money::dollars(50));
        assert!(stats.prerequisite_tech.is_none());
    }

    // ── Medal firepower scaling ─────────────────────────────────

    #[test]
    fn medal_firepower_0_medals_is_1x() {
        let unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        assert_eq!(unit.medals, 0);
        // Regulars base fp = 2, 0 medals = 1.0x => 2.0
        assert!((unit.effective_firepower() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn medal_firepower_1_medal_is_1_25x() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.award_medal();
        // 2 * 1.25 = 2.5
        assert!((unit.effective_firepower() - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn medal_firepower_4_medals_is_2x() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        for _ in 0..4 {
            unit.award_medal();
        }
        assert_eq!(unit.medals, 4);
        // 2 * 2.0 = 4.0
        assert!((unit.effective_firepower() - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn medal_firepower_guards_with_3_medals() {
        let mut unit = ArmyUnit::new(UnitId(2), ArmyUnitType::Guards, NationId(1), ProvinceId(1));
        for _ in 0..3 {
            unit.award_medal();
        }
        // Guards base fp = 5, 3 medals = 1.75x => 8.75
        assert!((unit.effective_firepower() - 8.75).abs() < f64::EPSILON);
    }

    // ── Health damage in 5% increments ──────────────────────────

    #[test]
    fn take_damage_rounds_to_5_percent() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        assert_eq!(unit.health, 100);

        // 7 damage rounds down to 5
        unit.take_damage(7);
        assert_eq!(unit.health, 95);

        // 13 damage rounds down to 10
        unit.take_damage(13);
        assert_eq!(unit.health, 85);
    }

    #[test]
    fn take_damage_exact_5_increment() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.take_damage(20);
        assert_eq!(unit.health, 80);
    }

    #[test]
    fn take_damage_cannot_go_below_zero() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.take_damage(250); // way more than 100 hp
        assert_eq!(unit.health, 0);
        assert!(!unit.is_alive());
    }

    #[test]
    fn take_damage_less_than_5_rounds_to_zero_damage() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.take_damage(4); // rounds to 0
        assert_eq!(unit.health, 100);
    }

    // ── Healing ─────────────────────────────────────────────────

    #[test]
    fn heal_basic() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.health = 50;
        unit.heal(10);
        assert_eq!(unit.health, 60);
    }

    #[test]
    fn heal_caps_at_100() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.health = 95;
        unit.heal(20);
        assert_eq!(unit.health, 100);
    }

    #[test]
    fn heal_medal_bonus() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.health = 50;
        unit.medals = 2;
        // effective healing = 10 * (1 + 2/2) = 10 * 2 = 20
        unit.heal(10);
        assert_eq!(unit.health, 70);
    }

    #[test]
    fn heal_4_medals_bonus() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.health = 50;
        unit.medals = 4;
        // effective healing = 10 * (1 + 4/2) = 10 * 3 = 30
        unit.heal(10);
        assert_eq!(unit.health, 80);
    }

    // ── Militia cannot move ─────────────────────────────────────

    #[test]
    fn militia_cannot_move() {
        assert!(!ArmyUnitType::Militia.can_move());
        assert_eq!(ArmyUnitType::Militia.stats().movement, 0);
    }

    #[test]
    fn regulars_can_move() {
        assert!(ArmyUnitType::Regulars.can_move());
    }

    #[test]
    fn militia_is_garrison_category() {
        assert_eq!(ArmyUnitType::Militia.category(), UnitCategory::Garrison);
    }

    // ── Upgrade paths ───────────────────────────────────────────

    #[test]
    fn regulars_upgrade_to_rifle_infantry() {
        assert_eq!(
            ArmyUnitType::Regulars.upgrade_to(),
            Some(ArmyUnitType::RifleInfantry)
        );
    }

    #[test]
    fn rifle_infantry_upgrade_to_modern_infantry() {
        assert_eq!(
            ArmyUnitType::RifleInfantry.upgrade_to(),
            Some(ArmyUnitType::ModernInfantry)
        );
    }

    #[test]
    fn modern_infantry_no_upgrade() {
        assert_eq!(ArmyUnitType::ModernInfantry.upgrade_to(), None);
    }

    #[test]
    fn grenadiers_upgrade_to_guards() {
        assert_eq!(
            ArmyUnitType::Grenadiers.upgrade_to(),
            Some(ArmyUnitType::Guards)
        );
    }

    #[test]
    fn cuirassiers_upgrade_to_carbine_cavalry() {
        assert_eq!(
            ArmyUnitType::Cuirassiers.upgrade_to(),
            Some(ArmyUnitType::CarbineCavalry)
        );
    }

    #[test]
    fn light_artillery_upgrade_chain() {
        assert_eq!(
            ArmyUnitType::LightArtillery.upgrade_to(),
            Some(ArmyUnitType::FieldArtillery)
        );
        assert_eq!(
            ArmyUnitType::FieldArtillery.upgrade_to(),
            Some(ArmyUnitType::MobileArtillery)
        );
        assert_eq!(ArmyUnitType::MobileArtillery.upgrade_to(), None);
    }

    #[test]
    fn standard_artillery_upgrade_chain() {
        assert_eq!(
            ArmyUnitType::StandardArtillery.upgrade_to(),
            Some(ArmyUnitType::SiegeArtillery)
        );
        assert_eq!(
            ArmyUnitType::SiegeArtillery.upgrade_to(),
            Some(ArmyUnitType::RailroadGun)
        );
        assert_eq!(ArmyUnitType::RailroadGun.upgrade_to(), None);
    }

    #[test]
    fn militia_no_upgrade() {
        assert_eq!(ArmyUnitType::Militia.upgrade_to(), None);
    }

    #[test]
    fn sapper_no_upgrade() {
        assert_eq!(ArmyUnitType::Sapper.upgrade_to(), None);
    }

    // ── Maintenance cost calculations ───────────────────────────

    #[test]
    fn regulars_maintenance_1_arm() {
        let unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        // 1 arm * $25 = $25
        assert_eq!(unit.maintenance_cost(), Money::dollars(25));
    }

    #[test]
    fn cuirassiers_maintenance_2_arms() {
        let unit = ArmyUnit::new(
            UnitId(2),
            ArmyUnitType::Cuirassiers,
            NationId(1),
            ProvinceId(1),
        );
        // 2 arms * $25 = $50
        assert_eq!(unit.maintenance_cost(), Money::dollars(50));
    }

    #[test]
    fn guards_maintenance_3_arms() {
        let unit = ArmyUnit::new(UnitId(3), ArmyUnitType::Guards, NationId(1), ProvinceId(1));
        // 3 arms * $25 = $75
        assert_eq!(unit.maintenance_cost(), Money::dollars(75));
    }

    #[test]
    fn siege_artillery_maintenance_4_arms() {
        let unit = ArmyUnit::new(
            UnitId(4),
            ArmyUnitType::SiegeArtillery,
            NationId(1),
            ProvinceId(1),
        );
        // 4 arms * $25 = $100
        assert_eq!(unit.maintenance_cost(), Money::dollars(100));
    }

    #[test]
    fn railroad_gun_maintenance_5_arms() {
        let unit = ArmyUnit::new(
            UnitId(5),
            ArmyUnitType::RailroadGun,
            NationId(1),
            ProvinceId(1),
        );
        // 5 arms * $25 = $125
        assert_eq!(unit.maintenance_cost(), Money::dollars(125));
    }

    #[test]
    fn militia_maintenance_1_arm() {
        let unit = ArmyUnit::new(UnitId(6), ArmyUnitType::Militia, NationId(1), ProvinceId(1));
        // 1 arm * $25 = $25
        assert_eq!(unit.maintenance_cost(), Money::dollars(25));
    }

    // ── ArmyUnit::new defaults ──────────────────────────────────

    #[test]
    fn new_unit_starts_at_full_health() {
        let unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        assert_eq!(unit.health, 100);
        assert_eq!(unit.medals, 0);
        assert!(unit.is_alive());
    }

    #[test]
    fn new_unit_movement_matches_stats() {
        let unit = ArmyUnit::new(UnitId(1), ArmyUnitType::Scouts, NationId(1), ProvinceId(1));
        assert_eq!(unit.movement_remaining, 7);
    }

    // ── is_alive ────────────────────────────────────────────────

    #[test]
    fn unit_at_zero_health_is_dead() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.health = 0;
        assert!(!unit.is_alive());
    }

    #[test]
    fn unit_at_5_health_is_alive() {
        let mut unit = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(1),
        );
        unit.health = 5;
        assert!(unit.is_alive());
    }

    // ── Category helper ─────────────────────────────────────────

    #[test]
    fn all_categories_are_correct() {
        assert_eq!(ArmyUnitType::Militia.category(), UnitCategory::Garrison);
        assert_eq!(ArmyUnitType::Regulars.category(), UnitCategory::Infantry);
        assert_eq!(ArmyUnitType::Cuirassiers.category(), UnitCategory::Cavalry);
        assert_eq!(
            ArmyUnitType::LightArtillery.category(),
            UnitCategory::Artillery
        );
        assert_eq!(ArmyUnitType::Sapper.category(), UnitCategory::Special);
    }
}
