//! Session-based state management for the WASM bridge.
//!
//! Holds the `GameState` server-side (inside the WASM module) so the
//! frontend no longer needs to round-trip the full state on every call.
//! Exposes three entry points:
//!   - `wasm_session_new_game`  — creates and stores a game
//!   - `wasm_session_command`   — applies a typed `FrontendCommand`
//!   - `wasm_session_query`     — responds to a typed `FrontendQuery`
//!   - `wasm_session_process_turn` — advances the turn and returns the report
//!
//! The existing `wasm_*` API remains available for backward compatibility.

use application::commands::{CommandResult, FrontendCommand};
use application::queries::{
    FrontendQuery, get_diplomacy_screen, get_industry_screen, get_map_screen,
    get_trade_screen, get_transport_screen,
};
use domain::game_state::{GameState, new_game_with_config};
use domain::map::MapGenConfig;
use domain::turn::process_turn;
use domain::types::*;
use wasm_bindgen::prelude::*;

use crate::flavor_bridge;

// ── Session storage ──────────────────────────────────────────────────────────

std::thread_local! {
    static SESSION: std::cell::RefCell<Option<GameState>> =
        std::cell::RefCell::new(None);
}

fn with_session<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&GameState) -> R,
{
    SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .map(f)
            .ok_or_else(|| "no active session — call wasm_session_new_game first".to_string())
    })
}

fn with_session_mut<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut GameState) -> R,
{
    SESSION.with(|s| {
        s.borrow_mut()
            .as_mut()
            .map(f)
            .ok_or_else(|| "no active session".to_string())
    })
}

fn ok_json() -> String {
    r#"{"ok":true}"#.to_string()
}

fn err_json(msg: &str) -> String {
    format!(r#"{{"ok":false,"error":{}}}"#, serde_json::json!(msg))
}

// ── Session lifecycle ────────────────────────────────────────────────────────

/// Create a new game and store it in the session.
/// After this call `wasm_session_command` / `wasm_session_query` become available.
#[wasm_bindgen]
pub fn wasm_session_new_game(
    map_key: &str,
    difficulty: u8,
    nation_index: usize,
    map_width: i32,
    map_height: i32,
    num_great_powers: u32,
    num_minor_nations: u32,
    flavor_key: &str,
) -> String {
    let diff = crate::difficulty_from_u8(difficulty);
    let cfg = MapGenConfig {
        width: map_width.clamp(30, 200),
        height: map_height.clamp(20, 150),
        num_great_powers: (num_great_powers as usize).clamp(1, 20),
        num_minor_nations: (num_minor_nations as usize).min(32),
    };
    let mut game = new_game_with_config(map_key, diff, nation_index, cfg);
    flavor_bridge::apply_flavor(&mut game, flavor_key);

    SESSION.with(|s| *s.borrow_mut() = Some(game));
    ok_json()
}

/// Destroy the current session (frees the stored GameState).
#[wasm_bindgen]
pub fn wasm_session_destroy() {
    SESSION.with(|s| *s.borrow_mut() = None);
}

/// Returns `true` if a session is currently active.
#[wasm_bindgen]
pub fn wasm_session_active() -> bool {
    SESSION.with(|s| s.borrow().is_some())
}

/// Load a game from a JSON snapshot into the session.
/// Use this to restore a saved game.
#[wasm_bindgen]
pub fn wasm_session_load(game_json: &str) -> String {
    match crate::game_from_json(game_json) {
        Ok(game) => {
            SESSION.with(|s| *s.borrow_mut() = Some(game));
            ok_json()
        }
        Err(e) => err_json(&format!("deserialize failed: {e}")),
    }
}

/// Serialize the current session state to JSON (for saving).
#[wasm_bindgen]
pub fn wasm_session_save() -> String {
    match with_session(|g| crate::game_to_json(g)) {
        Ok(json) => json,
        Err(e) => err_json(&e),
    }
}

// ── Turn processing ──────────────────────────────────────────────────────────

/// Process the next turn. Returns the turn report as JSON.
#[wasm_bindgen]
pub fn wasm_session_process_turn() -> String {
    let result = with_session_mut(|game| {
        let report = process_turn(game);
        let human = game.human_player_nation;
        serde_json::json!({
            "ok": true,
            "report": {
                "turn": format!("{}", report.turn),
                "year": report.year,
                "quarter": report.quarter,
                "headlines": report.newspaper_headlines.iter()
                    .map(|h| {
                        let mut obj = serde_json::json!({"text": &h.text, "category": format!("{:?}", h.category)});
                        if let Some(ref reason) = h.reason { obj["reason"] = serde_json::json!(reason); }
                        if h.is_non_action { obj["is_non_action"] = serde_json::json!(true); }
                        if !h.nation_ids.is_empty() { obj["nation_ids"] = serde_json::json!(h.nation_ids.iter().map(|id| id.0).collect::<Vec<_>>()); }
                        obj
                    })
                    .collect::<Vec<_>>(),
                "resources": report.resource_production.iter()
                    .filter(|(nid, _, _)| *nid == human)
                    .map(|(_, r, q)| serde_json::json!({"resource": format!("{:?}", r), "quantity": q}))
                    .collect::<Vec<_>>(),
                "scores": report.scores.iter().map(|(id, name, score)| serde_json::json!({"nation_id": id.0, "name": name, "score": score})).collect::<Vec<_>>(),
            }
        })
        .to_string()
    });
    match result {
        Ok(s) => s,
        Err(e) => err_json(&e),
    }
}

// ── Command dispatch ─────────────────────────────────────────────────────────

/// Apply a typed `FrontendCommand` to the session state.
///
/// The `cmd_json` must be a JSON object with a `"type"` field matching one of
/// the `FrontendCommand` variants, e.g. `{"type":"end_turn"}` or
/// `{"type":"research_tech","tech_name":"Steam Power"}`.
///
/// Returns `{"ok":true}` on success or `{"ok":false,"error":"..."}` on failure.
#[wasm_bindgen]
pub fn wasm_session_command(cmd_json: &str) -> String {
    let cmd: FrontendCommand = match serde_json::from_str(cmd_json) {
        Ok(c) => c,
        Err(e) => return err_json(&format!("invalid command JSON: {e}")),
    };

    let result = with_session_mut(|game| apply_command(game, cmd));
    match result {
        Ok(r) => serde_json::to_string(&r).unwrap_or_else(|_| ok_json()),
        Err(e) => err_json(&e),
    }
}

/// Respond to a typed `FrontendQuery` with a view-model JSON payload.
///
/// Returns a JSON object appropriate to the query type. On error, returns
/// `{"ok":false,"error":"..."}`.
#[wasm_bindgen]
pub fn wasm_session_query(query_json: &str) -> String {
    let query: FrontendQuery = match serde_json::from_str(query_json) {
        Ok(q) => q,
        Err(e) => return err_json(&format!("invalid query JSON: {e}")),
    };

    let result = with_session(|game| respond_to_query(game, query));
    match result {
        Ok(s) => s,
        Err(e) => err_json(&e),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Parse a commodity name string into a `Commodity`, trying Resource, Material,
/// and Goods in order. Accepts plain names like "Timber", "Steel", "Cloth".
fn parse_commodity(name: &str) -> Option<domain::economy::trade::Commodity> {
    use domain::economy::trade::Commodity;
    if let Ok(r) = name.parse::<domain::types::ResourceType>() {
        return Some(Commodity::Resource(r));
    }
    if let Ok(m) = name.parse::<domain::types::MaterialType>() {
        return Some(Commodity::Material(m));
    }
    if let Ok(g) = name.parse::<domain::types::GoodsType>() {
        return Some(Commodity::Goods(g));
    }
    None
}

// ── Command application ──────────────────────────────────────────────────────

fn apply_command(game: &mut GameState, cmd: FrontendCommand) -> CommandResult {
    use FrontendCommand::*;

    if game.observer_mode {
        match &cmd {
            EndTurn | QueueUnitMove { .. } | CancelUnitMove { .. } | DisbandUnit { .. }
            | RecruitArmyUnit { .. } | AssignBeachhead { .. } | HireCivilian { .. }
            | DeployCivilian { .. } | RecallCivilian { .. } | EngineerBuild { .. }
            | ExpandBuilding { .. } | BuildFreightCar { .. } | SetTransportAllocation { .. }
            | SetPlayerSellOrder { .. } | SetPlayerBuyOrder { .. } | SetTradeSubsidy { .. }
            | ResearchTech { .. } | DiplomacyBuildConsulate { .. } | DiplomacyBuildEmbassy { .. }
            | DiplomacyProposeNap { .. } | DiplomacyProposeAlliance { .. }
            | DiplomacyDeclareWar { .. } | DiplomacySendGrant { .. }
            | DiplomacyBreakTreaty { .. } | DiplomacyProposePeace { .. }
            | AcceptProposal { .. } | RejectProposal { .. } | BuildShip { .. }
            | SetShipOperation { .. } => {
                return CommandResult::error("mutations not allowed in observer mode")
            }
        }
    }

    match cmd {
        EndTurn => CommandResult::success(),

        QueueUnitMove { unit_id, target_province } => {
            let uid = domain::map::UnitId(unit_id);
            let nid = game.human_player_nation;
            let dest = ProvinceId(target_province);
            match game.get_province(dest) {
                None => return CommandResult::error("province not found"),
                Some(p) => {
                    let owner = p.owner;
                    let at_war = game.world.diplomacy.is_at_war(nid, owner);
                    let anarchic = game.get_nation(owner).is_some_and(|n| n.diplomacy.is_in_anarchy);
                    if owner != nid && !at_war && !anarchic {
                        return CommandResult::error("cannot move to that province");
                    }
                }
            }
            game.transient.pending_moves.retain(|(_, id, _)| *id != uid);
            game.transient.pending_moves.push((nid, uid, dest));
            CommandResult::success()
        }

        CancelUnitMove { unit_id } => {
            let uid = domain::map::UnitId(unit_id);
            game.transient.pending_moves.retain(|(_, id, _)| *id != uid);
            CommandResult::success()
        }

        DisbandUnit { unit_id } => {
            let uid = domain::map::UnitId(unit_id);
            let nid = game.human_player_nation;
            let nation = match game.get_nation_mut(nid) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            let before = nation.military.army.len();
            nation.military.army.retain(|u| u.id != uid);
            if nation.military.army.len() == before {
                CommandResult::error("unit not found")
            } else {
                CommandResult::success()
            }
        }

        RecruitArmyUnit { nation_id, unit_type } => {
            let nation_id = NationId(nation_id);
            use domain::military::units::ArmyUnitType;
            let unit_type = match unit_type.as_str() {
                "Regulars" => ArmyUnitType::Regulars,
                "Guards" => ArmyUnitType::Guards,
                "LightArtillery" => ArmyUnitType::LightArtillery,
                "SiegeArtillery" => ArmyUnitType::SiegeArtillery,
                "Cuirassiers" => ArmyUnitType::Cuirassiers,
                _ => return CommandResult::error("unknown unit type"),
            };
            let cap = game.get_nation(nation_id).map(|n| n.capital_province_id).unwrap_or(domain::types::ProvinceId(0));
            let id = game.alloc_unit_id();
            let nation = match game.get_nation_mut(nation_id) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            nation.military.army.push(domain::military::units::ArmyUnit::new(id, unit_type, nation_id, cap));
            CommandResult::success()
        }

        AssignBeachhead { nation_id, target_province } => {
            game.transient.pending_landings.push((NationId(nation_id), ProvinceId(target_province), game.turn));
            CommandResult::success()
        }

        ResearchTech { tech_name } => {
            let nid = game.human_player_nation;
            let nation = match game.get_nation(nid) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            let lower = tech_name.to_lowercase();
            let tech = game
                .game_data
                .tech_tree
                .available_techs(&nation.researched_techs, game.turn.year())
                .into_iter()
                .find(|t| t.name.to_lowercase().contains(&lower));
            match tech {
                None => CommandResult::error(format!("tech not found: {tech_name}")),
                Some(t) => {
                    let cost = t.cost;
                    let tid = t.id;
                    let nation = game.get_nation_mut(nid).unwrap();
                    if nation.economy.treasury.checked_sub(cost).is_none() {
                        return CommandResult::error("insufficient funds");
                    }
                    nation.economy.treasury -= cost;
                    nation.research_tech(tid);
                    CommandResult::success()
                }
            }
        }

        BuildFreightCar { nation_id } => {
            let nation_id = NationId(nation_id);
            let nation = match game.get_nation_mut(nation_id) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            nation.military.transport.build_freight_cars(1);
            CommandResult::success()
        }

        ExpandBuilding { nation_id, building_type } => {
            let nation_id = NationId(nation_id);
            use domain::economy::buildings::{Building, BuildingType};
            use domain::types::MaterialType;
            let bt: BuildingType = match building_type.parse::<BuildingType>() {
                Ok(b) => b,
                Err(_) => return CommandResult::error("unknown building type"),
            };
            let nation = match game.get_nation_mut(nation_id) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            let building = match nation.economy.buildings.iter().find(|b| b.building_type == bt) {
                Some(b) => b,
                None => return CommandResult::error("building not found"),
            };
            if building.is_expanding() {
                return CommandResult::error("building expansion already in progress");
            }
            let increase = building.next_capacity().saturating_sub(building.capacity);
            let (lumber_needed, steel_needed) = Building::expansion_cost(increase);
            let lumber_have = nation.economy.materials.get(&MaterialType::Lumber).copied().unwrap_or(0);
            let steel_have = nation.economy.materials.get(&MaterialType::Steel).copied().unwrap_or(0);
            if lumber_have < lumber_needed || steel_have < steel_needed {
                return CommandResult::error("insufficient materials for expansion");
            }
            *nation.economy.materials.entry(MaterialType::Lumber).or_insert(0) -= lumber_needed;
            *nation.economy.materials.entry(MaterialType::Steel).or_insert(0) -= steel_needed;
            let building = nation.economy.buildings.iter_mut().find(|b| b.building_type == bt).unwrap();
            building.start_expansion_to_next_tier();
            CommandResult::success()
        }

        HireCivilian { nation_id, civilian_type } => {
            let nation_id = NationId(nation_id);
            use domain::economy::civilians::parse_civilian_type;
            let ct = match parse_civilian_type(&civilian_type) {
                Some(t) => t,
                None => return CommandResult::error("unknown civilian type"),
            };
            let cost = ct.creation_cost(&game.game_data.game_config);
            let nation = match game.get_nation(nation_id) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            if nation.economy.treasury < cost {
                return CommandResult::error("insufficient funds");
            }
            let id = game.alloc_unit_id();
            let nation = game.get_nation_mut(nation_id).unwrap();
            nation.economy.treasury -= cost;
            nation.military.civilians.push(domain::economy::civilians::Civilian::new(id, ct, nation_id));
            CommandResult::success()
        }

        DeployCivilian { civilian_id, hex_q, hex_r } => {
            let cid = domain::map::UnitId(civilian_id);
            let nid = game.human_player_nation;
            let coord = domain::hex::HexCoord::new(hex_q, hex_r);
            let has_province = game.world.provinces.iter().any(|p| p.tiles.contains(&coord));
            if !has_province {
                return CommandResult::error("no province at that location");
            }
            let nation = match game.get_nation_mut(nid) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            let civ = match nation.military.civilians.iter_mut().find(|c| c.id == cid) {
                Some(c) => c,
                None => return CommandResult::error("civilian not found"),
            };
            civ.position = Some(coord);
            CommandResult::success()
        }

        RecallCivilian { civilian_id } => {
            let cid = domain::map::UnitId(civilian_id);
            let nid = game.human_player_nation;
            let nation = match game.get_nation_mut(nid) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            let civ = match nation.military.civilians.iter_mut().find(|c| c.id == cid) {
                Some(c) => c,
                None => return CommandResult::error("civilian not found"),
            };
            civ.position = None;
            CommandResult::success()
        }

        EngineerBuild { civilian_id: _, build_kind: _ } => {
            CommandResult::error("engineer build not yet implemented in session API")
        }

        BuildShip { nation_id, ship_type } => {
            let nation_id = NationId(nation_id);
            use domain::military::ships::{ShipCategory, ShipType};
            let st: ShipType = match ship_type.parse::<ShipType>() {
                Ok(s) => s,
                Err(_) => return CommandResult::error("unknown ship type"),
            };
            let id = game.alloc_unit_id();
            let nation = match game.get_nation_mut(nation_id) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            let ship = domain::military::ships::Ship::new(id, st, nation_id);
            match st.category() {
                ShipCategory::Merchant => nation.military.merchant_fleet.push(ship),
                ShipCategory::Warship => {
                    nation.military.warships.push(ship);
                    nation.military.warships_built += 1;
                }
            }
            CommandResult::success()
        }

        SetShipOperation { ship_id, operation: _ } => {
            let sid = domain::map::UnitId(ship_id);
            let _ = game.world.nations.iter().any(|n| n.military.warships.iter().any(|s| s.id == sid));
            CommandResult::error("ship operation not yet implemented in session API")
        }

        SetTransportAllocation { nation_id: _, commodity: _, amount: _ } => {
            CommandResult::error("transport allocation not yet implemented in session API")
        }

        SetPlayerSellOrder { nation_id, commodity, quantity, price_cents: _ } => {
            let nation_id = NationId(nation_id);
            let c = match parse_commodity(&commodity) {
                Some(c) => c,
                None => return CommandResult::error("unknown commodity"),
            };
            let nation = match game.get_nation_mut(nation_id) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            nation.diplomacy.player_sell_orders.retain(|o| o.commodity != c);
            nation.diplomacy.player_sell_orders.push(domain::economy::trade::PlayerSellOrder {
                commodity: c,
                quantity,
            });
            CommandResult::success()
        }

        SetPlayerBuyOrder { nation_id, commodity, quantity, price_cents } => {
            let nation_id = NationId(nation_id);
            let r: ResourceType = match commodity.parse::<ResourceType>() {
                Ok(r) => r,
                Err(_) => return CommandResult::error("unknown resource"),
            };
            let max_price = Money::from_cents(price_cents as i64);
            let nation = match game.get_nation_mut(nation_id) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            nation.diplomacy.player_buy_orders.retain(|o| o.resource != r);
            nation.diplomacy.player_buy_orders.push(domain::economy::trade::PlayerBuyOrder {
                resource: r,
                quantity,
                max_price_per_unit: max_price,
            });
            CommandResult::success()
        }

        SetTradeSubsidy { from_nation, to_nation, subsidy_dollars } => {
            let from_nation = NationId(from_nation);
            let to_nation = NationId(to_nation);
            let nation = match game.get_nation_mut(from_nation) {
                Some(n) => n,
                None => return CommandResult::error("nation not found"),
            };
            let amount = Money::dollars(subsidy_dollars as i64);
            if subsidy_dollars == 0 {
                nation.diplomacy.trade_subsidies.remove(&to_nation);
            } else {
                nation.diplomacy.trade_subsidies.insert(to_nation, amount);
            }
            CommandResult::success()
        }

        DiplomacyBuildConsulate { player, target } => {
            let player = NationId(player);
            let target = NationId(target);
            match game.world.diplomacy.build_consulate(player, target) {
                Ok(_) => {
                    let cost = Money::dollars(game.game_data.game_config.consulate_cost as i64);
                    if let Some(n) = game.get_nation_mut(player) {
                        n.economy.treasury -= cost;
                    }
                    CommandResult::success()
                }
                Err(e) => CommandResult::error(e),
            }
        }

        DiplomacyBuildEmbassy { player, target } => {
            let player = NationId(player);
            let target = NationId(target);
            match game.world.diplomacy.build_embassy(player, target) {
                Ok(_) => {
                    let cost = Money::dollars(game.game_data.game_config.embassy_cost as i64);
                    if let Some(n) = game.get_nation_mut(player) {
                        n.economy.treasury -= cost;
                    }
                    CommandResult::success()
                }
                Err(e) => CommandResult::error(e),
            }
        }

        DiplomacyProposeNap { from, to } => {
            let from = NationId(from);
            let to = NationId(to);
            match game.world.diplomacy.propose_pact(from, to) {
                Ok(_) => CommandResult::success(),
                Err(e) => CommandResult::error(e),
            }
        }

        DiplomacyProposeAlliance { from, to } => {
            let from = NationId(from);
            let to = NationId(to);
            match game.world.diplomacy.propose_alliance(from, to) {
                Ok(_) => CommandResult::success(),
                Err(e) => CommandResult::error(e),
            }
        }

        DiplomacyDeclareWar { from, to } => {
            let from = NationId(from);
            let to = NationId(to);
            game.world.diplomacy.declare_war(from, to);
            CommandResult::success()
        }

        DiplomacySendGrant { from, to, amount_dollars } => {
            let from = NationId(from);
            let to = NationId(to);
            let amount = Money::dollars(amount_dollars as i64);
            if let Some(n) = game.get_nation_mut(from) {
                if n.economy.treasury < amount {
                    return CommandResult::error("insufficient funds");
                }
                n.economy.treasury -= amount;
            }
            if let Some(n) = game.get_nation_mut(to) {
                n.economy.treasury += amount;
            }
            CommandResult::success()
        }

        DiplomacyBreakTreaty { from, to } => {
            let from = NationId(from);
            let to = NationId(to);
            use domain::events::TreatyType;
            game.world.diplomacy.break_treaty(from, to, TreatyType::Alliance);
            game.world.diplomacy.break_treaty(from, to, TreatyType::NonAggressionPact);
            CommandResult::success()
        }

        DiplomacyProposePeace { from, to } => {
            let from = NationId(from);
            let to = NationId(to);
            match game.world.diplomacy.propose_peace(from, to, game.turn) {
                Ok(_) => CommandResult::success(),
                Err(e) => CommandResult::error(e),
            }
        }

        AcceptProposal { nation_id, proposal_index } => {
            let nation_id = NationId(nation_id);
            use domain::events::TreatyType;
            let proposals: Vec<_> = game.world.diplomacy
                .pending_proposals
                .iter()
                .filter(|p| p.to == nation_id)
                .cloned()
                .collect();
            let idx = proposal_index as usize;
            if idx >= proposals.len() {
                return CommandResult::error("proposal index out of range");
            }
            let proposal = proposals[idx].clone();
            // Apply first; only remove from pending on success.
            match proposal.proposal_type {
                TreatyType::NonAggressionPact => {
                    if let Err(e) = game.world.diplomacy.propose_pact(proposal.from, proposal.to) {
                        return CommandResult::error(e);
                    }
                }
                TreatyType::Alliance => {
                    if let Err(e) = game.world.diplomacy.propose_alliance(proposal.from, proposal.to) {
                        return CommandResult::error(e);
                    }
                }
                TreatyType::PeaceTreaty => {
                    game.world.diplomacy.queue_peace(proposal.from, proposal.to);
                }
                _ => return CommandResult::error("proposal type not supported in session API"),
            }
            game.world.diplomacy.pending_proposals.retain(|p| {
                !(p.to == proposal.to && p.from == proposal.from && p.proposal_type == proposal.proposal_type)
            });
            CommandResult::success()
        }

        RejectProposal { nation_id, proposal_index } => {
            let nation_id = NationId(nation_id);
            let proposals: Vec<_> = game.world.diplomacy
                .pending_proposals
                .iter()
                .filter(|p| p.to == nation_id)
                .cloned()
                .collect();
            let idx = proposal_index as usize;
            if idx >= proposals.len() {
                return CommandResult::error("proposal index out of range");
            }
            let proposal = &proposals[idx];
            game.world.diplomacy.pending_proposals.retain(|p| {
                !(p.to == proposal.to && p.from == proposal.from && p.proposal_type == proposal.proposal_type)
            });
            CommandResult::success()
        }
    }
}

// ── Query responses ──────────────────────────────────────────────────────────

fn respond_to_query(game: &GameState, query: FrontendQuery) -> String {
    use FrontendQuery::*;
    match query {
        MapScreen => {
            let data = get_map_screen(game);
            serde_json::to_string(&serde_json::json!({
                "ok": true,
                "turn": data.turn,
                "nation_name": data.nation_name,
                "treasury": data.treasury,
                "province_count": data.province_count,
                "army_count": data.army_count,
                "civilian_count": data.civilian_count,
            }))
            .unwrap_or_else(|_| ok_json())
        }
        TransportScreen => {
            let data = get_transport_screen(game);
            serde_json::to_string(&serde_json::json!({
                "ok": true,
                "freight_cars": data.freight_cars,
                "total_capacity": data.total_capacity,
                "total_production": data.total_production,
                "utilization_percent": data.utilization_percent,
            }))
            .unwrap_or_else(|_| ok_json())
        }
        IndustryScreen => {
            let data = get_industry_screen(game);
            serde_json::to_string(&serde_json::json!({
                "ok": true,
                "buildings": data.buildings,
                "workers": data.workers,
                "warehouse_summary": data.warehouse_summary,
            }))
            .unwrap_or_else(|_| ok_json())
        }
        TradeScreen => {
            let data = get_trade_screen(game);
            serde_json::to_string(&serde_json::json!({
                "ok": true,
                "cargo_capacity": data.cargo_capacity,
                "cargo_used": data.cargo_used,
                "partners": data.partners.iter().map(|p| serde_json::json!({
                    "nation_name": &p.nation_name,
                    "nation_id": p.nation_id,
                    "has_consulate": p.has_consulate,
                    "relationship_score": p.relationship_score,
                    "available_resources": &p.available_resources,
                })).collect::<Vec<_>>(),
            }))
            .unwrap_or_else(|_| ok_json())
        }
        DiplomacyScreen => {
            let data = get_diplomacy_screen(game);
            serde_json::to_string(&serde_json::json!({
                "ok": true,
                "standing": data.standing,
                "great_power_relations": data.great_power_relations,
                "minor_nation_relations": data.minor_nation_relations,
                "council_projection": data.council_projection,
            }))
            .unwrap_or_else(|_| ok_json())
        }
    }
}

// ── Session command tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::game_state::new_game;
    use domain::types::Difficulty;

    fn setup() -> GameState {
        new_game("default", Difficulty::Normal, 0)
    }

    fn cmd(json: &str) -> serde_json::Value {
        serde_json::from_str(json).unwrap()
    }

    // ── QueueUnitMove ────────────────────────────────────────────

    #[test]
    fn queue_unit_move_to_own_province_succeeds() {
        let mut game = setup();
        let nid = game.human_player_nation;
        let own_province = game.get_nation(nid).unwrap().capital_province_id;
        let unit_id = game.get_nation(nid).unwrap().military.army[0].id.0;

        let cmd = FrontendCommand::QueueUnitMove { unit_id, target_province: own_province.0 };
        let result = apply_command(&mut game, cmd);
        assert!(result.ok, "move to own province should succeed");
        assert!(!game.transient.pending_moves.is_empty());
    }

    #[test]
    fn cancel_unit_move_removes_pending() {
        let mut game = setup();
        let nid = game.human_player_nation;
        let own_province = game.get_nation(nid).unwrap().capital_province_id;
        let unit_id = game.get_nation(nid).unwrap().military.army[0].id.0;

        apply_command(&mut game, FrontendCommand::QueueUnitMove { unit_id, target_province: own_province.0 });
        assert!(!game.transient.pending_moves.is_empty());

        let result = apply_command(&mut game, FrontendCommand::CancelUnitMove { unit_id });
        assert!(result.ok);
        assert!(game.transient.pending_moves.is_empty());
    }

    // ── HireCivilian ─────────────────────────────────────────────

    #[test]
    fn hire_civilian_deducts_treasury() {
        let mut game = setup();
        let nid = game.human_player_nation;
        let before = game.get_nation(nid).unwrap().economy.treasury;
        let before_count = game.get_nation(nid).unwrap().military.civilians.len();

        let result = apply_command(&mut game, FrontendCommand::HireCivilian {
            nation_id: nid.0,
            civilian_type: "Farmer".to_string(),
        });
        assert!(result.ok, "{:?}", result.message);
        let after = game.get_nation(nid).unwrap().economy.treasury;
        assert!(after < before, "treasury should decrease after hiring");
        assert_eq!(game.get_nation(nid).unwrap().military.civilians.len(), before_count + 1);
    }

    #[test]
    fn hire_civilian_fails_if_insufficient_funds() {
        let mut game = setup();
        let nid = game.human_player_nation;
        game.get_nation_mut(nid).unwrap().economy.treasury = domain::types::Money::ZERO;

        let result = apply_command(&mut game, FrontendCommand::HireCivilian {
            nation_id: nid.0,
            civilian_type: "Farmer".to_string(),
        });
        assert!(!result.ok, "should fail with no funds");
    }

    // ── BuildShip — category routing ─────────────────────────────

    #[test]
    fn build_warship_goes_to_warships() {
        let mut game = setup();
        let nid = game.human_player_nation;
        let before = game.get_nation(nid).unwrap().military.warships.len();

        let result = apply_command(&mut game, FrontendCommand::BuildShip {
            nation_id: nid.0,
            ship_type: "Frigate".to_string(),
        });
        assert!(result.ok, "{:?}", result.message);
        assert_eq!(game.get_nation(nid).unwrap().military.warships.len(), before + 1);
    }

    #[test]
    fn build_merchant_ship_goes_to_merchant_fleet() {
        let mut game = setup();
        let nid = game.human_player_nation;
        let before = game.get_nation(nid).unwrap().military.merchant_fleet.len();

        let result = apply_command(&mut game, FrontendCommand::BuildShip {
            nation_id: nid.0,
            ship_type: "Trader".to_string(),
        });
        assert!(result.ok, "{:?}", result.message);
        assert_eq!(game.get_nation(nid).unwrap().military.merchant_fleet.len(), before + 1);
    }

    // ── SetPlayerSellOrder ────────────────────────────────────────

    #[test]
    fn set_player_sell_order_stores_order() {
        let mut game = setup();
        let nid = game.human_player_nation;

        let result = apply_command(&mut game, FrontendCommand::SetPlayerSellOrder {
            nation_id: nid.0,
            commodity: "Timber".to_string(),
            quantity: 5,
            price_cents: 1000,
        });
        assert!(result.ok, "{:?}", result.message);
        assert_eq!(game.get_nation(nid).unwrap().diplomacy.player_sell_orders.len(), 1);
    }

    // ── AcceptProposal — NAP ──────────────────────────────────────

    #[test]
    fn accept_nap_proposal_applies_treaty() {
        use domain::diplomacy::DiplomaticProposal;
        use domain::events::TreatyType;

        let mut game = setup();
        let nid = game.human_player_nation;
        let other = game.great_powers().iter().find(|n| n.id != nid).unwrap().id;

        game.world.diplomacy.pending_proposals.push(DiplomaticProposal {
            from: other,
            to: nid,
            proposal_type: TreatyType::NonAggressionPact,
            turn_proposed: game.turn,
            attacker: None,
            cascade_remaining: None,
        });

        let result = apply_command(&mut game, FrontendCommand::AcceptProposal {
            nation_id: nid.0,
            proposal_index: 0,
        });
        assert!(result.ok, "{:?}", result.message);
        assert!(game.world.diplomacy.pending_proposals.is_empty(), "proposal should be removed");
        let rel = game.world.diplomacy.get_relation(nid, other).unwrap();
        assert!(rel.has_treaty(TreatyType::NonAggressionPact), "NAP treaty should be applied");
    }

    // ── ExpandBuilding re-entry guard ────────────────────────────

    #[test]
    fn expand_building_rejects_while_already_expanding() {
        use domain::economy::buildings::BuildingType;

        let mut game = setup();
        let nid = game.human_player_nation;

        // Manually start an expansion on LumberMill
        if let Some(mill) = game.get_nation_mut(nid).unwrap().get_building_mut(BuildingType::LumberMill) {
            mill.start_expansion(1);
        } else {
            return; // no LumberMill on this difficulty — test is N/A
        }

        let result = apply_command(&mut game, FrontendCommand::ExpandBuilding {
            nation_id: nid.0,
            building_type: "LumberMill".to_string(),
        });
        assert!(!result.ok, "re-entry should be rejected when expansion is in progress");
    }

    // ── PeaceTreaty acceptance ────────────────────────────────────

    #[test]
    fn accept_peace_calls_queue_peace() {
        use domain::diplomacy::DiplomaticProposal;
        use domain::events::TreatyType;

        let mut game = setup();
        let nid = game.human_player_nation;
        let other = game.great_powers().iter().find(|n| n.id != nid).unwrap().id;

        game.world.diplomacy.declare_war(other, nid);
        game.world.diplomacy.pending_proposals.push(DiplomaticProposal {
            from: other,
            to: nid,
            proposal_type: TreatyType::PeaceTreaty,
            turn_proposed: game.turn,
            attacker: None,
            cascade_remaining: None,
        });

        let result = apply_command(&mut game, FrontendCommand::AcceptProposal {
            nation_id: nid.0,
            proposal_index: 0,
        });
        assert!(result.ok, "{:?}", result.message);
        assert!(!game.world.diplomacy.is_at_war(nid, other), "war should end after peace acceptance");
        assert!(game.world.diplomacy.pending_proposals.is_empty());
    }

    // ── AcceptProposal failure keeps proposal ────────────────────

    #[test]
    fn accept_proposal_failure_keeps_proposal_in_pending() {
        use domain::diplomacy::DiplomaticProposal;
        use domain::events::TreatyType;

        let mut game = setup();
        let nid = game.human_player_nation;
        let other = game.great_powers().iter().find(|n| n.id != nid).unwrap().id;

        // PactDefenseRequest is unsupported — apply will return error
        game.world.diplomacy.pending_proposals.push(DiplomaticProposal {
            from: other,
            to: nid,
            proposal_type: TreatyType::PactDefenseRequest,
            turn_proposed: game.turn,
            attacker: None,
            cascade_remaining: None,
        });

        let result = apply_command(&mut game, FrontendCommand::AcceptProposal {
            nation_id: nid.0,
            proposal_index: 0,
        });
        assert!(!result.ok, "unsupported proposal type should fail");
        assert_eq!(game.world.diplomacy.pending_proposals.len(), 1, "proposal should remain on failure");
    }

    // ── Observer mode blocks mutations ───────────────────────────

    #[test]
    fn observer_mode_blocks_all_mutations() {
        let mut game = setup();
        game.observer_mode = true;

        let result = apply_command(&mut game, FrontendCommand::EndTurn);
        assert!(!result.ok, "mutations should be blocked in observer mode");
    }

    // ── Queries ───────────────────────────────────────────────────

    #[test]
    fn map_screen_query_returns_ok() {
        let game = setup();
        let resp = respond_to_query(&game, FrontendQuery::MapScreen);
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["ok"], true);
        assert!(v["turn"].is_string());
    }

    #[test]
    fn invalid_command_json_returns_error() {
        SESSION.with(|s| *s.borrow_mut() = Some(setup()));
        let result = wasm_session_command("not valid json");
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["ok"], false);
    }

    #[test]
    fn query_without_session_returns_error() {
        SESSION.with(|s| *s.borrow_mut() = None);
        let result = wasm_session_query("{\"type\":\"map_screen\"}");
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["ok"], false);
    }
}
