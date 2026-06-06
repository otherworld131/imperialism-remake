#![deny(warnings)]
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod app;
pub mod camera;
pub mod civilian_assets;
pub mod colors;
pub mod hex_renderer;
pub mod ui;

pub use app::run_game;
pub use application;
