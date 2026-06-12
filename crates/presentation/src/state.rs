//! App-wide state machines: setup vs. in-game, the turn phase, and the
//! active game screen.

use bevy::prelude::*;

/// Top-level app mode: the game-setup flow (config → preview → capital) or
/// a live game. [`Screen`] only matters while `InGame`; during `Setup` it
/// stays on the default `Map` so the preview map renders underneath the
/// setup chrome.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Setup,
    InGame,
}

/// Whether the player can act or a turn is resolving on a background thread.
/// Setup's async world generation also parks in `Processing` so input,
/// debug-screenshot frame counting, and the busy overlay behave uniformly.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TurnPhase {
    #[default]
    Idle,
    Processing,
}

/// Active game screen (web `ScreenTab` parity). `Industry`, `Trade`, `Tech`,
/// `Ledger`, `News`, `Battles` and `Legend` are full-screen overlays drawn
/// over the live map world; `Transport` and `Diplomacy` keep the map visible
/// (Diplomacy zoom-locks it and forces the diplomatic overlay). `News` also
/// auto-opens after every end turn (the newspaper interstitial).
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
    News,
    Battles,
    Legend,
}

impl Screen {
    /// Screens that hide the map entirely (web `isFullScreen`).
    pub fn is_full_screen(self) -> bool {
        matches!(
            self,
            Screen::Industry
                | Screen::Trade
                | Screen::Tech
                | Screen::Ledger
                | Screen::News
                | Screen::Battles
                | Screen::Legend
        )
    }
}

/// Run condition: the map is visible and interactive (Map or Transport).
pub fn map_interactive(screen: Res<State<Screen>>) -> bool {
    !screen.get().is_full_screen()
}
