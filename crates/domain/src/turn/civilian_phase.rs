//! Civilian/settlement phase of the turn pipeline.
//!
//! Tracks province connectivity to the capital and the Hamlet → Village →
//! Town progression. The heavier `resolve_civilian_actions` (engineer work,
//! prospector reveals, farmer improvements) remains in `processor.rs` for
//! a follow-up PR; this module owns the post-combat connectivity sweep
//! and settlement upgrade headlines.

use crate::events::{Headline, HeadlineCategory};
use crate::game_state::GameState;
use crate::map::SettlementLevel;
use crate::turn::processor::{TurnReport, connected_provinces};
use crate::types::*;

pub(super) fn update_province_connectivity(game: &mut GameState) {
    let nation_ids: Vec<NationId> = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power() && !n.diplomacy.is_in_anarchy)
        .map(|n| n.id)
        .collect();

    for nation_id in nation_ids {
        let connected = connected_provinces(game, nation_id);
        for prov in game.world.provinces.iter_mut() {
            // Only upgrade connectivity (false → true), never downgrade.
            // Full disconnection tracking will be added with the transport system.
            if prov.owner == nation_id && connected.contains(&prov.id) {
                prov.connected_to_capital = true;
            }
        }
    }
}

pub(super) fn update_settlements(game: &mut GameState, report: &mut TurnReport) {
    // Collect province IDs and their owner for processing
    let province_data: Vec<(ProvinceId, NationId)> =
        game.world.provinces.iter().map(|p| (p.id, p.owner)).collect();

    for (province_id, owner_id) in &province_data {
        let province = match game.world.provinces.iter().find(|p| p.id == *province_id) {
            Some(p) => p,
            None => continue,
        };

        let owner_nation = game.world.nations.iter().find(|n| n.id == *owner_id);

        // Skip settlement progression for Minor Nation provinces or anarchic nations
        let skip = owner_nation
            .map(|n| !n.is_great_power() || n.diplomacy.is_in_anarchy)
            .unwrap_or(false);
        if skip {
            continue;
        }

        if province.connected_to_capital {
            let mut just_became_village = false;

            match province.industrialization_turns_remaining {
                None => {
                    // Just connected or already industrialized; if still Hamlet, start countdown
                    if province.settlement_level == SettlementLevel::Hamlet {
                        let prov = game
                            .world.provinces
                            .iter_mut()
                            .find(|p| p.id == *province_id)
                            .unwrap();
                        prov.industrialization_turns_remaining = Some(6);
                    }
                }
                Some(remaining) => {
                    if remaining <= 1 {
                        // Countdown complete: upgrade settlement
                        let prov = game
                            .world.provinces
                            .iter_mut()
                            .find(|p| p.id == *province_id)
                            .unwrap();

                        if prov.settlement_level == SettlementLevel::Hamlet {
                            prov.settlement_level = SettlementLevel::Village;
                            prov.industrialization_turns_remaining = None;
                            // Start the Town countdown (12 turns)
                            prov.town_countdown = Some(12);
                            just_became_village = true;

                            let headline = format!("{} has grown into a Village!", prov.name);
                            report
                                .newspaper_headlines
                                .push(Headline::new(headline.clone(), HeadlineCategory::Growth).for_nation(*owner_id));
                            report
                                .settlement_upgrades
                                .push((*province_id, "Village".to_string()));
                        } else {
                            prov.industrialization_turns_remaining = None;
                        }
                    } else {
                        // Tick down
                        let prov = game
                            .world.provinces
                            .iter_mut()
                            .find(|p| p.id == *province_id)
                            .unwrap();
                        prov.industrialization_turns_remaining = Some(remaining - 1);
                    }
                }
            }

            // Village → Town progression: tick down the town_countdown
            // Skip if the province just became a Village this turn
            if !just_became_village {
                let prov_level = game
                    .world.provinces
                    .iter()
                    .find(|p| p.id == *province_id)
                    .map(|p| (p.settlement_level, p.town_countdown));

                if let Some((SettlementLevel::Village, Some(remaining))) = prov_level {
                    if remaining <= 1 {
                        let prov = game
                            .world.provinces
                            .iter_mut()
                            .find(|p| p.id == *province_id)
                            .unwrap();
                        prov.settlement_level = SettlementLevel::Town;
                        prov.town_countdown = None;

                        let headline = format!("{} has grown into a Town!", prov.name);
                        report
                            .newspaper_headlines
                            .push(Headline::new(headline.clone(), HeadlineCategory::Growth).for_nation(*owner_id));
                        report
                            .settlement_upgrades
                            .push((*province_id, "Town".to_string()));
                    } else {
                        let prov = game
                            .world.provinces
                            .iter_mut()
                            .find(|p| p.id == *province_id)
                            .unwrap();
                        prov.town_countdown = Some(remaining - 1);
                    }
                }
            }
        }
    }
}
