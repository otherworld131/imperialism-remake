//! Lua game API — exposes game state queries and effect functions to Lua scripts.
//!
//! This module provides the bridge between the Lua scripting engine and
//! the game domain. Functions registered here are callable from Lua scripts.

use mlua::{Lua, Result as LuaResult};

/// Register the game API functions into the Lua state.
///
/// Creates a `game` global table with query and effect functions that
/// scripts can call to interact with the game state.
pub fn register_game_api(lua: &Lua) -> LuaResult<()> {
    let game_table = lua.create_table()?;

    // Logging function — scripts can report messages back to the engine
    let log_fn = lua.create_function(|_, msg: String| {
        // In the future this could push to an event queue
        eprintln!("[lua] {}", msg);
        Ok(())
    })?;
    game_table.set("log", log_fn)?;

    lua.globals().set("game", game_table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_game_api_creates_game_table() {
        let lua = Lua::new();
        register_game_api(&lua).unwrap();

        let result: mlua::Value = lua.globals().get("game").unwrap();
        assert!(matches!(result, mlua::Value::Table(_)));
    }

    #[test]
    fn game_log_is_callable() {
        let lua = Lua::new();
        register_game_api(&lua).unwrap();

        let result: LuaResult<()> = lua.load(r#"game.log("test message")"#).exec();
        assert!(result.is_ok());
    }
}
