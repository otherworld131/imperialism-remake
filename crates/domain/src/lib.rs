#![deny(warnings, clippy::all)]

/// Domain-level errors for inventory and reservation operations.
#[derive(Debug, Clone, PartialEq)]
pub enum DomainError {
    /// Attempted to reserve/consume more of a commodity than is available.
    InsufficientInventory {
        requested: u32,
        available: u32,
    },
    /// Attempted to commit or release a reservation that does not exist.
    ReservationNotFound(crate::types::ReservationId),
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsufficientInventory { requested, available } => write!(
                f,
                "insufficient inventory: requested {requested}, available {available}"
            ),
            Self::ReservationNotFound(id) => write!(f, "reservation not found: {id}"),
        }
    }
}

pub mod ai;
pub mod data;
pub mod diplomacy;
pub mod economy;
pub mod events;
pub mod game_state;
pub mod hex;
pub mod map;
pub mod military;
pub mod nation;
pub mod platform;
pub mod scenarios;
#[cfg(feature = "lua")]
pub mod scripting;
pub mod services;
pub mod tech;
pub mod turn;
pub mod types;
