//! Fitness test (review F-101): non-ASCII characters appearing in plain
//! string/char literals under `src/` must have a glyph in the bundled
//! pixel UI font. The whole UI renders through that one font, so a missing
//! codepoint means visible tofu on some screen. Scope: a line-based scan of
//! ordinary `"…"`/`'…'` literals — raw/multiline strings and `\u{…}`
//! escapes are not parsed (none carry UI text in this crate today).
//!
//! When this fails: either replace the character with ASCII, or patch a
//! pixel glyph into the font via `assets-src/fonts/patch_glyphs.py` (see
//! `assets-src/fonts/README.md`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Collect non-ASCII characters inside string/char literals on one line,
/// ignoring `//` comments outside of literals. Line-based on purpose:
/// multi-line/raw strings with exotic characters don't occur in this crate,
/// and over-collection would only make the test stricter, not wrong.
fn collect_literal_chars(line: &str, out: &mut BTreeSet<char>) {
    let mut chars = line.chars().peekable();
    let mut in_str = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            } else if !c.is_ascii() {
                out.insert(c);
            }
        } else {
            match c {
                '"' => in_str = true,
                '/' if chars.peek() == Some(&'/') => break,
                // Char literal: 'x'. Lifetimes ('a) have no closing quote
                // right after the next char, so requiring it filters them.
                '\'' => {
                    if let Some(&value) = chars.peek() {
                        let mut lookahead = chars.clone();
                        lookahead.next();
                        if lookahead.peek() == Some(&'\'') && !value.is_ascii() {
                            out.insert(value);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn bundled_font_covers_every_ui_character() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let font_bytes =
        std::fs::read(root.join("assets/fonts/Jersey15-Regular.ttf")).expect("bundled UI font");
    let face = ttf_parser::Face::parse(&font_bytes, 0).expect("parse UI font");

    let mut sources = Vec::new();
    rust_sources(&root.join("src"), &mut sources);
    assert!(!sources.is_empty());

    let mut used = BTreeSet::new();
    for path in sources {
        let src = std::fs::read_to_string(&path).expect("read source file");
        for line in src.lines() {
            collect_literal_chars(line, &mut used);
        }
    }
    assert!(!used.is_empty(), "expected some non-ASCII UI characters");

    let missing: Vec<String> = used
        .iter()
        .filter(|&&c| face.glyph_index(c).is_none())
        .map(|&c| format!("U+{:04X} {c:?}", c as u32))
        .collect();
    assert!(
        missing.is_empty(),
        "UI font is missing glyphs for characters used in string literals: \
         {missing:?} — patch them via assets-src/fonts/patch_glyphs.py or \
         replace them with ASCII"
    );
}
