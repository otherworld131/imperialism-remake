-- AI Personality: Economic
--
-- Invests heavily in production and technology.
-- Trade priority: high, War threshold: moderate, Research: most expensive tech.

economic = {
    -- Trade and diplomacy
    trade_priority = 0.5,       -- synced to Rust default (was 0.7 in Lua)
    alliance_preference = 0.5,  -- synced to Rust default (was 0.6 in Lua)

    -- Military
    min_army_size = 3,          -- matches Rust default
    max_army_size = 7,          -- synced to Rust default (was 6 in Lua)

    -- Economy
    infrastructure_budget = 2000, -- synced to Rust default (was 3000 in Lua)
    worker_threshold = 5,       -- synced to Rust default (was 3 in Lua)

    -- Research
    research_strategy = "cheapest", -- synced to Rust default (was "expensive" in Lua)

    -- War (cooldown-based system)
    war_cooldown = 15,          -- moderate cooldown
    war_threshold = 0.6,        -- moderate-high bar for war
    army_min_for_war = 5,       -- needs good army
    opportunism_weight = 0.4,   -- modest exploitation of weakness
    min_artillery_for_minor_war = 3, -- need 3 artillery to breach minor defenses

    -- Opportunity gate (card #97)
    min_opportunity_start = 0.40,
    min_opportunity_end = 0.15,
    min_opportunity_decay_turns = 25,
    resource_bonus_per_missing = 0.08,
    resource_bonus_cap = 0.15,

    -- Army building tiers
    tier1_army_max = 3,
    tier2_army_max = 5,
    tier3_army_max = 10,
    tier1_treasury = 2500,
    tier2_treasury = 6000,
    tier3_treasury = 12000,
    tier4_treasury = 30000,

    -- Diplomacy
    consulate_max_per_turn = 2,
    propose_pacts = true,
    propose_alliances = true,
    grant_amount = 500,
    grant_interval = 6,
    embassy_treasury_threshold = 10000,
    max_alliances = 1,

    -- Naval. Warship count is no longer capped (card #112) — navy growth
    -- runs through ai_scored_spending like army growth. Material
    -- availability is the real throttle.
    max_merchant_ships = 3,
    min_army_naval_invasion = 6,  -- moderately cautious about overseas ops

    -- Economy
    expansion_threshold_multiplier = 1,
    use_tier_expansion = true,
    high_treasury_expansion_threshold = 12000,
    trade_resource_reserve = 15,
    trade_treasury_cap = 25000,
    -- Card [3/6]: industrialist treats imports as strategic; lower floor so
    -- buy-side bids stay funded even when treasury dips.
    trade_buy_treasury_floor = 2000,
    trade_buy_buffer_turns = 2,
    -- Card #465: industrialist tolerates small army; small reserve.
    arms_sell_reserve = 6,
    goods_sell_treasury_threshold = 2000,
    goods_reserve = 3,
    -- Industrialist tolerates slightly larger stockpiles before dumping.
    goods_fat_stockpile_threshold = 40,
    food_processing_expansion_threshold = 2,
    infra_budget_scale_threshold = 15000,

    -- Card [2/6]: industrialist tilt — heavier hardware/furniture, larger food buffer
    lumber_furniture_weight = 0.75,
    steel_armory_weight_peace = 0.1,
    steel_armory_weight_war = 0.5,
    canned_food_buffer = 2.0,
    canned_food_stockpile_target = 15,
    min_chain_target = 1,
    -- Economic personality is the most expansion-hungry: larger reserve so
    -- more buildings can grow in parallel.
    expansions_per_turn_target = 3,
    expansion_reserve_buildings_factor = 0.75,

    -- Worker training
    worker_train_threshold = 1,
    worker_promote_threshold = 1,

    -- Spending weights (need-based scoring system)
    spending_military_weight = 0.7,
    spending_economy_weight = 1.5,
    spending_diplomacy_weight = 0.8,
    treasury_reserve = 1500,
    min_score_threshold = 5.0,

    -- Tactical
    peace_war_duration_threshold = 20,
    peace_province_loss_ratio = 0.50,
    fort_strategy = "border",

    -- Field-army distribution (cards #5, #9)
    capital_reserve_normal = 2,
    capital_reserve_threatened = 6,
    max_redeploys_per_turn = 4,

    -- Retreat (card #18)
    retreat_prebattle_ratio = 2.5,
    retreat_postbattle_fp_loss = 0.55,

    -- Attack acceptance (card #99 phase 2): FP-based, defender FP includes
    -- terrain/fort/militia. Economic plays a touch more cautiously.
    attack_fp_vs_minor = 1.5,
    attack_fp_vs_gp = 1.4,

    -- Rest-heal and capital-save (cards #8, #20)
    rest_health_threshold = 50,       -- skip wounded units at half health
    capital_save_for_last_penalty = 25, -- moderate deterrent for minor capitals
    spending_naval_weight = 0.8,      -- below military weight (economy first)

    -- Naval landing gate (card #7)
    naval_min_adjacent_strength_ratio = 1.8,

    -- Coalition assessment weights (economy-heavy)
    coalition_mil_weight = 0.4,
    coalition_prov_weight = 0.3,
    coalition_econ_weight = 0.3,

    -- Economic-score multipliers (economy-heavy: weight workers more, treasury less)
    econ_score_treasury_divisor = 8000.0,   -- treasury contributes more (smaller divisor)
    econ_score_buildings_multiplier = 0.15, -- buildings count more for an economic AI
    econ_score_workers_multiplier = 0.07,   -- workforce premium

    -- Peace proposal thresholds
    peace_accept_threshold = 0.45,
    peace_reject_threshold = 0.70,
    peace_stalemate_duration = 15,

    -- War worthiness thresholds
    won_enough_captures = 2,
    won_enough_marginal = 0.35,
    lost_enough_losses = 2,
    lost_enough_likelihood = 0.35,

    -- Treaty evaluation thresholds
    nap_accept_threshold = 0.3,
    alliance_accept_threshold = 0.5,
    alliance_rival_penalty = 0.4,
    alliance_overcommit_penalty = 0.2,
    treaty_personality_bias = 0.0,

    -- War-decision tunables.
    -- Economic: moderate relations bar — won't pick fights but won't get pushed around.
    war_relations_threshold = -40,

    -- Peace-proposal tunables.
    -- Economic: trade-oriented; seeks peace once war becomes costly.
    peace_loss_min_duration = 15,
    peace_loss_max_win_likelihood = 0.6,
    peace_desperate_win_likelihood = -1.0,

    -- Treaty-response tunables.
    -- Economic: accepts NAPs readily (good for trade stability), passes on alliances.
    treaty_alliance_response_kind = "fall_through",
    treaty_alliance_response_param = 0.0,
    treaty_nap_response_kind = "relationship_at_least",
    treaty_nap_response_param = -20.0,
}

return economic
