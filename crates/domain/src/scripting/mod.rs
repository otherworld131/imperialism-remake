//! Lua scripting engine for moddable game logic.
//!
//! Provides a sandboxed Lua VM that can run tech effect scripts,
//! AI behavior scripts, and event hook callbacks. The sandbox strips
//! dangerous functions (os, io, require, etc.) so mods cannot access
//! the filesystem or network.

pub mod game_api;
pub mod sandbox;

use mlua::Lua;

/// The Lua scripting engine. Wraps a sandboxed Lua VM with game API bindings.
pub struct LuaEngine {
    lua: Lua,
    /// Registered event hook names → Lua function references.
    hooks: Vec<(String, mlua::RegistryKey)>,
}

impl LuaEngine {
    /// Create a new sandboxed Lua engine with game API registered.
    pub fn new() -> Result<Self, String> {
        let lua = Lua::new();
        sandbox::sandbox(&lua).map_err(|e| format!("Sandbox setup failed: {}", e))?;
        game_api::register_game_api(&lua)
            .map_err(|e| format!("Game API registration failed: {}", e))?;
        Ok(LuaEngine {
            lua,
            hooks: Vec::new(),
        })
    }

    /// Execute a Lua script string in the sandboxed environment.
    pub fn exec(&self, script: &str) -> Result<(), String> {
        self.lua
            .load(script)
            .exec()
            .map_err(|e| format!("Lua execution error: {}", e))
    }

    /// Evaluate a Lua expression and return the result as a string.
    pub fn eval_string(&self, expr: &str) -> Result<String, String> {
        self.lua
            .load(expr)
            .eval::<String>()
            .map_err(|e| format!("Lua eval error: {}", e))
    }

    /// Register an event hook. The script should define a function with the given name.
    /// When `fire_hook` is called with that name, the function will be invoked.
    pub fn register_hook(&mut self, hook_name: &str, script: &str) -> Result<(), String> {
        // Execute the script to define the function
        self.exec(script)?;

        // Get the function from globals
        let func: mlua::Function =
            self.lua.globals().get(hook_name).map_err(|e| {
                format!("Hook '{}' not found after loading script: {}", hook_name, e)
            })?;

        // Store a registry key for the function
        let key = self
            .lua
            .create_registry_value(func)
            .map_err(|e| format!("Failed to register hook '{}': {}", hook_name, e))?;

        self.hooks.push((hook_name.to_string(), key));
        Ok(())
    }

    /// Fire all registered hooks with the given name.
    /// Returns a list of string results (one per matching hook).
    pub fn fire_hook(&self, hook_name: &str) -> Vec<Result<String, String>> {
        let mut results = Vec::new();
        for (name, key) in &self.hooks {
            if name == hook_name {
                let func: mlua::Function = match self.lua.registry_value(key) {
                    Ok(f) => f,
                    Err(e) => {
                        results.push(Err(format!("Failed to retrieve hook '{}': {}", name, e)));
                        continue;
                    }
                };
                match func.call::<String>(()) {
                    Ok(s) => results.push(Ok(s)),
                    Err(e) => results.push(Err(format!("Hook '{}' error: {}", name, e))),
                }
            }
        }
        results
    }

    /// Get a reference to the underlying Lua state for advanced usage.
    pub fn lua(&self) -> &Lua {
        &self.lua
    }
}

impl Default for LuaEngine {
    fn default() -> Self {
        Self::new().expect("Failed to create default LuaEngine")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_lua_engine() {
        let engine = LuaEngine::new().unwrap();
        assert!(engine.hooks.is_empty());
    }

    #[test]
    fn exec_simple_script() {
        let engine = LuaEngine::new().unwrap();
        engine.exec("local x = 1 + 2").unwrap();
    }

    #[test]
    fn exec_invalid_script_returns_error() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.exec("this is not valid lua +++");
        assert!(result.is_err());
    }

    #[test]
    fn eval_string_expression() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.eval_string(r#"return "hello world""#).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn sandbox_is_applied() {
        let engine = LuaEngine::new().unwrap();
        let result = engine.exec("os.execute('ls')");
        assert!(result.is_err(), "os should be sandboxed");
    }

    #[test]
    fn game_api_is_available() {
        let engine = LuaEngine::new().unwrap();
        engine.exec(r#"game.log("test from engine")"#).unwrap();
    }

    #[test]
    fn register_and_fire_hook() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .register_hook(
                "on_turn_start",
                r#"function on_turn_start() return "turn started" end"#,
            )
            .unwrap();

        let results = engine.fire_hook("on_turn_start");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_ref().unwrap(), "turn started");
    }

    #[test]
    fn fire_hook_with_no_registered_hooks() {
        let engine = LuaEngine::new().unwrap();
        let results = engine.fire_hook("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn register_multiple_hooks_same_name() {
        let mut engine = LuaEngine::new().unwrap();
        engine
            .register_hook(
                "on_turn_end",
                r#"function on_turn_end() return "hook1" end"#,
            )
            .unwrap();
        // Register another with the same name (overwrites the global but both registry keys exist)
        engine
            .register_hook(
                "on_turn_end",
                r#"function on_turn_end() return "hook2" end"#,
            )
            .unwrap();

        let results = engine.fire_hook("on_turn_end");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn default_engine_works() {
        let engine = LuaEngine::default();
        engine.exec("local x = 42").unwrap();
    }
}
