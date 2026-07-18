pub(crate) mod civilian_phase;
pub(crate) mod diplomacy_phase;
pub mod economy_phase;
pub(crate) mod news_phase;
pub mod processor;
pub(crate) mod rewards_phase;
pub mod scoring;
pub mod session;
pub(crate) mod trade_phase;

pub use processor::{
    TurnReport, accept_pact_defense, accept_request_to_join_empire, begin_turn,
    connected_provinces, continue_pact_defense_cascade, finish_turn, process_turn,
    projected_immigration_queue_capacity, reject_request_to_join_empire,
};
pub use scoring::{
    CouncilVoteResult, GovernorVoteDetail, NationScore, calculate_score, governor_vote,
    run_council_vote,
};
pub use session::{DiploSessionEvent, SessionOffer, TurnSession};
