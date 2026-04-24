//! WASM shim for the standalone re-roll demo page.
//!
//! Compiled only when the `wasm` feature is active. The native CLI binary
//! never pulls this in. The API is intentionally string-based so the demo
//! JS can parse the result without knowing the internal structs.

use wasm_bindgen::prelude::*;

use crate::{FlagRules, GovernmentForm, GovernmentMix, Rng, generate_nations};

/// Parse a "kingdom=80,republic=20" style string into a `GovernmentMix`.
/// Unknown forms are ignored. Empty input returns the GP default mix.
fn parse_mix_spec(spec: &str) -> GovernmentMix {
    let spec = spec.trim();
    if spec.is_empty() {
        return GovernmentMix::great_power_default();
    }
    let mut mix = GovernmentMix::new();
    for entry in spec.split(',') {
        let entry = entry.trim();
        let (name, weight) = match entry.split_once('=') {
            Some(x) => x,
            None => continue,
        };
        let needle = name.trim().to_ascii_lowercase().replace(['_', '-', ' '], "");
        let weight: f32 = weight.trim().parse().unwrap_or(0.0);
        for form in GovernmentForm::ALL {
            let label = form.short_label().to_ascii_lowercase().replace(' ', "");
            let debug = format!("{form:?}").to_ascii_lowercase();
            if needle == label || needle == debug {
                mix = mix.add(*form, weight);
                break;
            }
        }
    }
    if mix.weights.is_empty() {
        GovernmentMix::great_power_default()
    } else {
        mix
    }
}

/// Generate `count` nations and return them as a JSON array. The JSON schema
/// matches the Rust `NationFlavor` struct (serde's default derivation).
///
/// `mix_spec` accepts `"kingdom=80,republic=20"` style strings; empty falls
/// back to the default great-power mix.
#[wasm_bindgen]
pub fn generate_nations_json(seed: u64, count: u32, mix_spec: &str) -> String {
    let mix = parse_mix_spec(mix_spec);
    let rules = FlagRules::default();
    let mut rng = Rng::from_seed(seed);
    let nations = generate_nations(&mut rng, count as usize, &mix, &rules);
    serde_json::to_string(&nations).unwrap_or_else(|e| format!("[{{\"error\":\"{e}\"}}]"))
}

/// List every government form as a JSON array of `{id, label, description}`
/// so the UI can render checkboxes / weight sliders dynamically.
#[wasm_bindgen]
pub fn government_forms_json() -> String {
    let list: Vec<_> = GovernmentForm::ALL
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": format!("{f:?}"),
                "label": f.short_label(),
                "description": f.description(),
            })
        })
        .collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string())
}
