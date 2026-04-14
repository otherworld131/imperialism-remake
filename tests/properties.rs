//! Property-based tests for domain invariants.
//!
//! These tests exercise domain logic across many inputs to verify that
//! key invariants always hold, without requiring an external property-testing
//! crate.

use domain::hex::HexCoord;
use domain::map::Tile;
use domain::map::{generate_map, validate_map};
use domain::types::*;

// ── Hex coordinate pixel roundtrip ──────────────────────────────

/// Test that hex coordinate conversions are invertible for a wide range of coordinates.
#[test]
fn hex_pixel_roundtrip_property() {
    let size = 32.0;
    for q in -20..=20 {
        for r in -20..=20 {
            let c = HexCoord::new(q, r);
            let (px, py) = c.to_pixel(size);
            let recovered = HexCoord::from_pixel(px, py, size);
            assert_eq!(c, recovered, "Pixel roundtrip failed for ({}, {})", q, r);
        }
    }
}

/// Test roundtrip for multiple hex sizes.
#[test]
fn hex_pixel_roundtrip_various_sizes() {
    let sizes = [1.0, 10.0, 16.0, 32.0, 64.0, 100.0, 256.0];
    for &size in &sizes {
        for q in -10..=10 {
            for r in -10..=10 {
                let c = HexCoord::new(q, r);
                let (px, py) = c.to_pixel(size);
                let recovered = HexCoord::from_pixel(px, py, size);
                assert_eq!(
                    c, recovered,
                    "Pixel roundtrip failed for ({}, {}) at size {}",
                    q, r, size
                );
            }
        }
    }
}

// ── Hex distance triangle inequality ────────────────────────────

/// Test that hex distance satisfies the triangle inequality: d(a,c) <= d(a,b) + d(b,c).
#[test]
fn hex_distance_triangle_inequality() {
    let coords = [
        HexCoord::new(0, 0),
        HexCoord::new(3, -1),
        HexCoord::new(-2, 4),
        HexCoord::new(5, 5),
        HexCoord::new(-3, -3),
        HexCoord::new(1, -5),
        HexCoord::new(10, -10),
        HexCoord::new(-7, 2),
    ];
    for a in &coords {
        for b in &coords {
            for c in &coords {
                assert!(
                    a.distance(*c) <= a.distance(*b) + b.distance(*c),
                    "Triangle inequality violated: d({},{}) = {} > d({},{}) + d({},{}) = {} + {}",
                    a,
                    c,
                    a.distance(*c),
                    a,
                    b,
                    b,
                    c,
                    a.distance(*b),
                    b.distance(*c)
                );
            }
        }
    }
}

/// Test that hex distance is always non-negative and symmetric.
#[test]
fn hex_distance_non_negative_and_symmetric() {
    for q1 in -10..=10 {
        for r1 in -10..=10 {
            let a = HexCoord::new(q1, r1);
            let b = HexCoord::new(-q1, -r1);
            assert!(a.distance(b) >= 0, "Distance should never be negative");
            assert_eq!(a.distance(b), b.distance(a), "Distance should be symmetric");
        }
    }
}

// ── Hex cube constraint ─────────────────────────────────────────

/// Test that the cube constraint q + r + s = 0 holds for all HexCoords.
#[test]
fn hex_cube_constraint_always_holds() {
    for q in -50..=50 {
        for r in -50..=50 {
            let c = HexCoord::new(q, r);
            assert_eq!(
                c.q + c.r + c.s(),
                0,
                "Cube constraint violated for ({}, {}): q + r + s = {}",
                q,
                r,
                c.q + c.r + c.s()
            );
        }
    }
}

// ── Hex distance to self is always zero ─────────────────────────

#[test]
fn hex_distance_to_self_always_zero() {
    for q in -20..=20 {
        for r in -20..=20 {
            let c = HexCoord::new(q, r);
            assert_eq!(
                c.distance(c),
                0,
                "Distance to self should be 0 for ({}, {})",
                q,
                r
            );
        }
    }
}

// ── Map generation always produces valid maps ───────────────────

/// Test that map generation always produces valid maps regardless of seed string.
#[test]
fn map_generation_always_valid() {
    let keys = [
        "a",
        "b",
        "test",
        "hello world",
        "12345",
        "!@#$%",
        "very_long_key_that_is_32_chars!!",
        "",
        "unicode: hello",
        "spaces   lots   of   spaces",
        "newline\nembedded",
    ];
    for key in &keys {
        let map = generate_map(key);
        assert_eq!(
            map.great_power_nations.len(),
            7,
            "Key '{}' produced wrong GP count",
            key
        );
        assert_eq!(
            map.minor_nations.len(),
            16,
            "Key '{}' produced wrong MN count",
            key
        );
        assert_eq!(
            map.provinces.len(),
            120,
            "Key '{}' produced wrong province count",
            key
        );
        for gp in &map.great_power_nations {
            assert_eq!(
                gp.province_ids.len(),
                8,
                "GP {} (key '{}') has wrong province count",
                gp.name,
                key
            );
        }
        for mn in &map.minor_nations {
            assert_eq!(
                mn.province_ids.len(),
                4,
                "MN {} (key '{}') has wrong province count",
                mn.name,
                key
            );
        }
        // Every province must have at least one tile.
        for province in &map.provinces {
            assert!(
                !province.tiles.is_empty(),
                "Province {} (key '{}') has no tiles",
                province.name,
                key
            );
        }
        // Map must have both sea and land tiles.
        assert!(map.hex_map.tile_count() > 0, "Map has no tiles");
    }
}

// ── Money arithmetic safety ─────────────────────────────────────

/// Test that Money arithmetic never silently overflows for reasonable game values.
#[test]
fn money_arithmetic_safety() {
    let amounts: Vec<i64> = vec![0, 1, 100, 1000, 10000, 100000, 1000000];
    for &a in &amounts {
        for &b in &amounts {
            let ma = Money::dollars(a);
            let mb = Money::dollars(b);
            let sum = ma + mb;
            assert_eq!(
                sum.as_dollars(),
                a + b,
                "Money addition failed: {} + {} = {} (expected {})",
                a,
                b,
                sum.as_dollars(),
                a + b
            );
        }
    }
}

/// Test that Money subtraction produces correct results including negatives.
#[test]
fn money_subtraction_consistency() {
    let amounts: Vec<i64> = vec![0, 1, 50, 100, 500, 1000, 5000];
    for &a in &amounts {
        for &b in &amounts {
            let ma = Money::dollars(a);
            let mb = Money::dollars(b);
            let diff = ma - mb;
            assert_eq!(
                diff.as_dollars(),
                a - b,
                "Money subtraction failed: {} - {} = {} (expected {})",
                a,
                b,
                diff.as_dollars(),
                a - b
            );
            if a >= b {
                assert!(!diff.is_negative());
            }
            if a < b {
                assert!(diff.is_negative());
            }
        }
    }
}

/// Test checked_sub consistency: it either returns Some (non-negative) or None.
#[test]
fn money_checked_sub_consistency() {
    let amounts: Vec<i64> = vec![0, 1, 10, 100, 1000, 10000];
    for &a in &amounts {
        for &b in &amounts {
            let ma = Money::dollars(a);
            let mb = Money::dollars(b);
            match ma.checked_sub(mb) {
                Some(result) => {
                    assert!(
                        !result.is_negative(),
                        "checked_sub returned Some but result is negative: {a} - {b}"
                    );
                    assert_eq!(result.as_dollars(), a - b);
                }
                None => {
                    assert!(a < b, "checked_sub returned None but a >= b: {a} >= {b}");
                }
            }
        }
    }
}

// ── TurnNumber consistency ──────────────────────────────────────

/// Test that TurnNumber year/quarter is always consistent and reconstructable.
#[test]
fn turn_number_consistency() {
    for turn in 1..=500 {
        let t = TurnNumber::new(turn);
        let reconstructed = TurnNumber::from_year_quarter(t.year(), t.quarter());
        assert_eq!(
            t, reconstructed,
            "TurnNumber roundtrip failed for turn {}",
            turn
        );
        assert!(
            t.quarter() >= 1 && t.quarter() <= 4,
            "Quarter out of range for turn {}: {}",
            turn,
            t.quarter()
        );
        assert!(
            t.year() >= 1815,
            "Year out of range for turn {}: {}",
            turn,
            t.year()
        );
    }
}

/// Test that consecutive turns advance year/quarter correctly.
#[test]
fn turn_number_sequential_advancement() {
    let mut prev = TurnNumber::new(1);
    for turn_num in 2..=401 {
        let current = TurnNumber::new(turn_num);
        // The current turn should be exactly one after the previous.
        assert_eq!(prev.next(), current);

        // Quarter should cycle 1 -> 2 -> 3 -> 4 -> 1.
        let expected_quarter = if prev.quarter() == 4 {
            1
        } else {
            prev.quarter() + 1
        };
        assert_eq!(
            current.quarter(),
            expected_quarter,
            "Quarter did not advance correctly from turn {} to {}",
            turn_num - 1,
            turn_num
        );

        // Year should increment when crossing Q4 -> Q1.
        if prev.quarter() == 4 {
            assert_eq!(current.year(), prev.year() + 1);
        } else {
            assert_eq!(current.year(), prev.year());
        }

        prev = current;
    }
}

// ── Terrain yields never negative ───────────────────────────────

/// Test that tiles with resources produce valid yields (quantity > 0 when they yield).
#[test]
fn terrain_yields_never_negative() {
    let terrain_resource_pairs = [
        (TerrainType::Grassland, ResourceType::Grain),
        (TerrainType::Grassland, ResourceType::Fruit),
        (TerrainType::Grassland, ResourceType::Cotton),
        (TerrainType::Grassland, ResourceType::Livestock),
        (TerrainType::Hills, ResourceType::Wool),
        (TerrainType::Forest, ResourceType::Timber),
    ];
    for (terrain, resource) in &terrain_resource_pairs {
        for level in 0..=3 {
            let mut tile = Tile::with_resource(*terrain, *resource);
            tile.set_improvement_level(level);
            if let Some(yield_amt) = tile.calculate_yield() {
                assert!(
                    yield_amt.quantity > 0,
                    "Terrain {:?} with {:?} at level {} produced 0 yield",
                    terrain,
                    resource,
                    level
                );
            }
        }
    }
}

/// Test that mining terrains with deposits always produce positive yields at appropriate levels.
#[test]
fn mining_yields_positive_when_improved() {
    let mining_terrains = [TerrainType::Hills, TerrainType::Mountain];
    let deposits = [
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Gems,
    ];

    for terrain in &mining_terrains {
        for deposit in &deposits {
            let mut tile = Tile::new(*terrain);
            tile.reveal_deposit(*deposit);

            for level in 1..=3u8 {
                tile.set_improvement_level(level);
                let yield_amt = tile.calculate_yield();
                assert!(
                    yield_amt.is_some(),
                    "Mining terrain {:?} with {:?} at level {} should produce something",
                    terrain,
                    deposit,
                    level
                );
                if let Some(y) = yield_amt {
                    assert!(
                        y.quantity > 0,
                        "Mining terrain {:?} with {:?} at level {} produced 0 yield",
                        terrain,
                        deposit,
                        level
                    );
                }
            }
        }
    }
}

/// Test that improvement levels are always clamped to the resource's max.
#[test]
fn improvement_level_clamped_to_max() {
    let all_terrains = [
        TerrainType::Grassland,
        TerrainType::Hills,
        TerrainType::Forest,
        TerrainType::Mountain,
        TerrainType::Desert,
        TerrainType::Swamp,
        TerrainType::Tundra,
        TerrainType::Sea,
    ];

    for terrain in &all_terrains {
        // Tile without resource: improvement should stay at 0
        let mut tile = Tile::new(*terrain);
        tile.set_improvement_level(255);
        assert_eq!(
            tile.improvement_level(),
            0,
            "Terrain {:?} without resource: improvement should be clamped to 0",
            terrain
        );

        // Tile with resource: improvement should be clamped to resource max
        if terrain.is_land() {
            let mut tile = Tile::with_resource(*terrain, ResourceType::Grain);
            tile.set_improvement_level(255);
            assert!(
                tile.improvement_level() <= ResourceType::Grain.max_improvement_level(),
                "Terrain {:?} with Grain: improvement level {} exceeds max {}",
                terrain,
                tile.improvement_level(),
                ResourceType::Grain.max_improvement_level()
            );
        }
    }
}

// ── Hex ring and range sizes ────────────────────────────────────

/// Test that hex rings always have the correct number of elements.
#[test]
fn hex_ring_size_formula() {
    let center = HexCoord::new(0, 0);
    for r in 1..=20 {
        let ring = center.ring(r);
        assert_eq!(
            ring.len(),
            6 * r as usize,
            "Ring of radius {} should have {} elements but has {}",
            r,
            6 * r,
            ring.len()
        );
    }
}

/// Test that hex range (filled circle) always has the correct number of elements.
#[test]
fn hex_range_size_formula() {
    let center = HexCoord::new(0, 0);
    for r in 1..=15 {
        let range = center.range(r);
        // Range excludes self, so count = 3*r*(r+1)
        let expected = 3 * r as usize * (r as usize + 1);
        assert_eq!(
            range.len(),
            expected,
            "Range of radius {} should have {} elements but has {}",
            r,
            expected,
            range.len()
        );
    }
}

// ── Hex neighbors always at distance 1 ──────────────────────────

#[test]
fn hex_neighbors_always_distance_one() {
    for q in -10..=10 {
        for r in -10..=10 {
            let c = HexCoord::new(q, r);
            for n in c.neighbors() {
                assert_eq!(
                    c.distance(n),
                    1,
                    "Neighbor {} of {} should be at distance 1",
                    n,
                    c
                );
            }
        }
    }
}

// ── ResourceType properties ─────────────────────────────────────

// ── Map invariants for many random seeds ──────────────────────

/// Test that map validation passes for 50 random seeds.
#[test]
fn map_invariants_hold_for_50_random_seeds() {
    for i in 0..50 {
        let key = format!("prop_test_{}", i);
        let map = generate_map(&key);
        let result = validate_map(&map);
        assert!(
            result.is_ok(),
            "Map '{}' failed validation: {:?}",
            key,
            result.err()
        );
    }
}

// ── Starting condition viability ──────────────────────────────

/// Test that each Great Power starts with food resources (grain, fruit, or cotton tiles).
#[test]
fn each_great_power_has_food_tiles() {
    let seeds = ["food_check_a", "food_check_b", "food_check_c"];
    for seed in &seeds {
        let map = generate_map(seed);
        for gp in &map.great_power_nations {
            let mut has_food = false;
            for pid in &gp.province_ids {
                let province = map.provinces.iter().find(|p| p.id == *pid).unwrap();
                for &tile_coord in &province.tiles {
                    if let Some(tile) = map.hex_map.get_tile(tile_coord) {
                        if let Some(resource) = tile.resource_deposit() {
                            if matches!(
                                resource,
                                ResourceType::Grain | ResourceType::Fruit | ResourceType::Livestock
                            ) {
                                has_food = true;
                                break;
                            }
                        }
                    }
                }
                if has_food {
                    break;
                }
            }
            assert!(
                has_food,
                "Great Power {} (seed '{}') has no food-producing tiles",
                gp.name, seed
            );
        }
    }
}

/// Test that each Great Power starts with timber (forest tiles with timber resource).
#[test]
fn each_great_power_has_timber_tiles() {
    let seeds = ["timber_check_a", "timber_check_b", "timber_check_c"];
    for seed in &seeds {
        let map = generate_map(seed);
        for gp in &map.great_power_nations {
            let mut has_timber = false;
            for pid in &gp.province_ids {
                let province = map.provinces.iter().find(|p| p.id == *pid).unwrap();
                for &tile_coord in &province.tiles {
                    if let Some(tile) = map.hex_map.get_tile(tile_coord) {
                        if tile.resource_deposit() == Some(ResourceType::Timber) {
                            has_timber = true;
                            break;
                        }
                    }
                }
                if has_timber {
                    break;
                }
            }
            assert!(
                has_timber,
                "Great Power {} (seed '{}') has no timber-producing tiles",
                gp.name, seed
            );
        }
    }
}

/// Test that each Great Power starts with potential minerals (hills or mountains).
#[test]
fn each_great_power_has_mineral_potential() {
    let seeds = ["mineral_check_a", "mineral_check_b", "mineral_check_c"];
    for seed in &seeds {
        let map = generate_map(seed);
        for gp in &map.great_power_nations {
            let mut has_minerals = false;
            for pid in &gp.province_ids {
                let province = map.provinces.iter().find(|p| p.id == *pid).unwrap();
                for &tile_coord in &province.tiles {
                    if let Some(tile) = map.hex_map.get_tile(tile_coord) {
                        let terrain = tile.terrain();
                        if matches!(terrain, TerrainType::Hills | TerrainType::Mountain) {
                            has_minerals = true;
                            break;
                        }
                    }
                }
                if has_minerals {
                    break;
                }
            }
            assert!(
                has_minerals,
                "Great Power {} (seed '{}') has no hills/mountains for mineral potential",
                gp.name, seed
            );
        }
    }
}

// ── Production conservation laws ──────────────────────────────

/// Test that mill production output never exceeds input resources.
/// For Timber: 2 timber -> 1 lumber, so output <= input / 2
#[test]
fn production_output_never_exceeds_input() {
    use domain::economy::production::{ProductionChain, calculate_mill_production};

    for input in 1..=20u32 {
        let resources = vec![(ResourceType::Timber, input)];
        let result = calculate_mill_production(ProductionChain::Timber, &resources, 100, 100);
        let output: u32 = result.materials_produced.iter().map(|(_, q)| q).sum();
        assert!(
            output <= input,
            "Mill output {} exceeds input {} for Timber chain",
            output,
            input
        );
    }

    // Metal chain: 1 coal + 1 iron -> 1 steel, output <= min(coal, iron)
    for input in 1..=20u32 {
        let resources = vec![(ResourceType::Coal, input), (ResourceType::Iron, input)];
        let result = calculate_mill_production(ProductionChain::Metal, &resources, 100, 100);
        let output: u32 = result.materials_produced.iter().map(|(_, q)| q).sum();
        assert!(
            output <= input,
            "Mill output {} exceeds input {} for Metal chain",
            output,
            input
        );
    }

    // Textile chain: 2 cotton -> 1 fabric, output <= input / 2
    for input in 1..=20u32 {
        let resources = vec![(ResourceType::Cotton, input)];
        let result = calculate_mill_production(ProductionChain::Textile, &resources, 100, 100);
        let output: u32 = result.materials_produced.iter().map(|(_, q)| q).sum();
        assert!(
            output <= input,
            "Mill output {} exceeds input {} for Textile chain",
            output,
            input
        );
    }
}

/// Test that combat damage never exceeds total health of all engaged units.
#[test]
fn combat_damage_never_exceeds_total_health() {
    use domain::map::UnitId;
    use domain::military::combat::{CombatForce, resolve_battle};
    use domain::military::units::{ArmyUnit, ArmyUnitType};

    let attacker = CombatForce {
        nation: NationId(1),
        units: vec![
            ArmyUnit::new(
                UnitId(1),
                ArmyUnitType::Regulars,
                NationId(1),
                ProvinceId(1),
            ),
            ArmyUnit::new(UnitId(2), ArmyUnitType::Guards, NationId(1), ProvinceId(1)),
        ],
    };
    let defender = CombatForce {
        nation: NationId(2),
        units: vec![
            ArmyUnit::new(
                UnitId(3),
                ArmyUnitType::Regulars,
                NationId(2),
                ProvinceId(2),
            ),
            ArmyUnit::new(UnitId(4), ArmyUnitType::Militia, NationId(2), ProvinceId(2)),
        ],
    };

    let initial_atk_health: u32 = attacker.units.iter().map(|u| u.health as u32).sum();
    let initial_def_health: u32 = defender.units.iter().map(|u| u.health as u32).sum();
    let total_initial_health = initial_atk_health + initial_def_health;

    let result = resolve_battle(&attacker, &defender, ProvinceId(2), None, 0);

    // Total casualties (destroyed units) should not exceed total units
    let total_casualties = result.attacker_casualties.len() + result.defender_casualties.len();
    let total_units = attacker.units.len() + defender.units.len();
    assert!(
        total_casualties <= total_units,
        "Total casualties ({}) exceed total initial units ({})",
        total_casualties,
        total_units
    );

    // Total remaining health should not exceed total initial health
    let remaining_atk_health: u32 = result
        .attacker_survivors
        .iter()
        .map(|u| u.health as u32)
        .sum();
    let remaining_def_health: u32 = result
        .defender_survivors
        .iter()
        .map(|u| u.health as u32)
        .sum();
    let total_remaining_health = remaining_atk_health + remaining_def_health;
    assert!(
        total_remaining_health <= total_initial_health,
        "Total remaining health ({}) exceeds total initial health ({})",
        total_remaining_health,
        total_initial_health
    );
}

/// Test that tradeable/monetary classifications are consistent.
#[test]
fn resource_type_classification_consistency() {
    let all_resources = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Grain,
        ResourceType::Fruit,
        ResourceType::Livestock,
        ResourceType::Horses,
        ResourceType::Oil,
        ResourceType::Gold,
        ResourceType::Gems,
    ];

    // Monetary resources should always be tradeable.
    for resource in &all_resources {
        if resource.is_monetary() {
            assert!(
                resource.is_tradeable(),
                "Monetary resource {:?} should be tradeable",
                resource
            );
        }
    }

    // Non-tradeable resources should never be monetary.
    for resource in &all_resources {
        if !resource.is_tradeable() {
            assert!(
                !resource.is_monetary(),
                "Non-tradeable resource {:?} should not be monetary",
                resource
            );
        }
    }
}
