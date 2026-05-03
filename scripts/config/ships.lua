-- Naval unit definitions, source of truth for ship stats.
--
-- Stats mirror the original Imperialism (1997) manual where documented.
-- era (1|2|3|4) — historical bucket used by the AI and UI:
--   Era 1: pre-industrial sail (Trader, Indiaman, Frigate, ShipOfTheLine)
--   Era 2: early steam/iron (Clipper, Paddlewheeler, Raider, Ironclad)
--   Era 3: steel-hulled (AdvancedIronclad, ArmouredCruiser, Freighter)
--   Era 4: dreadnought (Dreadnought, Battlecruiser)
--
-- category: "Merchant" | "Warship"
-- firepower, range, armor, hull, speed, cargo — combat/transport stats.
-- fabric_cost, lumber_cost, arms_cost, steel_cost, coal_cost — build costs.
-- prerequisite_tech — tech name required to unlock, nil = available from start.

ships = {
  -- ── Merchant ────────────────────────────────────────────────
  { name = "Trader",        category = "Merchant", era = 1,
    firepower = 0, range = 0, armor =  0, hull = 25, speed = 0, cargo =  2,
    fabric_cost = 2, lumber_cost = 4, arms_cost = 0, steel_cost = 0, coal_cost =  0,
    prerequisite_tech = nil },

  { name = "Indiaman",      category = "Merchant", era = 1,
    firepower = 0, range = 0, armor =  5, hull = 40, speed = 0, cargo =  4,
    fabric_cost = 3, lumber_cost = 7, arms_cost = 0, steel_cost = 0, coal_cost =  0,
    prerequisite_tech = nil },

  { name = "Clipper",       category = "Merchant", era = 2,
    firepower = 0, range = 0, armor =  0, hull = 25, speed = 0, cargo =  4,
    fabric_cost = 2, lumber_cost = 6, arms_cost = 0, steel_cost = 0, coal_cost =  0,
    prerequisite_tech = "Streamlined Hulls" },

  { name = "Paddlewheeler", category = "Merchant", era = 2,
    firepower = 0, range = 0, armor =  5, hull = 35, speed = 0, cargo =  8,
    fabric_cost = 0, lumber_cost = 6, arms_cost = 0, steel_cost = 2, coal_cost = 10,
    prerequisite_tech = "Paddlewheels" },

  { name = "Freighter",     category = "Merchant", era = 3,
    firepower = 0, range = 0, armor = 10, hull = 50, speed = 0, cargo = 12,
    fabric_cost = 0, lumber_cost = 8, arms_cost = 0, steel_cost = 4, coal_cost = 15,
    prerequisite_tech = "Marine Engineering" },

  -- ── Warship ─────────────────────────────────────────────────
  { name = "Frigate",           category = "Warship", era = 1,
    firepower =  3, range =  5, armor = 10, hull = 35, speed = 2, cargo = 0,
    fabric_cost = 2, lumber_cost = 5, arms_cost = 2, steel_cost = 0, coal_cost =  0,
    prerequisite_tech = nil },

  { name = "ShipOfTheLine",     category = "Warship", era = 1,
    firepower =  6, range =  6, armor = 20, hull = 65, speed = 2, cargo = 0,
    fabric_cost = 3, lumber_cost = 8, arms_cost = 5, steel_cost = 0, coal_cost =  0,
    prerequisite_tech = nil },

  { name = "Raider",            category = "Warship", era = 2,
    firepower =  3, range =  7, armor = 20, hull = 30, speed = 3, cargo = 0,
    fabric_cost = 0, lumber_cost = 6, arms_cost = 3, steel_cost = 0, coal_cost = 10,
    prerequisite_tech = "Paddlewheels" },

  { name = "Ironclad",          category = "Warship", era = 2,
    firepower =  8, range =  7, armor = 30, hull = 50, speed = 3, cargo = 0,
    fabric_cost = 0, lumber_cost = 6, arms_cost = 4, steel_cost = 3, coal_cost = 12,
    prerequisite_tech = "Advanced Iron Working" },

  { name = "AdvancedIronclad",  category = "Warship", era = 3,
    firepower = 10, range =  8, armor = 40, hull = 60, speed = 3, cargo = 0,
    fabric_cost = 0, lumber_cost = 6, arms_cost = 5, steel_cost = 4, coal_cost = 15,
    prerequisite_tech = "Steel Armour Plate" },

  { name = "ArmouredCruiser",   category = "Warship", era = 3,
    firepower =  8, range =  9, armor = 35, hull = 55, speed = 3, cargo = 0,
    fabric_cost = 0, lumber_cost = 7, arms_cost = 4, steel_cost = 5, coal_cost = 15,
    prerequisite_tech = "Marine Engineering" },

  { name = "Dreadnought",       category = "Warship", era = 4,
    firepower = 15, range = 10, armor = 50, hull = 80, speed = 3, cargo = 0,
    fabric_cost = 0, lumber_cost = 10, arms_cost = 8, steel_cost = 8, coal_cost = 20,
    prerequisite_tech = "Improved Range-Finding" },

  { name = "Battlecruiser",     category = "Warship", era = 4,
    firepower = 12, range = 10, armor = 40, hull = 65, speed = 4, cargo = 0,
    fabric_cost = 0, lumber_cost = 8, arms_cost = 6, steel_cost = 6, coal_cost = 18,
    prerequisite_tech = "Improved Range-Finding" },
}
