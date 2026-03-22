-- AI Personality: Economic
--
-- Invests heavily in production and technology.
-- Trade priority: high, War threshold: moderate, Research: most expensive tech.

economic = {
    name = "Economic",

    -- Trade and diplomacy
    trade_priority = 0.7,       -- strong trade focus
    war_declaration_interval = 30, -- moderate war frequency
    alliance_preference = 0.6,  -- moderate alliance preference

    -- Military
    min_army_size = 3,          -- moderate standing army
    max_army_size = 6,          -- moderate military
    preferred_unit = nil,       -- no preference

    -- Economy
    infrastructure_budget = 3000, -- heavy infrastructure investment
    worker_threshold = 3,       -- recruits workers aggressively

    -- Research
    research_strategy = "expensive", -- pick most expensive available tech
}

function economic.evaluate_war(nation_id, target_id, relations)
    -- Economic: moderate war threshold
    if relations < -40 then
        return true
    end
    return false
end

function economic.pick_tech(available_techs)
    -- Pick most expensive tech (invest in the future)
    local best = nil
    local max_cost = -1
    for _, tech in ipairs(available_techs) do
        if tech.cost > max_cost then
            max_cost = tech.cost
            best = tech
        end
    end
    return best
end

return economic
