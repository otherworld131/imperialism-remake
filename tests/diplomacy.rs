mod test_helpers;

use domain::events::TreatyType;
use domain::game_state::new_game;
use domain::turn::process_turn;
use domain::types::*;

// ── Full diplomatic lifecycle ─────────────────────────────────────

#[test]
fn diplomatic_lifecycle_consulate_to_incorporation() {
    let mut game = new_game("diplo_lifecycle", Difficulty::Normal, 0);
    let player = game.human_player_nation;
    let mn_id = game.minor_nations()[0].id;

    // Step 1: Build consulate
    game.diplomacy.build_consulate(player, mn_id).unwrap();
    assert!(game.diplomacy.has_consulate(player, mn_id));

    // Step 2: Build embassy
    game.diplomacy.build_embassy(player, mn_id).unwrap();
    assert!(game.diplomacy.has_embassy(player, mn_id));

    // Step 3: Propose pact
    game.diplomacy.propose_pact(player, mn_id).unwrap();
    assert!(
        game.diplomacy
            .has_treaty(player, mn_id, TreatyType::NonAggressionPact)
    );

    // Step 4: Boost relationship to 75+ (send grants)
    for _ in 0..20 {
        game.diplomacy
            .send_grant(player, mn_id, Money::dollars(500));
    }

    // Step 5: Process turns until voluntary incorporation happens
    for _ in 0..10 {
        process_turn(&mut game);
    }

    // Check: MN should have incorporated (provinces transferred to player)
    let player_nation = game.get_nation(player).unwrap();
    assert!(
        player_nation.province_count() > 8,
        "Player should have gained provinces from incorporation, but has {}",
        player_nation.province_count()
    );
}

// ── War declaration -> alliance cascade -> peace -> standing ──────

#[test]
fn war_alliance_cascade_peace_standing() {
    let mut game = new_game("war_cascade", Difficulty::Normal, 0);
    let gp_ids: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
    let attacker = gp_ids[1]; // Devron
    let defender = gp_ids[2]; // Haxaco
    let ally = gp_ids[3]; // Kem

    // Form alliance between defender and ally
    game.diplomacy.propose_alliance(defender, ally).unwrap();
    assert!(
        game.diplomacy
            .has_treaty(defender, ally, TreatyType::Alliance)
    );

    // Initial standing
    let initial_standing = game.diplomacy.get_standing(attacker);

    // Declare war — should trigger alliance cascade
    game.diplomacy.declare_war(attacker, defender);
    assert!(game.diplomacy.is_at_war(attacker, defender));

    // Process a turn — alliance should activate (ally joins war)
    process_turn(&mut game);

    // Make peace
    game.diplomacy.make_peace(attacker, defender);
    assert!(!game.diplomacy.is_at_war(attacker, defender));

    // Verify standing was affected by war
    let final_standing = game.diplomacy.get_standing(attacker);
    // Standing should have decreased due to war declaration (breaks treaties)
    assert!(
        final_standing <= initial_standing,
        "Standing should not have increased after war: initial={}, final={}",
        initial_standing,
        final_standing
    );
}

// ── Council vote victory scenario ─────────────────────────────────

#[test]
fn council_vote_victory_scenario() {
    let mut game = new_game("council_victory", Difficulty::Normal, 0);
    let player = game.human_player_nation;

    // Boost relationships with all MNs for governor votes
    let mn_ids: Vec<NationId> = game.minor_nations().iter().map(|n| n.id).collect();
    for mn_id in &mn_ids {
        game.diplomacy.build_consulate(player, *mn_id).ok();
        game.diplomacy.build_embassy(player, *mn_id).ok();
        for _ in 0..30 {
            game.diplomacy
                .send_grant(player, *mn_id, Money::dollars(1000));
        }
    }

    // Advance to a decade election (1825 Q1 = turn 41)
    while !game.turn.is_decade_election() {
        process_turn(&mut game);
    }

    // Process the election turn
    let report = process_turn(&mut game);

    // Check vote results
    assert!(
        report.council_vote.is_some(),
        "Should have council vote on decade election turn"
    );
}

// ── MN responds to grants with higher relationship ────────────────

#[test]
fn minor_nation_responds_to_grants_with_higher_relationship() {
    let mut game = new_game("mn_grants", Difficulty::Normal, 0);
    let player = game.human_player_nation;
    let mn_id = game.minor_nations()[0].id;

    game.diplomacy.build_consulate(player, mn_id).unwrap();
    game.diplomacy.build_embassy(player, mn_id).unwrap();

    let initial_score = game
        .diplomacy
        .get_relation(player, mn_id)
        .map(|r| r.score)
        .unwrap_or(0);

    // Send grants
    for _ in 0..10 {
        game.diplomacy
            .send_grant(player, mn_id, Money::dollars(500));
    }

    let final_score = game
        .diplomacy
        .get_relation(player, mn_id)
        .map(|r| r.score)
        .unwrap_or(0);
    assert!(
        final_score > initial_score,
        "Grants should improve relationship: initial={}, final={}",
        initial_score,
        final_score
    );
}

// ── Standing affects treaty acceptance ────────────────────────────

#[test]
fn low_standing_prevents_treaty_proposals() {
    let mut game = new_game("standing_test", Difficulty::Normal, 0);
    let gp_ids: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
    let proposer = gp_ids[1];
    let target = gp_ids[2];

    // Reduce standing below 30
    game.diplomacy.reduce_standing(proposer, 75);
    assert!(game.diplomacy.get_standing(proposer) < 30);

    // Alliance proposal should fail due to low standing
    let result = game.diplomacy.propose_alliance(proposer, target);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Standing too low to propose treaties");
}

#[test]
fn low_standing_prevents_pact_proposals() {
    let mut game = new_game("pact_standing", Difficulty::Normal, 0);
    let player = game.human_player_nation;
    let mn_id = game.minor_nations()[0].id;

    // Build consulate and embassy first
    game.diplomacy.build_consulate(player, mn_id).unwrap();
    game.diplomacy.build_embassy(player, mn_id).unwrap();

    // Reduce standing below 30
    game.diplomacy.reduce_standing(player, 75);
    assert!(game.diplomacy.get_standing(player) < 30);

    // Pact proposal should fail due to low standing
    let result = game.diplomacy.propose_pact(player, mn_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Standing too low to propose treaties");
}

// ── Convenience method tests ──────────────────────────────────────

#[test]
fn is_at_war_convenience_method() {
    let mut game = new_game("at_war_test", Difficulty::Normal, 0);
    let gp_ids: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
    let a = gp_ids[0];
    let b = gp_ids[1];

    assert!(!game.diplomacy.is_at_war(a, b));
    game.diplomacy.declare_war(a, b);
    assert!(game.diplomacy.is_at_war(a, b));
    // Order-independent
    assert!(game.diplomacy.is_at_war(b, a));

    game.diplomacy.make_peace(a, b);
    assert!(!game.diplomacy.is_at_war(a, b));
}

#[test]
fn has_consulate_and_embassy_convenience_methods() {
    let mut game = new_game("consulate_test", Difficulty::Normal, 0);
    let player = game.human_player_nation;
    let mn_id = game.minor_nations()[0].id;

    assert!(!game.diplomacy.has_consulate(player, mn_id));
    assert!(!game.diplomacy.has_embassy(player, mn_id));

    game.diplomacy.build_consulate(player, mn_id).unwrap();
    assert!(game.diplomacy.has_consulate(player, mn_id));
    assert!(!game.diplomacy.has_embassy(player, mn_id));

    game.diplomacy.build_embassy(player, mn_id).unwrap();
    assert!(game.diplomacy.has_consulate(player, mn_id));
    assert!(game.diplomacy.has_embassy(player, mn_id));
}

// ── Would-accept-treaty standing check ────────────────────────────

#[test]
fn would_accept_treaty_respects_standing_threshold() {
    let mut game = new_game("accept_treaty", Difficulty::Normal, 0);
    let nation = game.great_powers()[0].id;

    // Default standing is 100 — should accept
    assert!(game.diplomacy.would_accept_treaty(nation));

    // Reduce to exactly 30 — should still accept
    game.diplomacy.reduce_standing(nation, 70);
    assert_eq!(game.diplomacy.get_standing(nation), 30);
    assert!(game.diplomacy.would_accept_treaty(nation));

    // Reduce to 29 — should reject
    game.diplomacy.reduce_standing(nation, 1);
    assert_eq!(game.diplomacy.get_standing(nation), 29);
    assert!(!game.diplomacy.would_accept_treaty(nation));
}

// ── Diplomacy edge case tests (plan 09) ──────────────────────────

#[test]
fn alliance_war_pact_combinations() {
    let mut game = new_game("edge_diplo", Difficulty::Normal, 0);
    let gps: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
    let mns: Vec<NationId> = game.minor_nations().iter().map(|n| n.id).collect();

    // Alliance between GP1 and GP2
    game.diplomacy.propose_alliance(gps[1], gps[2]).unwrap();

    // Pact between GP1 and MN1
    game.diplomacy.build_consulate(gps[1], mns[0]).ok();
    game.diplomacy.build_embassy(gps[1], mns[0]).ok();
    game.diplomacy.propose_pact(gps[1], mns[0]).unwrap();

    // GP3 declares war on GP2 — GP1 should be involved via alliance
    game.diplomacy.declare_war(gps[3], gps[2]);

    // GP4 attacks MN1 — GP1 should defend via pact
    game.diplomacy.declare_war(gps[4], mns[0]);

    // Process turn — alliance/pact obligations should trigger
    process_turn(&mut game);

    // Verify complex state
    assert!(game.diplomacy.is_at_war(gps[3], gps[2]));
    assert!(game.diplomacy.is_at_war(gps[4], mns[0]));
}

#[test]
fn cannot_ally_with_nation_at_war() {
    let mut game = new_game("war_ally", Difficulty::Normal, 0);
    let gps: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
    game.diplomacy.declare_war(gps[1], gps[2]);
    let result = game.diplomacy.propose_alliance(gps[1], gps[2]);
    assert!(result.is_err());
}

#[test]
fn cannot_pact_with_nation_at_war() {
    let mut game = new_game("war_pact", Difficulty::Normal, 0);
    let gps: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
    let mns: Vec<NationId> = game.minor_nations().iter().map(|n| n.id).collect();
    game.diplomacy.build_consulate(gps[1], mns[0]).ok();
    game.diplomacy.build_embassy(gps[1], mns[0]).ok();
    game.diplomacy.declare_war(gps[1], mns[0]);
    let result = game.diplomacy.propose_pact(gps[1], mns[0]);
    assert!(result.is_err());
}
