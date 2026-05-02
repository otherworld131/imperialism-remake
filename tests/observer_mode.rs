//! Observer mode: all 7 Great Powers play as AI; the "human" seat is just a viewpoint.

use domain::ai::run_ai_turns;
use domain::game_state::{new_game, new_observer_game};
use domain::turn::process_turn;
use domain::types::Difficulty;

#[test]
fn observer_game_assigns_personality_to_all_seven_gps() {
    let game = new_observer_game("obs_test_a", Difficulty::Normal);
    assert!(game.observer_mode);
    let gps = game.great_powers();
    assert_eq!(gps.len(), 7);
    for nation in &gps {
        assert!(
            nation.diplomacy.ai_personality.is_some(),
            "nation {} missing AI personality in observer mode",
            nation.name
        );
    }
}

#[test]
fn observer_mode_run_ai_turns_processes_human_seat() {
    let mut game = new_observer_game("obs_test_b", Difficulty::Normal);
    let human_id = game.human_player_nation;
    let treasury_before = game
        .get_nation(human_id)
        .expect("human nation exists")
        .economy
        .treasury;

    // Advance a few turns so the AI has chances to act for the human seat.
    for _ in 0..3 {
        run_ai_turns(&mut game);
        process_turn(&mut game);
    }

    let treasury_after = game
        .get_nation(human_id)
        .expect("human nation still exists")
        .economy
        .treasury;
    // Any change (income, spending, research, hiring) proves the AI is driving the seat.
    assert_ne!(
        treasury_before, treasury_after,
        "human seat treasury did not change; AI is not running for it"
    );
}

#[test]
fn observer_mode_bonus_applies_to_all_gps_on_hard() {
    let game = new_observer_game("obs_test_c", Difficulty::Hard);
    let treasuries: Vec<_> = game
        .great_powers()
        .iter()
        .map(|n| n.economy.treasury.as_dollars())
        .collect();
    // On Hard, every GP (including the observer's viewpoint) gets a +$1000 bonus.
    // Base Hard starting cash is $8000 → each GP should be at $9000.
    for t in &treasuries {
        assert_eq!(*t, 9000, "all GPs should have the Hard bonus, got {}", t);
    }
}

#[test]
fn non_observer_human_keeps_no_bonus_on_hard() {
    let game = new_game("obs_test_d", Difficulty::Hard, 0);
    assert!(!game.observer_mode);
    let human_id = game.human_player_nation;
    let human = game.get_nation(human_id).unwrap();
    assert_eq!(human.economy.treasury.as_dollars(), 8000);
    // And AI GPs have the bonus.
    for nation in game.great_powers().iter().filter(|n| n.id != human_id) {
        assert_eq!(nation.economy.treasury.as_dollars(), 9000);
    }
}
