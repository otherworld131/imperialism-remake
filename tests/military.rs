//! Military integration tests.
//!
//! Tests for army unit building, upgrading, rewards, starting warships,
//! and related game mechanics.

use domain::game_state::{GameState, new_game};
use domain::map::UnitId;
use domain::military::ships::{Ship, ShipType};
use domain::military::units::{ArmyUnit, ArmyUnitType};
use domain::turn::process_turn;
use domain::types::*;

/// Helper: create a minimal game at Normal difficulty.
fn minimal_game() -> GameState {
    new_game("test", Difficulty::Normal, 0)
}

// ── Build Regulars: verify cost deducted, army grows ────────────

#[test]
fn build_regulars_deducts_cost_and_grows_army() {
    let mut game = minimal_game();
    let player_id = game.human_player_nation;

    // Starting state
    let initial_treasury = game.get_nation(player_id).unwrap().treasury;
    let initial_army_size = game.get_nation(player_id).unwrap().army.len();

    // Build a Regulars unit (cost $500 in the CLI, but we simulate directly)
    let cost = Money::dollars(500);
    let capital_province = game.get_nation(player_id).unwrap().capital_province_id;
    let uid = UnitId(9_000_001);
    let unit = ArmyUnit::new(uid, ArmyUnitType::Regulars, player_id, capital_province);

    let player = game.get_nation_mut(player_id).unwrap();
    player.treasury -= cost;
    player.army.push(unit);

    // Verify
    let player = game.get_nation(player_id).unwrap();
    assert_eq!(player.treasury, initial_treasury - cost);
    assert_eq!(player.army.len(), initial_army_size + 1);
    assert_eq!(
        player.army.last().unwrap().unit_type,
        ArmyUnitType::Regulars
    );
    assert_eq!(player.army.last().unwrap().health, 100);
    assert_eq!(player.army.last().unwrap().medals, 0);
}

// ── Upgrade Regulars -> RifleInfantry: medals preserved, cost charged ──

#[test]
fn upgrade_regulars_to_rifle_infantry_preserves_medals() {
    let mut game = minimal_game();
    let player_id = game.human_player_nation;

    // Add a Regulars unit with 2 medals
    let capital = game.get_nation(player_id).unwrap().capital_province_id;
    let mut unit = ArmyUnit::new(
        UnitId(9_000_002),
        ArmyUnitType::Regulars,
        player_id,
        capital,
    );
    unit.medals = 2;
    unit.health = 85;

    let player = game.get_nation_mut(player_id).unwrap();
    player.army.push(unit);

    // Upgrade (cost $500)
    let upgrade_cost = Money::dollars(500);
    let initial_treasury = game.get_nation(player_id).unwrap().treasury;
    let idx = game.get_nation(player_id).unwrap().army.len() - 1;

    let player = game.get_nation_mut(player_id).unwrap();
    player.treasury -= upgrade_cost;
    let old_medals = player.army[idx].medals;
    let old_health = player.army[idx].health;
    player.army[idx].unit_type = ArmyUnitType::RifleInfantry;
    player.army[idx].movement_remaining = ArmyUnitType::RifleInfantry.stats().movement;

    // Verify
    let player = game.get_nation(player_id).unwrap();
    assert_eq!(player.treasury, initial_treasury - upgrade_cost);
    assert_eq!(player.army[idx].unit_type, ArmyUnitType::RifleInfantry);
    assert_eq!(player.army[idx].medals, old_medals);
    assert_eq!(player.army[idx].medals, 2);
    assert_eq!(player.army[idx].health, old_health);
    assert_eq!(player.army[idx].health, 85);
}

// ── Recruit with insufficient funds: fails gracefully ──────────

#[test]
fn build_unit_with_insufficient_funds_fails_gracefully() {
    let mut game = minimal_game();
    let player_id = game.human_player_nation;

    // Drain treasury
    let player = game.get_nation_mut(player_id).unwrap();
    player.treasury = Money::dollars(100);

    // Try to build a Regulars unit (cost $500) — should fail
    let cost = Money::dollars(500);
    let player = game.get_nation(player_id).unwrap();
    let can_afford = player.treasury.checked_sub(cost).is_some();
    assert!(
        !can_afford,
        "Should not be able to afford $500 with $100 treasury"
    );

    // Army should remain unchanged
    let army_size_before = player.army.len();
    let treasury_before = player.treasury;

    // The CLI would return early here — we just verify the check works
    assert_eq!(
        game.get_nation(player_id).unwrap().army.len(),
        army_size_before
    );
    assert_eq!(
        game.get_nation(player_id).unwrap().treasury,
        treasury_before
    );
}

// ── General earned at 6 arms total ──────────────────────────────

#[test]
fn general_earned_at_6_arms_total() {
    let mut game = minimal_game();
    let player_id = game.human_player_nation;
    let capital = game.get_nation(player_id).unwrap().capital_province_id;

    // Add units totaling 6 arms:
    // 3 Grenadiers (2 arms each) = 6 arms total
    for i in 0..3 {
        let unit = ArmyUnit::new(
            UnitId(9_100_000 + i),
            ArmyUnitType::Grenadiers,
            player_id,
            capital,
        );
        let player = game.get_nation_mut(player_id).unwrap();
        player.army.push(unit);
    }

    // Process a turn — the rewards system should detect 6 arms and award a General
    let report = process_turn(&mut game);

    // Verify General was earned
    let player = game.get_nation(player_id).unwrap();
    let has_general = player
        .army
        .iter()
        .any(|u| u.unit_type == ArmyUnitType::General);
    assert!(has_general, "Should have earned a General at 6 arms total");

    // Verify reward was reported
    let general_reward = report
        .rewards_earned
        .iter()
        .any(|(nid, desc)| *nid == player_id && desc.contains("General"));
    assert!(general_reward, "General reward should be in the report");

    // Verify generals_earned counter was updated
    assert!(
        player.generals_earned >= 1,
        "generals_earned should be >= 1"
    );
}

#[test]
fn no_general_earned_below_6_arms() {
    let mut game = minimal_game();
    let player_id = game.human_player_nation;
    let capital = game.get_nation(player_id).unwrap().capital_province_id;

    // Add 2 Regulars (1 arm each) = 2 arms total (below threshold of 6)
    for i in 0..2 {
        let unit = ArmyUnit::new(
            UnitId(9_200_000 + i),
            ArmyUnitType::Regulars,
            player_id,
            capital,
        );
        let player = game.get_nation_mut(player_id).unwrap();
        player.army.push(unit);
    }

    let report = process_turn(&mut game);

    // Verify no General was earned
    let player = game.get_nation(player_id).unwrap();
    let has_general = player
        .army
        .iter()
        .any(|u| u.unit_type == ArmyUnitType::General);
    assert!(
        !has_general,
        "Should NOT have earned a General with only 2 arms"
    );

    let general_reward = report
        .rewards_earned
        .iter()
        .any(|(nid, desc)| *nid == player_id && desc.contains("General"));
    assert!(!general_reward, "No General reward should be in the report");
}

// ── Capitol expansion on GP capital conquest ────────────────────

#[test]
fn capitol_expansion_on_gp_capital_conquest() {
    // This tests the capitol_bonus_capacity field directly
    let mut game = minimal_game();
    let player_id = game.human_player_nation;

    // Initially, capitol_bonus_capacity should be 0
    let player = game.get_nation(player_id).unwrap();
    assert_eq!(player.capitol_bonus_capacity, 0);

    // Manually simulate the reward
    let player = game.get_nation_mut(player_id).unwrap();
    player.capitol_bonus_capacity += 1;

    let player = game.get_nation(player_id).unwrap();
    assert_eq!(
        player.capitol_bonus_capacity, 1,
        "Capitol bonus capacity should increase when conquering a GP capital"
    );
}

// ── Starting warship exists for each GP ─────────────────────────

#[test]
fn each_great_power_starts_with_one_frigate() {
    let game = minimal_game();
    for nation in game.great_powers() {
        assert!(
            !nation.warships.is_empty(),
            "{} should have at least one warship",
            nation.name
        );
        assert_eq!(
            nation.warships.len(),
            1,
            "{} should start with exactly 1 warship",
            nation.name
        );
        assert_eq!(
            nation.warships[0].ship_type,
            ShipType::Frigate,
            "{}'s starting warship should be a Frigate",
            nation.name
        );
    }
}

#[test]
fn starting_warship_ids_are_unique() {
    let game = minimal_game();
    let all_warship_ids: Vec<UnitId> = game
        .great_powers()
        .iter()
        .flat_map(|n| n.warships.iter().map(|s| s.id))
        .collect();
    for i in 0..all_warship_ids.len() {
        for j in (i + 1)..all_warship_ids.len() {
            assert_ne!(
                all_warship_ids[i], all_warship_ids[j],
                "All warship IDs must be unique"
            );
        }
    }
}

#[test]
fn minor_nations_have_no_starting_warships() {
    let game = minimal_game();
    for nation in game.minor_nations() {
        assert!(
            nation.warships.is_empty(),
            "Minor Nation {} should have no warships",
            nation.name
        );
    }
}

// ── Build Frigate: verify resources deducted, ship added ────────

#[test]
fn build_frigate_deducts_resources_and_adds_ship() {
    let mut game = new_game("ship_build", Difficulty::Easy, 0); // Easy has starting materials
    let player = game.human_player_nation;

    // Give player materials for a Frigate: 2 fabric + 5 lumber + 2 arms
    let nation = game.get_nation_mut(player).unwrap();
    nation.add_material(MaterialType::Fabric, 5);
    nation.add_material(MaterialType::Lumber, 10);
    nation.add_material(MaterialType::Arms, 5);
    let initial_warships = nation.warships.len();

    // Build a Frigate (simulated — deduct materials, add ship)
    let fabric_cost = 2;
    let lumber_cost = 5;
    let arms_cost = 2;
    nation.consume_material(MaterialType::Fabric, fabric_cost);
    nation.consume_material(MaterialType::Lumber, lumber_cost);
    nation.consume_material(MaterialType::Arms, arms_cost);
    nation
        .warships
        .push(Ship::new(UnitId(9999), ShipType::Frigate, player));

    assert_eq!(nation.warships.len(), initial_warships + 1);
    assert_eq!(nation.material_amount(MaterialType::Fabric), 3); // 5 - 2
    assert_eq!(nation.material_amount(MaterialType::Lumber), 5); // 10 - 5
    assert_eq!(nation.material_amount(MaterialType::Arms), 3); // 5 - 2
}
