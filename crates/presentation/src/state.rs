//! App-wide state machines: the turn phase and the active game screen.

use bevy::prelude::*;

/// Whether the player can act or a turn is resolving on a background thread.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TurnPhase {
    #[default]
    Idle,
    Processing,
}

/// Active game screen (web `ScreenTab` parity). `Industry`, `Trade`, `Tech`
/// and `Ledger` are full-screen overlays drawn over the live map world;
/// `Transport` and `Diplomacy` keep the map visible (Diplomacy zoom-locks it
/// and forces the diplomatic overlay). Extensible for the M9 screens.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Screen {
    #[default]
    Map,
    Transport,
    Industry,
    Diplomacy,
    Trade,
    Tech,
    Ledger,
}

impl Screen {
    /// Screens that hide the map entirely (web `isFullScreen`).
    pub fn is_full_screen(self) -> bool {
        matches!(
            self,
            Screen::Industry | Screen::Trade | Screen::Tech | Screen::Ledger
        )
    }
}

/// Run condition: the map is visible and interactive (Map or Transport).
pub fn map_interactive(screen: Res<State<Screen>>) -> bool {
    !screen.get().is_full_screen()
}
