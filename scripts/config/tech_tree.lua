-- Technology Tree
--
-- Single source of truth for the Imperialism (1997) tech tree.
-- The Rust loader (`load_tech_tree` in `ai/lua_bridge.rs`) reads this table at
-- startup. There is no hardcoded Rust fallback: if this file is malformed the
-- game starts with an empty tech tree and logs a warning (which means
-- tech-gating effectively turns into "everything locked").
--
-- Schema:
--   tech_tree = {
--     { id, name, cost, earliest_year, latest_year,
--       prerequisites = { ... },
--       effects = { { kind = "...", ... }, ... } },
--     ...
--   }
--
-- Effect kinds (must match `TechEffect` in `crates/domain/src/tech/tree.rs`):
--   { kind = "EnableInfrastructure", value = "Railroad" }
--   { kind = "EnableTerrainImprovement", terrain = "Farm", max_level = 2 }
--   { kind = "UnlockShip", value = "Clipper" }
--   { kind = "UnlockBuilding", value = "TextileMill" }
--   { kind = "UnlockUnit", value = "Mechanised" }
--   { kind = "UpgradeUnit", from = "Regulars", to = "RifleInfantry" }
--   { kind = "EnableCivilian", value = "Forester" }
--   { kind = "LuaScript", value = "do_something()" }
--
-- Resource-development gates (manual p.89, "Benefits of Technology Table"):
--   "Farm"        — Grain on Grassland.
--   "Orchard"     — Fruit on Grassland.
--   "Plantation"  — Cotton on Grassland.
--   "Wool"        — Wool on Hills (sheep).
--   "Livestock"   — Livestock and Horses on Grassland.
--   "Forest"      — Timber in hardwood forest.
--   "Mining"      — Coal/Iron/Gold/Gems on Hills/Mountain.
--   "Oil"         — Oil on Desert/Swamp/Tundra.

tech_tree = {
    -- 1814 — starting techs.
    { id = 1, name = "High Pressure Steam Engine", cost = 0, earliest_year = 1815, latest_year = 1815, prerequisites = {},
      effects = { { kind = "EnableInfrastructure", value = "Railroad" } } },

    { id = 2, name = "Seed Drill", cost = 0, earliest_year = 1815, latest_year = 1815, prerequisites = {},
      effects = {
          { kind = "EnableTerrainImprovement", terrain = "Farm", max_level = 1 },
          { kind = "EnableTerrainImprovement", terrain = "Orchard", max_level = 1 },
      } },

    { id = 3, name = "Cotton Gin", cost = 1000, earliest_year = 1816, latest_year = 1820, prerequisites = {},
      effects = { { kind = "EnableTerrainImprovement", terrain = "Plantation", max_level = 1 } } },

    -- 1821-25
    { id = 4, name = "Iron Railroad Bridge", cost = 1500, earliest_year = 1821, latest_year = 1825, prerequisites = { 1 },
      effects = {
          { kind = "EnableInfrastructure", value = "Railroad Bridge" },
          { kind = "EnableCivilian", value = "Forester" },
          { kind = "EnableTerrainImprovement", terrain = "Forest", max_level = 1 },
      } },

    { id = 5, name = "Feed Grasses", cost = 1500, earliest_year = 1821, latest_year = 1825, prerequisites = {},
      effects = {
          { kind = "EnableCivilian", value = "Rancher" },
          { kind = "EnableTerrainImprovement", terrain = "Wool", max_level = 1 },
          { kind = "EnableTerrainImprovement", terrain = "Livestock", max_level = 1 },
      } },

    { id = 6, name = "Square-Set Timbering", cost = 1500, earliest_year = 1821, latest_year = 1825, prerequisites = { 1 },
      effects = { { kind = "EnableTerrainImprovement", terrain = "Mining", max_level = 2 } } },

    { id = 7, name = "Streamlined Hulls", cost = 1500, earliest_year = 1821, latest_year = 1825, prerequisites = {},
      effects = { { kind = "UnlockShip", value = "Clipper" } } },

    -- 1826-30
    { id = 8, name = "Spinning Jenny", cost = 3000, earliest_year = 1826, latest_year = 1830, prerequisites = { 3, 5 },
      effects = {
          { kind = "UnlockBuilding", value = "TextileMill" },
          { kind = "EnableTerrainImprovement", terrain = "Plantation", max_level = 2 },
          { kind = "EnableTerrainImprovement", terrain = "Wool", max_level = 2 },
      } },

    { id = 9, name = "Paddlewheels", cost = 3000, earliest_year = 1826, latest_year = 1830, prerequisites = {},
      effects = { { kind = "UnlockShip", value = "Paddlewheeler" } } },

    -- 1831-35
    { id = 10, name = "Steel and Iron Plows", cost = 3000, earliest_year = 1831, latest_year = 1835, prerequisites = { 2 },
      effects = {
          { kind = "EnableTerrainImprovement", terrain = "Farm", max_level = 2 },
          { kind = "EnableTerrainImprovement", terrain = "Orchard", max_level = 2 },
      } },

    -- 1836-40
    { id = 11, name = "Bessemer Converter", cost = 6000, earliest_year = 1836, latest_year = 1840, prerequisites = {},
      effects = { { kind = "UnlockBuilding", value = "SteelMill" } } },

    { id = 12, name = "Compound Steam Engine", cost = 7000, earliest_year = 1836, latest_year = 1840, prerequisites = { 4 },
      effects = {
          { kind = "EnableInfrastructure", value = "Advanced Railroad" },
          { kind = "EnableTerrainImprovement", terrain = "Forest", max_level = 2 },
      } },

    -- 1841-45
    { id = 13, name = "Breech-Loading Rifles", cost = 12000, earliest_year = 1841, latest_year = 1845, prerequisites = { 11 },
      effects = { { kind = "UpgradeUnit", from = "Regulars", to = "RifleInfantry" } } },

    { id = 14, name = "Rifled Artillery", cost = 10000, earliest_year = 1841, latest_year = 1845, prerequisites = {},
      effects = { { kind = "UpgradeUnit", from = "LightArtillery", to = "Artillery" } } },

    -- 1846-50
    { id = 15, name = "Advanced Iron Working", cost = 12000, earliest_year = 1846, latest_year = 1850, prerequisites = {},
      effects = { { kind = "UnlockShip", value = "Ironclad" } } },

    { id = 16, name = "Power Loom", cost = 12000, earliest_year = 1846, latest_year = 1850, prerequisites = { 8 },
      effects = {
          { kind = "UnlockBuilding", value = "AdvancedTextileMill" },
          { kind = "EnableTerrainImprovement", terrain = "Plantation", max_level = 3 },
          { kind = "EnableTerrainImprovement", terrain = "Wool", max_level = 3 },
      } },

    -- 1851-55
    { id = 17, name = "Mechanical Reaper", cost = 12000, earliest_year = 1851, latest_year = 1855, prerequisites = { 10 },
      effects = { { kind = "EnableTerrainImprovement", terrain = "Farm", max_level = 3 } } },

    -- 1856-60
    { id = 18, name = "Commercial Fertilizer", cost = 12000, earliest_year = 1856, latest_year = 1860, prerequisites = { 10 },
      effects = { { kind = "EnableTerrainImprovement", terrain = "Orchard", max_level = 3 } } },

    { id = 19, name = "Oil Drilling", cost = 25000, earliest_year = 1856, latest_year = 1860, prerequisites = {},
      effects = {
          { kind = "EnableCivilian", value = "Driller" },
          { kind = "EnableTerrainImprovement", terrain = "Oil", max_level = 1 },
      } },

    -- 1861-65
    { id = 20, name = "Barbed Wire", cost = 20000, earliest_year = 1861, latest_year = 1865, prerequisites = { 5 },
      effects = { { kind = "EnableTerrainImprovement", terrain = "Livestock", max_level = 2 } } },

    -- 1866-70
    { id = 21, name = "Steel Armour Plate", cost = 40000, earliest_year = 1866, latest_year = 1870, prerequisites = { 15 },
      effects = { { kind = "UnlockShip", value = "Advanced Ironclad" } } },

    -- 1871-75
    { id = 22, name = "Large Artillery", cost = 40000, earliest_year = 1871, latest_year = 1875, prerequisites = { 14 },
      effects = { { kind = "UnlockUnit", value = "SiegeArtillery" } } },

    { id = 23, name = "Dynamite", cost = 40000, earliest_year = 1871, latest_year = 1875, prerequisites = { 12, 6 },
      effects = {
          { kind = "EnableTerrainImprovement", terrain = "Mining", max_level = 3 },
          { kind = "EnableTerrainImprovement", terrain = "Forest", max_level = 3 },
      } },

    { id = 24, name = "Marine Engineering", cost = 40000, earliest_year = 1871, latest_year = 1875, prerequisites = { 21 },
      effects = { { kind = "UnlockShip", value = "Armoured Cruiser" } } },

    -- 1876-80
    { id = 25, name = "Machine Guns", cost = 100000, earliest_year = 1876, latest_year = 1880, prerequisites = { 13 },
      effects = { { kind = "UnlockUnit", value = "MachineGunners" } } },

    { id = 26, name = "Chemistry", cost = 120000, earliest_year = 1876, latest_year = 1880, prerequisites = { 19, 20 },
      effects = {
          { kind = "UnlockBuilding", value = "ChemicalPlant" },
          { kind = "EnableTerrainImprovement", terrain = "Oil", max_level = 2 },
          { kind = "EnableTerrainImprovement", terrain = "Livestock", max_level = 3 },
      } },

    -- 1881-85
    { id = 27, name = "Improved Range-Finding", cost = 150000, earliest_year = 1881, latest_year = 1885, prerequisites = { 24 },
      effects = { { kind = "UnlockShip", value = "Dreadnought" } } },

    { id = 28, name = "Internal Combustion", cost = 150000, earliest_year = 1881, latest_year = 1885, prerequisites = { 26 },
      effects = {
          { kind = "UnlockUnit", value = "Mechanised" },
          { kind = "EnableTerrainImprovement", terrain = "Oil", max_level = 3 },
      } },
}
