//! Golden contract tests pinning the JSON output of every `wasm_*` export.
//!
//! Purpose: the wasm-bridge bodies are being extracted into the native
//! `frontend-api` crate. These fixtures, recorded against the pre-refactor
//! code, guarantee the React frontend's JSON contract survives the move
//! byte-for-byte (modulo object key order — comparison is on parsed Values).
//!
//! Regenerate fixtures with:
//!   UPDATE_GOLDEN=1 cargo test -p wasm-bridge --test golden_contract
//!
//! Fixture forms (chosen automatically):
//! - `full`: the parsed output Value, stored verbatim (queries, new games).
//! - `diff`: structural diff of a command's output game state against the
//!   deterministic input state it was applied to — small and reviewable.
//!   Given the same base, equal diffs imply equal outputs.
//! - `process_turn` outputs are only shape-checked (the known turn-processing
//!   non-determinism makes value fixtures flaky).

use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

const GOLDEN_MAP_KEY: &str = "golden-contract";
const GOLDEN_FLAVOR: &str = "golden-flavor";

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("contract")
}

struct Recorder {
    update: bool,
    failures: Vec<String>,
}

impl Recorder {
    fn new() -> Self {
        let update = std::env::var("UPDATE_GOLDEN").is_ok();
        if update {
            fs::create_dir_all(fixture_dir()).expect("create fixture dir");
        }
        Recorder {
            update,
            failures: Vec::new(),
        }
    }

    /// Record/verify a full-output fixture.
    fn full(&mut self, name: &str, raw: &str) -> Value {
        let value = parse_output(raw);
        let fixture = json!({ "kind": "full", "value": value });
        self.check(name, fixture);
        value
    }

    /// Record/verify a command output as a structural diff against the
    /// exact input state the command was applied to.
    fn diff(&mut self, name: &str, base: &Value, raw: &str) -> Value {
        let value = parse_output(raw);
        let fixture = if value.get("error").is_some() {
            // Errors are tiny — pin them verbatim.
            json!({ "kind": "full", "value": value })
        } else if value.get("world").is_some() {
            // A plain game state: diff against the input it was applied to.
            let mut entries = Vec::new();
            diff_values(base, &value, "", &mut entries);
            json!({ "kind": "diff", "entries": entries })
        } else if value.get("game").is_some() {
            // Wrapped response { game, ...extras }: diff the game, pin extras.
            let mut entries = Vec::new();
            diff_values(base, &value["game"], "", &mut entries);
            let mut rest = value.clone();
            rest.as_object_mut().unwrap().remove("game");
            json!({ "kind": "wrapped", "entries": entries, "rest": rest })
        } else {
            json!({ "kind": "full", "value": value })
        };
        self.check(name, fixture);
        value
    }

    fn check(&mut self, name: &str, actual: Value) {
        let path = fixture_dir().join(format!("{name}.json"));
        if self.update {
            // Compact on purpose: fixtures are compared as parsed Values and
            // mismatches are reported as paths, so readability in-file buys
            // little against the ~2× size cost of pretty-printing.
            let compact = serde_json::to_string(&actual).unwrap();
            fs::write(&path, compact).unwrap_or_else(|e| panic!("write {name}: {e}"));
            return;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            self.failures.push(format!(
                "{name}: fixture missing — run `UPDATE_GOLDEN=1 cargo test -p wasm-bridge --test golden_contract`"
            ));
            return;
        };
        let expected: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                self.failures
                    .push(format!("{name}: fixture unreadable: {e}"));
                return;
            }
        };
        if expected != actual {
            let mut diffs = Vec::new();
            diff_values(&expected, &actual, "", &mut diffs);
            let head: Vec<String> = diffs
                .iter()
                .take(5)
                .map(|d| serde_json::to_string(d).unwrap())
                .collect();
            self.failures.push(format!(
                "{name}: output drifted from fixture ({} differing paths). First diffs: {}",
                diffs.len(),
                head.join(", ")
            ));
            if let Ok(dump) = std::env::var("GOLDEN_DUMP") {
                let dump_path = PathBuf::from(dump).join(format!("{name}.actual.json"));
                let _ = fs::write(&dump_path, serde_json::to_string_pretty(&actual).unwrap());
            }
        }
    }

    fn finish(self) {
        if self.update {
            return;
        }
        assert!(
            self.failures.is_empty(),
            "golden contract violations:\n{}",
            self.failures.join("\n")
        );
    }
}

fn parse_output(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// Structural diff: records every path where `b` differs from `a`.
/// Deterministic and exhaustive (no caps — command diffs are small; the
/// fixture file itself is the size guard).
fn diff_values(a: &Value, b: &Value, path: &str, out: &mut Vec<Value>) {
    if a == b {
        return;
    }
    match (a, b) {
        (Value::Object(ao), Value::Object(bo)) => {
            for (k, av) in ao {
                let p = format!("{path}/{k}");
                match bo.get(k) {
                    Some(bv) => diff_values(av, bv, &p, out),
                    None => out.push(json!({ "path": p, "removed": av })),
                }
            }
            for (k, bv) in bo {
                if !ao.contains_key(k) {
                    out.push(json!({ "path": format!("{path}/{k}"), "added": bv }));
                }
            }
        }
        (Value::Array(aa), Value::Array(ba)) => {
            let common = aa.len().min(ba.len());
            for i in 0..common {
                diff_values(&aa[i], &ba[i], &format!("{path}/{i}"), out);
            }
            for (i, av) in aa.iter().enumerate().skip(common) {
                out.push(json!({ "path": format!("{path}/{i}"), "removed": av }));
            }
            for (i, bv) in ba.iter().enumerate().skip(common) {
                out.push(json!({ "path": format!("{path}/{i}"), "added": bv }));
            }
        }
        _ => out.push(json!({ "path": path, "base": a, "new": b })),
    }
}

// ── Value extraction helpers ─────────────────────────────────────────────

fn as_u32(v: &Value) -> u32 {
    v.as_u64()
        .unwrap_or_else(|| panic!("expected number, got {v}")) as u32
}

fn nations(game: &Value) -> &Vec<Value> {
    game["world"]["nations"]
        .as_array()
        .expect("world.nations array")
}

fn is_great_power(nation: &Value) -> bool {
    nation["nation_type"]
        .as_str()
        .map(|s| s.contains("Great"))
        .unwrap_or(false)
}

/// First "id" found in the first element of the named top-level array.
fn first_id(v: &Value, array_key: &str) -> Option<u32> {
    v[array_key].as_array()?.first()?.get("id").map(as_u32)
}

#[test]
fn golden_contract() {
    let mut rec = Recorder::new();

    // ── Determinism guard: identical inputs must give identical games. ──
    let new_golden_game = || {
        wasm_bridge::wasm_new_game(
            GOLDEN_MAP_KEY,
            2, // Normal
            0,
            40,
            30,
            4,
            3,
            "",
            "",
            false,
            0,
            0,
        )
    };
    let g0_raw = new_golden_game();
    assert_eq!(
        g0_raw,
        new_golden_game(),
        "wasm_new_game is not deterministic for a fixed map key — all fixtures would be flaky"
    );
    let g0 = rec.full("new_game", &g0_raw);
    assert!(
        g0.get("error").is_none(),
        "golden new_game returned an error: {g0}"
    );

    // ── Derived ids ──────────────────────────────────────────────────────
    let human = as_u32(&g0["human_player_nation"]);
    let human_nation = nations(&g0)
        .iter()
        .find(|n| as_u32(&n["id"]) == human)
        .expect("human nation present");
    let capital_prov = as_u32(&human_nation["capital_province_id"]);
    let gp2 = nations(&g0)
        .iter()
        .find(|n| is_great_power(n) && as_u32(&n["id"]) != human)
        .map(|n| as_u32(&n["id"]))
        .expect("a second great power");
    let minor = nations(&g0)
        .iter()
        .find(|n| !is_great_power(n))
        .expect("a minor nation");
    let minor_id = as_u32(&minor["id"]);
    let minor_capital = as_u32(&minor["capital_province_id"]);

    // ── Standalone exports ───────────────────────────────────────────────
    rec.full("debug_marker", &wasm_bridge::wasm_debug_marker());
    rec.full(
        "max_workers_supportable",
        &wasm_bridge::wasm_max_workers_supportable(100, 50, 30).to_string(),
    );
    let scenarios = rec.full("get_scenarios", &wasm_bridge::wasm_get_scenarios());

    // ── Map / overlay queries ────────────────────────────────────────────
    rec.full(
        "get_map_data_fog",
        &wasm_bridge::wasm_get_map_data(&g0_raw, false),
    );
    let map_data = rec.full(
        "get_map_data_nofog",
        &wasm_bridge::wasm_get_map_data(&g0_raw, true),
    );
    rec.full(
        "get_navy_markers",
        &wasm_bridge::wasm_get_navy_markers(&g0_raw, false),
    );
    let sea_zones = rec.full("get_sea_zones", &wasm_bridge::wasm_get_sea_zones(&g0_raw));
    rec.full(
        "get_diplomacy_overlay",
        &wasm_bridge::wasm_get_diplomacy_overlay(&g0_raw, human),
    );
    rec.full(
        "get_military_overlay",
        &wasm_bridge::wasm_get_military_overlay(&g0_raw),
    );
    rec.full(
        "get_political_snapshot",
        &wasm_bridge::wasm_get_political_snapshot(&g0_raw, 1),
    );

    // ── Entity queries ───────────────────────────────────────────────────
    let units = rec.full(
        "get_units_in_province",
        &wasm_bridge::wasm_get_units_in_province(&g0_raw, capital_prov),
    );
    let civilians = rec.full(
        "get_civilians",
        &wasm_bridge::wasm_get_civilians(&g0_raw, human),
    );
    rec.full("get_ships", &wasm_bridge::wasm_get_ships(&g0_raw, human));
    rec.full(
        "get_buildable_units",
        &wasm_bridge::wasm_get_buildable_units(&g0_raw, human),
    );

    // ── Screen queries ───────────────────────────────────────────────────
    rec.full(
        "get_transport_data",
        &wasm_bridge::wasm_get_transport_data(&g0_raw, human),
    );
    rec.full(
        "get_industry_data",
        &wasm_bridge::wasm_get_industry_data(&g0_raw, human),
    );
    rec.full(
        "get_trade_data",
        &wasm_bridge::wasm_get_trade_data(&g0_raw, human),
    );
    rec.full(
        "get_diplomacy_screen_data",
        &wasm_bridge::wasm_get_diplomacy_screen_data(&g0_raw, human),
    );
    rec.full(
        "get_pending_proposals",
        &wasm_bridge::wasm_get_pending_proposals(&g0_raw, human),
    );
    let techs = rec.full(
        "get_available_techs",
        &wasm_bridge::wasm_get_available_techs(&g0_raw),
    );
    rec.full(
        "get_tech_screen_data",
        &wasm_bridge::wasm_get_tech_screen_data(&g0_raw),
    );
    rec.full(
        "get_ledger_data",
        &wasm_bridge::wasm_get_ledger_data(&g0_raw, human),
    );
    rec.full(
        "get_all_gp_ledger_data",
        &wasm_bridge::wasm_get_all_gp_ledger_data(&g0_raw),
    );
    rec.full(
        "get_newspaper_archive",
        &wasm_bridge::wasm_get_newspaper_archive(&g0_raw),
    );
    rec.full(
        "get_newspaper_archive_since",
        &wasm_bridge::wasm_get_newspaper_archive_since(&g0_raw, 0),
    );
    rec.full(
        "get_battle_data",
        &wasm_bridge::wasm_get_battle_data(&g0_raw),
    );

    // ── Lifecycle variants ───────────────────────────────────────────────
    let scenario_id = scenarios
        .as_array()
        .or_else(|| scenarios["scenarios"].as_array())
        .and_then(|a| a.first())
        .and_then(|s| s.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(sid) = &scenario_id {
        rec.full(
            "new_scenario_game",
            &wasm_bridge::wasm_new_scenario_game(sid, 2, 0, "", false, 0, 0),
        );
        rec.full(
            "new_observer_scenario_game",
            &wasm_bridge::wasm_new_observer_scenario_game(sid, 2, ""),
        );
    }
    let obs_raw = wasm_bridge::wasm_new_observer_game(GOLDEN_MAP_KEY, 2, 40, 30, 4, 3, "", "");
    let obs = rec.full("new_observer_game", &obs_raw);
    rec.diff(
        "set_human_player",
        &obs,
        &wasm_bridge::wasm_set_human_player(&obs_raw, 1),
    );
    rec.diff(
        "apply_flavor",
        &g0,
        &wasm_bridge::wasm_apply_flavor(&g0_raw, GOLDEN_FLAVOR),
    );

    // ── Turn processing: shape-only (known non-determinism) ─────────────
    let turn1 = parse_output(&wasm_bridge::wasm_process_turn(&g0_raw));
    assert!(
        turn1.get("error").is_none()
            && turn1["game"].get("world").is_some()
            && turn1.get("report").is_some(),
        "process_turn must return {{game, report}}"
    );
    let turn2 = parse_output(&wasm_bridge::wasm_process_turns(&g0_raw, 2));
    assert!(
        turn2.get("error").is_none()
            && turn2["game"].get("world").is_some()
            && turn2["reports"].as_array().map(|r| r.len()) == Some(2),
        "process_turns must return {{game, reports[2]}}"
    );

    // ── Tech commands ────────────────────────────────────────────────────
    let tech_name = techs
        .as_array()
        .or_else(|| techs["techs"].as_array())
        .and_then(|a| a.first())
        .and_then(|t| t.get("name"))
        .and_then(|v| v.as_str())
        .expect("an available tech")
        .to_string();
    rec.diff(
        "research_tech",
        &g0,
        &wasm_bridge::wasm_research_tech(&g0_raw, &tech_name),
    );
    let gq_raw = wasm_bridge::wasm_queue_tech_research(&g0_raw, &tech_name);
    let gq = rec.diff("queue_tech_research", &g0, &gq_raw);
    rec.diff(
        "cancel_tech_research",
        &gq,
        &wasm_bridge::wasm_cancel_tech_research(&gq_raw),
    );

    // ── Unit commands ────────────────────────────────────────────────────
    let unit_id = first_id(&units, "army_units").expect("a unit in the capital");
    let move_targets = rec.full(
        "get_valid_move_targets",
        &wasm_bridge::wasm_get_valid_move_targets(&g0_raw, human, unit_id),
    );
    rec.full(
        "get_upgrade_info",
        &wasm_bridge::wasm_get_upgrade_info(&g0_raw, human, unit_id),
    );
    let dest = move_targets["friendly"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .map(|v| as_u32(&v["province_id"]))
                .find(|&p| p != capital_prov)
        })
        .expect("a friendly move target outside the capital province");
    let gm_raw = wasm_bridge::wasm_queue_unit_move(&g0_raw, human, unit_id, dest);
    let gm = rec.diff("queue_unit_move", &g0, &gm_raw);
    assert!(gm.get("error").is_none(), "queue_unit_move failed: {gm}");
    rec.diff(
        "cancel_unit_move",
        &gm,
        &wasm_bridge::wasm_cancel_unit_move(&gm_raw, unit_id),
    );
    rec.diff(
        "disband_unit",
        &g0,
        &wasm_bridge::wasm_disband_unit(&g0_raw, unit_id),
    );
    rec.diff(
        "upgrade_unit",
        &g0,
        &wasm_bridge::wasm_upgrade_unit(&g0_raw, human, unit_id),
    );
    rec.diff(
        "upgrade_units",
        &g0,
        &wasm_bridge::wasm_upgrade_units(&g0_raw, human, &format!("[{unit_id}]")),
    );

    // ── Civilian commands ────────────────────────────────────────────────
    let mut civ_list = civilians["undeployed"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    civ_list.extend(
        civilians["deployed"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    );
    let civ_of = |kind: &str| -> Option<u32> {
        civ_list.iter().find_map(|c| {
            let ty = c.get("civilian_type").or(c.get("type"))?.as_str()?;
            if !ty.contains(kind) {
                return None;
            }
            c.get("id").map(as_u32)
        })
    };
    // A tile the farmer can plausibly improve: own land with a Grain resource.
    let farm_tile = map_data["tiles"].as_array().and_then(|tiles| {
        tiles.iter().find(|t| {
            t.get("owner").map(as_u32) == Some(human)
                && t.get("resource")
                    .and_then(|r| {
                        r.as_str().map(|s| s.contains("Grain")).or_else(|| {
                            r.get("resource_type")
                                .and_then(|v| v.as_str())
                                .map(|s| s.contains("Grain"))
                        })
                    })
                    .unwrap_or(false)
        })
    });
    if let (Some(farmer), Some(tile)) = (civ_of("Farmer"), farm_tile) {
        let q = tile["q"].as_i64().expect("tile q") as i32;
        let r = tile["r"].as_i64().expect("tile r") as i32;
        let gd_raw = wasm_bridge::wasm_deploy_civilian(&g0_raw, farmer, q, r);
        let gd = rec.diff("deploy_civilian", &g0, &gd_raw);
        rec.diff(
            "recall_civilian",
            &gd,
            &wasm_bridge::wasm_recall_civilian(&gd_raw, farmer),
        );
    }
    if let (Some(engineer), Some(tile)) = (civ_of("Engineer"), farm_tile) {
        let q = tile["q"].as_i64().expect("tile q") as i32;
        let r = tile["r"].as_i64().expect("tile r") as i32;
        let ge_raw = wasm_bridge::wasm_deploy_civilian(&g0_raw, engineer, q, r);
        let ge = rec.diff("deploy_civilian_engineer", &g0, &ge_raw);
        rec.diff(
            "engineer_build",
            &ge,
            &wasm_bridge::wasm_engineer_build(&ge_raw, engineer, "railroad"),
        );
    }

    // ── Recruitment / industry pending commands ──────────────────────────
    rec.diff(
        "recruit_army_unit",
        &g0,
        &wasm_bridge::wasm_recruit_army_unit(&g0_raw, human, "Regulars"),
    );
    rec.diff(
        "set_pending_army_recruits",
        &g0,
        &wasm_bridge::wasm_set_pending_army_recruits(&g0_raw, human, "Regulars", 2),
    );
    rec.diff(
        "set_pending_civilian_hire",
        &g0,
        &wasm_bridge::wasm_set_pending_civilian_hire(&g0_raw, human, "Farmer", 1),
    );
    rec.diff(
        "set_pending_training",
        &g0,
        &wasm_bridge::wasm_set_pending_training(&g0_raw, human, 1, 0),
    );
    rec.diff(
        "set_pending_immigration",
        &g0,
        &wasm_bridge::wasm_set_pending_immigration(&g0_raw, human, 1),
    );
    let gs_raw = wasm_bridge::wasm_build_ship(&g0_raw, human, "Trader");
    let gs = rec.diff("build_ship", &g0, &gs_raw);
    rec.diff(
        "cancel_ship_build",
        &gs,
        &wasm_bridge::wasm_cancel_ship_build(&gs_raw, human, "Trader"),
    );
    rec.diff(
        "set_pending_ships",
        &g0,
        &wasm_bridge::wasm_set_pending_ships(&g0_raw, human, "Trader", 1),
    );

    // ── Transport / industry / trade commands ────────────────────────────
    rec.diff(
        "set_pending_freight_cars",
        &g0,
        &wasm_bridge::wasm_set_pending_freight_cars(&g0_raw, human, 2),
    );
    rec.diff(
        "set_transport_allocation",
        &g0,
        &wasm_bridge::wasm_set_transport_allocation(&g0_raw, human, "Grain", 1),
    );
    rec.diff(
        "set_chain_target",
        &g0,
        &wasm_bridge::wasm_set_chain_target(&g0_raw, human, "timber", "mill", 5),
    );
    rec.diff(
        "expand_building",
        &g0,
        &wasm_bridge::wasm_expand_building(&g0_raw, human, "LumberMill"),
    );
    rec.diff(
        "set_auto_trade_with_minors",
        &g0,
        &wasm_bridge::wasm_set_auto_trade_with_minors(&g0_raw, human, true),
    );
    rec.diff(
        "set_trade_subsidy",
        &g0,
        &wasm_bridge::wasm_set_trade_subsidy(&g0_raw, human, minor_id, 500),
    );
    rec.diff(
        "set_player_sell_order",
        &g0,
        &wasm_bridge::wasm_set_player_sell_order(&g0_raw, human, "resource", "Grain", 1),
    );
    rec.diff(
        "set_buy_wishlist",
        &g0,
        &wasm_bridge::wasm_set_buy_wishlist(&g0_raw, human, "Coal", true),
    );

    // ── Naval commands ───────────────────────────────────────────────────
    let ships = parse_output(&wasm_bridge::wasm_get_ships(&g0_raw, human));
    let warship_zone = ships["warships"]
        .as_array()
        .and_then(|a| a.first())
        .map(|s| as_u32(&s["sea_zone"]))
        .expect("a starting warship");
    let adjacent_zone = sea_zones
        .as_array()
        .and_then(|zones| zones.iter().find(|z| as_u32(&z["id"]) == warship_zone))
        .and_then(|z| z["adjacent_zone_ids"].as_array())
        .and_then(|a| a.first())
        .map(as_u32)
        .expect("an adjacent sea zone");
    let gf_raw = wasm_bridge::wasm_move_fleet(&g0_raw, human, warship_zone, adjacent_zone);
    let gf = rec.diff("move_fleet", &g0, &gf_raw);
    assert!(gf.get("error").is_none(), "move_fleet failed: {gf}");
    rec.diff(
        "cancel_fleet_move",
        &gf,
        &wasm_bridge::wasm_cancel_fleet_move(&gf_raw, human, warship_zone),
    );
    rec.diff(
        "assign_beachhead",
        &g0,
        &wasm_bridge::wasm_assign_beachhead(&g0_raw, human, minor_capital),
    );

    // ── Diplomacy commands ───────────────────────────────────────────────
    rec.diff(
        "diplomacy_build_consulate",
        &g0,
        &wasm_bridge::wasm_diplomacy_build_consulate(&g0_raw, human, minor_id),
    );
    rec.diff(
        "diplomacy_build_embassy",
        &g0,
        &wasm_bridge::wasm_diplomacy_build_embassy(&g0_raw, human, minor_id),
    );
    let gn_raw = wasm_bridge::wasm_diplomacy_propose_nap(&g0_raw, human, gp2);
    let gn = rec.diff("diplomacy_propose_nap", &g0, &gn_raw);
    rec.diff(
        "diplomacy_dismiss_outgoing_proposal",
        &gn,
        &wasm_bridge::wasm_diplomacy_dismiss_outgoing_proposal(&gn_raw, human, gp2),
    );
    rec.diff(
        "diplomacy_propose_alliance",
        &g0,
        &wasm_bridge::wasm_diplomacy_propose_alliance(&g0_raw, human, gp2),
    );
    rec.diff(
        "diplomacy_declare_war",
        &g0,
        &wasm_bridge::wasm_diplomacy_declare_war(&g0_raw, human, gp2),
    );
    rec.diff(
        "diplomacy_send_grant",
        &g0,
        &wasm_bridge::wasm_diplomacy_send_grant(&g0_raw, human, gp2, 500),
    );
    rec.diff(
        "diplomacy_break_treaty",
        &g0,
        &wasm_bridge::wasm_diplomacy_break_treaty(&g0_raw, human, gp2, "NonAggressionPact"),
    );
    rec.diff(
        "diplomacy_propose_peace",
        &g0,
        &wasm_bridge::wasm_diplomacy_propose_peace(&g0_raw, human, gp2),
    );
    rec.diff(
        "diplomacy_dismiss_pending_action",
        &g0,
        &wasm_bridge::wasm_diplomacy_dismiss_pending_action(&g0_raw, human, gp2, "war"),
    );
    rec.diff(
        "accept_proposal",
        &g0,
        &wasm_bridge::wasm_accept_proposal(&g0_raw, human, 0),
    );
    rec.diff(
        "reject_proposal",
        &g0,
        &wasm_bridge::wasm_reject_proposal(&g0_raw, human, 0),
    );

    rec.finish();
}
