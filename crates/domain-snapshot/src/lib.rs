//! Mirror types for domain structs, carrying all serde derives.
//!
//! Domain stays serde-free; infrastructure serializes through these types.
//! Every module has `From<&domain::X> for Snap` and `From<Snap> for domain::X`.

pub mod diplomacy;
pub mod economy;
pub mod events;
pub mod game_state;
pub mod hex;
pub mod map;
pub mod military;
pub mod nation;
pub mod types;
