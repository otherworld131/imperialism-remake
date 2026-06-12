//! Recompute JSON view models whenever the data version moves (or the fog
//! debug toggle changes what the map query should return).

use bevy::prelude::*;

use crate::game::resources::{
    DataVersion, PendingMoveList, PerspectiveNation, PrevLedger, RenderSettings, SessionRes,
    TileIndex, TurnInfo, ViewModels,
};
use crate::game::vm;

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
) {
    if vms.version == data_version.0 && vms.fetched_fog_disabled == settings.disable_fog {
        return;
    }
    let Some(session) = session.0.as_ref() else {
        return;
    };
    let game = session.game();
    let disable_fog = settings.disable_fog;

    match frontend_api::map::get_map_data(game, disable_fog).map(vm::parse_map_tiles) {
        Ok(Ok(tiles)) => {
            index.by_coord = tiles
                .iter()
                .enumerate()
                .map(|(i, t)| ((t.q, t.r), i))
                .collect();
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
}
