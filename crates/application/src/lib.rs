#![deny(warnings, clippy::all)]

pub mod commands;
pub mod queries;

pub use domain::economy::CivilianType;
pub use domain::game_state::{GameState, new_game, new_game_with_data};
pub use domain::hex::HexCoord;
pub use domain::nation::NationColor;
pub use domain::scenarios;
pub use domain::turn::{TurnReport, calculate_score, process_turn};
pub use domain::types;
pub use domain::types::*;

/// Application-layer errors covering command/query validation failures and
/// wrapped domain errors.
#[derive(Debug)]
pub enum ApplicationError {
    /// A frontend command or query was structurally invalid.
    InvalidCommand { reason: String },
    /// A referenced entity (nation, province, …) was not found.
    NotFound { reason: String },
    /// The underlying domain rejected the operation.
    Domain(domain::DomainError),
}

impl ApplicationError {
    pub fn invalid(reason: impl std::fmt::Display) -> Self {
        Self::InvalidCommand {
            reason: reason.to_string(),
        }
    }
    pub fn not_found(reason: impl std::fmt::Display) -> Self {
        Self::NotFound {
            reason: reason.to_string(),
        }
    }
}

impl From<domain::DomainError> for ApplicationError {
    fn from(e: domain::DomainError) -> Self {
        Self::Domain(e)
    }
}

impl std::fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCommand { reason } => write!(f, "invalid command: {reason}"),
            Self::NotFound { reason } => write!(f, "not found: {reason}"),
            Self::Domain(e) => write!(f, "domain error: {e}"),
        }
    }
}

impl From<ApplicationError> for String {
    fn from(e: ApplicationError) -> String {
        e.to_string()
    }
}
