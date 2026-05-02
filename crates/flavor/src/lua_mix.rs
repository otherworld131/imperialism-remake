//! Loads the procedural government mix from a Lua-syntax data file.
//!
//! No `mlua` dependency: the file uses a tiny subset of Lua
//! (`identifier = number,` inside a top-level `table_name = { ... }`),
//! so a hand-rolled parser is enough and keeps the WASM bundle slim.
//! When parsing fails the loader returns the hardcoded defaults so
//! flavor stays usable even with a malformed config.

use crate::government::{GovernmentForm, GovernmentMix};

const DEFAULT_MIX_LUA: &str = include_str!("../scripts/government_mix.lua");

/// Load the GP and minor-nation mixes from the bundled Lua data file.
/// Falls back to `GovernmentMix::*_default()` if either table is missing.
pub fn load_default_mixes() -> (GovernmentMix, GovernmentMix) {
    load_mixes_from(DEFAULT_MIX_LUA)
}

/// Parse a Lua-syntax mix file and return `(great_power_mix, minor_nation_mix)`.
/// Each table missing from the source falls back to its hardcoded default.
pub fn load_mixes_from(source: &str) -> (GovernmentMix, GovernmentMix) {
    let gp =
        parse_table(source, "great_power_mix").unwrap_or_else(GovernmentMix::great_power_default);
    let mn =
        parse_table(source, "minor_nation_mix").unwrap_or_else(GovernmentMix::minor_nation_default);
    (gp, mn)
}

/// Parse a single top-level table `name = { Form = weight, ... }`.
/// Returns `None` if the table is missing or empty.
fn parse_table(source: &str, name: &str) -> Option<GovernmentMix> {
    let stripped = strip_line_comments(source);
    let after = find_table_body(&stripped, name)?;
    let body = take_braced(after)?;
    let mut mix = GovernmentMix::new();
    let mut any = false;
    for entry in body.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (key, value) = entry.split_once('=')?;
        let key = key.trim();
        let value = value.trim();
        let form = GovernmentForm::parse(key)?;
        let weight: f32 = value.parse().ok()?;
        mix = mix.add(form, weight);
        any = true;
    }
    if any { Some(mix) } else { None }
}

/// Drop everything from `--` to end-of-line on each line.
fn strip_line_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let line = match line.find("--") {
            Some(i) => &line[..i],
            None => line,
        };
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Locate `name = {` and return the slice starting just after the `{`.
fn find_table_body<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let mut search_from = 0;
    while let Some(idx) = source[search_from..].find(name) {
        let abs = search_from + idx;
        // Make sure this is a standalone identifier, not a substring.
        let before_ok = abs == 0
            || !source.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && source.as_bytes()[abs - 1] != b'_';
        let after_ok = source[abs + name.len()..]
            .chars()
            .next()
            .map(|c| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(true);
        if before_ok && after_ok {
            let rest = &source[abs + name.len()..];
            let trimmed = rest.trim_start();
            if let Some(after_eq) = trimmed.strip_prefix('=') {
                let after_eq = after_eq.trim_start();
                if let Some(after_brace) = after_eq.strip_prefix('{') {
                    return Some(after_brace);
                }
            }
        }
        search_from = abs + name.len();
    }
    None
}

/// Take everything up to the matching `}`. Nested braces are counted so a
/// `{ x = 1 }` inside the table doesn't truncate early. The current grammar
/// has no nesting but the helper is forgiving anyway.
fn take_braced(after_open: &str) -> Option<&str> {
    let mut depth: usize = 1;
    for (i, ch) in after_open.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&after_open[..i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_bundled_default_file() {
        let (gp, mn) = load_default_mixes();
        // Both tables should produce non-empty, positive-total mixes.
        let gp_total: f32 = gp.weights.iter().map(|(_, w)| *w).sum();
        let mn_total: f32 = mn.weights.iter().map(|(_, w)| *w).sum();
        assert!(gp_total > 0.0, "great_power_mix parsed empty");
        assert!(mn_total > 0.0, "minor_nation_mix parsed empty");
    }

    #[test]
    fn ignores_line_comments_and_trailing_commas() {
        let src = r#"
            -- a comment line
            great_power_mix = {
                Empire = 1.0, -- inline comment
                Kingdom = 2,
            }
        "#;
        let (gp, _) = load_mixes_from(src);
        assert_eq!(gp.weights.len(), 2);
    }

    #[test]
    fn missing_table_falls_back_to_default() {
        let src = "great_power_mix = { Empire = 1 }";
        let (_gp, mn) = load_mixes_from(src);
        // The default minor mix is non-empty; falling back means we still get one.
        assert!(!mn.weights.is_empty());
    }

    #[test]
    fn unknown_form_invalidates_mix() {
        // An unknown identifier short-circuits the whole table; we expect the
        // fallback (non-empty default) so callers aren't left with an empty
        // distribution.
        let src = "great_power_mix = { NotAForm = 5 }";
        let (gp, _) = load_mixes_from(src);
        assert!(!gp.weights.is_empty());
    }
}
