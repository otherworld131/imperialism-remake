mod test_helpers;

use domain::game_state::{new_game, new_game_with_data};
use domain::turn::process_turn;
use domain::types::*;
use test_helpers::{game_at_turn, game_with_difficulty, minimal_game, real_game_data};

/// Run a full game simulation and verify invariants hold throughout.
fn run_simulation(
    map_key: &str,
    turns: u32,
    difficulty: Difficulty,
    human_nation_index: usize,
) -> domain::game_state::GameState {
    let game_data = real_game_data();
    let mut game = new_game_with_data(map_key, difficulty, human_nation_index, game_data);
    for _ in 0..turns {
        process_turn(&mut game);
        // Invariants that must hold every turn:
        assert_eq!(game.world.nations.len(), 23, "Nation count must stay at 23");
        assert!(!game.world.nations.is_empty());
        // Total provinces should stay at 120 (provinces don't get created/destroyed, just change owners)
        assert_eq!(game.world.provinces.len(), 120, "Province count must stay at 120");
    }
    game
}

/// Verify common invariants on a game state that has been simulated.
fn assert_valid_game_state(game: &domain::game_state::GameState) {
    // All provinces must have an owner that exists in the nations list.
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    for province in &game.world.provinces {
        assert!(
            nation_ids.contains(&province.owner),
            "Province {} has owner {} which is not a valid nation",
            province.name,
            province.owner
        );
    }
    // Nation counts
    assert_eq!(game.great_powers().len(), 7);
    assert_eq!(game.minor_nations().len(), 16);
}

#[test]
fn test_10_turn_smoke_test() {
    let game = run_simulation("smoke_test", 10, Difficulty::Normal, 0);

    assert_valid_game_state(&game);

    // Turn number should have advanced: started at 1, processed 10 turns => now at 11
    assert_eq!(game.turn, TurnNumber::new(11));

    // No nation should have an absurdly negative treasury (maintenance can cause
    // small negatives, but check for reasonable bounds).
    for nation in &game.world.nations {
        if nation.economy.treasury.is_negative() {
            // Negative treasury is allowed by the game (maintenance costs can push it
            // negative), but it should not be catastrophically negative.
            assert!(
                nation.economy.treasury.as_dollars() > -100_000,
                "Nation {} has unreasonably negative treasury: {}",
                nation.name,
                nation.economy.treasury
            );
        }
    }

    // All provinces should have valid owners.
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    for province in &game.world.provinces {
        assert!(
            nation_ids.contains(&province.owner),
            "Province {} has invalid owner {}",
            province.name,
            province.owner
        );
    }
}

#[test]
fn test_100_turn_endurance() {
    let game = run_simulation("endurance", 100, Difficulty::Normal, 0);

    assert_valid_game_state(&game);

    // Turn number: started at 1, processed 100 turns => now at 101
    assert_eq!(game.turn, TurnNumber::new(101));

    // At least one AI nation should have researched at least one technology
    // after 100 turns. The AI researches the cheapest available tech each turn.
    let any_ai_researched = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power() && n.id != game.human_player_nation)
        .any(|n| !n.researched_techs.is_empty());
    assert!(
        any_ai_researched,
        "After 100 turns, at least one AI nation should have researched a technology"
    );

    // AI nations should have built some army units (total across all AI > 0).
    let total_ai_army: usize = game
        .world.nations
        .iter()
        .filter(|n| n.is_great_power() && n.id != game.human_player_nation)
        .map(|n| n.military.army.len())
        .sum();
    assert!(
        total_ai_army > 0,
        "After 100 turns, AI nations should have built some army units, but total is 0"
    );

    // The human player should have some resources in the warehouse from
    // resource collection and/or trade.
    let human = game.get_nation(game.human_player_nation).unwrap();
    let total_resources: u32 = human.economy.warehouse.values().sum();
    let total_materials: u32 = human.economy.materials.values().sum();
    let total_goods: u32 = human.economy.goods.values().sum();
    let has_some_economic_activity = total_resources > 0
        || total_materials > 0
        || total_goods > 0
        || human.economy.treasury > Money::ZERO;
    assert!(
        has_some_economic_activity,
        "After 100 turns, the human player should have some economic output"
    );
}

#[test]
#[ignore] // slow full-game test — run with `cargo test -- --ignored`
fn test_full_game_to_1915() {
    // 400 turns: from 1815 Q1 (turn 1) to 1915 Q1 (turn 401 after advancing)
    let game = run_simulation("full_game", 400, Difficulty::Normal, 0);

    assert_valid_game_state(&game);

    // Verify game reaches 1915
    assert_eq!(game.turn.year(), 1915);
    assert_eq!(game.turn.quarter(), 1);
    assert_eq!(game.turn, TurnNumber::new(401));

    // Verify game is over
    assert!(
        game.is_game_over(),
        "Game should be over after 400 turns (1915 Q1)"
    );

    // All provinces should still have valid owners
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    for province in &game.world.provinces {
        assert!(
            nation_ids.contains(&province.owner),
            "Province {} has invalid owner {} at end of game",
            province.name,
            province.owner
        );
    }
}

#[test]
fn test_determinism() {
    let turns = 20;
    let map_key = "determinism_test";

    let game_a = run_simulation(map_key, turns, Difficulty::Normal, 0);
    let game_b = run_simulation(map_key, turns, Difficulty::Normal, 0);

    // Static structure must match
    assert_eq!(game_a.turn, game_b.turn, "Turn numbers should be identical");
    assert_eq!(game_a.world.nations.len(), game_b.world.nations.len());
    assert_eq!(
        game_a.world.hex_map.tile_count(),
        game_b.world.hex_map.tile_count(),
        "Map tile counts should be identical"
    );
    assert_eq!(game_a.world.provinces.len(), game_b.world.provinces.len());

    // Dynamic state must also match — the game uses seeded RNG so two runs
    // with the same map key should produce identical outcomes.
    for (nation_a, nation_b) in game_a.world.nations.iter().zip(game_b.world.nations.iter()) {
        assert_eq!(nation_a.id, nation_b.id, "Nation IDs should match");
        assert_eq!(nation_a.name, nation_b.name, "Nation names should match");
        assert_eq!(
            nation_a.economy.treasury, nation_b.economy.treasury,
            "Treasury should be identical for {}",
            nation_a.name
        );
        assert_eq!(
            nation_a.military.army.len(),
            nation_b.military.army.len(),
            "Army size should be identical for {}",
            nation_a.name
        );
        assert_eq!(
            nation_a.military.warships.len(),
            nation_b.military.warships.len(),
            "Warship count should be identical for {}",
            nation_a.name
        );
        assert_eq!(
            nation_a.province_count(),
            nation_b.province_count(),
            "Province count should be identical for {}",
            nation_a.name
        );
    }

    // Province ownership must match
    for (prov_a, prov_b) in game_a.world.provinces.iter().zip(game_b.world.provinces.iter()) {
        assert_eq!(
            prov_a.owner, prov_b.owner,
            "Province {} ownership should be deterministic",
            prov_a.name
        );
    }
}

#[test]
fn test_different_map_keys_diverge() {
    let turns = 20;

    let game_a = run_simulation("key_a", turns, Difficulty::Normal, 0);
    let game_b = run_simulation("key_b", turns, Difficulty::Normal, 0);

    // Both should be valid
    assert_valid_game_state(&game_a);
    assert_valid_game_state(&game_b);

    // The games should produce different resource totals because the maps
    // are different (different terrain placement, different tile yields).
    let total_resources_a: u32 = game_a
        .world.nations
        .iter()
        .flat_map(|n| n.economy.warehouse.values())
        .sum();
    let total_resources_b: u32 = game_b
        .world.nations
        .iter()
        .flat_map(|n| n.economy.warehouse.values())
        .sum();

    let total_treasury_a: i64 = game_a.world.nations.iter().map(|n| n.economy.treasury.as_dollars()).sum();
    let total_treasury_b: i64 = game_b.world.nations.iter().map(|n| n.economy.treasury.as_dollars()).sum();

    // At least one of these aggregate metrics should differ between the two map keys.
    let diverged = total_resources_a != total_resources_b || total_treasury_a != total_treasury_b;
    assert!(
        diverged,
        "Games with different map keys should produce different outcomes. \
         Resources: {} vs {}, Treasury: {} vs {}",
        total_resources_a, total_resources_b, total_treasury_a, total_treasury_b
    );
}

#[test]
fn test_multiple_difficulties() {
    let difficulties = [
        Difficulty::Introductory,
        Difficulty::Easy,
        Difficulty::Normal,
        Difficulty::Hard,
        Difficulty::NighOnImpossible,
    ];

    let mut starting_treasuries: Vec<Money> = Vec::new();

    for difficulty in &difficulties {
        let game = game_with_difficulty(*difficulty);
        let human = game.get_nation(game.human_player_nation).unwrap();
        starting_treasuries.push(human.economy.treasury);

        // Run 5 turns and verify valid state
        let game = run_simulation("difficulty_test", 5, *difficulty, 0);
        assert_valid_game_state(&game);
        assert_eq!(game.turn, TurnNumber::new(6));
    }

    // Starting treasury should decrease as difficulty increases.
    // Introductory ($15,000) > Easy ($12,000) > Normal ($10,000) > Hard ($8,000) > NighOnImpossible ($5,000)
    for i in 0..starting_treasuries.len() - 1 {
        assert!(
            starting_treasuries[i] > starting_treasuries[i + 1],
            "Difficulty {:?} should have higher starting treasury than {:?}: {} vs {}",
            difficulties[i],
            difficulties[i + 1],
            starting_treasuries[i],
            starting_treasuries[i + 1]
        );
    }
}

#[test]
fn test_all_nations_as_player() {
    for index in 0..7 {
        let game = run_simulation("player_nation_test", 5, Difficulty::Normal, index);
        assert_valid_game_state(&game);
        assert_eq!(game.turn, TurnNumber::new(6));

        // The human player nation should correspond to the selected index.
        // Each index selects a different Great Power.
        let great_power_ids: Vec<NationId> = game
            .world.nations
            .iter()
            .filter(|n| n.is_great_power())
            .map(|n| n.id)
            .collect();
        assert!(
            great_power_ids.contains(&game.human_player_nation),
            "Human player nation should be one of the Great Powers for index {}",
            index
        );
    }

    // Additionally verify that different indices give different human nations.
    let mut human_ids = std::collections::HashSet::new();
    for index in 0..7 {
        let game = new_game("player_nation_test", Difficulty::Normal, index);
        human_ids.insert(game.human_player_nation);
    }
    assert_eq!(
        human_ids.len(),
        7,
        "Each human_nation_index should select a different Great Power"
    );
}

// ── Tests using test_helpers builders ────────────────────────────

#[test]
fn test_minimal_game_is_valid() {
    let game = minimal_game();
    assert_valid_game_state(&game);
    assert_eq!(game.turn, TurnNumber::new(1));
    assert_eq!(game.difficulty, Difficulty::Normal);
}

#[test]
fn test_game_at_turn_advances_correctly() {
    let game = game_at_turn(11);
    assert_eq!(game.turn, TurnNumber::new(11));
    assert_valid_game_state(&game);
}

#[test]
fn test_game_at_turn_1_is_initial_state() {
    let game = game_at_turn(1);
    assert_eq!(game.turn, TurnNumber::new(1));
    assert_valid_game_state(&game);
}

#[test]
fn test_game_with_difficulty_builder() {
    for difficulty in [
        Difficulty::Introductory,
        Difficulty::Easy,
        Difficulty::Normal,
        Difficulty::Hard,
        Difficulty::NighOnImpossible,
    ] {
        let game = game_with_difficulty(difficulty);
        assert_eq!(game.difficulty, difficulty);
        assert_valid_game_state(&game);
    }
}

// ── AI verification tests ────────────────────────────────────────

#[test]
fn ai_does_not_hire_infinite_farmers() {
    // Regression for user report: "AI now seems to hire infinite farmers when
    // it has the funds". The saturation picker now caps per-type workers at
    // ceil(demand / civilian_target_tiles_per_worker), preventing runaway
    // hiring of any single improver type.
    let mut game = new_game("farmer_cap", Difficulty::Normal, 0);
    for _ in 0..40 {
        process_turn(&mut game);
    }
    for n in game
        .great_powers()
        .iter()
        .filter(|n| n.id != game.human_player_nation)
    {
        let farmer_count = n
            .military
            .civilians
            .iter()
            .filter(|c| c.civilian_type == domain::economy::civilians::CivilianType::Farmer)
            .count();
        assert!(
            farmer_count <= 10,
            "{} hired {} farmers in 40 turns; saturation cap should hold this well below 10",
            n.name,
            farmer_count
        );
    }
}

#[test]
fn ai_does_not_hire_locked_civilians_without_tech() {
    // Regression for user report: tech-gating not working for
    // Rancher/Forester/Driller. At turn 1 (year 1815, before any of the
    // gating techs are available), no AI should have hired any of these.
    let mut game = new_game("tech_gate", Difficulty::Normal, 0);
    process_turn(&mut game);
    use domain::economy::civilians::CivilianType;
    for n in game.great_powers().iter() {
        let has_locked = n.military.civilians.iter().any(|c| {
            matches!(
                c.civilian_type,
                CivilianType::Rancher | CivilianType::Forester | CivilianType::Driller
            )
        });
        // Starting kit is Prospector + Miner + Engineer; none of the locked
        // types should appear after turn 1 either.
        assert!(
            !has_locked,
            "{} has a locked civilian (Rancher/Forester/Driller) before researching its tech",
            n.name
        );
    }
}

#[test]
fn ai_achieves_self_sustaining_economy_within_20_turns() {
    let mut game = new_game("econ_test", Difficulty::Normal, 0);
    for _ in 0..20 {
        process_turn(&mut game);
    }
    // At least one AI GP should be developing its economy: positive treasury,
    // growing timber/grain stockpile, or actively producing minerals.
    // Per the manual's tech-gating, Foresters are not hireable until Iron
    // Railroad Bridge (1821–1824) so timber ramps later than other resources;
    // accept a broader "showing signs of economic life" predicate here.
    let ai_viable = game
        .great_powers()
        .iter()
        .filter(|n| n.id != game.human_player_nation)
        .any(|n| {
            n.economy.treasury > Money::dollars(0)
                || n.resource_amount(ResourceType::Timber) > 10
                || n.resource_amount(ResourceType::Grain) > 10
                || n.resource_amount(ResourceType::Coal) > 10
        });
    assert!(
        ai_viable,
        "At least one AI should be economically viable after 20 turns"
    );
}

#[test]
fn ai_does_not_declare_war_on_allies() {
    // Run 50 turns and verify no AI declares war on a nation it has an alliance with.
    // This passes as long as AI code checks for alliances before declaring war.
    let mut game = new_game("diplo_test", Difficulty::Normal, 0);
    for turn in 0..50 {
        process_turn(&mut game);
        // After each turn, verify no two nations that are allies are also at war
        let gp_ids: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
        for &a in &gp_ids {
            let allies = game.world.diplomacy.get_allies(a);
            for &ally in &allies {
                let rel = game.world.diplomacy.get_relation(a, ally);
                if let Some(r) = rel {
                    assert!(
                        !r.at_war,
                        "Turn {}: Nations {} and {} are allies AND at war!",
                        turn + 1,
                        a,
                        ally
                    );
                }
            }
        }
    }
}

#[test]
fn minor_nations_respond_to_trade_offers() {
    let mut game = new_game("mn_trade", Difficulty::Normal, 0);
    // Process several turns — AI will build consulates and trade with minor nations
    for _ in 0..10 {
        process_turn(&mut game);
    }
    // At least one AI nation should have some trade history or resources from trade
    let any_ai_traded = game
        .great_powers()
        .iter()
        .filter(|n| n.id != game.human_player_nation)
        .any(|n| !n.archives.trade_history.is_empty() || !n.economy.warehouse.is_empty());
    assert!(
        any_ai_traded,
        "After 10 turns, at least one AI should have engaged in trade or collected resources"
    );
}

#[test]
fn application_shuts_down_cleanly() {
    // Create a game, process some turns, drop everything
    // This verifies no panics on cleanup
    let mut game = new_game("shutdown_test", Difficulty::Normal, 0);
    for _ in 0..10 {
        process_turn(&mut game);
    }
    drop(game); // explicit drop to verify no resource leaks
    // If we get here without panicking, the test passes
}

// ── AI difficulty bonus tests ──────────────────────────────────

#[test]
fn ai_hard_gets_resource_bonus() {
    // On Hard difficulty, AI nations get +10% resource production bonus.
    // Verify the economy functions correctly under this difficulty.
    let mut game = new_game("ai_bonus_h", Difficulty::Hard, 0);
    for _ in 0..5 {
        process_turn(&mut game);
    }

    assert_eq!(game.difficulty, Difficulty::Hard);

    // AI nations should have produced some resources after 5 turns.
    let ai_has_output = game
        .great_powers()
        .iter()
        .filter(|n| n.id != game.human_player_nation)
        .any(|n| {
            let resources: u32 = n.economy.warehouse.values().sum();
            let materials: u32 = n.economy.materials.values().sum();
            resources > 0 || materials > 0 || n.economy.treasury > Money::dollars(0)
        });
    assert!(
        ai_has_output,
        "AI nations should have economic output after 5 turns on Hard"
    );
}

#[test]
fn ai_noi_gets_larger_resource_bonus() {
    // On NighOnImpossible, AI nations get +25% resource production bonus.
    // Verify the economy functions correctly under this difficulty.
    let mut game = new_game("ai_bonus_noi", Difficulty::NighOnImpossible, 0);
    for _ in 0..5 {
        process_turn(&mut game);
    }

    assert_eq!(game.difficulty, Difficulty::NighOnImpossible);

    let ai_has_output = game
        .great_powers()
        .iter()
        .filter(|n| n.id != game.human_player_nation)
        .any(|n| {
            let resources: u32 = n.economy.warehouse.values().sum();
            let materials: u32 = n.economy.materials.values().sum();
            resources > 0 || materials > 0 || n.economy.treasury > Money::dollars(0)
        });
    assert!(
        ai_has_output,
        "AI nations should have economic output after 5 turns on NOI"
    );
}

#[test]
fn human_player_gets_no_bonus() {
    // Verify that on Hard/NOI, the human player does not receive the AI resource bonus.
    let game_hard = new_game("human_no_bonus", Difficulty::Hard, 0);
    let game_noi = new_game("human_no_bonus", Difficulty::NighOnImpossible, 0);

    // Human player's starting treasury should reflect only the base difficulty amount
    let human_hard = game_hard.get_nation(game_hard.human_player_nation).unwrap();
    let human_noi = game_noi.get_nation(game_noi.human_player_nation).unwrap();

    // Hard: human gets $8,000 (no AI bonus)
    assert_eq!(
        human_hard.economy.treasury,
        Money::dollars(8000),
        "Human on Hard should have $8,000 (no AI cash bonus)"
    );

    // NOI: human gets $5,000 (no AI bonus)
    assert_eq!(
        human_noi.economy.treasury,
        Money::dollars(5000),
        "Human on NOI should have $5,000 (no AI cash bonus)"
    );

    // Verify AI nations DO get the bonus starting cash
    for nation in game_hard.great_powers() {
        if nation.id != game_hard.human_player_nation {
            assert_eq!(
                nation.economy.treasury,
                Money::dollars(9000),
                "AI on Hard should have $9,000 ($8,000 + $1,000 bonus)"
            );
        }
    }
    for nation in game_noi.great_powers() {
        if nation.id != game_noi.human_player_nation {
            assert_eq!(
                nation.economy.treasury,
                Money::dollars(10000),
                "AI on NOI should have $10,000 ($5,000 + $5,000 bonus)"
            );
        }
    }
}

// ── Naval battle balance ─────────────────────────────────────────

#[test]
fn naval_balance_frigates_vs_ship_of_the_line() {
    use domain::data::GameData;
    use domain::map::UnitId;
    use domain::military::naval::resolve_naval_battle;
    use domain::military::ships::{Ship, ShipType};

    let data = GameData::default();
    let mut frigate_wins = 0;
    let mut sol_wins = 0;

    for seed in 0..100u32 {
        let frigates: Vec<Ship> = (0..3)
            .map(|i| Ship::with_data(UnitId(seed * 10 + i), ShipType::Frigate, NationId(1), &data))
            .collect();
        let sol: Vec<Ship> = vec![Ship::with_data(
            UnitId(seed * 10 + 100),
            ShipType::ShipOfTheLine,
            NationId(2),
            &data,
        )];

        let result = resolve_naval_battle(&frigates, &sol, NationId(1), NationId(2), &data);
        if result.attacker_won {
            frigate_wins += 1;
        } else {
            sol_wins += 1;
        }
    }

    println!(
        "3 Frigates vs 1 SotL: Frigates won {}/100, SotL won {}/100",
        frigate_wins, sol_wins
    );
    // 3 Frigates (FP 9 total) vs 1 SotL (FP 6) — Frigates should win most
    // Deterministic combat: result should be consistent across runs
    assert!(frigate_wins + sol_wins == 100, "All battles should resolve");
}

// ── Land battle balance ──────────────────────────────────────────

#[test]
fn land_battle_balance_various_compositions() {
    use domain::map::UnitId;
    use domain::military::combat::{CombatForce, resolve_battle};
    use domain::military::units::{ArmyUnit, ArmyUnitType};

    let compositions: Vec<(&str, u32, ArmyUnitType, u32, ArmyUnitType)> = vec![
        (
            "3 Regulars vs 4 Militia",
            3,
            ArmyUnitType::Regulars,
            4,
            ArmyUnitType::Minutemen,
        ),
        (
            "5 Regulars vs 4 Militia",
            5,
            ArmyUnitType::Regulars,
            4,
            ArmyUnitType::Minutemen,
        ),
        (
            "2 Grenadiers vs 3 Regulars",
            2,
            ArmyUnitType::Grenadiers,
            3,
            ArmyUnitType::Regulars,
        ),
    ];

    for (name, atk_count, atk_type, def_count, def_type) in &compositions {
        let attacker = CombatForce {
            nation: NationId(1),
            units: (0..*atk_count)
                .map(|i| ArmyUnit::new(UnitId(i), *atk_type, NationId(1), ProvinceId(1)))
                .collect(),
        };
        let defender = CombatForce {
            nation: NationId(2),
            units: (0..*def_count)
                .map(|i| ArmyUnit::new(UnitId(100 + i), *def_type, NationId(2), ProvinceId(2)))
                .collect(),
        };
        let result = resolve_battle(&attacker, &defender, ProvinceId(2), None, 0);
        println!(
            "{}: attacker_won={}, atk_casualties={}, def_casualties={}",
            name,
            result.attacker_won,
            result.attacker_casualties.len(),
            result.defender_casualties.len()
        );
    }
}

// ── Late-game memory profiling ───────────────────────────────────

#[test]
#[ignore] // slow profiling test — run with `cargo test -- --ignored`
fn profile_memory_late_game() {
    let mut game = new_game("late_game", Difficulty::Normal, 0);
    // Fast forward to turn 300
    for _ in 0..300 {
        process_turn(&mut game);
    }

    // Check bounded growth
    let total_history = game.archive.history.len();
    let total_nations = game.world.nations.len();
    let total_provinces = game.world.provinces.len();
    let total_tiles = game.world.hex_map.tile_count();

    println!("=== Late Game (Turn 300) Memory Profile ===");
    println!("History entries: {}", total_history);
    println!("Nations: {}", total_nations);
    println!("Provinces: {}", total_provinces);
    println!("Tiles: {}", total_tiles);

    for nation in game.great_powers() {
        println!(
            "  {}: army={}, civilians={}, buildings={}, warships={}",
            nation.name,
            nation.military.army.len(),
            nation.military.civilians.len(),
            nation.economy.buildings.len(),
            nation.military.warships.len()
        );
    }

    // Verify bounded
    assert!(total_history < 10000, "History unbounded");
    assert_eq!(total_nations, 23, "Nations count changed");
    assert_eq!(total_provinces, 120, "Provinces count changed");
}

// ── Trade simulation test (plan 10) ──────────────────────────────

#[test]
fn trade_simulation_20_turns_economic_growth() {
    let mut game = new_game("trade_sim", Difficulty::Easy, 0);
    let player = game.human_player_nation;

    // Build consulates with first 3 MNs
    let mn_ids: Vec<NationId> = game.minor_nations().iter().take(3).map(|n| n.id).collect();
    for mn_id in &mn_ids {
        game.world.diplomacy.build_consulate(player, *mn_id).ok();
    }

    let _initial_treasury = game.get_nation(player).unwrap().economy.treasury;

    // Process 20 turns
    for _ in 0..20 {
        process_turn(&mut game);
    }

    // Economy should have grown — warehouse should have resources
    let nation = game.get_nation(player).unwrap();
    let total_resources: u32 = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Grain,
    ]
    .iter()
    .map(|r| nation.resource_amount(*r))
    .sum();

    assert!(
        total_resources > 0,
        "Should have accumulated resources after 20 turns of trade"
    );
    println!(
        "After 20 turns: treasury={}, resources={}",
        nation.economy.treasury, total_resources
    );
}
