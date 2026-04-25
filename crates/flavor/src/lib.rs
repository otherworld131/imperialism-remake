//! Ancillary flavor systems: procedural country/city names, government forms,
//! and flag designs. The crate is deliberately decoupled from `domain` so the
//! scenario loader can call it once at game init and hand the resulting
//! display strings/SVG to `Nation`. Nothing here is reached during turn
//! resolution.
//!
//! Everything is deterministic given a seed so replays and save files stay
//! stable. The RNG is a small xorshift64 (no external crate deps).

pub mod flags;
pub mod government;
pub mod lua_mix;
pub mod names;
pub mod rng;

#[cfg(feature = "wasm")]
pub mod web;

pub use flags::{
    Emblem, EmblemPosition, FlagDesign, FlagExclusion, FlagPattern, FlagRules, random_for, svg_for,
};
pub use government::{GovernmentForm, GovernmentMix, government_title};
pub use lua_mix::{load_default_mixes, load_mixes_from};
pub use names::{
    CountryName, generate_city_name, generate_city_names, generate_country_name,
    generate_country_names,
};
pub use rng::Rng;

/// Bundle of every flavor artifact a nation needs for display.
/// Produced by `generate_for_seed` (or `generate_nations`) in one shot so
/// callers have a single entry point.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NationFlavor {
    pub name: String,
    pub adjective: String,
    pub demonym_singular: String,
    pub demonym_plural: String,
    pub government: GovernmentForm,
    pub government_title: String,
    pub flag: FlagDesign,
    pub flag_svg: String,
}

impl Default for NationFlavor {
    fn default() -> Self {
        // A neutral, non-empty default so old saves / tests that skip
        // flavor generation still deserialize cleanly.
        Self {
            name: String::new(),
            adjective: String::new(),
            demonym_singular: String::new(),
            demonym_plural: String::new(),
            government: GovernmentForm::ConstitutionalMonarchy,
            government_title: String::new(),
            flag: FlagDesign::default(),
            flag_svg: String::new(),
        }
    }
}

/// Generate a single nation's flavor from a seed + mix + rules.
pub fn generate_for_seed(seed: u64, mix: &GovernmentMix, rules: &FlagRules) -> NationFlavor {
    let mut rng = Rng::from_seed(seed);
    generate_one(&mut rng, mix, rules)
}

/// Generate `count` nations from a shared RNG stream. Distinct country names
/// across the batch are enforced by a retry loop.
pub fn generate_nations(
    rng: &mut Rng,
    count: usize,
    mix: &GovernmentMix,
    rules: &FlagRules,
) -> Vec<NationFlavor> {
    let mut out: Vec<NationFlavor> = Vec::with_capacity(count);
    let mut tries = 0;
    while out.len() < count && tries < count * 20 {
        let cand = generate_one(rng, mix, rules);
        if !out.iter().any(|n| n.name == cand.name) {
            out.push(cand);
        }
        tries += 1;
    }
    out
}

fn generate_one(rng: &mut Rng, mix: &GovernmentMix, rules: &FlagRules) -> NationFlavor {
    let name = generate_country_name(rng);
    let government = mix.pick(rng);
    let flag = random_for(rng, government, rules);
    let government_title = government_title(&name.name, government);
    let flag_svg = svg_for(&flag);
    NationFlavor {
        name: name.name,
        adjective: name.adjective,
        demonym_singular: name.demonym_singular,
        demonym_plural: name.demonym_plural,
        government,
        government_title,
        flag,
        flag_svg,
    }
}
