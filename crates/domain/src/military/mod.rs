pub mod combat;
pub mod ships;
pub mod units;
pub use combat::{
    BattleResult, CombatForce, create_garrison, fort_defense_bonus, resolve_battle,
    terrain_defense_bonus,
};
pub use ships::{Ship, ShipCategory, ShipStats, ShipType};
pub use units::{ArmyUnit, ArmyUnitType, UnitCategory, UnitStats};
