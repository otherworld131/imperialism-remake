//! Glue between domain `GameState` and the flavor crate for the web build.
//! Mirror of `src/flavor_bridge.rs` in the top-level binary — duplicated
//! here because the wasm-bridge crate can't import the binary's modules.
//! Kept tiny on purpose so divergence stays easy to spot.

use domain::game_state::GameState;
use domain::types::NationType;
use flavor::{FlagRules, GovernmentMix, generate_for_seed};

fn djb2(s: &str) -> u64 {
    let mut h: u64 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h
}

fn nation_seed(base_seed: u64, nation_index: u32) -> u64 {
    let mut z = base_seed.wrapping_add((nation_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

pub fn apply_flavor(game: &mut GameState) {
    let base_seed = djb2(&game.map_key);
    let rules = FlagRules::default();
    let gp_mix = GovernmentMix::great_power_default();
    let mn_mix = GovernmentMix::minor_nation_default();

    for nation in game.nations.iter_mut() {
        if !nation.flag_svg.is_empty() {
            continue;
        }
        let is_gp = nation.nation_type == NationType::GreatPower;
        let mix = if is_gp { &gp_mix } else { &mn_mix };
        let s = nation_seed(base_seed, nation.id.0);
        let flavor = generate_for_seed(s, mix, &rules);
        nation.adjective = flavor.adjective;
        nation.demonym_singular = flavor.demonym_singular;
        nation.demonym_plural = flavor.demonym_plural;
        nation.government_title = flavor.government_title;
        nation.flag_svg = flavor.flag_svg;
    }
}
