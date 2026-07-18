//! Map-click routing and selection state — the native port of the web
//! frontend's `handleTileClick` / `handleNavyMarkerClick` flows plus the
//! systems that keep movement/deploy/fleet affordances in sync.
//!
//! Click priority (App.tsx parity): pending fleet move → pending unit move →
//! civilian deploy → navy-marker selection → tile selection (with civilian
//! pick-up and capital unit auto-select). Every action only *queues* — the
//! commands it emits mutate pending state resolved at end turn.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::ui::UiScale;
use bevy::window::PrimaryWindow;
use std::collections::HashSet;

use crate::game::commands::GameCommand;
use crate::game::resources::{
    DeployMode, DeployState, DiploUi, EngineerPrompt, EngineerPromptState, FleetTargets, GameMeta,
    MoveTargets, PendingMoveList, PendingMoves, ProvinceUnits, RailLinkOption, RailLinkOptions,
    RailLinkState, SelectedCivilian, SelectedNavy, SelectedShips, SelectedUnits, SessionRes,
    TileIndex, ViewModels,
};
use crate::game::vm::{self, MapTile};
use crate::map::icons::IconAssets;
use crate::map::navy;
use crate::map::picking::{HoverTarget, MapClick, PickingBlocker, SelectedHex};
use crate::screens::diplomacy::{can_target_nation, invalid_target_reason};
use crate::state::Screen;
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonProps, ModalStack, Toast, TooltipText};

/// Terrains a prospector can search (web `PROSPECTOR_TERRAIN`).
const PROSPECTOR_TERRAIN: [&str; 5] = ["Hills", "Mountain", "Swamp", "Desert", "Tundra"];

/// Buttons inside the engineer build popover: the build kind this icon
/// orders ("railroad" | "depot" | "port").
#[derive(Component, Clone, Copy)]
pub struct EngineerChoiceButton(pub &'static str);

// ── Deploy-mode entry (web handleDeployCivilian) ─────────────────────────

/// Compute the deployable-tile set for one civilian type. Ported verbatim
/// from `App.tsx::handleDeployCivilian`: owned land tiles without a
/// civilian, where the type can work (visible resources only).
pub fn compute_deploy_state(
    civilian_id: i64,
    civ_type: &str,
    redeploy_from: Option<(i32, i32)>,
    tiles: &[MapTile],
    player_nation: u32,
) -> DeployState {
    let player = i64::from(player_nation);
    let mut deployable: HashSet<(i32, i32)> = HashSet::new();
    let mut prospected: HashSet<(i32, i32)> = HashSet::new();
    for t in tiles {
        let ter = t.terrain.as_str();
        // Prospector: mark already-searched tiles for the red-✗ overlay,
        // even when occupied.
        if civ_type == "Prospector"
            && t.nation_id == player
            && PROSPECTOR_TERRAIN.contains(&ter)
            && t.is_prospected
        {
            prospected.insert((t.q, t.r));
        }
        if t.nation_id != player || t.is_sea() || t.civilian_on_tile.is_some() {
            continue;
        }
        // Only visible resources count (hidden deposits are not targets).
        let res = match (&t.resource, t.resource_hidden) {
            (Some(r), false) => Some(r.as_str()),
            _ => None,
        };
        let can_work = match civ_type {
            "Farmer" => matches!(res, Some("Grain" | "Fruit" | "Cotton")),
            "Rancher" => matches!(res, Some("Wool" | "Livestock" | "Horses")),
            "Forester" => res == Some("Timber"),
            "Miner" => matches!(res, Some("Coal" | "Iron")),
            "Driller" => res == Some("Oil"),
            "Prospector" => PROSPECTOR_TERRAIN.contains(&ter) && !t.is_prospected,
            "Engineer" => true, // any owned land tile
            _ => false,
        };
        if can_work {
            deployable.insert((t.q, t.r));
        }
    }
    DeployState {
        civilian_id,
        civ_type: civ_type.to_string(),
        redeploy_from,
        deployable,
        prospected,
    }
}

// ── Map clicks ───────────────────────────────────────────────────────────

pub fn handle_map_click(
    mut clicks: MessageReader<MapClick>,
    session: Res<SessionRes>,
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
    move_targets: Res<MoveTargets>,
    fleet_targets: Res<FleetTargets>,
    theme: Res<Theme>,
    screen: Res<State<Screen>>,
    mut diplo: ResMut<DiploUi>,
    mut toasts: MessageWriter<Toast>,
    mut commands: Commands,
    mut game_commands: MessageWriter<GameCommand>,
    popover_ui: (
        Query<&Window, With<PrimaryWindow>>,
        Res<UiScale>,
        Option<Res<IconAssets>>,
    ),
    selections: (
        ResMut<SelectedHex>,
        ResMut<SelectedNavy>,
        ResMut<SelectedUnits>,
        ResMut<SelectedCivilian>,
        ResMut<DeployMode>,
        ResMut<EngineerPrompt>,
        ResMut<RailLinkOptions>,
    ),
) {
    let (windows, ui_scale, icons) = popover_ui;
    let (
        mut selected_hex,
        mut selected_navy,
        mut selected_units,
        mut selected_civilian,
        mut deploy,
        mut prompt,
        mut rail,
    ) = selections;
    for MapClick(target) in clicks.read() {
        // Any map click closes an open engineer build popover (choosing a
        // build icon is a UI click and never reaches here). Deploy mode is
        // left alone: clicking a highlighted tile below still redeploys.
        if let Some(state) = prompt.0.take() {
            commands.entity(state.root).despawn();
        }
        let tile = match target {
            HoverTarget::Hex(q, r) => {
                let Some(tiles) = vms.map.as_ref() else {
                    continue;
                };
                index.by_coord.get(&(*q, *r)).and_then(|i| tiles.get(*i))
            }
            _ => None,
        };

        // 0. Diplomacy screen: pending-marker dismiss → armed-action fire →
        //    plain selection (pins the nation card). Unit / civilian /
        //    fleet flows never trigger here (web parity).
        if *screen.get() == Screen::Diplomacy {
            match target {
                HoverTarget::Treaty {
                    nation_id,
                    action_key,
                } => {
                    if !meta.observer {
                        game_commands.write(GameCommand::DismissPendingDiplomacy {
                            target: *nation_id,
                            action_key: action_key.clone(),
                        });
                    }
                }
                HoverTarget::Hex(..) => {
                    let Some(tile) = tile else {
                        continue;
                    };
                    if let Some(action) = diplo.queued.clone() {
                        if meta.observer {
                            continue;
                        }
                        let target_nation = (!tile.is_sea() && tile.nation_id >= 0)
                            .then_some(tile.nation_id as u32)
                            .filter(|id| *id != meta.player_nation);
                        let valid = target_nation.is_some_and(|id| {
                            can_target_nation(&action, id, vms.diplomacy_screen.as_ref())
                        });
                        if !valid {
                            let reason = invalid_target_reason(
                                &action,
                                target_nation,
                                meta.player_nation,
                                vms.diplomacy_screen.as_ref(),
                            )
                            .unwrap_or_else(|| {
                                "Select a foreign nation for this diplomatic action.".into()
                            });
                            // The armed action stays armed (web parity).
                            toasts.write(Toast::error(reason));
                            continue;
                        }
                        if let Some(target_nation) = target_nation {
                            diplo.queued = None;
                            game_commands.write(GameCommand::QueueDiplomacy {
                                action,
                                target: target_nation,
                            });
                        }
                        continue;
                    }
                    // No armed action: pin the clicked nation's card.
                    selected_hex.0 = Some((tile.q, tile.r));
                    selected_navy.0 = None;
                    selected_units.0.clear();
                }
                _ => {}
            }
            continue;
        }

        // 1. Fleet movement: player fleet selected + clicked sea hex inside
        //    an adjacent zone → queue a fleet move (resolved at end turn).
        if let Some(tile) = tile
            && !meta.observer
            && fleet_targets.0.contains(&(tile.q, tile.r))
            && let Some(marker) = selected_navy
                .0
                .as_deref()
                .and_then(|key| vms.navy_markers.iter().find(|m| navy::marker_key(m) == key))
            && let Some(from_zone) = marker.sea_zone_id
        {
            let to_zone = vms
                .sea_zones
                .iter()
                .find(|z| z.hexes.iter().any(|h| h.q == tile.q && h.r == tile.r))
                .map(|z| z.id);
            if let Some(to_zone) = to_zone {
                game_commands.write(GameCommand::MoveFleet { from_zone, to_zone });
                continue;
            }
        }

        // 2. Implicit movement mode: units selected + clicked tile is a
        //    valid target → queue all selected unit moves (all-or-nothing).
        if let Some(tile) = tile
            && !selected_units.0.is_empty()
            && let Some(pid) = tile.province_id
            && (move_targets.friendly.contains(&pid) || move_targets.hostile.contains(&pid))
        {
            game_commands.write(GameCommand::QueueUnitMoves {
                unit_ids: selected_units.0.clone(),
                dest_province_id: pid as u32,
            });
            continue;
        }

        // 3. Deploy mode: clicks on highlighted tiles deploy; clicking
        //    another idle own civilian switches the armed deploy to it
        //    (and reopens an engineer's popover); anything else is ignored
        //    and the mode stays armed (web F-004).
        if let Some(state) = deploy.0.as_ref() {
            let Some(tile) = tile else {
                continue;
            };
            // 3a. Rail-link click (card #497): for a settled armed engineer,
            //     the six neighbouring hexes are link targets, not redeploy
            //     targets — clicking an allowed one orders the link, clicking
            //     a refused one explains why (the ghost preview showed red).
            if let Some(rail_state) = rail.0.as_ref()
                && rail_state.civilian_id == state.civilian_id
                && let Some(opt) = rail_state
                    .options
                    .iter()
                    .find(|o| (o.q, o.r) == (tile.q, tile.r))
            {
                if opt.allowed && opt.affordable {
                    game_commands.write(GameCommand::EngineerBuildRailLink {
                        civilian_id: state.civilian_id as u32,
                        to_q: tile.q,
                        to_r: tile.r,
                    });
                    deploy.0 = None;
                    rail.0 = None;
                } else {
                    let why = opt
                        .reason
                        .clone()
                        .unwrap_or_else(|| "not enough funds".to_string());
                    toasts.write(Toast::info(format!("Cannot lay track: {}", why)));
                }
                continue;
            }
            if !state.deployable.contains(&(tile.q, tile.r)) {
                if !meta.observer
                    && let Some(civ) = tile.civilian_on_tile.as_ref()
                    && civ.is_human
                    && tile.nation_id == i64::from(meta.player_nation)
                    && !civ.working
                {
                    selected_hex.0 = Some((tile.q, tile.r));
                    arm_civilian_deploy(
                        civ,
                        (tile.q, tile.r),
                        meta.player_nation,
                        &session,
                        &vms,
                        (&theme, icons.as_deref(), &windows, &ui_scale),
                        &mut commands,
                        &mut toasts,
                        &mut deploy,
                        &mut prompt,
                        &mut rail,
                    );
                }
                continue;
            }
            // Engineers deploy like everyone else (card #495): placement is
            // this turn's action; the build popover appears when the parked
            // engineer is clicked next turn.
            game_commands.write(GameCommand::DeployCivilian {
                civilian_id: state.civilian_id as u32,
                q: tile.q,
                r: tile.r,
                recall_first: state.redeploy_from.is_some(),
            });
            continue;
        }

        // 4. Navy marker: toggle selection, clear tile/unit selection.
        if let HoverTarget::Navy(key) = target {
            if selected_navy.0.as_deref() == Some(key.as_str()) {
                selected_navy.0 = None;
            } else {
                selected_navy.0 = Some(key.clone());
            }
            selected_hex.0 = None;
            selected_units.0.clear();
            continue;
        }

        // 5. Plain tile selection.
        let Some(tile) = tile else {
            continue;
        };
        selected_hex.0 = Some((tile.q, tile.r));
        if selected_navy.0.is_some() {
            selected_navy.0 = None;
        }
        selected_units.0.clear();

        // Civilian on the clicked tile: idle player civilians enter deploy
        // mode immediately; busy ones get selected (blinking marker).
        if !meta.observer
            && let Some(civ) = tile.civilian_on_tile.as_ref()
            && civ.is_human
            && tile.nation_id == i64::from(meta.player_nation)
        {
            if !civ.working {
                arm_civilian_deploy(
                    civ,
                    (tile.q, tile.r),
                    meta.player_nation,
                    &session,
                    &vms,
                    (&theme, icons.as_deref(), &windows, &ui_scale),
                    &mut commands,
                    &mut toasts,
                    &mut deploy,
                    &mut prompt,
                    &mut rail,
                );
                continue;
            }
            selected_civilian.0 = Some(civ.id);
        } else {
            selected_civilian.0 = None;
        }

        // Capital tile: auto-select every movable unit on player capitals
        // (the unit panel itself is kept in sync by `sync_province_units`).
        if !meta.observer
            && tile.is_capital
            && tile.nation_id == i64::from(meta.player_nation)
            && let Some(pid) = tile.province_id
            && let Some(session) = session.0.as_ref()
            && let Ok(Ok(units)) =
                frontend_api::units::get_units_in_province(session.game(), pid as u32)
                    .map(vm::parse_province_units)
        {
            selected_units.0 = units
                .army_units
                .iter()
                .filter(|u| u.category != "Garrison")
                .map(|u| u.id)
                .collect();
        }
    }
}

// ── Engineer build popover (card #495) ───────────────────────────────────

/// Arm deploy mode for an idle civilian standing on `(q, r)`; engineers
/// additionally get their build popover (or a toast explaining why builds
/// are blocked this turn). Shared by the plain-tile pickup and the
/// switch-civilian-while-deploying path.
#[allow(clippy::too_many_arguments)]
fn arm_civilian_deploy(
    civ: &vm::CivilianOnTile,
    (q, r): (i32, i32),
    player_nation: u32,
    session: &SessionRes,
    vms: &ViewModels,
    ui: (
        &Theme,
        Option<&IconAssets>,
        &Query<&Window, With<PrimaryWindow>>,
        &UiScale,
    ),
    commands: &mut Commands,
    toasts: &mut MessageWriter<Toast>,
    deploy: &mut DeployMode,
    prompt: &mut EngineerPrompt,
    rail: &mut RailLinkOptions,
) {
    let (theme, icons, windows, ui_scale) = ui;
    if let Some(tiles) = vms.map.as_ref() {
        deploy.0 = Some(compute_deploy_state(
            civ.id,
            &civ.civ_type,
            Some((q, r)),
            tiles,
            player_nation,
        ));
    }
    rail.0 = None;
    if civ.civ_type == "Engineer"
        && let Some(session) = session.0.as_ref()
        && let Ok(options) =
            frontend_api::units::get_engineer_build_options(session.game(), civ.id as u32)
    {
        if options
            .get("can_build_now")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            open_engineer_popover(
                commands,
                theme,
                icons,
                &options,
                popover_anchor(windows, ui_scale),
                civ.id,
                prompt,
            );
            // Card #497: a settled engineer's neighbouring hexes become rail
            // link targets — fetch the per-direction options that drive the
            // hover ghost preview and adjacent-click orders.
            rail.0 = parse_rail_link_state(
                frontend_api::units::get_rail_link_options(session.game(), civ.id as u32).ok(),
                civ.id,
                (q, r),
            );
        } else if let Some(reason) = options.get("blocked_reason").and_then(|v| v.as_str()) {
            toasts.write(Toast::info(format!("Engineer: {}", reason)));
        }
    }
}

/// Parse the six-direction rail-link options JSON for a settled engineer
/// (card #497). Returns `None` when the query failed or the engineer turns
/// out not to be buildable right now.
fn parse_rail_link_state(
    json: Option<serde_json::Value>,
    civilian_id: i64,
    origin: (i32, i32),
) -> Option<RailLinkState> {
    let json = json?;
    if !json
        .get("can_build_now")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    let options = json
        .get("options")?
        .as_array()?
        .iter()
        .filter_map(|o| {
            Some(RailLinkOption {
                q: o.get("q")?.as_i64()? as i32,
                r: o.get("r")?.as_i64()? as i32,
                allowed: o.get("allowed")?.as_bool().unwrap_or(false),
                affordable: o.get("affordable")?.as_bool().unwrap_or(false),
                cost: o.get("cost").and_then(|c| c.as_i64()),
                reason: o.get("reason").and_then(|s| s.as_str()).map(str::to_string),
            })
        })
        .collect();
    Some(RailLinkState {
        civilian_id,
        origin,
        options,
    })
}

/// Where the popover opens: just beside the cursor, clamped to the window
/// (in UI coordinates, i.e. divided by the global [`UiScale`]).
fn popover_anchor(windows: &Query<&Window, With<PrimaryWindow>>, ui_scale: &UiScale) -> Vec2 {
    let Ok(window) = windows.single() else {
        return Vec2::new(40.0, 40.0);
    };
    let scale = ui_scale.0.max(0.01);
    let cursor = window.cursor_position().unwrap_or_default() / scale;
    let bounds = Vec2::new(window.width(), window.height()) / scale;
    // Keep the strip (~200×64 UI px) fully on screen.
    (cursor + Vec2::new(14.0, 10.0)).min((bounds - Vec2::new(210.0, 80.0)).max(Vec2::ZERO))
}

/// Spawn the compact build strip: one icon button per build kind with the
/// cost beside it — no labels (ticket #495), tooltips carry the words.
/// Disallowed kinds render dimmed with the reason as tooltip so the strip
/// teaches the prerequisites (depot needs rail, port needs coast).
fn open_engineer_popover(
    commands: &mut Commands,
    theme: &Theme,
    icons: Option<&IconAssets>,
    options: &serde_json::Value,
    anchor: Vec2,
    civilian_id: i64,
    prompt: &mut EngineerPrompt,
) {
    let empty = Vec::new();
    let opts = options
        .get("options")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(anchor.x),
                top: Val::Px(anchor.y),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(7.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_SOLID),
            BorderColor::all(theme::GOLD),
            GlobalZIndex(180),
            FocusPolicy::Block,
            Interaction::default(),
            PickingBlocker,
        ))
        .id();

    commands.entity(root).with_children(|row| {
        for opt in opts {
            let kind = opt.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let (kind_static, icon_name) = match kind {
                "railroad" => ("railroad", "Railroad"),
                "depot" => ("depot", "Depot"),
                "port" => ("port", "Port"),
                _ => continue,
            };
            let allowed = opt
                .get("allowed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let affordable = opt
                .get("affordable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let cost = opt.get("cost").and_then(|v| v.as_i64());
            let reason = opt.get("reason").and_then(|v| v.as_str());
            let enabled = allowed && affordable;

            let tooltip = if let Some(reason) = reason {
                format!("{}: {}", icon_name, reason)
            } else if !affordable {
                format!(
                    "{} — ${} (insufficient funds)",
                    icon_name,
                    cost.unwrap_or(0)
                )
            } else {
                format!("Build {} — ${}", icon_name, cost.unwrap_or(0))
            };

            let button = widgets::spawn_button(
                row,
                theme,
                ButtonProps {
                    label: String::new(),
                    enabled,
                    ..default()
                },
            );
            row.commands()
                .entity(button)
                .insert((EngineerChoiceButton(kind_static), TooltipText(tooltip)))
                .with_children(|content| {
                    if let Some(image) = icons.and_then(|i| i.get("infrastructure", icon_name)) {
                        let mut icon = ImageNode::new(image);
                        if !enabled {
                            icon.color = Color::srgba(1.0, 1.0, 1.0, 0.35);
                        }
                        content.spawn((
                            Node {
                                width: Val::Px(22.0),
                                height: Val::Px(22.0),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            icon,
                            bevy::picking::Pickable::IGNORE,
                        ));
                    }
                    if let Some(cost) = cost {
                        content.spawn((
                            Text::new(format!("${}", cost)),
                            theme.font_bold(11.0),
                            TextColor(if !allowed {
                                theme::TEXT_DIM
                            } else if !affordable {
                                theme::ALARM
                            } else {
                                theme::GOLD
                            }),
                            Node {
                                margin: UiRect::left(Val::Px(4.0)),
                                ..default()
                            },
                            bevy::picking::Pickable::IGNORE,
                        ));
                    }
                });
        }

        // Card #497: railroads left the popover — point at the new gesture.
        row.spawn((
            Text::new("Click a neighbouring tile to lay track"),
            theme.font(10.5),
            TextColor(theme::TEXT_DIM),
            Node {
                margin: UiRect::left(Val::Px(6.0)),
                max_width: Val::Px(120.0),
                ..default()
            },
            bevy::picking::Pickable::IGNORE,
        ));
    });

    prompt.0 = Some(EngineerPromptState { civilian_id, root });
}

/// Popover buttons: an icon click orders the build on the engineer's hex.
pub fn handle_engineer_choice(
    mut activations: MessageReader<widgets::ButtonActivated>,
    buttons: Query<&EngineerChoiceButton>,
    mut prompt: ResMut<EngineerPrompt>,
    mut commands: Commands,
    mut game_commands: MessageWriter<GameCommand>,
) {
    for widgets::ButtonActivated(entity) in activations.read() {
        let Ok(choice) = buttons.get(*entity) else {
            continue;
        };
        let Some(state) = prompt.0.take() else {
            continue;
        };
        game_commands.write(GameCommand::EngineerBuild {
            civilian_id: state.civilian_id as u32,
            kind: choice.0,
        });
        commands.entity(state.root).despawn();
    }
}

/// Clear the prompt state when its popover node was despawned by other means.
pub fn sync_engineer_prompt(mut prompt: ResMut<EngineerPrompt>, nodes: Query<(), With<Node>>) {
    if let Some(state) = prompt.0.as_ref()
        && !nodes.contains(state.root)
    {
        prompt.0 = None;
    }
}

// ── Esc cascade ──────────────────────────────────────────────────────────

/// Esc cancels, in order: armed diplomacy action → engineer build popover →
/// deploy mode → unit selection → navy selection → tile selection → back to
/// the map screen (from Transport / Diplomacy) → quit. The full-screen
/// overlays gate this system off; `map_hud::screen_hotkeys` owns Esc there.
pub fn esc_cascade(
    keys: Res<ButtonInput<KeyCode>>,
    modals: Res<ModalStack>,
    focus: Res<bevy::input_focus::InputFocus>,
    screen: Res<State<Screen>>,
    mut next_screen: ResMut<NextState<Screen>>,
    mut diplo: ResMut<DiploUi>,
    mut deploy: ResMut<DeployMode>,
    mut selected_units: ResMut<SelectedUnits>,
    mut selected_navy: ResMut<SelectedNavy>,
    mut selected_hex: ResMut<SelectedHex>,
    mut selected_civilian: ResMut<SelectedCivilian>,
    mut prompt: ResMut<EngineerPrompt>,
    mut rail: ResMut<RailLinkOptions>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    // Modals own Esc; focused text inputs own the keyboard.
    if !keys.just_pressed(KeyCode::Escape) || !modals.is_empty() || focus.0.is_some() {
        return;
    }
    if diplo.queued.is_some()
        || diplo.show_grant_picker
        || diplo.show_break_picker
        || diplo.confirm_war
    {
        diplo.queued = None;
        diplo.show_grant_picker = false;
        diplo.show_break_picker = false;
        diplo.confirm_war = false;
    } else if let Some(state) = prompt.0.take() {
        // Engineer build popover closes first; redeploy mode stays armed.
        commands.entity(state.root).despawn();
    } else if deploy.0.is_some() {
        deploy.0 = None;
        rail.0 = None;
    } else if !selected_units.0.is_empty() {
        selected_units.0.clear();
    } else if selected_navy.0.is_some() {
        selected_navy.0 = None;
    } else if selected_hex.0.is_some() || selected_civilian.0.is_some() {
        selected_hex.0 = None;
        selected_civilian.0 = None;
    } else if *screen.get() != Screen::Map {
        next_screen.set(Screen::Map);
    } else {
        exit.write(AppExit::Success);
    }
}

// ── Derived state sync ───────────────────────────────────────────────────

/// Keep the unit-panel VM in step with the selected capital and the data
/// version (pending moves / upgrades refresh after every command).
pub fn sync_province_units(
    session: Res<SessionRes>,
    vms: Res<ViewModels>,
    index: Res<TileIndex>,
    selected_hex: Res<SelectedHex>,
    mut province_units: ResMut<ProvinceUnits>,
) {
    let target = selected_hex
        .0
        .and_then(|coord| {
            let tiles = vms.map.as_ref()?;
            tiles.get(*index.by_coord.get(&coord)?)
        })
        .filter(|t| t.is_capital)
        .and_then(|t| t.province_id);
    if province_units.province_id == target && province_units.version == vms.version {
        return;
    }
    let Some(session) = session.0.as_ref() else {
        return;
    };
    province_units.province_id = target;
    province_units.version = vms.version;
    province_units.vm = target.and_then(|pid| {
        frontend_api::units::get_units_in_province(session.game(), pid as u32)
            .ok()
            .and_then(|v| vm::parse_province_units(v).ok())
    });
}

/// Valid move-target highlights: the intersection of every selected unit's
/// legal destinations (web parity), recomputed when the selection or the
/// data version moves.
pub fn recompute_move_targets(
    session: Res<SessionRes>,
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    selected_units: Res<SelectedUnits>,
    mut targets: ResMut<MoveTargets>,
    mut last_version: Local<Option<u64>>,
) {
    if !selected_units.is_changed() && *last_version == Some(vms.version) {
        return;
    }
    *last_version = Some(vms.version);

    let mut friendly: Vec<u64> = Vec::new();
    let mut hostile: Vec<u64> = Vec::new();
    if !selected_units.0.is_empty()
        && let Some(session) = session.0.as_ref()
    {
        let game = session.game();
        let per_unit: Vec<vm::MoveTargetsVm> = selected_units
            .0
            .iter()
            .filter_map(|&id| {
                frontend_api::units::get_valid_move_targets(game, meta.player_nation, id)
                    .ok()
                    .and_then(|v| vm::parse_move_targets(v).ok())
            })
            .collect();
        if per_unit.len() == selected_units.0.len()
            && let Some(first) = per_unit.first()
        {
            friendly = first
                .friendly
                .iter()
                .filter(|t| {
                    per_unit.iter().all(|targets| {
                        targets
                            .friendly
                            .iter()
                            .any(|f| f.province_id == t.province_id)
                    })
                })
                .map(|t| t.province_id)
                .collect();
            hostile = first
                .hostile
                .iter()
                .filter(|t| {
                    per_unit.iter().all(|targets| {
                        targets
                            .hostile
                            .iter()
                            .any(|h| h.province_id == t.province_id)
                    })
                })
                .map(|t| t.province_id)
                .collect();
        }
    }
    if targets.friendly != friendly || targets.hostile != hostile {
        targets.friendly = friendly;
        targets.hostile = hostile;
    }
}

/// Fleet-selection bookkeeping: drop stale marker keys, auto-select every
/// warship in the selected player fleet's zone, and highlight the sea hexes
/// of adjacent (non-lake) zones as move targets.
pub fn sync_fleet_selection(
    meta: Res<GameMeta>,
    vms: Res<ViewModels>,
    mut selected_navy: ResMut<SelectedNavy>,
    mut selected_ships: ResMut<SelectedShips>,
    mut fleet_targets: ResMut<FleetTargets>,
    mut last: Local<Option<(String, u64)>>,
) {
    // Stale key (marker despawned after a turn) → clear selection.
    if let Some(key) = selected_navy.0.clone()
        && !vms.navy_markers.iter().any(|m| navy::marker_key(m) == key)
    {
        selected_navy.0 = None;
    }

    let state = selected_navy.0.clone().map(|key| (key, vms.version));
    if !selected_navy.is_changed() && *last == state && !vms.is_changed() {
        return;
    }
    *last = state;

    let marker = selected_navy
        .0
        .as_deref()
        .and_then(|key| vms.navy_markers.iter().find(|m| navy::marker_key(m) == key));
    let player_fleet = marker.filter(|m| {
        !meta.observer && m.kind == "fleet" && m.nation_id == i64::from(meta.player_nation)
    });

    let mut ships: Vec<u32> = Vec::new();
    let mut hexes: HashSet<(i32, i32)> = HashSet::new();
    if let Some(marker) = player_fleet
        && let Some(zone_id) = marker.sea_zone_id
    {
        if let Some(fleet) = vms.ships.as_ref() {
            ships = fleet
                .warships
                .iter()
                .filter(|s| s.sea_zone == Some(zone_id))
                .map(|s| s.id)
                .collect();
        }
        if let Some(zone) = vms.sea_zones.iter().find(|z| z.id == zone_id) {
            for adj_id in &zone.adjacent_zone_ids {
                let Some(adj) = vms.sea_zones.iter().find(|z| z.id == *adj_id) else {
                    continue;
                };
                if adj.is_lake {
                    continue;
                }
                for h in &adj.hexes {
                    hexes.insert((h.q, h.r));
                }
            }
        }
    }
    if selected_ships.0 != ships {
        selected_ships.0 = ships;
    }
    if fleet_targets.0 != hexes {
        fleet_targets.0 = hexes;
    }
}

/// Derive the pending-move map arrows from the queued-move list.
pub fn sync_pending_move_arrows(pending: Res<PendingMoveList>, mut arrows: ResMut<PendingMoves>) {
    if !pending.is_changed() {
        return;
    }
    let next: Vec<crate::game::resources::PendingMoveArrow> = pending
        .0
        .iter()
        .map(|m| crate::game::resources::PendingMoveArrow {
            source_province_id: m.source_province_id,
            dest_province_id: m.dest_province_id,
        })
        .collect();
    if arrows.0 != next {
        arrows.0 = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal map tile through the same JSON contract the live view
    /// models use, overriding only the fields a test cares about.
    fn tile(q: i32, overrides: serde_json::Value) -> MapTile {
        let mut base = serde_json::json!({
            "q": q, "r": 0, "map_width": 10, "map_height": 10,
            "terrain": "Grassland", "owner": "Drarang", "owner_color": "Drarang",
            "nation_id": 0, "province": "P", "province_id": 1,
            "is_capital": false, "is_country_capital": false, "is_minor": false,
            "is_incorporated_minor": false, "incorporated_nation_id": null,
            "is_anarchic": false, "is_prospected": false,
            "resource": null, "resource_hidden": false,
            "improvement_level": 0, "max_improvement_level": 0,
            "rail_links": [], "has_depot": false, "has_port": false,
            "has_fort": false, "has_river": false, "fort_level": 0,
            "port_blockaded": false, "army_unit_count": 0, "army_firepower": 0.0,
            "army_composition": null, "naval_ship_count": 0, "naval_firepower": 0,
            "civilian_on_tile": null, "visible": true, "visual_group": null,
        });
        base.as_object_mut()
            .unwrap()
            .extend(overrides.as_object().unwrap().clone());
        serde_json::from_value(base).unwrap()
    }

    fn deployable(civ_type: &str, tiles: &[MapTile]) -> HashSet<(i32, i32)> {
        compute_deploy_state(1, civ_type, None, tiles, 0).deployable
    }

    #[test]
    fn resource_workers_need_matching_visible_resources() {
        let tiles = vec![
            tile(0, serde_json::json!({"resource": "Grain"})),
            tile(1, serde_json::json!({"resource": "Cotton"})),
            tile(2, serde_json::json!({"resource": "Wool"})),
            tile(3, serde_json::json!({"resource": "Timber"})),
            tile(4, serde_json::json!({"resource": "Coal"})),
            tile(
                5,
                serde_json::json!({"resource": "Iron", "resource_hidden": true}),
            ),
            tile(6, serde_json::json!({"resource": "Oil"})),
            tile(7, serde_json::json!({"resource": "Grain", "nation_id": 3})),
            tile(
                8,
                serde_json::json!({"resource": "Grain", "terrain": "Sea"}),
            ),
        ];
        assert_eq!(
            deployable("Farmer", &tiles),
            HashSet::from([(0, 0), (1, 0)])
        );
        assert_eq!(deployable("Rancher", &tiles), HashSet::from([(2, 0)]));
        assert_eq!(deployable("Forester", &tiles), HashSet::from([(3, 0)]));
        // Hidden Iron is not a valid Miner target (visible resources only).
        assert_eq!(deployable("Miner", &tiles), HashSet::from([(4, 0)]));
        assert_eq!(deployable("Driller", &tiles), HashSet::from([(6, 0)]));
    }

    #[test]
    fn engineer_works_any_owned_land_tile_without_civilian() {
        let occupied = serde_json::json!({"civilian_on_tile": {
            "id": 9, "type": "Farmer", "working": true, "turns_remaining": 2,
            "build_task": null, "owner": "Drarang", "owner_color": "Drarang",
            "is_human": true,
        }});
        let tiles = vec![
            tile(0, serde_json::json!({})),
            tile(1, serde_json::json!({"terrain": "Sea"})),
            tile(2, serde_json::json!({"nation_id": 2})),
            tile(3, occupied),
        ];
        assert_eq!(deployable("Engineer", &tiles), HashSet::from([(0, 0)]));
    }

    #[test]
    fn prospector_searches_unprospected_rough_terrain_and_marks_searched() {
        let tiles = vec![
            tile(0, serde_json::json!({"terrain": "Hills"})),
            tile(
                1,
                serde_json::json!({"terrain": "Mountain", "is_prospected": true}),
            ),
            tile(2, serde_json::json!({"terrain": "Grassland"})),
            tile(3, serde_json::json!({"terrain": "Swamp"})),
            tile(4, serde_json::json!({"terrain": "Desert", "nation_id": 5})),
            tile(5, serde_json::json!({"terrain": "Tundra"})),
        ];
        let state = compute_deploy_state(1, "Prospector", None, &tiles, 0);
        assert_eq!(state.deployable, HashSet::from([(0, 0), (3, 0), (5, 0)]));
        // Already-searched rough terrain gets the red-✗ overlay.
        assert_eq!(state.prospected, HashSet::from([(1, 0)]));
    }
}
