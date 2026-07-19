//! Map-rendering queries: tile data, fog-of-war, navy markers, sea zones,
//! overlays and the historical political snapshot.
//!
//! Verbatim moves from `crates/wasm-bridge/src/lib.rs` — bodies must stay
//! byte-identical to the originals (error JSON strings included).

use crate::ApiError;
use crate::guards::{pending_break_treaties, pending_grant_amount_dollars};
use domain::events::TreatyType;
use domain::game_state::GameState;
use domain::hex::HexCoord;
use domain::military::ships::{Ship, ShipCategory};
use domain::types::*;

/// Compute the set of hexes visible to the human player under fog-of-war.
///
/// A hex is visible if:
/// (a) it belongs to one of the player's provinces, or a province hosting one
///     of the player's army units ("occupied" provinces);
/// (b) it belongs to a province sharing a hex edge with an occupied province;
/// (c) it's a non-province hex directly adjacent to an occupied province;
/// (d) it lies in a sea zone that touches an occupied province (the whole zone
///     is revealed, not just the strip next to the coast);
/// (e) it lies in the sea zone where one of the player's ships currently sits,
///     or in any sea zone bordering that one; or
/// (f) it belongs to a coastal province of the sea zone where one of the
///     player's ships currently sits.
///
/// Returns an empty set when `disable_fog` is true (callers should
/// special-case that).
pub fn compute_visible_hexes(
    game: &GameState,
    disable_fog: bool,
) -> std::collections::HashSet<domain::hex::HexCoord> {
    if disable_fog {
        return std::collections::HashSet::new();
    }
    let human_nation_id = game.human_player_nation;
    let mut visible: std::collections::HashSet<domain::hex::HexCoord> =
        std::collections::HashSet::new();

    // "Occupied" = owned OR hosting one of our army units. A unit posted in a
    // foreign province (allied transit, captured-but-not-incorporated, etc.)
    // grants the same reconnaissance as a homeland province.
    let mut occupied_provinces: std::collections::HashSet<ProvinceId> =
        std::collections::HashSet::new();
    for province in &game.world.provinces {
        if province.owner == human_nation_id {
            occupied_provinces.insert(province.id);
        }
    }
    if let Some(human) = game.get_nation(human_nation_id) {
        for unit in &human.military.army {
            occupied_provinces.insert(unit.position);
        }
    }

    let mut border_ring: std::collections::HashSet<domain::hex::HexCoord> =
        std::collections::HashSet::new();
    for province in &game.world.provinces {
        if occupied_provinces.contains(&province.id) {
            for &coord in &province.tiles {
                visible.insert(coord);
                for nb in coord.neighbors() {
                    border_ring.insert(nb);
                }
            }
        }
    }

    for province in &game.world.provinces {
        if occupied_provinces.contains(&province.id) {
            continue;
        }
        if province.tiles.iter().any(|t| border_ring.contains(t)) {
            for &coord in &province.tiles {
                visible.insert(coord);
            }
        }
    }

    for coord in &border_ring {
        visible.insert(*coord);
    }

    // Collect the set of sea zones to fully reveal.
    //   - any zone bordering an occupied province (point 2);
    //   - any zone holding one of our ships, plus that zone's own neighbours
    //     (point 1: line-of-sight extends one step from the ship's *current*
    //     zone — not from every zone it could move into).
    let mut visible_zones: std::collections::HashSet<domain::map::sea_zones::SeaZoneId> =
        std::collections::HashSet::new();

    for zone in &game.world.sea_zones {
        if zone
            .coastal_provinces
            .iter()
            .any(|pid| occupied_provinces.contains(pid))
        {
            visible_zones.insert(zone.id);
        }
    }

    let mut occupied_zones: std::collections::HashSet<domain::map::sea_zones::SeaZoneId> =
        std::collections::HashSet::new();
    if let Some(human) = game.get_nation(human_nation_id) {
        for ship in human
            .military
            .warships
            .iter()
            .chain(human.military.merchant_fleet.iter())
        {
            if let Some(z) = ship.sea_zone {
                occupied_zones.insert(z);
            }
        }
    }

    // Coastal provinces of zones currently holding one of our ships become
    // fully visible — the ship can see the shore it's parked next to. We do
    // *not* extend this to adjacent zones' coastal provinces: a ship one
    // zone away from a foreign coast shouldn't reveal that coast outright.
    let mut visible_coastal: std::collections::HashSet<ProvinceId> =
        std::collections::HashSet::new();
    for zone in &game.world.sea_zones {
        if !occupied_zones.contains(&zone.id) {
            continue;
        }
        visible_zones.insert(zone.id);
        for &adj in &zone.adjacent_zone_ids {
            visible_zones.insert(adj);
        }
        for pid in &zone.coastal_provinces {
            visible_coastal.insert(*pid);
        }
    }

    for zone in &game.world.sea_zones {
        if visible_zones.contains(&zone.id) {
            for &hex in &zone.hexes {
                visible.insert(hex);
            }
        }
    }
    for province in &game.world.provinces {
        if visible_coastal.contains(&province.id) {
            for &coord in &province.tiles {
                visible.insert(coord);
            }
        }
    }

    visible
}

/// Get map data for rendering. Returns JSON array of tile objects.
/// `disable_fog` — when true, all tiles are visible and enemy data is not filtered.
pub fn get_map_data(game: &GameState, disable_fog: bool) -> Result<serde_json::Value, ApiError> {
    let human_nation_id = game.human_player_nation;

    // Build province→nation lookup using Province.owner (the ground truth)
    // and identify country capitals
    let nation_lookup: std::collections::HashMap<NationId, (&str, String)> = game
        .world
        .nations
        .iter()
        .map(|n| (n.id, (n.name.as_str(), format!("{:?}", n.color))))
        .collect();
    let nation_type_lookup: std::collections::HashMap<NationId, NationType> = game
        .world
        .nations
        .iter()
        .map(|n| (n.id, n.nation_type))
        .collect();
    let nation_anarchy_lookup: std::collections::HashMap<NationId, bool> = game
        .world
        .nations
        .iter()
        .map(|n| (n.id, n.diplomacy.is_in_anarchy))
        .collect();
    let mut province_nation: std::collections::HashMap<ProvinceId, (String, String, NationId)> =
        std::collections::HashMap::new();
    for prov in &game.world.provinces {
        if let Some((name, color)) = nation_lookup.get(&prov.owner) {
            province_nation.insert(prov.id, (name.to_string(), color.clone(), prov.owner));
        }
    }
    // Build province → incorporated_from lookup
    let province_incorporated: std::collections::HashMap<ProvinceId, Option<NationId>> = game
        .world
        .provinces
        .iter()
        .map(|p| (p.id, p.incorporated_from))
        .collect();

    let mut country_capital_provinces: std::collections::HashSet<ProvinceId> =
        std::collections::HashSet::new();
    for nation in &game.world.nations {
        country_capital_provinces.insert(nation.capital_province_id);
    }

    // Build province → (total army FP, unit count) lookup
    let mut province_army: std::collections::HashMap<ProvinceId, (f64, u32)> =
        std::collections::HashMap::new();
    // Per-province composition breakdown keyed by unit-type name. Used by the
    // hex tooltip to show "Guards × 2, Regulars × 3" at capitals.
    let mut province_army_composition: std::collections::HashMap<
        ProvinceId,
        std::collections::BTreeMap<String, u32>,
    > = std::collections::HashMap::new();
    for nation in &game.world.nations {
        for unit in &nation.military.army {
            let e = province_army.entry(unit.position).or_insert((0.0, 0));
            e.0 += unit.effective_firepower();
            e.1 += 1;
            let bucket = province_army_composition.entry(unit.position).or_default();
            *bucket.entry(format!("{:?}", unit.unit_type)).or_insert(0) += 1;
        }
    }

    // Build nation → (naval FP, warship count) lookup
    let nation_naval: std::collections::HashMap<NationId, (u32, usize)> = game
        .world
        .nations
        .iter()
        .map(|n| {
            (
                n.id,
                (n.total_naval_firepower(&game.game_data), n.warship_count()),
            )
        })
        .collect();

    // Build hex coord → civilian lookup for ALL nations
    let mut civilian_on_tile: std::collections::HashMap<domain::hex::HexCoord, serde_json::Value> =
        std::collections::HashMap::new();
    for nation in &game.world.nations {
        let (nation_name, nation_color) = nation_lookup
            .get(&nation.id)
            .map(|(name, color)| (*name, color.as_str()))
            .unwrap_or(("", ""));
        let is_human = nation.id == human_nation_id;
        for civ in &nation.military.civilians {
            if let Some(pos) = civ.position {
                // If tile already has a civilian, only overwrite if this is the human player
                if civilian_on_tile.contains_key(&pos) && !is_human {
                    continue;
                }
                civilian_on_tile.insert(
                    pos,
                    serde_json::json!({
                        "id": civ.id.0,
                        "type": format!("{}", civ.civilian_type),
                        "working": civ.working,
                        "turns_remaining": civ.turns_remaining,
                        "build_task": civ.build_task.map(|t| format!("{}", t)),
                        "owner": nation_name,
                        "owner_color": nation_color,
                        "is_human": is_human,
                    }),
                );
            }
        }
    }

    let visible_hexes = compute_visible_hexes(game, disable_fog);

    // Card #408: precompute the set of port-tile coords blockaded for the
    // human player so the UI can render them with a "blockaded" indicator.
    let blockaded_ports = domain::military::naval::compute_blockaded_ports(game, human_nation_id);

    let map_width = game.world.hex_map.width();
    let map_height = game.world.hex_map.height();

    let tiles: Vec<serde_json::Value> = game
        .world.hex_map
        .all_tiles()
        .map(|(coord, tile)| {
            let is_visible = disable_fog || visible_hexes.contains(&coord);

            let (owner_name, owner_color, owner_nation_id) = tile
                .province_id
                .and_then(|pid| province_nation.get(&pid))
                .map(|(n, c, nid)| (n.as_str(), c.as_str(), nid.0))
                .unwrap_or(("", "", 0));

            let province_name = tile
                .province_id
                .and_then(|pid| game.get_province(pid))
                .map(|p| p.name.as_str())
                .unwrap_or("");

            // Minor nation / incorporated status
            let owner_nid = NationId(owner_nation_id);
            let is_minor = owner_nation_id != 0
                && nation_type_lookup
                    .get(&owner_nid)
                    .copied()
                    .unwrap_or(NationType::GreatPower)
                    == NationType::MinorNation;
            let incorporated_from_id = tile
                .province_id
                .and_then(|pid| province_incorporated.get(&pid).copied().flatten());
            let is_incorporated_minor = incorporated_from_id.is_some();

            // Visual group: for incorporated provinces, use the minor nation's name;
            // otherwise use the owner name. Controls border grouping.
            let visual_group: Option<&str> = if let Some(inc_nid) = incorporated_from_id {
                nation_lookup.get(&inc_nid).map(|(name, _)| *name)
            } else {
                None
            };

            // For independent minor nations, override display color to Beige
            let display_color = if is_minor && !is_incorporated_minor && !owner_color.is_empty() {
                "Beige"
            } else {
                owner_color
            };

            // A tile is a country capital if it's marked as capital AND is in
            // the nation's capital province
            let is_country_capital = tile.is_capital
                && tile
                    .province_id
                    .is_some_and(|pid| country_capital_provinces.contains(&pid));

            // Strength data — only on capital tiles, filtered by fog of war
            let (army_fp, army_count) = if tile.is_capital && is_visible {
                tile.province_id
                    .and_then(|pid| province_army.get(&pid))
                    .copied()
                    .unwrap_or((0.0, 0))
            } else {
                (0.0, 0)
            };

            let army_composition: Option<&std::collections::BTreeMap<String, u32>> =
                if tile.is_capital && is_visible {
                    tile.province_id
                        .and_then(|pid| province_army_composition.get(&pid))
                } else {
                    None
                };

            let (naval_fp, naval_count) = if is_country_capital && is_visible {
                nation_naval
                    .get(&NationId(owner_nation_id))
                    .copied()
                    .unwrap_or((0, 0))
            } else {
                (0, 0)
            };

            // Civilian data — hidden on fogged tiles
            let civ_data = if is_visible {
                civilian_on_tile.get(&coord)
            } else {
                None
            };

            serde_json::json!({
                "q": coord.q,
                "r": coord.r,
                "terrain": format!("{:?}", tile.terrain()),
                "resource": tile.resource_deposit().map(|r| format!("{:?}", r)),
                "resource_hidden": tile.resource_deposit().is_some() && !tile.has_visible_resource(),
                "has_river": tile.has_river(),
                "is_capital": tile.is_capital,
                "is_country_capital": is_country_capital,
                "improvement_level": tile.improvement_level(),
                "max_improvement_level": tile.resource_deposit().map(|r| r.max_improvement_level()).unwrap_or(0),
                "owner": owner_name,
                "owner_color": display_color,
                "province": province_name,
                "province_id": tile.province_id.map(|pid| pid.0),
                "rail_links": domain::hex::HEX_DIRECTIONS
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| game.world.hex_map.has_rail_link(coord, coord + **d))
                    .map(|(i, _)| i as u8)
                    .collect::<Vec<u8>>(),
                "has_depot": tile.infrastructure.has_depot,
                "has_port": tile.infrastructure.has_port,
                "port_blockaded": blockaded_ports.contains(&coord),
                "has_fort": tile.infrastructure.has_fort,
                "fort_level": tile.infrastructure.fort_level,
                "map_width": map_width,
                "map_height": map_height,
                "nation_id": owner_nation_id,
                "army_firepower": army_fp,
                "army_unit_count": army_count,
                "army_composition": army_composition,
                "naval_firepower": naval_fp,
                "naval_ship_count": naval_count,
                "civilian_on_tile": civ_data,
                "is_minor": is_minor,
                "is_incorporated_minor": is_incorporated_minor,
                "incorporated_nation_id": incorporated_from_id.map(|n| n.0),
                "is_anarchic": nation_anarchy_lookup.get(&owner_nid).copied().unwrap_or(false),
                "visual_group": visual_group,
                "visible": is_visible,
                "is_prospected": tile.is_prospected(),
            })
        })
        .collect();

    Ok(serde_json::Value::Array(tiles))
}

/// Get navy markers for map rendering. One aggregate marker per
/// (nation, fleet|beachhead-target). Returns a JSON array.
///
/// Fog of war: markers belonging to other nations are only returned when their
/// anchor hex is visible to the human player (same visibility rule as the
/// map-data call). With `disable_fog = true`, all markers are returned.
pub fn get_navy_markers(
    game: &GameState,
    disable_fog: bool,
) -> Result<serde_json::Value, ApiError> {
    use domain::military::navy_placement::{beachhead_anchor, beachhead_coast_tile, fleet_anchor};

    let human_nation_id = game.human_player_nation;
    let visible_hexes = compute_visible_hexes(game, disable_fog);

    let province_name_by_id: std::collections::HashMap<ProvinceId, &str> = game
        .world
        .provinces
        .iter()
        .map(|p| (p.id, p.name.as_str()))
        .collect();

    let mut markers: Vec<serde_json::Value> = Vec::new();

    for nation in &game.world.nations {
        if nation.military.warships.is_empty() {
            continue;
        }

        let owner_name = nation.name.as_str();
        let owner_color = format!("{:?}", nation.color);
        let iron_navy = nation_has_iron_navy_tech(game, nation);

        // Fleet markers always represent ships at their actual location. A
        // Beachhead assignment is just intent until `pending_landings`
        // confirms a real landing site, so keep those ships with the fleet
        // marker and emit a separate beachhead marker only after establishment.
        let established_beachhead_targets: std::collections::BTreeSet<ProvinceId> = game
            .transient
            .pending_landings
            .iter()
            .filter(|(nid, _, _)| *nid == nation.id)
            .map(|(_, pid, _)| *pid)
            .collect();
        let fleet_group: Vec<&Ship> = nation
            .military
            .warships
            .iter()
            .filter(|ship| ship.ship_type.category() == ShipCategory::Warship)
            .filter(|ship| match ship.operation {
                Some(domain::military::naval::NavalOperation::Beachhead(pid)) => {
                    !established_beachhead_targets.contains(&pid)
                }
                _ => true,
            })
            .collect();

        // ── Fleet markers (grouped by sea zone) ──────────────────
        if !fleet_group.is_empty() {
            // Group ships by their sea_zone field.
            let mut by_zone: std::collections::BTreeMap<Option<u32>, Vec<&Ship>> =
                std::collections::BTreeMap::new();
            for ship in &fleet_group {
                by_zone
                    .entry(ship.sea_zone.map(|sz| sz.0))
                    .or_default()
                    .push(ship);
            }

            for (zone_id_opt, zone_ships) in by_zone {
                // Resolve anchor: use zone centroid when zone is known, else fall
                // back to fleet_anchor (home-port proximity rule).
                let (anchor, sz_id, sz_name) = if let Some(zone_id) = zone_id_opt {
                    let zone = game.world.sea_zones.iter().find(|z| z.id.0 == zone_id);
                    if let Some(z) = zone {
                        if z.hexes.is_empty() {
                            // Empty zone — fall back to fleet_anchor
                            let Some(a) =
                                fleet_anchor(nation, &game.world.hex_map, &game.world.provinces)
                            else {
                                continue;
                            };
                            (a, Some(zone_id), Some(z.name.clone()))
                        } else {
                            // Median centroid of zone hexes
                            let mut qs: Vec<i32> = z.hexes.iter().map(|h| h.q).collect();
                            let mut rs: Vec<i32> = z.hexes.iter().map(|h| h.r).collect();
                            qs.sort_unstable();
                            rs.sort_unstable();
                            let cq = qs[qs.len() / 2];
                            let cr = rs[rs.len() / 2];
                            (HexCoord::new(cq, cr), Some(zone_id), Some(z.name.clone()))
                        }
                    } else {
                        // Zone id not found — fall back
                        let Some(a) =
                            fleet_anchor(nation, &game.world.hex_map, &game.world.provinces)
                        else {
                            continue;
                        };
                        (a, None, None)
                    }
                } else {
                    // No zone assigned — use legacy fleet_anchor and back-fill
                    // the sea-zone id by looking up which zone contains that
                    // anchor. Without this back-fill, fleets created before
                    // the AI assigns them a home zone (typical for the human
                    // player at turn 1) ship with sea_zone_id=null, which
                    // leaves the frontend unable to compute fleet-move
                    // adjacency targets (card #471).
                    let Some(a) = fleet_anchor(nation, &game.world.hex_map, &game.world.provinces)
                    else {
                        continue;
                    };
                    let containing = game
                        .world
                        .sea_zones
                        .iter()
                        .find(|z| z.hexes.iter().any(|h| h.q == a.q && h.r == a.r));
                    match containing {
                        Some(z) => (a, Some(z.id.0), Some(z.name.clone())),
                        None => (a, None, None),
                    }
                };

                let is_human = nation.id == human_nation_id;
                let is_visible = if disable_fog || is_human {
                    true
                } else if let Some(zone_id) = zone_id_opt {
                    game.world
                        .sea_zones
                        .iter()
                        .find(|z| z.id.0 == zone_id)
                        .is_some_and(|z| z.hexes.iter().any(|hex| visible_hexes.contains(hex)))
                } else {
                    visible_hexes.contains(&anchor)
                };
                if !is_visible {
                    continue;
                }
                if let Some(mut marker) = build_marker(
                    anchor,
                    nation.id,
                    owner_name,
                    &owner_color,
                    "fleet",
                    iron_navy,
                    None,
                    None,
                    &zone_ships,
                    &game.game_data,
                ) {
                    if let Some(id) = sz_id {
                        marker["sea_zone_id"] = serde_json::Value::Number(id.into());
                        // Card #471: surface the queued destination so the
                        // frontend can draw a pending-move arrow from this
                        // marker to the destination zone's centroid (mirrors
                        // how army `pending_moves` are rendered).
                        if let Some((_, _, to_z)) = game
                            .transient
                            .pending_fleet_moves
                            .iter()
                            .find(|(n, fz, _)| *n == nation.id && fz.0 == id)
                        {
                            marker["pending_move_to_zone_id"] =
                                serde_json::Value::Number(to_z.0.into());
                        }
                    }
                    if let Some(name) = sz_name {
                        marker["sea_zone_name"] = serde_json::Value::String(name);
                    }
                    markers.push(marker);
                }
            }
        }

        // ── Beachhead markers ────────────────────────────────────
        for pid in established_beachhead_targets {
            let ships: Vec<&Ship> = nation
                .military
                .warships
                .iter()
                .filter(|ship| ship.ship_type.category() == ShipCategory::Warship)
                .filter(|ship| {
                    ship.operation == Some(domain::military::naval::NavalOperation::Beachhead(pid))
                })
                .collect();
            if ships.is_empty() {
                continue;
            }
            let target = match game.get_province(pid) {
                Some(p) => p,
                None => continue,
            };
            let anchor = match beachhead_anchor(&game.world.hex_map, target) {
                Some(a) => a,
                None => continue,
            };
            let is_human = nation.id == human_nation_id;
            let is_visible = disable_fog || is_human || visible_hexes.contains(&anchor);
            if !is_visible {
                continue;
            }
            let coast_tile = beachhead_coast_tile(&game.world.hex_map, target);
            let target_province_name = province_name_by_id
                .get(&pid)
                .copied()
                .unwrap_or("")
                .to_string();
            if let Some(marker) = build_marker(
                anchor,
                nation.id,
                owner_name,
                &owner_color,
                "beachhead",
                iron_navy,
                Some(target_province_name),
                coast_tile,
                &ships,
                &game.game_data,
            ) {
                markers.push(marker);
            }
        }
    }

    Ok(serde_json::Value::Array(markers))
}

/// True when `nation` has researched a technology that unlocks an iron /
/// steam warship (ship era ≥ 2, e.g. "Advanced Iron Working" → Ironclad).
/// Drives the fleet-marker art era on the map (card #544): sail silhouettes
/// before, battleship/cruiser silhouettes after.
fn nation_has_iron_navy_tech(game: &GameState, nation: &domain::nation::Nation) -> bool {
    use domain::military::ships::ShipType;
    use domain::tech::TechEffect;
    nation.researched_techs.iter().any(|tid| {
        game.game_data.tech_tree.get(*tid).is_some_and(|tech| {
            tech.effects.iter().any(|effect| match effect {
                TechEffect::UnlockShip(name) => name.parse::<ShipType>().is_ok_and(|ship_type| {
                    let stats = game.game_data.ship_stats(ship_type);
                    stats.category == ShipCategory::Warship && stats.era >= 2
                }),
                _ => false,
            })
        })
    })
}

#[allow(clippy::too_many_arguments)]
fn build_marker(
    anchor: domain::hex::HexCoord,
    nation_id: NationId,
    owner_name: &str,
    owner_color: &str,
    kind: &str,
    iron_navy: bool,
    target_province: Option<String>,
    target_hex: Option<domain::hex::HexCoord>,
    ships: &[&Ship],
    data: &domain::data::GameData,
) -> Option<serde_json::Value> {
    if ships.is_empty() {
        return None;
    }

    let mut by_type: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    let mut by_operation: std::collections::BTreeMap<String, u32> =
        std::collections::BTreeMap::new();
    let mut total_fp: u32 = 0;
    let mut total_hull: u32 = 0;

    for ship in ships {
        let type_key = format!("{:?}", ship.ship_type);
        *by_type.entry(type_key).or_insert(0) += 1;
        let op_key = format_operation(ship.operation);
        *by_operation.entry(op_key).or_insert(0) += 1;
        total_fp += data.ship_stats(ship.ship_type).firepower;
        total_hull += ship.hull_remaining;
    }

    let mut json = serde_json::json!({
        "q": anchor.q,
        "r": anchor.r,
        "nation_id": nation_id.0,
        "owner_name": owner_name,
        "owner_color": owner_color,
        "kind": kind,
        "ship_count": ships.len(),
        "total_fp": total_fp,
        "total_hull": total_hull,
        "by_type": by_type,
        "by_operation": by_operation,
        // Card #544: the owning nation has unlocked iron/steam warships —
        // the map draws battleship/cruiser silhouettes instead of sail ships.
        "iron_navy": iron_navy,
        // Always true at emission — invisible markers are filtered upstream.
        // The field is kept in the contract so the frontend never has to
        // re-derive visibility.
        "visible": true,
    });
    if let Some(name) = target_province {
        json["target_province"] = serde_json::Value::String(name);
    }
    if let Some(hex) = target_hex {
        json["target_hex"] = serde_json::json!({ "q": hex.q, "r": hex.r });
    }
    Some(json)
}

fn format_operation(op: Option<domain::military::naval::NavalOperation>) -> String {
    use domain::military::naval::NavalOperation;
    match op {
        None => "Idle".to_string(),
        Some(NavalOperation::Patrol) => "Patrol".to_string(),
        Some(NavalOperation::Escort) => "Escort".to_string(),
        Some(NavalOperation::Blockade(n)) => format!("Blockade(n{})", n.0),
        Some(NavalOperation::Beachhead(p)) => format!("Beachhead(p{})", p.0),
        Some(NavalOperation::Reconnaissance(n)) => format!("Recon(n{})", n.0),
    }
}

/// Get sea zone data for map rendering.
///
/// Returns a JSON array of zones:
/// `[{id, name, is_lake, center_q, center_r, hexes: [{q, r}]}]`
///
/// `center_q` / `center_r` are the median q and r of the zone's hexes
/// (deterministic centroid). Zones with no hexes are omitted.
pub fn get_sea_zones(game: &GameState) -> Result<serde_json::Value, ApiError> {
    let zones: Vec<serde_json::Value> = game
        .world
        .sea_zones
        .iter()
        .filter(|z| !z.hexes.is_empty())
        .map(|z| {
            // Median q and r as a deterministic center estimate.
            let mut qs: Vec<i32> = z.hexes.iter().map(|h| h.q).collect();
            let mut rs: Vec<i32> = z.hexes.iter().map(|h| h.r).collect();
            qs.sort_unstable();
            rs.sort_unstable();
            let center_q = qs[qs.len() / 2];
            let center_r = rs[rs.len() / 2];

            let hexes: Vec<serde_json::Value> = z
                .hexes
                .iter()
                .map(|h| serde_json::json!({ "q": h.q, "r": h.r }))
                .collect();

            let adjacent: Vec<u32> = z.adjacent_zone_ids.iter().map(|id| id.0).collect();

            serde_json::json!({
                "id": z.id.0,
                "name": z.name,
                "is_lake": z.is_lake,
                "center_q": center_q,
                "center_r": center_r,
                "hexes": hexes,
                "adjacent_zone_ids": adjacent,
            })
        })
        .collect();

    Ok(serde_json::Value::Array(zones))
}

/// Get diplomacy overlay data for a specific nation's perspective.
/// Returns JSON with relations from the selected nation to all others.
pub fn get_diplomacy_overlay(
    game: &GameState,
    nation_id: u32,
) -> Result<serde_json::Value, ApiError> {
    let selected_nid = NationId(nation_id);
    let selected_name = game
        .get_nation(selected_nid)
        .map(|n| n.name.as_str())
        .unwrap_or("Unknown");
    let selected_in_anarchy = game
        .get_nation(selected_nid)
        .is_some_and(|n| n.diplomacy.is_in_anarchy);

    let relations: Vec<serde_json::Value> = game
        .world
        .nations
        .iter()
        .filter(|n| n.id != selected_nid)
        .map(|n| {
            let rel = game.world.diplomacy.get_relation(selected_nid, n.id);
            // Card #31: a nation in anarchy is displayed as at war with
            // everyone regardless of the underlying relation record. This
            // must match the diplomacy-screen override so the two surfaces
            // agree. Either side being anarchic forces "At War".
            let target_in_anarchy = n.diplomacy.is_in_anarchy;
            let raw_at_war = rel.map(|r| r.at_war).unwrap_or(false);
            let at_war = raw_at_war || target_in_anarchy || selected_in_anarchy;
            let (status, score) = match rel {
                Some(r) => {
                    let s = if at_war {
                        "At War"
                    } else if r.has_treaty(domain::events::TreatyType::Alliance) {
                        "Alliance"
                    } else if r.has_treaty(domain::events::TreatyType::NonAggressionPact) {
                        "NAP"
                    } else {
                        "Neutral"
                    };
                    (s, r.score)
                }
                None => (if at_war { "At War" } else { "Neutral" }, 0),
            };
            let treaties: Vec<String> = rel
                .map(|r| {
                    r.active_treaties
                        .iter()
                        .map(|t| format!("{:?}", t))
                        .collect()
                })
                .unwrap_or_default();
            let has_consulate = rel.map(|r| r.has_consulate).unwrap_or(false);
            let has_embassy = rel.map(|r| r.has_embassy).unwrap_or(false);
            let has_pending_consulate = game.has_pending_consulate(selected_nid, n.id);
            let has_pending_embassy = game.has_pending_embassy(selected_nid, n.id);
            let has_pending_war = game.has_pending_war(selected_nid, n.id);
            let pending_grant_amount_dollars =
                pending_grant_amount_dollars(game, selected_nid, n.id);
            let pending_break_treaties: Vec<String> =
                pending_break_treaties(game, selected_nid, n.id)
                    .into_iter()
                    .map(|t| format!("{:?}", t))
                    .collect();
            let has_pending_nap = game.world.diplomacy.pending_proposals.iter().any(|p| {
                p.proposal_type == TreatyType::NonAggressionPact
                    && p.from == selected_nid
                    && p.to == n.id
            });
            let has_pending_alliance = game.world.diplomacy.pending_proposals.iter().any(|p| {
                p.proposal_type == TreatyType::Alliance && p.from == selected_nid && p.to == n.id
            });
            let has_pending_peace = game.world.diplomacy.pending_proposals.iter().any(|p| {
                p.proposal_type == TreatyType::PeaceTreaty && p.from == selected_nid && p.to == n.id
            });

            serde_json::json!({
                "nation_name": n.name,
                "nation_id": n.id.0,
                "nation_color": format!("{:?}", n.color),
                "score": score,
                "at_war": at_war,
                "status": status,
                "treaties": treaties,
                "has_consulate": has_consulate,
                "has_embassy": has_embassy,
                "has_pending_consulate": has_pending_consulate,
                "has_pending_embassy": has_pending_embassy,
                "has_pending_war": has_pending_war,
                "pending_grant_amount_dollars": pending_grant_amount_dollars,
                "pending_break_treaties": pending_break_treaties,
                "has_pending_nap": has_pending_nap,
                "has_pending_alliance": has_pending_alliance,
                "has_pending_peace": has_pending_peace,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "selected_nation": selected_name,
        "selected_nation_id": nation_id,
        "relations": relations,
    }))
}

/// Get military overlay data for all nations (army + naval strength summaries).
pub fn get_military_overlay(game: &GameState) -> Result<serde_json::Value, ApiError> {
    let entries: Vec<serde_json::Value> = game
        .world
        .nations
        .iter()
        .map(|n| {
            serde_json::json!({
                "nation_name": n.name,
                "nation_id": n.id.0,
                "nation_color": format!("{:?}", n.color),
                "total_army_fp": n.total_military_firepower(),
                "total_naval_fp": n.total_naval_firepower(&game.game_data),
                "army_unit_count": n.military.army.len(),
                "warship_count": n.warship_count(),
            })
        })
        .collect();

    Ok(serde_json::Value::Array(entries))
}

/// Return a political-map snapshot for a specific past turn. Each tile is
/// annotated with the owning nation at that turn, plus the display flags
/// needed to render a read-only political view in a modal.
///
/// Returns `{"error": "..."}` if the game can't be deserialized or the
/// requested turn has no snapshot.
pub fn get_political_snapshot(game: &GameState, turn: u32) -> Result<serde_json::Value, ApiError> {
    let target = TurnNumber::new(turn);
    let Some((_, snapshot)) = game
        .archive
        .political_archive
        .iter()
        .find(|(t, _)| *t == target)
    else {
        return Err(ApiError::msg(format!(
            "no political snapshot for turn {}",
            turn
        )));
    };

    // Rebuild province_id → (owner NationId, incorporated_from) at that turn.
    let prov_state: std::collections::HashMap<ProvinceId, (NationId, Option<NationId>)> = snapshot
        .provinces
        .iter()
        .map(|&(pid, owner, inc)| (pid, (owner, inc)))
        .collect();

    let nation_lookup: std::collections::HashMap<NationId, (&str, String, NationType)> = game
        .world
        .nations
        .iter()
        .map(|n| {
            (
                n.id,
                (n.name.as_str(), format!("{:?}", n.color), n.nation_type),
            )
        })
        .collect();

    // Capital provinces at the archived turn (not current). Capital can move
    // during the game — using current state would mis-place historical markers.
    let country_capital_provinces: std::collections::HashSet<ProvinceId> =
        snapshot.capitals.iter().map(|&(_, pid)| pid).collect();

    let province_name: std::collections::HashMap<ProvinceId, &str> = game
        .world
        .provinces
        .iter()
        .map(|p| (p.id, p.name.as_str()))
        .collect();

    let map_width = game.world.hex_map.width();
    let map_height = game.world.hex_map.height();

    let tiles: Vec<serde_json::Value> = game
        .world
        .hex_map
        .all_tiles()
        .map(|(coord, tile)| {
            let (owner_name, owner_color, is_minor, is_incorporated_minor, visual_group) = tile
                .province_id
                .and_then(|pid| prov_state.get(&pid).copied())
                .and_then(|(owner, inc)| {
                    nation_lookup.get(&owner).map(|(name, color, ntype)| {
                        let incorporated = inc.is_some();
                        let is_minor = *ntype == NationType::MinorNation;
                        // Independent minors always render as Beige; incorporated
                        // minors keep the overlord color but lighter.
                        let display_color = if is_minor && !incorporated && !color.is_empty() {
                            "Beige".to_string()
                        } else {
                            color.clone()
                        };
                        // Visual group: incorporated minors keep a separate
                        // border group keyed on the original minor's name, so
                        // they read as distinct countries in the political view.
                        let vg: Option<String> = inc
                            .and_then(|nid| nation_lookup.get(&nid))
                            .map(|(n, _, _)| (*n).to_string());
                        (*name, display_color, is_minor, incorporated, vg)
                    })
                })
                .unwrap_or(("", String::new(), false, false, None));

            let prov_name = tile
                .province_id
                .and_then(|pid| province_name.get(&pid).copied())
                .unwrap_or("");

            let is_country_capital = tile.is_capital
                && tile
                    .province_id
                    .is_some_and(|pid| country_capital_provinces.contains(&pid));

            serde_json::json!({
                "q": coord.q,
                "r": coord.r,
                "terrain": format!("{:?}", tile.terrain()),
                "owner": owner_name,
                "owner_color": owner_color,
                "province": prov_name,
                "is_capital": tile.is_capital,
                "is_country_capital": is_country_capital,
                "is_minor": is_minor,
                "is_incorporated_minor": is_incorporated_minor,
                "visual_group": visual_group,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "turn": target.0,
        "year": target.year(),
        "quarter": target.quarter(),
        "map_width": map_width,
        "map_height": map_height,
        "tiles": tiles,
    }))
}
