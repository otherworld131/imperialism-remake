pub mod processor;
pub mod scoring;

pub use processor::{TurnReport, connected_provinces, process_turn};
pub use scoring::{
    CouncilVoteResult, GovernorVoteDetail, NationScore, calculate_score, governor_vote,
    run_council_vote,
};
