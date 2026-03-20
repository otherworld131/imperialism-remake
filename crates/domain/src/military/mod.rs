pub mod combat;
pub mod ships;
pub mod units;
pub use combat::{BattleResult, CombatForce, create_garrison, resolve_battle};
pub use ships::{Ship, ShipCategory, ShipStats, ShipType};
pub use units::{ArmyUnit, ArmyUnitType, UnitCategory, UnitStats};
