-- AI Personality: Balanced
--
-- Adapts to circumstances. Moderate in all areas.
-- Trade priority: medium, War threshold: moderate, Research: cheapest available.

balanced = {
    -- Trade and diplomacy
    trade_priority = 0.5,       -- moderate trade focus
    alliance_preference = 0.5,  -- moderate preference for alliances

    -- Military
    min_army_size = 3,          -- minimum standing army
    max_army_size = 7,          -- maximum before stopping recruitment

    -- Economy
    infrastructure_budget = 2000, -- max spend per turn on infrastructure
    worker_threshold = 5,       -- recruit workers above this food surplus

    -- Research
    research_strategy = "cheapest", -- pick cheapest available tech

    -- War (cooldown-based system)
    war_cooldown = 12,          -- turns between war declarations
    war_threshold = 0.5,        -- moderate bar for declaring war
    army_min_for_war = 4,       -- needs decent army
    opportunism_weight = 0.5,   -- moderate exploitation of weakness
    min_artillery_for_minor_war = 2, -- need 2 artillery to breach minor defenses

    -- Opportunity gate (card #97): decaying minimum firepower/province
    -- advantage required before declaring war.
    min_opportunity_start = 0.35,
    min_opportunity_end = 0.10,
    min_opportunity_decay_turns = 20,
    resource_bonus_per_missing = 0.10,
    resource_bonus_cap = 0.20,

    -- Army building tiers
    tier1_army_max = 3,
    tier2_army_max = 5,
    tier3_army_max = 12,
    tier1_treasury = 2000,
    tier2_treasury = 5000,
    tier3_treasury = 10000,
    tier4_treasury = 25000,

    -- Diplomacy
    consulate_max_per_turn = 2,
    propose_pacts = true,
    propose_alliances = true,
    grant_amount = 500,
    grant_interval = 8,
    embassy_treasury_threshold = 10000,
    max_alliances = 1,

    -- Naval. Warship count is no longer capped (card #112) — navy growth
    -- runs through ai_scored_spending like army growth. Material
    -- availability is the real throttle.
    max_merchant_ships = 3,
    min_army_naval_invasion = 5,  -- needs 5+ units for overseas attack

    -- Economy
    expansion_threshold_multiplier = 2,
    use_tier_expansion = true,
    high_treasury_expansion_threshold = 15000,
    -- Card [3/6]: buy-side trade. Floor lowered so early-game nations can
    -- still bid on critical chain inputs they don't produce locally.
    trade_buy_treasury_floor = 1500,
    trade_buy_buffer_turns = 2,
    transport_slack_buffer_turns = 30, -- stop hauling slack once warehouse holds 30 turns of per-turn consumption
    -- Card #465: hold back at least this many arms above queued-recruit demand.
    arms_sell_reserve = 12,
    -- Fat-stockpile dump: drain finished goods above 30 units regardless of
    -- treasury so the warehouse doesn't bloat once trade goes positive.
    food_processing_expansion_threshold = 2,
    infra_budget_scale_threshold = 20000,

    -- Card [2/6]: production-chain target split weights
    lumber_furniture_weight = 0.7,         -- 70% of lumber → furniture, 30% → paper
    steel_armory_weight_peace = 0.2,       -- 20% of steel → armory in peacetime
    steel_armory_weight_war = 0.7,         -- 70% of steel → armory at war
    canned_food_buffer = 1.5,              -- canned food target = 1.5× immigration queue
    canned_food_stockpile_target = 10,     -- flat canned stockpile target; raw composite meals come first
    min_chain_target = 1,                  -- floor so transient shortages don't zero a chain
    expansions_per_turn_target = 2,        -- reserve lumber+steel for ≥2 simultaneous expansions
    expansion_reserve_buildings_factor = 0.5, -- + ceil(expandable_buildings × 0.5) extra slots


    -- Worker training
    worker_train_threshold = 1,
    worker_promote_threshold = 2,

    -- Spending weights (need-based scoring system)
    spending_military_weight = 1.0,
    spending_economy_weight = 1.0,
    spending_diplomacy_weight = 0.8,
    treasury_reserve = 1000,
    min_score_threshold = 5.0,

    -- Tactical
    peace_war_duration_threshold = 20,
    peace_province_loss_ratio = 0.50,
    fort_strategy = "border",

    -- Field-army distribution (cards #5, #9)
    capital_reserve_normal = 2,       -- units kept at capital when safe
    capital_reserve_threatened = 6,   -- units concentrated at capital under imminent threat
    max_redeploys_per_turn = 4,       -- cap on units moved per turn to prevent thrash

    -- Retreat (card #18)
    retreat_prebattle_ratio = 3.0,    -- opponent-to-own FP ratio that triggers pre-battle retreat
    retreat_postbattle_fp_loss = 0.60, -- fraction of FP lost at which a side retreats mid-battle

    -- Attack acceptance (card #99 phase 2): FP-based, defender FP includes
    -- terrain/fort/militia. 1.5 = "3-vs-2" advantage required to commit.
    attack_fp_vs_minor = 1.5,
    attack_fp_vs_gp = 1.3,

    -- Rest-heal and capital-save (cards #8, #20)
    rest_health_threshold = 50,       -- skip wounded units at half health
    capital_save_for_last_penalty = 25, -- moderate deterrent for minor capitals
    spending_naval_weight = 1.0,      -- same as military weight

    -- Naval landing gate (card #7)
    naval_min_adjacent_strength_ratio = 1.5, -- adjacent prov is "too hard" if defenders > army * this ratio

    -- Coalition assessment weights (defaults)
    coalition_mil_weight = 0.5,
    coalition_prov_weight = 0.3,
    coalition_econ_weight = 0.2,

    -- Peace proposal thresholds
    peace_accept_threshold = 0.45,
    peace_reject_threshold = 0.70,
    peace_stalemate_duration = 15,

    -- War worthiness thresholds
    won_enough_captures = 2,
    won_enough_marginal = 0.30,
    lost_enough_losses = 2,
    lost_enough_likelihood = 0.30,

    -- Treaty evaluation thresholds
    nap_accept_threshold = 0.3,
    alliance_accept_threshold = 0.5,
    alliance_rival_penalty = 0.4,
    alliance_overcommit_penalty = 0.2,
    treaty_personality_bias = 0.1,

    -- War-decision tunables (former evaluate_war function body).
    -- score = need + opportunity * opportunism_weight; war declared if
    -- score > war_threshold OR relations < war_relations_threshold.
    war_relations_threshold = -50,

    -- Peace-proposal tunables (former evaluate_peace function body).
    -- AI proposes peace if duration >= peace_loss_min_duration AND
    -- captured <= lost AND win_likelihood < peace_loss_max_win_likelihood.
    -- AND/OR (independent rule) if win_likelihood < peace_desperate_win_likelihood.
    -- Set peace_loss_min_duration to a very high value to disable that branch.
    -- Set peace_desperate_win_likelihood to a negative value to disable that branch.
    peace_loss_min_duration = 20,
    peace_loss_max_win_likelihood = 0.6,
    peace_desperate_win_likelihood = -1.0,

    -- Treaty-response tunables (former evaluate_treaty_response function body).
    -- response_kind ∈ { "power_below", "relationship_at_least", "reject", "fall_through" }
    --   "power_below"           → accept if power_ratio < param (param: f64)
    --   "relationship_at_least" → accept if relationship >= param (param: i32)
    --   "reject"                → always return false
    --   "fall_through"          → return nil; Rust generic logic decides
    treaty_alliance_response_kind = "power_below",
    treaty_alliance_response_param = 0.8,
    treaty_nap_response_kind = "fall_through",
    treaty_nap_response_param = 0.0,
}

return balanced
