pub mod combat;
pub mod naval;
pub mod navy_placement;
pub mod ships;
pub mod units;
pub use combat::{
    BattleResult, CombatForce, TargetingPriority, create_garrison, effective_fort_bonus,
    fort_defense_bonus, resolve_battle, resolve_battle_with_targeting, terrain_defense_bonus,
};
pub use naval::{
    NavalBattleResult, NavalOperation, ReconResult, beachhead_force_size, blockade_with_escorts,
    calculate_blockade_effect, escort_protection, naval_reconnaissance, resolve_naval_battle,
};
pub use ships::{Ship, ShipCategory, ShipStats, ShipType};
pub use units::{ArmyUnit, ArmyUnitType, UnitCategory, UnitStats};
