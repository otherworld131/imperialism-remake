//! Drag-buffered slider on top of the headless `bevy_ui_widgets::Slider`
//! core.
//!
//! The core turns pointer drags into `ValueChange<f32>` notifications and
//! tracks the drag lifecycle; this facade buffers those notifications in a
//! local [`UiSliderDrag`] (visuals only) and **commits exactly once on
//! release** via the [`SliderCommitted`] message — mirroring the React UI's
//! commit-on-release contract. Keyboard steps and track clicks are not drags,
//! so they commit immediately.
//!
//! Supports min/max/step, an optional "unlimited" notch one step past `max`
//! that maps to the [`UNLIMITED`] (`u32::MAX`) sentinel, and a custom value
//! label formatter.

use std::sync::Arc;

use bevy::picking::Pickable;
use bevy::picking::events::{DragEnd, DragStart, Pointer};
use bevy::prelude::*;
use bevy::ui_widgets::{
    CoreSliderDragState, Slider as CoreSlider, SliderRange, SliderStep, SliderThumb, SliderValue,
    TrackClick, ValueChange,
};

use crate::theme::{self, Theme};

/// Sentinel a committed "unlimited" value maps to.
pub const UNLIMITED: u32 = u32::MAX;

/// Formats the value label; `None` means the value is in the unlimited notch.
pub type SliderFormatFn = Arc<dyn Fn(f32) -> String + Send + Sync>;

/// Slider configuration; lives on the core slider entity.
#[derive(Component, Clone)]
pub struct UiSlider {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub unlimited: bool,
    pub format: Option<SliderFormatFn>,
}

impl UiSlider {
    /// Snap a raw core value onto the step grid, clamped to the full range
    /// (including the unlimited notch when enabled).
    fn snap(&self, value: f32) -> f32 {
        let step = if self.step > 0.0 { self.step } else { 1.0 };
        let snapped = self.min + ((value - self.min) / step).round() * step;
        snapped.clamp(self.min, self.range_end())
    }

    /// Upper end of the core range: one extra step when unlimited.
    fn range_end(&self) -> f32 {
        if self.unlimited {
            self.max + self.step.max(1.0)
        } else {
            self.max
        }
    }

    fn is_unlimited_value(&self, value: f32) -> bool {
        self.unlimited && value > self.max + self.step.max(1.0) * 0.5 - f32::EPSILON
    }

    fn label_for(&self, value: f32) -> String {
        if self.is_unlimited_value(value) {
            "∞".to_string()
        } else if let Some(format) = &self.format {
            format(value)
        } else if self.step >= 1.0 {
            format!("{value:.0}")
        } else {
            format!("{value:.2}")
        }
    }
}

/// Local drag buffer: while dragging, only this (and the visuals reading it)
/// move. The committed value is published once, on release.
#[derive(Component, Debug, Default)]
pub struct UiSliderDrag {
    pub dragging: bool,
    pub value: f32,
}

/// The committed slider value, emitted once per interaction.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct SliderCommitted {
    pub entity: Entity,
    /// Snapped value within `min..=max` (meaningless when `unlimited`).
    pub value: f32,
    /// The thumb sits in the unlimited notch.
    pub unlimited: bool,
}

impl SliderCommitted {
    /// Game-facing integer: [`UNLIMITED`] when the unlimited notch is chosen.
    pub fn as_u32(&self) -> u32 {
        if self.unlimited {
            UNLIMITED
        } else {
            self.value.max(0.0) as u32
        }
    }
}

#[derive(Component)]
struct UiSliderFill;

#[derive(Component)]
struct UiSliderThumbStyle;

/// Value label sitting next to the track, linked back to its slider.
#[derive(Component)]
struct UiSliderLabel(Entity);

pub struct SliderProps {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub value: f32,
    pub unlimited: bool,
    pub width: Val,
    pub format: Option<SliderFormatFn>,
}

impl Default for SliderProps {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            value: 0.0,
            unlimited: false,
            width: Val::Px(180.0),
            format: None,
        }
    }
}

/// Spawns `[track ── thumb] label` as one row; returns the slider (track)
/// entity, which is what [`SliderCommitted::entity`] refers to.
pub fn spawn_slider(
    parent: &mut ChildSpawnerCommands,
    theme: &Theme,
    props: SliderProps,
) -> Entity {
    let ui = UiSlider {
        min: props.min,
        max: props.max,
        step: props.step,
        unlimited: props.unlimited,
        format: props.format,
    };
    let initial = ui.snap(props.value);
    let label = ui.label_for(initial);
    let range_end = ui.range_end();

    let mut slider = Entity::PLACEHOLDER;
    parent
        .spawn((Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            ..default()
        },))
        .with_children(|row| {
            slider = row
                .spawn((
                    CoreSlider {
                        track_click: TrackClick::Snap,
                    },
                    SliderValue(initial),
                    SliderRange::new(props.min, range_end),
                    SliderStep(ui.step.max(f32::MIN_POSITIVE)),
                    UiSliderDrag {
                        dragging: false,
                        value: initial,
                    },
                    ui,
                    Node {
                        width: props.width,
                        height: Val::Px(18.0),
                        align_items: AlignItems::Center,
                        padding: UiRect::horizontal(Val::Px(0.0)),
                        ..default()
                    },
                ))
                .with_children(|track| {
                    // Recessed groove.
                    track
                        .spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(0.0),
                                right: Val::Px(0.0),
                                height: Val::Px(6.0),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(3.0)),
                                ..default()
                            },
                            BackgroundColor(theme::INSET_BG),
                            BorderColor::all(theme::BORDER),
                            Pickable::IGNORE,
                        ))
                        .with_children(|groove| {
                            groove.spawn((
                                Node {
                                    height: Val::Percent(100.0),
                                    width: Val::Percent(0.0),
                                    border_radius: BorderRadius::all(Val::Px(3.0)),
                                    ..default()
                                },
                                BackgroundColor(theme::GOLD.with_alpha(0.45)),
                                UiSliderFill,
                                Pickable::IGNORE,
                            ));
                        });
                    track.spawn((
                        SliderThumb,
                        UiSliderThumbStyle,
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            width: Val::Px(12.0),
                            height: Val::Px(16.0),
                            border: UiRect::all(Val::Px(1.0)),
                            border_radius: BorderRadius::all(Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(theme::GOLD),
                        BorderColor::all(theme::BORDER),
                    ));
                })
                .id();
            row.spawn((
                Text::new(label),
                theme.font(13.0),
                TextColor(theme::TEXT),
                UiSliderLabel(slider),
                Pickable::IGNORE,
            ));
        });
    slider
}

// ── Commit semantics ────────────────────────────────────────────────

/// Core notification: buffer while dragging, commit immediately otherwise
/// (keyboard arrows / Home / End / track-click snap are single gestures).
fn on_value_change(
    change: On<ValueChange<f32>>,
    mut sliders: Query<(&UiSlider, &mut UiSliderDrag, &CoreSliderDragState)>,
    mut commands: Commands,
    mut out: MessageWriter<SliderCommitted>,
) {
    let Ok((ui, mut drag, core_state)) = sliders.get_mut(change.source) else {
        return;
    };
    let snapped = ui.snap(change.value);
    drag.value = snapped;
    if core_state.dragging || drag.dragging {
        // Buffered: visuals follow `drag.value`; nothing is published.
        return;
    }
    commit(change.source, ui, snapped, &mut commands, &mut out);
}

fn on_drag_start(
    drag_start: On<Pointer<DragStart>>,
    mut sliders: Query<&mut UiSliderDrag, With<UiSlider>>,
) {
    if let Ok(mut drag) = sliders.get_mut(drag_start.entity) {
        drag.dragging = true;
    }
}

/// Release: publish the buffered value exactly once.
fn on_drag_end(
    drag_end: On<Pointer<DragEnd>>,
    mut sliders: Query<(&UiSlider, &mut UiSliderDrag)>,
    mut commands: Commands,
    mut out: MessageWriter<SliderCommitted>,
) {
    let Ok((ui, mut drag)) = sliders.get_mut(drag_end.entity) else {
        return;
    };
    if !drag.dragging {
        return;
    }
    drag.dragging = false;
    let value = drag.value;
    commit(drag_end.entity, ui, value, &mut commands, &mut out);
}

fn commit(
    entity: Entity,
    ui: &UiSlider,
    value: f32,
    commands: &mut Commands,
    out: &mut MessageWriter<SliderCommitted>,
) {
    // External state management: reflect the committed value into the core.
    commands.entity(entity).insert(SliderValue(value));
    let unlimited = ui.is_unlimited_value(value);
    out.write(SliderCommitted {
        entity,
        value: if unlimited { ui.max } else { value },
        unlimited,
    });
}

// ── Visuals ─────────────────────────────────────────────────────────

fn sync_slider_visuals(
    sliders: Query<(Entity, &UiSlider, &UiSliderDrag, &ComputedNode, &Children)>,
    mut thumbs: Query<(&mut Node, &ComputedNode), With<UiSliderThumbStyle>>,
    grooves: Query<&Children, (Without<UiSlider>, Without<UiSliderThumbStyle>)>,
    mut fills: Query<&mut Node, (With<UiSliderFill>, Without<UiSliderThumbStyle>)>,
    mut labels: Query<(&UiSliderLabel, &mut Text)>,
) {
    for (entity, ui, drag, computed, children) in &sliders {
        let span = (ui.range_end() - ui.min).max(f32::MIN_POSITIVE);
        let fraction = ((drag.value - ui.min) / span).clamp(0.0, 1.0);
        let track_width = computed.size().x * computed.inverse_scale_factor;
        for child in children {
            if let Ok((mut node, thumb_computed)) = thumbs.get_mut(*child) {
                let thumb_width = thumb_computed.size().x * thumb_computed.inverse_scale_factor;
                let travel = (track_width - thumb_width).max(0.0);
                let target = Val::Px((fraction * travel * 10.0).round() / 10.0);
                if node.left != target {
                    node.left = target;
                }
            }
            if let Ok(groove_children) = grooves.get(*child) {
                for groove_child in groove_children {
                    if let Ok(mut node) = fills.get_mut(*groove_child) {
                        let target = Val::Percent((fraction * 1000.0).round() / 10.0);
                        if node.width != target {
                            node.width = target;
                        }
                    }
                }
            }
        }
        for (link, mut text) in &mut labels {
            if link.0 == entity {
                let label = ui.label_for(drag.value);
                if **text != label {
                    **text = label;
                }
            }
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_message::<SliderCommitted>();
    app.add_observer(on_value_change);
    app.add_observer(on_drag_start);
    app.add_observer(on_drag_end);
    app.add_systems(Update, sync_slider_visuals);
}

// ── Tests: commit-on-release contract ───────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::picking::backend::HitData;
    use bevy::picking::pointer::{Location, PointerButton, PointerId};
    use bevy::window::WindowRef;

    /// Headless app with the core slider plugin (drag lifecycle) and our
    /// facade observers, but no rendering.
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::ui_widgets::SliderPlugin);
        app.add_message::<SliderCommitted>();
        app.add_observer(on_value_change);
        app.add_observer(on_drag_start);
        app.add_observer(on_drag_end);
        app
    }

    fn spawn_test_slider(app: &mut App, ui: UiSlider, value: f32) -> Entity {
        let initial = ui.snap(value);
        let range_end = ui.range_end();
        let (min, step) = (ui.min, ui.step);
        app.world_mut()
            .spawn((
                CoreSlider {
                    track_click: TrackClick::Snap,
                },
                SliderValue(initial),
                SliderRange::new(min, range_end),
                SliderStep(step),
                UiSliderDrag {
                    dragging: false,
                    value: initial,
                },
                ui,
            ))
            .id()
    }

    fn pointer_location(app: &mut App) -> Location {
        let window = app.world_mut().spawn(()).id();
        let target = WindowRef::Primary
            .normalize(Some(window))
            .expect("primary window ref");
        Location {
            target: bevy::camera::NormalizedRenderTarget::Window(target),
            position: Vec2::ZERO,
        }
    }

    fn drag_start(app: &mut App, slider: Entity) {
        let location = pointer_location(app);
        let camera = app.world_mut().spawn(()).id();
        app.world_mut().trigger(Pointer {
            entity: slider,
            pointer_id: PointerId::Mouse,
            pointer_location: location,
            event: DragStart {
                button: PointerButton::Primary,
                hit: HitData::new(camera, 0.0, None, None),
            },
        });
    }

    fn drag_end(app: &mut App, slider: Entity) {
        let location = pointer_location(app);
        app.world_mut().trigger(Pointer {
            entity: slider,
            pointer_id: PointerId::Mouse,
            pointer_location: location,
            event: DragEnd {
                button: PointerButton::Primary,
                distance: Vec2::ZERO,
            },
        });
    }

    fn core_value_change(app: &mut App, slider: Entity, value: f32) {
        app.world_mut().trigger(ValueChange {
            source: slider,
            value,
        });
    }

    fn drain_commits(app: &mut App) -> Vec<SliderCommitted> {
        app.world_mut()
            .resource_mut::<Messages<SliderCommitted>>()
            .drain()
            .collect()
    }

    fn plain_slider() -> UiSlider {
        UiSlider {
            min: 0.0,
            max: 100.0,
            step: 5.0,
            unlimited: false,
            format: None,
        }
    }

    #[test]
    fn drag_buffers_and_commits_once_on_release() {
        let mut app = test_app();
        let slider = spawn_test_slider(&mut app, plain_slider(), 0.0);

        drag_start(&mut app, slider);
        core_value_change(&mut app, slider, 10.0);
        core_value_change(&mut app, slider, 40.0);
        core_value_change(&mut app, slider, 65.0);

        // Mid-drag: visuals buffer moved, nothing committed.
        assert!(
            drain_commits(&mut app).is_empty(),
            "no commit while dragging"
        );
        let drag = app.world().get::<UiSliderDrag>(slider).unwrap();
        assert!(drag.dragging);
        assert_eq!(drag.value, 65.0);
        assert_eq!(
            app.world().get::<SliderValue>(slider).unwrap().0,
            0.0,
            "core committed value untouched while dragging"
        );

        drag_end(&mut app, slider);
        app.update();

        let commits = drain_commits(&mut app);
        assert_eq!(commits.len(), 1, "exactly one commit on release");
        assert_eq!(commits[0].value, 65.0);
        assert!(!commits[0].unlimited);
        assert_eq!(commits[0].as_u32(), 65);
        assert_eq!(app.world().get::<SliderValue>(slider).unwrap().0, 65.0);

        // A stray second release does not double-commit.
        drag_end(&mut app, slider);
        app.update();
        assert!(drain_commits(&mut app).is_empty());
    }

    #[test]
    fn non_drag_changes_commit_immediately() {
        let mut app = test_app();
        let slider = spawn_test_slider(&mut app, plain_slider(), 50.0);

        // Keyboard step / track-click snap arrive as ValueChange without any
        // drag lifecycle around them.
        core_value_change(&mut app, slider, 55.0);
        app.update();

        let commits = drain_commits(&mut app);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].value, 55.0);
        assert_eq!(app.world().get::<SliderValue>(slider).unwrap().0, 55.0);
    }

    #[test]
    fn values_snap_to_step() {
        let mut app = test_app();
        let slider = spawn_test_slider(&mut app, plain_slider(), 0.0);

        core_value_change(&mut app, slider, 17.0);
        app.update();

        let commits = drain_commits(&mut app);
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].value, 15.0, "17 snaps to the 5-step grid");
    }

    #[test]
    fn unlimited_notch_maps_to_sentinel() {
        let mut app = test_app();
        let ui = UiSlider {
            unlimited: true,
            ..plain_slider()
        };
        let slider = spawn_test_slider(&mut app, ui, 0.0);

        drag_start(&mut app, slider);
        // Drag past max into the extra notch (max 100 + step 5 = 105).
        core_value_change(&mut app, slider, 240.0);
        assert!(drain_commits(&mut app).is_empty());

        drag_end(&mut app, slider);
        app.update();

        let commits = drain_commits(&mut app);
        assert_eq!(commits.len(), 1);
        assert!(commits[0].unlimited);
        assert_eq!(commits[0].as_u32(), UNLIMITED);

        // Back inside the normal range the sentinel disappears.
        drag_start(&mut app, slider);
        core_value_change(&mut app, slider, 80.0);
        drag_end(&mut app, slider);
        app.update();
        let commits = drain_commits(&mut app);
        assert_eq!(commits.len(), 1);
        assert!(!commits[0].unlimited);
        assert_eq!(commits[0].as_u32(), 80);
    }
}
