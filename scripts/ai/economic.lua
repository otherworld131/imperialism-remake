-- AI Personality: Economic
--
-- Invests heavily in production and technology.
-- Trade priority: high, War threshold: moderate, Research: most expensive tech.

economic = {
    name = "Economic",

    -- Trade and diplomacy
    trade_priority = 0.7,       -- strong trade focus
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

    -- War (cooldown-based system)
    war_cooldown = 15,          -- moderate cooldown
    war_threshold = 0.6,        -- moderate-high bar for war
    army_min_for_war = 5,       -- needs good army
    opportunism_weight = 0.4,   -- modest exploitation of weakness

    -- Army building tiers
    tier1_army_max = 3,
    tier2_army_max = 5,
    tier3_army_max = 10,
    tier1_treasury = 2500,
    tier2_treasury = 6000,
    tier3_treasury = 12000,

    -- Diplomacy
    consulate_max_per_turn = 2,
    propose_pacts = true,
    propose_alliances = false,
    grant_amount = 500,
    grant_interval = 6,
    embassy_treasury_threshold = 10000,
    max_alliances = 1,

    -- Naval
    max_warships_low_treasury = 2,
    max_warships_high_treasury = 4,
    max_merchant_ships = 3,

    -- Economy
    expansion_threshold_multiplier = 1,

    -- Tactical
    peace_war_duration_threshold = 20,
    peace_province_loss_ratio = 0.50,
    fort_strategy = "border",
}

function economic.evaluate_war(nation_id, target_id, relations, need_score, opportunity_score)
    -- Use need/opportunity if available (new system)
    if need_score and opportunity_score then
        local score = (need_score or 0) + (opportunity_score or 0) * 0.4
        if score > 0.6 then
            return true
        end
    end
    -- Fallback: moderate war threshold
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
