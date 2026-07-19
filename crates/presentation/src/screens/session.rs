//! Between-turns session screens (card #494): after End Turn the first half
//! of the pipeline resolves, then these screens run over the live map with
//! the normal chrome hidden — the diplomatic session (accept/reject
//! proposals, be notified of every diplomatic change), the trade session
//! (wishlist offers arrive seller by seller, best relationships first), and
//! finally the read-only trade summary before the newspaper opens.

use bevy::prelude::*;

use crate::game::resources::{
    DataVersion, GameMeta, SessionLabelFilter, SessionRes, TurnInfo, TurnSessionUi, ViewModels,
};
use crate::game::turn_runner::{self, ActiveTurn};
use crate::game::vm::{self, SessionOfferVm};
use crate::map::camera::GameCamera;
use crate::map::icons::IconAssets;
use crate::map::layers::{MapBounds, MapMode};
use crate::screens::common::{spawn_icon, split_camel};
use crate::screens::ledger::FlagCache;
use crate::state::{Screen, TurnPhase};
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, SliderCommitted, SliderProps};

/// Chrome hidden while a between-turns session is on screen (top bar, side
/// panel, map-mode dropup).
#[derive(Component)]
pub struct SessionHiddenChrome;

/// `(map mode, camera translation, ortho scale)` captured when the
/// diplomatic session takes over the map (political mode, fit zoom);
/// restored when it ends.
#[derive(Resource, Default)]
pub struct SessionSavedView(pub Option<(MapMode, Vec3, f32)>);

#[derive(Component)]
pub struct DiploSessionRoot;

#[derive(Component)]
pub struct TradeSessionRoot;

#[derive(Component)]
pub struct SummaryRoot;

/// Advances past the current diplomatic notification.
#[derive(Component)]
pub struct DiploContinueBtn;

/// Accept / reject the first remaining proposal (its authoritative index).
#[derive(Component)]
pub struct DiploAcceptBtn(pub u32);

#[derive(Component)]
pub struct DiploRejectBtn(pub u32);

/// Buy the current offer at the slider amount.
#[derive(Component)]
pub struct OfferBuyBtn;

/// Decline the current offer, move to the next seller.
#[derive(Component)]
pub struct OfferPassBtn;

/// Skip every remaining offer for the current resource.
#[derive(Component)]
pub struct OfferSkipResourceBtn(pub String);

#[derive(Component)]
pub struct OfferAmountSlider;

#[derive(Component)]
pub struct SummaryContinueBtn;

/// Debug/perf escape hatch: existing screenshot drivers and the automated
/// perf run end turns expecting `Idle` right after resolution — they would
/// stall forever on an interactive session. Suppressing routes the turn
/// through the same two-halves path with auto decisions and no interstitials.
pub fn sessions_suppressed() -> bool {
    [
        "SESSION_AUTO_SKIP",
        "PERF_STATS",
        "M6_DEBUG",
        "M7_DEBUG",
        "M8_DEBUG",
        "M9_DEBUG",
        "M10_DEBUG",
    ]
    .iter()
    .any(|v| std::env::var(v).is_ok())
}

/// Hide the map chrome while a session screen is up (card #494: "during
/// those sessions the header/footer should not be shown").
pub fn sync_session_chrome(
    phase: Res<State<TurnPhase>>,
    mut chrome: Query<&mut Visibility, With<SessionHiddenChrome>>,
) {
    if !phase.is_changed() {
        return;
    }
    let hidden = phase.get().is_session();
    for mut visibility in &mut chrome {
        let target = if hidden {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != target {
            *visibility = target;
        }
    }
}

/// Re-fetch the session view after a decision mutated the paused state.
fn refresh_view(session_res: &SessionRes, ui: &mut TurnSessionUi) {
    ui.view = session_res.0.as_ref().and_then(|session| {
        frontend_api::turn_session::session_view(session)
            .ok()
            .and_then(|v| vm::parse_session_view(v).ok())
    });
}

/// Route out of the diplomatic session: into the trade session when the
/// player has offers to review, otherwise resume the turn.
#[allow(clippy::too_many_arguments)]
fn leave_diplo_session(
    ui: &TurnSessionUi,
    meta: &GameMeta,
    session_res: &mut SessionRes,
    active: &mut ActiveTurn,
    next_phase: &mut NextState<TurnPhase>,
) {
    if !meta.observer && ui.current_offer().is_some() {
        next_phase.set(TurnPhase::TradeSession);
    } else {
        turn_runner::start_finish_turn(session_res, active, next_phase);
    }
}

// ── Shared layout helpers ────────────────────────────────────────────────

fn spawn_flag(row: &mut ChildSpawnerCommands, flags: &FlagCache, nation_id: u32, height: f32) {
    if let Some(handle) = flags.get(nation_id) {
        row.spawn((
            Node {
                width: Val::Px(height * 1.5),
                height: Val::Px(height),
                flex_shrink: 0.0,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(theme::BORDER),
            ImageNode::new(handle),
        ));
    }
}

fn session_title(parent: &mut ChildSpawnerCommands, theme: &Theme, title: &str, progress: &str) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            width: Val::Percent(100.0),
            padding: UiRect::bottom(Val::Px(8.0)),
            border: UiRect::bottom(Val::Px(2.0)),
            ..default()
        })
        .insert(BorderColor::all(theme::GOLD))
        .with_children(|bar| {
            bar.spawn((
                Text::new(title),
                theme.font_bold(16.0),
                TextColor(theme::GOLD),
            ));
            bar.spawn((
                Text::new(progress),
                theme.font(12.0),
                TextColor(theme::TEXT_DIM),
            ));
        });
}

fn relation_label(score: i64) -> (String, Color) {
    let (label, color) = match score {
        s if s >= 40 => ("Friendly", Color::srgb_u8(0x44, 0xaa, 0x44)),
        s if s > 0 => ("Cordial", Color::srgb_u8(0x88, 0xaa, 0x44)),
        0 => ("Neutral", Color::srgb_u8(0x88, 0x88, 0x88)),
        s if s > -40 => ("Cool", Color::srgb_u8(0xcc, 0xaa, 0x44)),
        _ => ("Hostile", Color::srgb_u8(0xe6, 0x39, 0x46)),
    };
    (format!("{label} ({score:+})"), color)
}

// ── Diplomatic session ───────────────────────────────────────────────────

pub fn enter_diplo_session(
    mut commands: Commands,
    mut saved: ResMut<SessionSavedView>,
    mut mode: ResMut<MapMode>,
    bounds: Option<Res<MapBounds>>,
    windows: Query<&Window>,
    mut camera: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    // Take over the map like the Diplomacy screen (F4): political colors,
    // whole map fitted above the session sheet; restored on exit.
    if let Ok((mut transform, mut projection)) = camera.single_mut()
        && let Projection::Orthographic(ref mut ortho) = *projection
    {
        saved.0 = Some((*mode, transform.translation, ortho.scale));
        if let (Some(bounds), Ok(window)) = (bounds.as_deref(), windows.single()) {
            // Chrome is hidden; leave room for the bottom sheet (~190 px).
            let usable_h = (window.height() - 190.0).max(200.0);
            let usable_w = window.width().max(200.0);
            let world_h = bounds.max.y - bounds.min.y + 60.0;
            let fit = (bounds.width_px / usable_w).max(world_h / usable_h);
            ortho.scale = fit;
            transform.translation.x = bounds.center.x;
            // Bias the view center up so the sheet doesn't cover the
            // southern map edge.
            transform.translation.y = bounds.center.y - 60.0 * fit;
        }
    }
    *mode = MapMode::Political;

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(14.0)),
            row_gap: Val::Px(10.0),
            border: UiRect::top(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(theme::PANEL_BG_SOLID),
        BorderColor::all(theme::GOLD),
        Interaction::default(),
        crate::map::picking::PickingBlocker,
        DiploSessionRoot,
        GlobalZIndex(40),
    ));
}

pub fn exit_diplo_session(
    mut commands: Commands,
    roots: Query<Entity, With<DiploSessionRoot>>,
    mut saved: ResMut<SessionSavedView>,
    mut label_filter: ResMut<SessionLabelFilter>,
    mut mode: ResMut<MapMode>,
    mut camera: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    label_filter.0 = None;
    if let Some((saved_mode, translation, scale)) = saved.0.take() {
        *mode = saved_mode;
        if let Ok((mut transform, mut projection)) = camera.single_mut()
            && let Projection::Orthographic(ref mut ortho) = *projection
        {
            transform.translation = translation;
            ortho.scale = scale;
        }
    }
}

/// Rebuild the diplomatic-session sheet whenever the session state changes.
#[allow(clippy::too_many_arguments)]
pub fn update_diplo_session(
    ui: Res<TurnSessionUi>,
    meta: Res<GameMeta>,
    turn_info: Res<TurnInfo>,
    theme: Res<Theme>,
    flags: Res<FlagCache>,
    vms: Res<ViewModels>,
    mut label_filter: ResMut<SessionLabelFilter>,
    mut commands: Commands,
    roots: Query<Entity, With<DiploSessionRoot>>,
    added: Query<(), Added<DiploSessionRoot>>,
) {
    if !ui.is_changed() && added.is_empty() {
        return;
    }
    let Ok(root) = roots.single() else {
        return;
    };
    let Some(view) = ui.view.as_ref() else {
        return;
    };

    let total = view.diplo_events.len() + view.proposals.len();
    let position = (ui.diplo_index + 1).min(total.max(1));

    // Only the countries involved in the current exchange keep their name
    // label on the political map (card #494). Involvement = structured
    // nation ids plus any nation whose name appears in the item text (AI
    // war declarations only carry the actor id).
    let mut involved: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let name_of = |id: u32| -> Option<String> {
        vms.nations
            .iter()
            .find(|n| n.nation_id == id)
            .map(|n| n.name.clone())
    };
    if ui.diplo_index < view.diplo_events.len() {
        let event = &view.diplo_events[ui.diplo_index];
        for id in &event.nation_ids {
            involved.extend(name_of(*id));
        }
        for nation in &vms.nations {
            if !nation.name.is_empty() && event.text.contains(&nation.name) {
                involved.insert(nation.name.clone());
            }
        }
    } else if let Some(proposal) = view.proposals.first() {
        involved.extend(name_of(proposal.from_nation_id));
        involved.extend(name_of(meta.player_nation));
    }
    let new_filter = (!involved.is_empty()).then_some(involved);
    if label_filter.0 != new_filter {
        label_filter.0 = new_filter;
    }

    commands.entity(root).despawn_children();
    let observer = meta.observer;
    commands.entity(root).with_children(|sheet| {
        session_title(
            sheet,
            &theme,
            &format!("Diplomatic Session — {}", turn_info.label),
            &format!("{position} / {total}"),
        );

        if ui.diplo_index < view.diplo_events.len() {
            // Notification card.
            let event = &view.diplo_events[ui.diplo_index];
            sheet
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(12.0),
                    ..default()
                })
                .with_children(|row| {
                    for nation_id in event.nation_ids.iter().take(2) {
                        spawn_flag(row, &flags, *nation_id, 34.0);
                    }
                    row.spawn((
                        Text::new(event.text.clone()),
                        theme.font_bold(14.5),
                        TextColor(theme::TEXT),
                    ));
                });
            sheet
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    ..default()
                })
                .with_children(|row| {
                    let button = widgets::spawn_button(
                        row,
                        &theme,
                        ButtonProps {
                            label: "Continue \u{2192}".into(),
                            font_size: 13.0,
                            width: Some(Val::Px(160.0)),
                            ..default()
                        },
                    );
                    row.commands().entity(button).insert(DiploContinueBtn);
                });
        } else if let Some(proposal) = view.proposals.first() {
            // Actionable proposal card (observers only watch).
            let expiry = proposal.turns_until_expiry;
            sheet
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(12.0),
                    ..default()
                })
                .with_children(|row| {
                    spawn_flag(row, &flags, proposal.from_nation_id, 34.0);
                    row.spawn((
                        Text::new(proposal.display_text.clone()),
                        theme.font_bold(14.5),
                        TextColor(theme::TEXT),
                    ));
                    row.spawn((
                        Text::new(format!(
                            "(expires in {expiry} turn{})",
                            if expiry == 1 { "" } else { "s" }
                        )),
                        theme.font(11.0),
                        TextColor(theme::TEXT_DIM),
                    ));
                });
            sheet
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                })
                .with_children(|row| {
                    if observer {
                        let button = widgets::spawn_button(
                            row,
                            &theme,
                            ButtonProps {
                                label: "Continue \u{2192}".into(),
                                font_size: 13.0,
                                width: Some(Val::Px(160.0)),
                                ..default()
                            },
                        );
                        row.commands().entity(button).insert(DiploContinueBtn);
                    } else {
                        let accept = widgets::spawn_button(
                            row,
                            &theme,
                            ButtonProps {
                                label: "\u{2713} Accept".into(),
                                font_size: 13.0,
                                width: Some(Val::Px(140.0)),
                                ..default()
                            },
                        );
                        row.commands()
                            .entity(accept)
                            .insert(DiploAcceptBtn(proposal.index));
                        let reject = widgets::spawn_button(
                            row,
                            &theme,
                            ButtonProps {
                                label: "Reject".into(),
                                font_size: 13.0,
                                width: Some(Val::Px(140.0)),
                                ..default()
                            },
                        );
                        row.commands()
                            .entity(reject)
                            .insert(DiploRejectBtn(proposal.index));
                    }
                });
        }

        // Preview of what's still coming.
        let mut upcoming: Vec<String> = Vec::new();
        for event in view.diplo_events.iter().skip(ui.diplo_index + 1) {
            upcoming.push(event.text.clone());
        }
        let proposals_skip = if ui.diplo_index < view.diplo_events.len() {
            0
        } else {
            1
        };
        for proposal in view.proposals.iter().skip(proposals_skip) {
            upcoming.push(proposal.display_text.clone());
        }
        if !upcoming.is_empty() {
            let preview: Vec<String> = upcoming.iter().take(3).cloned().collect();
            let more = upcoming.len().saturating_sub(preview.len());
            let mut line = format!("coming up:  {}", preview.join("  ·  "));
            if more > 0 {
                line.push_str(&format!("  (+{more} more)"));
            }
            sheet.spawn((
                Text::new(line),
                theme.font_italic(10.5),
                TextColor(theme::TEXT_DIM),
            ));
        }
    });
}

/// Handle the diplomatic-session buttons. Proposal responses mutate the
/// paused game state directly (the treaty counts for this very turn).
#[allow(clippy::too_many_arguments)]
pub fn handle_diplo_session_buttons(
    mut activations: MessageReader<ButtonActivated>,
    continues: Query<(), With<DiploContinueBtn>>,
    accepts: Query<&DiploAcceptBtn>,
    rejects: Query<&DiploRejectBtn>,
    meta: Res<GameMeta>,
    mut ui: ResMut<TurnSessionUi>,
    mut session_res: ResMut<SessionRes>,
    mut active: ResMut<ActiveTurn>,
    mut data_version: ResMut<DataVersion>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
    mut toasts: MessageWriter<widgets::Toast>,
) {
    for ButtonActivated(entity) in activations.read() {
        let entity = *entity;
        let mut acted = false;
        if continues.contains(entity) {
            ui.diplo_index += 1;
            acted = true;
        } else if let Ok(accept) = accepts.get(entity)
            && let Some(session) = session_res.0.as_mut()
        {
            if let Err(err) = frontend_api::diplomacy::accept_proposal(
                session.game_mut(),
                meta.player_nation,
                accept.0,
            ) {
                toasts.write(widgets::Toast::error(format!(
                    "Accept failed: {}",
                    err.message()
                )));
            }
            data_version.0 += 1;
            refresh_view(&session_res, &mut ui);
            acted = true;
        } else if let Ok(reject) = rejects.get(entity)
            && let Some(session) = session_res.0.as_mut()
        {
            if let Err(err) = frontend_api::diplomacy::reject_proposal(
                session.game_mut(),
                meta.player_nation,
                reject.0,
            ) {
                toasts.write(widgets::Toast::error(format!(
                    "Reject failed: {}",
                    err.message()
                )));
            }
            data_version.0 += 1;
            refresh_view(&session_res, &mut ui);
            acted = true;
        }
        if acted && !ui.has_diplo_items() {
            leave_diplo_session(&ui, &meta, &mut session_res, &mut active, &mut next_phase);
        }
    }
}

// ── Trade session ────────────────────────────────────────────────────────

pub fn enter_trade_session(mut commands: Commands, mut ui: ResMut<TurnSessionUi>) {
    // Default the amount to the maximum for the first offer; the update
    // system keeps it in sync on offer changes.
    ui.amount = 0;
    // Full-screen panel (card #494 mockup): current offer on the left,
    // the queue of upcoming offers on the right.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(16.0)),
            row_gap: Val::Px(12.0),
            ..default()
        },
        BackgroundColor(theme::PANEL_BG_SOLID),
        Interaction::default(),
        crate::map::picking::PickingBlocker,
        TradeSessionRoot,
        GlobalZIndex(40),
    ));
}

pub fn exit_trade_session(mut commands: Commands, roots: Query<Entity, With<TradeSessionRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

/// The purchase caps for one offer: remaining stock, remaining cargo, and
/// what the treasury still affords.
fn offer_max(view: &vm::SessionViewVm, offer: &SessionOfferVm) -> u32 {
    let cargo_left = view.cargo_capacity.saturating_sub(view.cargo_committed);
    let cash_left = (view.treasury - view.money_committed).max(0);
    let affordable = if offer.price > 0 {
        (cash_left / offer.price).max(0) as u32
    } else {
        offer.remaining
    };
    offer.remaining.min(cargo_left).min(affordable)
}

/// Rebuild the trade-session card whenever the current offer changes.
#[allow(clippy::too_many_arguments)]
pub fn update_trade_session(
    mut ui: ResMut<TurnSessionUi>,
    turn_info: Res<TurnInfo>,
    theme: Res<Theme>,
    flags: Res<FlagCache>,
    icons: Option<Res<IconAssets>>,
    mut commands: Commands,
    roots: Query<Entity, With<TradeSessionRoot>>,
    added: Query<(), Added<TradeSessionRoot>>,
    mut last_key: Local<Option<(u32, String)>>,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    let key = ui
        .current_offer()
        .map(|o| (o.seller_id, o.resource.clone()));
    if *last_key == key && added.is_empty() && !ui.is_changed() {
        return;
    }
    let rebuild = *last_key != key || !added.is_empty();
    *last_key = key.clone();
    if !rebuild {
        return;
    }
    let Some(offer) = ui.current_offer().cloned() else {
        return;
    };
    let (max, affordable, offers_left, treasury_now, treasury_after, cargo_used, cargo_total) = {
        let Some(view) = ui.view.as_ref() else {
            return;
        };
        let raw_max = offer_max(view, &offer);
        let max = raw_max.max(1);
        // Progress: how many wishlist offers remain.
        let offers_left = view
            .offers
            .iter()
            .filter(|o| {
                o.remaining > 0
                    && !ui.skipped_resources.contains(&o.resource)
                    && !ui
                        .answered_offers
                        .contains(&(o.seller_id, o.resource.clone()))
            })
            .count();
        (
            max,
            raw_max > 0,
            offers_left,
            view.treasury - view.money_committed,
            view.treasury - view.money_committed - offer.price * i64::from(max),
            view.cargo_committed,
            view.cargo_capacity,
        )
    };
    ui.bypass_change_detection().amount = max;

    let icons = icons.as_deref();
    let (rel_label, rel_color) = relation_label(offer.relation_score);
    let resource_label = split_camel(&offer.resource);

    // Upcoming queue: every remaining offer in arrival order (the first
    // row is the offer shown on the left).
    let queue: Vec<SessionOfferVm> = ui
        .view
        .as_ref()
        .map(|view| {
            view.offers
                .iter()
                .filter(|o| {
                    o.remaining > 0
                        && !ui.skipped_resources.contains(&o.resource)
                        && !ui
                            .answered_offers
                            .contains(&(o.seller_id, o.resource.clone()))
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|screen| {
        session_title(
            screen,
            &theme,
            &format!("Trade Session — {}", turn_info.label),
            &format!(
                "{offers_left} offer{} left",
                if offers_left == 1 { "" } else { "s" }
            ),
        );

        // Content row: current offer (left) + upcoming queue (right).
        screen
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(24.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                ..default()
            })
            .with_children(|content| {
                // ── Left: the current offer ─────────────────────────────
                content
                    .spawn(Node {
                        width: Val::Percent(55.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(14.0),
                        padding: UiRect::all(Val::Px(10.0)),
                        ..default()
                    })
                    .with_children(|card| {
                        card.spawn((
                            Text::new("CURRENT OFFER"),
                            theme.font(10.0),
                            TextColor(theme::TEXT_DIM),
                        ));
                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(12.0),
                            ..default()
                        })
                        .with_children(|row| {
                            spawn_flag(row, &flags, offer.seller_id, 44.0);
                            row.spawn(Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(2.0),
                                ..default()
                            })
                            .with_children(|col| {
                                col.spawn((
                                    Text::new(offer.seller_name.clone()),
                                    theme.font_bold(16.0),
                                    TextColor(theme::TEXT),
                                ));
                                col.spawn((
                                    Text::new(rel_label),
                                    theme.font(11.5),
                                    TextColor(rel_color),
                                ));
                            });
                        });
                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Text::new("offers"),
                                theme.font(13.0),
                                TextColor(theme::TEXT_DIM),
                            ));
                            spawn_icon(row, icons, "commodities", &offer.resource, 22.0);
                            row.spawn((
                                Text::new(format!(
                                    "{resource_label} × {}  at ${} / unit",
                                    offer.remaining, offer.price
                                )),
                                theme.font_bold(15.0),
                                TextColor(theme::GOLD),
                            ));
                        });

                        // Amount slider.
                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(10.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Text::new("amount:"),
                                theme.font(13.0),
                                TextColor(theme::TEXT),
                            ));
                            let format_max = max;
                            let slider = widgets::spawn_slider(
                                row,
                                &theme,
                                SliderProps {
                                    min: 1.0,
                                    max: max as f32,
                                    step: 1.0,
                                    value: max as f32,
                                    width: Val::Px(220.0),
                                    format: Some(std::sync::Arc::new(move |v| {
                                        format!("{v:.0} / {format_max}")
                                    })),
                                    unlimited: false,
                                },
                            );
                            row.commands().entity(slider).insert(OfferAmountSlider);
                        });

                        // Buttons.
                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(10.0),
                            flex_wrap: FlexWrap::Wrap,
                            row_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|row| {
                            let buy = widgets::spawn_button(
                                row,
                                &theme,
                                ButtonProps {
                                    label: if affordable {
                                        format!(
                                            "\u{2713} Buy {max} for ${}",
                                            offer.price * i64::from(max)
                                        )
                                    } else {
                                        "Cannot afford".into()
                                    },
                                    font_size: 13.0,
                                    enabled: affordable,
                                    ..default()
                                },
                            );
                            row.commands()
                                .entity(buy)
                                .insert((OfferBuyBtn, BuyButtonLabelMax));
                            let pass = widgets::spawn_button(
                                row,
                                &theme,
                                ButtonProps {
                                    label: "Pass".into(),
                                    font_size: 13.0,
                                    ..default()
                                },
                            );
                            row.commands().entity(pass).insert(OfferPassBtn);
                            let skip = widgets::spawn_button(
                                row,
                                &theme,
                                ButtonProps {
                                    label: format!("Skip remaining {resource_label} offers"),
                                    font_size: 13.0,
                                    ..default()
                                },
                            );
                            row.commands()
                                .entity(skip)
                                .insert(OfferSkipResourceBtn(offer.resource.clone()));
                        });
                    });

                // ── Right: upcoming offers ──────────────────────────────
                content
                    .spawn((
                        Node {
                            flex_grow: 1.0,
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(6.0),
                            padding: UiRect::all(Val::Px(10.0)),
                            border: UiRect::left(Val::Px(1.0)),
                            min_height: Val::Px(0.0),
                            ..default()
                        },
                        BorderColor::all(theme::BORDER),
                    ))
                    .with_children(|right| {
                        right.spawn((
                            Text::new("COMING UP"),
                            theme.font(10.0),
                            TextColor(theme::TEXT_DIM),
                        ));
                        let list = widgets::spawn_scroll_area(
                            right,
                            &theme,
                            widgets::ScrollProps {
                                flex_grow: 1.0,
                                ..default()
                            },
                        );
                        let theme_ref = &theme;
                        let flags_ref = &flags;
                        right.commands().entity(list.content).with_children(|rows| {
                            for (index, entry) in queue.iter().enumerate() {
                                let is_current = index == 0;
                                rows.spawn(Node {
                                    flex_direction: FlexDirection::Row,
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(8.0),
                                    padding: UiRect::vertical(Val::Px(3.0)),
                                    ..default()
                                })
                                .with_children(|row| {
                                    spawn_flag(row, flags_ref, entry.seller_id, 18.0);
                                    row.spawn((
                                        Text::new(entry.seller_name.clone()),
                                        if is_current {
                                            theme_ref.font_bold(12.5)
                                        } else {
                                            theme_ref.font(12.5)
                                        },
                                        TextColor(if is_current {
                                            theme::GOLD
                                        } else {
                                            theme::TEXT
                                        }),
                                        Node {
                                            width: Val::Px(150.0),
                                            flex_shrink: 0.0,
                                            ..default()
                                        },
                                    ));
                                    spawn_icon(row, icons, "commodities", &entry.resource, 16.0);
                                    row.spawn((
                                        Text::new(format!(
                                            "{} ×{}",
                                            split_camel(&entry.resource),
                                            entry.remaining
                                        )),
                                        theme_ref.font(12.5),
                                        TextColor(theme::TEXT),
                                    ));
                                    row.spawn((
                                        Text::new(format!("${}", entry.price)),
                                        theme_ref.font(11.0),
                                        TextColor(theme::TEXT_DIM),
                                    ));
                                });
                            }
                        });
                    });
            });

        // Footer strip: treasury + cargo.
        screen
            .spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::top(Val::Px(8.0)),
                    border: UiRect::top(Val::Px(2.0)),
                    ..default()
                },
                BorderColor::all(theme::GOLD),
            ))
            .with_children(|row| {
                row.spawn((
                    Text::new(format!(
                        "treasury ${treasury_now} \u{2192} ${treasury_after}"
                    )),
                    theme.font(12.0),
                    TextColor(theme::TEXT_DIM),
                ));
                row.spawn((
                    Text::new(format!("cargo hold {cargo_used} / {cargo_total}")),
                    theme.font(12.0),
                    TextColor(theme::TEXT_DIM),
                ));
            });
    });
}

/// Marker: the Buy button's label shows the current slider amount.
#[derive(Component)]
pub struct BuyButtonLabelMax;

/// Keep the Buy button's label and `ui.amount` in sync with the slider.
pub fn handle_offer_slider(
    mut commits: MessageReader<SliderCommitted>,
    sliders: Query<(), With<OfferAmountSlider>>,
    buy_buttons: Query<&Children, With<BuyButtonLabelMax>>,
    mut labels: Query<&mut Text>,
    mut ui: ResMut<TurnSessionUi>,
) {
    for commit in commits.read() {
        if !sliders.contains(commit.entity) {
            continue;
        }
        let amount = commit.as_u32().max(1);
        ui.bypass_change_detection().amount = amount;
        let price = ui.current_offer().map(|o| o.price).unwrap_or(0);
        for children in &buy_buttons {
            for child in children {
                if let Ok(mut text) = labels.get_mut(*child) {
                    **text = format!("\u{2713} Buy {amount} for ${}", price * i64::from(amount));
                }
            }
        }
    }
}

/// Handle Buy / Pass / Skip. Buying records the trade on the paused turn;
/// when no offers remain the finish half of the turn starts.
#[allow(clippy::too_many_arguments)]
pub fn handle_trade_session_buttons(
    mut activations: MessageReader<ButtonActivated>,
    buys: Query<(), With<OfferBuyBtn>>,
    passes: Query<(), With<OfferPassBtn>>,
    skips: Query<&OfferSkipResourceBtn>,
    mut ui: ResMut<TurnSessionUi>,
    mut session_res: ResMut<SessionRes>,
    mut active: ResMut<ActiveTurn>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
    mut toasts: MessageWriter<widgets::Toast>,
) {
    for ButtonActivated(entity) in activations.read() {
        let entity = *entity;
        let mut acted = false;
        if buys.contains(entity) {
            let Some(offer) = ui.current_offer().cloned() else {
                continue;
            };
            let amount = ui.amount.max(1);
            if let Some(session) = session_res.0.as_mut() {
                match frontend_api::turn_session::accept_trade(
                    session,
                    offer.seller_id,
                    &offer.resource,
                    amount,
                ) {
                    Ok(bought) => {
                        toasts.write(widgets::Toast::info(format!(
                            "Bought {bought} {} from {}",
                            split_camel(&offer.resource),
                            offer.seller_name
                        )));
                    }
                    Err(err) => {
                        toasts.write(widgets::Toast::error(format!(
                            "Buy failed: {}",
                            err.message()
                        )));
                    }
                }
            }
            ui.answered_offers
                .insert((offer.seller_id, offer.resource.clone()));
            refresh_view(&session_res, &mut ui);
            acted = true;
        } else if passes.contains(entity) {
            if let Some(offer) = ui.current_offer().cloned() {
                ui.answered_offers.insert((offer.seller_id, offer.resource));
            }
            acted = true;
        } else if let Ok(skip) = skips.get(entity) {
            ui.skipped_resources.insert(skip.0.clone());
            acted = true;
        }
        if acted && ui.current_offer().is_none() {
            turn_runner::start_finish_turn(&mut session_res, &mut active, &mut next_phase);
        }
    }
}

// ── Trade summary ────────────────────────────────────────────────────────

pub fn enter_summary(mut commands: Commands) {
    // Full-screen report (card #494 mockup).
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(16.0)),
            row_gap: Val::Px(12.0),
            ..default()
        },
        BackgroundColor(theme::PANEL_BG_SOLID),
        Interaction::default(),
        crate::map::picking::PickingBlocker,
        SummaryRoot,
        GlobalZIndex(40),
    ));
}

pub fn exit_summary(mut commands: Commands, roots: Query<Entity, With<SummaryRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

/// Build the read-only trade summary (bought | sold columns + net).
#[allow(clippy::too_many_arguments)]
pub fn update_summary(
    ui: Res<TurnSessionUi>,
    turn_info: Res<TurnInfo>,
    theme: Res<Theme>,
    flags: Res<FlagCache>,
    icons: Option<Res<IconAssets>>,
    mut commands: Commands,
    roots: Query<Entity, With<SummaryRoot>>,
    added: Query<(), Added<SummaryRoot>>,
) {
    if !ui.is_changed() && added.is_empty() {
        return;
    }
    let Ok(root) = roots.single() else {
        return;
    };
    let icons = icons.as_deref();
    let bought: Vec<_> = ui.summary.iter().filter(|t| t.bought).collect();
    let sold: Vec<_> = ui.summary.iter().filter(|t| !t.bought).collect();
    let net: i64 = ui
        .summary
        .iter()
        .map(|t| {
            if t.bought {
                -t.total_cost
            } else {
                t.total_cost
            }
        })
        .sum();

    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|card| {
        session_title(
            card,
            &theme,
            &format!("Trade Summary — {}", turn_info.label),
            "",
        );
        card.spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(32.0),
            flex_grow: 1.0,
            min_height: Val::Px(0.0),
            ..default()
        })
        .with_children(|columns| {
            for (title, rows, color) in [
                ("BOUGHT", &bought, Color::srgb_u8(0xe6, 0x39, 0x46)),
                ("SOLD", &sold, Color::srgb_u8(0x2a, 0x9d, 0x8f)),
            ] {
                columns
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        flex_grow: 1.0,
                        ..default()
                    })
                    .with_children(|col| {
                        col.spawn((
                            Text::new(title),
                            theme.font_bold(11.0),
                            TextColor(theme::TEXT_DIM),
                        ));
                        if rows.is_empty() {
                            col.spawn((
                                Text::new("— nothing"),
                                theme.font_italic(11.5),
                                TextColor(theme::TEXT_DIM),
                            ));
                        }
                        for row in rows.iter() {
                            col.spawn(Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(8.0),
                                padding: UiRect::vertical(Val::Px(2.0)),
                                ..default()
                            })
                            .with_children(|line| {
                                spawn_flag(line, &flags, row.partner_id, 18.0);
                                line.spawn((
                                    Text::new(row.partner_name.clone()),
                                    theme.font(12.5),
                                    TextColor(theme::TEXT),
                                    Node {
                                        width: Val::Px(150.0),
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                ));
                                spawn_icon(line, icons, "commodities", &row.resource, 16.0);
                                line.spawn((
                                    Text::new(format!(
                                        "{} ×{}",
                                        split_camel(&row.resource),
                                        row.quantity,
                                    )),
                                    theme.font(12.5),
                                    TextColor(theme::TEXT),
                                ));
                                line.spawn((
                                    Text::new(format!(
                                        "{}${}",
                                        if row.bought { "-" } else { "+" },
                                        row.total_cost
                                    )),
                                    theme.font_bold(12.5),
                                    TextColor(color),
                                ));
                            });
                        }
                    });
            }
        });
        card.spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::top(Val::Px(8.0)),
            border: UiRect::top(Val::Px(1.0)),
            ..default()
        })
        .insert(BorderColor::all(theme::BORDER))
        .with_children(|footer| {
            footer.spawn((
                Text::new(format!(
                    "net this session:  {}${}",
                    if net >= 0 { "+" } else { "-" },
                    net.abs()
                )),
                theme.font_bold(13.0),
                TextColor(if net >= 0 {
                    Color::srgb_u8(0x2a, 0x9d, 0x8f)
                } else {
                    Color::srgb_u8(0xe6, 0x39, 0x46)
                }),
            ));
            let button = widgets::spawn_button(
                footer,
                &theme,
                ButtonProps {
                    label: "Continue \u{2192}".into(),
                    font_size: 13.0,
                    width: Some(Val::Px(160.0)),
                    ..default()
                },
            );
            footer.commands().entity(button).insert(SummaryContinueBtn);
        });
    });
}

/// Summary → newspaper (web end-turn order, card #494: summary first).
pub fn handle_summary_buttons(
    mut activations: MessageReader<ButtonActivated>,
    continues: Query<(), With<SummaryContinueBtn>>,
    mut ui: ResMut<TurnSessionUi>,
    mut next_phase: ResMut<NextState<TurnPhase>>,
    mut next_screen: ResMut<NextState<Screen>>,
) {
    for ButtonActivated(entity) in activations.read() {
        if continues.contains(*entity) {
            *ui = TurnSessionUi::default();
            next_phase.set(TurnPhase::Idle);
            next_screen.set(Screen::News);
        }
    }
}
