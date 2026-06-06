use application::CivilianType;
use bevy::prelude::*;

use crate::hex_renderer::{GameStateResource, hex_to_pixel};

const BADGE_RADIUS: f32 = 10.0;

#[derive(Component)]
pub struct CivilianBadgeMarker;

#[derive(Clone, Copy)]
pub struct CivilianVisual {
    name: &'static str,
    code: &'static str,
    fill: Color,
    ink: Color,
}

const CIVILIAN_VISUALS: [CivilianVisual; 7] = [
    CivilianVisual {
        name: "Farmer",
        code: "FA",
        fill: Color::srgb(0.54, 0.68, 0.24),
        ink: Color::srgb(0.08, 0.10, 0.04),
    },
    CivilianVisual {
        name: "Rancher",
        code: "RA",
        fill: Color::srgb(0.72, 0.48, 0.22),
        ink: Color::srgb(0.10, 0.06, 0.03),
    },
    CivilianVisual {
        name: "Forester",
        code: "FO",
        fill: Color::srgb(0.12, 0.48, 0.26),
        ink: Color::srgb(0.86, 0.96, 0.78),
    },
    CivilianVisual {
        name: "Miner",
        code: "MI",
        fill: Color::srgb(0.44, 0.45, 0.43),
        ink: Color::srgb(0.96, 0.92, 0.76),
    },
    CivilianVisual {
        name: "Prospector",
        code: "PR",
        fill: Color::srgb(0.76, 0.62, 0.24),
        ink: Color::srgb(0.12, 0.09, 0.03),
    },
    CivilianVisual {
        name: "Engineer",
        code: "EN",
        fill: Color::srgb(0.38, 0.48, 0.58),
        ink: Color::srgb(0.92, 0.96, 1.00),
    },
    CivilianVisual {
        name: "Driller",
        code: "DR",
        fill: Color::srgb(0.28, 0.24, 0.20),
        ink: Color::srgb(0.98, 0.80, 0.42),
    },
];

pub fn render_deployed_civilians(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    game: Res<GameStateResource>,
) {
    let badge_mesh = meshes.add(Circle::new(BADGE_RADIUS));
    let ring_mesh = meshes.add(Circle::new(BADGE_RADIUS + 2.0).to_ring(2.0));
    let ring_material = materials.add(Color::srgba(0.04, 0.04, 0.035, 0.95));

    for nation in &game.0.world.nations {
        for civilian in &nation.military.civilians {
            let Some(coord) = civilian.position else {
                continue;
            };
            let visual = visual_for_type(civilian.civilian_type);
            let pos = hex_to_pixel(coord.q, coord.r);
            let fill_material = materials.add(visual.fill);

            commands.spawn((
                Mesh2d(ring_mesh.clone()),
                MeshMaterial2d(ring_material.clone()),
                Transform::from_xyz(pos.x + 7.0, pos.y + 7.0, 4.0),
                CivilianBadgeMarker,
            ));
            commands.spawn((
                Mesh2d(badge_mesh.clone()),
                MeshMaterial2d(fill_material),
                Transform::from_xyz(pos.x + 7.0, pos.y + 7.0, 4.1),
                CivilianBadgeMarker,
            ));
            commands.spawn((
                Text2d::new(visual.code),
                TextFont {
                    font_size: 8.5,
                    ..default()
                },
                TextColor(visual.ink),
                TextLayout::new_with_justify(Justify::Center),
                Transform::from_xyz(pos.x + 7.0, pos.y + 6.2, 4.3),
                CivilianBadgeMarker,
            ));
        }
    }
}

pub fn spawn_civilian_asset_strip(parent: &mut ChildSpawnerCommands) {
    parent
        .spawn((
            Node {
                width: Val::Px(330.0),
                height: Val::Px(48.0),
                position_type: PositionType::Absolute,
                left: Val::Px(18.0),
                top: Val::Px(228.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(7.0),
                padding: UiRect::horizontal(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.025, 0.027, 0.024, 0.80)),
        ))
        .with_children(|strip| {
            strip.spawn((
                Text::new("Civilians"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.86, 0.84, 0.74)),
            ));

            for visual in CIVILIAN_VISUALS {
                strip
                    .spawn((
                        Node {
                            width: Val::Px(26.0),
                            height: Val::Px(26.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(Val::Px(2.0)),
                            ..default()
                        },
                        BorderColor::all(Color::srgba(0.04, 0.04, 0.035, 0.95)),
                        BackgroundColor(visual.fill),
                    ))
                    .with_children(|badge| {
                        badge.spawn((
                            Text::new(visual.code),
                            TextFont {
                                font_size: 10.0,
                                ..default()
                            },
                            TextColor(visual.ink),
                        ));
                    });
            }
        });
}

fn visual_for_type(civilian_type: CivilianType) -> CivilianVisual {
    let name = match civilian_type {
        CivilianType::Farmer => "Farmer",
        CivilianType::Rancher => "Rancher",
        CivilianType::Forester => "Forester",
        CivilianType::Miner => "Miner",
        CivilianType::Prospector => "Prospector",
        CivilianType::Engineer => "Engineer",
        CivilianType::Driller => "Driller",
    };
    CIVILIAN_VISUALS
        .iter()
        .copied()
        .find(|visual| visual.name == name)
        .expect("all civilian types have a presentation visual")
}
