//! Core ECS resources tying the Bevy world to the game session.

use bevy::prelude::*;
use frontend_api::Session;
use std::collections::HashMap;

use crate::game::vm::MapTile;

/// The game session. `None` only while a turn is resolving on the async
/// task pool (the task owns the session for that window).
#[derive(Resource)]
pub struct SessionRes(pub Option<Session>);

/// Monotonic counter bumped whenever game state changes (end turn, commands
/// in later milestones). View models and map layers compare against it to
/// know when to recompute.
#[derive(Resource)]
pub struct DataVersion(pub u64);

/// JSON-derived view models recomputed when `version` falls behind
/// [`DataVersion`].
#[derive(Resource, Default)]
pub struct ViewModels {
    pub map: Option<Vec<MapTile>>,
    pub version: u64,
}

/// Fast (q, r) → index lookup into `ViewModels::map`, rebuilt alongside it.
#[derive(Resource, Default)]
pub struct TileIndex {
    pub by_coord: HashMap<(i32, i32), usize>,
}

/// Calendar display for the HUD, updated from each turn report.
#[derive(Resource)]
pub struct TurnInfo {
    pub label: String,
}

impl Default for TurnInfo {
    fn default() -> Self {
        // New games always begin at turn 1 = 1815 Q1; every later label
        // comes verbatim from the turn report.
        Self {
            label: "1815 Q1".to_string(),
        }
    }
}
