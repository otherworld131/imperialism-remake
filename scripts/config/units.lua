-- Land military unit definitions, source of truth.
--
-- Stat columns mirror the original Imperialism (1997) manual:
--   firepower (FPN)            base attack vs infantry/garrison
--   firepower_mounted (FPM)    bonus attack vs cavalry / when charging.
--                              Currently NOT consumed by combat resolution
--                              (see Trello "wire up FPM in combat").
--   defense (DEF)              base defensive value. Currently NOT consumed
--                              (see Trello "wire up DEF & terrain bonus").
--   defense_terrain_bonus      extra DEF when defending in favorable terrain
--                              (forest/hills/fort). Bracketed value in the
--                              manual is the bonus, not the total. Currently
--                              NOT consumed.
--   range, movement, arms_required  — used by current code.
--   cost (dollars), maintenance_per_turn (dollars)  — project-specific.
--   requires_horse              — recruit needs a Horse resource.
--   prerequisite_tech           — tech name that must be researched.
--   era (1|2|3)                 — historical bucket; drives upgrade gating.
--
-- The dollar costs and maintenance values are project-specific (the original
-- manual table doesn't list them). Resource costs (e.g. Steel, Coal for
-- mechanised units) are out of scope here and live with the production code.
--
-- Per-role 3-era upgrade chains:
--   Garrison:   Minutemen      -> Militia        -> Conscript
--   Skirmisher: Skirmishers    -> Sharpshooters  -> Rangers
--   Line:       Regulars       -> RifleInfantry  -> Infantry
--   Elite:      Grenadiers     -> Guards         -> MachineGunners
--   Light cav:  Hussars        -> Carbineers     -> Mechanised
--   Heavy cav:  Cuirassiers    -> Armour         (no Era II in the original)
--   Light arty: LightArtillery -> FieldArtillery -> MobileArtillery
--   Heavy arty: Artillery      -> SiegeArtillery -> RailroadGuns
--   Engineer:   Sapper         -> CombatEngineer -> Saboteur

units = {
  -- ── Garrison ──────────────────────────────────────────────
  { name = "Minutemen",      category = "Garrison",  era = 1,
    firepower =  5, firepower_mounted = 0, defense =  5, defense_terrain_bonus = 0,
    range = 1, movement = 0, arms_required = 0, requires_horse = false,
    cost = 0, maintenance_per_turn = 0, prerequisite_tech = nil },
  { name = "Militia",        category = "Garrison",  era = 2,
    firepower = 10, firepower_mounted = 0, defense =  7, defense_terrain_bonus = 0,
    range = 2, movement = 0, arms_required = 1, requires_horse = false,
    cost = 0, maintenance_per_turn = 0, prerequisite_tech = nil },
  { name = "Conscript",      category = "Garrison",  era = 3,
    firepower = 17, firepower_mounted = 0, defense =  5, defense_terrain_bonus = 0,
    range = 2, movement = 4, arms_required = 1, requires_horse = false,
    cost = 100, maintenance_per_turn = 25, prerequisite_tech = "Modern Warfare" },

  -- ── Skirmisher infantry ─────────────────────────────────
  { name = "Skirmishers",    category = "Infantry",  era = 1,
    firepower =  5, firepower_mounted = 0, defense =  5, defense_terrain_bonus = 0,
    range = 1, movement = 4, arms_required = 1, requires_horse = false,
    cost = 100, maintenance_per_turn = 25, prerequisite_tech = nil },
  { name = "Sharpshooters",  category = "Infantry",  era = 2,
    firepower = 11, firepower_mounted = 0, defense =  5, defense_terrain_bonus = 0,
    range = 3, movement = 4, arms_required = 1, requires_horse = false,
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = "Sharpshooter Training" },
  { name = "Rangers",        category = "Infantry",  era = 3,
    firepower = 15, firepower_mounted = 0, defense = 10, defense_terrain_bonus = 0,
    range = 5, movement = 4, arms_required = 1, requires_horse = false,
    cost = 250, maintenance_per_turn = 50, prerequisite_tech = "Ranger Training" },

  -- ── Line infantry ───────────────────────────────────────
  { name = "Regulars",       category = "Infantry",  era = 1,
    firepower = 10, firepower_mounted = 5, defense =  5, defense_terrain_bonus = 0,
    range = 1, movement = 4, arms_required = 1, requires_horse = false,
    cost = 100, maintenance_per_turn = 25, prerequisite_tech = nil },
  { name = "RifleInfantry",  category = "Infantry",  era = 2,
    firepower = 15, firepower_mounted = 0, defense =  7, defense_terrain_bonus = 1,
    range = 2, movement = 4, arms_required = 1, requires_horse = false,
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = "Rifling" },
  { name = "Infantry",       category = "Infantry",  era = 3,
    firepower = 22, firepower_mounted = 0, defense = 10, defense_terrain_bonus = 0,
    range = 2, movement = 4, arms_required = 2, requires_horse = false,
    cost = 300, maintenance_per_turn = 75, prerequisite_tech = "Modern Warfare" },

  -- ── Elite/heavy infantry ────────────────────────────────
  { name = "Grenadiers",     category = "Infantry",  era = 1,
    firepower = 12, firepower_mounted = 0, defense =  5, defense_terrain_bonus = 1,
    range = 1, movement = 4, arms_required = 1, requires_horse = false,
    cost = 150, maintenance_per_turn = 50, prerequisite_tech = "Grenadier Tactics" },
  { name = "Guards",         category = "Infantry",  era = 2,
    firepower = 17, firepower_mounted = 0, defense =  9, defense_terrain_bonus = 0,
    range = 2, movement = 4, arms_required = 1, requires_horse = false,
    cost = 250, maintenance_per_turn = 75, prerequisite_tech = "Professional Army" },
  { name = "MachineGunners", category = "Infantry",  era = 3,
    firepower = 28, firepower_mounted = 0, defense = 12, defense_terrain_bonus = 0,
    range = 2, movement = 4, arms_required = 2, requires_horse = false,
    cost = 400, maintenance_per_turn = 100, prerequisite_tech = "Machine Guns" },

  -- ── Light cavalry ───────────────────────────────────────
  { name = "Hussars",        category = "Cavalry",   era = 1,
    firepower =  7, firepower_mounted = 10, defense =  4, defense_terrain_bonus = 0,
    range = 1, movement = 7, arms_required = 1, requires_horse = true,
    cost = 150, maintenance_per_turn = 25, prerequisite_tech = nil },
  { name = "Carbineers",     category = "Cavalry",   era = 2,
    firepower = 11, firepower_mounted = 13, defense =  7, defense_terrain_bonus = 0,
    range = 2, movement = 7, arms_required = 1, requires_horse = true,
    cost = 250, maintenance_per_turn = 50, prerequisite_tech = "Carbines" },
  { name = "Mechanised",     category = "Cavalry",   era = 3,
    firepower = 30, firepower_mounted = 0, defense = 10, defense_terrain_bonus = 2,
    range = 4, movement = 6, arms_required = 2, requires_horse = false,
    cost = 400, maintenance_per_turn = 75, prerequisite_tech = "Mechanisation" },

  -- ── Heavy cavalry ───────────────────────────────────────
  { name = "Cuirassiers",    category = "Cavalry",   era = 1,
    firepower = 15, firepower_mounted = 0, defense =  9, defense_terrain_bonus = 0,
    range = 1, movement = 7, arms_required = 1, requires_horse = true,
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = nil },
  { name = "Armour",         category = "Cavalry",   era = 3,
    firepower = 30, firepower_mounted = 0, defense = 16, defense_terrain_bonus = 0,
    range = 6, movement = 6, arms_required = 4, requires_horse = false,
    cost = 500, maintenance_per_turn = 100, prerequisite_tech = "Armoured Vehicles" },

  -- ── Light artillery ─────────────────────────────────────
  { name = "LightArtillery", category = "Artillery", era = 1,
    firepower = 10, firepower_mounted = 0, defense =  9, defense_terrain_bonus = 0,
    range = 3, movement = 3, arms_required = 2, requires_horse = false,
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = nil },
  { name = "FieldArtillery", category = "Artillery", era = 2,
    firepower = 17, firepower_mounted = 0, defense = 12, defense_terrain_bonus = 1,
    range = 5, movement = 3, arms_required = 2, requires_horse = false,
    cost = 350, maintenance_per_turn = 75, prerequisite_tech = "Field Artillery" },
  { name = "MobileArtillery",category = "Artillery", era = 3,
    firepower = 22, firepower_mounted = 0, defense = 12, defense_terrain_bonus = 1,
    range = 5, movement = 4, arms_required = 2, requires_horse = false,
    cost = 450, maintenance_per_turn = 100, prerequisite_tech = "Mobile Artillery" },

  -- ── Heavy artillery ─────────────────────────────────────
  { name = "Artillery",      category = "Artillery", era = 1,
    firepower = 16, firepower_mounted = 0, defense = 11, defense_terrain_bonus = 1,
    range = 4, movement = 2, arms_required = 2, requires_horse = false,
    cost = 300, maintenance_per_turn = 75, prerequisite_tech = "Improved Artillery" },
  { name = "SiegeArtillery", category = "Artillery", era = 2,
    firepower = 21, firepower_mounted = 0, defense =  9, defense_terrain_bonus = 11,
    range = 6, movement = 2, arms_required = 3, requires_horse = false,
    cost = 500, maintenance_per_turn = 100, prerequisite_tech = "Siege Warfare" },
  { name = "RailroadGuns",   category = "Artillery", era = 3,
    firepower = 50, firepower_mounted = 0, defense = 20, defense_terrain_bonus = 5,
    range = 17, movement = 0, arms_required = 4, requires_horse = false,
    cost = 600, maintenance_per_turn = 125, prerequisite_tech = "Railroad Artillery" },

  -- ── Engineer ────────────────────────────────────────────
  { name = "Sapper",         category = "Special",   era = 1,
    firepower =  5, firepower_mounted = 0, defense =  4, defense_terrain_bonus = 0,
    range = 1, movement = 4, arms_required = 1, requires_horse = false,
    cost = 150, maintenance_per_turn = 25, prerequisite_tech = "Engineering" },
  { name = "CombatEngineer", category = "Special",   era = 2,
    firepower =  7, firepower_mounted = 0, defense =  7, defense_terrain_bonus = 0,
    range = 2, movement = 4, arms_required = 1, requires_horse = false,
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = "Engineering" },
  { name = "Saboteur",       category = "Special",   era = 3,
    firepower =  9, firepower_mounted = 0, defense = 10, defense_terrain_bonus = 2,
    range = 1, movement = 4, arms_required = 1, requires_horse = false,
    cost = 250, maintenance_per_turn = 50, prerequisite_tech = "Modern Warfare" },

  -- ── Special ─────────────────────────────────────────────
  { name = "General",        category = "Special",   era = 1,
    firepower =  0, firepower_mounted = 0, defense =  0, defense_terrain_bonus = 0,
    range = 0, movement = 8, arms_required = 0, requires_horse = false,
    cost = 0, maintenance_per_turn = 0, prerequisite_tech = nil },

  -- ── Project-specific: minor-nation capital defense ──────
  -- Not in the original manual roster. Auto-spawned at minor-nation capitals;
  -- never recruited by the player.
  { name = "GarrisonArtillery", category = "Garrison", era = 1,
    firepower =  4, firepower_mounted = 0, defense =  0, defense_terrain_bonus = 0,
    range = 3, movement = 0, arms_required = 0, requires_horse = false,
    cost = 0, maintenance_per_turn = 0, prerequisite_tech = nil },
}
