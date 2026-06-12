//! Map-click routing and selection state — the native port of the web
//! frontend's `handleTileClick` / `handleNavyMarkerClick` flows plus the
//! systems that keep movement/deploy/fleet affordances in sync.
//!
//! Click priority (App.tsx parity): pending fleet move → pending unit move →
//! civilian deploy → navy-marker selection → tile selection (with civilian
//! pick-up and capital unit auto-select). Every action only *queues* — the
//! commands it emits mutate pending state resolved at end turn.

use bevy::prelude::*;
use std::collections::HashSet;

use crate::game::commands::GameCommand;
use crate::game::resources::{
    DeployMode, DeployState, DiploUi, EngineerPrompt, EngineerPromptState, FleetTargets, GameMeta,
    MoveTargets, PendingMoveList, PendingMoves, ProvinceUnits, SelectedCivilian, SelectedNavy,
    SelectedShips, SelectedUnits, SessionRes, TileIndex, ViewModels,
};
use crate::game::vm::{self, MapTile};
use crate::map::navy;
use crate::map::picking::{HoverTarget, MapClick, SelectedHex};
use crate::screens::diplomacy::{can_target_nation, invalid_target_reason};
use crate::state::Screen;
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonProps, ModalProps, ModalStack, Toast};

/// Terrains a prospector can search (web `PROSPECTOR_TERRAIN`).
const PROSPECTOR_TERRAIN: [&str; 5] = ["Hills", "Mountain", "Swamp", "Desert", "Tundra"];

/// Buttons inside the engineer build-choice modal. `Some(kind)` starts the
/// build chain; `None` cancels the prompt (deploy mode stays active).
#[derive(Component, Clone, Copy)]
pub struct EngineerChoiceButton(pub Option<&'static str>);

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
    mut modal_stack: ResMut<ModalStack>,
    mut game_commands: MessageWriter<GameCommand>,
    selections: (
        ResMut<SelectedHex>,
        ResMut<SelectedNavy>,
        ResMut<SelectedUnits>,
        ResMut<SelectedCivilian>,
        ResMut<DeployMode>,
        ResMut<EngineerPrompt>,
    ),
) {
    let (
        mut selected_hex,
        mut selected_navy,
        mut selected_units,
        mut selected_civilian,
        mut deploy,
        mut prompt,
    ) = selections;
    for MapClick(target) in clicks.read() {
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

        // 3. Deploy mode: only clicks on highlighted tiles act; anything
        //    else is ignored and the mode stays armed (web F-004).
        if let Some(state) = deploy.0.as_ref() {
            let Some(tile) = tile else {
                continue;
            };
            if !state.deployable.contains(&(tile.q, tile.r)) {
                continue;
            }
            if state.civ_type == "Engineer" {
                // Popup: what should the engineer build on that tile?
                let handles = widgets::open_modal(
                    &mut commands,
                    &mut modal_stack,
                    &theme,
                    ModalProps {
                        title: format!("Engineer at ({}, {})", tile.q, tile.r),
                        width: Val::Px(340.0),
                    },
                );
                let q = tile.q;
                let r = tile.r;
                let civilian_id = state.civilian_id;
                let redeploy = state.redeploy_from.is_some();
                commands.entity(handles.content).with_children(|content| {
                    content.spawn((
                        Text::new("What should this engineer build?"),
                        theme.font(13.0),
                        TextColor(theme::TEXT),
                    ));
                    content
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(8.0),
                            ..default()
                        })
                        .with_children(|row| {
                            for (label, kind) in [
                                ("Railroad", "railroad"),
                                ("Depot", "depot"),
                                ("Port", "port"),
                            ] {
                                let button =
                                    widgets::spawn_button(row, &theme, ButtonProps::label(label));
                                row.commands()
                                    .entity(button)
                                    .insert(EngineerChoiceButton(Some(kind)));
                            }
                            let cancel =
                                widgets::spawn_button(row, &theme, ButtonProps::label("Cancel"));
                            row.commands()
                                .entity(cancel)
                                .insert(EngineerChoiceButton(None));
                        });
                });
                prompt.0 = Some(EngineerPromptState {
                    civilian_id,
                    redeploy,
                    q,
                    r,
                    modal: handles.root,
                });
                continue;
            }
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
                if let Some(tiles) = vms.map.as_ref() {
                    deploy.0 = Some(compute_deploy_state(
                        civ.id,
                        &civ.civ_type,
                        Some((tile.q, tile.r)),
                        tiles,
                        meta.player_nation,
                    ));
                }
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

// ── Engineer prompt plumbing ─────────────────────────────────────────────

/// Engineer modal buttons: a build kind fires the recall→deploy→build chain;
/// Cancel just dismisses the popup (deploy mode stays active, web parity).
pub fn handle_engineer_choice(
    mut activations: MessageReader<widgets::ButtonActivated>,
    buttons: Query<&EngineerChoiceButton>,
    mut prompt: ResMut<EngineerPrompt>,
    mut modal_stack: ResMut<ModalStack>,
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
        if let Some(kind) = choice.0 {
            game_commands.write(GameCommand::EngineerBuild {
                civilian_id: state.civilian_id as u32,
                q: state.q,
                r: state.r,
                kind,
                recall_first: state.redeploy,
            });
        }
        widgets::close_top_modal(&mut commands, &mut modal_stack);
    }
}

/// Clear the prompt state when its modal was closed by other means
/// (Esc / the ✕ button) — equivalent to choosing Cancel.
pub fn sync_engineer_prompt(mut prompt: ResMut<EngineerPrompt>, nodes: Query<(), With<Node>>) {
    if let Some(state) = prompt.0.as_ref()
        && !nodes.contains(state.modal)
    {
        prompt.0 = None;
    }
}

// ── Esc cascade ──────────────────────────────────────────────────────────

/// Esc cancels, in order: engineer popup (handled by the modal stack) →
/// armed diplomacy action → deploy mode → unit selection → navy selection →
/// tile selection → back to the map screen (from Transport / Diplomacy) →
/// quit. The full-screen overlays gate this system off;
/// `map_hud::screen_hotkeys` owns Esc there.
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
    mut exit: MessageWriter<AppExit>,
) {
    // Modals own Esc (the engineer popup pops first); focused text inputs
    // own the keyboard.
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
    } else if deploy.0.is_some() {
        deploy.0 = None;
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
            "has_railroad": false, "has_depot": false, "has_port": false,
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
