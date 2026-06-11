#![deny(warnings, clippy::all)]
#![allow(clippy::too_many_arguments, clippy::type_complexity)]
//! Native Bevy frontend. Talks to the game exclusively through
//! `frontend_api`: the [`frontend_api::Session`] is held as an opaque token
//! and every read goes through a JSON view model, never domain state.

pub mod app;
pub mod game;
pub mod map;
pub mod screens;
pub mod state;
pub mod theme;

pub use app::run_game;
