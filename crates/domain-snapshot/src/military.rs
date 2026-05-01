use crate::types::{NationId, ProvinceId, TerrainType};
use domain::military as d;

// ── UnitId ────────────────────────────────────────────────────────

// Stored as plain u32 in the snapshot.

// ── ArmyUnitType ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ArmyUnitType {
    Militia, GarrisonArtillery, Regulars, Grenadiers, RifleInfantry, Guards, Sharpshooters,
    ModernInfantry, MachineGunners, Rangers, Cuirassiers, Scouts, CarbineCavalry, Armour,
    Mechanised, LightArtillery, StandardArtillery, FieldArtillery, SiegeArtillery, RailroadGun,
    MobileArtillery, Sapper, General,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArmyUnit {
    pub id: u32,
    pub unit_type: ArmyUnitType,
    pub owner: NationId,
    pub position: ProvinceId,
    pub health: u8,
    pub medals: u8,
    pub movement_remaining: u32,
}

// ── Ships ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ShipType {
    Trader, Indiaman, Clipper, Paddlewheeler, Freighter,
    Frigate, ShipOfTheLine, Raider, Ironclad, AdvancedIronclad,
    ArmouredCruiser, Dreadnought, Battlecruiser,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum NavalOperation {
    Patrol,
    Escort,
    Blockade(NationId),
    Beachhead(ProvinceId),
    Reconnaissance(NationId),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Ship {
    pub id: u32,
    pub ship_type: ShipType,
    pub owner: NationId,
    pub hull_remaining: u32,
    pub sea_zone: Option<u32>,
    #[serde(default)]
    pub operation: Option<NavalOperation>,
}

// ── Battle results ────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BattleResult {
    pub attacker: NationId,
    pub defender: NationId,
    pub province: ProvinceId,
    pub attacker_won: bool,
    pub attacker_casualties: Vec<ArmyUnitType>,
    pub defender_casualties: Vec<ArmyUnitType>,
    pub attacker_survivors: Vec<ArmyUnit>,
    pub defender_survivors: Vec<ArmyUnit>,
    pub terrain: Option<TerrainType>,
    pub fort_level: u8,
    pub attacker_initial_fp: f64,
    pub defender_initial_fp: f64,
    pub attacker_initial_count: usize,
    pub defender_initial_count: usize,
    pub retreated: bool,
    pub defender_retreated: bool,
    pub attacker_retreated_to: Vec<(u32, ProvinceId)>,
    pub defender_retreated_to: Vec<(u32, ProvinceId)>,
    pub siege_reduced_fort: bool,
    pub medal_awards: Vec<(ArmyUnitType, u8)>,
    pub attacker_origin_provinces: Vec<ProvinceId>,
    #[serde(default)]
    pub is_naval_landing: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NavalBattleResult {
    pub attacker: NationId,
    pub defender: NationId,
    pub attacker_won: bool,
    pub attacker_ships_lost: Vec<ShipType>,
    pub defender_ships_lost: Vec<ShipType>,
    pub attacker_survivors: Vec<Ship>,
    pub defender_survivors: Vec<Ship>,
}

// ── NationMilitary ────────────────────────────────────────────────

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NationMilitary {
    #[serde(default)]
    pub army: Vec<ArmyUnit>,
    #[serde(default)]
    pub civilians: Vec<crate::economy::Civilian>,
    #[serde(default)]
    pub transport: crate::economy::TransportSystem,
    #[serde(default)]
    pub merchant_fleet: Vec<Ship>,
    #[serde(default)]
    pub warships: Vec<Ship>,
    #[serde(default)]
    pub total_arms_built: u32,
    #[serde(default)]
    pub generals_earned: u32,
    #[serde(default)]
    pub warships_built: u32,
    #[serde(default)]
    pub warships_lost: u32,
    #[serde(default)]
    pub total_ships_of_the_line_built: u32,
    #[serde(default)]
    pub admirals_earned: u32,
    #[serde(default)]
    pub capitol_bonus_capacity: u32,
    #[serde(default)]
    pub has_colony: bool,
    #[serde(default)]
    pub expert_rewards_earned: u8,
}

// ═══════════════════════════════════════════════════════════════════
// From impls
// ═══════════════════════════════════════════════════════════════════

impl From<d::units::ArmyUnitType> for ArmyUnitType {
    fn from(v: d::units::ArmyUnitType) -> Self {
        use d::units::ArmyUnitType as D;
        match v {
            D::Militia => Self::Militia,
            D::GarrisonArtillery => Self::GarrisonArtillery,
            D::Regulars => Self::Regulars,
            D::Grenadiers => Self::Grenadiers,
            D::RifleInfantry => Self::RifleInfantry,
            D::Guards => Self::Guards,
            D::Sharpshooters => Self::Sharpshooters,
            D::ModernInfantry => Self::ModernInfantry,
            D::MachineGunners => Self::MachineGunners,
            D::Rangers => Self::Rangers,
            D::Cuirassiers => Self::Cuirassiers,
            D::Scouts => Self::Scouts,
            D::CarbineCavalry => Self::CarbineCavalry,
            D::Armour => Self::Armour,
            D::Mechanised => Self::Mechanised,
            D::LightArtillery => Self::LightArtillery,
            D::StandardArtillery => Self::StandardArtillery,
            D::FieldArtillery => Self::FieldArtillery,
            D::SiegeArtillery => Self::SiegeArtillery,
            D::RailroadGun => Self::RailroadGun,
            D::MobileArtillery => Self::MobileArtillery,
            D::Sapper => Self::Sapper,
            D::General => Self::General,
        }
    }
}
impl From<ArmyUnitType> for d::units::ArmyUnitType {
    fn from(v: ArmyUnitType) -> Self {
        use d::units::ArmyUnitType as D;
        match v {
            ArmyUnitType::Militia => D::Militia,
            ArmyUnitType::GarrisonArtillery => D::GarrisonArtillery,
            ArmyUnitType::Regulars => D::Regulars,
            ArmyUnitType::Grenadiers => D::Grenadiers,
            ArmyUnitType::RifleInfantry => D::RifleInfantry,
            ArmyUnitType::Guards => D::Guards,
            ArmyUnitType::Sharpshooters => D::Sharpshooters,
            ArmyUnitType::ModernInfantry => D::ModernInfantry,
            ArmyUnitType::MachineGunners => D::MachineGunners,
            ArmyUnitType::Rangers => D::Rangers,
            ArmyUnitType::Cuirassiers => D::Cuirassiers,
            ArmyUnitType::Scouts => D::Scouts,
            ArmyUnitType::CarbineCavalry => D::CarbineCavalry,
            ArmyUnitType::Armour => D::Armour,
            ArmyUnitType::Mechanised => D::Mechanised,
            ArmyUnitType::LightArtillery => D::LightArtillery,
            ArmyUnitType::StandardArtillery => D::StandardArtillery,
            ArmyUnitType::FieldArtillery => D::FieldArtillery,
            ArmyUnitType::SiegeArtillery => D::SiegeArtillery,
            ArmyUnitType::RailroadGun => D::RailroadGun,
            ArmyUnitType::MobileArtillery => D::MobileArtillery,
            ArmyUnitType::Sapper => D::Sapper,
            ArmyUnitType::General => D::General,
        }
    }
}

impl From<&d::units::ArmyUnit> for ArmyUnit {
    fn from(v: &d::units::ArmyUnit) -> Self {
        Self {
            id: v.id.0,
            unit_type: v.unit_type.into(),
            owner: v.owner.into(),
            position: v.position.into(),
            health: v.health,
            medals: v.medals,
            movement_remaining: v.movement_remaining,
        }
    }
}
impl From<ArmyUnit> for d::units::ArmyUnit {
    fn from(v: ArmyUnit) -> Self {
        use domain::map::UnitId;
        let mut unit = Self::new(UnitId(v.id), v.unit_type.into(), v.owner.into(), v.position.into());
        unit.health = v.health;
        unit.medals = v.medals;
        unit.movement_remaining = v.movement_remaining;
        unit
    }
}

impl From<d::ships::ShipType> for ShipType {
    fn from(v: d::ships::ShipType) -> Self {
        use d::ships::ShipType as D;
        match v {
            D::Trader => Self::Trader,
            D::Indiaman => Self::Indiaman,
            D::Clipper => Self::Clipper,
            D::Paddlewheeler => Self::Paddlewheeler,
            D::Freighter => Self::Freighter,
            D::Frigate => Self::Frigate,
            D::ShipOfTheLine => Self::ShipOfTheLine,
            D::Raider => Self::Raider,
            D::Ironclad => Self::Ironclad,
            D::AdvancedIronclad => Self::AdvancedIronclad,
            D::ArmouredCruiser => Self::ArmouredCruiser,
            D::Dreadnought => Self::Dreadnought,
            D::Battlecruiser => Self::Battlecruiser,
        }
    }
}
impl From<ShipType> for d::ships::ShipType {
    fn from(v: ShipType) -> Self {
        use d::ships::ShipType as D;
        match v {
            ShipType::Trader => D::Trader,
            ShipType::Indiaman => D::Indiaman,
            ShipType::Clipper => D::Clipper,
            ShipType::Paddlewheeler => D::Paddlewheeler,
            ShipType::Freighter => D::Freighter,
            ShipType::Frigate => D::Frigate,
            ShipType::ShipOfTheLine => D::ShipOfTheLine,
            ShipType::Raider => D::Raider,
            ShipType::Ironclad => D::Ironclad,
            ShipType::AdvancedIronclad => D::AdvancedIronclad,
            ShipType::ArmouredCruiser => D::ArmouredCruiser,
            ShipType::Dreadnought => D::Dreadnought,
            ShipType::Battlecruiser => D::Battlecruiser,
        }
    }
}

impl From<d::naval::NavalOperation> for NavalOperation {
    fn from(v: d::naval::NavalOperation) -> Self {
        match v {
            d::naval::NavalOperation::Patrol => Self::Patrol,
            d::naval::NavalOperation::Escort => Self::Escort,
            d::naval::NavalOperation::Blockade(n) => Self::Blockade(n.into()),
            d::naval::NavalOperation::Beachhead(p) => Self::Beachhead(p.into()),
            d::naval::NavalOperation::Reconnaissance(n) => Self::Reconnaissance(n.into()),
        }
    }
}
impl From<NavalOperation> for d::naval::NavalOperation {
    fn from(v: NavalOperation) -> Self {
        match v {
            NavalOperation::Patrol => Self::Patrol,
            NavalOperation::Escort => Self::Escort,
            NavalOperation::Blockade(n) => Self::Blockade(n.into()),
            NavalOperation::Beachhead(p) => Self::Beachhead(p.into()),
            NavalOperation::Reconnaissance(n) => Self::Reconnaissance(n.into()),
        }
    }
}

impl From<&d::ships::Ship> for Ship {
    fn from(v: &d::ships::Ship) -> Self {
        Self {
            id: v.id.0,
            ship_type: v.ship_type.into(),
            owner: v.owner.into(),
            hull_remaining: v.hull_remaining,
            sea_zone: v.sea_zone.map(|z| z.0),
            operation: v.operation.map(Into::into),
        }
    }
}
impl From<Ship> for d::ships::Ship {
    fn from(v: Ship) -> Self {
        use domain::map::{sea_zones::SeaZoneId, UnitId};
        // Hull is overridden immediately, so the initial value is irrelevant.
        let mut s = d::ships::Ship::new(UnitId(v.id), v.ship_type.into(), v.owner.into(), 0);
        s.hull_remaining = v.hull_remaining;
        s.sea_zone = v.sea_zone.map(SeaZoneId);
        s.operation = v.operation.map(Into::into);
        s
    }
}

impl From<&d::combat::BattleResult> for BattleResult {
    fn from(v: &d::combat::BattleResult) -> Self {
        Self {
            attacker: v.attacker.into(),
            defender: v.defender.into(),
            province: v.province.into(),
            attacker_won: v.attacker_won,
            attacker_casualties: v.attacker_casualties.iter().copied().map(Into::into).collect(),
            defender_casualties: v.defender_casualties.iter().copied().map(Into::into).collect(),
            attacker_survivors: v.attacker_survivors.iter().map(Into::into).collect(),
            defender_survivors: v.defender_survivors.iter().map(Into::into).collect(),
            terrain: v.terrain.map(Into::into),
            fort_level: v.fort_level,
            attacker_initial_fp: v.attacker_initial_fp,
            defender_initial_fp: v.defender_initial_fp,
            attacker_initial_count: v.attacker_initial_count,
            defender_initial_count: v.defender_initial_count,
            retreated: v.retreated,
            defender_retreated: v.defender_retreated,
            attacker_retreated_to: v.attacker_retreated_to.iter().map(|(uid, pid)| (uid.0, (*pid).into())).collect(),
            defender_retreated_to: v.defender_retreated_to.iter().map(|(uid, pid)| (uid.0, (*pid).into())).collect(),
            siege_reduced_fort: v.siege_reduced_fort,
            medal_awards: v.medal_awards.iter().map(|(ut, m)| ((*ut).into(), *m)).collect(),
            attacker_origin_provinces: v.attacker_origin_provinces.iter().copied().map(Into::into).collect(),
            is_naval_landing: v.is_naval_landing,
        }
    }
}
impl From<BattleResult> for d::combat::BattleResult {
    fn from(v: BattleResult) -> Self {
        use domain::map::UnitId;
        Self {
            attacker: v.attacker.into(),
            defender: v.defender.into(),
            province: v.province.into(),
            attacker_won: v.attacker_won,
            attacker_casualties: v.attacker_casualties.into_iter().map(Into::into).collect(),
            defender_casualties: v.defender_casualties.into_iter().map(Into::into).collect(),
            attacker_survivors: v.attacker_survivors.into_iter().map(Into::into).collect(),
            defender_survivors: v.defender_survivors.into_iter().map(Into::into).collect(),
            terrain: v.terrain.map(Into::into),
            fort_level: v.fort_level,
            attacker_initial_fp: v.attacker_initial_fp,
            defender_initial_fp: v.defender_initial_fp,
            attacker_initial_count: v.attacker_initial_count,
            defender_initial_count: v.defender_initial_count,
            retreated: v.retreated,
            defender_retreated: v.defender_retreated,
            attacker_retreated_to: v.attacker_retreated_to.into_iter().map(|(id, p)| (UnitId(id), p.into())).collect(),
            defender_retreated_to: v.defender_retreated_to.into_iter().map(|(id, p)| (UnitId(id), p.into())).collect(),
            siege_reduced_fort: v.siege_reduced_fort,
            medal_awards: v.medal_awards.into_iter().map(|(ut, m)| (ut.into(), m)).collect(),
            attacker_origin_provinces: v.attacker_origin_provinces.into_iter().map(Into::into).collect(),
            is_naval_landing: v.is_naval_landing,
        }
    }
}

impl From<&d::naval::NavalBattleResult> for NavalBattleResult {
    fn from(v: &d::naval::NavalBattleResult) -> Self {
        Self {
            attacker: v.attacker.into(),
            defender: v.defender.into(),
            attacker_won: v.attacker_won,
            attacker_ships_lost: v.attacker_ships_lost.iter().copied().map(Into::into).collect(),
            defender_ships_lost: v.defender_ships_lost.iter().copied().map(Into::into).collect(),
            attacker_survivors: v.attacker_survivors.iter().map(Into::into).collect(),
            defender_survivors: v.defender_survivors.iter().map(Into::into).collect(),
        }
    }
}
impl From<NavalBattleResult> for d::naval::NavalBattleResult {
    fn from(v: NavalBattleResult) -> Self {
        Self {
            attacker: v.attacker.into(),
            defender: v.defender.into(),
            attacker_won: v.attacker_won,
            attacker_ships_lost: v.attacker_ships_lost.into_iter().map(Into::into).collect(),
            defender_ships_lost: v.defender_ships_lost.into_iter().map(Into::into).collect(),
            attacker_survivors: v.attacker_survivors.into_iter().map(Into::into).collect(),
            defender_survivors: v.defender_survivors.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<&domain::nation::NationMilitary> for NationMilitary {
    fn from(v: &domain::nation::NationMilitary) -> Self {
        Self {
            army: v.army.iter().map(Into::into).collect(),
            civilians: v.civilians.iter().map(Into::into).collect(),
            transport: (&v.transport).into(),
            merchant_fleet: v.merchant_fleet.iter().map(Into::into).collect(),
            warships: v.warships.iter().map(Into::into).collect(),
            total_arms_built: v.total_arms_built,
            generals_earned: v.generals_earned,
            warships_built: v.warships_built,
            warships_lost: v.warships_lost,
            total_ships_of_the_line_built: v.total_ships_of_the_line_built,
            admirals_earned: v.admirals_earned,
            capitol_bonus_capacity: v.capitol_bonus_capacity,
            has_colony: v.has_colony,
            expert_rewards_earned: v.expert_rewards_earned,
        }
    }
}
impl From<NationMilitary> for domain::nation::NationMilitary {
    fn from(v: NationMilitary) -> Self {
        domain::nation::NationMilitary {
            army: v.army.into_iter().map(Into::into).collect(),
            civilians: v.civilians.into_iter().map(Into::into).collect(),
            transport: v.transport.into(),
            merchant_fleet: v.merchant_fleet.into_iter().map(Into::into).collect(),
            warships: v.warships.into_iter().map(Into::into).collect(),
            total_arms_built: v.total_arms_built,
            generals_earned: v.generals_earned,
            warships_built: v.warships_built,
            warships_lost: v.warships_lost,
            total_ships_of_the_line_built: v.total_ships_of_the_line_built,
            admirals_earned: v.admirals_earned,
            capitol_bonus_capacity: v.capitol_bonus_capacity,
            has_colony: v.has_colony,
            expert_rewards_earned: v.expert_rewards_earned,
            // Transient per-turn state: reset on load (budgets recompute each turn).
            fleet_moves_remaining: std::collections::HashMap::new(),
        }
    }
}
