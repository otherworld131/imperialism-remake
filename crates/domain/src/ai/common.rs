use crate::game_state::GameState;
#[cfg(test)]
use crate::map::UnitId;
use crate::types::*;
#[cfg(test)]
use std::sync::atomic::{AtomicU32, Ordering};

/// AI personality types that affect decision-making priorities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AiPersonality {
    /// Prioritizes military, declares wars early.
    Aggressive,
    /// Prioritizes trade and alliances, avoids war.
    Diplomatic,
    /// Prioritizes production and tech investment.
    Economic,
    /// Default: adapts to circumstances.
    Balanced,
}

impl std::fmt::Display for AiPersonality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiPersonality::Aggressive => write!(f, "Aggressive"),
            AiPersonality::Diplomatic => write!(f, "Diplomatic"),
            AiPersonality::Economic => write!(f, "Economic"),
            AiPersonality::Balanced => write!(f, "Balanced"),
        }
    }
}

/// Returns the default AI personality for a Great Power based on nation index.
///
/// - Index 0: Balanced (usually human-controlled)
/// - Index 1: Aggressive
/// - Index 2: Economic
/// - Index 3: Balanced
/// - Index 4: Diplomatic
/// - Index 5: Aggressive
/// - Index 6: Balanced
pub fn personality_for_nation_index(index: usize) -> AiPersonality {
    match index {
        0 => AiPersonality::Balanced,
        1 => AiPersonality::Aggressive,
        2 => AiPersonality::Economic,
        3 => AiPersonality::Balanced,
        4 => AiPersonality::Diplomatic,
        5 => AiPersonality::Aggressive,
        6 => AiPersonality::Balanced,
        _ => AiPersonality::Balanced,
    }
}

const ALL_PERSONALITIES: [AiPersonality; 4] = [
    AiPersonality::Aggressive,
    AiPersonality::Diplomatic,
    AiPersonality::Economic,
    AiPersonality::Balanced,
];

/// Generate random AI personalities for `count` nations using a deterministic seed.
/// Each slot independently picks from the four personality types.
pub fn random_personalities(seed: u64, count: usize) -> Vec<AiPersonality> {
    let mut state = seed.max(1);
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        // xorshift64
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let idx = (state as usize) % ALL_PERSONALITIES.len();
        result.push(ALL_PERSONALITIES[idx]);
    }
    result
}

/// Global counter for generating unique UnitIds for AI-built army units.
/// Only used in test helpers; production code uses `GameState::alloc_unit_id`.
#[cfg(test)]
static AI_UNIT_ID_COUNTER: AtomicU32 = AtomicU32::new(2_000_000);

/// Generate a unique UnitId for an AI-built unit.
/// Only used in test helpers; production code uses `GameState::alloc_unit_id`.
#[cfg(test)]
pub fn next_unit_id() -> UnitId {
    UnitId(AI_UNIT_ID_COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Get the AI personality for a nation, defaulting to Balanced.
pub(crate) fn get_personality(game: &GameState, nation_id: NationId) -> AiPersonality {
    game.get_nation(nation_id)
        .and_then(|n| n.ai_personality)
        .unwrap_or(AiPersonality::Balanced)
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;
    use crate::data::GameData;
    use crate::diplomacy::DiplomacyState;
    use crate::economy::civilians::{Civilian, CivilianType};
    use crate::hex::HexCoord;
    use crate::map::{HexMap, Province};
    use crate::nation::{Nation, NationColor};

    /// Build a game state with a human nation and one AI great power.
    pub(crate) fn test_game_with_ai() -> GameState {
        let coord = HexCoord::new(0, 0);
        let hex_map = HexMap::new(10, 10);

        let province1 = Province::new(
            ProvinceId(1),
            "Human Land".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "AI Land".to_string(),
            NationId(2),
            HexCoord::new(3, 3),
            vec![HexCoord::new(3, 3)],
            4,
        );

        let mut human_nation = Nation::new(
            NationId(1),
            "HumanNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        human_nation.treasury = Money::dollars(10000);

        let mut ai_nation = Nation::new(
            NationId(2),
            "AINation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        ai_nation.treasury = Money::dollars(10000);
        // Pre-populate with 4 civilians so AI does not hire more during tests
        for i in 0..4 {
            ai_nation.civilians.push(Civilian::new(
                UnitId(10000 + i),
                CivilianType::Farmer,
                NationId(2),
            ));
        }

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2],
            nations: vec![human_nation, ai_nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            political_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
            last_cash_flow: std::collections::HashMap::new(),
            last_resource_flow: std::collections::HashMap::new(),
            pending_ai_cash_spending: Vec::new(),
            pending_ai_cash_income: Vec::new(),
        };
        crate::military::combat::seed_militia_from_garrison_count(&mut game);
        game
    }

    /// Build a game state that includes a minor nation for war tests.
    pub(crate) fn test_game_with_ai_and_minor() -> GameState {
        let coord = HexCoord::new(0, 0);
        let mut hex_map = HexMap::new(10, 10);

        // Configure tiles so provinces 2 and 3 are adjacent (needed for
        // reachability checks in war-declaration and attack-targeting logic).
        hex_map.set_tile(
            HexCoord::new(3, 3),
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        hex_map.set_tile(
            HexCoord::new(4, 3),
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(3)),
        );

        let province1 = Province::new(
            ProvinceId(1),
            "Human Land".to_string(),
            NationId(1),
            coord,
            vec![coord],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "AI Land".to_string(),
            NationId(2),
            HexCoord::new(3, 3),
            vec![HexCoord::new(3, 3)],
            4,
        );
        let province3 = Province::new(
            ProvinceId(3),
            "Minor Capital".to_string(),
            NationId(3),
            HexCoord::new(4, 3),
            vec![HexCoord::new(4, 3)],
            3,
        );

        let mut human_nation = Nation::new(
            NationId(1),
            "HumanNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        human_nation.treasury = Money::dollars(10000);

        let mut ai_nation = Nation::new(
            NationId(2),
            "AINation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        ai_nation.treasury = Money::dollars(10000);
        // Pre-populate with 4 civilians so AI does not hire more during tests
        for i in 0..4 {
            ai_nation.civilians.push(Civilian::new(
                UnitId(10000 + i),
                CivilianType::Farmer,
                NationId(2),
            ));
        }

        let minor_nation = Nation::new(
            NationId(3),
            "MinorLand".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(3),
        );

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2, province3],
            nations: vec![human_nation, ai_nation, minor_nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            political_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
            last_cash_flow: std::collections::HashMap::new(),
            last_resource_flow: std::collections::HashMap::new(),
            pending_ai_cash_spending: Vec::new(),
            pending_ai_cash_income: Vec::new(),
        };
        crate::military::combat::seed_militia_from_garrison_count(&mut game);
        game
    }

    /// Build a game state with two adjacent provinces for border tests.
    pub(crate) fn test_game_with_adjacent_provinces() -> GameState {
        let mut hex_map = HexMap::new(20, 20);

        // AI province tiles: (0,0) and (1,0)
        let ai_tile1 = HexCoord::new(0, 0);
        let ai_tile2 = HexCoord::new(1, 0);
        hex_map.set_tile(
            ai_tile1,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );
        hex_map.set_tile(
            ai_tile2,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(2)),
        );

        // Enemy province tile: (2,0) — adjacent to (1,0)
        let enemy_tile = HexCoord::new(2, 0);
        hex_map.set_tile(
            enemy_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(3)),
        );

        // Human province tile
        let human_tile = HexCoord::new(5, 5);
        hex_map.set_tile(
            human_tile,
            crate::map::tile::Tile::with_province(TerrainType::Grassland, ProvinceId(1)),
        );

        let province1 = Province::new(
            ProvinceId(1),
            "Human Land".to_string(),
            NationId(1),
            human_tile,
            vec![human_tile],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "AI Land".to_string(),
            NationId(2),
            ai_tile1,
            vec![ai_tile1, ai_tile2],
            4,
        );
        let province3 = Province::new(
            ProvinceId(3),
            "Enemy Land".to_string(),
            NationId(3),
            enemy_tile,
            vec![enemy_tile],
            3,
        );

        let mut human_nation = Nation::new(
            NationId(1),
            "HumanNation".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        human_nation.treasury = Money::dollars(10000);

        let mut ai_nation = Nation::new(
            NationId(2),
            "AINation".to_string(),
            NationColor::Red,
            NationType::GreatPower,
            ProvinceId(2),
        );
        ai_nation.treasury = Money::dollars(20000);
        ai_nation.ai_personality = Some(AiPersonality::Balanced);
        // Pre-populate with 4 civilians
        for i in 0..4 {
            ai_nation.civilians.push(Civilian::new(
                UnitId(10000 + i),
                CivilianType::Farmer,
                NationId(2),
            ));
        }

        let enemy_nation = Nation::new(
            NationId(3),
            "EnemyLand".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(3),
        );

        let mut diplomacy = DiplomacyState::new();
        // Declare war between AI and enemy
        diplomacy.declare_war(NationId(2), NationId(3));

        let mut game = GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "test".to_string(),
            hex_map,
            provinces: vec![province1, province2, province3],
            nations: vec![human_nation, ai_nation, enemy_nation],
            human_player_nation: NationId(1),
            events: Vec::new(),
            game_data: GameData::default(),
            diplomacy,
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            pending_landings: Vec::new(),
            history: Vec::new(),
            high_scores: Vec::new(),
            newspaper_archive: Vec::new(),
            battle_archive: Vec::new(),
            political_archive: Vec::new(),
            ai_debug: false,
            observer_mode: false,
            last_cash_flow: std::collections::HashMap::new(),
            last_resource_flow: std::collections::HashMap::new(),
            pending_ai_cash_spending: Vec::new(),
            pending_ai_cash_income: Vec::new(),
        };
        crate::military::combat::seed_militia_from_garrison_count(&mut game);
        game
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personality_assignment_is_deterministic() {
        assert_eq!(personality_for_nation_index(0), AiPersonality::Balanced);
        assert_eq!(personality_for_nation_index(1), AiPersonality::Aggressive);
        assert_eq!(personality_for_nation_index(2), AiPersonality::Economic);
        assert_eq!(personality_for_nation_index(3), AiPersonality::Balanced);
        assert_eq!(personality_for_nation_index(4), AiPersonality::Diplomatic);
        assert_eq!(personality_for_nation_index(5), AiPersonality::Aggressive);
        assert_eq!(personality_for_nation_index(6), AiPersonality::Balanced);
        // Out-of-range defaults to Balanced
        assert_eq!(personality_for_nation_index(99), AiPersonality::Balanced);
    }

    #[test]
    fn personality_display_format() {
        assert_eq!(format!("{}", AiPersonality::Aggressive), "Aggressive");
        assert_eq!(format!("{}", AiPersonality::Diplomatic), "Diplomatic");
        assert_eq!(format!("{}", AiPersonality::Economic), "Economic");
        assert_eq!(format!("{}", AiPersonality::Balanced), "Balanced");
    }

    #[test]
    fn new_game_assigns_ai_personalities() {
        let gs = crate::game_state::new_game("test", Difficulty::Normal, 0);
        // Human player (index 0, Deneb) should have no personality
        let human = gs.get_nation(gs.human_player_nation).unwrap();
        assert_eq!(
            human.ai_personality, None,
            "Human player should not have an AI personality"
        );

        // Other Great Powers should have personalities
        for nation in gs.great_powers() {
            if nation.id == gs.human_player_nation {
                continue;
            }
            assert!(
                nation.ai_personality.is_some(),
                "AI Great Power {} should have a personality",
                nation.name
            );
        }
    }

    #[test]
    fn random_personalities_deterministic() {
        let a = random_personalities(42, 6);
        let b = random_personalities(42, 6);
        assert_eq!(a, b, "Same seed should produce same result");
    }

    #[test]
    fn random_personalities_different_seeds_differ() {
        let a = random_personalities(1, 6);
        let b = random_personalities(2, 6);
        assert_ne!(
            a, b,
            "Different seeds should usually produce different results"
        );
    }

    #[test]
    fn random_personalities_correct_length() {
        assert_eq!(random_personalities(99, 0).len(), 0);
        assert_eq!(random_personalities(99, 3).len(), 3);
        assert_eq!(random_personalities(99, 6).len(), 6);
    }

    #[test]
    fn random_personalities_produces_variety_across_seeds() {
        let mut seen = std::collections::HashSet::new();
        for seed in 0..100 {
            for p in random_personalities(seed, 6) {
                seen.insert(p);
            }
        }
        assert_eq!(
            seen.len(),
            4,
            "All 4 personality types should appear across 100 seeds"
        );
    }
}
