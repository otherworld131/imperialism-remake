//! Recompute JSON view models whenever the data version moves (or the fog
//! debug toggle changes what the map query should return).

use bevy::prelude::*;

use crate::game::resources::{
    DataVersion, PerspectiveNation, RenderSettings, SessionRes, TileIndex, ViewModels,
};
use crate::game::vm;

pub fn refresh_view_models(
    session: Res<SessionRes>,
    data_version: Res<DataVersion>,
    settings: Res<RenderSettings>,
    perspective: Res<PerspectiveNation>,
    mut vms: ResMut<ViewModels>,
    mut index: ResMut<TileIndex>,
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

    vms.version = data_version.0;
    vms.fetched_fog_disabled = disable_fog;
}
