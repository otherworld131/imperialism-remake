-- AI Personality: Balanced
--
-- Adapts to circumstances. Moderate in all areas.
-- Trade priority: medium, War threshold: moderate, Research: cheapest available.

balanced = {
    name = "Balanced",

    -- Trade and diplomacy
    trade_priority = 0.5,       -- moderate trade focus
    alliance_preference = 0.5,  -- moderate preference for alliances

    -- Military
    min_army_size = 3,          -- minimum standing army
    max_army_size = 7,          -- maximum before stopping recruitment
    preferred_unit = nil,       -- no preference, builds what's needed

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
    propose_alliances = false,
    grant_amount = 500,
    grant_interval = 8,
    embassy_treasury_threshold = 10000,
    max_alliances = 1,

    -- Naval
    max_warships_low_treasury = 2,
    max_warships_high_treasury = 4,
    max_merchant_ships = 3,
    min_army_naval_invasion = 5,  -- needs 5+ units for overseas attack

    -- Economy
    expansion_threshold_multiplier = 2,
    use_tier_expansion = true,
    high_treasury_expansion_threshold = 15000,
    trade_resource_reserve = 10,
    trade_treasury_cap = 20000,
    goods_sell_treasury_threshold = 3000,
    goods_reserve = 2,
    food_processing_expansion_threshold = 2,
    infra_budget_scale_threshold = 20000,

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
}

function balanced.evaluate_war(nation_id, target_id, relations, need_score, opportunity_score)
    -- Use need/opportunity if available (new system)
    if need_score and opportunity_score then
        local score = (need_score or 0) + (opportunity_score or 0) * 0.5
        if score > 0.5 then
            return true
        end
    end
    -- Fallback: declare war only if relations are poor
    if relations < -50 then
        return true
    end
    return false
end

function balanced.pick_tech(available_techs)
    -- Pick cheapest tech
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

function balanced.evaluate_peace(nation_id, enemy_id, win_likelihood, captured, lost, duration)
    -- Balanced AI seeks peace after 20+ turns with no net gains, but only if not clearly winning
    if duration >= 20 and captured <= lost and win_likelihood < 0.6 then
        return true
    end
    return nil  -- fall through to Rust logic
end

function balanced.evaluate_treaty_response(nation_id, proposer_id, treaty_type, relationship, power_ratio)
    -- Balanced AI accepts alliances more readily when outpowered
    if treaty_type == "Alliance" and power_ratio < 0.8 then
        return true
    end
    return nil  -- fall through to Rust logic
end

return balanced
