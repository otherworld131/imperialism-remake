//! Ledger / statistics screen queries.
//!
//! Verbatim moves from `crates/wasm-bridge/src/lib.rs` — bodies must stay
//! byte-identical to the originals (error JSON strings included).

use crate::ApiError;
use domain::game_state::GameState;
use domain::types::*;

/// Return comprehensive ledger/statistics data for a nation.
pub fn get_ledger_data(game: &GameState, nation_id: u32) -> Result<serde_json::Value, ApiError> {
    let nid = NationId(nation_id);
    let nation = match game.get_nation(nid) {
        Some(n) => n,
        None => return Err(ApiError::raw("{\"error\":\"Nation not found\"}")),
    };

    // Economy
    let treasury_dollars = nation.economy.treasury.as_dollars();
    let subsidies: Vec<serde_json::Value> = nation
        .diplomacy
        .trade_subsidies
        .iter()
        .map(|(target_id, amount)| {
            let name = game
                .get_nation(*target_id)
                .map(|n| n.name.as_str())
                .unwrap_or("?");
            serde_json::json!({"nation": name, "amount": amount.as_dollars()})
        })
        .collect();

    // Buildings
    let buildings: Vec<serde_json::Value> = nation
        .economy
        .buildings
        .iter()
        .map(|b| {
            serde_json::json!({
                "type": format!("{:?}", b.building_type),
                "capacity": b.capacity,
                "upgrading": b.turns_until_upgrade > 0,
            })
        })
        .collect();

    // Resources, materials, goods
    let resources: Vec<serde_json::Value> = nation
        .economy
        .warehouse
        .iter()
        .filter(|(_, qty)| **qty > 0)
        .map(|(rt, qty)| serde_json::json!({"name": format!("{:?}", rt), "quantity": qty}))
        .collect();
    let materials: Vec<serde_json::Value> = nation
        .economy
        .materials
        .iter()
        .filter(|(_, qty)| **qty > 0)
        .map(|(mt, qty)| serde_json::json!({"name": format!("{:?}", mt), "quantity": qty}))
        .collect();
    let goods: Vec<serde_json::Value> = nation
        .economy
        .goods
        .iter()
        .filter(|(_, qty)| **qty > 0)
        .map(|(gt, qty)| serde_json::json!({"name": format!("{:?}", gt), "quantity": qty}))
        .collect();

    // Military — army by type (BTreeMap: deterministic output order)
    let mut army_counts: std::collections::BTreeMap<String, (u32, u32)> =
        std::collections::BTreeMap::new();
    for unit in &nation.military.army {
        let type_name = format!("{:?}", unit.unit_type);
        let fp = unit.unit_type.stats().firepower;
        let entry = army_counts.entry(type_name).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += fp;
    }
    let army_by_type: Vec<serde_json::Value> = army_counts
        .iter()
        .map(|(name, (count, fp))| {
            serde_json::json!({"unit_type": name, "count": count, "firepower": fp})
        })
        .collect();
    let total_army_fp: u32 = army_counts.values().map(|(_, fp)| fp).sum();

    // Warships by type (BTreeMap: deterministic output order)
    let mut warship_counts: std::collections::BTreeMap<String, u32> =
        std::collections::BTreeMap::new();
    for ship in &nation.military.warships {
        let type_name = format!("{:?}", ship.ship_type);
        *warship_counts.entry(type_name).or_insert(0) += 1;
    }
    let warships_by_type: Vec<serde_json::Value> = warship_counts
        .iter()
        .map(|(name, count)| serde_json::json!({"ship_type": name, "count": count}))
        .collect();

    // Diplomacy summary
    let standing = game.world.diplomacy.get_standing(nid);
    let mut consulate_count = 0u32;
    let mut embassy_count = 0u32;
    let mut treaties: Vec<serde_json::Value> = Vec::new();
    let mut wars: Vec<String> = Vec::new();

    for other in &game.world.nations {
        if other.id == nid {
            continue;
        }
        if let Some(rel) = game.world.diplomacy.get_relation(nid, other.id) {
            if rel.has_consulate {
                consulate_count += 1;
            }
            if rel.has_embassy {
                embassy_count += 1;
            }
            if rel.at_war {
                wars.push(other.name.clone());
            }
            for t in &rel.active_treaties {
                treaties.push(
                    serde_json::json!({"nation": other.name, "treaty_type": format!("{:?}", t)}),
                );
            }
        }
    }

    let result = serde_json::json!({
        "economy": {
            "treasury": treasury_dollars,
            "goods_revenue": nation.archives.goods_sales_revenue_dollars,
            "subsidies": subsidies,
        },
        "production": {
            "buildings": buildings,
            "resources": resources,
            "materials": materials,
            "goods": goods,
        },
        "military": {
            "army_by_type": army_by_type,
            "total_army_fp": total_army_fp,
            "total_army_count": nation.military.army.len(),
            "field_army_count": nation.field_army_count(),
            "militia_count": nation.military.army.len() - nation.field_army_count(),
            "warships_by_type": warships_by_type,
            "total_warship_count": nation.military.warships.len(),
            "merchant_ships": nation.military.merchant_fleet.len(),
            "total_arms_built": nation.military.total_arms_built,
            "generals_earned": nation.military.generals_earned,
        },
        "diplomacy": {
            "standing": standing,
            "consulates": consulate_count,
            "embassies": embassy_count,
            "treaties": treaties,
            "wars": wars,
        },
        "labor": {
            "untrained": nation.economy.labor.untrained,
            "trained": nation.economy.labor.trained,
            "expert": nation.economy.labor.expert,
            "total": nation.economy.labor.total_workers(),
        },
    });

    Ok(result)
}

/// Return ledger data for ALL Great Powers.
pub fn get_all_gp_ledger_data(game: &GameState) -> Result<serde_json::Value, ApiError> {
    let entries: Vec<serde_json::Value> = game
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|nation| {
            let nid = nation.id;
            let nation_name = &nation.name;
            let nation_color = format!("{:?}", nation.color);
            let is_human = nid == game.human_player_nation;

            // Per-nation ledger data (same logic as get_ledger_data)
            let treasury_dollars = nation.economy.treasury.as_dollars();
            let provinces = nation.province_ids.len();

            let mut total_army_fp: u32 = 0;
            let total_army_count = nation.military.army.len();
            for unit in &nation.military.army {
                total_army_fp += unit.unit_type.stats().firepower;
            }
            let total_warship_count = nation.military.warships.len();
            let merchant_ships = nation.military.merchant_fleet.len();

            let building_count = nation.economy.buildings.len();

            let standing = game.world.diplomacy.get_standing(nid);
            let mut consulate_count = 0u32;
            let mut embassy_count = 0u32;
            let mut alliance_count = 0u32;
            let mut war_count = 0u32;
            let mut wars: Vec<String> = Vec::new();
            let mut alliances: Vec<String> = Vec::new();

            for other in &game.world.nations {
                if other.id == nid {
                    continue;
                }
                if let Some(rel) = game.world.diplomacy.get_relation(nid, other.id) {
                    if rel.has_consulate {
                        consulate_count += 1;
                    }
                    if rel.has_embassy {
                        embassy_count += 1;
                    }
                    if rel.at_war {
                        war_count += 1;
                        wars.push(other.name.clone());
                    }
                    if rel.has_treaty(domain::events::TreatyType::Alliance) {
                        alliance_count += 1;
                        alliances.push(other.name.clone());
                    }
                }
            }

            // Resource totals
            let total_resources: u32 = nation.economy.warehouse.values().sum();
            let total_materials: u32 = nation.economy.materials.values().sum();
            let total_goods: u32 = nation.economy.goods.values().sum();

            // Per-resource breakdown
            let resources_detail: serde_json::Map<String, serde_json::Value> = nation
                .economy
                .warehouse
                .iter()
                .map(|(k, v)| (format!("{:?}", k), serde_json::json!(*v)))
                .collect();

            // Per-material breakdown
            let materials_detail: serde_json::Map<String, serde_json::Value> = nation
                .economy
                .materials
                .iter()
                .map(|(k, v)| (format!("{:?}", k), serde_json::json!(*v)))
                .collect();

            // Per-goods breakdown
            let goods_detail: serde_json::Map<String, serde_json::Value> = nation
                .economy
                .goods
                .iter()
                .map(|(k, v)| (format!("{:?}", k), serde_json::json!(*v)))
                .collect();

            // Technology data
            let researched_count = nation.researched_techs.len();
            let researched_names: Vec<String> = nation
                .researched_techs
                .iter()
                .filter_map(|tid| {
                    game.game_data
                        .tech_tree
                        .all_techs()
                        .iter()
                        .find(|t| t.id == *tid)
                        .map(|t| t.name.clone())
                })
                .collect();

            // Per-nation cash-flow breakdown (last processed turn) — read from
            // `game.transient.last_cash_flow`, populated by the turn processor.
            let cash_flow_json = if let Some(flow) = game.transient.last_cash_flow.get(&nid) {
                let income_map: serde_json::Map<String, serde_json::Value> = flow
                    .income_totals_by_source()
                    .into_iter()
                    .map(|(k, v)| (k.label().to_string(), serde_json::json!(v)))
                    .collect();
                let expense_map: serde_json::Map<String, serde_json::Value> = flow
                    .expense_totals_by_sink()
                    .into_iter()
                    .map(|(k, v)| (k.label().to_string(), serde_json::json!(v)))
                    .collect();
                let income_by_cat: serde_json::Map<String, serde_json::Value> = flow
                    .income_by_category()
                    .into_iter()
                    .map(|(k, v)| (k.label().to_string(), serde_json::json!(v)))
                    .collect();
                let expense_by_cat: serde_json::Map<String, serde_json::Value> = flow
                    .expense_by_category()
                    .into_iter()
                    .map(|(k, v)| (k.label().to_string(), serde_json::json!(v)))
                    .collect();
                serde_json::json!({
                    "opening_treasury": flow.opening_treasury.as_dollars(),
                    "closing_treasury": flow.closing_treasury.as_dollars(),
                    "total_income": flow.total_income().as_dollars(),
                    "total_expense": flow.total_expense().as_dollars(),
                    "observed_delta": flow.observed_delta().as_dollars(),
                    "accounted_delta": flow.accounted_delta().as_dollars(),
                    "reconciliation_mismatch": flow.reconciliation_mismatch().as_dollars(),
                    "reconciles": flow.reconciles(),
                    "income_totals": income_map,
                    "expense_totals": expense_map,
                    "income_by_category": income_by_cat,
                    "expense_by_category": expense_by_cat,
                })
            } else {
                serde_json::Value::Null
            };
            let cumulative_income: serde_json::Map<String, serde_json::Value> = nation
                .archives
                .cash_income_totals
                .iter()
                .map(|(k, v)| (k.label().to_string(), serde_json::json!(*v)))
                .collect();
            let cumulative_expense: serde_json::Map<String, serde_json::Value> = nation
                .archives
                .cash_expense_totals
                .iter()
                .map(|(k, v)| (k.label().to_string(), serde_json::json!(*v)))
                .collect();

            // Resource-flow (last turn) — best-effort visibility, NOT reconciled.
            let resource_flow_json = if let Some(flow) = game.transient.last_resource_flow.get(&nid)
            {
                let inflow: Vec<serde_json::Value> = flow
                    .inflow
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "stockpile": e.stockpile.label(),
                            "source": e.source.label(),
                            "category": e.source.category().label(),
                            "amount": e.amount,
                        })
                    })
                    .collect();
                let outflow: Vec<serde_json::Value> = flow
                    .outflow
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "stockpile": e.stockpile.label(),
                            "sink": e.sink.label(),
                            "category": e.sink.category().label(),
                            "amount": e.amount,
                        })
                    })
                    .collect();
                // Per-stockpile inflow by category: { "Timber": { "Production": 10, "Trade": 5 } }
                let mut inflow_by_stockpile: serde_json::Map<String, serde_json::Value> =
                    serde_json::Map::new();
                for (stock, by_cat) in flow.inflow_by_stockpile_and_category() {
                    let m: serde_json::Map<String, serde_json::Value> = by_cat
                        .into_iter()
                        .map(|(c, v)| (c.label().to_string(), serde_json::json!(v)))
                        .collect();
                    inflow_by_stockpile.insert(stock.label(), serde_json::Value::Object(m));
                }
                let mut outflow_by_stockpile: serde_json::Map<String, serde_json::Value> =
                    serde_json::Map::new();
                for (stock, by_cat) in flow.outflow_by_stockpile_and_category() {
                    let m: serde_json::Map<String, serde_json::Value> = by_cat
                        .into_iter()
                        .map(|(c, v)| (c.label().to_string(), serde_json::json!(v)))
                        .collect();
                    outflow_by_stockpile.insert(stock.label(), serde_json::Value::Object(m));
                }
                serde_json::json!({
                    "inflow": inflow,
                    "outflow": outflow,
                    "inflow_by_stockpile_category": inflow_by_stockpile,
                    "outflow_by_stockpile_category": outflow_by_stockpile,
                })
            } else {
                serde_json::Value::Null
            };

            serde_json::json!({
                "nation_id": nid.0,
                "nation_name": nation_name,
                "nation_color": nation_color,
                "is_human": is_human,
                "economy": {
                    "treasury": treasury_dollars,
                    "provinces": provinces,
                    "buildings": building_count,
                    "goods_revenue": nation.archives.goods_sales_revenue_dollars,
                    "total_resources": total_resources,
                    "total_materials": total_materials,
                    "total_goods": total_goods,
                },
                "cash_flow": cash_flow_json,
                "resource_flow": resource_flow_json,
                "cumulative": {
                    "income_totals": cumulative_income,
                    "expense_totals": cumulative_expense,
                },
                "labor": {
                    "untrained": nation.economy.labor.untrained,
                    "trained": nation.economy.labor.trained,
                    "expert": nation.economy.labor.expert,
                    "total": nation.economy.labor.total_workers(),
                },
                "military": {
                    "total_army_count": total_army_count,
                    "total_army_fp": total_army_fp,
                    "field_army_count": nation.field_army_count(),
                    "militia_count": total_army_count - nation.field_army_count(),
                    "total_warship_count": total_warship_count,
                    "merchant_ships": merchant_ships,
                    "generals_earned": nation.military.generals_earned,
                    "total_arms_built": nation.military.total_arms_built,
                },
                "diplomacy": {
                    "standing": standing,
                    "consulates": consulate_count,
                    "embassies": embassy_count,
                    "alliances": alliance_count,
                    "alliance_names": alliances,
                    "wars": war_count,
                    "war_names": wars,
                },
                "resources_detail": resources_detail,
                "materials_detail": materials_detail,
                "goods_detail": goods_detail,
                "technology": {
                    "researched_count": researched_count,
                    "researched_names": researched_names,
                },
            })
        })
        .collect();

    Ok(serde_json::Value::Array(entries))
}
