use domain::economy::buildings::BuildingType;
use domain::economy::ledger::{CashFlow, CashSink, CashSource, FlowCategory, ResourceFlow};
use domain::game_state::GameState;
use domain::hex::HexCoord;
use domain::map::HexMap;
use domain::map::infrastructure;
use domain::military::units::ArmyUnitType;
use domain::nation::Nation;
use domain::turn::{TurnReport, calculate_score};
use domain::types::*;

// ── Colored output helpers ────────────────────────────────────────

pub(crate) fn color_green(s: &str) -> String {
    format!("\x1b[92m{}\x1b[0m", s)
}

pub(crate) fn color_red(s: &str) -> String {
    format!("\x1b[91m{}\x1b[0m", s)
}

pub(crate) fn color_yellow(s: &str) -> String {
    format!("\x1b[93m{}\x1b[0m", s)
}

pub(crate) fn color_bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}

// ── Formatting helpers ────────────────────────────────────────────

/// Format a number with comma separators (e.g. 1290 -> "1,290").
pub(crate) fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

/// Format a list of casualty unit types into a readable string.
/// Groups identical types: e.g., "2 Militia, 1 Regulars destroyed"
pub(crate) fn format_casualties(casualties: &[ArmyUnitType]) -> String {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for ut in casualties {
        *counts.entry(format!("{:?}", ut)).or_insert(0) += 1;
    }
    let parts: Vec<String> = counts
        .iter()
        .map(|(name, count)| format!("{} {}", count, name))
        .collect();
    format!("{} destroyed", parts.join(", "))
}

/// Return the player's rank (1-based) and total score from the turn report scores.
pub(crate) fn score_summary(
    scores: &[(NationId, String, u32)],
    player_id: NationId,
) -> Option<(usize, u32)> {
    // Scores are already sorted descending by total.
    for (i, (nid, _, total)) in scores.iter().enumerate() {
        if *nid == player_id {
            return Some((i + 1, *total));
        }
    }
    None
}

pub(crate) fn terrain_char(terrain: TerrainType) -> char {
    match terrain {
        TerrainType::Grassland => 'G',
        TerrainType::Hills => 'H',
        TerrainType::Forest => 'F',
        TerrainType::Mountain => 'M',
        TerrainType::Desert => 'D',
        TerrainType::Swamp => 'S',
        TerrainType::Tundra => 'T',
        TerrainType::Sea => '~',
    }
}

pub(crate) fn nation_color_code(nation: &Nation) -> &str {
    use domain::nation::NationColor;
    match nation.color {
        NationColor::Yellow => "\x1b[93m",
        NationColor::Orange => "\x1b[33m",
        NationColor::LightBlue => "\x1b[96m",
        NationColor::Red => "\x1b[91m",
        NationColor::Green => "\x1b[92m",
        NationColor::Purple => "\x1b[95m",
        NationColor::Blue => "\x1b[94m",
        NationColor::Gray => "\x1b[37m",
        NationColor::Brown => "\x1b[33m",
        NationColor::Pink => "\x1b[95m",
        NationColor::Teal => "\x1b[36m",
        NationColor::Olive => "\x1b[32m",
        NationColor::Maroon => "\x1b[31m",
        NationColor::Navy => "\x1b[34m",
        NationColor::Cyan => "\x1b[36m",
        NationColor::Lime => "\x1b[92m",
        NationColor::Coral => "\x1b[91m",
        NationColor::Lavender => "\x1b[35m",
        NationColor::Tan => "\x1b[33m",
        NationColor::Salmon => "\x1b[91m",
        NationColor::Khaki => "\x1b[93m",
        NationColor::Indigo => "\x1b[34m",
    }
}

// ── Recruitment capacity (display-only helper) ────────────────────

pub(crate) fn max_recruitment_capacity(player: &Nation) -> u32 {
    let province_count = player.province_count() as u32;
    let has_expanded_capitol = player
        .economy.buildings
        .iter()
        .any(|b| b.building_type == BuildingType::Capitol && b.capacity > 1);
    let per_province = if has_expanded_capitol { 3 } else { 4 };
    province_count / per_province
}

fn human_player(game: &GameState) -> Option<&Nation> {
    match game.get_nation(game.human_player_nation) {
        Some(player) => Some(player),
        None => {
            println!("  Internal error: human player nation is missing from game state.");
            None
        }
    }
}

// ── Print functions ───────────────────────────────────────────────

pub(crate) fn print_prompt(game: &GameState) {
    let Some(nation) = human_player(game) else {
        return;
    };
    print!(
        "  [{} | {} | {}] > ",
        nation.name, game.turn, nation.economy.treasury
    );
}

pub(crate) fn print_status(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };
    println!("  Playing as: {} (Great Power)", player.name);
    println!("  Turn: {} (Year {})", game.turn, game.turn.year());
    println!("  Treasury: {}", player.economy.treasury);
    println!("  Provinces: {}", player.province_count());
    println!("  Difficulty: {:?}", game.difficulty);
}

pub(crate) fn print_warehouse(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };
    println!("  WAREHOUSE:");

    let mut has_any = false;

    // Resources
    let resources = [
        ResourceType::Timber,
        ResourceType::Coal,
        ResourceType::Iron,
        ResourceType::Cotton,
        ResourceType::Wool,
        ResourceType::Grain,
        ResourceType::Fruit,
        ResourceType::Livestock,
        ResourceType::Horses,
        ResourceType::Oil,
        ResourceType::Gold,
        ResourceType::Gems,
    ];
    println!("    Raw Resources:");
    for r in &resources {
        let amt = player.resource_amount(*r);
        if amt > 0 {
            println!("      {:?}: {}", r, amt);
            has_any = true;
        }
    }

    // Materials
    let materials = [
        MaterialType::Lumber,
        MaterialType::Steel,
        MaterialType::Fabric,
        MaterialType::Paper,
        MaterialType::Arms,
        MaterialType::CannedFood,
    ];
    println!("    Materials:");
    for m in &materials {
        let amt = player.economy.materials.get(m).copied().unwrap_or(0);
        if amt > 0 {
            println!("      {:?}: {}", m, amt);
            has_any = true;
        }
    }

    // Goods
    let goods = [
        GoodsType::Furniture,
        GoodsType::Clothing,
        GoodsType::Hardware,
    ];
    println!("    Finished Goods:");
    for g in &goods {
        let amt = player.economy.goods.get(g).copied().unwrap_or(0);
        if amt > 0 {
            println!("      {:?}: {}", g, amt);
            has_any = true;
        }
    }

    if !has_any {
        println!("    (empty — end your first turn to begin gathering resources)");
    }
}

pub(crate) fn print_provinces(game: &GameState) {
    println!("  YOUR PROVINCES:");
    for province in &game.world.provinces {
        if province.owner == game.human_player_nation {
            let mut terrain_counts: std::collections::BTreeMap<char, u32> =
                std::collections::BTreeMap::new();
            for tile_coord in &province.tiles {
                if let Some(tile) = game.world.hex_map.get_tile(*tile_coord) {
                    *terrain_counts
                        .entry(terrain_char(tile.terrain()))
                        .or_insert(0) += 1;
                }
            }
            let terrain_summary: String = terrain_counts
                .iter()
                .map(|(ch, count)| format!("{}x{}", count, ch))
                .collect::<Vec<_>>()
                .join(" ");

            let settlement = match province.settlement_level {
                domain::map::SettlementLevel::Hamlet => "Hamlet",
                domain::map::SettlementLevel::Village => "Village",
                domain::map::SettlementLevel::Town => "Town",
            };
            println!(
                "    {} ({} tiles, {}) [{}]",
                province.name,
                province.tile_count(),
                settlement,
                terrain_summary
            );
        }
    }
}

pub(crate) fn print_nations(game: &GameState) {
    println!("  GREAT POWERS:");
    for nation in game.great_powers() {
        let marker = if nation.id == game.human_player_nation {
            " ◄ YOU"
        } else {
            ""
        };
        println!(
            "    {:>2}. {:<12} {:>2} provinces  {}{}",
            nation.id.0,
            nation.name,
            nation.province_count(),
            nation.economy.treasury,
            marker
        );
    }
    println!();
    println!("  MINOR NATIONS:");
    for nation in game.minor_nations() {
        println!(
            "    {:>2}. {:<12} {:>2} provinces",
            nation.id.0,
            nation.name,
            nation.province_count(),
        );
    }
}

pub(crate) fn print_buildings(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };
    println!("  BUILDINGS:");
    if player.economy.buildings.is_empty() {
        println!("    (none)");
    } else {
        for b in &player.economy.buildings {
            let pending = if b.pending_capacity > 0 {
                format!(
                    " (+{} in {} turn{})",
                    b.pending_capacity,
                    b.turns_until_upgrade,
                    if b.turns_until_upgrade != 1 { "s" } else { "" }
                )
            } else {
                String::new()
            };
            println!(
                "    {:?}: capacity {}{}",
                b.building_type, b.capacity, pending
            );
        }
    }
    println!();
    println!(
        "  Workers: {} untrained, {} trained, {} expert",
        player.economy.labor.untrained, player.economy.labor.trained, player.economy.labor.expert
    );
}

pub(crate) fn print_population(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };

    let untrained = player.economy.labor.untrained;
    let trained = player.economy.labor.trained;
    let expert = player.economy.labor.expert;
    let total = player.economy.labor.total_workers();

    println!("  POPULATION:");
    println!("    Untrained workers: {}", untrained);
    println!("    Trained workers:   {}", trained);
    println!("    Expert workers:    {}", expert);
    println!("    Total:             {}", total);
    println!();

    // Food requirement
    let food_needed = total;
    let grain = player.resource_amount(ResourceType::Grain);
    let fruit = player.resource_amount(ResourceType::Fruit);
    let livestock = player.resource_amount(ResourceType::Livestock);
    let total_food = grain + fruit + livestock;

    println!("  FOOD:");
    println!(
        "    Available: {} grain, {} fruit, {} livestock ({} total)",
        grain, fruit, livestock, total_food
    );
    println!("    Requirement: {} per turn (1 per worker)", food_needed);
    if total_food >= food_needed {
        println!("    Status: SURPLUS (+{})", total_food - food_needed);
    } else {
        println!(
            "    Status: DEFICIT (-{}) -- workers will starve!",
            food_needed - total_food
        );
    }
    println!();

    // Recruitment capacity
    let max_recruits = max_recruitment_capacity(player);
    println!("  RECRUITMENT:");
    println!(
        "    Max workers: {} (based on {} provinces)",
        max_recruits,
        player.province_count()
    );
    println!("    Current workers: {} / {}", total, max_recruits);
    let has_expanded_capitol = player
        .economy.buildings
        .iter()
        .any(|b| b.building_type == BuildingType::Capitol && b.capacity > 1);
    if has_expanded_capitol {
        println!("    Capitol expanded: 1 worker per 3 provinces");
    } else {
        println!("    Capitol base: 1 worker per 4 provinces (expand Capitol for 1 per 3)");
    }
}

pub(crate) fn print_tech(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };
    let year = game.turn.year();

    // Show already researched technologies
    println!("  RESEARCHED TECHNOLOGIES:");
    if player.researched_techs.is_empty() {
        println!("    (none)");
    } else {
        for tech_id in &player.researched_techs {
            if let Some(tech) = game.game_data.tech_tree.get(*tech_id) {
                println!("    [x] {}", tech.name);
            }
        }
    }
    println!();

    // Show available technologies
    let available = game
        .game_data
        .tech_tree
        .available_techs(&player.researched_techs, year);
    println!("  AVAILABLE FOR RESEARCH (year {}):", year);
    if available.is_empty() {
        println!("    (none available this year)");
    } else {
        for tech in &available {
            println!("    [ ] {} (cost: {})", tech.name, tech.cost);
        }
    }
    println!();
    println!("  Use 'research <name>' to research a technology.");
}

pub(crate) fn print_trade(game: &GameState) {
    use domain::economy::trade;

    let Some(player) = human_player(game) else {
        return;
    };
    let cargo_capacity = player.total_cargo_capacity(&game.game_data);

    println!("  TRADE STATUS:");
    println!(
        "    Merchant fleet: {} ships, {} cargo holds",
        player.merchant_ship_count(),
        cargo_capacity
    );
    println!();

    let offers = trade::generate_minor_nation_offers(&game.world.nations, &game.world.provinces, &game.world.hex_map);

    if offers.is_empty() {
        println!("  No trade offerings available from Minor Nations.");
        return;
    }

    // Group offers by seller
    let mut by_seller: std::collections::BTreeMap<String, (NationId, Vec<&trade::TradeOffer>)> =
        std::collections::BTreeMap::new();
    for offer in &offers {
        let seller_name = game
            .get_nation(offer.seller)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("Nation {}", offer.seller.0));
        let entry = by_seller
            .entry(seller_name)
            .or_insert((offer.seller, Vec::new()));
        entry.1.push(offer);
    }

    println!("  MINOR NATION TRADE OFFERINGS:");
    for (name, (seller_id, nation_offers)) in &by_seller {
        // Check consulate status
        let has_consulate = game
            .world.diplomacy
            .get_relation(game.human_player_nation, *seller_id)
            .is_some_and(|r| r.has_consulate);
        let status = if has_consulate {
            color_green("[Consulate]")
        } else {
            color_red("[No Consulate]")
        };
        println!("    {} {}:", name, status);
        let mut sorted_offers: Vec<_> = nation_offers.iter().collect();
        sorted_offers.sort_by_key(|o| format!("{:?}", o.resource));
        for offer in sorted_offers {
            println!(
                "      {:?}: {} available at {} each",
                offer.resource, offer.quantity, offer.price_per_unit
            );
        }
    }
    println!();

    // Show active subsidies
    if !player.diplomacy.trade_subsidies.is_empty() {
        println!("  ACTIVE TRADE SUBSIDIES:");
        let mut subsidy_entries: Vec<_> = player.diplomacy.trade_subsidies.iter().collect();
        subsidy_entries.sort_by_key(|(nid, _)| nid.0);
        for (target_id, amount) in subsidy_entries {
            let target_name = game
                .get_nation(*target_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("Nation {}", target_id.0));
            println!("    {:<12} {} per turn", target_name, amount);
        }
        let total_subsidy: Money = player
            .diplomacy.trade_subsidies
            .values()
            .copied()
            .fold(Money::ZERO, |acc, v| acc + v);
        println!("    Total subsidy cost: {} per turn", total_subsidy);
        println!();
    }

    // Show cargo utilization: estimate how many holds are used by current trade volume
    if cargo_capacity > 0 {
        // Count total quantity being traded (sum of last turn's transactions for this player).
        // Exclude world-market auto-sells (NationId(0)) and manufactured-goods sentinel entries.
        let cargo_used: u32 = player
            .archives.trade_history
            .iter()
            .filter(|th| th.turn == game.turn || th.turn.0 + 1 == game.turn.0)
            .filter(|th| th.partner != player.id)
            .filter(|th| th.partner.0 != 0)
            .filter(|th| th.commodity_label == format!("{:?}", th.resource))
            .map(|th| th.quantity)
            .sum();
        let cargo_used = cargo_used.min(cargo_capacity);
        println!("  Cargo: {}/{} holds used", cargo_used, cargo_capacity);
        println!("  (Trade is auto-resolved each turn — requires consulate + cargo capacity)");
    } else {
        println!(
            "  {} No cargo capacity! Build merchant ships to enable trade.",
            color_red("WARNING:")
        );
    }

    // Show recent trade history (last 10 entries)
    if !player.archives.trade_history.is_empty() {
        println!();
        println!("  RECENT TRADE HISTORY:");
        let history_len = player.archives.trade_history.len();
        let start = history_len.saturating_sub(10);
        for entry in &player.archives.trade_history[start..] {
            let partner_name = game
                .get_nation(entry.partner)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("Nation {}", entry.partner.0));
            println!(
                "    {} — {:?} x{} with {} for {}",
                entry.turn, entry.resource, entry.quantity, partner_name, entry.total_cost
            );
        }
    }
}

pub(crate) fn print_civilians(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };
    if player.military.civilians.is_empty() {
        println!("  No civilian units. Use 'hire <type>' to hire one.");
        println!("  Types: prospector, miner, engineer, farmer, rancher, forester, driller");
        return;
    }
    println!("  CIVILIAN UNITS:");
    for (i, c) in player.military.civilians.iter().enumerate() {
        let pos_str = match c.position {
            Some(coord) => format!("({}, {})", coord.q, coord.r),
            None => "Undeployed".to_string(),
        };
        let status_str = if c.working {
            format!("Working ({} turns left)", c.turns_remaining)
        } else {
            "Idle".to_string()
        };
        println!(
            "    [{}] {} — {} — {}",
            i, c.civilian_type, pos_str, status_str
        );
    }
}

pub(crate) fn print_scores(game: &GameState) {
    println!("  NATION SCORES:");
    println!(
        "    {:<12} {:>6} {:>6} {:>6} {:>6} {:>6} {:>8}",
        "Nation", "Mil", "Labor", "Trans", "Diplo", "Prov", "TOTAL"
    );
    println!("    {}", "-".repeat(58));
    let mut scores: Vec<_> = game
        .great_powers()
        .iter()
        .map(|n| {
            let s = calculate_score(n, &game.game_data);
            (n.id, n.name.clone(), s)
        })
        .collect();
    scores.sort_by(|a, b| b.2.total.cmp(&a.2.total));
    for (id, name, s) in &scores {
        let marker = if *id == game.human_player_nation {
            " ◄"
        } else {
            ""
        };
        println!(
            "    {:<12} {:>6} {:>6} {:>6} {:>6} {:>6} {:>8}{}",
            name,
            s.military_score,
            s.labor_score,
            s.transport_score,
            s.diplomatic_score,
            s.province_score,
            s.total,
            marker
        );
    }
}

pub(crate) fn print_diplomacy(game: &GameState) {
    let player_id = game.human_player_nation;
    let standing = game.world.diplomacy.get_standing(player_id);
    let Some(player) = human_player(game) else {
        return;
    };

    println!("  DIPLOMATIC STATUS (Standing: {})", standing);
    println!();

    // Show Great Power relations
    println!("  GREAT POWERS:");
    for gp in game.great_powers() {
        if gp.id == player_id {
            continue;
        }
        let status = match game.world.diplomacy.get_relation(player_id, gp.id) {
            Some(rel) => {
                if rel.at_war {
                    "AT WAR".to_string()
                } else if rel.has_treaty(domain::events::TreatyType::Alliance) {
                    format!("Allied (score: {})", rel.score)
                } else {
                    format!("Neutral (score: {})", rel.score)
                }
            }
            None => "No contact".to_string(),
        };
        println!("    {:<12} {}", gp.name, status);
    }
    println!();

    // Show Minor Nation relations
    println!("  MINOR NATIONS:");
    for mn in game.minor_nations() {
        let status = match game.world.diplomacy.get_relation(player_id, mn.id) {
            Some(rel) => {
                if rel.at_war {
                    "AT WAR".to_string()
                } else if rel.has_treaty(domain::events::TreatyType::NonAggressionPact) {
                    format!("Pact + Embassy (score: {})", rel.score)
                } else if rel.has_embassy {
                    format!("Embassy (score: {})", rel.score)
                } else if rel.has_consulate {
                    format!("Consulate (score: {})", rel.score)
                } else {
                    format!("No relations (score: {})", rel.score)
                }
            }
            None => "No relations".to_string(),
        };
        let subsidy_info = player
            .diplomacy.trade_subsidies
            .get(&mn.id)
            .filter(|s| **s != Money::ZERO)
            .map(|s| format!(" [Subsidy: {}/turn]", s))
            .unwrap_or_default();
        println!("    {:<12} {}{}", mn.name, status, subsidy_info);
    }
}

pub(crate) fn print_help() {
    println!("  {}", color_bold("ECONOMY:"));
    println!("    warehouse         — Show your resource warehouse");
    println!("    buildings         — Show your buildings and workers");
    println!("    population / pop  — Show population, food balance, recruitment capacity");
    println!("    transport         — Show your transport system (freight cars, capacity)");
    println!("    trade             — Show Minor Nation trade offerings and subsidies");
    println!("    subsidy <n> <$>   — Set trade subsidy with Minor Nation (0 to remove)");
    println!("    build <building>  — Build a new mill or factory");
    println!("    expand <building> — Expand an existing building's capacity");
    println!("    recruit           — Recruit an untrained worker");
    println!("    train             — Train an untrained worker to trained");
    println!("    build car         — Build a freight car");
    println!("    build ship <type> — Build a merchant ship (trader, indiaman)");
    println!("    fleet             — Show your merchant fleet");
    println!("    sell <res> <qty>  — Sell resources at base price");
    println!();
    println!("  {}", color_bold("MILITARY:"));
    println!("    military / army   — Show your army units and their stats");
    println!("    build unit <type> — Build a military unit");
    println!("    upgrade <index>   — Upgrade army unit #i to its next type ($500)");
    println!("    move <i> <prov>   — Move army unit #i to a province you own");
    println!("    attack <nation>   — Order an attack on a nation you are at war with");
    println!("    navy              — Show your warship fleet");
    println!("    build warship <t> — Build a warship (frigate, ship-of-the-line)");
    println!("    blockade <nation> — Show blockade status against an enemy nation");
    println!("    produce arms      — Convert 1 steel into 1 arms");
    println!();
    println!("  {}", color_bold("DIPLOMACY:"));
    println!("    diplomacy         — Show diplomatic relations with all nations");
    println!("    consulate <name>  — Build a trade consulate with a Minor Nation ($500)");
    println!("    embassy <name>    — Build an embassy with a Minor Nation ($5,000)");
    println!("    pact <name>       — Propose non-aggression pact with a Minor Nation");
    println!("    alliance <name>   — Propose alliance with a Great Power");
    println!("    grant <name> <$>  — Send cash grant to improve relations");
    println!("    war <name>        — Declare war on a nation");
    println!("    peace <name>      — Propose peace with a nation you are at war with");
    println!();
    println!("  {}", color_bold("CIVILIANS:"));
    println!("    civilians         — List your civilian units (type, position, status)");
    println!("    hire <type>       — Hire a civilian specialist");
    println!("    deploy <i> <prov> — Deploy civilian #i to a province to start working");
    println!();
    println!("  {}", color_bold("TECHNOLOGY:"));
    println!("    tech              — Show technology tree status");
    println!("    research <name>   — Research a technology by name");
    println!();
    println!("  {}", color_bold("MAP:"));
    println!("    map               — Show the world map");
    println!("    provinces         — Show your provinces");
    println!("    infra             — Show infrastructure (railroads, depots, ports)");
    println!("    info <name>       — Show detailed info about any nation");
    println!("    nations           — Show all nations");
    println!("    score             — Show scores for all Great Powers");
    println!();
    println!("  {}", color_bold("INFRASTRUCTURE:"));
    println!("    build railroad    — Build a railroad in your capital province");
    println!("    build depot       — Build a depot on your capital tile ($2,000)");
    println!("    build port        — Build a port on a coastal tile ($3,000)");
    println!("    build fort [prov] — Build/upgrade a fort (capital if no province given)");
    println!();
    println!("  {}", color_bold("GAME:"));
    println!("    [Enter] / turn    — End turn (gather resources, advance time)");
    println!("    orders / pending  — Show pending orders before turn end");
    println!("    auto <turns>      — Fast-forward N turns with minimal output");
    println!("    overview          — Comprehensive empire overview");
    println!("    history           — Show timeline of major events");
    println!("    save              — Save the current game (shows existing saves)");
    println!("    load              — List saved games");
    println!("    load <filename>   — Load a saved game");
    println!("    delete <filename> — Delete a saved game");
    println!("    saveinfo <file>   — Show save file metadata without loading");
    println!("    quicksave / qs    — Quick save to quicksave.json");
    println!("    quickload / ql    — Quick load from quicksave.json");
    println!("    help              — Show this help");
    println!("    quit              — Exit the game");
}

pub(crate) fn print_legend() {
    println!("  Terrain: G=Grassland F=Forest H=Hills M=Mountain ~=Sea");
    println!("           D=Desert S=Swamp T=Tundra  ★=Capital");
}

pub(crate) fn print_overview(game: &GameState) {
    let player_id = game.human_player_nation;
    let player = match game.get_nation(player_id) {
        Some(n) => n,
        None => return,
    };

    let year = game.turn.year();
    let quarter = game.turn.quarter();

    // Count province types
    let total_provinces = player.province_count();
    let homeland_provinces = game
        .world.provinces
        .iter()
        .filter(|p| p.owner == player_id)
        .count();

    // Population breakdown
    let untrained = player.economy.labor.untrained;
    let trained = player.economy.labor.trained;
    let expert = player.economy.labor.expert;
    let total_workers = player.economy.labor.total_workers();

    // Army
    let army_count = player.military.army.len();
    let army_fp = {
        let fp = player.total_military_firepower();
        if fp == 0.0 { 0.0 } else { fp }
    };

    // Civilians
    let civilian_count = player.military.civilians.len();
    let mut civ_types: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for c in &player.military.civilians {
        *civ_types.entry(format!("{}", c.civilian_type)).or_insert(0) += 1;
    }

    // Freight
    let freight_cars = player.military.transport.freight_cars;
    let freight_capacity = player.military.transport.total_capacity();

    // Buildings
    let standard_count = player
        .economy.buildings
        .iter()
        .filter(|b| {
            !matches!(
                b.building_type,
                BuildingType::LumberMill
                    | BuildingType::SteelMill
                    | BuildingType::TextileMill
                    | BuildingType::FurnitureFactory
                    | BuildingType::HardwareFactory
                    | BuildingType::ClothingFactory
            )
        })
        .count();
    let mill_count = player
        .economy.buildings
        .iter()
        .filter(|b| {
            matches!(
                b.building_type,
                BuildingType::LumberMill | BuildingType::SteelMill | BuildingType::TextileMill
            )
        })
        .count();
    let factory_count = player
        .economy.buildings
        .iter()
        .filter(|b| {
            matches!(
                b.building_type,
                BuildingType::FurnitureFactory
                    | BuildingType::HardwareFactory
                    | BuildingType::ClothingFactory
            )
        })
        .count();

    // Tech
    let researched_count = player.researched_techs.len();
    let total_techs = game.game_data.tech_tree.total_tech_count();

    // Score
    let score = calculate_score(player, &game.game_data);
    let mut all_scores: Vec<_> = game
        .great_powers()
        .iter()
        .map(|n| (n.id, calculate_score(n, &game.game_data).total))
        .collect();
    all_scores.sort_by(|a, b| b.1.cmp(&a.1));
    let rank = all_scores
        .iter()
        .position(|(nid, _)| *nid == player_id)
        .map(|i| i + 1)
        .unwrap_or(0);
    let gp_count = all_scores.len();

    let title = format!(
        "EMPIRE OF {} \u{2014} Year {} Q{}",
        player.name.to_uppercase(),
        year,
        quarter
    );

    println!("  {}", "\u{2550}".repeat(44));
    println!("  {}", color_bold(&title));
    println!("  {}", "\u{2550}".repeat(44));
    println!(
        "  Treasury:     ${}",
        format_number(player.economy.treasury.as_dollars() as u32)
    );
    println!("  Provinces:    {}", homeland_provinces);
    println!(
        "  Population:   {} workers ({} untrained, {} trained, {} expert)",
        total_workers, untrained, trained, expert
    );
    println!("  Army:         {} units (FP: {:.1})", army_count, army_fp);
    if civilian_count > 0 {
        let civ_detail: Vec<String> = civ_types
            .iter()
            .map(|(t, c)| format!("{} {}", c, t))
            .collect();
        println!(
            "  Civilians:    {} ({})",
            civilian_count,
            civ_detail.join(", ")
        );
    } else {
        println!("  Civilians:    0");
    }
    println!(
        "  Freight:      {} cars (capacity: {})",
        freight_cars, freight_capacity
    );
    println!(
        "  Buildings:    {} standard + {} mills + {} factories",
        standard_count, mill_count, factory_count
    );
    println!(
        "  Technologies: {} of {} researched",
        researched_count, total_techs
    );
    println!(
        "  Score:        {} (#{} of {})",
        format_number(score.total),
        rank,
        gp_count
    );
    println!("  {}", "\u{2550}".repeat(44));

    // Suppress unused variable warnings
    let _ = total_provinces;
}

pub(crate) fn print_history(game: &GameState) {
    if game.archive.history.is_empty() {
        println!("  No major events recorded yet.");
        return;
    }

    println!("  {}", color_bold("HISTORY (last 20 events):"));
    println!();

    let start = if game.archive.history.len() > 20 {
        game.archive.history.len() - 20
    } else {
        0
    };

    for (turn, event) in &game.archive.history[start..] {
        println!(
            "  {} Q{} (Turn {}): {}",
            turn.year(),
            turn.quarter(),
            turn.0,
            game.render_history_event(event)
        );
    }
}

pub(crate) fn print_pending_orders(game: &GameState) {
    let player_id = game.human_player_nation;
    let mut has_orders = false;

    // Pending attacks
    let player_attacks: Vec<_> = game
        .transient.pending_attacks
        .iter()
        .filter(|(nid, _)| *nid == player_id)
        .collect();
    if !player_attacks.is_empty() {
        println!("  PENDING ATTACKS:");
        for (_, province_id) in &player_attacks {
            let prov_name = game
                .get_province(*province_id)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            println!("    -> Attack on {}", prov_name);
        }
        has_orders = true;
    }

    // Pending unit movements
    let player_moves: Vec<_> = game
        .transient.pending_moves
        .iter()
        .filter(|(nid, _, _)| *nid == player_id)
        .collect();
    if !player_moves.is_empty() {
        println!("  PENDING UNIT MOVEMENTS:");
        for (_, unit_id, dest_id) in &player_moves {
            let unit_desc = game
                .get_nation(player_id)
                .and_then(|n| n.military.army.iter().find(|u| u.id == *unit_id))
                .map(|u| format!("{:?}", u.unit_type))
                .unwrap_or_else(|| format!("Unit#{}", unit_id.0));
            let dest_name = game
                .get_province(*dest_id)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            println!("    -> {} moving to {}", unit_desc, dest_name);
        }
        has_orders = true;
    }

    // Working civilians
    if let Some(player) = game.get_nation(player_id) {
        if let Some(pending) = game.transient.pending_economy_orders.get(&player_id)
            && !pending.is_empty()
        {
            println!("  RESERVED ECONOMY ORDERS:");
            for order in pending {
                println!("    -> {} phase", order.phase);
                if order.treasury > Money::ZERO {
                    println!("       treasury reserved: ${}", order.treasury.as_dollars());
                }
                if !order.inventory.is_empty() {
                    let items: Vec<String> = order
                        .inventory
                        .iter()
                        .map(|(commodity, qty)| format!("{qty} {commodity}"))
                        .collect();
                    println!("       inventory reserved: {}", items.join(", "));
                }
                if !order.labor.is_empty() {
                    let labor: Vec<String> = order
                        .labor
                        .iter()
                        .map(|(tier, qty)| format!("{qty} {tier:?}"))
                        .collect();
                    println!("       labor reserved: {}", labor.join(", "));
                }
            }
            has_orders = true;
        }

        let working: Vec<_> = player.military.civilians.iter().filter(|c| c.working).collect();
        if !working.is_empty() {
            println!("  WORKING CIVILIANS:");
            for civ in &working {
                let pos_str = civ
                    .position
                    .map(|p| format!("({}, {})", p.q, p.r))
                    .unwrap_or_else(|| "unknown".to_string());
                let remaining = civ.turns_remaining;
                println!(
                    "    {} at {} ({} turn{} remaining)",
                    civ.civilian_type,
                    pos_str,
                    remaining,
                    if remaining == 1 { "" } else { "s" }
                );
            }
            has_orders = true;
        }

        // Buildings under expansion
        let expanding: Vec<_> = player
            .economy.buildings
            .iter()
            .filter(|b| b.is_expanding())
            .collect();
        if !expanding.is_empty() {
            println!("  BUILDINGS UNDER EXPANSION:");
            for bldg in &expanding {
                println!(
                    "    {:?} (expanding, {} turn{} remaining)",
                    bldg.building_type,
                    bldg.expansion_turns_remaining(),
                    if bldg.expansion_turns_remaining() == 1 {
                        ""
                    } else {
                        "s"
                    }
                );
            }
            has_orders = true;
        }
    }

    if !has_orders {
        println!("  No pending orders.");
    }
}

// ── Turn report display ───────────────────────────────────────────

pub(crate) fn print_turn_report(game: &GameState, report: &TurnReport) {
    let player_id = game.human_player_nation;

    // ── Treasury ledger (all Great Powers — AI debugging view) ───
    print_treasury_ledger(game, report);

    // ── Newspaper ──────────────────────────────────────────────
    println!();
    println!("  {}", color_bold("THE IMPERIAL TIMES"));
    let date_str = format!(
        "{} Q{}, Turn {}",
        report.year, report.quarter, report.turn.0
    );
    let pad = 42usize.saturating_sub(date_str.len() + 6);
    println!("  \u{2554}{}\u{2557}", "\u{2550}".repeat(42));
    println!("  \u{2551}  {}{}  \u{2551}", date_str, " ".repeat(pad));
    println!("  \u{255a}{}\u{255d}", "\u{2550}".repeat(42));
    for headline in &report.newspaper_headlines {
        println!("    {}", headline.text);
    }
    println!();

    // ── Compact turn summary ───────────────────────────────────
    let header = format!(
        "Turn {} ({} Q{}) ",
        report.turn.0, report.year, report.quarter
    );
    let line_len = 42usize.saturating_sub(header.len());
    let summary_bar = format!(
        "\u{2500}\u{2500} {}{}",
        color_bold(&header),
        "\u{2500}".repeat(line_len)
    );
    println!("  {}", summary_bar);

    // Economy line: resources gathered
    let mut by_type: std::collections::HashMap<ResourceType, u32> =
        std::collections::HashMap::new();
    for (nid, res, amt) in &report.resource_production {
        if *nid == player_id {
            *by_type.entry(*res).or_insert(0) += amt;
        }
    }
    if !by_type.is_empty() {
        let mut sorted: Vec<_> = by_type.into_iter().collect();
        sorted.sort_by_key(|(r, _)| format!("{:?}", r));
        let resource_parts: Vec<String> = sorted
            .iter()
            .map(|(r, a)| format!("+{} {:?}", a, r))
            .collect();
        let resource_str = resource_parts.join(", ");

        // Food status
        let food_str = if let Some(player) = game.get_nation(player_id) {
            let grain = player.resource_amount(ResourceType::Grain);
            let fruit = player.resource_amount(ResourceType::Fruit);
            let livestock = player.resource_amount(ResourceType::Livestock);
            let total_food = grain + fruit + livestock;
            let food_needed = player.economy.labor.total_workers();
            if total_food >= food_needed {
                color_green(&format!("OK (surplus {})", total_food - food_needed))
            } else {
                color_red(&format!("DEFICIT ({})", food_needed - total_food))
            }
        } else {
            String::new()
        };

        println!(
            "  Economy:  {} | Food: {}",
            color_green(&resource_str),
            food_str
        );
    }

    // Trade line
    let player_trades_bought: Vec<_> = report
        .trade_transactions
        .iter()
        .filter(|txn| txn.buyer == player_id)
        .collect();
    let player_trades_sold: Vec<_> = report
        .trade_transactions
        .iter()
        .filter(|txn| txn.seller == player_id)
        .collect();
    if !player_trades_bought.is_empty() || !player_trades_sold.is_empty() {
        let total_bought: u32 = player_trades_bought.iter().map(|t| t.quantity).sum();
        let total_cost: i64 = player_trades_bought
            .iter()
            .map(|t| t.total_cost.as_dollars())
            .sum();
        let total_earned: i64 = player_trades_sold
            .iter()
            .map(|t| t.total_cost.as_dollars())
            .sum();

        let mut parts = Vec::new();
        if total_bought > 0 {
            parts.push(format!(
                "Bought {} resources for ${}",
                total_bought,
                format_number(total_cost as u32)
            ));
        }
        if total_earned > 0 {
            parts.push(format!("Earned ${}", format_number(total_earned as u32)));
        }
        println!("  Trade:    {}", parts.join(" | "));
    }

    // Subsidy costs line
    let player_subsidies: Vec<_> = report
        .subsidy_costs
        .iter()
        .filter(|(gp, _, _)| *gp == player_id)
        .collect();
    if !player_subsidies.is_empty() {
        let total_subsidy: i64 = player_subsidies
            .iter()
            .map(|(_, _, c)| c.as_dollars())
            .sum();
        println!(
            "  Subsidies: ${} paid to {} Minor Nations",
            format_number(total_subsidy as u32),
            player_subsidies.len()
        );
    }

    // Trade diplomacy line
    let player_diplo: Vec<_> = report
        .trade_diplomacy
        .iter()
        .filter(|(a, _, _)| *a == player_id)
        .collect();
    if !player_diplo.is_empty() {
        let parts: Vec<String> = player_diplo
            .iter()
            .map(|(_, target, imp)| {
                let name = game
                    .get_nation(*target)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| format!("Nation {}", target.0));
                format!("{} +{}", name, imp)
            })
            .collect();
        println!(
            "  Diplomacy: Trade improved relations: {}",
            parts.join(", ")
        );
    }

    // Industry line
    let player_prod: Vec<_> = report
        .production_output
        .iter()
        .filter(|(nid, _, _)| *nid == player_id)
        .collect();
    if !player_prod.is_empty() {
        let parts: Vec<String> = player_prod
            .iter()
            .map(|(_, item, qty)| format!("{} {}", qty, item))
            .collect();
        println!("  Industry: Produced {}", parts.join(", "));
    }

    // Town production line
    let player_town_prod: Vec<_> = report
        .town_production
        .iter()
        .filter(|(nid, _, _)| *nid == player_id)
        .collect();
    if !player_town_prod.is_empty() {
        // Group by province: find producing provinces owned by the player
        let mut town_parts: Vec<String> = Vec::new();
        for (_, item, qty) in &player_town_prod {
            town_parts.push(format!("{} {}", qty, item));
        }
        // Find the producing province names
        let producing_provinces: Vec<String> = game
            .world.provinces
            .iter()
            .filter(|p| p.owner == player_id && p.can_produce())
            .map(|p| format!("{} ({:?})", p.name, p.settlement_level))
            .collect();
        let prov_info = if producing_provinces.is_empty() {
            String::new()
        } else {
            format!(" from {}", producing_provinces.join(", "))
        };
        println!(
            "  Towns:    Produced {}{}",
            town_parts.join(", "),
            prov_info
        );
    }

    // Unit movement line
    let player_movements: Vec<_> = report
        .unit_movements
        .iter()
        .filter(|(nid, _)| *nid == player_id)
        .collect();
    if !player_movements.is_empty() {
        for (_, desc) in &player_movements {
            println!("  Movement: {}", desc);
        }
    }

    // Military line
    let player_battles: Vec<_> = report
        .battles
        .iter()
        .filter(|b| b.attacker == player_id || b.defender == player_id)
        .collect();
    if player_battles.is_empty() {
        println!("  Military: No battles");
    } else {
        for battle in &player_battles {
            let prov_name = game
                .get_province(battle.province)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            if battle.attacker_won && battle.attacker == player_id {
                println!(
                    "  Military: {}",
                    color_green(&format!("VICTORY at {}", prov_name))
                );
            } else if !battle.attacker_won && battle.defender == player_id {
                println!(
                    "  Military: {}",
                    color_green(&format!("Defended {}", prov_name))
                );
            } else {
                println!(
                    "  Military: {}",
                    color_red(&format!("DEFEAT at {}", prov_name))
                );
            }
        }
    }

    // Naval battles line
    let player_naval_battles: Vec<_> = report
        .naval_battles
        .iter()
        .filter(|b| b.attacker == player_id || b.defender == player_id)
        .collect();
    if !player_naval_battles.is_empty() {
        for nb in &player_naval_battles {
            let enemy_id = if nb.attacker == player_id {
                nb.defender
            } else {
                nb.attacker
            };
            let enemy_name = game
                .get_nation(enemy_id)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let we_won = (nb.attacker_won && nb.attacker == player_id)
                || (!nb.attacker_won && nb.defender == player_id);
            if we_won {
                println!(
                    "  Naval:    {}",
                    color_green(&format!(
                        "NAVAL VICTORY vs {} ({} enemy ships sunk)",
                        enemy_name,
                        if nb.attacker == player_id {
                            nb.defender_ships_lost.len()
                        } else {
                            nb.attacker_ships_lost.len()
                        }
                    ))
                );
            } else {
                println!(
                    "  Naval:    {}",
                    color_red(&format!(
                        "NAVAL DEFEAT vs {} ({} ships lost)",
                        enemy_name,
                        if nb.attacker == player_id {
                            nb.attacker_ships_lost.len()
                        } else {
                            nb.defender_ships_lost.len()
                        }
                    ))
                );
            }
        }
    }

    // Blockade effects (show if any headline mentions BLOCKADE for our nation)
    let blockade_headlines: Vec<_> = report
        .newspaper_headlines
        .iter()
        .filter(|h| h.text.contains("BLOCKADE"))
        .collect();
    if !blockade_headlines.is_empty() {
        for headline in &blockade_headlines {
            println!("  Blockade: {}", color_yellow(&headline.text));
        }
    }

    // Score line
    if let Some((rank, total)) = score_summary(&report.scores, player_id) {
        println!("  Score:    {} (#{})", format_number(total), rank);
    }

    println!("  {}", "\u{2500}".repeat(44));

    // ── Detailed events (starvation, gold, battles, etc.) ──────
    // Starvation
    for (nid, workers_lost) in &report.starvation {
        if *nid == player_id {
            println!(
                "  {}",
                color_red(&format!(
                    "WARNING: {} workers starved due to food shortage!",
                    workers_lost
                ))
            );
        }
    }

    // Gold income
    for (nid, income) in &report.gold_income {
        if *nid == player_id {
            println!(
                "  {}",
                color_green(&format!("Gold/Gems income: {}", income))
            );
        }
    }

    // Battle details (full report)
    if !report.battles.is_empty() {
        println!();
        for battle in &report.battles {
            let atk_name = game
                .get_nation(battle.attacker)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let def_name = game
                .get_nation(battle.defender)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let prov_name = game
                .get_province(battle.province)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            let result_str = if battle.attacker_won {
                color_green("VICTORY!")
            } else if battle.retreated {
                color_yellow("RETREAT (attacker withdrew)")
            } else {
                color_red("DEFEAT")
            };

            let game_cfg = &game.game_data.game_config;
            let terrain_str = match battle.terrain {
                Some(t) => {
                    let bonus = domain::military::terrain_defense_bonus(t, game_cfg);
                    if bonus > 0.0 {
                        format!("{:?} (+{:.0}% defense)", t, bonus * 100.0)
                    } else {
                        format!("{:?} (no bonus)", t)
                    }
                }
                None => "Unknown".to_string(),
            };

            let fort_str = if battle.fort_level > 0 {
                if battle.siege_reduced_fort {
                    let full_bonus = domain::military::fort_defense_bonus(battle.fort_level, game_cfg);
                    let reduced_bonus =
                        domain::military::effective_fort_bonus(battle.fort_level, true, game_cfg);
                    format!(
                        "Level {} (+{:.0}% -> +{:.0}% defense, reduced by siege artillery)",
                        battle.fort_level,
                        full_bonus * 100.0,
                        reduced_bonus * 100.0,
                    )
                } else {
                    let bonus = domain::military::fort_defense_bonus(battle.fort_level, game_cfg);
                    format!(
                        "Level {} (+{:.0}% defense)",
                        battle.fort_level,
                        bonus * 100.0
                    )
                }
            } else {
                "None".to_string()
            };

            let atk_casualty_str = if battle.attacker_casualties.is_empty() {
                "None".to_string()
            } else {
                format_casualties(&battle.attacker_casualties)
            };
            let def_casualty_str = if battle.defender_casualties.is_empty() {
                "None".to_string()
            } else {
                format_casualties(&battle.defender_casualties)
            };

            let def_fp_str = if battle.fort_level > 0
                || battle
                    .terrain
                    .map(|t| domain::military::terrain_defense_bonus(t, game_cfg) > 0.0)
                    .unwrap_or(false)
            {
                format!(
                    "{:.1} (incl. terrain/fort bonus)",
                    battle.defender_initial_fp
                )
            } else {
                format!("{:.1}", battle.defender_initial_fp)
            };

            println!("  {}", "=".repeat(42));
            println!("  {}", color_bold(&format!("BATTLE OF {}", prov_name)));
            println!("  {}", "=".repeat(42));
            println!(
                "  Attacker: {} ({} units, FP: {:.1})",
                atk_name, battle.attacker_initial_count, battle.attacker_initial_fp
            );
            println!(
                "  Defender: {} ({} units, FP: {})",
                def_name, battle.defender_initial_count, def_fp_str
            );
            println!("  Terrain: {}", terrain_str);
            println!("  Fort: {}", fort_str);
            println!();
            println!("  Result: {}", result_str);
            println!("  Attacker casualties: {}", atk_casualty_str);
            println!("  Defender casualties: {}", def_casualty_str);
            if battle.retreated {
                println!(
                    "  {}",
                    color_yellow("Attacker retreated! Surviving units took additional damage.")
                );
            }
            if battle.attacker_won {
                println!("  {}", color_green("Province conquered!"));
            }
            // Show medal awards
            if !battle.medal_awards.is_empty() {
                println!();
                println!("  Medal awards:");
                // Group by unit type
                let mut medal_counts: std::collections::BTreeMap<String, Vec<u8>> =
                    std::collections::BTreeMap::new();
                for (ut, count) in &battle.medal_awards {
                    medal_counts
                        .entry(format!("{:?}", ut))
                        .or_default()
                        .push(*count);
                }
                for (unit_name, medals) in &medal_counts {
                    let medal_strs: Vec<String> = medals
                        .iter()
                        .map(|m| {
                            let stars = "*".repeat(*m as usize);
                            format!("[{}]", stars)
                        })
                        .collect();
                    println!("    {} {}", unit_name, medal_strs.join(" "));
                }
            }
            println!("  {}", "=".repeat(42));
        }
    }

    // Civilian completions
    let player_civs: Vec<_> = report
        .civilian_completions
        .iter()
        .filter(|(nid, _)| *nid == player_id)
        .collect();
    if !player_civs.is_empty() {
        println!();
        println!("  Civilian work completed:");
        for (_, desc) in &player_civs {
            println!("    {}", color_green(desc));
        }
    }

    // Transport overflow
    let player_overflow: Vec<_> = report
        .transport_overflow
        .iter()
        .filter(|(nid, _, _)| *nid == player_id)
        .collect();
    if !player_overflow.is_empty() {
        println!();
        println!(
            "  {}",
            color_yellow("Transport overflow (resources left in field):")
        );
        for (_, res, qty) in &player_overflow {
            println!("    {:?}: {}", res, qty);
        }
    }

    // Immigration
    for (nid, count) in &report.immigration {
        if *nid == player_id {
            println!(
                "  {}",
                color_green(&format!(
                    "Immigration: {} new worker{} recruited!",
                    count,
                    if *count == 1 { "" } else { "s" }
                ))
            );
        }
    }

    // Settlement upgrades
    for (prov_id, level) in &report.settlement_upgrades {
        if let Some(prov) = game.get_province(*prov_id) {
            println!(
                "  {}",
                color_green(&format!(
                    "Settlement upgrade: {} is now a {}!",
                    prov.name, level
                ))
            );
        }
    }

    println!();

    // Council vote results
    if let Some(ref vote) = report.council_vote {
        println!("  \u{2554}{}\u{2557}", "\u{2550}".repeat(42));
        println!("  \u{2551}  COUNCIL OF GOVERNORS VOTE            \u{2551}");
        println!("  \u{255a}{}\u{255d}", "\u{2550}".repeat(42));
        for (nid, votes) in &vote.votes {
            let name = game
                .get_nation(*nid)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let marker = if Some(*nid) == vote.winner {
                " \u{25c4} WINNER"
            } else {
                ""
            };
            println!("    {:<12} {:>3} governors{}", name, votes, marker);
        }
        println!(
            "    Majority needed: {}/{}",
            vote.majority_threshold, vote.total_governors
        );
        println!();
        if let Some(winner_id) = vote.winner
            && winner_id == game.human_player_nation
        {
            println!("  {}", color_green("*** YOU HAVE WON THE GAME! ***"));
        }
    }
}

// ── Game end summary (deduplicated) ──────────────────────────────

/// Unified game-end summary. When `report` is `Some`, shows council vote
/// details and determines the winner from the vote; when `None`, determines
/// the winner from the highest score.
pub(crate) fn print_game_end_summary(game: &GameState, report: Option<&TurnReport>) {
    println!("  ╔════════════════════════════════════════╗");
    println!("  ║        FINAL GAME SUMMARY              ║");
    println!("  ╚════════════════════════════════════════╝");
    println!();

    // Final scores for all Great Powers
    println!("    {:<12} {:>8} {:>8}", "Nation", "Score", "Provinces");
    println!("    {}", "-".repeat(32));

    let mut scores: Vec<_> = game
        .great_powers()
        .iter()
        .map(|n| {
            let s = calculate_score(n, &game.game_data);
            (n.id, n.name.clone(), s.total, n.province_count())
        })
        .collect();
    scores.sort_by(|a, b| b.2.cmp(&a.2));

    for (id, name, total, prov_count) in &scores {
        let marker = if *id == game.human_player_nation {
            " <-- YOU"
        } else {
            ""
        };
        println!(
            "    {:<12} {:>8} {:>8}{}",
            name,
            format_number(*total),
            prov_count,
            marker
        );
    }
    println!();

    // Show Council of Governors vote details (only when report is provided)
    if let Some(report) = report
        && let Some(ref vote) = report.council_vote
    {
        println!("  Council of Governors Vote:");
        println!("    {}", "-".repeat(40));

        // Show MN governor preferences
        let mn_details: Vec<_> = vote
            .governor_details
            .iter()
            .filter(|d| d.owner_type == NationType::MinorNation)
            .collect();

        if !mn_details.is_empty() {
            println!("    Minor Nation governor preferences:");
            // Group by province owner for cleaner display
            let mut shown_owners: std::collections::HashSet<NationId> =
                std::collections::HashSet::new();
            for detail in &mn_details {
                if shown_owners.insert(detail.province_owner) {
                    let owner_name = game
                        .get_nation(detail.province_owner)
                        .map(|n| n.name.as_str())
                        .unwrap_or("Unknown");
                    let voted_for_name = game
                        .get_nation(detail.voted_for)
                        .map(|n| n.name.as_str())
                        .unwrap_or("Unknown");
                    println!(
                        "      Governor of {} votes for {} ({})",
                        owner_name, voted_for_name, detail.reason
                    );
                }
            }
            println!();
        }

        // Show vote tally
        println!("    Vote tally:");
        for (nid, count) in &vote.votes {
            let name = game
                .get_nation(*nid)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            let marker = if vote.winner == Some(*nid) {
                " <-- WINNER"
            } else {
                ""
            };
            println!("      {:<12} {:>3} votes{}", name, count, marker);
        }
        println!(
            "    Threshold for majority: {} / {}",
            vote.majority_threshold, vote.total_governors
        );
        println!();

        // Show which GP had most trade influence over MN governors
        let mut mn_influence: std::collections::HashMap<NationId, u32> =
            std::collections::HashMap::new();
        for detail in &mn_details {
            *mn_influence.entry(detail.voted_for).or_insert(0) += 1;
        }
        if !mn_influence.is_empty()
            && let Some((&most_influential, &mn_votes)) =
                mn_influence.iter().max_by_key(|&(_, v)| *v)
        {
            let name = game
                .get_nation(most_influential)
                .map(|n| n.name.as_str())
                .unwrap_or("Unknown");
            println!(
                "    Most trade influence over Minor Nations: {} ({} MN governors)",
                name, mn_votes
            );
        }
        println!();
    }

    // Diplomatic standings (shown when report is provided)
    if report.is_some() {
        println!("  Diplomatic Standings:");
        for gp in game.great_powers() {
            let standing = game.world.diplomacy.get_standing(gp.id);
            let marker = if gp.id == game.human_player_nation {
                " <-- YOU"
            } else {
                ""
            };
            println!("    {:<12} standing: {:>4}{}", gp.name, standing, marker);
        }
        println!();
    }

    // Determine winner
    let winner_id = if let Some(report) = report {
        if let Some(ref vote) = report.council_vote {
            vote.winner
        } else {
            // No council vote in final report — winner is highest scorer
            scores.first().map(|(id, _, _, _)| *id)
        }
    } else {
        // No report at all — winner is highest scorer
        scores.first().map(|(id, _, _, _)| *id)
    };

    if let Some(wid) = winner_id {
        let winner_name = game
            .get_nation(wid)
            .map(|n| n.name.as_str())
            .unwrap_or("Unknown");
        if wid == game.human_player_nation {
            println!(
                "  *** CONGRATULATIONS! {} (YOU) wins the game! ***",
                winner_name
            );
        } else {
            println!("  {} wins the game.", winner_name);
        }
    }

    // Human player ranking (shown when no report, i.e. auto-play path)
    if report.is_none() {
        let human_rank = scores
            .iter()
            .position(|(id, _, _, _)| *id == game.human_player_nation)
            .map(|i| i + 1)
            .unwrap_or(0);
        let total_gp = scores.len();
        println!(
            "  Your ranking: #{} of {} Great Powers",
            human_rank, total_gp
        );
        println!();
    }

    // Show high scores
    if !game.archive.high_scores.is_empty() {
        if report.is_some() {
            println!();
        }
        println!("  High Scores:");
        for (i, (name, score, date)) in game.archive.high_scores.iter().enumerate() {
            println!(
                "    {}. {} - {} ({})",
                i + 1,
                name,
                format_number(*score),
                date
            );
        }
    }
}

// ── Military display ─────────────────────────────────────────────

pub(crate) fn print_military(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };

    println!("  ARMY ({} units):", player.military.army.len());
    if player.military.army.is_empty() {
        println!("    (no units -- use 'build unit <type>' to recruit)");
    } else {
        for (i, unit) in player.military.army.iter().enumerate() {
            let province_name = game
                .get_province(unit.position)
                .map(|p| p.name.as_str())
                .unwrap_or("Unknown");
            println!(
                "    [{}] {:?} (HP: {}, Medals: {}, FP: {:.1}) at {}",
                i,
                unit.unit_type,
                unit.health,
                unit.medals,
                unit.effective_firepower(),
                province_name
            );
        }
        println!();
        println!(
            "  Total firepower: {:.1}",
            player.total_military_firepower()
        );
    }

    if !game.transient.pending_attacks.is_empty() {
        println!();
        println!("  PENDING ATTACKS:");
        for (attacker_id, province_id) in &game.transient.pending_attacks {
            if *attacker_id == game.human_player_nation {
                let province_name = game
                    .get_province(*province_id)
                    .map(|p| p.name.as_str())
                    .unwrap_or("Unknown");
                println!("    -> {}", province_name);
            }
        }
    }
}

// ── Transport display ────────────────────────────────────────────

pub(crate) fn print_transport(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };
    let ts = &player.military.transport;
    println!("  TRANSPORT SYSTEM:");
    println!("    Freight cars: {}", ts.freight_cars);
    println!("    Total capacity: {} units", ts.total_capacity());
    println!(
        "    Military transport capacity: {} army units",
        ts.military_transport_capacity()
    );
    println!();

    // Show connected vs disconnected provinces
    let connected = domain::turn::connected_provinces(game, game.human_player_nation);
    let total_provinces = player.province_count();
    let connected_count = connected.len();
    let disconnected_count = total_provinces - connected_count;
    println!(
        "  Provinces: {} connected, {} disconnected (of {} total)",
        connected_count, disconnected_count, total_provinces
    );

    // Calculate total production from connected tiles
    let mut total_production: std::collections::HashMap<ResourceType, u32> =
        std::collections::HashMap::new();
    for province in &game.world.provinces {
        if province.owner != game.human_player_nation {
            continue;
        }
        if !connected.contains(&province.id) {
            continue;
        }
        for tile_coord in &province.tiles {
            if let Some(tile) = game.world.hex_map.get_tile(*tile_coord)
                && let Some(yield_amount) = tile.calculate_yield()
            {
                *total_production.entry(yield_amount.resource).or_insert(0) +=
                    yield_amount.quantity;
            }
        }
    }

    let total_produced: u32 = total_production.values().sum();
    let capacity = ts.total_capacity();
    println!();
    println!("  RESOURCE PRODUCTION (connected tiles):");
    if total_production.is_empty() {
        println!("    (none)");
    } else {
        let mut sorted: Vec<_> = total_production.iter().collect();
        sorted.sort_by_key(|(r, _)| format!("{:?}", r));
        for (resource, amount) in &sorted {
            println!("    {:?}: {}", resource, amount);
        }
    }
    println!();
    println!(
        "  Total production: {} | Capacity: {} | {}",
        total_produced,
        capacity,
        if total_produced <= capacity {
            format!("Surplus capacity: {}", capacity - total_produced)
        } else {
            format!(
                "DEFICIT: {} resources will overflow",
                total_produced - capacity
            )
        }
    );

    println!();
    let (labor, lumber, steel) =
        domain::economy::transport::TransportSystem::build_freight_car_cost();
    println!(
        "  Use 'build car' to build a freight car (cost: {} labor, {} lumber, {} steel).",
        labor, lumber, steel
    );
}

// ── Infrastructure display ───────────────────────────────────────

pub(crate) fn print_infrastructure(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };
    let Some(capital_province) = game.get_province(player.capital_province_id) else {
        println!("  Internal error: capital province is missing from game state.");
        return;
    };
    let capital_tile = capital_province.capital_tile;

    println!("  INFRASTRUCTURE STATUS:");
    println!();

    for province_id in &player.province_ids {
        let province = match game.get_province(*province_id) {
            Some(p) => p,
            None => continue,
        };

        let mut railroads = 0u32;
        let mut depots = 0u32;
        let mut ports = 0u32;
        let mut total_land = 0u32;

        for coord in &province.tiles {
            if let Some(tile) = game.world.hex_map.get_tile(*coord)
                && tile.terrain().is_land()
            {
                total_land += 1;
                if tile.infrastructure.has_railroad {
                    railroads += 1;
                }
                if tile.infrastructure.has_depot {
                    depots += 1;
                }
                if tile.infrastructure.has_port {
                    ports += 1;
                }
            }
        }

        let connected = infrastructure::is_province_connected(
            &game.world.hex_map,
            capital_tile,
            *province_id,
            &game.world.provinces,
        );
        let conn_str = if *province_id == player.capital_province_id {
            "(capital)"
        } else if connected {
            "CONNECTED"
        } else {
            "not connected"
        };

        println!(
            "    {}: {} railroads/{} land, {} depot(s), {} port(s) [{}]",
            province.name, railroads, total_land, depots, ports, conn_str
        );
    }
}

// ── Fleet display ────────────────────────────────────────────────

/// Print the merchant fleet.
pub(crate) fn print_fleet(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };

    if player.military.merchant_fleet.is_empty() {
        println!(
            "  No merchant ships. Use 'build ship trader' or 'build ship indiaman' to build one."
        );
        return;
    }

    println!("  MERCHANT FLEET:");

    // Count ships by type
    let mut counts: std::collections::BTreeMap<String, (usize, u32)> =
        std::collections::BTreeMap::new();
    for ship in &player.military.merchant_fleet {
        let name = format!("{:?}", ship.ship_type);
        let cargo = game.game_data.ship_stats(ship.ship_type).cargo;
        let entry = counts.entry(name).or_insert((0, cargo));
        entry.0 += 1;
    }

    for (name, (count, cargo)) in &counts {
        println!("    {}x {} (cargo: {} each)", count, name, cargo);
    }
    println!(
        "    Total cargo capacity: {} holds",
        player.total_cargo_capacity(&game.game_data)
    );
}

/// Print the warship fleet.
pub(crate) fn print_navy(game: &GameState) {
    let Some(player) = human_player(game) else {
        return;
    };

    if player.military.warships.is_empty() {
        println!(
            "  No warships. Use 'build warship frigate' or 'build warship ship-of-the-line' to build one."
        );
        return;
    }

    println!("  NAVY:");

    // Count ships by type
    let mut counts: std::collections::BTreeMap<String, (usize, u32, u32, u32)> =
        std::collections::BTreeMap::new();
    for ship in &player.military.warships {
        let name = format!("{:?}", ship.ship_type);
        let stats = game.game_data.ship_stats(ship.ship_type);
        let entry = counts
            .entry(name)
            .or_insert((0, stats.firepower, stats.armor, stats.hull));
        entry.0 += 1;
    }

    for (name, (count, fp, armor, hull)) in &counts {
        println!(
            "    {}x {} (FP: {}, Armor: {}, Hull: {})",
            count, name, fp, armor, hull
        );
    }
    println!(
        "    Total naval firepower: {}",
        player.total_naval_firepower(&game.game_data)
    );
}

// ── Nation info display ──────────────────────────────────────────

pub(crate) fn print_nation_info(game: &GameState, query: &str) {
    let target = match game.find_nation_by_name(query) {
        Some(n) => n,
        None => {
            println!("  No unique nation matches '{}'. Be more specific.", query);
            return;
        }
    };

    let target_id = target.id;
    let player_id = game.human_player_nation;

    println!("  ══════════════════════════════════════");
    println!(
        "  {} ({})",
        target.name,
        if target.is_great_power() {
            "Great Power"
        } else {
            "Minor Nation"
        }
    );
    println!("  ══════════════════════════════════════");
    println!();

    // Province count and names
    let owned_provinces: Vec<_> = game
        .world.provinces
        .iter()
        .filter(|p| p.owner == target_id)
        .collect();
    println!("  Provinces: {}", owned_provinces.len());
    for p in &owned_provinces {
        let settlement = match p.settlement_level {
            domain::map::SettlementLevel::Hamlet => "Hamlet",
            domain::map::SettlementLevel::Village => "Village",
            domain::map::SettlementLevel::Town => "Town",
        };
        println!(
            "    - {} ({}, {} tiles)",
            p.name,
            settlement,
            p.tile_count()
        );
    }
    println!();

    // Treasury (Great Powers only)
    if target.is_great_power() {
        println!("  Treasury: {}", target.economy.treasury);
    }

    // AI personality (for AI-controlled Great Powers)
    if target_id != player_id
        && let Some(personality) = target.diplomacy.ai_personality
    {
        println!("  AI Personality: {}", personality);
    }

    // Army size and total firepower
    println!("  Army: {} units", target.military.army.len());
    if !target.military.army.is_empty() {
        println!(
            "  Total firepower: {:.1}",
            target.total_military_firepower()
        );
    }
    println!();

    // Diplomatic relations with player
    if target_id != player_id {
        let status = match game.world.diplomacy.get_relation(player_id, target_id) {
            Some(rel) => {
                if rel.at_war {
                    "AT WAR".to_string()
                } else if target.is_great_power() {
                    if rel.has_treaty(domain::events::TreatyType::Alliance) {
                        format!("Allied (score: {})", rel.score)
                    } else {
                        format!("Neutral (score: {})", rel.score)
                    }
                } else if rel.has_embassy {
                    format!("Embassy (score: {})", rel.score)
                } else if rel.has_consulate {
                    format!("Consulate (score: {})", rel.score)
                } else {
                    format!("No relations (score: {})", rel.score)
                }
            }
            None => "No contact".to_string(),
        };
        println!("  Relations with you: {}", status);
    } else {
        println!("  (This is your nation)");
    }

    // Score (Great Powers only)
    if target.is_great_power() {
        let score = calculate_score(target, &game.game_data);
        println!(
            "  Score: {} (Mil:{} Lab:{} Trans:{} Dip:{} Prov:{})",
            format_number(score.total),
            score.military_score,
            score.labor_score,
            score.transport_score,
            score.diplomatic_score,
            score.province_score,
        );
    }
}

// ── Map rendering ────────────────────────────────────────────────

pub(crate) fn render_map(hex_map: &HexMap, nations: &[Nation]) {
    let reset = "\x1b[0m";
    let sea_color = "\x1b[34m";

    let mut province_nation: std::collections::HashMap<ProvinceId, &Nation> =
        std::collections::HashMap::new();
    for nation in nations {
        for pid in &nation.province_ids {
            province_nation.insert(*pid, nation);
        }
    }

    for r in 0..hex_map.height() {
        if r % 2 == 1 {
            print!("   ");
        } else {
            print!("  ");
        }

        for q in 0..hex_map.width() {
            let coord = HexCoord::new(q, r);
            match hex_map.get_tile(coord) {
                Some(tile) => {
                    let ch = terrain_char(tile.terrain());
                    if tile.terrain() == TerrainType::Sea {
                        print!("{sea_color}~{reset} ");
                    } else if let Some(pid) = tile.province_id {
                        if let Some(nation) = province_nation.get(&pid) {
                            let color = nation_color_code(nation);
                            if tile.is_capital {
                                print!("{color}★{reset} ");
                            } else {
                                print!("{color}{ch}{reset} ");
                            }
                        } else {
                            print!("{ch} ");
                        }
                    } else {
                        print!("{ch} ");
                    }
                }
                None => print!("  "),
            }
        }
        println!();
    }
}

/// Debug-grade per-turn view of where every Great Power's money came from
/// and where it went. Prints a compact block per GP, with a reconciliation
/// mismatch row if the aggregator missed a source (should always be $0).
pub(crate) fn print_treasury_ledger(game: &GameState, report: &TurnReport) {
    if report.cash_flow.is_empty() {
        return;
    }

    println!();
    println!("  {}", color_bold("Treasury ledger — all Great Powers"));

    let mut gps: Vec<&Nation> = game.world.nations.iter().filter(|n| n.is_great_power()).collect();
    gps.sort_by_key(|n| n.id.0);

    for nation in gps {
        let flow = match report.cash_flow.get(&nation.id) {
            Some(f) => f,
            None => continue,
        };
        print_treasury_ledger_block(nation, flow);
        if let Some(rflow) = report.resource_flow.get(&nation.id) {
            print_resource_flow_line(rflow);
        }
    }
    println!();
}

fn print_treasury_ledger_block(nation: &Nation, flow: &CashFlow) {
    let delta = flow.observed_delta().as_dollars();
    let delta_str = if delta >= 0 {
        color_green(&format!("+${}", format_number_signed(delta)))
    } else {
        color_red(&format!("-${}", format_number_signed(-delta)))
    };
    println!(
        "  {:<12} ${:>8} \u{2192} ${:>8}  ({})",
        nation.name,
        format_number_signed(flow.opening_treasury.as_dollars()),
        format_number_signed(flow.closing_treasury.as_dollars()),
        delta_str,
    );

    let income_totals = flow.income_totals_by_source();
    let expense_totals = flow.expense_totals_by_sink();
    let total_income = flow.total_income().as_dollars();
    let total_expense = flow.total_expense().as_dollars();

    if total_income > 0 {
        // Category summary first — what the user most wants to see.
        let by_cat = flow.income_by_category();
        println!(
            "      Income  ${:<8}  {}",
            format_number_signed(total_income),
            format_category_dollars(&by_cat),
        );
        // Then the detailed per-source breakdown for debugging.
        let mut parts: Vec<(CashSource, i64)> =
            income_totals.into_iter().filter(|(_, v)| *v > 0).collect();
        parts.sort_by_key(|(k, _)| *k as u8);
        let s: Vec<String> = parts
            .iter()
            .map(|(src, amt)| format!("{} ${}", src.label(), format_number_signed(*amt)))
            .collect();
        println!("        detail:  {}", s.join(" | "));
    }
    if total_expense > 0 {
        let by_cat = flow.expense_by_category();
        println!(
            "      Expense ${:<8}  {}",
            format_number_signed(total_expense),
            format_category_dollars(&by_cat),
        );
        let mut parts: Vec<(CashSink, i64)> =
            expense_totals.into_iter().filter(|(_, v)| *v > 0).collect();
        parts.sort_by_key(|(k, _)| *k as u8);
        let s: Vec<String> = parts
            .iter()
            .map(|(sink, amt)| format!("{} ${}", sink.label(), format_number_signed(*amt)))
            .collect();
        println!("        detail:  {}", s.join(" | "));
    }

    let mismatch = flow.reconciliation_mismatch().as_dollars();
    if mismatch != 0 {
        println!(
            "      {}",
            color_red(&format!(
                "Reconciliation mismatch: ${}",
                format_number_signed(mismatch)
            ))
        );
    }
}

fn print_resource_flow_line(flow: &ResourceFlow) {
    if flow.is_empty() {
        return;
    }
    // Cross-stockpile summary by category (3 buckets).
    let in_by_cat = flow.inflow_by_category();
    let out_by_cat = flow.outflow_by_category();
    if !in_by_cat.is_empty() {
        println!(
            "      {} {}",
            color_green("In  +"),
            format_category_units(&in_by_cat)
        );
    }
    if !out_by_cat.is_empty() {
        println!(
            "      {} {}",
            color_red("Out -"),
            format_category_units(&out_by_cat)
        );
    }
    // Detail: top few stockpiles with per-category breakdown so the user can
    // drill in. Keep it compact — just the biggest movers.
    let in_stock_cat = flow.inflow_by_stockpile_and_category();
    let out_stock_cat = flow.outflow_by_stockpile_and_category();
    let top_inflow_stocks: Vec<_> = {
        let totals = flow.inflow_totals_by_stockpile();
        let mut v: Vec<_> = totals.into_iter().collect();
        v.sort_by_key(|(_, a)| std::cmp::Reverse(*a));
        v.into_iter().take(4).collect()
    };
    let top_outflow_stocks: Vec<_> = {
        let totals = flow.outflow_totals_by_stockpile();
        let mut v: Vec<_> = totals.into_iter().collect();
        v.sort_by_key(|(_, a)| std::cmp::Reverse(*a));
        v.into_iter().take(4).collect()
    };
    if !top_inflow_stocks.is_empty() {
        let detail: Vec<String> = top_inflow_stocks
            .iter()
            .map(|(s, _)| {
                let cats = in_stock_cat.get(s).cloned().unwrap_or_default();
                format!("{} ({})", s.label(), format_category_units(&cats))
            })
            .collect();
        println!("        in detail:  {}", detail.join(", "));
    }
    if !top_outflow_stocks.is_empty() {
        let detail: Vec<String> = top_outflow_stocks
            .iter()
            .map(|(s, _)| {
                let cats = out_stock_cat.get(s).cloned().unwrap_or_default();
                format!("{} ({})", s.label(), format_category_units(&cats))
            })
            .collect();
        println!("        out detail: {}", detail.join(", "));
    }
}

/// Format a `FlowCategory → $amount` map as e.g.
/// `production $1,230 | trade $450 | consumption $800`, showing only
/// nonzero buckets in stable order.
fn format_category_dollars(map: &std::collections::HashMap<FlowCategory, i64>) -> String {
    const ORDER: [FlowCategory; 3] = [
        FlowCategory::Production,
        FlowCategory::Trade,
        FlowCategory::Consumption,
    ];
    ORDER
        .iter()
        .filter_map(|c| {
            let v = *map.get(c).unwrap_or(&0);
            if v == 0 {
                None
            } else {
                Some(format!("{} ${}", c.label(), format_number_signed(v)))
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Format a `FlowCategory → u32` map for resource quantities.
fn format_category_units(map: &std::collections::HashMap<FlowCategory, u32>) -> String {
    const ORDER: [FlowCategory; 3] = [
        FlowCategory::Production,
        FlowCategory::Trade,
        FlowCategory::Consumption,
    ];
    ORDER
        .iter()
        .filter_map(|c| {
            let v = *map.get(c).unwrap_or(&0);
            if v == 0 {
                None
            } else {
                Some(format!("{} {}", c.label(), v))
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn format_number_signed(n: i64) -> String {
    let is_neg = n < 0;
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    let formatted: String = result.chars().rev().collect();
    if is_neg {
        format!("-{}", formatted)
    } else {
        formatted
    }
}
