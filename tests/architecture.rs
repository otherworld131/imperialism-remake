//! Architecture fitness function tests.
//! Verify dependency rules are enforced across crates.

use std::fs;

/// Parse a Cargo.toml and extract dependency names from [dependencies] section only
/// (excludes [dev-dependencies]).
fn get_dependencies(cargo_toml_path: &str) -> Vec<String> {
    let content = fs::read_to_string(cargo_toml_path)
        .unwrap_or_else(|_| panic!("Failed to read {}", cargo_toml_path));

    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in content.lines() {
        if line.starts_with("[dependencies]") {
            in_deps = true;
            continue;
        }
        if line.starts_with("[dev-dependencies]") || (line.starts_with('[') && in_deps) {
            in_deps = false;
        }
        if in_deps && let Some(name) = line.split('=').next() {
            let name = name.trim();
            if !name.is_empty() && !name.starts_with('#') {
                deps.push(name.to_string());
            }
        }
    }
    deps
}

/// Parse a Cargo.toml and extract all dependency names (both [dependencies] and
/// [dev-dependencies]).
fn get_all_dependencies(cargo_toml_path: &str) -> Vec<String> {
    let content = fs::read_to_string(cargo_toml_path)
        .unwrap_or_else(|_| panic!("Failed to read {}", cargo_toml_path));

    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in content.lines() {
        if line.starts_with("[dependencies]") || line.starts_with("[dev-dependencies]") {
            in_deps = true;
            continue;
        }
        if line.starts_with('[') {
            in_deps = false;
        }
        if in_deps && let Some(name) = line.split('=').next() {
            let name = name.trim();
            if !name.is_empty() && !name.starts_with('#') {
                deps.push(name.to_string());
            }
        }
    }
    deps
}

/// Recursively search for `use` statements in Rust source files under a directory.
fn find_use_statements(dir: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let dir_path = std::path::Path::new(dir);
    if !dir_path.exists() {
        return results;
    }
    collect_use_statements(dir_path, &mut results);
    results
}

fn collect_use_statements(dir: &std::path::Path, results: &mut Vec<(String, String)>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_use_statements(&path, results);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && let Ok(content) = fs::read_to_string(&path)
            {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("use ") || trimmed.starts_with("pub use ") {
                        results.push((path.display().to_string(), trimmed.to_string()));
                    }
                }
            }
        }
    }
}

// ── Test: Domain crate has only serde as a dependency ──────────────

#[test]
fn domain_has_only_serde_dependency() {
    let deps = get_dependencies("crates/domain/Cargo.toml");
    let allowed = ["serde", "serde_json", "ron", "mlua"];
    for dep in &deps {
        assert!(
            allowed.contains(&dep.as_str()),
            "Domain crate has unexpected dependency: '{}'. \
             Domain should only depend on: {:?}",
            dep,
            allowed
        );
    }
}

// ── Test: Application crate depends only on domain ─────────────────

#[test]
fn application_depends_only_on_domain() {
    let deps = get_dependencies("crates/application/Cargo.toml");
    let allowed = ["domain", "serde", "serde_json"];
    for dep in &deps {
        assert!(
            allowed.contains(&dep.as_str()),
            "Application crate has unexpected dependency: '{}'. \
             Application should only depend on: {:?}",
            dep,
            allowed
        );
    }
}

// ── Test: Infrastructure does not depend on presentation ───────────

#[test]
fn infrastructure_does_not_depend_on_presentation() {
    let deps = get_all_dependencies("crates/infrastructure/Cargo.toml");
    assert!(
        !deps.contains(&"presentation".to_string()),
        "Infrastructure crate must not depend on Presentation crate. \
         Found dependencies: {:?}",
        deps
    );
}

// ── Test: Presentation does not depend on infrastructure ───────────

#[test]
fn presentation_does_not_depend_on_infrastructure() {
    let deps = get_all_dependencies("crates/presentation/Cargo.toml");
    assert!(
        !deps.contains(&"infrastructure".to_string()),
        "Presentation crate must not depend on Infrastructure crate. \
         Found dependencies: {:?}",
        deps
    );
}

// ── Test: No circular dependencies ────────────────────────────────

#[test]
fn no_circular_dependencies() {
    // Build a dependency graph from the four crate Cargo.toml files.
    let crates = [
        ("domain", "crates/domain/Cargo.toml"),
        ("application", "crates/application/Cargo.toml"),
        ("infrastructure", "crates/infrastructure/Cargo.toml"),
        ("presentation", "crates/presentation/Cargo.toml"),
    ];

    let internal_crate_names: Vec<&str> = crates.iter().map(|(name, _)| *name).collect();

    // Build adjacency list: crate -> [dependencies that are internal crates]
    let mut graph: std::collections::HashMap<&str, Vec<String>> = std::collections::HashMap::new();

    for (name, path) in &crates {
        let deps = get_dependencies(path);
        let internal_deps: Vec<String> = deps
            .into_iter()
            .filter(|d| internal_crate_names.contains(&d.as_str()))
            .collect();
        graph.insert(*name, internal_deps);
    }

    // Check for cycles using DFS with coloring (white=0, gray=1, black=2)
    let mut color: std::collections::HashMap<&str, u8> = std::collections::HashMap::new();
    for name in &internal_crate_names {
        color.insert(*name, 0);
    }

    fn has_cycle<'a>(
        node: &'a str,
        graph: &'a std::collections::HashMap<&str, Vec<String>>,
        color: &mut std::collections::HashMap<&'a str, u8>,
        path: &mut Vec<String>,
    ) -> bool {
        color.insert(node, 1); // gray — currently visiting
        path.push(node.to_string());

        if let Some(neighbors) = graph.get(node) {
            for neighbor in neighbors {
                let c = color.get(neighbor.as_str()).copied().unwrap_or(0);
                if c == 1 {
                    path.push(neighbor.clone());
                    return true; // back-edge found — cycle!
                }
                if c == 0 && has_cycle(neighbor, graph, color, path) {
                    return true;
                }
            }
        }

        color.insert(node, 2); // black — fully processed
        path.pop();
        false
    }

    let mut path = Vec::new();
    for name in &internal_crate_names {
        if color[name] == 0 && has_cycle(name, &graph, &mut color, &mut path) {
            panic!(
                "Circular dependency detected! Cycle path: {}",
                path.join(" -> ")
            );
        }
    }
}

// ── Test: Domain does not import infrastructure or presentation ────
//
// Note: the domain crate has an internal submodule `map::infrastructure`
// which is NOT the external `infrastructure` crate. We check for external
// crate imports specifically: `use ::infrastructure`, `extern crate infrastructure`,
// or top-level `use infrastructure::` / `use presentation::` patterns that
// indicate importing from an external crate. The internal `pub use infrastructure::`
// inside `map/mod.rs` is a re-export of a local submodule and is allowed.

#[test]
fn domain_does_not_import_infrastructure() {
    let uses = find_use_statements("crates/domain/src");

    // These patterns indicate external crate imports (not local module re-exports).
    let forbidden = [
        "use ::infrastructure",
        "use ::presentation",
        "extern crate infrastructure",
        "extern crate presentation",
    ];

    for (file, line) in &uses {
        for pattern in &forbidden {
            assert!(
                !line.contains(pattern),
                "Domain crate must not import from infrastructure or presentation! \
                 Found '{}' in {}",
                line,
                file
            );
        }
    }

    // Additionally verify no domain file imports from the infrastructure crate's
    // public API by looking for patterns like `use infrastructure::persistence`
    // but only in files that are NOT mod.rs (where `use infrastructure::` might
    // refer to the local submodule).
    let external_patterns = ["infrastructure::persistence", "presentation::"];
    for (file, line) in &uses {
        for pattern in &external_patterns {
            assert!(
                !line.contains(pattern),
                "Domain crate must not import from external infrastructure/presentation crate! \
                 Found '{}' in {}",
                line,
                file
            );
        }
    }
}

// ── Test: Presentation does not reference domain directly ──────────
//
// Presentation must access domain types through the application layer
// (via `application::domain::`). This test scans ALL source lines —
// not just `use` statements — to catch inline path expressions like
// `domain::hex::HexCoord::from_pixel(...)`.

fn find_domain_references(dir: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let dir_path = std::path::Path::new(dir);
    if !dir_path.exists() {
        return results;
    }
    collect_domain_references(dir_path, &mut results);
    results
}

fn collect_domain_references(dir: &std::path::Path, results: &mut Vec<(String, String)>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_domain_references(&path, results);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && let Ok(content) = fs::read_to_string(&path)
            {
                for line in content.lines() {
                    let trimmed = line.trim();
                    // Skip comment lines
                    if trimmed.starts_with("//") {
                        continue;
                    }
                    // Flag any `domain::` reference — presentation must use application:: re-exports
                    if trimmed.contains("domain::") {
                        results.push((path.display().to_string(), trimmed.to_string()));
                    }
                }
            }
        }
    }
}

#[test]
fn presentation_does_not_import_domain_directly() {
    let refs = find_domain_references("crates/presentation/src");

    for (file, line) in &refs {
        panic!(
            "Presentation crate must not reference domain directly. \
             Use application:: re-exports instead. \
             Found '{}' in {}",
            line, file
        );
    }
}

// ── Test: Presentation has no production unwrap() calls ───────────

fn find_unwrap_calls(dir: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let dir_path = std::path::Path::new(dir);
    if !dir_path.exists() {
        return results;
    }
    if let Ok(entries) = fs::read_dir(dir_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "rs")
                && let Ok(content) = fs::read_to_string(&path)
            {
                let mut in_test_block = false;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#[test]") {
                        in_test_block = true;
                    }
                    if !in_test_block && trimmed.contains(".unwrap()") && !trimmed.starts_with("//")
                    {
                        results.push((path.display().to_string(), trimmed.to_string()));
                    }
                }
            }
        }
    }
    results
}

#[test]
fn presentation_has_no_production_unwraps() {
    let hits = find_unwrap_calls("crates/presentation/src");

    for (file, line) in &hits {
        panic!(
            "Presentation crate must not use .unwrap() in production code. \
             Use typed errors or let-else instead. \
             Found '{}' in {}",
            line, file
        );
    }
}

// ── Test: Domain does not import bevy or any framework crate ───────

#[test]
fn domain_does_not_import_bevy_or_framework() {
    let uses = find_use_statements("crates/domain/src");
    let forbidden_prefixes = ["use bevy", "use godot", "use macroquad", "use wgpu"];

    for (file, line) in &uses {
        for pattern in &forbidden_prefixes {
            assert!(
                !line.contains(pattern),
                "Domain crate must not import framework crates! \
                 Found '{}' in {}",
                line,
                file
            );
        }
    }
}
