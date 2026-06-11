//! Styled widget kit (parchment theme) for every game screen.
//!
//! Each widget is a `spawn_*` constructor plus the systems/observers that
//! make it behave; [`WidgetsPlugin`] registers everything. Interaction cores
//! come from Bevy's experimental `bevy_ui_widgets` crate where they help
//! (button/checkbox/radio/slider/scrollbar) — that API is wrapped here and
//! **must never leak outside this module**: consumers talk to the kit through
//! the `spawn_*` functions, facade components, and Bevy messages
//! ([`ButtonActivated`], [`SliderCommitted`], …).

pub mod button;
pub mod checkbox;
pub mod dropdown;
pub mod modal;
pub mod progress;
pub mod scroll;
pub mod slider;
pub mod table;
pub mod tabs;
pub mod text_input;
pub mod toast;
pub mod tooltip;

pub use button::{ButtonActivated, ButtonProps, UiButton, spawn_button};
pub use checkbox::{
    CheckboxProps, CheckboxToggled, RadioSelected, UiCheckbox, UiRadioGroup, spawn_checkbox,
    spawn_radio_group,
};
pub use dropdown::{
    DropdownChanged, DropdownProps, MultiDropdownChanged, MultiDropdownProps, UiDropdown,
    UiMultiDropdown, spawn_dropdown, spawn_multi_dropdown,
};
pub use modal::{ModalHandles, ModalProps, ModalStack, close_top_modal, open_modal};
pub use progress::{ProgressProps, UiProgress, spawn_progress};
pub use scroll::{ScrollHandles, ScrollProps, UiScrollArea, spawn_scroll_area};
pub use slider::{SliderCommitted, SliderProps, UNLIMITED, UiSlider, spawn_slider};
pub use table::{ColumnSpec, TableProps, UiTable, spawn_table};
pub use tabs::{TabChanged, TabGroup, TabsHandles, spawn_tabs};
pub use text_input::{TextInputChanged, TextInputProps, UiTextInput, spawn_text_input};
pub use toast::{Toast, ToastKind};
pub use tooltip::TooltipText;

use bevy::input_focus::InputDispatchPlugin;
use bevy::prelude::*;
use bevy::ui_widgets::{
    ButtonPlugin, CheckboxPlugin, RadioGroupPlugin, ScrollbarPlugin, SliderPlugin,
};

/// One-stop registration for the whole widget kit.
pub struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::theme::Theme>();
        if !app.is_plugin_added::<InputDispatchPlugin>() {
            app.add_plugins(InputDispatchPlugin);
        }
        // Headless interaction cores (experimental, wrapped by this module).
        app.add_plugins((
            ButtonPlugin,
            CheckboxPlugin,
            RadioGroupPlugin,
            ScrollbarPlugin,
            SliderPlugin,
        ));
        // Our styled facades.
        app.add_plugins((
            button::plugin,
            checkbox::plugin,
            dropdown::plugin,
            modal::plugin,
            progress::plugin,
            scroll::plugin,
            slider::plugin,
            table::plugin,
            tabs::plugin,
            text_input::plugin,
            toast::plugin,
            tooltip::plugin,
        ));
    }
}

/// Enable/disable any kit widget (button, checkbox, slider…). Disabled
/// widgets ignore input and render dimmed.
pub fn set_enabled(commands: &mut Commands, widget: Entity, enabled: bool) {
    if enabled {
        commands
            .entity(widget)
            .remove::<bevy::ui::InteractionDisabled>();
    } else {
        commands
            .entity(widget)
            .insert(bevy::ui::InteractionDisabled);
    }
}
