-- AI Personality: Aggressive
--
-- Declares wars early and often. Prioritizes military build-up.
-- Trade priority: low, War threshold: low, Research: military techs first.

aggressive = {
    name = "Aggressive",

    -- Trade and diplomacy
    trade_priority = 0.3,       -- low trade focus
    war_declaration_interval = 15, -- frequent war declarations
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
}

function aggressive.evaluate_war(nation_id, target_id, relations)
    -- Aggressive: low threshold for war
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

return aggressive
