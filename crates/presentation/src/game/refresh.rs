//! Recompute JSON view models whenever the data version moves.

use bevy::prelude::*;

use crate::game::resources::{DataVersion, SessionRes, TileIndex, ViewModels};
use crate::game::vm;

pub fn refresh_view_models(
    session: Res<SessionRes>,
    data_version: Res<DataVersion>,
    mut vms: ResMut<ViewModels>,
    mut index: ResMut<TileIndex>,
) {
    if vms.version == data_version.0 {
        return;
    }
    let Some(session) = session.0.as_ref() else {
        return;
    };

    // disable_fog: observer mode watches the whole board.
    match frontend_api::map::get_map_data(session.game(), true).map(vm::parse_map_tiles) {
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
    vms.version = data_version.0;
}
