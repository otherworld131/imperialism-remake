//! Pre-execution economy observability for UI and debug tooling (Trello #169).
//!
//! Surfaces what is committed this turn but not yet executed, what could still
//! be spent before end-of-turn, and why specific actions are blocked.
//!
//! The query methods live on `NationEconomy` (in `nation.rs`); this module
//! defines the supporting types.

use crate::economy::trade::Commodity;
use crate::economy::labor::WorkerType;
use crate::types::*;

/// Why an economy action cannot be executed right now.
///
/// Returned by `NationEconomy::block_reason_for_commodity` and
/// `NationEconomy::block_reason_for_treasury` to let the UI explain to the
/// player why a build or trade action is blocked.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BlockReason {
    /// The nation does not hold enough of the required commodity.
    InsufficientInventory {
        commodity: Commodity,
        needed: u32,
        available: u32,
    },
    /// The nation does not have enough labor of the required tier.
    InsufficientLabor {
        tier: WorkerType,
        needed: u32,
        available: u32,
    },
    /// The nation does not have enough freight capacity.
    InsufficientFreight { needed: u32, available: u32 },
    /// The nation does not have enough treasury funds.
    InsufficientTreasury { needed: Money, available: Money },
    /// A named prerequisite (e.g. a building or tech) is missing.
    MissingPrerequisite(String),
}

impl std::fmt::Display for BlockReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockReason::InsufficientInventory { commodity, needed, available } => {
                write!(f, "Need {needed} {commodity} but only {available} available")
            }
            BlockReason::InsufficientLabor { tier, needed, available } => {
                write!(f, "Need {needed} {tier:?} workers but only {available} available")
            }
            BlockReason::InsufficientFreight { needed, available } => {
                write!(f, "Need {needed} freight capacity but only {available} available")
            }
            BlockReason::InsufficientTreasury { needed, available } => {
                write!(f, "Need {needed} but treasury only has {available}")
            }
            BlockReason::MissingPrerequisite(name) => {
                write!(f, "Missing prerequisite: {name}")
            }
        }
    }
}
