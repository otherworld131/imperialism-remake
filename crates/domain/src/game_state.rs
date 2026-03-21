use crate::ai::basic::personality_for_nation_index;
use crate::diplomacy::DiplomacyState;
use crate::economy::buildings::{Building, BuildingType};
use crate::economy::civilians::{Civilian, CivilianType, next_civilian_id};
use crate::events::DomainEvent;
use crate::map::{HexMap, Province, UnitId};
use crate::military::ships::{Ship, ShipType};
use crate::nation::{Nation, NationColor};
use crate::tech::TechTree;
use crate::types::*;

/// Top-level aggregate root representing the complete state of a game.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct GameState {
    /// Current turn number.
    pub turn: TurnNumber,
    /// Difficulty setting for this game.
    pub difficulty: Difficulty,
    /// Key identifying which map is loaded.
    pub map_key: String,
    /// The hex map containing all tiles.
    pub hex_map: HexMap,
    /// All provinces in the game.
    pub provinces: Vec<Province>,
    /// All nations in the game (Great Powers + Minor Nations).
    pub nations: Vec<Nation>,
    /// The NationId of the human player's nation.
    pub human_player_nation: NationId,
    /// Event log for the current turn (transient, not saved).
    #[serde(skip)]
    pub events: Vec<DomainEvent>,
    /// The technology tree for this game (reconstructed on load).
    #[serde(skip, default = "TechTree::default")]
    pub tech_tree: TechTree,
    /// Diplomatic relations and standing between nations.
    pub diplomacy: DiplomacyState,
    /// Pending attacks to resolve this turn: (attacker NationId, target ProvinceId).
    #[serde(default)]
    pub pending_attacks: Vec<(NationId, ProvinceId)>,
    /// Pending unit movements to resolve this turn: (nation, unit_id, destination province).
    #[serde(default)]
    pub pending_moves: Vec<(NationId, crate::map::UnitId, ProvinceId)>,
    /// History of major game events: (turn_number, description).
    #[serde(default)]
    pub history: Vec<(TurnNumber, String)>,
}

impl GameState {
    /// Look up a nation by its ID.
    pub fn get_nation(&self, id: NationId) -> Option<&Nation> {
        self.nations.iter().find(|n| n.id == id)
    }

    /// Look up a nation by its ID (mutable).
    pub fn get_nation_mut(&mut self, id: NationId) -> Option<&mut Nation> {
        self.nations.iter_mut().find(|n| n.id == id)
    }

    /// Look up a province by its ID.
    pub fn get_province(&self, id: ProvinceId) -> Option<&Province> {
        self.provinces.iter().find(|p| p.id == id)
    }

    /// Look up a province by its ID (mutable).
    pub fn get_province_mut(&mut self, id: ProvinceId) -> Option<&mut Province> {
        self.provinces.iter_mut().find(|p| p.id == id)
    }

    /// Returns all Great Power nations.
    pub fn great_powers(&self) -> Vec<&Nation> {
        self.nations.iter().filter(|n| n.is_great_power()).collect()
    }

    /// Returns all Minor Nations.
    pub fn minor_nations(&self) -> Vec<&Nation> {
        self.nations
            .iter()
            .filter(|n| !n.is_great_power())
            .collect()
    }

    /// Advance to the next turn.
    pub fn advance_turn(&mut self) {
        self.turn = self.turn.next();
    }

    /// Record a domain event.
    pub fn push_event(&mut self, event: DomainEvent) {
        self.events.push(event);
    }

    /// Whether the game is over (turn >= 1915 Q1).
    pub fn is_game_over(&self) -> bool {
        self.turn.is_game_end() || self.turn > TurnNumber::from_year_quarter(1915, 1)
    }

    /// Find a nation by partial, case-insensitive name match.
    /// Returns `None` if no nation matches or if multiple nations match.
    pub fn find_nation_by_name(&self, partial: &str) -> Option<&Nation> {
        let lower = partial.to_lowercase();
        let matches: Vec<&Nation> = self
            .nations
            .iter()
            .filter(|n| n.name.to_lowercase().contains(&lower))
            .collect();
        if matches.len() == 1 {
            Some(matches[0])
        } else {
            None
        }
    }
}

// ── Great Power colors (matched to original game) ───────────────

const GP_COLORS: [NationColor; 7] = [
    NationColor::Yellow,    // Deneb
    NationColor::Orange,    // Devron
    NationColor::LightBlue, // Haxaco
    NationColor::Red,       // Kem
    NationColor::Green,     // Ordune
    NationColor::Purple,    // Patagon
    NationColor::Blue,      // Zimm
];

const MN_COLORS: [NationColor; 16] = [
    NationColor::Gray,
    NationColor::Brown,
    NationColor::Pink,
    NationColor::Teal,
    NationColor::Olive,
    NationColor::Maroon,
    NationColor::Navy,
    NationColor::Cyan,
    NationColor::Lime,
    NationColor::Coral,
    NationColor::Lavender,
    NationColor::Tan,
    NationColor::Salmon,
    NationColor::Khaki,
    NationColor::Indigo,
    NationColor::Gray, // reuse for 16th
];

/// Create a new game from a map key and difficulty.
/// This is the main entry point for starting a game.
pub fn new_game(map_key: &str, difficulty: Difficulty, human_nation_index: usize) -> GameState {
    let generated = crate::map::generate_map(map_key);

    let mut nations = Vec::new();

    // Create Great Power nations
    for (i, setup) in generated.great_power_nations.iter().enumerate() {
        let starting_cash = match difficulty {
            Difficulty::Introductory => Money::dollars(15000),
            Difficulty::Easy => Money::dollars(12000),
            Difficulty::Normal => Money::dollars(10000),
            Difficulty::Hard => Money::dollars(8000),
            Difficulty::NighOnImpossible => Money::dollars(5000),
        };

        let mut nation = Nation::new(
            setup.nation_id,
            setup.name.clone(),
            GP_COLORS[i],
            NationType::GreatPower,
            setup.capital_province,
        );
        nation.treasury = starting_cash;
        for pid in &setup.province_ids {
            nation.add_province(*pid);
        }

        // Starting buildings — all Great Powers get fixed buildings
        let fixed_buildings = [
            BuildingType::Armory,
            BuildingType::Capitol,
            BuildingType::FoodProcessing,
            BuildingType::Railyard,
            BuildingType::Shipyard,
            BuildingType::TradeSchool,
            BuildingType::University,
            BuildingType::Warehouse,
        ];
        for bt in &fixed_buildings {
            nation.buildings.push(Building::new(*bt, 1));
        }

        // On Easy/Introductory, add starting mills and factories
        if matches!(difficulty, Difficulty::Easy | Difficulty::Introductory) {
            nation
                .buildings
                .push(Building::new(BuildingType::LumberMill, 2));
            nation
                .buildings
                .push(Building::new(BuildingType::SteelMill, 2));
            nation
                .buildings
                .push(Building::new(BuildingType::TextileMill, 2));
            nation
                .buildings
                .push(Building::new(BuildingType::FurnitureFactory, 1));
            nation
                .buildings
                .push(Building::new(BuildingType::HardwareFactory, 1));
            nation
                .buildings
                .push(Building::new(BuildingType::ClothingFactory, 1));
        }

        // Starting workers based on difficulty
        match difficulty {
            Difficulty::Introductory | Difficulty::Easy => {
                nation.labor.untrained = 5;
                nation.labor.trained = 3;
            }
            Difficulty::Normal => {
                nation.labor.untrained = 3;
                nation.labor.trained = 1;
            }
            Difficulty::Hard | Difficulty::NighOnImpossible => {
                nation.labor.untrained = 2;
            }
        }

        // Starting civilians: 1 Farmer + 1 Forester for each Great Power
        let farmer = Civilian::new(next_civilian_id(), CivilianType::Farmer, setup.nation_id);
        let forester = Civilian::new(next_civilian_id(), CivilianType::Forester, setup.nation_id);
        nation.civilians.push(farmer);
        nation.civilians.push(forester);

        // Starting merchant fleet: 1 Trader for each Great Power
        let trader = Ship::new(
            UnitId(1_500_000 + i as u32),
            ShipType::Trader,
            setup.nation_id,
        );
        nation.merchant_fleet.push(trader);

        // Assign AI personality for non-human Great Powers
        if i != human_nation_index.min(generated.great_power_nations.len() - 1) {
            nation.ai_personality = Some(personality_for_nation_index(i));
        }

        nations.push(nation);
    }

    // Create Minor Nations
    for (i, setup) in generated.minor_nations.iter().enumerate() {
        let mut nation = Nation::new(
            setup.nation_id,
            setup.name.clone(),
            MN_COLORS[i],
            NationType::MinorNation,
            setup.capital_province,
        );
        for pid in &setup.province_ids {
            nation.add_province(*pid);
        }
        nations.push(nation);
    }

    let human_nation_id = generated.great_power_nations
        [human_nation_index.min(generated.great_power_nations.len() - 1)]
    .nation_id;

    let mut diplomacy = DiplomacyState::new();
    let gp_ids: Vec<NationId> = nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();
    diplomacy.initialize_great_powers(&gp_ids);

    GameState {
        turn: TurnNumber::new(1),
        difficulty,
        map_key: map_key.to_string(),
        hex_map: generated.hex_map,
        provinces: generated.provinces,
        nations,
        human_player_nation: human_nation_id,
        events: Vec::new(),
        tech_tree: TechTree::new(),
        diplomacy,
        pending_attacks: Vec::new(),
        pending_moves: Vec::new(),
        history: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::TurnStarted;
    use crate::hex::HexCoord;
    use crate::nation::NationColor;

    /// Helper: build a minimal GameState for testing.
    fn sample_game_state() -> GameState {
        let capital_tile = HexCoord::new(0, 0);

        let province1 = Province::new(
            ProvinceId(1),
            "France City".to_string(),
            NationId(1),
            capital_tile,
            vec![capital_tile],
            4,
        );
        let province2 = Province::new(
            ProvinceId(2),
            "Bavaria".to_string(),
            NationId(2),
            HexCoord::new(3, 3),
            vec![HexCoord::new(3, 3)],
            3,
        );

        let nation1 = Nation::new(
            NationId(1),
            "France".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(1),
        );
        let nation2 = Nation::new(
            NationId(2),
            "Bavaria".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(2),
        );

        GameState {
            turn: TurnNumber::new(1),
            difficulty: Difficulty::Normal,
            map_key: "europe".to_string(),
            hex_map: HexMap::new(10, 10),
            provinces: vec![province1, province2],
            nations: vec![nation1, nation2],
            human_player_nation: NationId(1),
            events: Vec::new(),
            tech_tree: TechTree::new(),
            diplomacy: DiplomacyState::new(),
            pending_attacks: Vec::new(),
            pending_moves: Vec::new(),
            history: Vec::new(),
        }
    }

    #[test]
    fn new_game_creates_valid_state() {
        let gs = new_game("test", Difficulty::Normal, 0);
        assert_eq!(gs.great_powers().len(), 7);
        assert_eq!(gs.minor_nations().len(), 16);
        assert_eq!(gs.provinces.len(), 120);
        assert_eq!(gs.turn, TurnNumber::new(1));
        assert!(!gs.is_game_over());
    }

    #[test]
    fn new_game_starting_treasury_varies_by_difficulty() {
        let intro = new_game("test", Difficulty::Introductory, 0);
        let easy = new_game("test", Difficulty::Easy, 0);
        let normal = new_game("test", Difficulty::Normal, 0);
        let hard = new_game("test", Difficulty::Hard, 0);
        let noi = new_game("test", Difficulty::NighOnImpossible, 0);

        let intro_cash = intro
            .get_nation(intro.human_player_nation)
            .unwrap()
            .treasury;
        let easy_cash = easy.get_nation(easy.human_player_nation).unwrap().treasury;
        let normal_cash = normal
            .get_nation(normal.human_player_nation)
            .unwrap()
            .treasury;
        let hard_cash = hard.get_nation(hard.human_player_nation).unwrap().treasury;
        let noi_cash = noi.get_nation(noi.human_player_nation).unwrap().treasury;

        assert_eq!(intro_cash, Money::dollars(15000));
        assert_eq!(easy_cash, Money::dollars(12000));
        assert_eq!(normal_cash, Money::dollars(10000));
        assert_eq!(hard_cash, Money::dollars(8000));
        assert_eq!(noi_cash, Money::dollars(5000));

        assert!(intro_cash > easy_cash);
        assert!(easy_cash > normal_cash);
        assert!(normal_cash > hard_cash);
        assert!(hard_cash > noi_cash);
    }

    // ── Difficulty-specific starting conditions ───────────────

    #[test]
    fn new_game_introductory_has_extra_resources() {
        let gs = new_game("test", Difficulty::Introductory, 0);
        for nation in gs.great_powers() {
            // $15,000 starting cash
            assert_eq!(
                nation.treasury,
                Money::dollars(15000),
                "{} should start with $15,000 on Introductory",
                nation.name
            );

            // 5 untrained + 3 trained workers
            assert_eq!(
                nation.labor.untrained, 5,
                "{} should have 5 untrained workers on Introductory",
                nation.name
            );
            assert_eq!(
                nation.labor.trained, 3,
                "{} should have 3 trained workers on Introductory",
                nation.name
            );

            // Pre-built mills and factories
            assert!(
                nation.has_building(BuildingType::LumberMill),
                "{} should have a LumberMill on Introductory",
                nation.name
            );
            assert!(
                nation.has_building(BuildingType::SteelMill),
                "{} should have a SteelMill on Introductory",
                nation.name
            );
            assert!(
                nation.has_building(BuildingType::TextileMill),
                "{} should have a TextileMill on Introductory",
                nation.name
            );
            assert!(
                nation.has_building(BuildingType::FurnitureFactory),
                "{} should have a FurnitureFactory on Introductory",
                nation.name
            );
            assert!(
                nation.has_building(BuildingType::HardwareFactory),
                "{} should have a HardwareFactory on Introductory",
                nation.name
            );
            assert!(
                nation.has_building(BuildingType::ClothingFactory),
                "{} should have a ClothingFactory on Introductory",
                nation.name
            );
        }
    }

    #[test]
    fn new_game_easy_has_mills_and_factories() {
        let gs = new_game("test", Difficulty::Easy, 0);
        for nation in gs.great_powers() {
            // Easy also gets pre-built mills and factories
            assert!(
                nation.has_building(BuildingType::LumberMill),
                "{} should have a LumberMill on Easy",
                nation.name
            );
            assert!(
                nation.has_building(BuildingType::SteelMill),
                "{} should have a SteelMill on Easy",
                nation.name
            );
            assert!(
                nation.has_building(BuildingType::TextileMill),
                "{} should have a TextileMill on Easy",
                nation.name
            );
            assert!(
                nation.has_building(BuildingType::FurnitureFactory),
                "{} should have a FurnitureFactory on Easy",
                nation.name
            );

            // $12,000 starting cash
            assert_eq!(
                nation.treasury,
                Money::dollars(12000),
                "{} should start with $12,000 on Easy",
                nation.name
            );

            // 5 untrained + 3 trained workers
            assert_eq!(nation.labor.untrained, 5);
            assert_eq!(nation.labor.trained, 3);
        }
    }

    #[test]
    fn new_game_normal_has_no_mills() {
        let gs = new_game("test", Difficulty::Normal, 0);
        for nation in gs.great_powers() {
            // Normal does NOT get pre-built mills or factories
            assert!(
                !nation.has_building(BuildingType::LumberMill),
                "{} should NOT have a LumberMill on Normal",
                nation.name
            );
            assert!(
                !nation.has_building(BuildingType::SteelMill),
                "{} should NOT have a SteelMill on Normal",
                nation.name
            );
            assert!(
                !nation.has_building(BuildingType::TextileMill),
                "{} should NOT have a TextileMill on Normal",
                nation.name
            );
            assert!(
                !nation.has_building(BuildingType::FurnitureFactory),
                "{} should NOT have a FurnitureFactory on Normal",
                nation.name
            );

            // $10,000 starting cash
            assert_eq!(
                nation.treasury,
                Money::dollars(10000),
                "{} should start with $10,000 on Normal",
                nation.name
            );

            // 3 untrained + 1 trained workers
            assert_eq!(nation.labor.untrained, 3);
            assert_eq!(nation.labor.trained, 1);
        }
    }

    #[test]
    fn new_game_hard_has_less_starting_cash() {
        let gs = new_game("test", Difficulty::Hard, 0);
        for nation in gs.great_powers() {
            // $8,000 starting cash
            assert_eq!(
                nation.treasury,
                Money::dollars(8000),
                "{} should start with $8,000 on Hard",
                nation.name
            );

            // No mills or factories
            assert!(
                !nation.has_building(BuildingType::LumberMill),
                "{} should NOT have a LumberMill on Hard",
                nation.name
            );

            // Only 2 untrained workers, 0 trained
            assert_eq!(nation.labor.untrained, 2);
            assert_eq!(nation.labor.trained, 0);
        }
    }

    #[test]
    fn new_game_noi_has_minimal_resources() {
        let gs = new_game("test", Difficulty::NighOnImpossible, 0);
        for nation in gs.great_powers() {
            // $5,000 starting cash
            assert_eq!(
                nation.treasury,
                Money::dollars(5000),
                "{} should start with $5,000 on NOI",
                nation.name
            );

            // Only 2 untrained workers
            assert_eq!(
                nation.labor.untrained, 2,
                "{} should have only 2 untrained workers on NOI",
                nation.name
            );
            assert_eq!(
                nation.labor.trained, 0,
                "{} should have 0 trained workers on NOI",
                nation.name
            );

            // No mills or factories
            assert!(!nation.has_building(BuildingType::LumberMill));
            assert!(!nation.has_building(BuildingType::SteelMill));
        }
    }

    #[test]
    fn each_difficulty_starts_valid_game() {
        let difficulties = [
            Difficulty::Introductory,
            Difficulty::Easy,
            Difficulty::Normal,
            Difficulty::Hard,
            Difficulty::NighOnImpossible,
        ];
        for difficulty in &difficulties {
            let gs = new_game("test", *difficulty, 0);
            assert_eq!(
                gs.great_powers().len(),
                7,
                "Failed for difficulty {:?}",
                difficulty
            );
            assert_eq!(
                gs.minor_nations().len(),
                16,
                "Failed for difficulty {:?}",
                difficulty
            );
            assert_eq!(
                gs.provinces.len(),
                120,
                "Failed for difficulty {:?}",
                difficulty
            );
            assert_eq!(
                gs.turn,
                TurnNumber::new(1),
                "Failed for difficulty {:?}",
                difficulty
            );
            assert!(
                !gs.is_game_over(),
                "Game should not be over at start for {:?}",
                difficulty
            );
            // Human player should exist and be a Great Power
            let human = gs.get_nation(gs.human_player_nation).unwrap();
            assert!(human.is_great_power());
        }
    }

    // ── Nation lookup ─────────────────────────────────────────

    #[test]
    fn get_nation_found() {
        let gs = sample_game_state();
        let nation = gs.get_nation(NationId(1));
        assert!(nation.is_some());
        assert_eq!(nation.unwrap().name, "France");
    }

    #[test]
    fn get_nation_not_found() {
        let gs = sample_game_state();
        assert!(gs.get_nation(NationId(99)).is_none());
    }

    #[test]
    fn get_nation_mut_modifies() {
        let mut gs = sample_game_state();
        let nation = gs.get_nation_mut(NationId(1)).unwrap();
        nation.treasury = Money::dollars(1000);
        assert_eq!(
            gs.get_nation(NationId(1)).unwrap().treasury,
            Money::dollars(1000)
        );
    }

    // ── Province lookup ───────────────────────────────────────

    #[test]
    fn get_province_found() {
        let gs = sample_game_state();
        let province = gs.get_province(ProvinceId(2));
        assert!(province.is_some());
        assert_eq!(province.unwrap().name, "Bavaria");
    }

    #[test]
    fn get_province_not_found() {
        let gs = sample_game_state();
        assert!(gs.get_province(ProvinceId(99)).is_none());
    }

    #[test]
    fn get_province_mut_modifies() {
        let mut gs = sample_game_state();
        let province = gs.get_province_mut(ProvinceId(1)).unwrap();
        province.connected_to_capital = true;
        assert!(gs.get_province(ProvinceId(1)).unwrap().connected_to_capital);
    }

    // ── Great powers / minor nations ──────────────────────────

    #[test]
    fn great_powers_returns_only_great_powers() {
        let gs = sample_game_state();
        let gps = gs.great_powers();
        assert_eq!(gps.len(), 1);
        assert_eq!(gps[0].name, "France");
    }

    #[test]
    fn minor_nations_returns_only_minors() {
        let gs = sample_game_state();
        let minors = gs.minor_nations();
        assert_eq!(minors.len(), 1);
        assert_eq!(minors[0].name, "Bavaria");
    }

    // ── Turn management ───────────────────────────────────────

    #[test]
    fn advance_turn_increments() {
        let mut gs = sample_game_state();
        assert_eq!(gs.turn, TurnNumber::new(1));
        gs.advance_turn();
        assert_eq!(gs.turn, TurnNumber::new(2));
        gs.advance_turn();
        assert_eq!(gs.turn, TurnNumber::new(3));
    }

    // ── Events ────────────────────────────────────────────────

    #[test]
    fn push_event_stores_event() {
        let mut gs = sample_game_state();
        assert!(gs.events.is_empty());
        gs.push_event(DomainEvent::TurnStarted(TurnStarted {
            turn: TurnNumber::new(1),
        }));
        assert_eq!(gs.events.len(), 1);
    }

    // ── Game over ─────────────────────────────────────────────

    #[test]
    fn game_not_over_at_start() {
        let gs = sample_game_state();
        assert!(!gs.is_game_over());
    }

    #[test]
    fn game_over_at_1915_q1() {
        let mut gs = sample_game_state();
        gs.turn = TurnNumber::from_year_quarter(1915, 1);
        assert!(gs.is_game_over());
    }

    #[test]
    fn game_over_past_1915_q1() {
        let mut gs = sample_game_state();
        gs.turn = TurnNumber::from_year_quarter(1915, 2);
        assert!(gs.is_game_over());
    }

    #[test]
    fn game_not_over_at_1914_q4() {
        let mut gs = sample_game_state();
        gs.turn = TurnNumber::from_year_quarter(1914, 4);
        assert!(!gs.is_game_over());
    }

    // ── Starting civilians ───────────────────────────────────

    #[test]
    fn new_game_great_powers_have_starting_civilians() {
        let gs = new_game("test", Difficulty::Normal, 0);
        for nation in gs.great_powers() {
            assert_eq!(
                nation.civilians.len(),
                2,
                "Great Power {} should start with 2 civilians",
                nation.name
            );
            // First should be a Farmer
            assert_eq!(
                nation.civilians[0].civilian_type,
                CivilianType::Farmer,
                "{} should have a Farmer as first civilian",
                nation.name
            );
            // Second should be a Forester
            assert_eq!(
                nation.civilians[1].civilian_type,
                CivilianType::Forester,
                "{} should have a Forester as second civilian",
                nation.name
            );
        }
    }

    #[test]
    fn new_game_minor_nations_have_no_civilians() {
        let gs = new_game("test", Difficulty::Normal, 0);
        for nation in gs.minor_nations() {
            assert!(
                nation.civilians.is_empty(),
                "Minor Nation {} should have no civilians",
                nation.name
            );
        }
    }

    #[test]
    fn new_game_civilians_have_unique_ids() {
        let gs = new_game("test", Difficulty::Normal, 0);
        let all_ids: Vec<crate::map::UnitId> = gs
            .great_powers()
            .iter()
            .flat_map(|n| n.civilians.iter().map(|c| c.id))
            .collect();
        for i in 0..all_ids.len() {
            for j in (i + 1)..all_ids.len() {
                assert_ne!(all_ids[i], all_ids[j], "All civilian IDs must be unique");
            }
        }
    }

    // ── find_nation_by_name ──────────────────────────────────

    #[test]
    fn find_nation_by_name_exact_match() {
        let gs = new_game("test", Difficulty::Normal, 0);
        let nation = gs.find_nation_by_name("Deneb");
        assert!(nation.is_some());
        assert_eq!(nation.unwrap().name, "Deneb");
    }

    #[test]
    fn find_nation_by_name_case_insensitive() {
        let gs = new_game("test", Difficulty::Normal, 0);
        let nation = gs.find_nation_by_name("deneb");
        assert!(nation.is_some());
        assert_eq!(nation.unwrap().name, "Deneb");
    }

    #[test]
    fn find_nation_by_name_no_match() {
        let gs = new_game("test", Difficulty::Normal, 0);
        assert!(gs.find_nation_by_name("Atlantis").is_none());
    }

    #[test]
    fn find_nation_by_name_ambiguous_partial_match_returns_none() {
        let gs = new_game("test", Difficulty::Normal, 0);
        // "Dun" matches both "Dundee" and "Dunbar" etc. in Ordune's provinces,
        // but we're matching nation names. Let's check a prefix that matches
        // multiple nations. "D" matches "Deneb", "Devron", "Dedge" — multiple
        // nations, so it should return None.
        let result = gs.find_nation_by_name("D");
        assert!(
            result.is_none(),
            "Ambiguous partial match should return None"
        );
    }

    // ── is_game_over boundary ────────────────────────────────

    #[test]
    fn is_game_over_at_exact_boundary_1915_q1() {
        let mut gs = sample_game_state();
        gs.turn = TurnNumber::from_year_quarter(1915, 1);
        assert!(gs.is_game_over(), "Game should be over at exactly 1915 Q1");
    }
}
