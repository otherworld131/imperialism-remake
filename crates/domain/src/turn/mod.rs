pub(crate) mod trade_phase;
pub(crate) mod rewards_phase;
pub(crate) mod diplomacy_phase;
pub(crate) mod civilian_phase;
pub(crate) mod economy_phase;
pub(crate) mod news_phase;
pub mod processor;
pub mod scoring;

pub use processor::{
    TurnReport, accept_pact_defense, accept_request_to_join_empire, connected_provinces,
    continue_pact_defense_cascade, process_turn, reject_request_to_join_empire,
};
pub use scoring::{
    CouncilVoteResult, GovernorVoteDetail, NationScore, calculate_score, governor_vote,
    run_council_vote,
};
