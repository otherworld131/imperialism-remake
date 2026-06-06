use application::CivilianType;
use bevy::prelude::*;

use crate::hex_renderer::{GameStateResource, hex_to_pixel};

const MAP_SPRITE_SIZE: f32 = 30.0;
const HUD_SPRITE_SIZE: f32 = 34.0;

#[derive(Component)]
pub struct CivilianSpriteMarker;

#[derive(Clone, Copy)]
pub struct CivilianVisual {
    name: &'static str,
    asset: &'static str,
}

const CIVILIAN_VISUALS: [CivilianVisual; 7] = [
    CivilianVisual {
        name: "Farmer",
        asset: "civilians/farmer.png",
    },
    CivilianVisual {
        name: "Rancher",
        asset: "civilians/rancher.png",
    },
    CivilianVisual {
        name: "Forester",
        asset: "civilians/forester.png",
    },
    CivilianVisual {
        name: "Miner",
        asset: "civilians/miner.png",
    },
    CivilianVisual {
        name: "Prospector",
        asset: "civilians/prospector.png",
    },
    CivilianVisual {
        name: "Engineer",
        asset: "civilians/engineer.png",
    },
    CivilianVisual {
        name: "Driller",
        asset: "civilians/driller.png",
    },
];

pub fn render_civilians(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    game: Res<GameStateResource>,
) {
    for nation in &game.0.world.nations {
        for (index, civilian) in nation.military.civilians.iter().enumerate() {
            let Some(base_coord) = civilian.position.or_else(|| {
                if nation.id == game.0.human_player_nation {
                    game.0
                        .get_province(nation.capital_province_id)
                        .map(|province| province.capital_tile)
                } else {
                    None
                }
            }) else {
                continue;
            };

            let visual = visual_for_type(civilian.civilian_type);
            let pos = hex_to_pixel(base_coord.q, base_coord.r) + undeployed_offset(index);
            commands.spawn((
                Sprite {
                    image: asset_server.load(visual.asset),
                    custom_size: Some(Vec2::splat(MAP_SPRITE_SIZE)),
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y - 2.0, 4.2),
                CivilianSpriteMarker,
            ));
        }
    }
}

pub fn spawn_civilian_asset_strip(parent: &mut ChildSpawnerCommands, asset_server: &AssetServer) {
    parent
        .spawn((
            Node {
                width: Val::Px(330.0),
                height: Val::Px(58.0),
                position_type: PositionType::Absolute,
                left: Val::Px(18.0),
                top: Val::Px(228.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
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
                strip.spawn((
                    ImageNode::new(asset_server.load(visual.asset)),
                    Node {
                        width: Val::Px(HUD_SPRITE_SIZE),
                        height: Val::Px(HUD_SPRITE_SIZE),
                        ..default()
                    },
                ));
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

fn undeployed_offset(index: usize) -> Vec2 {
    const OFFSETS: [Vec2; 7] = [
        Vec2::new(-18.0, -15.0),
        Vec2::new(0.0, -17.0),
        Vec2::new(18.0, -15.0),
        Vec2::new(-10.0, 5.0),
        Vec2::new(10.0, 5.0),
        Vec2::new(-23.0, 6.0),
        Vec2::new(23.0, 6.0),
    ];
    OFFSETS[index % OFFSETS.len()]
}
