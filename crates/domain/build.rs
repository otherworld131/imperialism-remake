//! Build-time Lua → JSON codegen.
//!
//! Runs natively at build time (host toolchain), spins up an mlua VM, loads
//! every script under `scripts/`, and writes the relevant globals as JSON
//! to `$OUT_DIR/lua_baked.json`. The WASM build of `lua_bridge` (where the
//! `lua` feature is OFF) embeds that JSON via `include_bytes!` and
//! deserializes it at startup, so the browser sees identical AI tunables
//! to the CLI without needing to embed a Lua VM.

use mlua::{Lua, Table, Value};
use std::path::PathBuf;

fn main() {
    // Re-run the build script if any of our Lua sources change.
    println!("cargo:rerun-if-changed=../../scripts/config/game.lua");
    println!("cargo:rerun-if-changed=../../scripts/config/units.lua");
    println!("cargo:rerun-if-changed=../../scripts/config/ships.lua");
    println!("cargo:rerun-if-changed=../../scripts/config/tech_tree.lua");
    println!("cargo:rerun-if-changed=../../scripts/ai/balanced.lua");
    println!("cargo:rerun-if-changed=../../scripts/ai/aggressive.lua");
    println!("cargo:rerun-if-changed=../../scripts/ai/diplomatic.lua");
    println!("cargo:rerun-if-changed=../../scripts/ai/economic.lua");
    println!("cargo:rerun-if-changed=build.rs");

    let scripts = [
        ("game_config", include_str!("../../scripts/config/game.lua")),
        ("balanced", include_str!("../../scripts/ai/balanced.lua")),
        (
            "aggressive",
            include_str!("../../scripts/ai/aggressive.lua"),
        ),
        (
            "diplomatic",
            include_str!("../../scripts/ai/diplomatic.lua"),
        ),
        ("economic", include_str!("../../scripts/ai/economic.lua")),
    ];

    let lua = Lua::new();
    for (name, src) in &scripts {
        if let Err(e) = lua.load(*src).exec() {
            panic!("[domain build.rs] failed to load {}: {}", name, e);
        }
    }

    // Build the JSON output.
    let game_config = lua_table_to_json(
        &lua.globals()
            .get::<Table>("game_config")
            .expect("game_config table missing"),
    );

    let mut personality_configs = serde_json::Map::new();
    for (lua_name, rust_name) in &[
        ("aggressive", "Aggressive"),
        ("balanced", "Balanced"),
        ("diplomatic", "Diplomatic"),
        ("economic", "Economic"),
    ] {
        let table: Table = lua
            .globals()
            .get(*lua_name)
            .unwrap_or_else(|_| panic!("personality table {} missing", lua_name));
        personality_configs.insert(rust_name.to_string(), lua_table_to_json(&table));
    }

    let document = serde_json::json!({
        "game_config": game_config,
        "personality_configs": personality_configs,
    });

    let out_dir = std::env::var_os("OUT_DIR").expect("OUT_DIR not set by cargo");
    let out_path: PathBuf = PathBuf::from(out_dir).join("lua_baked.json");
    let serialized =
        serde_json::to_string(&document).expect("serializing baked Lua data should not fail");
    std::fs::write(&out_path, serialized)
        .unwrap_or_else(|e| panic!("writing {}: {}", out_path.display(), e));
}

/// Convert a Lua value to a `serde_json::Value`. Functions, userdata, and
/// other non-data Lua values become JSON null (we don't bake those).
fn lua_value_to_json(value: Value) -> serde_json::Value {
    match value {
        Value::Nil => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::Value::Bool(b),
        Value::Integer(i) => serde_json::Value::Number(i.into()),
        Value::Number(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.to_str().unwrap().to_string()),
        Value::Table(t) => lua_table_to_json(&t),
        // Functions, threads, userdata, light userdata: not serializable
        // to JSON. We emit null and let the consumer treat it as missing.
        _ => serde_json::Value::Null,
    }
}

/// Convert a Lua table to a JSON object (or array if numeric-keyed).
fn lua_table_to_json(table: &Table) -> serde_json::Value {
    // Detect arrays: keys are 1..=n consecutive integers.
    let len = table.raw_len();
    let mut is_array = len > 0;
    if is_array {
        for i in 1..=len {
            let v: Value = match table.raw_get::<Value>(i) {
                Ok(v) => v,
                Err(_) => {
                    is_array = false;
                    break;
                }
            };
            if matches!(v, Value::Nil) {
                is_array = false;
                break;
            }
        }
    }

    if is_array {
        let mut out = Vec::with_capacity(len as usize);
        for i in 1..=len {
            let v: Value = table.raw_get(i).unwrap_or(Value::Nil);
            out.push(lua_value_to_json(v));
        }
        return serde_json::Value::Array(out);
    }

    // Object: iterate string keys.
    let mut out = serde_json::Map::new();
    for pair in table.clone().pairs::<Value, Value>() {
        let (k, v) = match pair {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        let key = match k {
            Value::String(s) => s.to_str().unwrap().to_string(),
            // Skip non-string keys — they wouldn't deserialize into Rust
            // structs anyway, and the Lua scripts don't use them at this layer.
            _ => continue,
        };
        // Functions in personality tables (e.g. ai_evaluate_war) are not
        // baked — they're being ported to Rust. Skip silently.
        if matches!(v, Value::Function(_)) {
            continue;
        }
        out.insert(key, lua_value_to_json(v));
    }
    serde_json::Value::Object(out)
}
