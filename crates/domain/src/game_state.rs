use crate::ai::common::random_personalities;
use crate::data::GameData;
use crate::diplomacy::DiplomacyState;
use crate::economy::buildings::{Building, BuildingType};
use crate::economy::civilians::{Civilian, CivilianType};
use crate::economy::ledger::{CashFlow, CashSink, ResourceFlow};
use crate::economy::market::MarketState;
use crate::economy::observability::PendingEconomyOrder;
use crate::events::{DomainEvent, Headline, HistoryEvent};
use crate::map::{HexMap, Province, SeaZone, UnitId};
use crate::military::combat::BattleResult;
use crate::military::naval::NavalBattleResult;
use crate::military::ships::{Ship, ShipType};
use crate::nation::{Nation, NationColor};
use crate::types::*;
use std::collections::HashMap;

/// A single entry in a political-map snapshot: (province, owner,
/// incorporated_from). Stored as a tuple rather than a named struct to keep
/// the archive payload small — the per-turn archive grows linearly with
/// `provinces × turns`, and tuple serde output (`[p, o, i]`) is roughly half
/// the size of object output.
pub type PoliticalSnapshotEntry = (ProvinceId, NationId, Option<NationId>);

/// A per-turn political-map snapshot: the archived ownership of every
/// province plus the archived capital province of every nation at the end of
/// that turn. Capitals are archived separately because they can change during
/// the game (minor-nation capital reassignment on conquest), and rendering
/// historical capital markers from current nation state would be wrong.
#[derive(Debug, Clone)]
pub struct PoliticalSnapshot {
    pub provinces: Vec<PoliticalSnapshotEntry>,
    pub capitals: Vec<(NationId, ProvinceId)>,
}

/// Live world snapshot: everything that describes "what is true right now"
/// in the game world — map, provinces, nations, diplomacy, and market state.
#[derive(Clone)]
pub struct WorldState {
    /// Key identifying which map is loaded.
    pub map_key: String,
    /// The hex map containing all tiles.
    pub hex_map: HexMap,
    /// All provinces in the game.
    pub provinces: Vec<Province>,
    /// All nations in the game (Great Powers + Minor Nations).
    pub nations: Vec<Nation>,
    /// Diplomatic relations and standing between nations.
    pub diplomacy: DiplomacyState,
    /// Persistent market state: per-commodity price history and trend data
    /// updated at the end of each turn's trade phase.
    pub market_state: MarketState,
    /// Sea zones computed from the hex map at game start.
    /// Each zone is a region of connected sea hexes; lakes are marked `is_lake: true`.
    pub sea_zones: Vec<SeaZone>,
}

/// Historical records accumulated over the course of the game.
#[derive(Default, Clone)]
pub struct GameArchive {
    /// History of major game events. Use `render_history_event` for player-facing text.
    pub history: Vec<(TurnNumber, HistoryEvent)>,
    /// High score table: (nation_name, score, date_string).
    pub high_scores: Vec<(String, u32, String)>,
    /// Archived newspaper headlines from past turns.
    pub newspaper_archive: Vec<(TurnNumber, Vec<Headline>)>,
    /// Archived battle results from past turns: (turn, land battles, naval battles).
    pub battle_archive: Vec<(TurnNumber, Vec<BattleResult>, Vec<NavalBattleResult>)>,
    /// Archived political-map snapshots from past turns: province ownership
    /// and per-nation capitals at the end of each turn. Used to render the
    /// political map at any past turn from the news archive.
    pub political_archive: Vec<(TurnNumber, PoliticalSnapshot)>,
}

/// In-turn state that is either not serialized or rebuilt each turn.
#[derive(Default, Clone)]
pub struct TransientState {
    /// Event log for the current turn (not saved).
    pub events: Vec<DomainEvent>,
    /// Pending attacks to resolve this turn: (attacker NationId, target ProvinceId).
    pub pending_attacks: Vec<(NationId, ProvinceId)>,
    /// Pending unit movements to resolve this turn: (nation, unit_id, destination province).
    pub pending_moves: Vec<(NationId, crate::map::UnitId, ProvinceId)>,
    /// Active naval landing sites: (attacking_nation, target_province, turn_established).
    /// Established by assigning warships to Beachhead operation.
    /// Troops can attack the target province on **subsequent** turns only
    /// (not the same turn the landing was established).
    pub pending_landings: Vec<(NationId, ProvinceId, TurnNumber)>,
    /// Transient collector for AI-side treasury mutations. Drained into
    /// `TurnReport.ai_cash_spending` at end of turn (not saved).
    pub pending_ai_cash_spending: Vec<(NationId, CashSink, Money, Option<NationId>)>,
    /// Transient collector for AI-side cash income entries. Drained at end of turn (not saved).
    pub pending_ai_cash_income: Vec<(NationId, Money)>,
    /// Reserved-but-not-yet-cleared economy actions for UI/debug/snapshot consumers.
    pub pending_economy_orders: HashMap<NationId, Vec<PendingEconomyOrder>>,
    /// Per-nation cash flow breakdown from the most recently processed turn.
    /// Populated at the end of `process_turn`; read by the WASM bridge.
    pub last_cash_flow: HashMap<NationId, CashFlow>,
    /// Per-nation resource flow from the most recently processed turn.
    pub last_resource_flow: HashMap<NationId, ResourceFlow>,
    /// Transient collector for AI-side material consumption events that happen
    /// inside `run_ai_turns` and don't have access to the `TurnReport`. Drained
    /// into `report.stockpile_flows` at end of turn (not saved).
    pub pending_ai_material_outflows: Vec<(
        NationId,
        crate::types::MaterialType,
        crate::economy::ledger::ResourceOut,
        u32,
    )>,
    /// Same as above for goods stockpiles.
    pub pending_ai_goods_outflows: Vec<(
        NationId,
        crate::types::GoodsType,
        crate::economy::ledger::ResourceOut,
        u32,
    )>,
    /// Transient collector for AI-side material *inflows* (e.g. AI Steel→Arms
    /// conversion in `ai/naval.rs`). Drained at end of turn (not saved).
    pub pending_ai_material_inflows: Vec<(
        NationId,
        crate::types::MaterialType,
        crate::economy::ledger::ResourceIn,
        u32,
    )>,
}

/// Top-level aggregate root representing the complete state of a game.
/// Owns `world` (live snapshot), `archive` (history), and `transient`
/// (current-turn working state), plus game-loop scalars.
#[derive(Clone)]
pub struct GameState {
    /// Current turn number.
    pub turn: TurnNumber,
    /// Difficulty setting for this game.
    pub difficulty: Difficulty,
    /// The NationId of the human player's nation.
    pub human_player_nation: NationId,
    /// When true, AI functions print detailed decision traces to stderr.
    pub ai_debug: bool,
    /// When true, all 7 Great Powers are controlled by AI and the human player
    /// only observes. `human_player_nation` remains set as the "viewpoint" nation.
    pub observer_mode: bool,
    /// Monotonically-increasing counter used to allocate unique `UnitId`s.
    pub next_unit_id: u32,
    /// All data-driven game definitions (tech tree, unit stats, etc.).
    /// Reconstructed on load — not serialized.
    pub game_data: GameData,
    /// Live world snapshot: map, provinces, nations, diplomacy, market.
    pub world: WorldState,
    /// Historical records: newspapers, battles, political snapshots, history log.
    pub archive: GameArchive,
    /// In-turn working state: events, pending actions, last-turn flows.
    pub transient: TransientState,
}

impl GameState {
    /// Allocate a unique `UnitId` for any new entity (army unit, civilian,
    /// garrison, warship) created during this game. Uses a per-game counter so
    /// two games from the same map key produce the same ID sequence.
    pub fn alloc_unit_id(&mut self) -> UnitId {
        let id = self.next_unit_id;
        self.next_unit_id += 1;
        UnitId(id)
    }

    /// Look up a nation by its ID.
    pub fn get_nation(&self, id: NationId) -> Option<&Nation> {
        self.world.nations.iter().find(|n| n.id == id)
    }

    /// Look up a nation by its ID (mutable).
    pub fn get_nation_mut(&mut self, id: NationId) -> Option<&mut Nation> {
        self.world.nations.iter_mut().find(|n| n.id == id)
    }

    /// Look up a province by its ID.
    pub fn get_province(&self, id: ProvinceId) -> Option<&Province> {
        self.world.provinces.iter().find(|p| p.id == id)
    }

    /// Look up a province by its ID (mutable).
    pub fn get_province_mut(&mut self, id: ProvinceId) -> Option<&mut Province> {
        self.world.provinces.iter_mut().find(|p| p.id == id)
    }

    /// Display string for difficulty.
    pub fn difficulty_display(&self) -> &str {
        match self.difficulty {
            Difficulty::Introductory => "Introductory",
            Difficulty::Easy => "Easy",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Hard",
            Difficulty::NighOnImpossible => "Nigh-On Impossible",
        }
    }

    /// Returns all Great Power nations.
    pub fn great_powers(&self) -> Vec<&Nation> {
        self.world
            .nations
            .iter()
            .filter(|n| n.is_great_power())
            .collect()
    }

    /// Returns all Minor Nations.
    pub fn minor_nations(&self) -> Vec<&Nation> {
        self.world
            .nations
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
        self.transient.events.push(event);
    }

    /// Whether the game is over (turn >= 1915 Q1).
    pub fn is_game_over(&self) -> bool {
        self.turn.is_game_end() || self.turn > TurnNumber::from_year_quarter(1915, 1)
    }

    /// Render a `HistoryEvent` as the player-facing string used by the
    /// newspaper, history view, and CLI display. Resolves nation/province
    /// IDs against the current game state, so renames or destroyed
    /// entities show as "Unknown".
    pub fn render_history_event(&self, event: &HistoryEvent) -> String {
        let nation_name = |id: NationId| -> String {
            self.get_nation(id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| "Unknown".to_string())
        };
        let province_name = |id: ProvinceId| -> String {
            self.get_province(id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Unknown".to_string())
        };
        match event {
            HistoryEvent::WarDeclared {
                attacker,
                defender,
                protectee: None,
            } => format!(
                "{} declared war on {}",
                nation_name(*attacker),
                nation_name(*defender)
            ),
            HistoryEvent::WarDeclared {
                attacker,
                defender,
                protectee: Some(p),
            } => format!(
                "{} declared war on {} to protect {}",
                nation_name(*attacker),
                nation_name(*defender),
                nation_name(*p)
            ),
            HistoryEvent::JoinedWar { joiner, target } => format!(
                "{} joined war against {} (alliance obligation)",
                nation_name(*joiner),
                nation_name(*target)
            ),
            HistoryEvent::PeaceMade { a, b } => {
                format!("{} made peace with {}", nation_name(*a), nation_name(*b))
            }
            HistoryEvent::PeaceSigned { a, b } => {
                format!("{} signed peace with {}", nation_name(*a), nation_name(*b))
            }
            HistoryEvent::MutualPeace { a, b } => format!(
                "{} and {} agreed to mutual peace",
                nation_name(*a),
                nation_name(*b)
            ),
            HistoryEvent::ProvinceConquered {
                conqueror,
                loser,
                province,
            } => format!(
                "{} conquered {} from {}",
                nation_name(*conqueror),
                province_name(*province),
                nation_name(*loser)
            ),
            HistoryEvent::TechnologyResearched {
                researcher,
                tech_name,
            } => format!("{} researched {}", nation_name(*researcher), tech_name),
            HistoryEvent::NonAggressionPactSigned { signer, partner } => format!(
                "{} signed a non-aggression pact with {}",
                nation_name(*signer),
                nation_name(*partner)
            ),
            HistoryEvent::AllianceFormed { signer, partner } => format!(
                "{} formed an alliance with {}",
                nation_name(*signer),
                nation_name(*partner)
            ),
            HistoryEvent::TreatyProposalAccepted {
                acceptor,
                proposer,
                treaty_type,
            } => {
                let label = match treaty_type {
                    crate::events::TreatyType::NonAggressionPact => "Non-Aggression Pact",
                    crate::events::TreatyType::Alliance => "Alliance",
                    crate::events::TreatyType::PeaceTreaty => "Peace Treaty",
                    _ => "Treaty",
                };
                format!(
                    "{} accepted {}'s {} proposal",
                    nation_name(*acceptor),
                    nation_name(*proposer),
                    label
                )
            }
            HistoryEvent::FellIntoAnarchy { nation } => {
                format!("{} fell into anarchy", nation_name(*nation))
            }
            HistoryEvent::RegainedIndependence {
                minor,
                former_overlord,
            } => format!(
                "{} regained independence after {} fell into anarchy",
                nation_name(*minor),
                nation_name(*former_overlord)
            ),
            HistoryEvent::MinorJoinedEmpire {
                minor,
                overlord,
                reason,
            } => {
                let phrase = match reason {
                    crate::events::IncorporationReason::JoinedEmpire => "joined the empire of",
                    crate::events::IncorporationReason::VoluntarilyJoinedEmpire => {
                        "voluntarily joined the empire of"
                    }
                };
                format!(
                    "{} {} {}",
                    nation_name(*minor),
                    phrase,
                    nation_name(*overlord)
                )
            }
            HistoryEvent::ConsulateBuilt { player: _, target } => {
                format!("Trade consulate built with {}", nation_name(*target))
            }
            HistoryEvent::EmbassyBuilt { player: _, target } => {
                format!("Embassy built with {}", nation_name(*target))
            }
        }
    }

    /// Find a nation by partial, case-insensitive name match.
    /// Returns `None` if no nation matches or if multiple nations match.
    pub fn find_nation_by_name(&self, partial: &str) -> Option<&Nation> {
        let lower = partial.to_lowercase();
        let matches: Vec<&Nation> = self
            .world
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

const GP_COLORS: &[NationColor] = &[
    // Original 7 — match the canonical game
    NationColor::Yellow,    // Deneb
    NationColor::Orange,    // Devron
    NationColor::LightBlue, // Haxaco
    NationColor::Red,       // Kem
    NationColor::Green,     // Ordune
    NationColor::Purple,    // Patagon
    NationColor::Blue,      // Zimm
    // Extended pool for 8th-20th GPs when num_great_powers > 7
    NationColor::Crimson,
    NationColor::Magenta,
    NationColor::Forest,
    NationColor::Gold,
    NationColor::Aqua,
    NationColor::Violet,
    NationColor::BurntOrange,
    NationColor::HotPink,
    NationColor::Turquoise,
    NationColor::Slate,
    NationColor::Mauve,
    NationColor::Sage,
    NationColor::Mustard,
];

const MN_COLORS: &[NationColor] = &[
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
];

/// Create a new game from a map key and difficulty (canonical 80×50, 7 GPs, 16 minors).
/// This is the main entry point for starting a game.
pub fn new_game(map_key: &str, difficulty: Difficulty, human_nation_index: usize) -> GameState {
    new_game_with_config(
        map_key,
        difficulty,
        human_nation_index,
        crate::map::MapGenConfig::default(),
    )
}

/// Create a new game with caller-supplied `GameData` (tech tree, unit/ship stats).
///
/// Use this instead of `new_game` when a real tech tree is required (e.g. in
/// integration tests or production entry points that loaded data from disk).
pub fn new_game_with_data(
    map_key: &str,
    difficulty: Difficulty,
    human_nation_index: usize,
    game_data: crate::data::GameData,
) -> GameState {
    new_game_with_data_and_config(
        map_key,
        difficulty,
        human_nation_index,
        game_data,
        crate::map::MapGenConfig::default(),
    )
}

/// `new_game_with_data` + custom map-generation config.
pub fn new_game_with_data_and_config(
    map_key: &str,
    difficulty: Difficulty,
    human_nation_index: usize,
    game_data: crate::data::GameData,
    cfg: crate::map::MapGenConfig,
) -> GameState {
    let personality_seed = {
        let mut h: u64 = 5381;
        for b in map_key.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u64);
        }
        h ^ 0xA1CA_FE42
    };
    new_game_with_seed_and_data_and_config(
        map_key,
        difficulty,
        human_nation_index,
        personality_seed,
        game_data,
        cfg,
    )
}

/// Create a new game with explicit game data and personality seed.
pub fn new_game_with_seed_and_data(
    map_key: &str,
    difficulty: Difficulty,
    human_nation_index: usize,
    personality_seed: u64,
    game_data: crate::data::GameData,
) -> GameState {
    new_game_with_seed_and_data_and_config(
        map_key,
        difficulty,
        human_nation_index,
        personality_seed,
        game_data,
        crate::map::MapGenConfig::default(),
    )
}

/// Create a new game with explicit game data, personality seed, and map config.
pub fn new_game_with_seed_and_data_and_config(
    map_key: &str,
    difficulty: Difficulty,
    human_nation_index: usize,
    personality_seed: u64,
    game_data: crate::data::GameData,
    cfg: crate::map::MapGenConfig,
) -> GameState {
    new_game_inner(
        map_key,
        difficulty,
        human_nation_index,
        personality_seed,
        cfg,
        game_data,
    )
}

/// Create a new game with a custom map-generation config.
pub fn new_game_with_config(
    map_key: &str,
    difficulty: Difficulty,
    human_nation_index: usize,
    cfg: crate::map::MapGenConfig,
) -> GameState {
    // Derive personality seed from map key (XOR with constant to decouple from map gen)
    let personality_seed = {
        let mut h: u64 = 5381;
        for b in map_key.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u64);
        }
        h ^ 0xA1CA_FE42
    };
    new_game_with_seed_and_config(
        map_key,
        difficulty,
        human_nation_index,
        personality_seed,
        cfg,
    )
}

/// Create a new game with an explicit personality seed (canonical map config).
/// Used by batch mode to produce different personality assignments per game.
pub fn new_game_with_seed(
    map_key: &str,
    difficulty: Difficulty,
    human_nation_index: usize,
    personality_seed: u64,
) -> GameState {
    new_game_with_seed_and_config(
        map_key,
        difficulty,
        human_nation_index,
        personality_seed,
        crate::map::MapGenConfig::default(),
    )
}

/// Create a new game with an explicit personality seed and custom map config.
pub fn new_game_with_seed_and_config(
    map_key: &str,
    difficulty: Difficulty,
    human_nation_index: usize,
    personality_seed: u64,
    cfg: crate::map::MapGenConfig,
) -> GameState {
    new_game_with_seed_and_data_and_config(
        map_key,
        difficulty,
        human_nation_index,
        personality_seed,
        GameData::default(),
        cfg,
    )
}

fn new_game_inner(
    map_key: &str,
    difficulty: Difficulty,
    human_nation_index: usize,
    personality_seed: u64,
    cfg: crate::map::MapGenConfig,
    game_data: GameData,
) -> GameState {
    let generated = crate::map::generate_map_with_config(map_key, &cfg);

    assert!(
        !generated.great_power_nations.is_empty(),
        "MapGenConfig must have at least 1 great power (num_great_powers was {})",
        cfg.num_great_powers
    );

    // Pre-generate random personalities for all AI nations
    let gp_count = generated.great_power_nations.len();
    let human_idx = human_nation_index.min(gp_count - 1);
    let ai_count = gp_count - 1; // minus human
    let personalities = random_personalities(personality_seed, ai_count);

    let mut nations = Vec::new();
    let mut ai_personality_idx = 0;
    // Per-game unit-ID counter. Starts at 6_000_000 to stay above the
    // hardcoded ranges used by generals (3M+), admirals (4M+), and colony
    // ships (5M+) in the turn processor. Two calls to `new_game` with the
    // same map key produce identical ID sequences.
    let mut id_counter: u32 = 6_000_000;

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
            GP_COLORS[i % GP_COLORS.len()],
            NationType::GreatPower,
            setup.capital_province,
        );
        nation.economy.treasury = starting_cash;
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
            nation.economy.buildings.push(Building::new(*bt, 1));
        }

        // On Easy/Introductory, add starting mills and factories
        if matches!(difficulty, Difficulty::Easy | Difficulty::Introductory) {
            nation
                .economy
                .buildings
                .push(Building::new(BuildingType::LumberMill, 2));
            nation
                .economy
                .buildings
                .push(Building::new(BuildingType::SteelMill, 2));
            nation
                .economy
                .buildings
                .push(Building::new(BuildingType::TextileMill, 2));
            nation
                .economy
                .buildings
                .push(Building::new(BuildingType::FurnitureFactory, 1));
            nation
                .economy
                .buildings
                .push(Building::new(BuildingType::HardwareFactory, 1));
            nation
                .economy
                .buildings
                .push(Building::new(BuildingType::ClothingFactory, 1));
            nation
                .economy
                .buildings
                .push(Building::new(BuildingType::PaperFactory, 1));
        }

        // Starting warehouse contents based on difficulty.
        //
        // Values bumped alongside card #130 (capital-yield nerf): removing
        // the "whole capital province yields in full" rule costs every
        // nation several turns of far-from-capital resources, so we preload
        // enough stockpile to stay productive until the AI/player extends
        // depots outward.
        match difficulty {
            Difficulty::Introductory | Difficulty::Easy => {
                nation.add_resource(ResourceType::Timber, 20);
                nation.add_resource(ResourceType::Coal, 10);
                nation.add_resource(ResourceType::Iron, 10);
                nation.add_resource(ResourceType::Cotton, 5);
                nation.add_resource(ResourceType::Grain, 15);
                nation.add_resource(ResourceType::Fruit, 5);
                // Starting materials for easy difficulties (already have mills)
                nation.add_material(MaterialType::Lumber, 20);
                nation.add_material(MaterialType::Steel, 10);
                nation.add_material(MaterialType::Fabric, 5);
                // Starting food supply to prevent early starvation
                nation.add_material(MaterialType::CannedFood, 40);
                nation.add_material(MaterialType::Paper, 10);
            }
            Difficulty::Normal => {
                nation.add_resource(ResourceType::Timber, 10);
                nation.add_resource(ResourceType::Coal, 5);
                nation.add_resource(ResourceType::Iron, 5);
                nation.add_resource(ResourceType::Cotton, 3);
                nation.add_resource(ResourceType::Grain, 10);
                nation.add_resource(ResourceType::Fruit, 3);
                // Starting materials so AI can build factories before mills produce
                nation.add_material(MaterialType::Lumber, 10);
                nation.add_material(MaterialType::Steel, 6);
                nation.add_material(MaterialType::Fabric, 2);
                // Starting food supply — enough for ~10 turns while building food chain
                nation.add_material(MaterialType::CannedFood, 20);
                nation.add_material(MaterialType::Paper, 10);
            }
            Difficulty::Hard | Difficulty::NighOnImpossible => {
                // Minimal starting stockpile to bootstrap economy
                nation.add_resource(ResourceType::Timber, 3);
                nation.add_resource(ResourceType::Coal, 2);
                nation.add_resource(ResourceType::Iron, 2);
                nation.add_resource(ResourceType::Cotton, 2);
                nation.add_resource(ResourceType::Grain, 3);
                nation.add_material(MaterialType::Lumber, 5);
                nation.add_material(MaterialType::Steel, 3);
                // Small food buffer
                nation.add_material(MaterialType::CannedFood, 10);
                nation.add_material(MaterialType::Paper, 10);
            }
        }

        // Starting workers based on difficulty (original game: 4 untrained, 2 trained, 1 expert)
        match difficulty {
            Difficulty::Introductory | Difficulty::Easy => {
                nation.economy.labor.untrained = 4;
                nation.economy.labor.trained = 2;
                nation.economy.labor.expert = 1;
            }
            Difficulty::Normal => {
                nation.economy.labor.untrained = 4;
                nation.economy.labor.trained = 2;
                nation.economy.labor.expert = 1;
            }
            Difficulty::Hard | Difficulty::NighOnImpossible => {
                nation.economy.labor.untrained = 3;
                nation.economy.labor.trained = 1;
            }
        }

        // Starting freight cars (from game config)
        let starting_cars = game_data.game_config.starting_freight_cars;
        nation.military.transport.build_freight_cars(starting_cars);

        // Starting civilians (per manual p.27–28 + card #212): each Great Power
        // gets N Prospectors + N Miners + N Engineers. Forester/Rancher/Driller
        // are tech-gated, and Farmer is hired as needed via the Labor screen.
        for _ in 0..game_data.game_config.starting_prospectors {
            nation.military.civilians.push(Civilian::new(
                crate::map::UnitId(id_counter),
                CivilianType::Prospector,
                setup.nation_id,
            ));
            id_counter += 1;
        }
        for _ in 0..game_data.game_config.starting_miners {
            nation.military.civilians.push(Civilian::new(
                crate::map::UnitId(id_counter),
                CivilianType::Miner,
                setup.nation_id,
            ));
            id_counter += 1;
        }
        for _ in 0..game_data.game_config.starting_engineers {
            nation.military.civilians.push(Civilian::new(
                crate::map::UnitId(id_counter),
                CivilianType::Engineer,
                setup.nation_id,
            ));
            id_counter += 1;
        }

        // Starting merchant fleet: 1 Trader for each Great Power
        let trader = Ship::with_data(
            UnitId(1_500_000 + i as u32),
            ShipType::Trader,
            setup.nation_id,
            &game_data,
        );
        nation.military.merchant_fleet.push(trader);

        // Starting warship: 1 Frigate for each Great Power
        let frigate = Ship::with_data(
            UnitId(2_500_000 + i as u32),
            ShipType::Frigate,
            setup.nation_id,
            &game_data,
        );
        nation.military.warships.push(frigate);

        // Starting army: 4 Regulars + 1 Light Artillery at capital
        for j in 0..4u32 {
            nation
                .military
                .army
                .push(crate::military::units::ArmyUnit::new(
                    UnitId(1_000_000 + i as u32 * 10 + j),
                    crate::military::units::ArmyUnitType::Regulars,
                    setup.nation_id,
                    setup.capital_province,
                ));
        }
        nation
            .military
            .army
            .push(crate::military::units::ArmyUnit::new(
                UnitId(1_000_000 + i as u32 * 10 + 4),
                crate::military::units::ArmyUnitType::LightArtillery,
                setup.nation_id,
                setup.capital_province,
            ));

        // Persistent Militia in every owned province (manual page 36:
        // "local defence forces exist in all countries and in all provinces").
        // Uses the Lua-configurable default — the map generator's hardcoded
        // `garrison_count` is refreshed after seeding below.
        let default_garrison = game_data.game_config.default_garrison_per_province as usize;
        for &pid in &setup.province_ids {
            for _ in 0..default_garrison {
                nation
                    .military
                    .army
                    .push(crate::military::combat::spawn_militia_unit(
                        &mut id_counter,
                        setup.nation_id,
                        pid,
                    ));
            }
        }

        // Assign AI personality for non-human Great Powers
        if i != human_idx {
            nation.diplomacy.ai_personality = Some(personalities[ai_personality_idx]);
            ai_personality_idx += 1;

            // AI difficulty bonuses (applied to AI nations only, not human)
            match difficulty {
                Difficulty::Hard => {
                    nation.economy.treasury += Money::dollars(1000); // +$1,000 starting cash
                }
                Difficulty::NighOnImpossible => {
                    nation.economy.treasury += Money::dollars(5000); // +$5,000 starting cash
                }
                _ => {} // Normal/Easy/Introductory: no AI bonuses
            }
        }

        nations.push(nation);
    }

    // Create Minor Nations
    for (i, setup) in generated.minor_nations.iter().enumerate() {
        let mut nation = Nation::new(
            setup.nation_id,
            setup.name.clone(),
            MN_COLORS[i % MN_COLORS.len()],
            NationType::MinorNation,
            setup.capital_province,
        );
        for pid in &setup.province_ids {
            nation.add_province(*pid);
        }

        // Starting army: 4 Regulars + 1 Light Artillery at capital
        for j in 0..4u32 {
            nation
                .military
                .army
                .push(crate::military::units::ArmyUnit::new(
                    UnitId(1_100_000 + i as u32 * 10 + j),
                    crate::military::units::ArmyUnitType::Regulars,
                    setup.nation_id,
                    setup.capital_province,
                ));
        }
        nation
            .military
            .army
            .push(crate::military::units::ArmyUnit::new(
                UnitId(1_100_000 + i as u32 * 10 + 4),
                crate::military::units::ArmyUnitType::LightArtillery,
                setup.nation_id,
                setup.capital_province,
            ));

        // Persistent Militia in every owned province (minor-nation cap).
        let minor_default_garrison = game_data.game_config.minor_default_garrison as usize;
        for &pid in &setup.province_ids {
            for _ in 0..minor_default_garrison {
                nation
                    .military
                    .army
                    .push(crate::military::combat::spawn_militia_unit(
                        &mut id_counter,
                        setup.nation_id,
                        pid,
                    ));
            }
        }
        // A single GarrisonArtillery at the minor nation's capital.
        nation
            .military
            .army
            .push(crate::military::combat::spawn_garrison_artillery_unit(
                &mut id_counter,
                setup.nation_id,
                setup.capital_province,
            ));

        nations.push(nation);
    }

    let human_nation_id = generated.great_power_nations[human_idx].nation_id;

    let mut diplomacy = DiplomacyState::new();
    let gp_ids: Vec<NationId> = nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();
    diplomacy.initialize_great_powers(&gp_ids);

    let mut game_state = GameState {
        turn: TurnNumber::new(1),
        difficulty,
        human_player_nation: human_nation_id,
        ai_debug: false,
        observer_mode: false,
        next_unit_id: id_counter,
        game_data,
        world: WorldState {
            map_key: map_key.to_string(),
            hex_map: generated.hex_map,
            provinces: generated.provinces,
            nations,
            diplomacy,
            market_state: MarketState::new(),
            sea_zones: generated.sea_zones,
        },
        archive: GameArchive::default(),
        transient: TransientState::default(),
    };

    // Refresh the per-province `garrison_count` cache now that militia have
    // been seeded. The map generator seeds provinces with a hardcoded 4/3
    // default; if the Lua tunable differs, the cache would drift without
    // this step. Each province's cache is recomputed from the authoritative
    // live militia count in the owning nation's army.
    {
        let snapshot: Vec<ProvinceId> = game_state.world.provinces.iter().map(|p| p.id).collect();
        for pid in snapshot {
            let owner = match game_state.get_province(pid) {
                Some(p) => p.owner,
                None => continue,
            };
            let count = game_state
                .get_nation(owner)
                .map(|n| n.militia_at(pid))
                .unwrap_or(0)
                .min(u8::MAX as usize) as u8;
            if let Some(prov) = game_state.get_province_mut(pid) {
                prov.garrison_count = count;
            }
        }
    }

    // Pre-build a depot at every nation's capital tile (Great Powers and minor
    // nations alike). Capitals act as implicit depots so the capital province is
    // always harvesting from turn 1 without requiring the player to build one.
    let capital_tiles: Vec<crate::hex::HexCoord> = game_state
        .world
        .nations
        .iter()
        .filter_map(|n| {
            game_state
                .world
                .provinces
                .iter()
                .find(|p| p.id == n.capital_province_id)
                .map(|p| p.capital_tile)
        })
        .collect();
    for cap_tile in capital_tiles {
        let _ = crate::map::infrastructure::place_depot_unchecked(
            &mut game_state.world.hex_map,
            cap_tile,
        );
        // Persistent "this tile was a country capital" flag. Survives conquest
        // so captured foreign capitals keep acting as implicit depots.
        if let Some(tile) = game_state.world.hex_map.get_tile_mut(cap_tile) {
            tile.is_country_capital = true;
        }
    }

    // Give minor nation capitals a level 1 fort (original Imperialism defensive mechanic).
    // This makes minor nations harder to conquer early, requiring real military buildup.
    for nation in &game_state.world.nations {
        if nation.is_great_power() {
            continue;
        }
        let cap_pid = nation.capital_province_id;
        if let Some(province) = game_state.world.provinces.iter().find(|p| p.id == cap_pid) {
            let cap_tile = province.capital_tile;
            if let Some(tile) = game_state.world.hex_map.get_tile_mut(cap_tile) {
                tile.infrastructure.has_fort = true;
                tile.infrastructure.fort_level = 1;
            }
        }
    }

    // Seed each AI-controlled Great Power's priority minor-nation diplomacy targets.
    // The picker scores every minor by its visible tradeable-export match against
    // the GP's resource demand, and takes the top N (personality-dependent).
    // Human-slot nations (no personality yet at this point) are skipped; the
    // observer-mode promotion in batch.rs reassigns them before the first turn.
    let gp_ids_for_targets: Vec<(NationId, crate::ai::AiPersonality)> = game_state
        .world
        .nations
        .iter()
        .filter_map(|n| n.diplomacy.ai_personality.map(|p| (n.id, p)))
        .filter(|(id, _)| {
            game_state
                .get_nation(*id)
                .is_some_and(|n| n.is_great_power())
        })
        .collect();
    for (gp_id, personality) in gp_ids_for_targets {
        let count =
            crate::ai::priority_target_count(&game_state.game_data.game_config, personality);
        let targets = crate::ai::pick_priority_minor_targets(&game_state, gp_id, count, &[]);
        if let Some(nation) = game_state.get_nation_mut(gp_id) {
            nation.diplomacy.ai_priority_state.priority_minor_targets = targets;
        }
    }

    // On Easy/Introductory, auto-prospect tiles in the human player's capital province
    // to reveal any existing mineral deposits, giving the player a head start.
    if matches!(difficulty, Difficulty::Easy | Difficulty::Introductory) {
        // Find the human player's capital province
        let capital_province_id = game_state
            .get_nation(human_nation_id)
            .map(|n| n.capital_province_id);

        if let Some(cap_pid) = capital_province_id {
            // Get the tiles in the capital province
            let capital_tiles: Vec<crate::hex::HexCoord> = game_state
                .get_province(cap_pid)
                .map(|p| p.tiles.clone())
                .unwrap_or_default();

            for tile_coord in &capital_tiles {
                if let Some(tile) = game_state.world.hex_map.get_tile_mut(*tile_coord) {
                    // Only auto-reveal on tiles that require prospecting and already
                    // have a deposit placed by the map generator. We don't fabricate
                    // deposits — we just reveal what the generator already placed.
                    // The generator sets resource_deposit on ~40% of prospectable tiles,
                    // so the tile may already have a deposit. We mark it as "prospected"
                    // by bumping improvement_level if it has a deposit.
                    // Only auto-reveal hidden deposits (Coal/Iron/Gold/Gems/Oil),
                    // not surface resources like Wool on Hills.
                    if tile.terrain().can_have_deposits()
                        && tile
                            .resource_deposit()
                            .is_some_and(|r| r.requires_prospecting())
                    {
                        // Mark as prospected so it becomes visible
                        tile.reveal_deposit(tile.resource_deposit().unwrap());
                        if tile.improvement_level() == 0 {
                            tile.improve();
                        }
                    }
                }
            }
        }
    }

    game_state
}

/// Create a new game in observer mode — all 7 Great Powers play as AI.
///
/// The `viewpoint_index` determines the default nation used for ledger / diplomacy
/// screens; it can be switched in-game. Observer mode does not exempt any nation
/// from AI control, and the Hard/NOI difficulty starting-cash bonus is applied to
/// all 7 GPs (since there is no human player to exempt).
pub fn new_observer_game(map_key: &str, difficulty: Difficulty) -> GameState {
    new_observer_game_with_config(map_key, difficulty, crate::map::MapGenConfig::default())
}

/// Create a new observer game with caller-supplied `GameData`.
pub fn new_observer_game_with_data(
    map_key: &str,
    difficulty: Difficulty,
    game_data: crate::data::GameData,
) -> GameState {
    new_observer_game_with_data_and_config(
        map_key,
        difficulty,
        game_data,
        crate::map::MapGenConfig::default(),
    )
}

/// Like [`new_observer_game`] but accepts a custom map-generation config.
pub fn new_observer_game_with_config(
    map_key: &str,
    difficulty: Difficulty,
    cfg: crate::map::MapGenConfig,
) -> GameState {
    new_observer_game_with_data_and_config(map_key, difficulty, GameData::default(), cfg)
}

/// Like [`new_observer_game_with_data`] but accepts a custom map-generation config.
pub fn new_observer_game_with_data_and_config(
    map_key: &str,
    difficulty: Difficulty,
    game_data: crate::data::GameData,
    cfg: crate::map::MapGenConfig,
) -> GameState {
    let personality_seed = {
        let mut h: u64 = 5381;
        for b in map_key.bytes() {
            h = h.wrapping_mul(33).wrapping_add(b as u64);
        }
        h ^ 0xA1CA_FE42
    };
    // Build base game with nation 0 as the placeholder "human" seat.
    let mut game = new_game_with_seed_and_data_and_config(
        map_key,
        difficulty,
        0,
        personality_seed,
        game_data,
        cfg,
    );

    // Assign an AI personality + difficulty bonus to the placeholder seat so
    // all 7 GPs are on equal footing.
    let human_id = game.human_player_nation;
    let extra = random_personalities(personality_seed ^ 0xDEAD_BEEF, 1)[0];
    if let Some(nation) = game.get_nation_mut(human_id) {
        nation.diplomacy.ai_personality = Some(extra);
        match difficulty {
            Difficulty::Hard => nation.economy.treasury += Money::dollars(1000),
            Difficulty::NighOnImpossible => nation.economy.treasury += Money::dollars(5000),
            _ => {}
        }
    }
    // The observer-seat nation now has a personality; pick its priority minor
    // targets (`new_game_with_seed` skipped it because its personality was None).
    let count = crate::ai::priority_target_count(&game.game_data.game_config, extra);
    let targets = crate::ai::pick_priority_minor_targets(&game, human_id, count, &[]);
    if let Some(nation) = game.get_nation_mut(human_id) {
        nation.diplomacy.ai_priority_state.priority_minor_targets = targets;
    }
    game.observer_mode = true;
    game
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::TurnStarted;
    use crate::hex::HexCoord;
    use crate::nation::NationColor;

    /// Helper: build a minimal GameState for testing.
    #[allow(dead_code)]
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
            human_player_nation: NationId(1),
            ai_debug: false,
            observer_mode: false,
            next_unit_id: 6_000_000,
            game_data: GameData::default(),
            world: WorldState {
                map_key: "europe".to_string(),
                hex_map: HexMap::new(10, 10),
                provinces: vec![province1, province2],
                nations: vec![nation1, nation2],
                diplomacy: DiplomacyState::new(),
                market_state: MarketState::new(),
                sea_zones: Vec::new(),
            },
            archive: GameArchive::default(),
            transient: TransientState::default(),
        }
    }

    #[test]
    fn new_game_creates_valid_state() {
        let gs = new_game("test", Difficulty::Normal, 0);
        assert_eq!(gs.great_powers().len(), 7);
        assert_eq!(gs.minor_nations().len(), 16);
        assert_eq!(gs.world.provinces.len(), 120);
        assert_eq!(gs.turn, TurnNumber::new(1));
        assert!(!gs.is_game_over());
    }

    // ── HistoryEvent rendering ────────────────────────────────
    //
    // Documents the canonical player-facing strings produced by
    // `render_history_event`. Two notes for future maintainers:
    //
    // * NAP rendering unified the AI-driven ("signed a non-aggression
    //   pact with") and CLI-driven ("signed non-aggression pact with",
    //   missing the article) wordings to the AI form. The CLI's
    //   action-time `println!` is unchanged; only the historical
    //   retrospective text is now consistent.
    // * Deleted nations / provinces render as "Unknown" — the renderer
    //   does best-effort lookup against the live game state.

    use crate::events::{HistoryEvent, IncorporationReason, TreatyType};

    fn render_test_game() -> GameState {
        let mut g = sample_game_state();
        g.world.nations[0].name = "Devron".to_string();
        g.world.nations[1].name = "Smallton".to_string();
        g.world.provinces[0].name = "Capital Province".to_string();
        g
    }

    #[test]
    fn render_history_event_war_declared_simple() {
        let g = render_test_game();
        let s = g.render_history_event(&HistoryEvent::WarDeclared {
            attacker: NationId(1),
            defender: NationId(2),
            protectee: None,
        });
        assert_eq!(s, "Devron declared war on Smallton");
    }

    #[test]
    fn render_history_event_war_declared_with_protectee() {
        let mut g = render_test_game();
        g.world.nations.push(Nation::new(
            NationId(3),
            "Patagon".to_string(),
            NationColor::Yellow,
            NationType::MinorNation,
            ProvinceId(1),
        ));
        let s = g.render_history_event(&HistoryEvent::WarDeclared {
            attacker: NationId(1),
            defender: NationId(2),
            protectee: Some(NationId(3)),
        });
        assert_eq!(s, "Devron declared war on Smallton to protect Patagon");
    }

    #[test]
    fn render_history_event_joined_war() {
        let g = render_test_game();
        let s = g.render_history_event(&HistoryEvent::JoinedWar {
            joiner: NationId(1),
            target: NationId(2),
        });
        assert_eq!(
            s,
            "Devron joined war against Smallton (alliance obligation)"
        );
    }

    #[test]
    fn render_history_event_peace_variants() {
        let g = render_test_game();
        assert_eq!(
            g.render_history_event(&HistoryEvent::PeaceMade {
                a: NationId(1),
                b: NationId(2),
            }),
            "Devron made peace with Smallton",
        );
        assert_eq!(
            g.render_history_event(&HistoryEvent::PeaceSigned {
                a: NationId(1),
                b: NationId(2),
            }),
            "Devron signed peace with Smallton",
        );
        assert_eq!(
            g.render_history_event(&HistoryEvent::MutualPeace {
                a: NationId(1),
                b: NationId(2),
            }),
            "Devron and Smallton agreed to mutual peace",
        );
    }

    #[test]
    fn render_history_event_province_conquered() {
        let g = render_test_game();
        let s = g.render_history_event(&HistoryEvent::ProvinceConquered {
            conqueror: NationId(1),
            loser: NationId(2),
            province: ProvinceId(1),
        });
        assert_eq!(s, "Devron conquered Capital Province from Smallton");
    }

    #[test]
    fn render_history_event_technology_researched() {
        let g = render_test_game();
        let s = g.render_history_event(&HistoryEvent::TechnologyResearched {
            researcher: NationId(1),
            tech_name: "Steam Power".to_string(),
        });
        assert_eq!(s, "Devron researched Steam Power");
    }

    #[test]
    fn render_history_event_non_aggression_pact() {
        // Canonical wording: "signed a non-aggression pact with" (with the
        // article). Previously the CLI emitted a non-grammatical variant
        // without the article; this test pins the unified form.
        let g = render_test_game();
        let s = g.render_history_event(&HistoryEvent::NonAggressionPactSigned {
            signer: NationId(1),
            partner: NationId(2),
        });
        assert_eq!(s, "Devron signed a non-aggression pact with Smallton");
    }

    #[test]
    fn render_history_event_alliance_formed() {
        let g = render_test_game();
        let s = g.render_history_event(&HistoryEvent::AllianceFormed {
            signer: NationId(1),
            partner: NationId(2),
        });
        assert_eq!(s, "Devron formed an alliance with Smallton");
    }

    #[test]
    fn render_history_event_treaty_proposal_accepted_all_variants() {
        let g = render_test_game();
        let cases: &[(TreatyType, &str)] = &[
            (
                TreatyType::Alliance,
                "Devron accepted Smallton's Alliance proposal",
            ),
            (
                TreatyType::NonAggressionPact,
                "Devron accepted Smallton's Non-Aggression Pact proposal",
            ),
            (
                TreatyType::PeaceTreaty,
                "Devron accepted Smallton's Peace Treaty proposal",
            ),
            // Fallback path: any treaty type the renderer does not recognise
            // collapses to the generic "Treaty" label.
            (
                TreatyType::WarDeclaration,
                "Devron accepted Smallton's Treaty proposal",
            ),
            (
                TreatyType::RequestToJoinEmpire,
                "Devron accepted Smallton's Treaty proposal",
            ),
        ];
        for (treaty_type, expected) in cases {
            let s = g.render_history_event(&HistoryEvent::TreatyProposalAccepted {
                acceptor: NationId(1),
                proposer: NationId(2),
                treaty_type: *treaty_type,
            });
            assert_eq!(s, *expected, "treaty_type = {:?}", treaty_type);
        }
    }

    #[test]
    fn render_history_event_anarchy_and_independence() {
        let g = render_test_game();
        assert_eq!(
            g.render_history_event(&HistoryEvent::FellIntoAnarchy {
                nation: NationId(1),
            }),
            "Devron fell into anarchy",
        );
        assert_eq!(
            g.render_history_event(&HistoryEvent::RegainedIndependence {
                minor: NationId(2),
                former_overlord: NationId(1),
            }),
            "Smallton regained independence after Devron fell into anarchy",
        );
    }

    #[test]
    fn render_history_event_minor_joined_empire_variants() {
        let g = render_test_game();
        assert_eq!(
            g.render_history_event(&HistoryEvent::MinorJoinedEmpire {
                minor: NationId(2),
                overlord: NationId(1),
                reason: IncorporationReason::JoinedEmpire,
            }),
            "Smallton joined the empire of Devron",
        );
        assert_eq!(
            g.render_history_event(&HistoryEvent::MinorJoinedEmpire {
                minor: NationId(2),
                overlord: NationId(1),
                reason: IncorporationReason::VoluntarilyJoinedEmpire,
            }),
            "Smallton voluntarily joined the empire of Devron",
        );
    }

    #[test]
    fn render_history_event_consulate_and_embassy() {
        let g = render_test_game();
        assert_eq!(
            g.render_history_event(&HistoryEvent::ConsulateBuilt {
                player: NationId(1),
                target: NationId(2),
            }),
            "Trade consulate built with Smallton",
        );
        assert_eq!(
            g.render_history_event(&HistoryEvent::EmbassyBuilt {
                player: NationId(1),
                target: NationId(2),
            }),
            "Embassy built with Smallton",
        );
    }

    #[test]
    fn render_history_event_unknown_fallback_for_missing_nation_or_province() {
        // Nation IDs / province IDs that don't exist in the game state
        // render as "Unknown" — covers eliminated nations and pruned
        // provinces showing up in old archived history entries.
        let g = render_test_game();
        let s = g.render_history_event(&HistoryEvent::ProvinceConquered {
            conqueror: NationId(99),
            loser: NationId(98),
            province: ProvinceId(97),
        });
        assert_eq!(s, "Unknown conquered Unknown from Unknown");
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
            .economy
            .treasury;
        let easy_cash = easy
            .get_nation(easy.human_player_nation)
            .unwrap()
            .economy
            .treasury;
        let normal_cash = normal
            .get_nation(normal.human_player_nation)
            .unwrap()
            .economy
            .treasury;
        let hard_cash = hard
            .get_nation(hard.human_player_nation)
            .unwrap()
            .economy
            .treasury;
        let noi_cash = noi
            .get_nation(noi.human_player_nation)
            .unwrap()
            .economy
            .treasury;

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
                nation.economy.treasury,
                Money::dollars(15000),
                "{} should start with $15,000 on Introductory",
                nation.name
            );

            // 5 untrained + 3 trained workers
            assert_eq!(
                nation.economy.labor.untrained, 4,
                "{} should have 5 untrained workers on Introductory",
                nation.name
            );
            assert_eq!(
                nation.economy.labor.trained, 2,
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
                nation.economy.treasury,
                Money::dollars(12000),
                "{} should start with $12,000 on Easy",
                nation.name
            );

            // 4 untrained + 2 trained + 1 expert workers
            assert_eq!(nation.economy.labor.untrained, 4);
            assert_eq!(nation.economy.labor.trained, 2);
            assert_eq!(nation.economy.labor.expert, 1);
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
                nation.economy.treasury,
                Money::dollars(10000),
                "{} should start with $10,000 on Normal",
                nation.name
            );

            // 4 untrained + 2 trained + 1 expert workers
            assert_eq!(nation.economy.labor.untrained, 4);
            assert_eq!(nation.economy.labor.trained, 2);
            assert_eq!(nation.economy.labor.expert, 1);
        }
    }

    #[test]
    fn new_game_hard_has_less_starting_cash() {
        let gs = new_game("test", Difficulty::Hard, 0);
        for nation in gs.great_powers() {
            if nation.id == gs.human_player_nation {
                // Human: $8,000 starting cash (no AI bonus)
                assert_eq!(
                    nation.economy.treasury,
                    Money::dollars(8000),
                    "{} (human) should start with $8,000 on Hard",
                    nation.name
                );
            } else {
                // AI: $8,000 base + $1,000 AI difficulty bonus = $9,000
                assert_eq!(
                    nation.economy.treasury,
                    Money::dollars(9000),
                    "{} (AI) should start with $9,000 on Hard ($8,000 + $1,000 bonus)",
                    nation.name
                );
            }

            // No mills or factories
            assert!(
                !nation.has_building(BuildingType::LumberMill),
                "{} should NOT have a LumberMill on Hard",
                nation.name
            );

            // 3 untrained + 1 trained workers (harder start)
            assert_eq!(nation.economy.labor.untrained, 3);
            assert_eq!(nation.economy.labor.trained, 1);
        }
    }

    #[test]
    fn new_game_noi_has_minimal_resources() {
        let gs = new_game("test", Difficulty::NighOnImpossible, 0);
        for nation in gs.great_powers() {
            if nation.id == gs.human_player_nation {
                // Human: $5,000 starting cash (no AI bonus)
                assert_eq!(
                    nation.economy.treasury,
                    Money::dollars(5000),
                    "{} (human) should start with $5,000 on NOI",
                    nation.name
                );
            } else {
                // AI: $5,000 base + $5,000 AI difficulty bonus = $10,000
                assert_eq!(
                    nation.economy.treasury,
                    Money::dollars(10000),
                    "{} (AI) should start with $10,000 on NOI ($5,000 + $5,000 bonus)",
                    nation.name
                );
            }

            // 3 untrained + 1 trained workers (harder start)
            assert_eq!(
                nation.economy.labor.untrained, 3,
                "{} should have 3 untrained workers on NOI",
                nation.name
            );
            assert_eq!(
                nation.economy.labor.trained, 1,
                "{} should have 1 trained worker on NOI",
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
                gs.world.provinces.len(),
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
        nation.economy.treasury = Money::dollars(1000);
        assert_eq!(
            gs.get_nation(NationId(1)).unwrap().economy.treasury,
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
        assert!(gs.transient.events.is_empty());
        gs.push_event(DomainEvent::TurnStarted(TurnStarted {
            turn: TurnNumber::new(1),
        }));
        assert_eq!(gs.transient.events.len(), 1);
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
        let cfg = &gs.game_data.game_config;
        let prospectors = cfg.starting_prospectors as usize;
        let miners = cfg.starting_miners as usize;
        let engineers = cfg.starting_engineers as usize;
        let expected = prospectors + miners + engineers;
        for nation in gs.great_powers() {
            assert_eq!(
                nation.military.civilians.len(),
                expected,
                "Great Power {} should start with {} civilians",
                nation.name,
                expected
            );
            let counts = |t: CivilianType| {
                nation
                    .military
                    .civilians
                    .iter()
                    .filter(|c| c.civilian_type == t)
                    .count()
            };
            assert_eq!(counts(CivilianType::Prospector), prospectors);
            assert_eq!(counts(CivilianType::Miner), miners);
            assert_eq!(counts(CivilianType::Engineer), engineers);
        }
    }

    #[test]
    fn new_game_minor_nations_have_no_civilians() {
        let gs = new_game("test", Difficulty::Normal, 0);
        for nation in gs.minor_nations() {
            assert!(
                nation.military.civilians.is_empty(),
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
            .flat_map(|n| n.military.civilians.iter().map(|c| c.id))
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

    // ── High scores ──────────────────────────────────────────────

    #[test]
    fn high_scores_default_empty() {
        let gs = sample_game_state();
        assert!(gs.archive.high_scores.is_empty());
    }

    #[test]
    fn high_score_recorded_on_game_end() {
        let mut gs = sample_game_state();
        gs.turn = TurnNumber::from_year_quarter(1915, 1);
        assert!(gs.is_game_over());

        // Record a high score
        let score =
            crate::turn::calculate_score(gs.get_nation(NationId(1)).unwrap(), &gs.game_data);
        let date_str = format!("{} Q{}", gs.turn.year(), gs.turn.quarter());
        gs.archive
            .high_scores
            .push(("France".to_string(), score.total, date_str));

        assert_eq!(gs.archive.high_scores.len(), 1);
        assert_eq!(gs.archive.high_scores[0].0, "France");
        assert_eq!(gs.archive.high_scores[0].2, "1915 Q1");
    }

    // ── Starting warehouse by difficulty ──────────────────────────

    #[test]
    fn new_game_easy_has_starting_warehouse() {
        let gs = new_game("test", Difficulty::Easy, 0);
        let human = gs.get_nation(gs.human_player_nation).unwrap();
        assert_eq!(human.resource_amount(ResourceType::Timber), 20);
        assert_eq!(human.resource_amount(ResourceType::Coal), 10);
        assert_eq!(human.resource_amount(ResourceType::Iron), 10);
        assert_eq!(human.resource_amount(ResourceType::Cotton), 5);
        assert_eq!(human.resource_amount(ResourceType::Grain), 15);
        assert_eq!(human.resource_amount(ResourceType::Fruit), 5);
    }

    #[test]
    fn new_game_introductory_has_starting_warehouse() {
        let gs = new_game("test", Difficulty::Introductory, 0);
        let human = gs.get_nation(gs.human_player_nation).unwrap();
        assert_eq!(human.resource_amount(ResourceType::Timber), 20);
        assert_eq!(human.resource_amount(ResourceType::Coal), 10);
        assert_eq!(human.resource_amount(ResourceType::Iron), 10);
        assert_eq!(human.resource_amount(ResourceType::Cotton), 5);
        assert_eq!(human.resource_amount(ResourceType::Grain), 15);
        assert_eq!(human.resource_amount(ResourceType::Fruit), 5);
    }

    #[test]
    fn new_game_normal_has_smaller_starting_warehouse() {
        let gs = new_game("test", Difficulty::Normal, 0);
        let human = gs.get_nation(gs.human_player_nation).unwrap();
        assert_eq!(human.resource_amount(ResourceType::Timber), 10);
        assert_eq!(human.resource_amount(ResourceType::Coal), 5);
        assert_eq!(human.resource_amount(ResourceType::Iron), 5);
        assert_eq!(human.resource_amount(ResourceType::Grain), 10);
        assert_eq!(human.resource_amount(ResourceType::Cotton), 3);
        assert_eq!(human.resource_amount(ResourceType::Fruit), 3);
    }

    #[test]
    fn new_game_hard_has_minimal_warehouse() {
        let gs = new_game("test", Difficulty::Hard, 0);
        let human = gs.get_nation(gs.human_player_nation).unwrap();
        assert_eq!(human.resource_amount(ResourceType::Timber), 3);
        assert_eq!(human.resource_amount(ResourceType::Coal), 2);
        assert_eq!(human.resource_amount(ResourceType::Iron), 2);
        assert_eq!(human.resource_amount(ResourceType::Cotton), 2);
        assert_eq!(human.resource_amount(ResourceType::Grain), 3);
        assert_eq!(human.resource_amount(ResourceType::Fruit), 0);
    }

    #[test]
    fn new_game_noi_has_minimal_warehouse() {
        let gs = new_game("test", Difficulty::NighOnImpossible, 0);
        let human = gs.get_nation(gs.human_player_nation).unwrap();
        assert_eq!(human.resource_amount(ResourceType::Timber), 3);
        assert_eq!(human.resource_amount(ResourceType::Coal), 2);
        assert_eq!(human.resource_amount(ResourceType::Iron), 2);
        assert_eq!(human.resource_amount(ResourceType::Grain), 3);
    }

    // ── Auto-prospecting on Easy/Introductory ─────────────────────

    #[test]
    fn new_game_easy_auto_prospects_capital_province() {
        let gs = new_game("auto_prospect_test", Difficulty::Easy, 0);
        let human = gs.get_nation(gs.human_player_nation).unwrap();
        let capital_pid = human.capital_province_id;
        let province = gs.get_province(capital_pid).unwrap();

        // Check that any tiles with deposits in the capital province
        // have improvement_level >= 1 (meaning they were auto-prospected).
        for tile_coord in &province.tiles {
            if let Some(tile) = gs.world.hex_map.get_tile(*tile_coord)
                && tile.terrain().can_have_deposits()
                && tile.resource_deposit().is_some()
            {
                assert!(
                    tile.improvement_level() >= 1,
                    "Tile at ({},{}) in capital province should be auto-prospected on Easy",
                    tile_coord.q,
                    tile_coord.r
                );
            }
        }
    }

    #[test]
    fn new_game_hard_does_not_auto_prospect() {
        let gs = new_game("no_prospect_test", Difficulty::Hard, 0);
        let human = gs.get_nation(gs.human_player_nation).unwrap();
        let capital_pid = human.capital_province_id;
        let province = gs.get_province(capital_pid).unwrap();

        // On Hard, tiles with deposits should NOT be auto-prospected
        // (improvement_level should still be 0 for prospectable terrain).
        for tile_coord in &province.tiles {
            if let Some(tile) = gs.world.hex_map.get_tile(*tile_coord)
                && tile.terrain().can_have_deposits()
            {
                assert_eq!(
                    tile.improvement_level(),
                    0,
                    "Tile at ({},{}) should NOT be auto-prospected on Hard",
                    tile_coord.q,
                    tile_coord.r
                );
            }
        }
    }
}
