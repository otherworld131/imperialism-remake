//! Diplomatic proposal modal (web `ProposalModal`): opened automatically
//! after turn resolution when proposals addressed to the player remain
//! (war declarations are auto-acknowledged during end turn). Accept /
//! Reject only mutate the proposal list — treaty effects were queued by
//! the backend and resolve through the normal pipeline.

use bevy::prelude::*;

use crate::game::commands::GameCommand;
use crate::game::resources::ProposalPrompt;
use crate::game::vm::ProposalsVm;
use crate::theme::{self, Theme};
use crate::widgets::{self, ButtonActivated, ButtonProps, ModalProps, ModalStack};

#[derive(Component)]
pub struct ProposalContent;

#[derive(Component)]
pub struct ProposalAcceptButton(pub u32);

#[derive(Component)]
pub struct ProposalRejectButton(pub u32);

#[derive(Component)]
pub struct ProposalDismissButton;

/// The open modal's root entity, if any.
#[derive(Resource, Default)]
pub struct ProposalModalState(pub Option<Entity>);

/// Keep the modal in step with [`ProposalPrompt`]: open it when proposals
/// arrive, rebuild its rows when the list changes, close it when emptied.
/// An externally closed modal (Esc / ✕) clears the prompt.
pub fn sync_proposal_modal(
    mut commands: Commands,
    theme: Res<Theme>,
    mut prompt: ResMut<ProposalPrompt>,
    mut state: ResMut<ProposalModalState>,
    mut modal_stack: ResMut<ModalStack>,
    nodes: Query<(), With<Node>>,
    contents: Query<Entity, With<ProposalContent>>,
) {
    // Modal closed by Esc / ✕ → drop the prompt (web "Dismiss").
    if let Some(root) = state.0
        && !nodes.contains(root)
    {
        state.0 = None;
        if prompt.0.is_some() {
            prompt.0 = None;
        }
        return;
    }

    if !prompt.is_changed() {
        return;
    }

    let proposals = prompt.0.as_ref().filter(|p| !p.proposals.is_empty());
    match (proposals, state.0) {
        (None, Some(root)) => {
            // Emptied (last Accept/Reject) → close.
            commands.entity(root).despawn();
            state.0 = None;
        }
        (None, None) => {}
        (Some(proposals), Some(_)) => {
            // Rebuild the rows in place.
            if let Ok(content) = contents.single() {
                commands.entity(content).despawn_children();
                let proposals = proposals.clone();
                let theme_ref = &theme;
                commands.entity(content).with_children(|body| {
                    spawn_rows(body, theme_ref, &proposals);
                });
            }
        }
        (Some(proposals), None) => {
            let handles = widgets::open_modal(
                &mut commands,
                &mut modal_stack,
                &theme,
                ModalProps {
                    title: "Diplomatic Proposals".into(),
                    width: Val::Px(500.0),
                },
            );
            state.0 = Some(handles.root);
            let proposals = proposals.clone();
            commands.entity(handles.content).with_children(|body| {
                body.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(8.0),
                        ..default()
                    },
                    ProposalContent,
                ))
                .with_children(|rows| {
                    spawn_rows(rows, &theme, &proposals);
                });
                // Footer: Dismiss.
                body.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::FlexEnd,
                    border: UiRect::top(Val::Px(1.0)),
                    padding: UiRect::top(Val::Px(8.0)),
                    ..default()
                })
                .with_children(|footer| {
                    let dismiss = widgets::spawn_button(
                        footer,
                        &theme,
                        ButtonProps {
                            label: "Dismiss".into(),
                            font_size: 12.0,
                            ..default()
                        },
                    );
                    footer
                        .commands()
                        .entity(dismiss)
                        .insert(ProposalDismissButton);
                });
            });
        }
    }
}

fn spawn_rows(parent: &mut ChildSpawnerCommands, theme: &Theme, proposals: &ProposalsVm) {
    for proposal in &proposals.proposals {
        let is_war = proposal.proposal_type == "WarDeclaration";
        parent
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.03)),
            ))
            .with_children(|row| {
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                })
                .with_children(|line| {
                    line.spawn((
                        Text::new(proposal.display_text.clone()),
                        theme.font_bold(13.0),
                        TextColor(theme::TEXT),
                    ));
                    let turns = proposal.turns_until_expiry;
                    line.spawn((
                        Text::new(format!(
                            "(expires in {turns} turn{})",
                            if turns == 1 { "" } else { "s" }
                        )),
                        theme.font(11.0),
                        TextColor(theme::TEXT_DIM),
                    ));
                });
                row.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|buttons| {
                    let accept = widgets::spawn_button(
                        buttons,
                        theme,
                        ButtonProps {
                            label: if is_war {
                                "Acknowledge".into()
                            } else {
                                "Accept".into()
                            },
                            font_size: 12.0,
                            ..default()
                        },
                    );
                    buttons
                        .commands()
                        .entity(accept)
                        .insert(ProposalAcceptButton(proposal.index));
                    if !is_war {
                        let reject = widgets::spawn_button(
                            buttons,
                            theme,
                            ButtonProps {
                                label: "Reject".into(),
                                font_size: 12.0,
                                ..default()
                            },
                        );
                        buttons
                            .commands()
                            .entity(reject)
                            .insert(ProposalRejectButton(proposal.index));
                    }
                });
            });
    }
}

pub fn handle_proposal_buttons(
    mut activations: MessageReader<ButtonActivated>,
    accepts: Query<&ProposalAcceptButton>,
    rejects: Query<&ProposalRejectButton>,
    dismisses: Query<(), With<ProposalDismissButton>>,
    mut prompt: ResMut<ProposalPrompt>,
    mut out: MessageWriter<GameCommand>,
) {
    for ButtonActivated(entity) in activations.read() {
        if let Ok(accept) = accepts.get(*entity) {
            out.write(GameCommand::AcceptProposal { index: accept.0 });
        } else if let Ok(reject) = rejects.get(*entity) {
            out.write(GameCommand::RejectProposal { index: reject.0 });
        } else if dismisses.contains(*entity) {
            prompt.0 = None;
        }
    }
}
