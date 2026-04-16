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

-- Cotton Gin: Enables plantation improvement
function on_research_cotton_gin(nation_id)
    game.log("Nation " .. nation_id .. " researched Cotton Gin — plantations can be improved")
    return "plantation_enabled"
end

-- Iron Railroad Bridge: Enables railroad bridges
function on_research_iron_railroad_bridge(nation_id)
    game.log("Nation " .. nation_id .. " researched Iron Railroad Bridge — bridges enabled")
    return "railroad_bridge_enabled"
end

-- Feed Grasses: Enables open range improvement
function on_research_feed_grasses(nation_id)
    game.log("Nation " .. nation_id .. " researched Feed Grasses — open range can be improved")
    return "open_range_enabled"
end

-- Square-Set Timbering: Enables mountain improvement level 2
function on_research_square_set_timbering(nation_id)
    game.log("Nation " .. nation_id .. " researched Square-Set Timbering — mountain mining improved")
    return "mountain_improvement_2"
end

-- Streamlined Hulls: Unlocks Clipper
function on_research_streamlined_hulls(nation_id)
    game.log("Nation " .. nation_id .. " researched Streamlined Hulls — clippers available")
    return "clipper_unlocked"
end

-- Spinning Jenny: Unlocks Textile Mill
function on_research_spinning_jenny(nation_id)
    game.log("Nation " .. nation_id .. " researched Spinning Jenny — textile mills unlocked")
    return "textile_mill_unlocked"
end

-- Paddlewheels: Unlocks Paddlewheeler
function on_research_paddlewheels(nation_id)
    game.log("Nation " .. nation_id .. " researched Paddlewheels — paddlewheelers available")
    return "paddlewheeler_unlocked"
end

-- Steel Plows: Enables farm improvement level 2
function on_research_steel_plows(nation_id)
    game.log("Nation " .. nation_id .. " researched Steel Plows — farms improved further")
    return "farm_improvement_2"
end

-- Compound Steam Engine: Enables advanced railroad
function on_research_compound_steam_engine(nation_id)
    game.log("Nation " .. nation_id .. " researched Compound Steam Engine — advanced railroads enabled")
    return "advanced_railroad_enabled"
end

-- Breech-Loading Rifles: Upgrades infantry
function on_research_breech_loading_rifles(nation_id)
    game.log("Nation " .. nation_id .. " researched Breech-Loading Rifles — rifle infantry available")
    return "rifle_infantry_unlocked"
end

-- Rifled Artillery: Upgrades artillery
function on_research_rifled_artillery(nation_id)
    game.log("Nation " .. nation_id .. " researched Rifled Artillery — standard artillery available")
    return "standard_artillery_unlocked"
end

-- Advanced Iron Working: Unlocks Ironclad
function on_research_advanced_iron_working(nation_id)
    game.log("Nation " .. nation_id .. " researched Advanced Iron Working — ironclads available")
    return "ironclad_unlocked"
end

-- Power Loom: Unlocks advanced textile production
function on_research_power_loom(nation_id)
    game.log("Nation " .. nation_id .. " researched Power Loom — advanced textiles unlocked")
    return "advanced_textile_mill_unlocked"
end

-- Mechanical Reaper: Enables farm improvement level 3
function on_research_mechanical_reaper(nation_id)
    game.log("Nation " .. nation_id .. " researched Mechanical Reaper — maximum farm output")
    return "farm_improvement_3"
end

-- Commercial Fertilizer: Enables orchard improvement level 3
function on_research_commercial_fertilizer(nation_id)
    game.log("Nation " .. nation_id .. " researched Commercial Fertilizer — orchards improved")
    return "orchard_improvement_3"
end

-- Oil Drilling: Enables desert oil extraction
function on_research_oil_drilling(nation_id)
    game.log("Nation " .. nation_id .. " researched Oil Drilling — oil wells enabled")
    return "oil_drilling_enabled"
end

-- Barbed Wire: Enables open range improvement level 2
function on_research_barbed_wire(nation_id)
    game.log("Nation " .. nation_id .. " researched Barbed Wire — ranches improved")
    return "open_range_improvement_2"
end

-- Steel Armour Plate: Unlocks Advanced Ironclad
function on_research_steel_armour_plate(nation_id)
    game.log("Nation " .. nation_id .. " researched Steel Armour Plate — advanced ironclads available")
    return "advanced_ironclad_unlocked"
end

-- Large Artillery: Unlocks Siege Artillery
function on_research_large_artillery(nation_id)
    game.log("Nation " .. nation_id .. " researched Large Artillery — siege artillery available")
    return "siege_artillery_unlocked"
end

-- Dynamite: Enables mountain improvement level 3
function on_research_dynamite(nation_id)
    game.log("Nation " .. nation_id .. " researched Dynamite — deep mountain mining enabled")
    return "mountain_improvement_3"
end

-- Marine Engineering: Unlocks Armoured Cruiser and Freighter
function on_research_marine_engineering(nation_id)
    game.log("Nation " .. nation_id .. " researched Marine Engineering — cruisers and freighters available")
    return "armoured_cruiser_unlocked"
end

-- Improved Range-Finding: Unlocks Dreadnought
function on_research_range_finding(nation_id)
    game.log("Nation " .. nation_id .. " researched Improved Range-Finding — dreadnoughts available")
    return "dreadnought_unlocked"
end

-- Chemistry: Unlocks Chemical Plant
function on_research_chemistry(nation_id)
    game.log("Nation " .. nation_id .. " researched Chemistry — chemical plants unlocked")
    return "chemical_plant_unlocked"
end
