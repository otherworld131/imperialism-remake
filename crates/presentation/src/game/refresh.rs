//! Recompute JSON view models whenever the data version moves (or the fog
//! debug toggle changes what the map query should return).

use bevy::prelude::*;

use crate::game::resources::{
    DataVersion, FreshRail, PendingMoveList, PerspectiveNation, PrevLedger, RailEdge,
    RenderSettings, SessionRes, TileIndex, TurnInfo, ViewModels,
};
use crate::game::vm;
use crate::map::layers::RAIL_DIRS;

/// Rotate the fresh-rail tracker against the freshly fetched map tiles: on a
/// turn-label change the difference against the previous turn's edge set
/// becomes the highlighted "laid last turn" set; mid-turn refreshes only
/// absorb newly revealed edges into the baseline (rail cannot be built
/// mid-turn — every order resolves at end turn).
fn update_fresh_rail(fresh: &mut FreshRail, tiles: &[vm::MapTile], turn_label: &str) {
    let current: std::collections::HashSet<RailEdge> = tiles
        .iter()
        .flat_map(|t| {
            t.rail_links
                .iter()
                .filter(|&&dir| dir <= 2)
                .map(|&dir| {
                    let (dq, dr) = RAIL_DIRS[dir as usize];
                    ((t.q, t.r), (t.q + dq, t.r + dr))
                })
                .collect::<Vec<_>>()
        })
        .collect();
    if fresh.fetched_turn.as_deref() == Some(turn_label) {
        fresh.prev_edges.extend(current);
        return;
    }
    fresh.fresh_edges = if fresh.fetched_turn.is_some() {
        current.difference(&fresh.prev_edges).copied().collect()
    } else {
        Default::default()
    };
    fresh.prev_edges = current;
    fresh.fetched_turn = Some(turn_label.to_string());
}

pub fn refresh_view_models(
    session: Res<SessionRes>,
    data_version: Res<DataVersion>,
    settings: Res<RenderSettings>,
    perspective: Res<PerspectiveNation>,
    turn_info: Res<TurnInfo>,
    mut vms: ResMut<ViewModels>,
    mut index: ResMut<TileIndex>,
    mut pending_moves: ResMut<PendingMoveList>,
    mut prev_ledger: ResMut<PrevLedger>,
    mut fresh_rail: ResMut<FreshRail>,
) {
    if vms.version == data_version.0 && vms.fetched_fog_disabled == settings.disable_fog {
        return;
    }
    let Some(session) = session.0.as_ref() else {
        return;
    };
    let refresh_started = std::time::Instant::now();
    let game = session.game();
    let disable_fog = settings.disable_fog;

    match frontend_api::map::get_map_data(game, disable_fog).map(vm::parse_map_tiles) {
        Ok(Ok(mut tiles)) => {
            // Setup-preview transforms (web GameSetup `previewMapTiles`):
            // reveal hidden resources; in non-observer previews also strip
            // the provisional capital/depot/army markers the generator
            // placed (the player picks the real capital).
            if settings.preview_reveal_resources {
                for tile in &mut tiles {
                    tile.resource_hidden = false;
                }
            }
            if settings.preview_hide_ownership {
                for tile in &mut tiles {
                    let was_country_capital = tile.is_country_capital;
                    tile.is_capital = false;
                    tile.is_country_capital = false;
                    tile.improvement_level = 0;
                    tile.rail_links.clear();
                    if was_country_capital {
                        tile.has_depot = false;
                        tile.army_firepower = 0.0;
                        tile.army_unit_count = 0;
                        tile.army_composition = None;
                        tile.naval_firepower = 0;
                        tile.naval_ship_count = 0;
                    }
                }
            }
            index.by_coord = tiles
                .iter()
                .enumerate()
                .map(|(i, t)| ((t.q, t.r), i))
                .collect();
            update_fresh_rail(&mut fresh_rail, &tiles, &turn_info.label);
            vms.map = Some(tiles);
        }
        Ok(Err(err)) => {
            warn!("map view-model decode failed: {err}");
            vms.map = None;
            index.by_coord.clear();
        }
        Err(err) => {
            warn!("get_map_data failed: {}", err.message());
            vms.map = None;
            index.by_coord.clear();
        }
    }

    vms.navy_markers =
        match frontend_api::map::get_navy_markers(game, disable_fog).map(vm::parse_navy_markers) {
            Ok(Ok(markers)) => markers,
            Ok(Err(err)) => {
                warn!("navy-marker view-model decode failed: {err}");
                Vec::new()
            }
            Err(err) => {
                warn!("get_navy_markers failed: {}", err.message());
                Vec::new()
            }
        };

    vms.sea_zones = match frontend_api::map::get_sea_zones(game).map(vm::parse_sea_zones) {
        Ok(Ok(zones)) => zones,
        Ok(Err(err)) => {
            warn!("sea-zone view-model decode failed: {err}");
            Vec::new()
        }
        Err(err) => {
            warn!("get_sea_zones failed: {}", err.message());
            Vec::new()
        }
    };

    vms.diplomacy = match frontend_api::map::get_diplomacy_overlay(game, perspective.0)
        .map(vm::parse_diplomacy_overlay)
    {
        Ok(Ok(overlay)) => Some(overlay),
        Ok(Err(err)) => {
            warn!("diplomacy-overlay decode failed: {err}");
            None
        }
        Err(err) => {
            warn!("get_diplomacy_overlay failed: {}", err.message());
            None
        }
    };

    vms.military =
        match frontend_api::map::get_military_overlay(game).map(vm::parse_military_overlay) {
            Ok(Ok(entries)) => entries,
            Ok(Err(err)) => {
                warn!("military-overlay decode failed: {err}");
                Vec::new()
            }
            Err(err) => {
                warn!("get_military_overlay failed: {}", err.message());
                Vec::new()
            }
        };

    vms.civilians =
        match frontend_api::units::get_civilians(game, perspective.0).map(vm::parse_civilians) {
            Ok(Ok(civs)) => Some(civs),
            Ok(Err(err)) => {
                warn!("civilians decode failed: {err}");
                None
            }
            Err(err) => {
                warn!("get_civilians failed: {}", err.message());
                None
            }
        };

    vms.ships = match frontend_api::units::get_ships(game, perspective.0).map(vm::parse_ships) {
        Ok(Ok(ships)) => Some(ships),
        Ok(Err(err)) => {
            warn!("ships decode failed: {err}");
            None
        }
        Err(err) => {
            warn!("get_ships failed: {}", err.message());
            None
        }
    };

    vms.industry = match frontend_api::industry::get_industry_data(game, perspective.0)
        .map(vm::parse_industry)
    {
        Ok(Ok(industry)) => Some(industry),
        Ok(Err(err)) => {
            warn!("industry decode failed: {err}");
            None
        }
        Err(err) => {
            warn!("get_industry_data failed: {}", err.message());
            None
        }
    };

    vms.buildable = match frontend_api::units::get_buildable_units(game, perspective.0)
        .map(vm::parse_buildable_units)
    {
        Ok(Ok(buildable)) => Some(buildable),
        Ok(Err(err)) => {
            warn!("buildable-units decode failed: {err}");
            None
        }
        Err(err) => {
            warn!("get_buildable_units failed: {}", err.message());
            None
        }
    };

    vms.transport = match frontend_api::transport::get_transport_data(game, perspective.0)
        .map(vm::parse_transport)
    {
        Ok(Ok(transport)) => Some(transport),
        Ok(Err(err)) => {
            warn!("transport decode failed: {err}");
            None
        }
        Err(err) => {
            warn!("get_transport_data failed: {}", err.message());
            None
        }
    };

    vms.trade = match frontend_api::trade::get_trade_data(game, perspective.0).map(vm::parse_trade)
    {
        Ok(Ok(trade)) => Some(trade),
        Ok(Err(err)) => {
            warn!("trade decode failed: {err}");
            None
        }
        Err(err) => {
            warn!("get_trade_data failed: {}", err.message());
            None
        }
    };

    vms.diplomacy_screen =
        match frontend_api::diplomacy::get_diplomacy_screen_data(game, perspective.0)
            .map(vm::parse_diplomacy_screen)
        {
            Ok(Ok(screen)) => Some(screen),
            Ok(Err(err)) => {
                warn!("diplomacy-screen decode failed: {err}");
                None
            }
            Err(err) => {
                warn!("get_diplomacy_screen_data failed: {}", err.message());
                None
            }
        };

    vms.proposals = match frontend_api::diplomacy::get_pending_proposals(game, perspective.0)
        .map(vm::parse_proposals)
    {
        Ok(Ok(proposals)) => Some(proposals),
        Ok(Err(err)) => {
            warn!("proposals decode failed: {err}");
            None
        }
        Err(err) => {
            warn!("get_pending_proposals failed: {}", err.message());
            None
        }
    };

    vms.tech = match frontend_api::tech::get_tech_screen_data(game).map(vm::parse_tech_screen) {
        Ok(Ok(tech)) => Some(tech),
        Ok(Err(err)) => {
            warn!("tech-screen decode failed: {err}");
            None
        }
        Err(err) => {
            warn!("get_tech_screen_data failed: {}", err.message());
            None
        }
    };

    // Great-Power ledger. The previous-turn snapshot rotates only when the
    // turn label moved since the current entries were fetched (web
    // `prevLedgerTurnRef` parity) so mid-turn command refreshes keep the
    // same delta baseline.
    let ledger = match frontend_api::ledger::get_all_gp_ledger_data(game).map(vm::parse_gp_ledger) {
        Ok(Ok(entries)) => entries,
        Ok(Err(err)) => {
            warn!("gp-ledger decode failed: {err}");
            Vec::new()
        }
        Err(err) => {
            warn!("get_all_gp_ledger_data failed: {}", err.message());
            Vec::new()
        }
    };
    if let Some(fetched_turn) = prev_ledger.fetched_turn.as_ref()
        && *fetched_turn != turn_info.label
    {
        prev_ledger.entries = std::mem::take(&mut vms.ledger);
    }
    prev_ledger.fetched_turn = Some(turn_info.label.clone());
    vms.ledger = ledger;

    // Nation roster (names, colors, types, government titles, flag SVGs)
    // for the newspaper country filter and the legend screen.
    vms.nations = match vm::parse_nation_roster(frontend_api::flavor::get_nation_flags(game)) {
        Ok(nations) => nations,
        Err(err) => {
            warn!("nation-roster decode failed: {err}");
            Vec::new()
        }
    };

    let pending = match frontend_api::units::get_pending_unit_moves(game, perspective.0)
        .map(vm::parse_pending_moves)
    {
        Ok(Ok(moves)) => moves,
        Ok(Err(err)) => {
            warn!("pending-move decode failed: {err}");
            Vec::new()
        }
        Err(err) => {
            warn!("get_pending_unit_moves failed: {}", err.message());
            Vec::new()
        }
    };
    if pending_moves.0 != pending {
        pending_moves.0 = pending;
    }

    vms.version = data_version.0;
    vms.fetched_fog_disabled = disable_fog;
    info!(
        "view models refreshed (version {}): {:.1?}",
        data_version.0,
        refresh_started.elapsed(),
    );
}
