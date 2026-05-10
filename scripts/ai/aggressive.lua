-- AI Personality: Aggressive
--
-- Declares wars early and often. Prioritizes military build-up.
-- Trade priority: low, War threshold: low, Research: military techs first.

aggressive = {
    -- Trade and diplomacy
    trade_priority = 0.5,       -- synced to Rust default (was 0.3 in Lua)
    alliance_preference = 0.5,  -- synced to Rust default (was 0.2 in Lua)

    -- Military
    min_army_size = 3,          -- synced to Rust default (was 5 in Lua)
    max_army_size = 7,          -- synced to Rust default (was 12 in Lua)

    -- Economy
    infrastructure_budget = 2000, -- synced to Rust default (was 1500 in Lua)
    worker_threshold = 5,       -- synced to Rust default (was 3 in Lua)

    -- Research
    research_strategy = "cheapest", -- synced to Rust default (was "military" in Lua)

    -- War (cooldown-based system)
    war_cooldown = 8,           -- turns between war declarations
    war_threshold = 0.3,        -- low bar for declaring war
    army_min_for_war = 3,       -- attacks with smaller armies
    opportunism_weight = 0.8,   -- strongly exploits weakness
    min_artillery_for_minor_war = 2, -- need 2 artillery to breach minor defenses

    -- Opportunity gate (card #97): decaying minimum firepower/province
    -- advantage required before declaring war. High early, permissive later.
    min_opportunity_start = 0.25,      -- turn 0: small edge required
    min_opportunity_end = 0.05,        -- turn decay_turns+: nearly always open
    min_opportunity_decay_turns = 15,
    -- Resource-bonus knobs: trade covers most resource needs, so lacking
    -- resources is only a weak casus belli.
    resource_bonus_per_missing = 0.12,
    resource_bonus_cap = 0.25,

    -- Army building tiers
    tier1_army_max = 4,
    tier2_army_max = 7,
    tier3_army_max = 15,
    tier1_treasury = 1500,
    tier2_treasury = 3000,
    tier3_treasury = 6000,
    tier4_treasury = 15000,

    -- Diplomacy
    consulate_max_per_turn = 2,
    propose_pacts = true,   -- NAPs with minor nations protect trade partners
    propose_alliances = false,
    grant_amount = 0,
    grant_interval = 0,
    embassy_treasury_threshold = 15000,
    max_alliances = 1,

    -- Naval. Warship count is no longer capped (card #112) — navy growth
    -- runs through ai_scored_spending like army growth. Material
    -- availability is the real throttle.
    max_merchant_ships = 1,
    min_army_naval_invasion = 4,  -- attack overseas targets with 4+ units

    -- Economy
    expansion_threshold_multiplier = 2,
    use_tier_expansion = true,
    high_treasury_expansion_threshold = 20000,
    trade_resource_reserve = 8,
    trade_treasury_cap = 15000,
    -- Card [3/6]: aggressive accepts a thinner cash safety net but still
    -- buffers 2 turns so wartime steel/arms output isn't input-starved.
    trade_buy_treasury_floor = 1500,
    trade_buy_buffer_turns = 2,
    -- Card #465: militarist hoards arms — large reserve.
    arms_sell_reserve = 30,
    goods_sell_treasury_threshold = 4000,
    goods_reserve = 1,
    -- Aggressive likes liquidity for war — drain harder.
    goods_fat_stockpile_threshold = 20,
    food_processing_expansion_threshold = 3,
    infra_budget_scale_threshold = 25000,

    -- Card [2/6]: militarist tilt — heavier armory share, smaller civilian buffer
    lumber_furniture_weight = 0.6,
    steel_armory_weight_peace = 0.4,
    steel_armory_weight_war = 0.85,
    canned_food_buffer = 1.2,
    min_chain_target = 1,
    -- Aggressive militarises hardware/arms more aggressively, so a smaller
    -- expansion reserve — but still scale with economy size.
    expansions_per_turn_target = 1,
    expansion_reserve_buildings_factor = 0.3,


    -- Worker training
    worker_train_threshold = 1,
    worker_promote_threshold = 2,

    -- Spending weights (need-based scoring system)
    spending_military_weight = 1.8,
    spending_economy_weight = 0.6,
    spending_diplomacy_weight = 0.3,
    treasury_reserve = 500,
    min_score_threshold = 3.0,

    -- Tactical
    peace_war_duration_threshold = 30,
    peace_province_loss_ratio = 0.50,
    fort_strategy = "offensive",

    -- Field-army distribution (cards #5, #9)
    capital_reserve_normal = 1,       -- aggressive: fewer units sit at capital
    capital_reserve_threatened = 5,
    max_redeploys_per_turn = 6,       -- push more units forward per turn

    -- Retreat (card #18)
    retreat_prebattle_ratio = 4.0,    -- aggressive: only bail if defender FP is 4×+ our own
    retreat_postbattle_fp_loss = 0.70, -- fights harder before breaking

    -- Attack acceptance (card #99 phase 2): minimum ratio of our forward FP
    -- to the defender's effective FP (terrain + fort + militia included).
    -- Aggressive: ~3:2 vs minors, near parity vs GPs.
    attack_fp_vs_minor = 1.3,
    attack_fp_vs_gp = 1.1,

    -- Rest-heal and capital-save (cards #8, #20)
    rest_health_threshold = 30,       -- aggressive: only avoids combat when badly hurt
    capital_save_for_last_penalty = 10, -- small deterrent; aggression trumps caution
    spending_naval_weight = 1.2,      -- slightly above military weight (naval wars matter)

    -- Naval landing gate (card #7)
    naval_min_adjacent_strength_ratio = 1.2,

    -- Coalition assessment weights (military-heavy)
    coalition_mil_weight = 0.6,
    coalition_prov_weight = 0.25,
    coalition_econ_weight = 0.15,

    -- Peace proposal thresholds (very reluctant to accept peace)
    peace_accept_threshold = 0.35,
    peace_reject_threshold = 0.80,
    peace_stalemate_duration = 25,

    -- War worthiness thresholds (fights until heavy losses)
    won_enough_captures = 4,
    won_enough_marginal = 0.20,
    lost_enough_losses = 3,
    lost_enough_likelihood = 0.20,

    -- Treaty evaluation thresholds (very reluctant to ally)
    nap_accept_threshold = 0.5,
    alliance_accept_threshold = 0.7,
    alliance_rival_penalty = 0.5,
    alliance_overcommit_penalty = 0.3,
    treaty_personality_bias = -0.3,

    -- War-decision tunables.
    -- Aggressive: low relations bar so frequent wars even without need/opp.
    war_relations_threshold = -20,

    -- Peace-proposal tunables.
    -- Aggressive: only seek peace when truly desperate (win_likelihood < 0.15);
    -- the duration-based branch is disabled by a very high min duration.
    peace_loss_min_duration = 999,
    peace_loss_max_win_likelihood = 0.6,
    peace_desperate_win_likelihood = 0.15,

    -- Treaty-response tunables.
    -- Aggressive: never accepts alliances; accepts NAPs only when overpowered.
    treaty_alliance_response_kind = "reject",
    treaty_alliance_response_param = 0.0,
    treaty_nap_response_kind = "power_below",
    treaty_nap_response_param = 0.5,
}

return aggressive
