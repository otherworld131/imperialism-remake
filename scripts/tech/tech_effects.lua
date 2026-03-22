-- Tech effect scripts — per-technology on_researched callbacks
--
-- Each function is called when its corresponding technology is researched.
-- Functions receive the nation_id and can use the game API to query state
-- and apply effects.

-- High Pressure Steam Engine: Enables railroad construction
function on_research_steam_engine(nation_id)
    game.log("Nation " .. nation_id .. " researched High Pressure Steam Engine — railroads enabled")
    return "railroad_enabled"
end

-- Seed Drill: Enables farm improvement level 1
function on_research_seed_drill(nation_id)
    game.log("Nation " .. nation_id .. " researched Seed Drill — farms can be improved")
    return "farm_improvement_1"
end

-- Bessemer Converter: Unlocks Steel Mill
function on_research_bessemer(nation_id)
    game.log("Nation " .. nation_id .. " researched Bessemer Converter — steel mills unlocked")
    return "steel_mill_unlocked"
end

-- Machine Guns: Unlocks Machine Gun Corps
function on_research_machine_guns(nation_id)
    game.log("Nation " .. nation_id .. " researched Machine Guns — machine gunners available")
    return "machine_gunners_unlocked"
end

-- Internal Combustion: Unlocks motorized units
function on_research_internal_combustion(nation_id)
    game.log("Nation " .. nation_id .. " researched Internal Combustion — motorized infantry available")
    return "motorized_unlocked"
end

-- Improved Range-Finding: Unlocks Dreadnought
function on_research_range_finding(nation_id)
    game.log("Nation " .. nation_id .. " researched Improved Range-Finding — dreadnoughts available")
    return "dreadnought_unlocked"
end
