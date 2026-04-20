-- Game Configuration
--
-- Global game-rule constants. These define fundamental mechanics, NOT personality
-- preferences. All AI personalities and human players use the same values.
--
-- To mod the game, change values here. For AI behavior tuning, see scripts/ai/*.lua.

game_config = {
    -- Labor output per worker type
    untrained_labor = 1,
    trained_labor = 2,
    expert_labor = 4,

    -- Labor cost per unit of mill/factory output
    labor_per_production = 2,

    -- Building a civilian unit permanently removes one expert worker
    civilian_costs_expert = true,

    -- Production ratios (resource -> material -> good)
    resources_per_material = 2,    -- 2 timber -> 1 lumber, 2 cotton/wool -> 1 fabric
    materials_per_good = 2,        -- 2 lumber -> 1 furniture, 2 steel -> 1 hardware
    coal_iron_ratio = 1,           -- 1 coal + 1 iron -> 1 steel (special case)

    -- Food
    food_per_worker = 1,           -- each worker eats 1 food per turn
    starvation_cap = 2,            -- max workers lost to starvation per turn
    canned_food_ratio = 2,         -- 2 raw food -> 1 canned food

    -- Immigration requirements (per immigrant)
    immigration_canned_food = 1,
    immigration_clothing = 1,
    immigration_furniture = 1,
    provinces_per_immigrant = 4,           -- 1 immigrant per N provinces
    provinces_per_immigrant_upgraded = 3,  -- with upgraded Capitol

    -- Monetary conversion
    gold_value = 500,              -- cash per unit of gold transported
    gems_value = 1000,             -- cash per unit of gems transported

    -- Buildings
    expansion_delay_turns = 2,     -- turns for building expansion to complete
    use_tier_expansion = true,     -- use capacity tier progression (2->4->8->12->...)

    -- Starting conditions
    starting_freight_cars = 15,    -- freight cars each Great Power starts with
    starting_engineers = 1,        -- Engineer civilians each Great Power starts with

    -- Civilian hire costs ($)
    engineer_cost = 500,
    prospector_cost = 100,
    miner_cost = 1500,
    farmer_cost = 100,
    rancher_cost = 100,
    forester_cost = 100,
    driller_cost = 2000,

    -- Infrastructure build costs ($)
    depot_cost = 2000,
    port_cost = 3000,
    railroad_cost_grassland = 100,
    railroad_cost_forest = 100,
    railroad_cost_desert = 150,
    railroad_cost_tundra = 150,
    railroad_cost_swamp = 300,
    railroad_cost_hills = 200,
    railroad_cost_mountain = 500,
    fort_cost_level_1 = 5000,
    fort_cost_level_2 = 7500,
    fort_cost_level_3 = 10000,

    -- Engineer build-task durations (turns to complete)
    build_turns_railroad = 1,
    build_turns_depot = 2,
    build_turns_port = 3,

    -- Tech prerequisites for laying railroad on each land terrain.
    -- nil = always buildable. Reference: tech/tree.rs.
    railroad_tech_grassland = nil,
    railroad_tech_forest = nil,
    railroad_tech_desert = nil,
    railroad_tech_tundra = nil,
    railroad_tech_hills = nil,
    railroad_tech_swamp = "Iron Railroad Bridge",
    railroad_tech_mountain = "Compound Steam Engine",

    -- Number of turns over which the AI amortises a depot's yield when
    -- comparing candidate placements vs. the railroad + depot build cost.
    -- Depots are long-lived infrastructure; a 50-turn window reflects that a
    -- nation should always be investing in development when an opportunity
    -- exists, not hoarding cash.
    infrastructure_horizon_turns = 50,

    -- Engineer-hire scoring (drives `score_hire_engineer`).
    -- Score = base + path_len × path_coeff, capped, returns None at hire_max.
    engineer_hire_max = 3,
    engineer_hire_base = 100,
    engineer_hire_path_coeff = 30,
    engineer_hire_cap = 250,

    -- Improver-civilian hire scoring (drives `score_civilian`).
    -- Replaces the old fixed 4-step coverage ladder with a continuous
    -- saturation formula that scales with empire size:
    --   each existing improver "covers" ~target_tiles_per_worker improvable
    --   tiles; every unmet tile beyond that capacity adds
    --   coverage_per_unmet to the score.
    --   hire_bootstrap is added when civilian_count = 0 and there is at
    --   least one improvable tile (gets the first civilian hired).
    --   idle_penalty is subtracted per idle improver already on the roster.
    civilian_target_tiles_per_worker = 3,
    civilian_coverage_per_unmet = 3.0,
    civilian_hire_bootstrap = 15.0,
    civilian_idle_penalty = 8.0,

    -- Backlog scoring weights — points added per turn the spending category
    -- has been neglected. Each personality has a different sensitivity per
    -- category, encoding what they grow "impatient" about.
    --     Aggressive  → impatient about military, slow on diplomacy
    --     Economic    → impatient about infrastructure
    --     Diplomatic  → impatient about diplomacy
    --     Balanced    → mid on everything
    -- These are added to the raw category score before sorting, so neglected
    -- categories naturally climb the priority ladder until they fire.
    backlog_weight_aggressive_military = 50,
    backlog_weight_aggressive_infra = 25,
    backlog_weight_aggressive_diplomacy = 5,
    backlog_weight_aggressive_hire_engineer = 20,
    backlog_weight_aggressive_hire_improver = 15,
    backlog_weight_balanced_military = 30,
    backlog_weight_balanced_infra = 30,
    backlog_weight_balanced_diplomacy = 20,
    backlog_weight_balanced_hire_engineer = 25,
    backlog_weight_balanced_hire_improver = 20,
    backlog_weight_economic_military = 15,
    backlog_weight_economic_infra = 50,
    backlog_weight_economic_diplomacy = 20,
    backlog_weight_economic_hire_engineer = 30,
    backlog_weight_economic_hire_improver = 25,
    backlog_weight_diplomatic_military = 10,
    backlog_weight_diplomatic_infra = 35,
    backlog_weight_diplomatic_diplomacy = 40,
    backlog_weight_diplomatic_hire_engineer = 25,
    backlog_weight_diplomatic_hire_improver = 20,
    -- Cap on day-1 backlog: prevents starting-state backlog (no category yet
    -- invested in) from completely dominating the first ~20 turns.
    backlog_initial_cap = 20,

    -- Priority diplomacy targets: at game start every Great Power picks N
    -- minor nations whose visible exports best fill its resource deficits.
    -- Until consulate+embassy are established with all priority targets, the
    -- diplomacy score is huge for those targets (and zero for non-priority
    -- minors). Once secured, diplomacy spending drops to near zero.
    -- N depends on personality.
    priority_minor_target_score = 1000.0,
    priority_minor_targets_aggressive = 3,
    priority_minor_targets_balanced = 4,
    priority_minor_targets_economic = 4,
    priority_minor_targets_diplomatic = 5,

    -- Trade prices: materials (first-level processed)
    lumber_price = 150,
    steel_price = 200,
    fabric_price = 150,
    paper_price = 100,
    arms_price = 300,
    canned_food_price = 100,

    -- Trade prices: finished goods (second-level processed)
    furniture_price = 400,
    clothing_price = 400,
    hardware_price = 500,

    -- Diplomacy costs
    consulate_cost = 500,
    embassy_cost = 5000,

    -- Diplomatic relationship tuning
    -- Minor nations voluntarily join a Great Power's empire when their relation
    -- score reaches this value. Range is [-100, 100], so 90 = near-max trust.
    voluntary_incorporation_threshold = 90,
    -- Per-turn cap on relationship improvement from trade with a consulate.
    -- The raw improvement is the number of distinct resources traded;
    -- capping prevents broad trade portfolios from trivially maxing relations.
    trade_relation_improvement_cap = 2,
    -- Only apply the trade relationship improvement once every N turns.
    -- Combined with the cap above, this controls how fast a GP can befriend
    -- a minor nation through passive trade alone. Set to 1 for every turn.
    trade_relation_turn_interval = 3,

    -- AI trade behaviour
    ai_consulate_target = 4,                  -- AI GPs aim for this many consulates in minor nations
    ai_consulate_priority_score = 30.0,        -- scoring weight per missing consulate below target
    ai_consulate_beyond_target_score = 3.0,    -- base scoring weight per available MN beyond target
    ai_consulate_beyond_target_decay = 4.0,    -- penalty per extra consulate above target

    -- Map generation
    min_food_tile_percent = 20,  -- at least 20% of land tiles must produce food
    food_cluster_chance = 40,    -- % chance food terrain spreads to adjacent tile
}

return game_config
