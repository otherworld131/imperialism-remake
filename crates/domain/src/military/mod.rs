pub mod battle_outcome;
pub use battle_outcome::{
    BattleOutcome, BattleParams, BattleSite, ProvinceConquest, compute_battle_outcome,
};
pub mod combat;
pub mod naval;
pub mod navy_placement;
pub mod ships;
pub mod units;
#[cfg(test)]
pub use combat::create_garrison;
pub use combat::{
    BattleResult, CombatForce, TargetingPriority, effective_fort_bonus, fort_defense_bonus,
    resolve_battle, resolve_battle_with_targeting, terrain_defense_bonus,
};
pub use naval::{
    NavalBattleResult, NavalOperation, ReconResult, beachhead_force_size, blockade_with_escorts,
    calculate_blockade_effect, compute_blockaded_ports, escort_protection, naval_reconnaissance,
    resolve_naval_battle,
};
pub use ships::{Ship, ShipCategory, ShipStats, ShipType};
pub use units::{ArmyUnit, ArmyUnitType, UnitCategory, UnitStats};
