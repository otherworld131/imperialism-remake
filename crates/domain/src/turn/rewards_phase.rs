use crate::events::{Headline, HeadlineCategory};
use crate::game_state::GameState;
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::turn::processor::TurnReport;
use crate::types::*;

/// Resolve rewards: Generals earned from arms buildup, Admirals earned from Ship-of-the-Line
/// buildup, free Clippers for first colony, capitol expansion from GP capital conquest.
pub(super) fn resolve_rewards(game: &mut GameState, report: &mut TurnReport) {
    use crate::map::UnitId;
    use crate::military::ships::{Ship, ShipType};

    let nation_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    for nation_id in &nation_ids {
        let nation = match game.world.nations.iter().find(|n| n.id == *nation_id) {
            Some(n) => n,
            None => continue,
        };

        // Calculate total arms built. Militia and Generals themselves do not
        // count toward the General-reward threshold — Militia is raised from
        // provinces rather than built from arms, and Generals are the reward,
        // not the input. (Trello card #128.)
        let total_arms: u32 = nation
            .military
            .army
            .iter()
            .filter(|u| !matches!(u.unit_type, ArmyUnitType::Minutemen | ArmyUnitType::General))
            .map(|u| u.unit_type.stats().arms_required)
            .sum();

        // Update the tracked total
        let current_total = total_arms.max(nation.military.total_arms_built);

        // General thresholds: 6, 12, 20, 30, ...
        // The nth general is earned at: 6, 12, 20, 30 (6 + 6 + 8 + 10...)
        // Simplified: thresholds are 6, 12, 20, 30, 42, 56, ...
        let general_thresholds = [6u32, 12, 20, 30, 42, 56, 72, 90];
        let generals_earned_now = nation.military.generals_earned;

        let mut new_generals = 0u32;
        for (i, threshold) in general_thresholds.iter().enumerate() {
            if i as u32 >= generals_earned_now && current_total >= *threshold {
                new_generals += 1;
            }
        }

        if new_generals > 0 || current_total != nation.military.total_arms_built {
            let nation = match game.world.nations.iter_mut().find(|n| n.id == *nation_id) {
                Some(n) => n,
                None => continue,
            };
            nation.military.total_arms_built = current_total;

            for _ in 0..new_generals {
                nation.military.generals_earned += 1;
                let gen_id =
                    UnitId(3_000_000 + nation.id.0 * 100 + nation.military.generals_earned);
                let general_unit = ArmyUnit::new(
                    gen_id,
                    ArmyUnitType::General,
                    *nation_id,
                    nation.capital_province_id,
                );
                nation.military.army.push(general_unit);

                let nation_name = nation.name.clone();
                report
                    .rewards_earned
                    .push((*nation_id, format!("{} has earned a General!", nation_name)));
                report.newspaper_headlines.push(
                    Headline::new(
                        format!("{} has earned a General!", nation_name),
                        HeadlineCategory::Military,
                    )
                    .for_nation(*nation_id),
                );
            }
        }
    }

    // Admiral reward: track Ships-of-the-Line built per nation.
    // When count >= 5 (and then every 5 more): earn an Admiral (free bonus Ship-of-the-Line).
    for nation_id in &nation_ids {
        let nation = match game.world.nations.iter().find(|n| n.id == *nation_id) {
            Some(n) => n,
            None => continue,
        };

        // Count Ships-of-the-Line in warship fleet
        let sol_count: u32 = nation
            .military
            .warships
            .iter()
            .filter(|s| s.ship_type == ShipType::ShipOfTheLine)
            .count() as u32;

        let current_sol = sol_count.max(nation.military.total_ships_of_the_line_built);
        let admirals_earned_now = nation.military.admirals_earned;

        // Admiral thresholds: every 5 Ships-of-the-Line (5, 10, 15, ...)
        let mut new_admirals = 0u32;
        let mut threshold = 5u32;
        let mut idx = 0u32;
        while threshold <= current_sol {
            if idx >= admirals_earned_now {
                new_admirals += 1;
            }
            idx += 1;
            threshold += 5;
        }

        if new_admirals > 0 || current_sol != nation.military.total_ships_of_the_line_built {
            let bonus_hull = game.game_data.ship_stats(ShipType::ShipOfTheLine).hull;
            let nation = match game.world.nations.iter_mut().find(|n| n.id == *nation_id) {
                Some(n) => n,
                None => continue,
            };
            nation.military.total_ships_of_the_line_built = current_sol;

            for _ in 0..new_admirals {
                nation.military.admirals_earned += 1;
                // Award a free Ship-of-the-Line as the Admiral bonus warship
                let ship_id =
                    UnitId(4_000_000 + nation.id.0 * 100 + nation.military.admirals_earned);
                let bonus_ship =
                    Ship::new(ship_id, ShipType::ShipOfTheLine, *nation_id, bonus_hull);
                nation.military.warships.push(bonus_ship);

                let nation_name = nation.name.clone();
                report.rewards_earned.push((
                    *nation_id,
                    format!("{} has earned an Admiral!", nation_name),
                ));
                report.newspaper_headlines.push(
                    Headline::new(
                        format!("{} has earned an Admiral!", nation_name),
                        HeadlineCategory::Military,
                    )
                    .for_nation(*nation_id),
                );
            }
        }
    }

    // Capitol expansion: check if any GP conquered another GP's capital this turn
    // We detect this by checking battles where the attacker won and the province
    // was a capital of a Great Power.
    let battle_results: Vec<(NationId, ProvinceId)> = report
        .battles
        .iter()
        .filter(|b| b.attacker_won)
        .map(|b| (b.attacker, b.province))
        .collect();

    for (attacker_id, province_id) in battle_results {
        // Check if this province is a capital of any Great Power
        let is_gp_capital = game.world.nations.iter().any(|n| {
            n.is_great_power() && n.capital_province_id == province_id && n.id != attacker_id
        });

        if is_gp_capital
            && let Some(attacker) = game.world.nations.iter_mut().find(|n| n.id == attacker_id)
        {
            attacker.military.capitol_bonus_capacity += 1;
            let attacker_name = attacker.name.clone();
            report.rewards_earned.push((
                attacker_id,
                format!(
                    "{}'s capitol building has expanded from conquering a Great Power's capital!",
                    attacker_name
                ),
            ));
            report.newspaper_headlines.push(
                Headline::new(
                    format!("{}'s capitol building has expanded!", attacker_name),
                    HeadlineCategory::Growth,
                )
                .for_nation(attacker_id),
            );
        }
    }

    // Expert worker reward: at 10 experts -> +1 capitol_bonus_capacity,
    // at 30 experts -> +1 more. Tracked by expert_rewards_earned to prevent duplicates.
    let expert_thresholds: [(u32, u8); 2] = [(10, 1), (30, 2)];
    for nation_id in &nation_ids {
        let nation = match game.world.nations.iter().find(|n| n.id == *nation_id) {
            Some(n) => n,
            None => continue,
        };

        let expert_count = nation.economy.labor.expert;
        let already_earned = nation.military.expert_rewards_earned;

        // Determine how many rewards should have been earned by now
        let mut should_have_earned: u8 = 0;
        for &(threshold, reward_level) in &expert_thresholds {
            if expert_count >= threshold {
                should_have_earned = reward_level;
            }
        }

        if should_have_earned > already_earned {
            let new_rewards = should_have_earned - already_earned;
            let nation = match game.world.nations.iter_mut().find(|n| n.id == *nation_id) {
                Some(n) => n,
                None => continue,
            };
            nation.military.expert_rewards_earned = should_have_earned;
            nation.military.capitol_bonus_capacity += new_rewards as u32;

            let nation_name = nation.name.clone();
            for _ in 0..new_rewards {
                report.rewards_earned.push((
                    *nation_id,
                    format!(
                        "{}'s capitol has expanded from expert workforce development!",
                        nation_name
                    ),
                ));
                report.newspaper_headlines.push(
                    Headline::new(
                        format!(
                            "{}'s expert workforce drives capitol expansion!",
                            nation_name
                        ),
                        HeadlineCategory::Growth,
                    )
                    .for_nation(*nation_id),
                );
            }
        }
    }
}
