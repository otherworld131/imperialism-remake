-- AI Personality: Diplomatic
--
-- Avoids war, prioritizes trade and alliances.
-- Trade priority: high, War threshold: high, Research: economic techs first.

diplomatic = {
    -- Trade and diplomacy
    trade_priority = 0.5,       -- synced to Rust default (was 0.8 in Lua)
    alliance_preference = 0.5,  -- synced to Rust default (was 0.9 in Lua)

    -- Military
    min_army_size = 3,          -- synced to Rust default (was 2 in Lua)
    max_army_size = 7,          -- synced to Rust default (was 4 in Lua)

    -- Economy
    infrastructure_budget = 2000, -- synced to Rust default (was 2500 in Lua)
    worker_threshold = 5,       -- synced to Rust default (was 4 in Lua)

    -- Research
    research_strategy = "cheapest", -- synced to Rust default (was "economic" in Lua)

    -- War (cooldown-based system)
    war_cooldown = 20,          -- long cooldown between wars
    war_threshold = 0.9,        -- very high bar for declaring war
    army_min_for_war = 8,       -- needs overwhelming force
    opportunism_weight = 0.2,   -- barely exploits weakness
    min_artillery_for_minor_war = 3, -- need 3 artillery to breach minor defenses

    -- Opportunity gate (card #97)
    min_opportunity_start = 0.50,
    min_opportunity_end = 0.20,
    min_opportunity_decay_turns = 30,
    resource_bonus_per_missing = 0.06,
    resource_bonus_cap = 0.15,

    -- Army building tiers
    tier1_army_max = 2,
    tier2_army_max = 4,
    tier3_army_max = 8,
    tier1_treasury = 3000,
    tier2_treasury = 8000,
    tier3_treasury = 15000,
    tier4_treasury = 40000,

    -- Diplomacy
    consulate_max_per_turn = 4,
    propose_pacts = true,
    propose_alliances = true,
    grant_amount = 500,
    grant_interval = 4,
    embassy_treasury_threshold = 5000,
    max_alliances = 2,

    -- Naval. Warship count is no longer capped (card #112) — navy growth
    -- runs through ai_scored_spending like army growth. Material
    -- availability is the real throttle.
    max_merchant_ships = 5,
    min_army_naval_invasion = 8,  -- cautious: needs 8+ units for overseas

    -- Economy
    expansion_threshold_multiplier = 2,
    use_tier_expansion = true,
    high_treasury_expansion_threshold = 15000,
    -- Card [3/6]: diplomatic favors steady trade with neighbors; same buffer.
    trade_buy_treasury_floor = 1500,
    trade_buy_buffer_turns = 2,
    transport_slack_buffer_turns = 30, -- stop hauling slack once warehouse holds 30 turns of per-turn consumption
    -- Card #465: diplomatic keeps a small army; small reserve.
    arms_sell_reserve = 8,
    food_processing_expansion_threshold = 2,
    infra_budget_scale_threshold = 15000,

    -- Card [2/6]: balanced civilian tilt
    lumber_furniture_weight = 0.7,
    steel_armory_weight_peace = 0.15,
    steel_armory_weight_war = 0.6,
    canned_food_buffer = 1.7,
    canned_food_stockpile_target = 12,
    min_chain_target = 1,
    expansions_per_turn_target = 2,
    expansion_reserve_buildings_factor = 0.5,

    -- Worker training
    worker_train_threshold = 1,
    worker_promote_threshold = 2,

    -- Spending weights (need-based scoring system)
    spending_military_weight = 0.5,
    spending_economy_weight = 1.2,
    spending_diplomacy_weight = 1.8,
    treasury_reserve = 1000,
    min_score_threshold = 5.0,

    -- Tactical
    peace_war_duration_threshold = 10,
    peace_province_loss_ratio = 0.30,
    fort_strategy = "capital",

    -- Field-army distribution (cards #5, #9)
    capital_reserve_normal = 3,       -- keeps more at capital
    capital_reserve_threatened = 8,   -- heavy garrison when threatened
    max_redeploys_per_turn = 3,

    -- Retreat (card #18)
    retreat_prebattle_ratio = 2.0,    -- still cautious but won't bail on a fair fight
    retreat_postbattle_fp_loss = 0.50,

    -- Attack acceptance (card #99 phase 2): cautious. Defender FP includes
    -- terrain/fort/militia, so diplomatic AI needs a clean superiority.
    attack_fp_vs_minor = 1.7,
    attack_fp_vs_gp = 1.6,

    -- Rest-heal and capital-save (cards #8, #20)
    rest_health_threshold = 60,       -- diplomatic: very reluctant to fight with wounded units
    capital_save_for_last_penalty = 30, -- strong deterrent; prefer diplomatic absorption
    spending_naval_weight = 1.0,      -- balanced naval investment

    -- Naval landing gate (card #7)
    naval_min_adjacent_strength_ratio = 2.0, -- very reluctant amphibious unless overland is hopeless

    -- Coalition assessment weights
    coalition_mil_weight = 0.4,
    coalition_prov_weight = 0.3,
    coalition_econ_weight = 0.3,

    -- Peace proposal thresholds
    peace_accept_threshold = 0.50,
    peace_reject_threshold = 0.65,
    peace_stalemate_duration = 12,

    -- War worthiness thresholds
    won_enough_captures = 1,
    won_enough_marginal = 0.40,
    lost_enough_losses = 1,
    lost_enough_likelihood = 0.40,

    -- Treaty evaluation thresholds
    nap_accept_threshold = 0.2,
    alliance_accept_threshold = 0.4,
    alliance_rival_penalty = 0.3,
    alliance_overcommit_penalty = 0.1,
    treaty_personality_bias = 0.4,

    -- War-decision tunables.
    -- Diplomatic: very high relations bar; almost never declares war on relations alone.
    war_relations_threshold = -80,

    -- Peace-proposal tunables.
    -- Diplomatic: eager to end wars after short stalemates AND auto-peace when behind.
    peace_loss_min_duration = 10,
    peace_loss_max_win_likelihood = 0.6,
    peace_desperate_win_likelihood = 0.50,

    -- Treaty-response tunables.
    -- Diplomatic: very receptive — accepts alliances when relationship >= 0,
    -- accepts NAPs from anyone not openly hostile.
    treaty_alliance_response_kind = "relationship_at_least",
    treaty_alliance_response_param = 0.0,
    treaty_nap_response_kind = "relationship_at_least",
    treaty_nap_response_param = -30.0,
}

return diplomatic
