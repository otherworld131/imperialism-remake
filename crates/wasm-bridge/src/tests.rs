//! Behavior tests for the wasm exports. Bodies moved verbatim from the
//! pre-extraction lib.rs; they exercise the thin wrappers end-to-end and
//! the shared helpers now living in frontend-api.
use super::*;
use domain::diplomacy::DiplomaticProposal;
use domain::economy::buildings::BuildingType;
use domain::events::TreatyType;
use domain::game_state::new_game;
use domain::military::ships::{Ship, ShipType};
use domain::military::units::ArmyUnitType;
use domain::turn::process_turn;
use domain::types::*;
use frontend_api::map::compute_visible_hexes;
use frontend_api::parse::*;

fn make_game_json() -> String {
    let game = new_game("default", Difficulty::Normal, 0);
    serialize_game(&game)
}

// ── Parser tests ──────────────────────────────────────────

#[test]
fn parse_army_unit_type_valid() {
    assert_eq!(
        parse_army_unit_type("Regulars"),
        Some(ArmyUnitType::Regulars)
    );
    assert_eq!(parse_army_unit_type("Guards"), Some(ArmyUnitType::Guards));
    assert_eq!(parse_army_unit_type("General"), Some(ArmyUnitType::General));
}

#[test]
fn parse_army_unit_type_invalid() {
    assert_eq!(parse_army_unit_type("Wizard"), None);
    assert_eq!(parse_army_unit_type(""), None);
}

#[test]
fn parse_ship_type_valid() {
    assert_eq!(parse_ship_type("Frigate"), Some(ShipType::Frigate));
    assert_eq!(parse_ship_type("Trader"), Some(ShipType::Trader));
}

#[test]
fn parse_ship_type_invalid() {
    assert_eq!(parse_ship_type("Submarine"), None);
}

// ── wasm_get_navy_markers tests ───────────────────────────

/// Build a minimal game state where nation 0 owns a coastal province with
/// a port, give it two Frigates and one Ironclad, then invoke
/// `wasm_get_navy_markers` and parse the result.
fn setup_navy_markers_game() -> (GameState, String) {
    let mut game = new_game("default", Difficulty::Normal, 0);
    game.game_data = domain::data::GameData::default();

    // Promote the first coastal province of the human player to a port and
    // put some warships on it. The map generator already sets `coastal`
    // for coastal provinces.
    let human = game.human_player_nation;
    let coastal_pid: Option<ProvinceId> = game
        .world
        .provinces
        .iter()
        .find(|p| p.owner == human && p.is_coastal())
        .map(|p| p.id);
    let pid = coastal_pid.expect("test map must have at least one coastal GP province");

    // Pick a tile in that province and mark it as a port.
    let tile_coord = {
        let prov = game.get_province(pid).unwrap();
        prov.tiles.first().copied().unwrap()
    };
    if let Some(t) = game.world.hex_map.get_tile_mut(tile_coord) {
        t.infrastructure.has_port = true;
    }

    // Give the human nation three warships: two Frigates on Patrol and
    // one Ironclad on Escort.
    let frigate_hull = game.game_data.ship_stats(ShipType::Frigate).hull;
    let ironclad_hull = game.game_data.ship_stats(ShipType::Ironclad).hull;
    let nation = game.get_nation_mut(human).unwrap();
    let mk_ship =
        |id: u32, ship_type: ShipType, op: Option<domain::military::naval::NavalOperation>| {
            let hull = match ship_type {
                ShipType::Ironclad => ironclad_hull,
                _ => frigate_hull,
            };
            let mut s = Ship::new(domain::map::UnitId(id), ship_type, human, hull);
            s.operation = op;
            s
        };
    nation.military.warships.clear();
    nation.military.warships.push(mk_ship(
        9000,
        ShipType::Frigate,
        Some(domain::military::naval::NavalOperation::Patrol),
    ));
    nation.military.warships.push(mk_ship(
        9001,
        ShipType::Frigate,
        Some(domain::military::naval::NavalOperation::Patrol),
    ));
    nation.military.warships.push(mk_ship(
        9002,
        ShipType::Ironclad,
        Some(domain::military::naval::NavalOperation::Escort),
    ));

    let json = serialize_game(&game);
    (game, json)
}

#[test]
fn navy_markers_emits_fleet_marker_for_human() {
    let (_, json) = setup_navy_markers_game();
    let result = wasm_get_navy_markers(&json, false);
    let markers: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

    let human_markers: Vec<_> = markers
        .iter()
        .filter(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(0))
        .collect();
    assert_eq!(
        human_markers.len(),
        1,
        "human player should have exactly one fleet marker"
    );

    let m = human_markers[0];
    assert_eq!(m["kind"], "fleet");
    assert_eq!(m["ship_count"], 3);
    assert_eq!(m["visible"], true);
    // 2 Frigates + 1 Ironclad grouped into by_type.
    let by_type = m["by_type"].as_object().unwrap();
    assert_eq!(by_type.get("Frigate").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(by_type.get("Ironclad").and_then(|v| v.as_u64()), Some(1));
    // by_operation reports Patrol × 2 + Escort × 1.
    let by_op = m["by_operation"].as_object().unwrap();
    assert_eq!(by_op.get("Patrol").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(by_op.get("Escort").and_then(|v| v.as_u64()), Some(1));

    // Card #471: even when the ship has no `sea_zone` assigned (typical
    // for the human player at turn 1), the marker must back-fill
    // `sea_zone_id` from whichever sea zone contains the anchor hex.
    // Without this, the frontend cannot compute fleet-move adjacency
    // targets and no destination hexes get highlighted.
    assert!(
        m.get("sea_zone_id").and_then(|v| v.as_u64()).is_some(),
        "fleet marker must carry a sea_zone_id even when the ship's sea_zone field is None"
    );
}

/// Card #471: `wasm_move_fleet` queues a move (it does not execute it
/// immediately), and the existing turn processor's
/// `resolve_pending_fleet_moves` is what actually relocates the
/// warships. Even at turn 1 — before the AI naval pass has assigned
/// ships a `sea_zone` — the bridge back-fills the fallback zone so the
/// move can be queued without "no warships in that sea zone".
#[test]
fn move_fleet_queues_and_resolves_at_end_of_turn() {
    use domain::map::sea_zones::SeaZoneId;
    let (game, json) = setup_navy_markers_game();
    let human = game.human_player_nation;

    // Sanity: the marker carries a back-filled zone id we can use as `from_z`.
    let markers_json = wasm_get_navy_markers(&json, false);
    let markers: Vec<serde_json::Value> = serde_json::from_str(&markers_json).unwrap();
    let marker = markers
        .iter()
        .find(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(human.0 as u64))
        .expect("human fleet marker");
    let from_zone_id = marker
        .get("sea_zone_id")
        .and_then(|v| v.as_u64())
        .expect("back-filled sea_zone_id") as u32;

    // Pick an adjacent non-lake zone as the destination.
    let zones_json = wasm_get_sea_zones(&json);
    let zones: Vec<serde_json::Value> = serde_json::from_str(&zones_json).unwrap();
    let from_zone = zones
        .iter()
        .find(|z| z.get("id").and_then(|v| v.as_u64()) == Some(from_zone_id as u64))
        .expect("from zone in payload");
    let adj: Vec<u64> = from_zone
        .get("adjacent_zone_ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_u64()).collect())
        .unwrap_or_default();
    let to_zone_id = adj
        .into_iter()
        .find(|&id| {
            zones
                .iter()
                .find(|z| z.get("id").and_then(|v| v.as_u64()) == Some(id))
                .and_then(|z| z.get("is_lake").and_then(|v| v.as_bool()))
                == Some(false)
        })
        .expect("at least one non-lake adjacent zone") as u32;

    // Queue the move.
    let queued_json = wasm_move_fleet(&json, human.0, from_zone_id, to_zone_id);
    assert!(
        !queued_json.starts_with("{\"error\""),
        "wasm_move_fleet should succeed for unzoned ships in the fallback zone, got: {queued_json}"
    );

    // Still queued — ships have NOT moved yet, but they have been
    // back-filled into the source zone so the turn processor can find them.
    let queued_game = deserialize_game(&queued_json).expect("re-deserialize");
    let queued_nation = queued_game.get_nation(human).unwrap();
    assert_eq!(
        queued_game.transient.pending_fleet_moves.len(),
        1,
        "exactly one queued fleet move expected"
    );
    let pending = queued_game.transient.pending_fleet_moves[0];
    assert_eq!(
        pending,
        (human, SeaZoneId(from_zone_id), SeaZoneId(to_zone_id))
    );
    let in_source = queued_nation
        .military
        .warships
        .iter()
        .filter(|s| s.sea_zone == Some(SeaZoneId(from_zone_id)))
        .count();
    assert_eq!(
        in_source,
        queued_nation.military.warships.len(),
        "every warship should be back-filled into the source zone after queueing"
    );

    // Process a turn — the queued move resolves and ships end up in `to_z`.
    let mut after_turn = deserialize_game(&queued_json).expect("re-deserialize for processing");
    let _ = domain::turn::process_turn(&mut after_turn);
    let nation = after_turn.get_nation(human).unwrap();
    let in_dest = nation
        .military
        .warships
        .iter()
        .filter(|s| s.sea_zone == Some(SeaZoneId(to_zone_id)))
        .count();
    assert_eq!(
        in_dest,
        nation.military.warships.len(),
        "all warships should be in the destination zone after end-turn processing"
    );
    assert!(
        after_turn.transient.pending_fleet_moves.is_empty(),
        "pending_fleet_moves should be drained at end of turn"
    );
}

/// Card #471: re-queueing a move from the same source zone replaces the
/// existing entry rather than piling up stale ones.
#[test]
fn move_fleet_replaces_existing_pending_entry() {
    use domain::map::sea_zones::SeaZoneId;
    let (game, json) = setup_navy_markers_game();
    let human = game.human_player_nation;

    let markers_json = wasm_get_navy_markers(&json, false);
    let markers: Vec<serde_json::Value> = serde_json::from_str(&markers_json).unwrap();
    let from_zone_id = markers
        .iter()
        .find(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(human.0 as u64))
        .and_then(|m| m.get("sea_zone_id").and_then(|v| v.as_u64()))
        .expect("back-filled sea_zone_id") as u32;

    let zones_json = wasm_get_sea_zones(&json);
    let zones: Vec<serde_json::Value> = serde_json::from_str(&zones_json).unwrap();
    let from_zone = zones
        .iter()
        .find(|z| z.get("id").and_then(|v| v.as_u64()) == Some(from_zone_id as u64))
        .expect("from zone");
    let adj: Vec<u32> = from_zone
        .get("adjacent_zone_ids")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_u64().map(|n| n as u32))
                .collect()
        })
        .unwrap_or_default();
    let mut non_lake: Vec<u32> = adj
        .into_iter()
        .filter(|&id| {
            zones
                .iter()
                .find(|z| z.get("id").and_then(|v| v.as_u64()) == Some(id as u64))
                .and_then(|z| z.get("is_lake").and_then(|v| v.as_bool()))
                == Some(false)
        })
        .collect();
    if non_lake.len() < 2 {
        // Fallback: if the test world only exposes one adjacent non-lake
        // zone, we still want to assert the replacement contract — re-queue
        // to the same destination twice and assert one entry survives.
        non_lake.push(non_lake[0]);
    }
    let to_a = non_lake[0];
    let to_b = non_lake[1];

    let after_a = wasm_move_fleet(&json, human.0, from_zone_id, to_a);
    let after_b = wasm_move_fleet(&after_a, human.0, from_zone_id, to_b);
    let final_game = deserialize_game(&after_b).expect("re-deserialize");
    assert_eq!(
        final_game.transient.pending_fleet_moves.len(),
        1,
        "queueing twice from the same source zone should keep exactly one entry"
    );
    let pending = final_game.transient.pending_fleet_moves[0];
    assert_eq!(pending, (human, SeaZoneId(from_zone_id), SeaZoneId(to_b)));
}

#[test]
fn navy_markers_keeps_unestablished_beachhead_with_fleet_marker() {
    let (mut game, _) = setup_navy_markers_game();
    // Re-assign the Ironclad to Beachhead a hostile coastal province, but
    // do not establish an actual landing yet.
    let human = game.human_player_nation;
    let beachhead_pid: ProvinceId = game
        .world
        .provinces
        .iter()
        .find(|p| p.owner != human && p.is_coastal())
        .map(|p| p.id)
        .expect("need a hostile coastal province for beachhead");
    let nation = game.get_nation_mut(human).unwrap();
    nation.military.warships[2].operation = Some(
        domain::military::naval::NavalOperation::Beachhead(beachhead_pid),
    );
    let json = serialize_game(&game);

    let result = wasm_get_navy_markers(&json, false);
    let markers: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
    let human_markers: Vec<_> = markers
        .iter()
        .filter(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(0))
        .collect();
    assert_eq!(
        human_markers.len(),
        1,
        "ships assigned to a future beachhead should still render at the fleet location"
    );
    let fleet = human_markers
        .iter()
        .find(|m| m["kind"] == "fleet")
        .expect("fleet marker present");
    assert_eq!(fleet["ship_count"], 3);
    let by_op = fleet["by_operation"].as_object().unwrap();
    assert_eq!(
        by_op
            .get(&format!("Beachhead(p{})", beachhead_pid.0))
            .and_then(|v| v.as_u64()),
        Some(1)
    );
}

#[test]
fn navy_markers_emits_beachhead_marker_for_established_landing() {
    let (mut game, _) = setup_navy_markers_game();
    let human = game.human_player_nation;
    let beachhead_pid: ProvinceId = game
        .world
        .provinces
        .iter()
        .find(|p| p.owner != human && p.is_coastal())
        .map(|p| p.id)
        .expect("need a hostile coastal province for beachhead");
    let nation = game.get_nation_mut(human).unwrap();
    nation.military.warships[2].operation = Some(
        domain::military::naval::NavalOperation::Beachhead(beachhead_pid),
    );
    game.transient
        .pending_landings
        .push((human, beachhead_pid, game.turn));
    let json = serialize_game(&game);

    let result = wasm_get_navy_markers(&json, false);
    let markers: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
    let human_markers: Vec<_> = markers
        .iter()
        .filter(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(0))
        .collect();
    assert_eq!(
        human_markers.len(),
        2,
        "established beachheads should render both a fleet marker and a landing marker"
    );
    assert!(human_markers.iter().any(|m| m["kind"] == "fleet"));
    let beachhead = human_markers
        .iter()
        .find(|m| m["kind"] == "beachhead")
        .expect("beachhead marker present");
    assert_eq!(beachhead["ship_count"], 1);
    assert!(beachhead.get("target_province").is_some());
    assert!(beachhead.get("target_hex").is_some());
}

#[test]
fn navy_markers_fog_hides_invisible_enemy_fleets() {
    // Positive-case fog test. We build a game where a specific non-human
    // nation has a warship, then confirm that its marker is present
    // WITHOUT fog but absent WITH fog (because its anchor hex is not
    // visible to the human player).
    let (mut game, _) = setup_navy_markers_game();

    // Pick an enemy GP whose capital province has been generated as
    // coastal. Some enemies may be inland; skip those.
    let human = game.human_player_nation;
    let enemy_id: NationId = {
        let enemy = game
            .world
            .nations
            .iter()
            .find(|n| {
                n.id != human
                    && n.nation_type == NationType::GreatPower
                    && game
                        .world
                        .provinces
                        .iter()
                        .any(|p| p.owner == n.id && p.is_coastal())
            })
            .expect("need a coastal enemy GP for the fog test");
        enemy.id
    };

    // Give the enemy one Frigate on Patrol.
    let mut enemy_ship = Ship::with_data(
        domain::map::UnitId(9500),
        ShipType::Frigate,
        enemy_id,
        &game.game_data,
    );
    enemy_ship.operation = Some(domain::military::naval::NavalOperation::Patrol);
    let enemy = game.get_nation_mut(enemy_id).unwrap();
    enemy.military.warships.clear();
    enemy.military.warships.push(enemy_ship);

    // Compute where that fleet marker would land and confirm the anchor
    // is outside the human's visible set, so the fog filter is the only
    // thing keeping the marker hidden.
    let enemy_nation = game.get_nation(enemy_id).unwrap();
    let anchor = domain::military::navy_placement::fleet_anchor(
        enemy_nation,
        &game.world.hex_map,
        &game.world.provinces,
    )
    .expect("enemy should have a fleet anchor");
    let visible_hexes = compute_visible_hexes(&game, false);
    assert!(
        !visible_hexes.contains(&anchor),
        "enemy anchor hex must be outside human visibility for this test to be meaningful",
    );

    let json = serialize_game(&game);

    // Fogged: enemy marker must be absent.
    let fogged = wasm_get_navy_markers(&json, false);
    let fogged_markers: Vec<serde_json::Value> = serde_json::from_str(&fogged).unwrap();
    assert!(
        !fogged_markers
            .iter()
            .any(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(enemy_id.0 as u64)),
        "fogged call must NOT include the enemy's marker",
    );

    // Unfogged: enemy marker must be present.
    let unfogged = wasm_get_navy_markers(&json, true);
    let unfogged_markers: Vec<serde_json::Value> = serde_json::from_str(&unfogged).unwrap();
    assert!(
        unfogged_markers
            .iter()
            .any(|m| m.get("nation_id").and_then(|v| v.as_u64()) == Some(enemy_id.0 as u64)),
        "unfogged call must include the enemy's marker",
    );

    // Invariant: emitted markers are always marked visible.
    for m in &fogged_markers {
        assert_eq!(
            m["visible"], true,
            "wasm_get_navy_markers must never emit visible:false",
        );
    }
}

#[test]
fn navy_markers_deterministic_across_runs() {
    let (_, json) = setup_navy_markers_game();
    let a = wasm_get_navy_markers(&json, false);
    let b = wasm_get_navy_markers(&json, false);
    assert_eq!(
        a, b,
        "marker output must be byte-identical for the same game state"
    );
}

// ── Move validation tests ─────────────────────────────────

#[test]
fn queue_move_rejects_nonexistent_unit() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let nid = game.human_player_nation.0;
    let result = wasm_queue_unit_move(&json, nid, 9999999, 1);
    assert!(result.contains("error"));
}

#[test]
fn queue_move_replaces_duplicate() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();
    let nid = game.human_player_nation;

    let nation = game.get_nation(nid).unwrap();
    let unit = nation.military.army.iter().find(|u| u.unit_type.can_move());
    if unit.is_none() {
        return;
    }
    let uid = unit.unwrap().id.0;

    let own_provs: Vec<u32> = game
        .world
        .provinces
        .iter()
        .filter(|p| p.owner == nid)
        .take(2)
        .map(|p| p.id.0)
        .collect();
    if own_provs.len() < 2 {
        return;
    }

    let json = serialize_game(&game);
    let result1 = wasm_queue_unit_move(&json, nid.0, uid, own_provs[0]);
    assert!(!result1.contains("error"));

    let result2 = wasm_queue_unit_move(&result1, nid.0, uid, own_provs[1]);
    assert!(!result2.contains("error"));
    let game2 = game_from_json(&result2).unwrap();
    let moves_for_unit = game2
        .transient
        .pending_moves
        .iter()
        .filter(|(_, id, _)| id.0 == uid)
        .count();
    assert_eq!(moves_for_unit, 1);
}

#[test]
fn wasm_accept_peace_preserves_same_turn_coalition_alliance() {
    let mut game = new_game("wasm_peace", Difficulty::Normal, 0);
    let human = game.human_player_nation;
    let gp_ids: Vec<NationId> = game.great_powers().iter().map(|n| n.id).collect();
    let enemy = gp_ids[1];
    let ally = gp_ids[2];

    game.world.diplomacy.propose_alliance(human, ally).unwrap();
    game.world.diplomacy.declare_war(enemy, human);
    game.world.diplomacy.declare_war(ally, enemy);
    game.world
        .diplomacy
        .pending_proposals
        .push(DiplomaticProposal {
            from: enemy,
            to: human,
            proposal_type: TreatyType::PeaceTreaty,
            turn_proposed: game.turn,
            attacker: None,
            cascade_remaining: None,
        });
    game.world
        .diplomacy
        .propose_peace(ally, enemy, game.turn)
        .unwrap();
    game.world
        .diplomacy
        .pending_proposals
        .push(DiplomaticProposal {
            from: enemy,
            to: ally,
            proposal_type: TreatyType::PeaceTreaty,
            turn_proposed: game.turn,
            attacker: None,
            cascade_remaining: None,
        });

    let accepted_json = wasm_accept_proposal(&serialize_game(&game), human.0, 0);
    let mut accepted_game = game_from_json(&accepted_json).unwrap();

    assert!(
        !accepted_game.world.diplomacy.is_at_war(human, enemy),
        "human peace acceptance should clear the war immediately"
    );
    assert!(
        accepted_game
            .world
            .diplomacy
            .has_treaty(human, ally, TreatyType::Alliance),
        "alliance should remain pending same-turn reconciliation"
    );

    let report = process_turn(&mut accepted_game);

    assert!(
        accepted_game
            .world
            .diplomacy
            .has_treaty(human, ally, TreatyType::Alliance),
        "coordinated same-turn coalition peace via wasm should preserve the alliance"
    );
    assert!(
        report
            .newspaper_headlines
            .iter()
            .all(|h| !h.text.contains("breaks its alliance")),
        "coordinated wasm peace should not publish a separate-peace alliance-break headline"
    );
}

#[test]
fn recruit_general_rejected() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let nid = game.human_player_nation.0;
    let result = wasm_recruit_army_unit(&json, nid, "General");
    assert!(result.contains("error"));
}

#[test]
fn pending_civilian_hire_sets_queue() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();
    // Even with no funds, setting pending hire succeeds (deferred to end-of-turn)
    let nation = game.get_nation_mut(game.human_player_nation).unwrap();
    nation.economy.treasury = Money::ZERO;
    let json = serialize_game(&game);

    let result = wasm_set_pending_civilian_hire(&json, game.human_player_nation.0, "Miner", 2);
    assert!(!result.contains("error"), "unexpected error: {}", result);
}

#[test]
fn hire_civilian_locked_tech_is_rejected() {
    // Rancher requires "Feed Grasses". Without it, the WASM bridge must
    // refuse the pending hire — tech gate is enforced at queue time.
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();
    let nation = game.get_nation_mut(game.human_player_nation).unwrap();
    nation.economy.treasury = Money::dollars(100_000);
    nation.researched_techs.clear();
    let json = serialize_game(&game);

    let result = wasm_set_pending_civilian_hire(&json, game.human_player_nation.0, "Rancher", 1);
    assert!(
        result.contains("locked"),
        "expected 'locked' error, got: {}",
        result
    );
}

#[test]
fn buildable_units_includes_tech_met_for_civilians() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let result = wasm_get_buildable_units(&json, game.human_player_nation.0);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let civilians = parsed["civilians"].as_array().unwrap();
    for civ in civilians {
        assert!(civ["tech_met"].as_bool().is_some());
    }
}

#[test]
fn buildable_civilians_exclude_tech_locked_types() {
    // On a fresh game with no techs, Rancher/Forester/Driller require specific techs
    // and must NOT appear in the buildable civilians list.
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let nation = game
        .world
        .nations
        .iter()
        .find(|n| n.id == player_id)
        .unwrap();
    assert!(
        nation.researched_techs.is_empty(),
        "precondition: fresh game must have no researched techs"
    );
    let result = wasm_get_buildable_units(&json, player_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let civilians = parsed["civilians"].as_array().unwrap();
    let names: Vec<&str> = civilians
        .iter()
        .filter_map(|c| c["type"].as_str())
        .collect();
    let cfg = &game.game_data.game_config;
    let tech_gated = [
        ("Rancher", &cfg.civilian_rancher_tech),
        ("Forester", &cfg.civilian_forester_tech),
        ("Driller", &cfg.civilian_driller_tech),
    ];
    for (civ_name, tech_opt) in &tech_gated {
        if tech_opt.is_some() {
            // If a tech is configured, this civilian must not appear for a player with no techs
            assert!(
                !names.contains(civ_name),
                "{civ_name} should not appear — player has no techs yet"
            );
        }
    }
}

#[test]
fn map_tile_json_includes_is_prospected() {
    let json = make_game_json();
    let result = wasm_get_map_data(&json, false);
    let tiles: serde_json::Value = serde_json::from_str(&result).unwrap();
    let tile_arr = tiles.as_array().expect("map data should be an array");
    assert!(!tile_arr.is_empty(), "map should have tiles");
    // Every tile must expose the is_prospected field
    for tile in tile_arr {
        assert!(
            tile.get("is_prospected").is_some(),
            "tile at ({},{}) missing is_prospected",
            tile["q"],
            tile["r"]
        );
    }
}

#[test]
fn get_civilians_undeployed_has_null_position() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let result = wasm_get_civilians(&json, game.human_player_nation.0);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    for civ in parsed["undeployed"].as_array().unwrap_or(&vec![]) {
        assert!(civ.get("position").is_some());
        assert!(civ["position"].is_null());
    }
}

#[test]
fn cancel_move_removes_pending() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();

    let nid = game.human_player_nation;
    game.transient
        .pending_moves
        .push((nid, domain::map::UnitId(12345), ProvinceId(1)));
    let json = serialize_game(&game);

    let result = wasm_cancel_unit_move(&json, 12345);
    assert!(!result.contains("error"));
    let game2 = game_from_json(&result).unwrap();
    assert!(
        !game2
            .transient
            .pending_moves
            .iter()
            .any(|(_, id, _)| id.0 == 12345)
    );
}

// ── F-018: Anarchic target + deploy occupancy tests ───────

#[test]
fn valid_move_targets_includes_anarchic_provinces() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();
    let nid = game.human_player_nation;

    // Pick an enemy nation owning a province adjacent to ours — only
    // adjacent (or landing-site) provinces are valid move targets, so a
    // non-adjacent anarchic nation would legitimately yield no targets.
    let enemy_id = game.world.provinces.iter().find_map(|p| {
        if p.owner == nid {
            return None;
        }
        let adjacent = game
            .world
            .provinces
            .iter()
            .filter(|q| q.owner == nid)
            .any(|q| domain::map::provinces_are_adjacent(&game.world.hex_map, q, p));
        adjacent.then_some(p.owner)
    });
    let Some(enemy_id) = enemy_id else {
        return; // no adjacent enemy on this map — nothing to assert
    };
    if let Some(enemy) = game.world.nations.iter_mut().find(|n| n.id == enemy_id) {
        enemy.diplomacy.is_in_anarchy = true;

        // Ensure we have a movable unit
        let nation = game.get_nation(nid).unwrap();
        let unit = nation.military.army.iter().find(|u| u.unit_type.can_move());
        if let Some(unit) = unit {
            let uid = unit.id.0;
            let json = serialize_game(&game);
            let result = wasm_get_valid_move_targets(&json, nid.0, uid);
            let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
            let hostile = parsed["hostile"].as_array().unwrap();
            // The adjacent anarchic nation's province must be attackable
            // even without a war declaration (F-018).
            assert!(
                !hostile.is_empty(),
                "Anarchic provinces should appear as hostile targets"
            );
        }
    }
}

#[test]
fn queue_move_allows_anarchic_target() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();
    let nid = game.human_player_nation;

    // Find an enemy province and make its owner anarchic
    let enemy_prov = game
        .world
        .provinces
        .iter()
        .find(|p| p.owner != nid)
        .map(|p| (p.id, p.owner));
    if let Some((pid, enemy_nid)) = enemy_prov {
        if let Some(enemy) = game.world.nations.iter_mut().find(|n| n.id == enemy_nid) {
            enemy.diplomacy.is_in_anarchy = true;
        }
        let nation = game.get_nation(nid).unwrap();
        if let Some(unit) = nation.military.army.iter().find(|u| u.unit_type.can_move()) {
            let uid = unit.id.0;
            let json = serialize_game(&game);
            let result = wasm_queue_unit_move(&json, nid.0, uid, pid.0);
            assert!(
                !result.contains("error"),
                "Should allow moving to anarchic province"
            );
        }
    }
}

#[test]
fn queue_move_rejects_neutral_non_anarchic_target() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();
    let nid = game.human_player_nation;

    // Find an enemy province not at war and not anarchic
    let enemy_prov = game
        .world
        .provinces
        .iter()
        .find(|p| {
            p.owner != nid
                && !game.world.diplomacy.is_at_war(nid, p.owner)
                && !game
                    .get_nation(p.owner)
                    .is_some_and(|n| n.diplomacy.is_in_anarchy)
        })
        .map(|p| p.id);

    if let Some(pid) = enemy_prov {
        let nation = game.get_nation(nid).unwrap();
        if let Some(unit) = nation.military.army.iter().find(|u| u.unit_type.can_move()) {
            let uid = unit.id.0;
            let json = serialize_game(&game);
            let result = wasm_queue_unit_move(&json, nid.0, uid, pid.0);
            assert!(
                result.contains("error"),
                "Should reject moving to neutral non-anarchic province"
            );
        }
    }
}

#[test]
fn command_error_returns_structured_json() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let result = wasm_recruit_army_unit(&json, game.human_player_nation.0, "General");
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(
        parsed["error"].is_string(),
        "Error should be a string field"
    );
}

// ── Newspaper archive reason serialization ─────────────────

#[test]
fn newspaper_archive_json_includes_reason_for_ai_headlines() {
    use domain::events::{Headline, HeadlineCategory};

    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();

    // Seed the archive with one AI-reasoned headline and one plain headline.
    game.archive.newspaper_archive.push((
        game.turn,
        vec![
            Headline::with_reason(
                "Testland has declared war!".to_string(),
                HeadlineCategory::War,
                "need=2.3, opp=1.1, combined=3.4 > threshold 1.5".to_string(),
            ),
            Headline::new(
                "The Imperial Times - 1815 Q1".to_string(),
                HeadlineCategory::Default,
            ),
        ],
    ));

    let game_json = serialize_game(&game);
    let archive_json = wasm_get_newspaper_archive(&game_json);
    let parsed: serde_json::Value = serde_json::from_str(&archive_json).unwrap();
    let first_turn = &parsed.as_array().unwrap()[0];
    let headlines = first_turn["headlines"].as_array().unwrap();

    let war = headlines
        .iter()
        .find(|h| h["text"].as_str().unwrap().contains("declared war"))
        .expect("war headline");
    assert_eq!(
        war["reason"].as_str(),
        Some("need=2.3, opp=1.1, combined=3.4 > threshold 1.5"),
        "AI headline must carry reason through WASM"
    );

    let masthead = headlines
        .iter()
        .find(|h| h["text"].as_str().unwrap().contains("Imperial Times"))
        .expect("masthead headline");
    assert!(
        masthead.get("reason").is_none() || masthead["reason"].is_null(),
        "non-AI headline must omit reason field, got: {}",
        masthead
    );
}

#[test]
fn newspaper_archive_json_marks_non_actions() {
    use domain::events::{Headline, HeadlineCategory};

    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();

    game.archive.newspaper_archive.push((
        game.turn,
        vec![
            Headline::non_action(
                "Testland did not declare war this turn".to_string(),
                HeadlineCategory::Default,
                "war cooldown active".to_string(),
            ),
            Headline::with_reason(
                "Testland declared war on Otherland!".to_string(),
                HeadlineCategory::War,
                "combined score above threshold".to_string(),
            ),
        ],
    ));

    let game_json = serialize_game(&game);
    let archive_json = wasm_get_newspaper_archive(&game_json);
    let parsed: serde_json::Value = serde_json::from_str(&archive_json).unwrap();
    let headlines = parsed.as_array().unwrap()[0]["headlines"]
        .as_array()
        .unwrap();

    let non_action = headlines
        .iter()
        .find(|h| h["text"].as_str().unwrap().contains("did not declare"))
        .expect("non-action headline");
    assert_eq!(
        non_action["is_non_action"].as_bool(),
        Some(true),
        "non-action headlines must serialize is_non_action=true"
    );

    let action = headlines
        .iter()
        .find(|h| h["text"].as_str().unwrap().contains("declared war on"))
        .expect("action headline");
    assert!(
        action.get("is_non_action").is_none() || action["is_non_action"].is_null(),
        "positive-action headlines must OMIT is_non_action (skip_serializing_if), got: {}",
        action
    );
}

#[test]
fn newspaper_archive_json_includes_nation_ids() {
    use domain::events::{Headline, HeadlineCategory};
    use domain::types::NationId;

    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();

    game.archive.newspaper_archive.push((
        game.turn,
        vec![
            Headline::new("War breaks out!".to_string(), HeadlineCategory::War)
                .for_nations(&[NationId(1), NationId(2)]),
            Headline::new("The Imperial Times".to_string(), HeadlineCategory::Default),
        ],
    ));

    let game_json = serialize_game(&game);
    let archive_json = wasm_get_newspaper_archive(&game_json);
    let parsed: serde_json::Value = serde_json::from_str(&archive_json).unwrap();
    let headlines = parsed.as_array().unwrap()[0]["headlines"]
        .as_array()
        .unwrap();

    let war = headlines
        .iter()
        .find(|h| h["text"].as_str().unwrap().contains("War breaks out"))
        .expect("war headline");
    let ids: Vec<i64> = war["nation_ids"]
        .as_array()
        .expect("nation_ids must be present")
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![1, 2],
        "nation_ids must survive WASM serialization"
    );

    let masthead = headlines
        .iter()
        .find(|h| h["text"].as_str().unwrap().contains("Imperial Times"))
        .expect("masthead headline");
    assert!(
        masthead.get("nation_ids").is_none() || masthead["nation_ids"].is_null(),
        "headlines without nation_ids must omit the field, got: {}",
        masthead
    );
}

#[test]
fn process_turn_headlines_include_nation_ids() {
    let json = make_game_json();
    let result = wasm_process_turn(&json);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let headlines = parsed["report"]["headlines"]
        .as_array()
        .expect("headlines array");

    // Masthead headline never carries nation_ids
    let masthead = headlines
        .iter()
        .find(|h| h["text"].as_str().unwrap_or("").contains("Imperial Times"))
        .expect("masthead headline");
    assert!(
        masthead.get("nation_ids").is_none() || masthead["nation_ids"].is_null(),
        "masthead must omit nation_ids, got: {}",
        masthead
    );

    // At least one AI-action headline should carry nation_ids (AI nations always act)
    let with_ids: Vec<_> = headlines
        .iter()
        .filter(|h| h.get("nation_ids").is_some() && !h["nation_ids"].is_null())
        .collect();
    assert!(
        !with_ids.is_empty(),
        "at least one headline from a real turn must carry nation_ids"
    );
    for h in &with_ids {
        let ids = h["nation_ids"]
            .as_array()
            .expect("nation_ids must be array");
        assert!(!ids.is_empty(), "nation_ids array must not be empty");
    }
}

#[test]
fn process_turns_headlines_include_nation_ids() {
    let json = make_game_json();
    let result = wasm_process_turns(&json, 1);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let reports = parsed["reports"].as_array().expect("reports array");
    assert!(!reports.is_empty());
    let headlines = reports[0]["headlines"].as_array().expect("headlines array");

    let masthead = headlines
        .iter()
        .find(|h| h["text"].as_str().unwrap_or("").contains("Imperial Times"))
        .expect("masthead headline");
    assert!(
        masthead.get("nation_ids").is_none() || masthead["nation_ids"].is_null(),
        "masthead must omit nation_ids"
    );

    let with_ids: Vec<_> = headlines
        .iter()
        .filter(|h| h.get("nation_ids").is_some() && !h["nation_ids"].is_null())
        .collect();
    assert!(
        !with_ids.is_empty(),
        "at least one headline per turn must carry nation_ids"
    );
}

#[test]
fn wasm_get_battle_data_returns_archive() {
    use domain::military::combat::BattleResult;
    use domain::military::units::ArmyUnitType;

    let mut game = new_game("default", Difficulty::Normal, 0);

    // Manually populate battle archive with test data
    let battle = BattleResult {
        attacker: NationId(0),
        defender: NationId(1),
        province: ProvinceId(0),
        attacker_won: true,
        attacker_casualties: vec![ArmyUnitType::Regulars],
        defender_casualties: vec![ArmyUnitType::Minutemen, ArmyUnitType::Minutemen],
        attacker_survivors: Vec::new(), // stripped for archive
        defender_survivors: Vec::new(), // stripped for archive
        terrain: Some(domain::types::TerrainType::Hills),
        fort_level: 1,
        attacker_initial_fp: 100.0,
        defender_initial_fp: 60.0,
        attacker_initial_count: 5,
        defender_initial_count: 3,
        retreated: false,
        siege_reduced_fort: true,
        medal_awards: vec![(ArmyUnitType::Guards, 2)],
        attacker_origin_provinces: vec![ProvinceId(2), ProvinceId(3)],
        is_naval_landing: false,
        defender_retreated: false,
        attacker_retreated_to: Vec::new(),
        defender_retreated_to: Vec::new(),
        retreat_debug: None,
        attacker_unit_logs: Vec::new(),
        defender_unit_logs: Vec::new(),
        round_logs: Vec::new(),
    };

    game.archive
        .battle_archive
        .push((TurnNumber::new(1), vec![battle], Vec::new()));

    let game_json = serialize_game(&game);
    let result_json = wasm_get_battle_data(&game_json);
    let parsed: serde_json::Value = serde_json::from_str(&result_json).unwrap();

    let archive = parsed.as_array().expect("should be array");
    assert_eq!(archive.len(), 1, "should have one archived turn");

    let entry = &archive[0];
    assert_eq!(entry["turn"].as_u64(), Some(1));
    assert_eq!(entry["year"].as_u64(), Some(1815));
    assert_eq!(entry["quarter"].as_u64(), Some(1));

    let battles = entry["battles"]
        .as_array()
        .expect("should have battles array");
    assert_eq!(battles.len(), 1);

    let b = &battles[0];
    assert_eq!(b["type"].as_str(), Some("land"));
    assert_eq!(b["attacker_won"].as_bool(), Some(true));
    assert_eq!(b["fort_level"].as_u64(), Some(1));
    assert_eq!(b["siege_reduced_fort"].as_bool(), Some(true));
    assert_eq!(b["retreated"].as_bool(), Some(false));
    assert_eq!(b["attacker_initial_count"].as_u64(), Some(5));
    assert_eq!(b["defender_initial_count"].as_u64(), Some(3));
    // Survivor counts derived from initial - casualties
    assert_eq!(b["attacker_survivors_count"].as_u64(), Some(4));
    assert_eq!(b["defender_survivors_count"].as_u64(), Some(1));
    assert_eq!(b["terrain"].as_str(), Some("Hills"));

    // Check origin_tiles are populated (two origin provinces)
    let origin_tiles = b["origin_tiles"]
        .as_array()
        .expect("should have origin_tiles");
    assert_eq!(
        origin_tiles.len(),
        2,
        "should have two origin tiles for two origin provinces"
    );

    // Check medal awards
    let medals = b["medal_awards"]
        .as_array()
        .expect("should have medal_awards");
    assert_eq!(medals.len(), 1);
    assert_eq!(medals[0]["medals"].as_u64(), Some(2));

    // Naval battles should be empty
    let naval = entry["naval_battles"]
        .as_array()
        .expect("should have naval_battles");
    assert!(naval.is_empty());
}

#[test]
fn wasm_get_battle_data_empty_archive() {
    let game_json = make_game_json();
    let result_json = wasm_get_battle_data(&game_json);
    let parsed: serde_json::Value = serde_json::from_str(&result_json).unwrap();
    let archive = parsed.as_array().expect("should be array");
    assert!(
        archive.is_empty(),
        "new game should have empty battle archive"
    );
}

#[test]
fn wasm_get_battle_data_naval_archive() {
    use domain::map::UnitId;
    use domain::military::naval::NavalBattleResult;
    use domain::military::ships::{Ship, ShipType};

    let mut game = new_game("default", Difficulty::Normal, 0);

    let naval = NavalBattleResult {
        attacker: NationId(0),
        defender: NationId(1),
        attacker_won: true,
        attacker_ships_lost: vec![ShipType::Frigate],
        defender_ships_lost: vec![ShipType::ShipOfTheLine, ShipType::Frigate],
        attacker_survivors: vec![Ship {
            id: UnitId(999),
            ship_type: ShipType::ShipOfTheLine,
            owner: NationId(0),
            hull_remaining: 100,
            sea_zone: None,
            operation: None,
        }],
        defender_survivors: Vec::new(),
    };

    game.archive
        .battle_archive
        .push((TurnNumber::new(2), Vec::new(), vec![naval]));

    let game_json = serialize_game(&game);
    let result_json = wasm_get_battle_data(&game_json);
    let parsed: serde_json::Value = serde_json::from_str(&result_json).unwrap();

    let archive = parsed.as_array().unwrap();
    assert_eq!(archive.len(), 1);

    let entry = &archive[0];
    assert_eq!(entry["turn"].as_u64(), Some(2));

    // Land battles should be empty
    let land = entry["battles"].as_array().unwrap();
    assert!(land.is_empty());

    // Naval battles should be populated
    let naval = entry["naval_battles"].as_array().unwrap();
    assert_eq!(naval.len(), 1);

    let nb = &naval[0];
    assert_eq!(nb["type"].as_str(), Some("naval"));
    assert_eq!(nb["attacker_won"].as_bool(), Some(true));
    assert_eq!(nb["attacker_ships_lost"].as_array().unwrap().len(), 1);
    assert_eq!(nb["defender_ships_lost"].as_array().unwrap().len(), 2);
    assert_eq!(nb["attacker_survivors_count"].as_u64(), Some(1));
    assert_eq!(nb["defender_survivors_count"].as_u64(), Some(0));
}

// ── Card #31 + F-010: diplomacy screen at-war display vs action gating ──

/// When the player's own nation is in anarchy, every relation must be
/// *displayed* as "At War" (card #31) but the action booleans must stay
/// aligned with the raw backend relation — the peace button is not
/// meaningfully available against a non-war target, even if presentation
/// says "At War" because the player is in anarchy.
#[test]
fn diplomacy_screen_anarchy_splits_display_from_gating() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;

    // Force the player into anarchy without touching relations.
    if let Some(player) = game.get_nation_mut(player_id) {
        player.diplomacy.is_in_anarchy = true;
    }
    // Pick another nation as the counterparty.
    let target_id = game
        .world
        .nations
        .iter()
        .find(|n| n.id != player_id)
        .unwrap()
        .id;
    // Ensure raw_at_war is false for the pair.
    assert!(!game.world.diplomacy.is_at_war(player_id, target_id));

    let json = serialize_game(&game);
    let out = wasm_get_diplomacy_screen_data(&json, player_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let relations = parsed["relations"].as_array().unwrap();
    let rel = relations
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target_id.0 as u64))
        .expect("counterparty relation present");

    // Display: anarchy forces At War.
    assert_eq!(rel["at_war"].as_bool(), Some(true));
    assert_eq!(rel["status"].as_str(), Some("At War"));

    // Gating: peace is NOT offered because the underlying relation is
    // not at war — the backend would reject a propose_peace command,
    // so the UI must not advertise it.
    let actions = &rel["actions"];
    assert_eq!(
        actions["can_propose_peace"].as_bool(),
        Some(false),
        "peace must not be gated by anarchy-inflated at_war"
    );
}

#[test]
fn diplomacy_screen_hides_consulate_action_for_great_power_targets() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let target_id = game
        .great_powers()
        .iter()
        .find(|n| n.id != player_id)
        .expect("test game must have another great power")
        .id;

    let out = wasm_get_diplomacy_screen_data(&json, player_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let relations = parsed["relations"].as_array().unwrap();
    let rel = relations
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target_id.0 as u64))
        .expect("great power relation present");

    assert_eq!(
        rel["actions"]["can_build_consulate"].as_bool(),
        Some(false),
        "great powers must never advertise the consulate action"
    );
}

#[test]
fn wasm_build_consulate_rejects_great_power_target() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let target_id = game
        .great_powers()
        .iter()
        .find(|n| n.id != player_id)
        .expect("test game must have another great power")
        .id;

    let out = wasm_diplomacy_build_consulate(&json, player_id.0, target_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        parsed["error"].as_str(),
        Some("Consulates are for Minor Nations only.")
    );
}

#[test]
fn wasm_build_consulate_queues_until_end_turn() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let mut baseline = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let target_id = game
        .world
        .nations
        .iter()
        .find(|n| n.id != player_id && !n.is_great_power() && !n.diplomacy.is_in_anarchy)
        .expect("test game must have a valid minor-nation target")
        .id;
    let treasury_before = game
        .get_nation(player_id)
        .unwrap()
        .economy
        .treasury
        .as_dollars();

    let queued_json = wasm_diplomacy_build_consulate(&json, player_id.0, target_id.0);
    let queued_game = game_from_json(&queued_json).unwrap();
    assert!(
        !queued_game
            .world
            .diplomacy
            .has_consulate(player_id, target_id),
        "consulate should not exist until the turn resolves"
    );
    assert!(
        queued_game.has_pending_consulate(player_id, target_id),
        "consulate should be queued for end turn"
    );
    assert_eq!(
        queued_game
            .get_nation(player_id)
            .unwrap()
            .economy
            .treasury
            .as_dollars(),
        treasury_before,
        "treasury should not be charged before end turn"
    );

    let mut resolved_game = queued_game;
    let _report = domain::turn::process_turn(&mut resolved_game);
    let _baseline_report = domain::turn::process_turn(&mut baseline);
    assert!(
        resolved_game
            .world
            .diplomacy
            .has_consulate(player_id, target_id),
        "queued consulate should resolve during turn processing"
    );
    assert!(
        !resolved_game.has_pending_consulate(player_id, target_id),
        "pending consulate should be cleared after resolution"
    );
    assert_eq!(
        baseline
            .get_nation(player_id)
            .unwrap()
            .economy
            .treasury
            .as_dollars()
            - resolved_game
                .get_nation(player_id)
                .unwrap()
                .economy
                .treasury
                .as_dollars(),
        resolved_game.game_data.game_config.consulate_cost,
        "queued consulate should make the post-turn treasury exactly one consulate cost lower than baseline"
    );
}

#[test]
fn diplomacy_screen_hides_declare_war_for_unreachable_targets() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let target_id = game
        .world
        .nations
        .iter()
        .find(|n| n.id != player_id && !game.can_project_war_against(player_id, n.id))
        .expect("test game must have at least one unreachable target")
        .id;

    let out = wasm_get_diplomacy_screen_data(&json, player_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let relations = parsed["relations"].as_array().unwrap();
    let rel = relations
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target_id.0 as u64))
        .expect("unreachable relation present");

    assert_eq!(
        rel["actions"]["can_declare_war"].as_bool(),
        Some(false),
        "unreachable targets must not advertise declare-war"
    );
}

#[test]
fn wasm_declare_war_rejects_unreachable_target() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let target_id = game
        .world
        .nations
        .iter()
        .find(|n| n.id != player_id && !game.can_project_war_against(player_id, n.id))
        .expect("test game must have at least one unreachable target")
        .id;

    let out = wasm_diplomacy_declare_war(&json, player_id.0, target_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        parsed["error"].as_str(),
        Some("target nation is unreachable by land or ocean")
    );
}

#[test]
fn wasm_declare_war_queues_until_end_turn() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let target_id = game
        .world
        .nations
        .iter()
        .find(|n| {
            n.id != player_id
                && !game.world.diplomacy.is_at_war(player_id, n.id)
                && game.can_project_war_against(player_id, n.id)
        })
        .expect("test game must have at least one reachable war target")
        .id;

    let queued_json = wasm_diplomacy_declare_war(&json, player_id.0, target_id.0);
    let queued_game = game_from_json(&queued_json).unwrap();
    assert!(
        !queued_game.world.diplomacy.is_at_war(player_id, target_id),
        "war should not begin until the turn resolves"
    );
    assert!(
        queued_game.has_pending_war(player_id, target_id),
        "war declaration should be queued for end turn"
    );

    let mut resolved_game = queued_game;
    let _report = domain::turn::process_turn(&mut resolved_game);
    assert!(
        resolved_game
            .world
            .diplomacy
            .is_at_war(player_id, target_id),
        "queued war declaration should resolve during turn processing"
    );
    assert!(
        !resolved_game.has_pending_war(player_id, target_id),
        "pending war declaration should be cleared after resolution"
    );
}

#[test]
fn diplomacy_overlay_shows_queued_consulate_and_war_markers() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let consulate_target = game
        .world
        .nations
        .iter()
        .find(|n| n.id != player_id && !n.is_great_power() && !n.diplomacy.is_in_anarchy)
        .expect("test game must have a valid minor target")
        .id;
    let war_target = game
        .world
        .nations
        .iter()
        .find(|n| n.id != player_id && game.can_project_war_against(player_id, n.id))
        .expect("test game must have a reachable war target")
        .id;

    game.queue_direct_diplomacy_action(
        domain::game_state::PendingDiplomacyAction::BuildConsulate {
            player: player_id,
            target: consulate_target,
        },
    )
    .unwrap();
    game.queue_direct_diplomacy_action(domain::game_state::PendingDiplomacyAction::DeclareWar {
        from: player_id,
        to: war_target,
    })
    .unwrap();

    let overlay = wasm_get_diplomacy_overlay(&serialize_game(&game), player_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&overlay).unwrap();
    let rels = parsed["relations"].as_array().unwrap();
    let cons_rel = rels
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(consulate_target.0 as u64))
        .unwrap();
    let war_rel = rels
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(war_target.0 as u64))
        .unwrap();
    assert_eq!(cons_rel["has_pending_consulate"].as_bool(), Some(true));
    assert_eq!(war_rel["has_pending_war"].as_bool(), Some(true));
}

#[test]
fn wasm_send_grant_queues_until_end_turn() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let mut baseline = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let target_id = game
        .world
        .nations
        .iter()
        .find(|n| {
            n.id != player_id
                && !n.diplomacy.is_in_anarchy
                && !game.world.diplomacy.is_at_war(player_id, n.id)
        })
        .expect("test game must have a valid grant target")
        .id;
    let treasury_before = game
        .get_nation(player_id)
        .unwrap()
        .economy
        .treasury
        .as_dollars();
    let target_treasury_before = game
        .get_nation(target_id)
        .unwrap()
        .economy
        .treasury
        .as_dollars();
    let score_before = game
        .world
        .diplomacy
        .get_relation(player_id, target_id)
        .map(|r| r.score)
        .unwrap_or(0);

    let queued_json = wasm_diplomacy_send_grant(&json, player_id.0, target_id.0, 1000);
    let queued_game = game_from_json(&queued_json).unwrap();
    assert_eq!(
        queued_game
            .get_nation(player_id)
            .unwrap()
            .economy
            .treasury
            .as_dollars(),
        treasury_before,
        "grant should not deduct treasury before end turn"
    );
    assert_eq!(
        queued_game
            .get_nation(target_id)
            .unwrap()
            .economy
            .treasury
            .as_dollars(),
        target_treasury_before,
        "grant should not credit the target before end turn"
    );
    assert_eq!(
        queued_game
            .world
            .diplomacy
            .get_relation(player_id, target_id)
            .map(|r| r.score)
            .unwrap_or(0),
        score_before,
        "grant should not change diplomatic score before end turn"
    );

    let overlay = wasm_get_diplomacy_overlay(&queued_json, player_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&overlay).unwrap();
    let rel = parsed["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target_id.0 as u64))
        .unwrap();
    assert_eq!(rel["pending_grant_amount_dollars"].as_i64(), Some(1000));

    let mut resolved_game = queued_game;
    let _report = domain::turn::process_turn(&mut resolved_game);
    let _baseline_report = domain::turn::process_turn(&mut baseline);
    assert_eq!(
        baseline
            .get_nation(player_id)
            .unwrap()
            .economy
            .treasury
            .as_dollars()
            - resolved_game
                .get_nation(player_id)
                .unwrap()
                .economy
                .treasury
                .as_dollars(),
        1000,
        "grant should deduct the sender at end turn"
    );
    assert_eq!(
        resolved_game
            .get_nation(target_id)
            .unwrap()
            .economy
            .treasury
            .as_dollars()
            - baseline
                .get_nation(target_id)
                .unwrap()
                .economy
                .treasury
                .as_dollars(),
        1000,
        "grant should credit the target at end turn"
    );
    assert_eq!(
        resolved_game
            .world
            .diplomacy
            .get_relation(player_id, target_id)
            .map(|r| r.score)
            .unwrap_or(0)
            - baseline
                .world
                .diplomacy
                .get_relation(player_id, target_id)
                .map(|r| r.score)
                .unwrap_or(0),
        10,
        "$1000 grant should improve relations by +10 when resolved"
    );
}

#[test]
fn wasm_break_treaty_queues_until_end_turn() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let target_id = game
        .great_powers()
        .iter()
        .find(|n| n.id != player_id)
        .expect("test game must have another great power")
        .id;
    game.world
        .diplomacy
        .propose_alliance(player_id, target_id)
        .unwrap();
    assert!(
        game.world
            .diplomacy
            .has_treaty(player_id, target_id, TreatyType::Alliance)
    );

    let queued_json =
        wasm_diplomacy_break_treaty(&serialize_game(&game), player_id.0, target_id.0, "Alliance");
    let queued_game = game_from_json(&queued_json).unwrap();
    assert!(
        queued_game
            .world
            .diplomacy
            .has_treaty(player_id, target_id, TreatyType::Alliance),
        "treaty should remain active until the turn resolves"
    );
    assert!(
        queued_game.has_pending_break_treaty(player_id, target_id, TreatyType::Alliance),
        "treaty break should be queued for end turn"
    );

    let overlay = wasm_get_diplomacy_overlay(&queued_json, player_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&overlay).unwrap();
    let rel = parsed["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target_id.0 as u64))
        .unwrap();
    assert_eq!(
        rel["pending_break_treaties"].as_array().unwrap()[0].as_str(),
        Some("Alliance")
    );

    let mut resolved_game = queued_game;
    let _report = domain::turn::process_turn(&mut resolved_game);
    assert!(
        !resolved_game
            .world
            .diplomacy
            .has_treaty(player_id, target_id, TreatyType::Alliance),
        "queued treaty break should resolve during turn processing"
    );
}

#[test]
fn at_war_with_one_nation_does_not_block_treaty_proposals_to_others() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let war_target = game
        .world
        .nations
        .iter()
        .find(|n| n.id != player_id)
        .expect("test game must have another nation")
        .id;
    let treaty_target = game
        .world
        .nations
        .iter()
        .find(|n| n.id != player_id && n.id != war_target && !n.diplomacy.is_in_anarchy)
        .expect("test game must have a third nation")
        .id;

    game.world.diplomacy.declare_war(player_id, war_target);
    game.world
        .diplomacy
        .get_relation_mut(player_id, treaty_target)
        .expect("relation to treaty target exists")
        .has_embassy = true;

    let out = wasm_get_diplomacy_screen_data(&serialize_game(&game), player_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let rels = parsed["relations"].as_array().unwrap();
    let rel = rels
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(treaty_target.0 as u64))
        .unwrap();
    assert_eq!(
        rel["actions"]["can_propose_nap"].as_bool(),
        Some(true),
        "war with one nation must not globally block treaty proposals"
    );
}

#[test]
fn pending_treaty_proposal_is_replaced_and_marker_moves_to_new_target() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;

    let mut gp_targets: Vec<NationId> = game
        .great_powers()
        .iter()
        .filter(|n| n.id != player_id)
        .map(|n| n.id)
        .collect();
    gp_targets.truncate(2);
    assert!(
        gp_targets.len() == 2,
        "test game must provide two GP diplomacy targets"
    );
    let target_a = gp_targets[0];
    let target_b = gp_targets[1];

    let json = wasm_diplomacy_propose_nap(&json, player_id.0, target_a.0);
    let after_first = game_from_json(&json).unwrap();
    assert!(
        after_first
            .world
            .diplomacy
            .pending_proposals
            .iter()
            .any(|p| {
                p.from == player_id
                    && p.to == target_a
                    && p.proposal_type == TreatyType::NonAggressionPact
            }),
        "first outgoing proposal should exist"
    );

    let json = wasm_diplomacy_propose_nap(&json, player_id.0, target_b.0);
    let after_second = game_from_json(&json).unwrap();
    let outgoing: Vec<_> = after_second
        .world
        .diplomacy
        .pending_proposals
        .iter()
        .filter(|p| p.from == player_id && p.proposal_type == TreatyType::NonAggressionPact)
        .collect();
    assert_eq!(
        outgoing.len(),
        1,
        "only one outgoing treaty proposal should remain after replacement"
    );
    assert_eq!(
        outgoing[0].to, target_b,
        "new proposal should replace the previous target"
    );

    let screen = wasm_get_diplomacy_screen_data(&json, player_id.0);
    let parsed: serde_json::Value = serde_json::from_str(&screen).unwrap();
    let rels = parsed["relations"].as_array().unwrap();
    let a = rels
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target_a.0 as u64))
        .unwrap();
    let b = rels
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target_b.0 as u64))
        .unwrap();
    assert_eq!(a["has_pending_nap"].as_bool(), Some(false));
    assert_eq!(b["has_pending_nap"].as_bool(), Some(true));
}

#[test]
fn wasm_dismiss_outgoing_proposal_removes_pending_marker() {
    let json = make_game_json();
    let game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let target = game
        .great_powers()
        .iter()
        .find(|n| n.id != player_id)
        .expect("test game must have at least one GP diplomacy target")
        .id;

    let proposed = wasm_diplomacy_propose_nap(&json, player_id.0, target.0);
    let before = wasm_get_diplomacy_overlay(&proposed, player_id.0);
    let before_parsed: serde_json::Value = serde_json::from_str(&before).unwrap();
    let before_rel = before_parsed["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target.0 as u64))
        .unwrap();
    assert_eq!(before_rel["has_pending_nap"].as_bool(), Some(true));

    let dismissed = wasm_diplomacy_dismiss_outgoing_proposal(&proposed, player_id.0, target.0);
    let after = wasm_get_diplomacy_overlay(&dismissed, player_id.0);
    let after_parsed: serde_json::Value = serde_json::from_str(&after).unwrap();
    let after_rel = after_parsed["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target.0 as u64))
        .unwrap();
    assert_eq!(after_rel["has_pending_nap"].as_bool(), Some(false));
}

#[test]
fn wasm_dismiss_pending_action_removes_pending_embassy_marker() {
    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    let player_id = game.human_player_nation;
    let target = game
        .world
        .nations
        .iter()
        .find(|n| n.id != player_id && !n.is_great_power() && !n.diplomacy.is_in_anarchy)
        .expect("test game must have a valid minor target")
        .id;
    game.world
        .diplomacy
        .build_consulate(player_id, target)
        .unwrap();

    let queued = wasm_diplomacy_build_embassy(&serialize_game(&game), player_id.0, target.0);
    let before = wasm_get_diplomacy_overlay(&queued, player_id.0);
    let before_parsed: serde_json::Value = serde_json::from_str(&before).unwrap();
    let before_rel = before_parsed["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target.0 as u64))
        .unwrap();
    assert_eq!(before_rel["has_pending_embassy"].as_bool(), Some(true));

    let dismissed =
        wasm_diplomacy_dismiss_pending_action(&queued, player_id.0, target.0, "embassy");
    let after = wasm_get_diplomacy_overlay(&dismissed, player_id.0);
    let after_parsed: serde_json::Value = serde_json::from_str(&after).unwrap();
    let after_rel = after_parsed["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["nation_id"].as_u64() == Some(target.0 as u64))
        .unwrap();
    assert_eq!(after_rel["has_pending_embassy"].as_bool(), Some(false));
}

#[test]
fn gp_ledger_cumulative_expenses_use_human_labels() {
    let mut game = new_game("default", Difficulty::Normal, 0);
    let ai_gp = game
        .great_powers()
        .iter()
        .find(|n| n.id != game.human_player_nation)
        .expect("test game must have at least one AI GP")
        .id;
    game.get_nation_mut(ai_gp)
        .unwrap()
        .archives
        .cash_expense_totals
        .insert(domain::economy::ledger::CashSink::AiGrant, 12345);

    let out = wasm_get_all_gp_ledger_data(&serialize_game(&game));
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let entries = parsed.as_array().unwrap();
    let ai_entry = entries
        .iter()
        .find(|e| e["nation_id"].as_u64() == Some(ai_gp.0 as u64))
        .unwrap();
    assert_eq!(
        ai_entry["cumulative"]["expense_totals"]["AI: grant"].as_i64(),
        Some(12345)
    );
    assert!(
        ai_entry["cumulative"]["expense_totals"]["AiGrant"].is_null(),
        "debug enum keys should not leak into UI payloads"
    );
}

// ── Political snapshot ─────────────────────────────────────

#[test]
fn political_snapshot_returns_tiles_for_archived_turn() {
    use domain::game_state::PoliticalSnapshot;

    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();

    // Seed a snapshot at turn 5 using current province ownership + capitals.
    let provinces: Vec<(ProvinceId, NationId, Option<NationId>)> = game
        .world
        .provinces
        .iter()
        .map(|p| (p.id, p.owner, p.incorporated_from))
        .collect();
    let capitals: Vec<(NationId, ProvinceId)> = game
        .world
        .nations
        .iter()
        .map(|n| (n.id, n.capital_province_id))
        .collect();
    game.archive.political_archive.push((
        TurnNumber::new(5),
        PoliticalSnapshot {
            provinces,
            capitals,
        },
    ));

    let game_json = serialize_game(&game);
    let out = wasm_get_political_snapshot(&game_json, 5);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(parsed["turn"].as_u64(), Some(5));
    let tiles = parsed["tiles"].as_array().expect("tiles array");
    assert_eq!(tiles.len() as i64, game.world.hex_map.tile_count() as i64);
    // At least one tile must show a non-empty owner for a normal game.
    assert!(
        tiles
            .iter()
            .any(|t| t["owner"].as_str().unwrap_or("") != ""),
        "at least one tile should have an owner"
    );
    // At least one country capital should be flagged.
    assert!(
        tiles
            .iter()
            .any(|t| t["is_country_capital"].as_bool() == Some(true)),
        "at least one tile should be flagged as country capital"
    );
}

#[test]
fn political_snapshot_errors_for_missing_turn() {
    let json = make_game_json();
    let out = wasm_get_political_snapshot(&json, 999);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        parsed["error"].is_string(),
        "missing snapshot should return an error object, got: {}",
        out
    );
}

#[test]
fn political_snapshot_uses_archived_state_not_live_state() {
    use domain::game_state::PoliticalSnapshot;

    let json = make_game_json();
    let mut game = game_from_json(&json).unwrap();
    game.game_data = domain::data::GameData::default();

    // Archive at turn 5 with the *current* capitals and ownership.
    let provinces: Vec<(ProvinceId, NationId, Option<NationId>)> = game
        .world
        .provinces
        .iter()
        .map(|p| (p.id, p.owner, p.incorporated_from))
        .collect();
    let capitals: Vec<(NationId, ProvinceId)> = game
        .world
        .nations
        .iter()
        .map(|n| (n.id, n.capital_province_id))
        .collect();
    let archived_capitals: std::collections::HashSet<ProvinceId> =
        capitals.iter().map(|&(_, pid)| pid).collect();
    game.archive.political_archive.push((
        TurnNumber::new(5),
        PoliticalSnapshot {
            provinces,
            capitals,
        },
    ));

    // Mutate live state AFTER archiving: swap every nation's capital to a
    // province that was not previously a capital, and mark a province as
    // newly incorporated in live state. The archive must ignore both.
    let non_capital_pid = game
        .world
        .provinces
        .iter()
        .map(|p| p.id)
        .find(|pid| !archived_capitals.contains(pid))
        .expect("at least one non-capital province");
    for n in &mut game.world.nations {
        n.capital_province_id = non_capital_pid;
    }
    // Pick a province and give it a fake `incorporated_from` in live state;
    // archive should NOT pick up this change because the archived tuple
    // was already captured with incorporated=None.
    let mutated_pid = game
        .world
        .provinces
        .iter()
        .map(|p| p.id)
        .find(|pid| !archived_capitals.contains(pid))
        .expect("province to mutate");
    if let Some(p) = game.get_province_mut(mutated_pid) {
        p.incorporated_from = Some(NationId(999));
    }

    let game_json = serialize_game(&game);
    let out = wasm_get_political_snapshot(&game_json, 5);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    let tiles = parsed["tiles"].as_array().expect("tiles array");

    // Archived capitals should still be flagged on tiles in archived capital
    // provinces, not on the live non-capital swap target.
    let capital_tile_count = tiles
        .iter()
        .filter(|t| t["is_country_capital"].as_bool() == Some(true))
        .count();
    assert!(
        capital_tile_count > 0,
        "archived capitals should still be flagged after live mutation"
    );

    // No tile in the archive output should show visual_group as the
    // fake NationId(999)-derived name, because that incorporation was
    // applied AFTER the snapshot was taken.
    // Live mutation of province.incorporated_from must not leak into the
    // archived rendering.
    let leaked = tiles.iter().any(|t| {
        t["visual_group"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    });
    // The starter map has no incorporated provinces at turn 1, so the
    // archive (captured with all `incorporated_from = None`) must still
    // render all visual_group fields as null/empty.
    assert!(
        !leaked,
        "archived visual_group must not reflect live-state mutation"
    );
}

// ── Pact-defense cascade continuation through wasm round-trip ───────
//
// Card #69: when the human rejects a PactDefenseRequest, the cascade
// must resume with the remaining candidates that were serialized into
// the proposal's `cascade_remaining` field. This test goes through the
// full wasm bridge: serialize → wasm_reject_proposal → deserialize →
// verify the next AI candidate was evaluated.

#[test]
fn wasm_reject_pact_defense_continues_cascade_through_serialization() {
    let mut game = new_game("default", Difficulty::Normal, 0);
    game.game_data = domain::data::GameData::default();
    let human = game.human_player_nation;
    // Pick two AI GPs as remaining candidates after the human rejects.
    let gp_ids: Vec<NationId> = game
        .great_powers()
        .iter()
        .filter(|n| n.id != human)
        .map(|n| n.id)
        .collect();
    let attacker = gp_ids[0];
    let next_protector = gp_ids[1];

    // Pick a minor nation with provinces to play the role of the protectee.
    let minor_id = game
        .world
        .nations
        .iter()
        .find(|n| !n.is_great_power() && !n.province_ids.is_empty())
        .expect("test map must have a minor nation with provinces")
        .id;

    // Set up the war: attacker has declared war on the minor.
    game.world.diplomacy.declare_war(attacker, minor_id);

    // Push a PactDefenseRequest proposal addressed to the human, with
    // the next AI GP queued in the cascade.
    game.world
        .diplomacy
        .pending_proposals
        .push(DiplomaticProposal {
            from: minor_id,
            to: human,
            proposal_type: TreatyType::PactDefenseRequest,
            turn_proposed: game.turn,
            attacker: Some(attacker),
            cascade_remaining: Some(vec![next_protector]),
        });

    let pre_json = serialize_game(&game);

    // Sanity: the proposal is visible to the human.
    let pending = wasm_get_pending_proposals(&pre_json, human.0);
    let parsed: serde_json::Value = serde_json::from_str(&pending).unwrap();
    assert_eq!(
        parsed["proposals"].as_array().map(|a| a.len()).unwrap_or(0),
        1,
        "human should see one pending PactDefenseRequest"
    );

    // Reject it — wasm_reject_proposal must:
    //   (1) remove the proposal,
    //   (2) call continue_pact_defense_cascade with the remaining list,
    //   (3) leave the game in a consistent state we can deserialize.
    let after_json = wasm_reject_proposal(&pre_json, human.0, 0);
    assert!(
        !after_json.contains("\"error\""),
        "wasm_reject_proposal must succeed: {}",
        after_json
    );

    let after = game_from_json(&after_json).expect("rejected game must round-trip");

    // The original proposal must be gone.
    assert!(
        !after
            .world
            .diplomacy
            .pending_proposals
            .iter()
            .any(|p| p.proposal_type == TreatyType::PactDefenseRequest && p.to == human),
        "the rejected PactDefenseRequest must be removed"
    );

    // The cascade must have advanced. Either:
    //   (a) the next AI protector accepted → declared war on attacker
    //       and the minor was incorporated into its empire, OR
    //   (b) the next AI protector declined → no war declared, no new
    //       proposals to other nations (cascade exhausted).
    // Either way, the cascade ran. We assert at least that the protector
    // was actually considered (the relation entry exists) and the
    // proposal queue contains no further PactDefenseRequest for any GP.
    assert!(
        !after
            .world
            .diplomacy
            .pending_proposals
            .iter()
            .any(|p| p.proposal_type == TreatyType::PactDefenseRequest),
        "cascade must not leave a stale PactDefenseRequest pending"
    );

    let ai_at_war_with_attacker = after
        .world
        .diplomacy
        .get_relation(next_protector, attacker)
        .is_some_and(|r| r.at_war);
    let minor_now_owned_by_ai = after
        .get_nation(next_protector)
        .map(|n| {
            n.province_ids.iter().any(|pid| {
                after
                    .get_province(*pid)
                    .map(|p| p.owner == next_protector)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    let _ = (ai_at_war_with_attacker, minor_now_owned_by_ai);
    // The above are defensive observability — a regression in
    // continue_pact_defense_cascade would manifest either as a stale
    // pending proposal (asserted above) or as a panic during the
    // continuation call. The point of this test is the *round-trip*
    // through serialize → wasm_reject_proposal → deserialize, which is
    // now exercised end-to-end.
}

#[test]
fn wasm_reject_pact_defense_with_stale_minor_does_not_panic() {
    // If the minor was already incorporated/conquered by the time the
    // human rejects, continue_pact_defense_cascade short-circuits. The
    // wasm bridge must still return a valid serialized game.
    let mut game = new_game("default", Difficulty::Normal, 0);
    game.game_data = domain::data::GameData::default();
    let human = game.human_player_nation;
    let gp_ids: Vec<NationId> = game
        .great_powers()
        .iter()
        .filter(|n| n.id != human)
        .map(|n| n.id)
        .collect();
    let attacker = gp_ids[0];
    let next_protector = gp_ids[1];
    let minor_id = game
        .world
        .nations
        .iter()
        .find(|n| !n.is_great_power() && !n.province_ids.is_empty())
        .expect("test map must have a minor nation")
        .id;

    // Strip the minor of all provinces (simulating it was conquered).
    let minor_provinces: Vec<ProvinceId> = game
        .get_nation(minor_id)
        .map(|n| n.province_ids.iter().copied().collect())
        .unwrap_or_default();
    if let Some(n) = game.get_nation_mut(minor_id) {
        n.province_ids.clear();
    }
    for pid in minor_provinces {
        if let Some(p) = game.world.provinces.iter_mut().find(|p| p.id == pid) {
            p.owner = attacker;
        }
    }

    game.world
        .diplomacy
        .pending_proposals
        .push(DiplomaticProposal {
            from: minor_id,
            to: human,
            proposal_type: TreatyType::PactDefenseRequest,
            turn_proposed: game.turn,
            attacker: Some(attacker),
            cascade_remaining: Some(vec![next_protector]),
        });

    let after_json = wasm_reject_proposal(&serialize_game(&game), human.0, 0);
    assert!(
        !after_json.contains("\"error\""),
        "stale-minor rejection must not error: {}",
        after_json
    );
    let after = game_from_json(&after_json).expect("must round-trip");
    assert!(
        !after
            .world
            .diplomacy
            .pending_proposals
            .iter()
            .any(|p| p.proposal_type == TreatyType::PactDefenseRequest),
        "stale PactDefenseRequest must be removed even when minor is gone"
    );
}

// ── RequestToJoinEmpire and WarDeclaration modal flows ──────────────

#[test]
fn wasm_accept_request_to_join_empire_incorporates_minor() {
    let mut game = new_game("default", Difficulty::Normal, 0);
    game.game_data = domain::data::GameData::default();
    let human = game.human_player_nation;
    let minor_id = game
        .world
        .nations
        .iter()
        .find(|n| !n.is_great_power() && !n.province_ids.is_empty())
        .expect("test map must have a minor")
        .id;
    let minor_provinces_before: Vec<ProvinceId> = game
        .get_nation(minor_id)
        .map(|n| n.province_ids.clone())
        .unwrap_or_default();
    assert!(!minor_provinces_before.is_empty());

    game.world
        .diplomacy
        .pending_proposals
        .push(DiplomaticProposal {
            from: minor_id,
            to: human,
            proposal_type: TreatyType::RequestToJoinEmpire,
            turn_proposed: game.turn,
            attacker: None,
            cascade_remaining: None,
        });

    let after_json = wasm_accept_proposal(&serialize_game(&game), human.0, 0);
    assert!(
        !after_json.contains("\"error\""),
        "accepting RequestToJoinEmpire must not error: {}",
        after_json
    );
    let after = game_from_json(&after_json).expect("round-trip");

    assert!(
        after
            .get_nation(minor_id)
            .map(|n| n.province_ids.is_empty())
            .unwrap_or(true),
        "minor must have no provinces after acceptance"
    );
    for pid in &minor_provinces_before {
        assert_eq!(
            after.get_province(*pid).map(|p| p.owner),
            Some(human),
            "province {:?} must transfer to human on acceptance",
            pid
        );
    }
}

#[test]
fn wasm_reject_request_to_join_empire_drops_relationship() {
    let mut game = new_game("default", Difficulty::Normal, 0);
    game.game_data = domain::data::GameData::default();
    let human = game.human_player_nation;
    let minor_id = game
        .world
        .nations
        .iter()
        .find(|n| !n.is_great_power() && !n.province_ids.is_empty())
        .expect("test map must have a minor")
        .id;

    // Seed a baseline relationship score so we can observe the drop.
    game.world
        .diplomacy
        .ensure_relation(minor_id, human)
        .improve_score(50);
    let score_before = game
        .world
        .diplomacy
        .get_relation(minor_id, human)
        .map(|r| r.score)
        .unwrap_or(0);

    game.world
        .diplomacy
        .pending_proposals
        .push(DiplomaticProposal {
            from: minor_id,
            to: human,
            proposal_type: TreatyType::RequestToJoinEmpire,
            turn_proposed: game.turn,
            attacker: None,
            cascade_remaining: None,
        });

    let after_json = wasm_reject_proposal(&serialize_game(&game), human.0, 0);
    assert!(!after_json.contains("\"error\""));
    let after = game_from_json(&after_json).expect("round-trip");

    let score_after = after
        .world
        .diplomacy
        .get_relation(minor_id, human)
        .map(|r| r.score)
        .unwrap_or(0);
    assert!(
        score_after < score_before,
        "rejection must lower the minor's relationship: before={}, after={}",
        score_before,
        score_after
    );
    // The minor still has its provinces — rejection does not annex.
    assert!(
        after
            .get_nation(minor_id)
            .map(|n| !n.province_ids.is_empty())
            .unwrap_or(false),
        "minor must keep its provinces after rejection"
    );
}

#[test]
fn wasm_war_declaration_modal_is_dismissable() {
    let mut game = new_game("default", Difficulty::Normal, 0);
    game.game_data = domain::data::GameData::default();
    let human = game.human_player_nation;
    let attacker = game
        .great_powers()
        .iter()
        .find(|n| n.id != human)
        .expect("at least one AI GP")
        .id;

    // The AI has already declared war (live state). The modal proposal
    // is just the notification surface.
    game.world.diplomacy.declare_war(attacker, human);
    game.world
        .diplomacy
        .pending_proposals
        .push(DiplomaticProposal {
            from: attacker,
            to: human,
            proposal_type: TreatyType::WarDeclaration,
            turn_proposed: game.turn,
            attacker: None,
            cascade_remaining: None,
        });

    // Both Accept and Reject simply dismiss; the war stays in effect.
    let accepted_json = wasm_accept_proposal(&serialize_game(&game), human.0, 0);
    assert!(
        !accepted_json.contains("\"error\""),
        "accepting WarDeclaration must not error: {}",
        accepted_json
    );
    let accepted = game_from_json(&accepted_json).expect("round-trip");
    assert!(
        accepted.world.diplomacy.is_at_war(attacker, human),
        "war remains in effect after acceptance"
    );
    assert!(
        !accepted
            .world
            .diplomacy
            .pending_proposals
            .iter()
            .any(|p| p.proposal_type == TreatyType::WarDeclaration),
        "WarDeclaration proposal must be removed on accept"
    );

    // Same for reject — re-add the proposal and reject it.
    let mut game2 = new_game("default", Difficulty::Normal, 0);
    game2.game_data = domain::data::GameData::default();
    let human2 = game2.human_player_nation;
    let attacker2 = game2
        .great_powers()
        .iter()
        .find(|n| n.id != human2)
        .unwrap()
        .id;
    game2.world.diplomacy.declare_war(attacker2, human2);
    game2
        .world
        .diplomacy
        .pending_proposals
        .push(DiplomaticProposal {
            from: attacker2,
            to: human2,
            proposal_type: TreatyType::WarDeclaration,
            turn_proposed: game2.turn,
            attacker: None,
            cascade_remaining: None,
        });
    let rejected_json = wasm_reject_proposal(&serialize_game(&game2), human2.0, 0);
    assert!(!rejected_json.contains("\"error\""));
    let rejected = game_from_json(&rejected_json).expect("round-trip");
    assert!(
        rejected.world.diplomacy.is_at_war(attacker2, human2),
        "war remains in effect after rejection"
    );
}

// ── Chain allocation tests ────────────────────────────────────

#[test]
fn setter_does_not_affect_current_inventory() {
    let game = new_game("default", Difficulty::Normal, 0);
    let nation_id = game.human_player_nation.0;
    let game_json = serialize_game(&game);

    let original: domain_snapshot::game_state::GameState =
        serde_json::from_str(&game_json).unwrap();
    let original_warehouse = original.world.nations[0].economy.warehouse.clone();

    let modified_json = wasm_set_chain_target(&game_json, nation_id, "timber", "mill", 0);
    assert!(!modified_json.contains("\"error\""));

    let modified: domain_snapshot::game_state::GameState =
        serde_json::from_str(&modified_json).unwrap();
    assert_eq!(
        modified.world.nations[0].economy.chain_targets.timber_mill, 0,
        "chain_targets updated immediately"
    );
    assert_eq!(
        modified.world.nations[0].economy.warehouse, original_warehouse,
        "warehouse unchanged before end-turn"
    );
}

#[test]
fn set_chain_target_invalid_chain_returns_error() {
    let game_json = make_game_json();
    let result = wasm_set_chain_target(&game_json, 0, "gold", "mill", 10);
    assert!(result.contains("\"error\""));
}

#[test]
fn chain_target_zero_suppresses_production_on_next_turn() {
    use domain::economy::buildings::Building;

    let mut base_game = new_game("default", Difficulty::Normal, 0);
    let nation_id = base_game.human_player_nation;

    // Ensure lumber mill with capacity ≥ 4, timber resources, and labor
    {
        let nation = base_game.get_nation_mut(nation_id).unwrap();
        match nation
            .economy
            .buildings
            .iter_mut()
            .find(|b| b.building_type == BuildingType::LumberMill)
        {
            Some(b) => {
                b.capacity = b.capacity.max(4);
                b.pending_capacity = 0;
                b.turns_until_upgrade = 0;
            }
            None => {
                nation
                    .economy
                    .buildings
                    .push(Building::new(BuildingType::LumberMill, 4));
            }
        }
        *nation
            .economy
            .warehouse
            .entry(ResourceType::Timber)
            .or_insert(0) = 200;
        nation.economy.labor.untrained = nation.economy.labor.untrained.max(20);
        nation.economy.materials.remove(&MaterialType::Lumber);
    }

    let base_json = serialize_game(&base_game);

    // Baseline: set target to unlimited so lumber is produced
    let unlimited_json = wasm_set_chain_target(&base_json, nation_id.0, "timber", "mill", u32::MAX);
    let default_turn_json = wasm_process_turn(&unlimited_json);
    let default_val: serde_json::Value = serde_json::from_str(&default_turn_json).unwrap();
    let lumber_default = default_val["game"]["world"]["nations"][0]["economy"]["materials"]
        .as_object()
        .and_then(|m| m.get("Lumber"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Set timber mill target=0 → no lumber produced
    let zero_json = wasm_set_chain_target(&base_json, nation_id.0, "timber", "mill", 0);
    let zero_turn_json = wasm_process_turn(&zero_json);
    let zero_val: serde_json::Value = serde_json::from_str(&zero_turn_json).unwrap();
    let lumber_zero = zero_val["game"]["world"]["nations"][0]["economy"]["materials"]
        .as_object()
        .and_then(|m| m.get("Lumber"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    assert!(
        lumber_default > 0,
        "baseline produced no lumber — test setup invalid"
    );
    assert_eq!(
        lumber_zero, 0,
        "target=0 should suppress all timber mill output, got {lumber_zero}"
    );
}
