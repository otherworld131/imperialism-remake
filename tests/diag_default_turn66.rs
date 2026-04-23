// Quick diagnostic: run the default "imperialism" map for 66 turns in batch-style
// and dump detailed per-nation state focusing on: diplomacy treaties (who has
// consulate/embassy/NAP with whom + personalities), rail/depot layout, engineer
// positions, civilian (forester) placements vs timber tiles, and disconnected
// timber tiles.

use domain::events::TreatyType;
use domain::game_state::new_game_with_seed;
use domain::turn::{connected_provinces, process_turn};
use domain::types::*;

#[test]
#[ignore]
fn diag_default_turn66() {
    let mut game = new_game_with_seed("imperialism", Difficulty::Normal, 0, 0xC0FFEE);
    // Promote human slot to AI
    let human_id = game.human_player_nation;
    if let Some(nation) = game.get_nation_mut(human_id)
        && nation.ai_personality.is_none()
    {
        let p = domain::ai::common::random_personalities(0xDEAD_BEEF, 1)[0];
        nation.ai_personality = Some(p);
        let n = domain::ai::priority_target_count(&game.game_data.game_config, p);
        let t = domain::ai::pick_priority_minor_targets(&game, human_id, n, &[]);
        if let Some(nation) = game.get_nation_mut(human_id) {
            nation.ai_priority_state.priority_minor_targets = t;
        }
    }
    game.observer_mode = true;

    for _ in 0..66 {
        process_turn(&mut game);
    }

    // Personality map (id → (name, personality))
    let mut gp_personality: std::collections::HashMap<NationId, (String, String)> =
        std::collections::HashMap::new();
    for n in &game.nations {
        if n.is_great_power() {
            gp_personality.insert(
                n.id,
                (
                    n.name.clone(),
                    n.ai_personality
                        .map(|p| format!("{:?}", p))
                        .unwrap_or_else(|| "-".into()),
                ),
            );
        }
    }

    println!("=== DIPLOMACY (turn 66 = 1831 Q2) — GP ↔ GP ===");
    let gp_ids: Vec<NationId> = game
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();
    for &a in &gp_ids {
        for &b in &gp_ids {
            if a.0 >= b.0 {
                continue;
            }
            let rel = game.diplomacy.get_relation(a, b);
            let nap = game
                .diplomacy
                .has_treaty(a, b, TreatyType::NonAggressionPact);
            let ali = game.diplomacy.has_treaty(a, b, TreatyType::Alliance);
            let war = rel.is_some_and(|r| r.at_war);
            let score = rel.map(|r| r.score).unwrap_or(0);
            let (na, pa) = &gp_personality[&a];
            let (nb, pb) = &gp_personality[&b];
            if nap || ali || war || score.abs() > 20 {
                println!(
                    "  {:10}({:12}) — {:10}({:12}): score={:+4} {}{}{}",
                    na,
                    pa,
                    nb,
                    pb,
                    score,
                    if ali { "ALLY " } else { "" },
                    if nap { "NAP " } else { "" },
                    if war { "WAR " } else { "" }
                );
            }
        }
    }

    println!("\n=== DIPLOMACY — GP → Minor: consulates/embassies/NAPs ===");
    for &gp in &gp_ids {
        let (name, pers) = &gp_personality[&gp];
        let mut cons = 0usize;
        let mut emb = 0usize;
        let mut naps = 0usize;
        let mut targets: Vec<String> = Vec::new();
        for mn in &game.nations {
            if mn.is_great_power() || mn.province_ids.is_empty() {
                continue;
            }
            let rel = game.diplomacy.get_relation(gp, mn.id);
            if let Some(r) = rel {
                if r.has_consulate {
                    cons += 1;
                }
                if r.has_embassy {
                    emb += 1;
                }
            }
            if game
                .diplomacy
                .has_treaty(gp, mn.id, TreatyType::NonAggressionPact)
            {
                naps += 1;
            }
            if let Some(r) = rel {
                let tag = match (r.has_consulate, r.has_embassy) {
                    (true, true) => "E",
                    (true, false) => "C",
                    _ => continue,
                };
                targets.push(format!("{}({})", mn.name, tag));
            }
        }
        let priority: Vec<String> = game
            .get_nation(gp)
            .unwrap()
            .ai_priority_state
            .priority_minor_targets
            .iter()
            .filter_map(|id| game.get_nation(*id).map(|n| n.name.clone()))
            .collect();
        println!(
            "  {:10}({:12}): C={} E={} NAPs_with_MN={}  targets=[{}]  priority=[{}]",
            name,
            pers,
            cons,
            emb,
            naps,
            targets.join(","),
            priority.join(",")
        );
    }

    println!("\n=== INFRASTRUCTURE — GP rails/depots/engineers ===");
    for &gp in &gp_ids {
        let nation = game.get_nation(gp).unwrap();
        let (name, pers) = &gp_personality[&gp];
        let mut rails = 0;
        let mut depots = 0;
        let mut depot_coords: Vec<domain::hex::HexCoord> = Vec::new();
        let mut rail_coords: Vec<domain::hex::HexCoord> = Vec::new();
        for &pid in &nation.province_ids {
            if let Some(p) = game.get_province(pid) {
                for &c in &p.tiles {
                    if let Some(t) = game.hex_map.get_tile(c) {
                        if t.infrastructure.has_railroad {
                            rails += 1;
                            rail_coords.push(c);
                        }
                        if t.infrastructure.has_depot {
                            depots += 1;
                            depot_coords.push(c);
                        }
                    }
                }
            }
        }
        let engineer_info: Vec<String> = nation
            .civilians
            .iter()
            .filter(|c| c.civilian_type == domain::economy::civilians::CivilianType::Engineer)
            .map(|c| {
                format!(
                    "pos={:?} working={} turns_left={} task={:?}",
                    c.position, c.working, c.turns_remaining, c.build_task
                )
            })
            .collect();
        println!(
            "  {:10}({:12}): rails={} depots={} provinces={} engineers={}: {:?}",
            name,
            pers,
            rails,
            depots,
            nation.province_count(),
            nation
                .civilians
                .iter()
                .filter(|c| c.civilian_type == domain::economy::civilians::CivilianType::Engineer)
                .count(),
            engineer_info
        );
        if nation.province_count() > 0 {
            println!("    rails: {:?}", rail_coords);
            println!("    depots: {:?}", depot_coords);
        }
    }

    println!("\n=== FORESTERS vs TIMBER — per GP ===");
    for &gp in &gp_ids {
        let nation = game.get_nation(gp).unwrap();
        let (name, _) = &gp_personality[&gp];
        let foresters: Vec<_> = nation
            .civilians
            .iter()
            .filter(|c| c.civilian_type == domain::economy::civilians::CivilianType::Forester)
            .collect();
        let connected = connected_provinces(&game, gp);
        let owned: Vec<&domain::map::Province> =
            game.provinces.iter().filter(|p| p.owner == gp).collect();
        let collectable =
            domain::map::infrastructure::collectable_hexes(&game.hex_map, &owned, &connected);
        let mut connected_timber = 0;
        let mut disconnected_timber = 0;
        for &pid in &nation.province_ids {
            if let Some(p) = game.get_province(pid) {
                for &c in &p.tiles {
                    if let Some(t) = game.hex_map.get_tile(c)
                        && t.resource_deposit() == Some(ResourceType::Timber)
                    {
                        if collectable.contains(&c) {
                            connected_timber += 1;
                        } else {
                            disconnected_timber += 1;
                        }
                    }
                }
            }
        }
        let forester_info: Vec<String> = foresters
            .iter()
            .map(|c| format!("pos={:?} working={}", c.position, c.working))
            .collect();
        println!(
            "  {:10}: timber_connected={} timber_disconnected={} foresters={}: {:?}",
            name,
            connected_timber,
            disconnected_timber,
            foresters.len(),
            forester_info
        );
    }
}
