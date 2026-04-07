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

    -- Tactical
    peace_war_duration_threshold = 10,
    peace_province_loss_ratio = 0.30,
    fort_strategy = "capital",
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

return diplomatic
