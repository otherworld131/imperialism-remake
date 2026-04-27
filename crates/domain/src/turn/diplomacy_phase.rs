use crate::events::{DomainEvent, Headline, HeadlineCategory, HistoryEvent, TreatyType};
use crate::game_state::GameState;
use crate::turn::processor::TurnReport;
use crate::types::*;

/// Generate newspaper headlines for the turn report.
///
/// Gathers notable events from the turn: AI actions (tech research, military
/// buildup, war declarations), trade activity, and adds period-appropriate
/// flavor headlines that rotate based on the turn number.
/// Check for alliance obligations when nations are at war.
/// Resolve all pending diplomatic proposals from the turn.
///
/// Build a short, human-readable reason for an AI treaty-evaluation outcome,
/// drawing on the evaluator's core signals (personality, relation score, war
/// status, diplomatic infrastructure).
fn diplomacy_reason(
    game: &GameState,
    evaluator: NationId,
    counterpart: NationId,
    treaty_label: &str,
    accepted: bool,
) -> String {
    let personality = crate::ai::common::get_personality(game, evaluator);
    let (score, at_war, has_embassy, has_consulate) = game
        .world.diplomacy
        .get_relation(evaluator, counterpart)
        .map(|r| (r.score, r.at_war, r.has_embassy, r.has_consulate))
        .unwrap_or((0, false, false, false));
    let infra = if has_embassy {
        "embassy"
    } else if has_consulate {
        "consulate"
    } else {
        "no diplomatic infrastructure"
    };
    let verdict = if accepted { "accepted" } else { "rejected" };
    format!(
        "{} personality {} {} (relation={}, at_war={}, {})",
        personality, verdict, treaty_label, score, at_war, infra
    )
}

fn separate_peace_reason(
    game: &GameState,
    peacemaker: NationId,
    former_ally: NationId,
    enemy: NationId,
) -> String {
    let peacemaker_name = game
        .get_nation(peacemaker)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let ally_name = game
        .get_nation(former_ally)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let enemy_name = game
        .get_nation(enemy)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    format!(
        "Separate peace: {} ended its war with {} while ally {} remained at war with {}",
        peacemaker_name, enemy_name, ally_name, enemy_name
    )
}

pub(super) fn record_broken_alliance_headlines(
    game: &GameState,
    report: &mut TurnReport,
    broken_alliances: &[crate::diplomacy::relations::BrokenAlliance],
) {
    for broken in broken_alliances {
        let peacemaker_name = game
            .get_nation(broken.peacemaker)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        let ally_name = game
            .get_nation(broken.former_ally)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        report.newspaper_headlines.push(Headline::with_reason(
            format!(
                "{} breaks its alliance with {} after making separate peace",
                peacemaker_name, ally_name
            ),
            HeadlineCategory::Diplomacy,
            separate_peace_reason(game, broken.peacemaker, broken.former_ally, broken.enemy),
        ).for_nations(&[broken.peacemaker, broken.former_ally]));
    }
}

/// - Mutual peace proposals (both sides proposed): auto-accept.
/// - Player→AI proposals: evaluate using AI assessment logic.
/// - AI→Human proposals: keep pending for UI modal.
/// - AI→AI proposals: evaluate using AI assessment logic (alliance, NAP, peace).
///
/// Also expires stale proposals older than 4 turns.
pub(super) fn resolve_diplomatic_proposals(game: &mut GameState, report: &mut TurnReport) {
    let proposals: Vec<_> = game
        .world.diplomacy
        .drain_proposals()
        .into_iter()
        .filter(|p| {
            !game.get_nation(p.from).is_some_and(|n| n.diplomacy.is_in_anarchy)
                && !game.get_nation(p.to).is_some_and(|n| n.diplomacy.is_in_anarchy)
        })
        .collect();
    if proposals.is_empty() {
        return;
    }

    // Detect mutual peace proposals (both sides proposed peace)
    let mut mutual_peace: Vec<(NationId, NationId)> = Vec::new();
    for (i, p1) in proposals.iter().enumerate() {
        if p1.proposal_type != TreatyType::PeaceTreaty {
            continue;
        }
        for p2 in &proposals[i + 1..] {
            if p2.proposal_type == TreatyType::PeaceTreaty && p1.from == p2.to && p1.to == p2.from {
                mutual_peace.push((p1.from, p1.to));
            }
        }
    }

    // Apply mutual peace immediately
    for &(a, b) in &mutual_peace {
        if game.world.diplomacy.is_at_war(a, b) {
            game.world.diplomacy.queue_peace(a, b);
            report
                .events
                .push(DomainEvent::TreatyAccepted(crate::events::TreatyAccepted {
                    from: a,
                    to: b,
                    treaty_type: TreatyType::PeaceTreaty,
                }));
            let turn = game.turn;
            game.archive.history.push((turn, HistoryEvent::MutualPeace { a, b }));
        }
    }

    // Evaluate player→AI proposals using AI assessment logic,
    // and re-add AI→human proposals for UI handling.
    let human = game.human_player_nation;
    for proposal in proposals {
        // Skip proposals that were part of mutual peace
        let was_mutual = mutual_peace.iter().any(|&(a, b)| {
            (proposal.from == a && proposal.to == b) || (proposal.from == b && proposal.to == a)
        });
        if was_mutual {
            continue;
        }

        if proposal.from == human && proposal.to != human {
            // Player→AI: evaluate using AI assessment
            let target_id = proposal.to;
            let personality = crate::ai::common::get_personality(game, target_id);

            #[cfg(feature = "lua")]
            let lua_cfg = game
                .game_data
                .lua_engine
                .as_ref()
                .and_then(|e| crate::ai::lua_bridge::lua_get_config(e, personality));

            let accepted = match proposal.proposal_type {
                TreatyType::NonAggressionPact => crate::ai::assessment::evaluate_nap_proposal(
                    game,
                    human,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                TreatyType::Alliance => crate::ai::assessment::evaluate_alliance_proposal(
                    game,
                    human,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                TreatyType::PeaceTreaty => crate::ai::assessment::evaluate_peace_proposal(
                    game,
                    human,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                _ => false,
            };

            let from_name = game
                .get_nation(human)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let to_name = game
                .get_nation(target_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let treaty_label = match proposal.proposal_type {
                TreatyType::NonAggressionPact => "Non-Aggression Pact",
                TreatyType::Alliance => "Alliance",
                TreatyType::PeaceTreaty => "Peace Treaty",
                _ => "Treaty",
            };

            if accepted {
                // Apply the treaty — check result in case state drifted
                let applied = match proposal.proposal_type {
                    TreatyType::NonAggressionPact => {
                        game.world.diplomacy.propose_pact(human, target_id).is_ok()
                    }
                    TreatyType::Alliance => {
                        game.world.diplomacy.propose_alliance(human, target_id).is_ok()
                    }
                    TreatyType::PeaceTreaty => {
                        game.world.diplomacy.queue_peace(human, target_id);
                        true
                    }
                    _ => false,
                };
                if applied {
                    report.events.push(DomainEvent::TreatyAccepted(
                        crate::events::TreatyAccepted {
                            from: human,
                            to: target_id,
                            treaty_type: proposal.proposal_type,
                        },
                    ));
                    report.newspaper_headlines.push(Headline::with_reason(
                        format!(
                            "{} accepts {}'s {} proposal",
                            to_name, from_name, treaty_label
                        ),
                        HeadlineCategory::Diplomacy,
                        diplomacy_reason(game, target_id, human, treaty_label, true),
                    ).for_nations(&[target_id, human]));
                    let turn = game.turn;
                    game.archive.history.push((
                        turn,
                        HistoryEvent::TreatyProposalAccepted {
                            acceptor: target_id,
                            proposer: human,
                            treaty_type: proposal.proposal_type,
                        },
                    ));
                } else {
                    // AI accepted but treaty could not be applied (state drift)
                    report.events.push(DomainEvent::TreatyRejected(
                        crate::events::TreatyRejected {
                            from: human,
                            to: target_id,
                            treaty_type: proposal.proposal_type,
                        },
                    ));
                    report.newspaper_headlines.push(Headline::with_reason(
                        format!(
                            "{} proposal to {} could not be fulfilled",
                            treaty_label, to_name
                        ),
                        HeadlineCategory::Diplomacy,
                        "AI accepted but state drifted (counterpart relation changed mid-turn); treaty could not be applied".to_string(),
                    ).for_nations(&[target_id, human]));
                }
            } else {
                report
                    .events
                    .push(DomainEvent::TreatyRejected(crate::events::TreatyRejected {
                        from: human,
                        to: target_id,
                        treaty_type: proposal.proposal_type,
                    }));
                report.newspaper_headlines.push(Headline::with_reason(
                    format!(
                        "{} rejects {}'s {} proposal",
                        to_name, from_name, treaty_label
                    ),
                    HeadlineCategory::Diplomacy,
                    diplomacy_reason(game, target_id, human, treaty_label, false),
                ).for_nations(&[target_id, human]));
            }
        } else if proposal.to == human {
            // AI→human: keep pending for UI
            game.world.diplomacy.pending_proposals.push(proposal);
        } else {
            // AI→AI: evaluate the proposal at end of turn
            let from_id = proposal.from;
            let target_id = proposal.to;
            let personality = crate::ai::common::get_personality(game, target_id);

            #[cfg(feature = "lua")]
            let lua_cfg = game
                .game_data
                .lua_engine
                .as_ref()
                .and_then(|e| crate::ai::lua_bridge::lua_get_config(e, personality));

            let accepted = match proposal.proposal_type {
                TreatyType::NonAggressionPact => crate::ai::assessment::evaluate_nap_proposal(
                    game,
                    from_id,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                TreatyType::Alliance => crate::ai::assessment::evaluate_alliance_proposal(
                    game,
                    from_id,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                TreatyType::PeaceTreaty => crate::ai::assessment::evaluate_peace_proposal(
                    game,
                    from_id,
                    target_id,
                    personality,
                    #[cfg(feature = "lua")]
                    lua_cfg.as_ref(),
                ),
                _ => false,
            };

            let from_name = game
                .get_nation(from_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let to_name = game
                .get_nation(target_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let treaty_label = match proposal.proposal_type {
                TreatyType::NonAggressionPact => "Non-Aggression Pact",
                TreatyType::Alliance => "Alliance",
                TreatyType::PeaceTreaty => "Peace Treaty",
                _ => "Treaty",
            };

            if accepted {
                let applied = match proposal.proposal_type {
                    TreatyType::NonAggressionPact => {
                        game.world.diplomacy.propose_pact(from_id, target_id).is_ok()
                    }
                    TreatyType::Alliance => {
                        game.world.diplomacy.propose_alliance(from_id, target_id).is_ok()
                    }
                    TreatyType::PeaceTreaty => {
                        game.world.diplomacy.queue_peace(from_id, target_id);
                        true
                    }
                    _ => false,
                };
                if applied {
                    report.events.push(DomainEvent::TreatyAccepted(
                        crate::events::TreatyAccepted {
                            from: from_id,
                            to: target_id,
                            treaty_type: proposal.proposal_type,
                        },
                    ));
                    report.newspaper_headlines.push(Headline::with_reason(
                        format!(
                            "{} accepts {}'s {} proposal",
                            to_name, from_name, treaty_label
                        ),
                        HeadlineCategory::Diplomacy,
                        diplomacy_reason(game, target_id, from_id, treaty_label, true),
                    ).for_nations(&[target_id, from_id]));
                    let turn = game.turn;
                    game.archive.history.push((
                        turn,
                        HistoryEvent::TreatyProposalAccepted {
                            acceptor: target_id,
                            proposer: from_id,
                            treaty_type: proposal.proposal_type,
                        },
                    ));
                } else {
                    // AI accepted but treaty could not be applied (state drift)
                    report.events.push(DomainEvent::TreatyRejected(
                        crate::events::TreatyRejected {
                            from: from_id,
                            to: target_id,
                            treaty_type: proposal.proposal_type,
                        },
                    ));
                    report.newspaper_headlines.push(Headline::with_reason(
                        format!(
                            "{} proposal to {} could not be fulfilled",
                            treaty_label, to_name
                        ),
                        HeadlineCategory::Diplomacy,
                        "AI accepted but state drifted (counterpart relation changed mid-turn); treaty could not be applied".to_string(),
                    ).for_nations(&[target_id, from_id]));
                }
            } else {
                report
                    .events
                    .push(DomainEvent::TreatyRejected(crate::events::TreatyRejected {
                        from: from_id,
                        to: target_id,
                        treaty_type: proposal.proposal_type,
                    }));
                report.newspaper_headlines.push(Headline::with_reason(
                    format!(
                        "{} rejects {}'s {} proposal",
                        to_name, from_name, treaty_label
                    ),
                    HeadlineCategory::Diplomacy,
                    diplomacy_reason(game, target_id, from_id, treaty_label, false),
                ).for_nations(&[target_id, from_id]));
            }
        }
    }

    // Expire proposals older than 4 turns
    game.world.diplomacy.expire_proposals(game.turn, 4);
}
