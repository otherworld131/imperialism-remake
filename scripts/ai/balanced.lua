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

    -- Economy
    expansion_threshold_multiplier = 2,

    -- Tactical
    peace_war_duration_threshold = 20,
    peace_province_loss_ratio = 0.50,
    fort_strategy = "border",
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

return balanced
