-- AI Personality: Diplomatic
--
-- Avoids war, prioritizes trade and alliances.
-- Trade priority: high, War threshold: high, Research: economic techs first.

diplomatic = {
    name = "Diplomatic",

    -- Trade and diplomacy
    trade_priority = 0.8,       -- high trade focus
    alliance_preference = 0.9,  -- strongly prefers alliances

    -- Military
    min_army_size = 2,          -- small standing army
    max_army_size = 4,          -- minimal military
    preferred_unit = nil,       -- no preference

    -- Economy
    infrastructure_budget = 2500, -- invests in infrastructure
    worker_threshold = 4,       -- moderate worker recruitment

    -- Research
    research_strategy = "economic", -- prioritize economic techs

    -- War (cooldown-based system)
    war_cooldown = 20,          -- long cooldown between wars
    war_threshold = 0.9,        -- very high bar for declaring war
    army_min_for_war = 8,       -- needs overwhelming force
    opportunism_weight = 0.2,   -- barely exploits weakness

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

    -- Naval
    max_warships_low_treasury = 2,
    max_warships_high_treasury = 4,
    max_merchant_ships = 5,

    -- Economy
    expansion_threshold_multiplier = 2,
    use_tier_expansion = true,
    high_treasury_expansion_threshold = 15000,
    trade_resource_reserve = 10,
    trade_treasury_cap = 20000,
    goods_sell_treasury_threshold = 3000,
    goods_reserve = 2,
    food_processing_expansion_threshold = 2,
    infra_budget_scale_threshold = 15000,

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
}

function diplomatic.evaluate_war(nation_id, target_id, relations, need_score, opportunity_score)
    -- Use need/opportunity if available (new system)
    if need_score and opportunity_score then
        local score = (need_score or 0) + (opportunity_score or 0) * 0.2
        if score > 0.9 then
            return true
        end
    end
    -- Fallback: very reluctant to go to war
    if relations < -80 then
        return true
    end
    return false
end

function diplomatic.pick_tech(available_techs)
    -- Pick economic/building tech first
    for _, tech in ipairs(available_techs) do
        for _, effect in ipairs(tech.effects or {}) do
            if effect.type == "UnlockBuilding" or effect.type == "EnableTerrainImprovement" then
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

function diplomatic.evaluate_peace(nation_id, enemy_id, win_likelihood, captured, lost, duration)
    -- Diplomatic AI is eager to end wars: propose peace after short stalemates
    if duration >= 10 and captured <= lost and win_likelihood < 0.6 then
        return true
    end
    -- Always propose peace if win_likelihood is below 50%
    if win_likelihood < 0.50 then
        return true
    end
    return nil  -- fall through to Rust logic
end

function diplomatic.evaluate_treaty_response(nation_id, proposer_id, treaty_type, relationship, power_ratio)
    -- Diplomatic AI is very receptive to alliances
    if treaty_type == "Alliance" and relationship >= 0 then
        return true
    end
    -- Accept NAPs from anyone not hostile
    if treaty_type == "NonAggressionPact" and relationship >= -30 then
        return true
    end
    return nil  -- fall through to Rust logic
end

return diplomatic
