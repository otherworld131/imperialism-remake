//! Civilian/settlement phase of the turn pipeline.
//!
//! Tracks province connectivity to the capital and the first-production
//! delay countdown (Imp1 6-turn hamlet ramp). The Hamlet → Village → Town
//! promotion itself now happens in `resolve_town_production` based on
//! actual per-turn output, not on a separate fixed timer.

use crate::game_state::GameState;
use crate::turn::processor::{TurnReport, connected_provinces};
use crate::types::*;

pub(super) fn update_province_connectivity(game: &mut GameState) {
    let nation_ids: Vec<NationId> = game
        .world
        .nations
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

pub(super) fn update_settlements(game: &mut GameState, _report: &mut TurnReport) {
    let delay_turns = game.game_data.game_config.town_first_production_delay_turns;
    let province_data: Vec<(ProvinceId, NationId)> = game
        .world
        .provinces
        .iter()
        .map(|p| (p.id, p.owner))
        .collect();

    for (province_id, owner_id) in &province_data {
        let province = match game.world.provinces.iter().find(|p| p.id == *province_id) {
            Some(p) => p,
            None => continue,
        };

        // Skip settlement progression for Minor Nation provinces or anarchic nations.
        let owner_nation = game.world.nations.iter().find(|n| n.id == *owner_id);
        let skip = owner_nation
            .map(|n| !n.is_great_power() || n.diplomacy.is_in_anarchy)
            .unwrap_or(false);
        if skip {
            continue;
        }

        // The national capital province never industrializes (Imp1: home
        // capital uses the player's manual factories instead of contributing
        // free town output). Treat as a permanent Hamlet.
        let is_country_capital_province = province
            .tiles
            .iter()
            .any(|c| game.world.hex_map.get_tile(*c).is_some_and(|t| t.is_country_capital));
        if is_country_capital_province {
            continue;
        }

        if !province.connected_to_capital {
            continue;
        }

        match province.industrialization_turns_remaining {
            None if !province.town_production_unlocked => {
                // First turn connected and not yet ramping — start the delay.
                let prov = game
                    .world
                    .provinces
                    .iter_mut()
                    .find(|p| p.id == *province_id)
                    .unwrap();
                if delay_turns == 0 {
                    prov.town_production_unlocked = true;
                } else {
                    prov.industrialization_turns_remaining = Some(delay_turns);
                }
            }
            None => {
                // Already unlocked; per-turn `project_town_outputs` and the
                // promotion logic in `resolve_town_production` handle the
                // rest. Nothing to do here.
            }
            Some(remaining) => {
                let next = remaining.saturating_sub(1);
                let prov = game
                    .world
                    .provinces
                    .iter_mut()
                    .find(|p| p.id == *province_id)
                    .unwrap();
                if next == 0 {
                    prov.industrialization_turns_remaining = None;
                    prov.town_production_unlocked = true;
                } else {
                    prov.industrialization_turns_remaining = Some(next);
                }
            }
        }
    }
}
