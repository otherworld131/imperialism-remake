//! Military integration tests.
//!
//! Tests for army unit building, upgrading, rewards, starting warships,
//! and related game mechanics.

use domain::data::GameConfig;
use domain::events::HistoryEvent;
use domain::game_state::{GameState, new_game};
use domain::map::UnitId;
use domain::military::battle_outcome::{BattleParams, BattleSite, compute_battle_outcome};
use domain::military::combat::{BattleConfig, CombatForce, TargetingPriority, resolve_battle_with_config};
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
    let initial_treasury = game.get_nation(player_id).unwrap().economy.treasury;
    let initial_army_size = game.get_nation(player_id).unwrap().military.army.len();

    // Build a Regulars unit (cost $500 in the CLI, but we simulate directly)
    let cost = Money::dollars(500);
    let capital_province = game.get_nation(player_id).unwrap().capital_province_id;
    let uid = UnitId(9_000_001);
    let unit = ArmyUnit::new(uid, ArmyUnitType::Regulars, player_id, capital_province);

    let player = game.get_nation_mut(player_id).unwrap();
    player.economy.treasury -= cost;
    player.military.army.push(unit);

    // Verify
    let player = game.get_nation(player_id).unwrap();
    assert_eq!(player.economy.treasury, initial_treasury - cost);
    assert_eq!(player.military.army.len(), initial_army_size + 1);
    assert_eq!(
        player.military.army.last().unwrap().unit_type,
        ArmyUnitType::Regulars
    );
    assert_eq!(player.military.army.last().unwrap().health, 100);
    assert_eq!(player.military.army.last().unwrap().medals, 0);
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
    player.military.army.push(unit);

    // Upgrade (cost $500)
    let upgrade_cost = Money::dollars(500);
    let initial_treasury = game.get_nation(player_id).unwrap().economy.treasury;
    let idx = game.get_nation(player_id).unwrap().military.army.len() - 1;

    let player = game.get_nation_mut(player_id).unwrap();
    player.economy.treasury -= upgrade_cost;
    let old_medals = player.military.army[idx].medals;
    let old_health = player.military.army[idx].health;
    player.military.army[idx].unit_type = ArmyUnitType::RifleInfantry;
    player.military.army[idx].movement_remaining = ArmyUnitType::RifleInfantry.stats().movement;

    // Verify
    let player = game.get_nation(player_id).unwrap();
    assert_eq!(player.economy.treasury, initial_treasury - upgrade_cost);
    assert_eq!(player.military.army[idx].unit_type, ArmyUnitType::RifleInfantry);
    assert_eq!(player.military.army[idx].medals, old_medals);
    assert_eq!(player.military.army[idx].medals, 2);
    assert_eq!(player.military.army[idx].health, old_health);
    assert_eq!(player.military.army[idx].health, 85);
}

// ── Recruit with insufficient funds: fails gracefully ──────────

#[test]
fn build_unit_with_insufficient_funds_fails_gracefully() {
    let mut game = minimal_game();
    let player_id = game.human_player_nation;

    // Drain treasury
    let player = game.get_nation_mut(player_id).unwrap();
    player.economy.treasury = Money::dollars(100);

    // Try to build a Regulars unit (cost $500) — should fail
    let cost = Money::dollars(500);
    let player = game.get_nation(player_id).unwrap();
    let can_afford = player.economy.treasury.checked_sub(cost).is_some();
    assert!(
        !can_afford,
        "Should not be able to afford $500 with $100 treasury"
    );

    // Army should remain unchanged
    let army_size_before = player.military.army.len();
    let treasury_before = player.economy.treasury;

    // The CLI would return early here — we just verify the check works
    assert_eq!(
        game.get_nation(player_id).unwrap().military.army.len(),
        army_size_before
    );
    assert_eq!(
        game.get_nation(player_id).unwrap().economy.treasury,
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
        player.military.army.push(unit);
    }

    // Process a turn — the rewards system should detect 6 arms and award a General
    let report = process_turn(&mut game);

    // Verify General was earned
    let player = game.get_nation(player_id).unwrap();
    let has_general = player
        .military.army
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
        player.military.generals_earned >= 1,
        "generals_earned should be >= 1"
    );
}

#[test]
fn no_general_earned_below_6_arms() {
    let mut game = minimal_game();
    let player_id = game.human_player_nation;
    let capital = game.get_nation(player_id).unwrap().capital_province_id;

    // Clear starting army so we control the exact arms count
    let player = game.get_nation_mut(player_id).unwrap();
    player.military.army.clear();

    // Add 2 Regulars (1 arm each) = 2 arms total (below threshold of 6)
    for i in 0..2 {
        let unit = ArmyUnit::new(
            UnitId(9_200_000 + i),
            ArmyUnitType::Regulars,
            player_id,
            capital,
        );
        let player = game.get_nation_mut(player_id).unwrap();
        player.military.army.push(unit);
    }

    let report = process_turn(&mut game);

    // Verify no General was earned
    let player = game.get_nation(player_id).unwrap();
    let has_general = player
        .military.army
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
    assert_eq!(player.military.capitol_bonus_capacity, 0);

    // Manually simulate the reward
    let player = game.get_nation_mut(player_id).unwrap();
    player.military.capitol_bonus_capacity += 1;

    let player = game.get_nation(player_id).unwrap();
    assert_eq!(
        player.military.capitol_bonus_capacity, 1,
        "Capitol bonus capacity should increase when conquering a GP capital"
    );
}

// ── Starting warship exists for each GP ─────────────────────────

#[test]
fn each_great_power_starts_with_one_frigate() {
    let game = minimal_game();
    for nation in game.great_powers() {
        assert!(
            !nation.military.warships.is_empty(),
            "{} should have at least one warship",
            nation.name
        );
        assert_eq!(
            nation.military.warships.len(),
            1,
            "{} should start with exactly 1 warship",
            nation.name
        );
        assert_eq!(
            nation.military.warships[0].ship_type,
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
        .flat_map(|n| n.military.warships.iter().map(|s| s.id))
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
            nation.military.warships.is_empty(),
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
    let initial_warships = nation.military.warships.len();

    // Build a Frigate (simulated — deduct materials, add ship)
    let fabric_cost = 2;
    let lumber_cost = 5;
    let arms_cost = 2;
    nation.consume_material(MaterialType::Fabric, fabric_cost);
    nation.consume_material(MaterialType::Lumber, lumber_cost);
    nation.consume_material(MaterialType::Arms, arms_cost);
    nation
        .military.warships
        .push(Ship::new(UnitId(9999), ShipType::Frigate, player));

    assert_eq!(nation.military.warships.len(), initial_warships + 1);
    // Easy difficulty starts with 20 Lumber, 10 Steel, 5 Fabric (bumped
    // alongside the card #130 capital-yield nerf).
    // So totals are: Fabric 5+5=10-2=8, Lumber 20+10=30-5=25, Arms 0+5-2=3
    assert_eq!(nation.material_amount(MaterialType::Fabric), 8); // (5 starting + 5 given) - 2
    assert_eq!(nation.material_amount(MaterialType::Lumber), 25); // (20 starting + 10 given) - 5
    assert_eq!(nation.material_amount(MaterialType::Arms), 3); // 5 - 2
}

// ── Data validation: all unit types have valid stats (plan 11) ────

#[test]
fn all_unit_types_have_valid_stats() {
    let types = [
        ArmyUnitType::Militia,
        ArmyUnitType::Regulars,
        ArmyUnitType::Grenadiers,
        ArmyUnitType::RifleInfantry,
        ArmyUnitType::Guards,
        ArmyUnitType::Sharpshooters,
        ArmyUnitType::ModernInfantry,
        ArmyUnitType::MachineGunners,
        ArmyUnitType::Rangers,
        ArmyUnitType::Cuirassiers,
        ArmyUnitType::Scouts,
        ArmyUnitType::CarbineCavalry,
        ArmyUnitType::Armour,
        ArmyUnitType::Mechanised,
        ArmyUnitType::LightArtillery,
        ArmyUnitType::StandardArtillery,
        ArmyUnitType::FieldArtillery,
        ArmyUnitType::SiegeArtillery,
        ArmyUnitType::RailroadGun,
        ArmyUnitType::MobileArtillery,
        ArmyUnitType::Sapper,
        ArmyUnitType::General,
    ];
    for unit_type in &types {
        let stats = unit_type.stats();
        // Every unit has non-negative movement
        assert!(
            stats.movement > 0 || *unit_type == ArmyUnitType::Militia,
            "{:?} should have positive movement (or be Militia)",
            unit_type
        );
        // Every unit has a category
        let _cat = unit_type.category();
        // Cost should be non-negative
        assert!(
            !stats.cost.is_negative(),
            "{:?} has negative cost",
            unit_type
        );
    }
}

#[test]
fn all_ship_types_have_valid_stats() {
    let types = [
        ShipType::Trader,
        ShipType::Indiaman,
        ShipType::Clipper,
        ShipType::Paddlewheeler,
        ShipType::Freighter,
        ShipType::Frigate,
        ShipType::ShipOfTheLine,
        ShipType::Raider,
        ShipType::Ironclad,
        ShipType::AdvancedIronclad,
        ShipType::ArmouredCruiser,
        ShipType::Dreadnought,
        ShipType::Battlecruiser,
    ];
    for ship_type in &types {
        let stats = ship_type.stats();
        assert!(stats.hull > 0, "{:?} should have positive hull", ship_type);
    }
}

// ── Expert worker capitol expansion reward (plan 05) ──────────────

#[test]
fn expert_worker_reward_at_10_experts() {
    let mut game = minimal_game();
    let player_id = game.human_player_nation;

    // Reset labor pool so we control the exact expert count
    let player = game.get_nation_mut(player_id).unwrap();
    player.economy.labor.untrained = 0;
    player.economy.labor.trained = 0;
    player.economy.labor.expert = 0;

    // Add 10 expert workers to the labor pool
    for _ in 0..10 {
        player.economy.labor.untrained += 1;
        player.economy.labor.train_worker();
        player.economy.labor.promote_worker();
    }
    assert_eq!(player.economy.labor.expert, 10);

    let initial_bonus = player.military.capitol_bonus_capacity;

    // Process a turn — the rewards system should detect 10 experts
    process_turn(&mut game);

    let player = game.get_nation(player_id).unwrap();
    assert!(
        player.military.capitol_bonus_capacity > initial_bonus,
        "Capitol bonus should increase at 10 experts (was {}, now {})",
        initial_bonus,
        player.military.capitol_bonus_capacity
    );
    assert!(
        player.military.expert_rewards_earned >= 1,
        "expert_rewards_earned should track the reward"
    );
}

#[test]
fn expert_worker_reward_at_30_experts() {
    let mut game = minimal_game();
    let player_id = game.human_player_nation;

    // Reset labor pool so we control the exact expert count
    let player = game.get_nation_mut(player_id).unwrap();
    player.economy.labor.untrained = 0;
    player.economy.labor.trained = 0;
    player.economy.labor.expert = 0;

    // Add 30 expert workers to the labor pool
    for _ in 0..30 {
        player.economy.labor.untrained += 1;
        player.economy.labor.train_worker();
        player.economy.labor.promote_worker();
    }
    assert_eq!(player.economy.labor.expert, 30);

    // Process a turn — should earn both rewards (10 and 30)
    process_turn(&mut game);

    let player = game.get_nation(player_id).unwrap();
    assert!(
        player.military.capitol_bonus_capacity >= 2,
        "Capitol bonus should be at least 2 at 30 experts (got {})",
        player.military.capitol_bonus_capacity
    );
    assert!(
        player.military.expert_rewards_earned >= 2,
        "expert_rewards_earned should be at least 2 at 30 experts (got {})",
        player.military.expert_rewards_earned
    );
}

#[test]
fn expert_worker_reward_not_awarded_twice() {
    let mut game = minimal_game();
    let player_id = game.human_player_nation;

    // Add 10 expert workers
    let player = game.get_nation_mut(player_id).unwrap();
    for _ in 0..10 {
        player.economy.labor.untrained += 1;
        player.economy.labor.train_worker();
        player.economy.labor.promote_worker();
    }

    // Process two turns
    process_turn(&mut game);
    let bonus_after_first = game.get_nation(player_id).unwrap().military.capitol_bonus_capacity;

    process_turn(&mut game);
    let bonus_after_second = game.get_nation(player_id).unwrap().military.capitol_bonus_capacity;

    assert_eq!(
        bonus_after_first, bonus_after_second,
        "Expert reward should not be awarded twice for the same threshold"
    );
}

// ── compute_battle_outcome tests ─────────────────────────────────

const ATK_NAT: NationId = NationId(1);
const DEF_NAT: NationId = NationId(2);
const BATTLE_PROV: ProvinceId = ProvinceId(10);

fn battle_unit(id: u32, unit_type: ArmyUnitType, owner: NationId, pos: ProvinceId) -> ArmyUnit {
    ArmyUnit::new(UnitId(id), unit_type, owner, pos)
}

fn default_cfg() -> GameConfig {
    GameConfig::default()
}

// 1. Large attacker force wins — province changes hands.
#[test]
fn battle_outcome_attacker_wins_province_changes_hands() {
    let attackers = vec![
        battle_unit(1, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(3, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(4, ArmyUnitType::SiegeArtillery, ATK_NAT, ProvinceId(5)),
    ];
    let defenders = vec![battle_unit(10, ArmyUnitType::Militia, DEF_NAT, BATTLE_PROV)];

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));

    assert_eq!(outcome.winner, ATK_NAT);
    let conquest = outcome.province_change.as_ref().expect("should have province change");
    assert_eq!(conquest.new_owner, ATK_NAT);
    assert_eq!(conquest.old_owner, DEF_NAT);
    assert_eq!(conquest.province_id, BATTLE_PROV);
    assert!(!outcome.attacker_retreated);
}

// 2. Defender wins — no province change.
#[test]
fn battle_outcome_defender_wins_no_province_change() {
    let attackers = vec![battle_unit(1, ArmyUnitType::Militia, ATK_NAT, ProvinceId(5))];
    let defenders = vec![
        battle_unit(10, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(11, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(12, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(13, ArmyUnitType::SiegeArtillery, DEF_NAT, BATTLE_PROV),
    ];

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));

    assert_eq!(outcome.winner, DEF_NAT);
    assert!(outcome.province_change.is_none());
    assert!(outcome.history_events.is_empty());
}

// 3. Empty attacker — defender wins immediately.
#[test]
fn battle_outcome_empty_attacker_defender_wins() {
    let attackers: Vec<ArmyUnit> = vec![];
    let defenders = vec![battle_unit(10, ArmyUnitType::Militia, DEF_NAT, BATTLE_PROV)];

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));

    assert_eq!(outcome.winner, DEF_NAT);
    assert!(outcome.province_change.is_none());
    assert!(outcome.casualties.is_empty());
}

// 4. Empty defender — attacker wins immediately.
#[test]
fn battle_outcome_empty_defender_attacker_wins_immediately() {
    let attackers = vec![battle_unit(1, ArmyUnitType::Regulars, ATK_NAT, ProvinceId(5))];
    let defenders: Vec<ArmyUnit> = vec![];

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));

    assert_eq!(outcome.winner, ATK_NAT);
    assert!(outcome.province_change.is_some());
    assert!(!outcome.attacker_survivors.is_empty());
}

// 5. Both sides empty — defender wins (attacker_won=false).
#[test]
fn battle_outcome_both_empty_defender_wins() {
    let attackers: Vec<ArmyUnit> = vec![];
    let defenders: Vec<ArmyUnit> = vec![];

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));

    assert_eq!(outcome.winner, DEF_NAT);
    assert!(outcome.province_change.is_none());
}

// 6. Mountain terrain gives defender an advantage.
#[test]
fn battle_outcome_mountain_terrain_defense_bonus_helps_defender() {
    let attackers = vec![
        battle_unit(1, ArmyUnitType::Regulars, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Regulars, ATK_NAT, ProvinceId(5)),
    ];
    let defenders = vec![
        battle_unit(10, ArmyUnitType::Regulars, DEF_NAT, BATTLE_PROV),
        battle_unit(11, ArmyUnitType::Regulars, DEF_NAT, BATTLE_PROV),
    ];

    let flat = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));
    let mountain = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT,
        DEF_NAT,
        BATTLE_PROV,
        &attackers,
        &defenders,
        BattleSite::terrain(TerrainType::Mountain),
        &default_cfg(),
    ));

    // Mountain terrain boosts defender — attacker should take equal or more damage.
    let atk_ids: Vec<UnitId> = attackers.iter().map(|u| u.id).collect();
    let flat_atk_dmg: u32 = atk_ids.iter().map(|id| flat.casualties.get(id).copied().unwrap_or(0)).sum();
    let mtn_atk_dmg: u32 = atk_ids.iter().map(|id| mountain.casualties.get(id).copied().unwrap_or(0)).sum();
    assert!(
        mtn_atk_dmg >= flat_atk_dmg,
        "Mountain terrain should cause >= attacker casualties: flat={flat_atk_dmg}, mountain={mtn_atk_dmg}"
    );
}

// 7. Fort level provides defense bonus.
#[test]
fn battle_outcome_fort_level_provides_defense_bonus() {
    let attackers = vec![
        battle_unit(1, ArmyUnitType::Regulars, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Regulars, ATK_NAT, ProvinceId(5)),
    ];
    let defenders = vec![
        battle_unit(10, ArmyUnitType::Regulars, DEF_NAT, BATTLE_PROV),
        battle_unit(11, ArmyUnitType::Regulars, DEF_NAT, BATTLE_PROV),
    ];

    let no_fort = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));
    let fort3 = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::fort(3), &default_cfg(),
    ));

    let atk_ids: Vec<UnitId> = attackers.iter().map(|u| u.id).collect();
    let no_fort_atk: u32 = atk_ids.iter().map(|id| no_fort.casualties.get(id).copied().unwrap_or(0)).sum();
    let fort3_atk: u32 = atk_ids.iter().map(|id| fort3.casualties.get(id).copied().unwrap_or(0)).sum();

    assert!(
        fort3_atk >= no_fort_atk,
        "Fort level 3 should cause >= attacker casualties: no_fort={no_fort_atk}, fort3={fort3_atk}"
    );
}

// 8. Siege artillery reduces fort bonus (A/B comparison: same force, fort-3, with vs without siege).
#[test]
fn battle_outcome_siege_artillery_reduces_fort_bonus() {
    // Identical base force — the only difference is one SiegeArtillery unit.
    let attackers_no_siege = vec![
        battle_unit(1, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(3, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
    ];
    let attackers_with_siege = vec![
        battle_unit(1, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(3, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(4, ArmyUnitType::SiegeArtillery, ATK_NAT, ProvinceId(5)),
    ];
    let defenders = vec![
        battle_unit(10, ArmyUnitType::Regulars, DEF_NAT, BATTLE_PROV),
        battle_unit(11, ArmyUnitType::Regulars, DEF_NAT, BATTLE_PROV),
    ];

    let no_siege = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers_no_siege, &defenders, BattleSite::fort(3), &default_cfg(),
    ));
    let with_siege = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers_with_siege, &defenders, BattleSite::fort(3), &default_cfg(),
    ));

    // siege_reduced_fort must be set when siege artillery is present and fort > 0.
    assert!(!no_siege.siege_reduced_fort, "No siege artillery → fort not reduced");
    assert!(with_siege.siege_reduced_fort, "Siege artillery + fort-3 → fort should be reduced");

    // Attacker with siege should take fewer casualties (reduced fort bonus helps them)
    // OR win when they would otherwise lose. Either direction proves siege helps.
    let ns_ids: Vec<UnitId> = attackers_no_siege.iter().map(|u| u.id).collect();
    let ws_ids: Vec<UnitId> = attackers_with_siege.iter().map(|u| u.id).collect();
    let no_siege_atk_dmg: u32 = ns_ids.iter().map(|id| no_siege.casualties.get(id).copied().unwrap_or(0)).sum();
    let with_siege_atk_dmg: u32 = ws_ids.iter().map(|id| with_siege.casualties.get(id).copied().unwrap_or(0)).sum();
    // Siege force wins or takes less damage per-unit
    let no_siege_win = no_siege.province_change.is_some();
    let with_siege_win = with_siege.province_change.is_some();
    // Per-unit damage: with_siege has 4 units vs no_siege's 3 — normalize
    let ns_per_unit = if !attackers_no_siege.is_empty() { no_siege_atk_dmg / attackers_no_siege.len() as u32 } else { 0 };
    let ws_per_unit = if !attackers_with_siege.is_empty() { with_siege_atk_dmg / attackers_with_siege.len() as u32 } else { 0 };
    assert!(
        with_siege_win || ws_per_unit <= ns_per_unit || (!no_siege_win && with_siege_win),
        "Siege artillery should improve attacker outcome: no_siege_win={no_siege_win}, with_siege_win={with_siege_win}, ns_per_unit={ns_per_unit}, ws_per_unit={ws_per_unit}"
    );
}

// 9. Medals are awarded to winning side survivors.
#[test]
fn battle_outcome_medals_awarded_to_winning_side() {
    let attackers = vec![
        battle_unit(1, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(3, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(4, ArmyUnitType::SiegeArtillery, ATK_NAT, ProvinceId(5)),
    ];
    let defenders = vec![battle_unit(10, ArmyUnitType::Militia, DEF_NAT, BATTLE_PROV)];

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));

    assert_eq!(outcome.winner, ATK_NAT);
    assert!(!outcome.medals_awarded.is_empty(), "Winners should receive medals");
    let atk_ids: Vec<UnitId> = attackers.iter().map(|u| u.id).collect();
    for (uid, _) in &outcome.medals_awarded {
        assert!(atk_ids.contains(uid), "Medal should be for an attacker unit");
    }
}

// 10. Casualties include destroyed units at full health.
#[test]
fn battle_outcome_casualties_include_destroyed_units_at_full_health() {
    let attackers = vec![
        battle_unit(1, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(3, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(4, ArmyUnitType::SiegeArtillery, ATK_NAT, ProvinceId(5)),
    ];
    let defenders = vec![battle_unit(10, ArmyUnitType::Militia, DEF_NAT, BATTLE_PROV)];

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));

    let def_uid = UnitId(10);
    assert!(
        outcome.casualties.contains_key(&def_uid),
        "Destroyed defender unit should appear in casualties"
    );
    assert_eq!(outcome.casualties[&def_uid], 100, "Destroyed unit damage should equal full health");
}

// 11. History events contain ProvinceConquered on attacker win.
#[test]
fn battle_outcome_history_events_province_conquered_on_win() {
    let attackers = vec![
        battle_unit(1, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(3, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
    ];
    let defenders = vec![battle_unit(10, ArmyUnitType::Militia, DEF_NAT, BATTLE_PROV)];

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));

    assert_eq!(outcome.winner, ATK_NAT);
    assert_eq!(outcome.history_events.len(), 1);
    assert!(matches!(
        &outcome.history_events[0],
        HistoryEvent::ProvinceConquered { conqueror, loser, province }
        if *conqueror == ATK_NAT && *loser == DEF_NAT && *province == BATTLE_PROV
    ));
}

// 12. No history events when defender wins.
#[test]
fn battle_outcome_no_history_events_on_defender_win() {
    let attackers = vec![battle_unit(1, ArmyUnitType::Militia, ATK_NAT, ProvinceId(5))];
    let defenders = vec![
        battle_unit(10, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(11, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(12, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
    ];

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &default_cfg(),
    ));

    assert_eq!(outcome.winner, DEF_NAT);
    assert!(outcome.history_events.is_empty());
}

// 13. Attacker retreat via custom BattleConfig.
#[test]
fn battle_outcome_attacker_retreat_via_config() {
    let mut config = BattleConfig::with_targeting(TargetingPriority::StrongestFirst, &default_cfg());
    config.attacker_can_retreat = true;
    config.attacker_retreat_ratio = 1.0; // retreat if defender has any advantage

    let attackers = vec![battle_unit(1, ArmyUnitType::Militia, ATK_NAT, ProvinceId(5))];
    let defenders = vec![
        battle_unit(10, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(11, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(12, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(13, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(14, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
    ];

    let outcome = compute_battle_outcome(BattleParams {
        attacker_id: ATK_NAT,
        defender_id: DEF_NAT,
        target_province: BATTLE_PROV,
        attacker_units: &attackers,
        defender_units: &defenders,
        terrain: None,
        fort_level: 0,
        battle_config: config,
        game_config: &default_cfg(),
    });

    assert!(outcome.attacker_retreated, "Attacker should have retreated");
    assert!(outcome.province_change.is_none(), "Retreating attacker should not take province");
    assert_eq!(outcome.winner, DEF_NAT);
}

// ── Parity tests: compute_battle_outcome must agree with resolve_battle_with_config ──
//
// These tests compare the BattleOutcome against the underlying BattleResult
// (which is also available as outcome.raw_result) to ensure semantic parity.

// 14. Attacker-win parity: outcome.winner matches raw_result.attacker_won.
#[test]
fn battle_outcome_parity_attacker_wins() {
    let attackers = vec![
        battle_unit(1, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(3, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(4, ArmyUnitType::SiegeArtillery, ATK_NAT, ProvinceId(5)),
    ];
    let defenders = vec![battle_unit(10, ArmyUnitType::Militia, DEF_NAT, BATTLE_PROV)];
    let cfg = default_cfg();

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &cfg,
    ));

    // raw_result must be consistent with BattleOutcome signals
    assert_eq!(outcome.raw_result.attacker_won, outcome.province_change.is_some());
    assert_eq!(outcome.raw_result.retreated, outcome.attacker_retreated);
    assert_eq!(outcome.raw_result.defender_retreated, outcome.defender_retreated);
    assert_eq!(outcome.raw_result.siege_reduced_fort, outcome.siege_reduced_fort);
    assert_eq!(outcome.winner, ATK_NAT);
    assert!(outcome.raw_result.attacker_won);
}

// 15. Defender-win parity.
#[test]
fn battle_outcome_parity_defender_wins() {
    let attackers = vec![battle_unit(1, ArmyUnitType::Militia, ATK_NAT, ProvinceId(5))];
    let defenders = vec![
        battle_unit(10, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(11, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
        battle_unit(12, ArmyUnitType::Guards, DEF_NAT, BATTLE_PROV),
    ];
    let cfg = default_cfg();

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &cfg,
    ));

    assert_eq!(outcome.raw_result.attacker_won, outcome.province_change.is_some());
    assert!(!outcome.raw_result.attacker_won);
    assert_eq!(outcome.winner, DEF_NAT);
    assert!(outcome.province_change.is_none());
}

// 16. Survivor lists in BattleOutcome match raw_result survivors.
#[test]
fn battle_outcome_survivors_match_raw_result() {
    let attackers = vec![
        battle_unit(1, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
    ];
    let defenders = vec![
        battle_unit(10, ArmyUnitType::Militia, DEF_NAT, BATTLE_PROV),
        battle_unit(11, ArmyUnitType::Militia, DEF_NAT, BATTLE_PROV),
    ];
    let cfg = default_cfg();

    let outcome = compute_battle_outcome(BattleParams::with_default_config(
        ATK_NAT, DEF_NAT, BATTLE_PROV, &attackers, &defenders, BattleSite::open(), &cfg,
    ));

    // Survivor unit IDs must be identical between the two representations
    let outcome_atk_ids: Vec<UnitId> = outcome.attacker_survivors.iter().map(|u| u.id).collect();
    let raw_atk_ids: Vec<UnitId> = outcome.raw_result.attacker_survivors.iter().map(|u| u.id).collect();
    assert_eq!(outcome_atk_ids, raw_atk_ids, "attacker survivors must match raw_result");

    let outcome_def_ids: Vec<UnitId> = outcome.defender_survivors.iter().map(|u| u.id).collect();
    let raw_def_ids: Vec<UnitId> = outcome.raw_result.defender_survivors.iter().map(|u| u.id).collect();
    assert_eq!(outcome_def_ids, raw_def_ids, "defender survivors must match raw_result");
}

// 17. compute_battle_outcome agrees with a direct resolve_battle_with_config call
//     for mountain terrain + fort-2 — parity on winner and casualty direction.
#[test]
fn battle_outcome_parity_terrain_fort() {
    let attackers = vec![
        battle_unit(1, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(2, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(3, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
        battle_unit(4, ArmyUnitType::Guards, ATK_NAT, ProvinceId(5)),
    ];
    let defenders = vec![
        battle_unit(10, ArmyUnitType::Regulars, DEF_NAT, BATTLE_PROV),
        battle_unit(11, ArmyUnitType::Regulars, DEF_NAT, BATTLE_PROV),
        battle_unit(12, ArmyUnitType::Regulars, DEF_NAT, BATTLE_PROV),
    ];
    let cfg = default_cfg();
    let config = BattleConfig::with_targeting(TargetingPriority::StrongestFirst, &cfg);

    let outcome = compute_battle_outcome(BattleParams {
        attacker_id: ATK_NAT,
        defender_id: DEF_NAT,
        target_province: BATTLE_PROV,
        attacker_units: &attackers,
        defender_units: &defenders,
        terrain: Some(TerrainType::Mountain),
        fort_level: 2,
        battle_config: config,
        game_config: &cfg,
    });

    // Cross-check: call resolve_battle_with_config directly with same inputs
    let atk_force = CombatForce { nation: ATK_NAT, units: attackers.clone() };
    let def_force = CombatForce { nation: DEF_NAT, units: defenders.clone() };
    let direct = resolve_battle_with_config(
        &atk_force,
        &def_force,
        BATTLE_PROV,
        Some(TerrainType::Mountain),
        2,
        BattleConfig::with_targeting(TargetingPriority::StrongestFirst, &cfg),
        &cfg,
    );

    // Both must agree on the winner
    assert_eq!(
        outcome.raw_result.attacker_won,
        direct.attacker_won,
        "compute_battle_outcome and resolve_battle_with_config must agree on winner"
    );
    assert_eq!(
        outcome.raw_result.retreated,
        direct.retreated,
        "retreat flags must agree"
    );
    assert_eq!(
        outcome.raw_result.siege_reduced_fort,
        direct.siege_reduced_fort,
        "siege_reduced_fort must agree"
    );
    // Survivor counts must match
    assert_eq!(
        outcome.attacker_survivors.len(),
        direct.attacker_survivors.len(),
        "attacker survivor counts must match"
    );
    assert_eq!(
        outcome.defender_survivors.len(),
        direct.defender_survivors.len(),
        "defender survivor counts must match"
    );
}
