pub(crate) mod civilian_phase;
pub(crate) mod economy_phase;
pub(crate) mod news_phase;
pub mod processor;
pub mod scoring;

pub use processor::{
    TurnReport, accept_pact_defense, connected_provinces, continue_pact_defense_cascade,
    process_turn,
};
pub use scoring::{
    CouncilVoteResult, GovernorVoteDetail, NationScore, calculate_score, governor_vote,
    run_council_vote,
};
