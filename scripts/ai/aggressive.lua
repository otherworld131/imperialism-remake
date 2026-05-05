-- AI Personality: Aggressive
--
-- Declares wars early and often. Prioritizes military build-up.
-- Trade priority: low, War threshold: low, Research: military techs first.

aggressive = {
    name = "Aggressive",

    -- Trade and diplomacy
    trade_priority = 0.3,       -- low trade focus
    alliance_preference = 0.2,  -- rarely seeks alliances

    -- Military
    min_army_size = 5,          -- larger standing army
    max_army_size = 12,         -- builds more units
    preferred_unit = "Artillery", -- favors artillery

    -- Economy
    infrastructure_budget = 1500, -- less spending on infrastructure
    worker_threshold = 3,       -- recruits workers sooner

    -- Research
    research_strategy = "military", -- prioritize military techs

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
    goods_sell_treasury_threshold = 4000,
    goods_reserve = 1,
    food_processing_expansion_threshold = 3,
    infra_budget_scale_threshold = 25000,

    -- Card [2/6]: militarist tilt — heavier armory share, smaller civilian buffer
    lumber_furniture_weight = 0.6,
    steel_armory_weight_peace = 0.4,
    steel_armory_weight_war = 0.85,
    canned_food_buffer = 1.2,
    min_chain_target = 1,


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
    retreat_prebattle_ratio = 3.0,    -- aggressive: less likely to decline a fight
    retreat_postbattle_fp_loss = 0.70, -- fights harder before breaking

    -- Attack acceptance (card #99 phase 2): minimum ratio of our forward FP
    -- to the defender's local FP required to attack. Lower = more aggressive.
    attack_fp_vs_minor = 0.6,         -- attacks minors even at 60% of their FP
    attack_fp_vs_gp = 0.8,            -- presses GPs even when slightly outgunned

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
}

function aggressive.evaluate_war(nation_id, target_id, relations, need_score, opportunity_score)
    -- Use need/opportunity if available (new system)
    if need_score and opportunity_score then
        local score = (need_score or 0) + (opportunity_score or 0) * 0.8
        if score > 0.3 then
            return true
        end
    end
    -- Fallback: low threshold for war
    if relations < -20 then
        return true
    end
    return false
end

function aggressive.pick_tech(available_techs)
    -- Pick military tech first, then cheapest
    for _, tech in ipairs(available_techs) do
        for _, effect in ipairs(tech.effects or {}) do
            if effect.type == "UnlockUnit" or effect.type == "UpgradeUnit" then
                return tech
            end
        end
    end
    -- Fallback to cheapest
    local cheapest = nil
    local min_cost = math.huge
    for _, tech in ipairs(available_techs) do
        if tech.cost < min_cost then
            min_cost = tech.cost
            cheapest = tech
        end
    end
    return cheapest
end

function aggressive.evaluate_peace(nation_id, enemy_id, win_likelihood, captured, lost, duration)
    -- Aggressive AI only seeks peace when truly desperate
    if win_likelihood < 0.15 then
        return true
    end
    return nil  -- fall through to Rust logic
end

function aggressive.evaluate_treaty_response(nation_id, proposer_id, treaty_type, relationship, power_ratio)
    -- Aggressive AI almost never accepts alliances
    if treaty_type == "Alliance" then
        return false
    end
    -- Only accepts NAPs when significantly outpowered
    if treaty_type == "NonAggressionPact" and power_ratio < 0.5 then
        return true
    end
    return nil  -- fall through to Rust logic
end

return aggressive
