#![deny(warnings, clippy::all)]

/// Domain-level errors covering invariant violations, illegal moves, and lookup failures.
#[derive(Debug, Clone, PartialEq)]
pub enum DomainError {
    /// Attempted to reserve/consume more of a commodity than is available.
    InsufficientInventory { requested: u32, available: u32 },
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
        Self::IllegalMove {
            reason: reason.to_string(),
        }
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
            Self::InsufficientInventory {
                requested,
                available,
            } => write!(
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

#[cfg(test)]
#[macro_export]
macro_rules! test_game_state {
    (
        turn: $turn:expr,
        difficulty: $difficulty:expr,
        map_key: $map_key:expr,
        hex_map: $hex_map:expr,
        provinces: $provinces:expr,
        nations: $nations:expr,
        human_player_nation: $human_player_nation:expr,
        events: $events:expr,
        game_data: $game_data:expr,
        diplomacy: $diplomacy:expr,
        pending_attacks: $pending_attacks:expr,
        pending_moves: $pending_moves:expr,
        pending_landings: $pending_landings:expr,
        history: $history:expr,
        high_scores: $high_scores:expr,
        newspaper_archive: $newspaper_archive:expr,
        battle_archive: $battle_archive:expr,
        political_archive: $political_archive:expr,
        ai_debug: $ai_debug:expr,
        observer_mode: $observer_mode:expr,
        last_cash_flow: $last_cash_flow:expr,
        last_resource_flow: $last_resource_flow:expr,
        pending_ai_cash_spending: $pending_ai_cash_spending:expr,
        pending_ai_cash_income: $pending_ai_cash_income:expr,
        next_unit_id: $next_unit_id:expr
        $(, market_state: $market_state:expr )? $(,)?
    ) => {
        $crate::game_state::GameState {
            turn: $turn,
            difficulty: $difficulty,
            human_player_nation: $human_player_nation,
            ai_debug: $ai_debug,
            observer_mode: $observer_mode,
            next_unit_id: $next_unit_id,
            game_data: $game_data,
            world: $crate::game_state::WorldState {
                map_key: $map_key,
                hex_map: $hex_map,
                provinces: $provinces,
                nations: $nations,
                diplomacy: $diplomacy,
                market_state: $crate::test_game_state!(@market_state $( $market_state )?),
                sea_zones: Vec::new(),
            },
            archive: $crate::game_state::GameArchive {
                history: $history,
                high_scores: $high_scores,
                newspaper_archive: $newspaper_archive,
                battle_archive: $battle_archive,
                political_archive: $political_archive,
                market_archive: Vec::new(),
            },
            transient: $crate::game_state::TransientState {
                events: $events,
                pending_attacks: $pending_attacks,
                pending_moves: $pending_moves,
                pending_landings: $pending_landings,
                pending_ai_cash_spending: $pending_ai_cash_spending,
                pending_ai_cash_income: $pending_ai_cash_income,
                pending_economy_orders: std::collections::HashMap::new(),
                last_cash_flow: $last_cash_flow,
                last_resource_flow: $last_resource_flow,
                pending_ai_material_outflows: Vec::new(),
                pending_ai_goods_outflows: Vec::new(),
                pending_ai_material_inflows: Vec::new(),
            },
        }
    };
    (@market_state $market_state:expr) => { $market_state };
    (@market_state) => { $crate::economy::market::MarketState::new() };
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
