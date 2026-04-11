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
    starting_freight_cars = 5,     -- freight cars each Great Power starts with

    -- Diplomacy costs
    consulate_cost = 500,
    embassy_cost = 5000,

    -- Map generation
    min_food_tile_percent = 20,  -- at least 20% of land tiles must produce food
    food_cluster_chance = 40,    -- % chance food terrain spreads to adjacent tile
}

return game_config
