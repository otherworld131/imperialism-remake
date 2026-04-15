//! Lua sandboxing — strips dangerous standard library functions.
//!
//! Removes `os`, `io`, `loadfile`, `dofile`, `require`, and `debug` to
//! prevent Lua scripts from accessing the filesystem, network, or OS.

use mlua::Lua;

/// Remove dangerous globals from the Lua state.
///
/// After calling this, Lua scripts cannot:
/// - Read/write files (`io`, `loadfile`, `dofile`)
/// - Execute system commands (`os.execute`, `os.exit`, etc.)
/// - Load external modules (`require`)
/// - Inspect internal state (`debug`)
pub fn sandbox(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();

    // Remove dangerous standard libraries and functions
    globals.set("os", mlua::Value::Nil)?;
    globals.set("io", mlua::Value::Nil)?;
    globals.set("loadfile", mlua::Value::Nil)?;
    globals.set("dofile", mlua::Value::Nil)?;
    globals.set("require", mlua::Value::Nil)?;
    globals.set("debug", mlua::Value::Nil)?;
    globals.set("load", mlua::Value::Nil)?;
    globals.set("collectgarbage", mlua::Value::Nil)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_removes_os() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: mlua::Result<mlua::Value> = lua.globals().get("os");
        assert!(
            matches!(result, Ok(mlua::Value::Nil)),
            "os should be nil after sandboxing"
        );
    }

    #[test]
    fn sandbox_removes_io() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: mlua::Result<mlua::Value> = lua.globals().get("io");
        assert!(matches!(result, Ok(mlua::Value::Nil)));
    }

    #[test]
    fn sandbox_removes_loadfile() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: mlua::Result<mlua::Value> = lua.globals().get("loadfile");
        assert!(matches!(result, Ok(mlua::Value::Nil)));
    }

    #[test]
    fn sandbox_removes_require() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: mlua::Result<mlua::Value> = lua.globals().get("require");
        assert!(matches!(result, Ok(mlua::Value::Nil)));
    }

    #[test]
    fn sandbox_removes_debug() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: mlua::Result<mlua::Value> = lua.globals().get("debug");
        assert!(matches!(result, Ok(mlua::Value::Nil)));
    }

    #[test]
    fn sandbox_allows_math() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        // math should still work
        let result: f64 = lua.load("return math.floor(3.7)").eval().unwrap();
        assert_eq!(result, 3.0);
    }

    #[test]
    fn sandbox_allows_string() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: String = lua.load(r#"return string.upper("hello")"#).eval().unwrap();
        assert_eq!(result, "HELLO");
    }

    #[test]
    fn sandbox_allows_table() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: i64 = lua.load("local t = {1, 2, 3}; return #t").eval().unwrap();
        assert_eq!(result, 3);
    }

    #[test]
    fn sandbox_blocks_os_execute() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: mlua::Result<mlua::Value> = lua.load("return os.execute('ls')").eval();
        assert!(result.is_err(), "os.execute should be blocked");
    }

    #[test]
    fn sandbox_blocks_io_open() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: mlua::Result<mlua::Value> = lua.load("return io.open('/etc/passwd')").eval();
        assert!(result.is_err(), "io.open should be blocked");
    }

    #[test]
    fn sandbox_blocks_require() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: mlua::Result<mlua::Value> = lua.load("return require('os')").eval();
        assert!(result.is_err(), "require should be blocked");
    }

    #[test]
    fn sandbox_blocks_loadfile() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: mlua::Result<mlua::Value> = lua.load("return loadfile('/etc/passwd')").eval();
        assert!(result.is_err(), "loadfile should be blocked");
    }

    #[test]
    fn sandbox_blocks_load() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        let result: mlua::Result<mlua::Value> = lua.load("return load('return 42')()").eval();
        assert!(result.is_err(), "load should be blocked after sandboxing");
        // Also verify `load` is nil
        let is_nil: mlua::Result<mlua::Value> = lua.globals().get("load");
        assert!(
            matches!(is_nil, Ok(mlua::Value::Nil)),
            "load should be nil after sandboxing"
        );
    }

    #[test]
    fn sandbox_blocks_loadstring() {
        let lua = Lua::new();
        sandbox(&lua).unwrap();
        // In Lua 5.4, `loadstring` is unified with `load`. Since we remove
        // `load`, any attempt to call `loadstring` should also fail.
        let result: mlua::Result<mlua::Value> = lua.load("return loadstring('return 42')()").eval();
        assert!(
            result.is_err(),
            "loadstring should be blocked (unified with load in Lua 5.4)"
        );
    }
}
