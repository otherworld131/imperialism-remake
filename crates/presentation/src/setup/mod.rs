//! Game-setup flow (web `GameSetup.tsx` parity): the config step (scenario /
//! difficulty / map key / size / nations / toggles), the live map preview
//! with terrain-mix sliders and nation picking, and the capital-placement
//! substage with yield previews and ranked suggestions.
//!
//! The preview is a REAL game generated on the compute pool and rendered by
//! the regular map renderer; only the chrome here is setup-specific.

pub mod capital;
pub mod jobs;
pub mod ui;

use bevy::prelude::*;

/// One Great Power row in the preview sidebar.
#[derive(Debug, Clone, PartialEq)]
pub struct GpInfo {
    /// Great-Power index (the `nation_index` passed to game constructors).
    pub idx: usize,
    pub id: i64,
    pub name: String,
    pub color: String,
    pub government_title: String,
}

/// Where the setup flow is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SetupStep {
    #[default]
    Config,
    Preview,
}

/// Preview-step substage (non-observer): pick a nation, then its capital.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewStage {
    #[default]
    Nation,
    Capital,
}

/// The 17 terrain-mix knobs, mirroring `domain::map::TerrainMix` (the
/// defaults are read back through `frontend_api::setup::parse_terrain_mix`
/// so there is a single source of truth).
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainMixUi {
    pub grassland: f32,
    pub forest: f32,
    pub hills: f32,
    pub mountain: f32,
    pub desert: f32,
    pub swamp: f32,
    pub tundra: f32,
    pub forest_cluster: i32,
    pub hills_cluster: i32,
    pub mountain_cluster: i32,
    pub desert_cluster: i32,
    pub swamp_cluster: i32,
    pub pole_tundra_strength: f32,
    pub sea_hard_margin: i32,
    pub sea_falloff_radius: i32,
    pub land_amount: f32,
    pub river_source_percent: i32,
}

impl Default for TerrainMixUi {
    fn default() -> Self {
        let mix = frontend_api::setup::parse_terrain_mix("");
        Self {
            grassland: mix.grassland,
            forest: mix.forest,
            hills: mix.hills,
            mountain: mix.mountain,
            desert: mix.desert,
            swamp: mix.swamp,
            tundra: mix.tundra,
            forest_cluster: mix.forest_cluster,
            hills_cluster: mix.hills_cluster,
            mountain_cluster: mix.mountain_cluster,
            desert_cluster: mix.desert_cluster,
            swamp_cluster: mix.swamp_cluster,
            pole_tundra_strength: mix.pole_tundra_strength,
            sea_hard_margin: mix.sea_hard_margin,
            sea_falloff_radius: mix.sea_falloff_radius,
            land_amount: mix.land_amount,
            river_source_percent: mix.river_source_percent,
        }
    }
}

impl TerrainMixUi {
    /// Serialize for the `terrain_json` constructor parameter.
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "grassland": self.grassland,
            "forest": self.forest,
            "hills": self.hills,
            "mountain": self.mountain,
            "desert": self.desert,
            "swamp": self.swamp,
            "tundra": self.tundra,
            "forest_cluster": self.forest_cluster,
            "hills_cluster": self.hills_cluster,
            "mountain_cluster": self.mountain_cluster,
            "desert_cluster": self.desert_cluster,
            "swamp_cluster": self.swamp_cluster,
            "pole_tundra_strength": self.pole_tundra_strength,
            "sea_hard_margin": self.sea_hard_margin,
            "sea_falloff_radius": self.sea_falloff_radius,
            "land_amount": self.land_amount,
            "river_source_percent": self.river_source_percent,
        })
        .to_string()
    }
}

/// The 17 live terrain sliders, with their ranges (`GameSetup.tsx` parity;
/// `SeaRing` / `Falloff` ranges are interdependent and clamped on commit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub enum TerrainField {
    LandAmount,
    SeaRing,
    Falloff,
    RiverSources,
    Grassland,
    Forest,
    Hills,
    Mountain,
    Desert,
    Swamp,
    Tundra,
    ForestCluster,
    HillsCluster,
    MountainCluster,
    DesertCluster,
    SwampCluster,
    PoleTundra,
}

impl TerrainField {
    pub const ALL: [TerrainField; 17] = [
        TerrainField::LandAmount,
        TerrainField::SeaRing,
        TerrainField::Falloff,
        TerrainField::RiverSources,
        TerrainField::Grassland,
        TerrainField::Forest,
        TerrainField::Hills,
        TerrainField::Mountain,
        TerrainField::Desert,
        TerrainField::Swamp,
        TerrainField::Tundra,
        TerrainField::ForestCluster,
        TerrainField::HillsCluster,
        TerrainField::MountainCluster,
        TerrainField::DesertCluster,
        TerrainField::SwampCluster,
        TerrainField::PoleTundra,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::LandAmount => "Land amount",
            Self::SeaRing => "Sea ring (cells)",
            Self::Falloff => "Coastline falloff (cells)",
            Self::RiverSources => "River sources %",
            Self::Grassland => "Grassland",
            Self::Forest => "Forest",
            Self::Hills => "Hills",
            Self::Mountain => "Mountain",
            Self::Desert => "Desert",
            Self::Swamp => "Swamp",
            Self::Tundra => "Tundra",
            Self::ForestCluster => "Forest clustering",
            Self::HillsCluster => "Hills clustering",
            Self::MountainCluster => "Mountain clustering",
            Self::DesertCluster => "Desert clustering",
            Self::SwampCluster => "Swamp clustering",
            Self::PoleTundra => "Tundra at poles %",
        }
    }

    /// (min, max, step) of the slider for the current mix (the sea-ring and
    /// falloff ranges depend on each other).
    pub fn range(self, mix: &TerrainMixUi) -> (f32, f32, f32) {
        match self {
            Self::LandAmount => (0.30, 2.50, 0.05),
            Self::SeaRing => (0.0, (mix.sea_falloff_radius - 1).max(0) as f32, 1.0),
            Self::Falloff => ((mix.sea_hard_margin + 1).max(1) as f32, 20.0, 1.0),
            Self::PoleTundra => (0.0, 100.0, 1.0),
            _ => (0.0, 100.0, 1.0),
        }
    }

    /// Current slider value for the mix.
    pub fn get(self, mix: &TerrainMixUi) -> f32 {
        match self {
            Self::LandAmount => mix.land_amount,
            Self::SeaRing => mix.sea_hard_margin as f32,
            Self::Falloff => mix.sea_falloff_radius as f32,
            Self::RiverSources => mix.river_source_percent as f32,
            Self::Grassland => mix.grassland,
            Self::Forest => mix.forest,
            Self::Hills => mix.hills,
            Self::Mountain => mix.mountain,
            Self::Desert => mix.desert,
            Self::Swamp => mix.swamp,
            Self::Tundra => mix.tundra,
            Self::ForestCluster => mix.forest_cluster as f32,
            Self::HillsCluster => mix.hills_cluster as f32,
            Self::MountainCluster => mix.mountain_cluster as f32,
            Self::DesertCluster => mix.desert_cluster as f32,
            Self::SwampCluster => mix.swamp_cluster as f32,
            Self::PoleTundra => mix.pole_tundra_strength * 100.0,
        }
    }

    /// Write a committed slider value back, keeping the sea-ring/falloff
    /// invariants (falloff must exceed the hard margin).
    pub fn set(self, mix: &mut TerrainMixUi, value: f32) {
        match self {
            Self::LandAmount => mix.land_amount = value.clamp(0.30, 2.50),
            Self::SeaRing => {
                mix.sea_hard_margin = (value as i32).clamp(0, (mix.sea_falloff_radius - 1).max(0));
            }
            Self::Falloff => {
                mix.sea_falloff_radius = (value as i32).clamp(mix.sea_hard_margin + 1, 20);
            }
            Self::RiverSources => mix.river_source_percent = (value as i32).clamp(0, 100),
            Self::Grassland => mix.grassland = value.clamp(0.0, 100.0),
            Self::Forest => mix.forest = value.clamp(0.0, 100.0),
            Self::Hills => mix.hills = value.clamp(0.0, 100.0),
            Self::Mountain => mix.mountain = value.clamp(0.0, 100.0),
            Self::Desert => mix.desert = value.clamp(0.0, 100.0),
            Self::Swamp => mix.swamp = value.clamp(0.0, 100.0),
            Self::Tundra => mix.tundra = value.clamp(0.0, 100.0),
            Self::ForestCluster => mix.forest_cluster = (value as i32).clamp(0, 100),
            Self::HillsCluster => mix.hills_cluster = (value as i32).clamp(0, 100),
            Self::MountainCluster => mix.mountain_cluster = (value as i32).clamp(0, 100),
            Self::DesertCluster => mix.desert_cluster = (value as i32).clamp(0, 100),
            Self::SwampCluster => mix.swamp_cluster = (value as i32).clamp(0, 100),
            Self::PoleTundra => mix.pole_tundra_strength = (value / 100.0).clamp(0.0, 1.0),
        }
    }
}

/// Everything the player chose on the config step (plus the preview-step
/// nation/capital picks). Cloned into [`ActiveGameConfig`] when the campaign
/// begins so Restart can rebuild the exact same world.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct SetupConfig {
    /// `None` = Random Map.
    pub scenario: Option<String>,
    /// 0 Introductory … 4 Brutal.
    pub difficulty: u8,
    /// Raw map-key input; empty = the "imperialism" default seed.
    pub map_key: String,
    /// Names/flags seed; empty = reuse the effective map key.
    pub flavor_key: String,
    pub observer: bool,
    pub organic_borders: bool,
    pub hide_hex_grid: bool,
    pub width: i32,
    pub height: i32,
    pub num_great_powers: u32,
    pub num_minor_nations: u32,
    pub terrain: TerrainMixUi,
    pub picked_nation: Option<usize>,
    pub capital: Option<(i32, i32)>,
}

impl Default for SetupConfig {
    fn default() -> Self {
        Self {
            scenario: None,
            difficulty: 2,
            map_key: String::new(),
            flavor_key: String::new(),
            observer: true,
            organic_borders: true,
            hide_hex_grid: true,
            width: 80,
            height: 50,
            num_great_powers: 7,
            num_minor_nations: 16,
            terrain: TerrainMixUi::default(),
            picked_nation: None,
            capital: None,
        }
    }
}

impl SetupConfig {
    /// Web parity: blank map key falls back to the "imperialism" seed.
    pub fn effective_map_key(&self) -> &str {
        if self.map_key.trim().is_empty() {
            "imperialism"
        } else {
            self.map_key.trim()
        }
    }

    pub fn difficulty_label(&self) -> &'static str {
        DIFFICULTIES[self.difficulty.min(4) as usize]
    }
}

pub const DIFFICULTIES: [&str; 5] = ["Introductory", "Easy", "Normal", "Hard", "Brutal"];

pub const SIZE_PRESETS: [(&str, i32, i32); 3] = [
    ("Small (60×40)", 60, 40),
    ("Medium (80×50)", 80, 50),
    ("Large (120×70)", 120, 70),
];

/// The start parameters of the *active* game, kept for the Restart button
/// (web `gameStartParams`).
#[derive(Resource, Debug, Clone, Default)]
pub struct ActiveGameConfig(pub Option<SetupConfig>);

/// Volatile setup-flow UI state.
#[derive(Resource, Default)]
pub struct SetupUi {
    pub step: SetupStep,
    pub stage: PreviewStage,
    pub error: Option<String>,
    pub show_advanced: bool,
    /// Rebuild the config panel (structure changed: scenario pick,
    /// difficulty, size preset, advanced toggle, error).
    pub config_dirty: bool,
    /// Rebuild the preview chrome (stage change, re-roll, new world).
    pub preview_dirty: bool,
    /// Great Powers of the current preview world.
    pub gps: Vec<GpInfo>,
    pub hovered_capital: Option<capital::CapitalPreview>,
    pub picked_capital: Option<capital::CapitalPreview>,
    /// Suggestion row currently hovered in the sidebar (index into
    /// `suggestions`); takes display priority over the map hover.
    pub sidebar_hovered: Option<usize>,
    pub suggestions: Vec<capital::Suggestion>,
    /// Data version `suggestions` was computed at (`0` = stale).
    pub suggestions_version: u64,
    /// Scenario list fetched once at startup: `(id, name, description)`.
    pub scenarios: Vec<(String, String, String)>,
}

/// Setup-flow actions. Buttons carry a [`SetupActionBtn`]; the M10 debug
/// driver writes these directly so it exercises the same code paths.
#[derive(Message, Debug, Clone, PartialEq)]
pub enum SetupAction {
    SelectScenario(Option<String>),
    SetDifficulty(u8),
    SizePreset(i32, i32),
    ToggleAdvanced,
    OpenLoadModal,
    PreviewMap,
    BackToConfig,
    Reroll,
    RerollNames,
    RandomizeTerrain,
    ResetTerrain,
    PickNation(usize),
    EnterCapitalStage,
    LeaveCapitalStage,
    PickSuggestion(usize),
    BeginCampaign,
    ZoomIn,
    ZoomOut,
    /// `true` = political, `false` = terrain.
    SetMapMode(bool),
}

/// Attach to a kit button to fire a [`SetupAction`] on activation.
#[derive(Component, Debug, Clone)]
pub struct SetupActionBtn(pub SetupAction);

/// Map kit-button activations to setup actions.
pub fn route_action_buttons(
    mut activations: MessageReader<crate::widgets::ButtonActivated>,
    buttons: Query<&SetupActionBtn>,
    mut actions: MessageWriter<SetupAction>,
) {
    for crate::widgets::ButtonActivated(entity) in activations.read() {
        if let Ok(SetupActionBtn(action)) = buttons.get(*entity) {
            actions.write(action.clone());
        }
    }
}

/// Tiny xorshift for the Randomize-terrain / re-roll seeds (the presentation
/// crate has no rand dependency; quality doesn't matter here).
pub struct SetupRng(u64);

impl SetupRng {
    pub fn from_time() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
            .unwrap_or(0x9E37_79B9);
        Self(nanos | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform float in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        let unit = (self.next_u64() >> 11) as f32 / (1u64 << 53) as f32;
        lo + unit * (hi - lo)
    }

    /// Random lowercase base36 seed string (web `randomSeed` parity).
    pub fn seed_string(&mut self) -> String {
        const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
        (0..8)
            .map(|_| ALPHABET[(self.next_u64() % 36) as usize] as char)
            .collect()
    }
}

/// Web `randomizeTerrain` parity.
pub fn randomize_terrain(mix: &mut TerrainMixUi) {
    let mut rng = SetupRng::from_time();
    let hard_margin = rng.range(0.0, 3.0).round() as i32;
    let falloff = rng.range((hard_margin + 2) as f32, 12.0).round() as i32;
    *mix = TerrainMixUi {
        grassland: rng.range(20.0, 70.0),
        forest: rng.range(5.0, 35.0),
        hills: rng.range(3.0, 25.0),
        mountain: rng.range(2.0, 18.0),
        desert: rng.range(0.0, 18.0),
        swamp: rng.range(0.0, 12.0),
        tundra: rng.range(0.0, 10.0),
        forest_cluster: rng.range(10.0, 50.0).round() as i32,
        hills_cluster: rng.range(10.0, 40.0).round() as i32,
        mountain_cluster: rng.range(5.0, 25.0).round() as i32,
        desert_cluster: rng.range(5.0, 30.0).round() as i32,
        swamp_cluster: rng.range(5.0, 25.0).round() as i32,
        pole_tundra_strength: rng.range(0.0, 1.0),
        sea_hard_margin: hard_margin,
        sea_falloff_radius: falloff.max(hard_margin + 1),
        land_amount: rng.range(0.5, 1.6),
        river_source_percent: rng.range(0.0, 70.0).round() as i32,
    };
}
