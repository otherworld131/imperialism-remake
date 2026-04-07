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

    -- Army building tiers
    tier1_army_max = 4,
    tier2_army_max = 7,
    tier3_army_max = 15,
    tier1_treasury = 1500,
    tier2_treasury = 3000,
    tier3_treasury = 6000,

    -- Diplomacy
    consulate_max_per_turn = 2,
    propose_pacts = false,
    propose_alliances = false,
    grant_amount = 0,
    grant_interval = 0,
    embassy_treasury_threshold = 15000,
    max_alliances = 0,

    -- Naval
    max_warships_low_treasury = 4,
    max_warships_high_treasury = 6,
    max_merchant_ships = 1,

    -- Economy
    expansion_threshold_multiplier = 2,

    -- Tactical
    peace_war_duration_threshold = 30,
    peace_province_loss_ratio = 0.50,
    fort_strategy = "offensive",
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

return aggressive
