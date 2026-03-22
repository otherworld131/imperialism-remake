-- AI Personality: Balanced
--
-- Adapts to circumstances. Moderate in all areas.
-- Trade priority: medium, War threshold: moderate, Research: cheapest available.

balanced = {
    name = "Balanced",

    -- Trade and diplomacy
    trade_priority = 0.5,       -- moderate trade focus
    war_declaration_interval = 20, -- turns between war declarations
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
}

function balanced.evaluate_war(nation_id, target_id, relations)
    -- Balanced: declare war only if relations are poor and army is strong
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

return balanced
