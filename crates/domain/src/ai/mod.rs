pub mod assessment;
pub mod common;
mod diplomacy;
pub(crate) mod economy;
mod labor;
pub(crate) mod lua_bridge;
mod military;
pub(crate) mod naval;
mod research;
pub mod snapshot;
mod spending;
mod tactical;
mod transport;

pub use common::{AiPersonality, personality_for_nation_index};
pub use diplomacy::{ai_manage_diplomacy, ai_pre_election_strategy, minor_nation_bonus_trade};
pub use naval::ai_naval_strategy;
pub use snapshot::NationEconomySnapshot;
pub use spending::{SpendingCategory, pick_priority_minor_targets, priority_target_count};
pub use tactical::ai_tactical_decisions;

use crate::game_state::GameState;
use crate::types::*;

/// An AI action with its human-readable rationale.
///
/// When `is_non_action` is `true`, this represents a decision the AI *declined*
/// to make (e.g., considered but did not propose an alliance). Non-actions are
/// hidden from the newspaper by default and revealed only via a debug toggle.
#[derive(Debug, Clone)]
pub struct AiAction {
    pub text: String,
    pub reason: String,
    pub is_non_action: bool,
    pub nation_id: crate::types::NationId,
}

/// Run AI decisions for all non-human Great Powers.
///
/// Returns a list of notable actions taken by AI nations, suitable for
/// inclusion in the newspaper / turn report.
pub fn run_ai_turns(game: &mut GameState) -> Vec<AiAction> {
    let human_id = game.human_player_nation;
    let current_year = game.turn.year();

    // Collect AI nation IDs. In observer mode, all 7 Great Powers are AI-controlled
    // (the human seat is just a viewpoint; it also gets an AI personality at setup).
    let ai_nation_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| {
            (game.observer_mode || n.id != human_id)
                && n.is_great_power()
                && !n.diplomacy.is_in_anarchy
        })
        .map(|n| n.id)
        .collect();

    let mut actions: Vec<AiAction> = Vec::new();

    // Shuffle AI nation processing order to prevent first-mover advantage
    let mut ai_ids = ai_nation_ids.clone();
    let turn_seed = game.turn.0 as usize;
    for i in (1..ai_ids.len()).rev() {
        let j = (turn_seed.wrapping_mul(i + 7)) % (i + 1);
        ai_ids.swap(i, j);
    }

    if game.ai_debug {
        eprintln!(
            "--- AI Debug: Turn {} ({} Q{}) ---",
            game.turn.0,
            game.turn.year(),
            game.turn.quarter()
        );
    }

    // War declarations run first so that ai_military_strategy (below) can
    // immediately pick up new wars and apply the rest_health_threshold filter
    // when choosing whether to queue a same-turn attack.
    military::ai_declare_wars(game, &ai_ids, &mut actions);

    for nation_id in &ai_ids {
        if game.ai_debug {
            let name = game
                .get_nation(*nation_id)
                .map(|n| n.name.as_str())
                .unwrap_or("?");
            let personality = common::get_personality(game, *nation_id);
            eprintln!("[AI:{}] Processing (personality={})", name, personality);
        }
        research::ai_research_tech(game, *nation_id, current_year, &mut actions);
        economy::ai_manage_economy(game, *nation_id);
        // Card [2/6]: set per-chain output targets so labor & feed allocation
        // favor chains the AI can actually run, instead of leaving every
        // slider at u32::MAX (which spreads labor evenly and starves
        // resource-light chains like steel).
        economy::ai_set_production_targets(game, *nation_id);
        labor::ai_recruit_workers(game, *nation_id);
        // Need-based spending: replaces independent military, infrastructure,
        // consulate, embassy, and civilian hiring decisions
        spending::ai_scored_spending(game, *nation_id, &mut actions);
        spending::ai_diplomatic_mop_up(game, *nation_id);
        labor::ai_deploy_civilians(game, *nation_id);
        economy::ai_build_transport_proactive(game, *nation_id);
        // AI freight allocation: cover worker food + every active mill / cannery's
        // raw inputs. Without this the transport system delivers nothing remote
        // (cards [1/6], Trello).
        transport::ai_allocate_transport(game, *nation_id);
        diplomacy::ai_manage_diplomacy(game, *nation_id, &mut actions);
        diplomacy::ai_pre_election_strategy(game, *nation_id, &mut actions);
        naval::ai_build_merchant_ships(game, *nation_id);
        // Warship construction now flows through `ai_scored_spending` (card
        // #112) — the scored rotation picks Warship when backlog + need push
        // it above other spending categories. `ai_naval_strategy` below still
        // adds one extra build when the AI is outmatched at sea mid-war.
        naval::ai_naval_strategy(game, *nation_id, &mut actions);
        military::ai_military_strategy(game, *nation_id, &mut actions);
        tactical::ai_tactical_decisions(game, *nation_id, &mut actions);
        labor::ai_train_and_promote_workers(game, *nation_id);
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::TechId;
    use crate::events::TreatyType;
    use crate::hex::HexCoord;
    use crate::map::Province;
    use crate::nation::{Nation, NationColor};
    use common::test_helpers::test_game_with_ai;

    #[test]
    fn ai_does_not_touch_human_player() {
        let mut game = test_game_with_ai();
        let human = game.get_nation_mut(NationId(1)).unwrap();
        let original_treasury = human.economy.treasury;
        let original_techs = human.researched_techs.len();

        run_ai_turns(&mut game);

        let human = game.get_nation(NationId(1)).unwrap();
        assert_eq!(
            human.economy.treasury, original_treasury,
            "Human player should not be affected by AI turns"
        );
        assert_eq!(
            human.researched_techs.len(),
            original_techs,
            "Human player techs should not change"
        );
    }

    #[test]
    fn ai_researches_cheapest_available_tech() {
        let mut game = test_game_with_ai();
        // At 1815, two free techs are available (cost $0):
        // "High Pressure Steam Engine" (ID 1) and "Seed Drill" (ID 2)
        run_ai_turns(&mut game);

        let ai = game.get_nation(NationId(2)).unwrap();
        // Should have researched at least one of the two free techs
        assert!(
            ai.has_researched(TechId(1)) || ai.has_researched(TechId(2)),
            "AI should research a free tech"
        );
        // Note: this test no longer checks treasury delta. After the
        // turn-1-spending fix, an AI on a fresh game with no rival armies
        // and no warm embassy candidates may legitimately spend nothing
        // on turn 1 — research is free, and that's what this test cares
        // about.
    }

    #[test]
    fn queued_separate_peace_hides_ally_from_get_allies_until_finalization() {
        let mut game = test_game_with_ai();

        let province3 = Province::new(
            ProvinceId(3),
            "Ally Land".to_string(),
            NationId(3),
            HexCoord::new(6, 6),
            vec![HexCoord::new(6, 6)],
            2,
        );
        game.world.provinces.push(province3);

        let mut ally = Nation::new(
            NationId(3),
            "AllyNation".to_string(),
            NationColor::Gray,
            NationType::GreatPower,
            ProvinceId(3),
        );
        ally.economy.treasury = Money::dollars(10000);
        ally.diplomacy.ai_personality = Some(AiPersonality::Balanced);
        game.world.nations.push(ally);

        game.world
            .diplomacy
            .initialize_great_powers(&[NationId(1), NationId(2), NationId(3)]);
        game.world
            .diplomacy
            .propose_alliance(NationId(2), NationId(3))
            .unwrap();
        game.world.diplomacy.declare_war(NationId(2), NationId(1));
        game.world.diplomacy.declare_war(NationId(3), NationId(1));
        game.world.diplomacy.queue_peace(NationId(2), NationId(1));

        assert!(
            !game
                .world
                .diplomacy
                .get_allies(NationId(2))
                .contains(&NationId(3)),
            "queued separate peace should hide suspended allies from AI war planning"
        );
        assert!(
            game.world
                .diplomacy
                .has_treaty(NationId(2), NationId(3), TreatyType::Alliance),
            "the alliance treaty should remain active until same-turn reconciliation finalizes it"
        );
    }

    #[test]
    fn build_scripts_exist_and_are_executable() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let scripts = [
            "scripts/build.sh",
            "scripts/test.sh",
            "scripts/check.sh",
            "scripts/pre-commit",
        ];

        // Find the workspace root by going up from the crate dir
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap();

        for script in &scripts {
            let path = workspace_root.join(script);
            assert!(
                path.exists(),
                "Script {} should exist at {:?}",
                script,
                path
            );

            let metadata = fs::metadata(&path).unwrap();
            let mode = metadata.permissions().mode();
            assert!(
                mode & 0o111 != 0,
                "Script {} should be executable (mode: {:o})",
                script,
                mode
            );
        }
    }
}
