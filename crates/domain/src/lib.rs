#![deny(warnings, clippy::all)]

/// Domain-level errors covering invariant violations, illegal moves, and lookup failures.
#[derive(Debug, Clone, PartialEq)]
pub enum DomainError {
    /// Attempted to reserve/consume more of a commodity than is available.
    InsufficientInventory {
        requested: u32,
        available: u32,
    },
    /// Attempted to commit or release a reservation that does not exist.
    ReservationNotFound(crate::types::ReservationId),
    /// An operation was called with an invalid argument (e.g. negative amount).
    InvalidOperation(String),
    /// A referenced nation does not exist (e.g. eliminated or invalid id).
    NationNotFound(crate::types::NationId),
    /// A referenced province does not exist.
    ProvinceNotFound(crate::types::ProvinceId),
    /// A referenced tile coordinate is out of bounds or not present on the map.
    TileNotFound(crate::hex::HexCoord),
    /// A move, build, or diplomatic action is prohibited by game rules.
    IllegalMove { reason: String },
}

impl DomainError {
    /// Construct an `IllegalMove` error from any `Display`-able value.
    pub fn illegal(reason: impl std::fmt::Display) -> Self {
        Self::IllegalMove { reason: reason.to_string() }
    }
}

impl From<DomainError> for String {
    fn from(e: DomainError) -> String {
        e.to_string()
    }
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsufficientInventory { requested, available } => write!(
                f,
                "insufficient inventory: requested {requested}, available {available}"
            ),
            Self::ReservationNotFound(id) => write!(f, "reservation not found: {id}"),
            Self::InvalidOperation(msg) => write!(f, "invalid operation: {msg}"),
            Self::NationNotFound(id) => write!(f, "nation not found: {id:?}"),
            Self::ProvinceNotFound(id) => write!(f, "province not found: {id:?}"),
            Self::TileNotFound(coord) => write!(f, "tile not found at {coord:?}"),
            Self::IllegalMove { reason } => write!(f, "illegal move: {reason}"),
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
