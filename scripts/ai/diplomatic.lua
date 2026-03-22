-- AI Personality: Diplomatic
--
-- Avoids war, prioritizes trade and alliances.
-- Trade priority: high, War threshold: high, Research: economic techs first.

diplomatic = {
    name = "Diplomatic",

    -- Trade and diplomacy
    trade_priority = 0.8,       -- high trade focus
    war_declaration_interval = 40, -- rarely declares war
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
}

function diplomatic.evaluate_war(nation_id, target_id, relations)
    -- Diplomatic: very reluctant to go to war
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
