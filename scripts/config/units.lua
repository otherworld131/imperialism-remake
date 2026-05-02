-- Land military unit definitions, source of truth.
--
-- Stat columns mirror the original Imperialism (1997) manual:
--   firepower (FPN)            base attack vs infantry/garrison
--   firepower_mounted (FPM)    melee attack when adjacent (range 1).
--   defense (DEF)              base defensive multiplier.
--   defense_terrain_bonus      extra DEF when defending in favorable terrain
--                              (forest/hills/fort). Bracketed value in the
--                              manual is the bonus, not the total.
--   range, movement, arms_required  — used by current code.
--   requires_horse              — recruit needs a Horse resource.
--   fuel_required               — Oil units consumed at recruitment.
--   recruit_tier                — labor tier consumed: Untrained, Trained, Expert.
--   cost (dollars), maintenance_per_turn (dollars)  — project-specific.
--   prerequisite_tech           — thematic label (informational only; actual
--                                 tech gating uses required_tech() in Rust).
--   era (1|2|3)                 — historical bucket; drives upgrade gating.
--
-- Per-role 3-era upgrade chains:
--   Garrison:   Minutemen      -> Militia        -> Conscript
--   Skirmisher: Skirmishers    -> Sharpshooters  -> Rangers
--   Line:       Regulars       -> RifleInfantry  -> Infantry
--   Elite:      Grenadiers     -> Guards         -> MachineGunners
--   Light cav:  Hussars        -> Carbineers     -> Mechanised
--   Light cav2: Scouts (standalone recon, Era II)
--   Heavy cav:  Cuirassiers    -> Armour         (no Era II in the original)
--   Light arty: LightArtillery -> FieldArtillery -> MobileArtillery
--   Horse arty: HorseArtillery (standalone, Era II)
--   Heavy arty: Artillery      -> SiegeArtillery -> RailroadGuns
--   Engineer:   Sapper         -> CombatEngineer -> Saboteur
--   Commandos: standalone Era III special forces

units = {
  -- ── Garrison ──────────────────────────────────────────────
  { name = "Minutemen",      category = "Garrison",  era = 1,
    firepower =  5, firepower_mounted = 5,  defense =  4, defense_terrain_bonus = 1,
    range = 1, movement = 0, arms_required = 0, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 0, maintenance_per_turn = 0, prerequisite_tech = nil },
  { name = "Militia",        category = "Garrison",  era = 2,
    firepower =  7, firepower_mounted = 7,  defense =  4, defense_terrain_bonus = 1,
    range = 2, movement = 0, arms_required = 0, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 0, maintenance_per_turn = 0, prerequisite_tech = "Breech-Loading Rifles" },
  { name = "Conscript",      category = "Garrison",  era = 3,
    firepower = 10, firepower_mounted = 10, defense = 10, defense_terrain_bonus = 2,
    range = 2, movement = 4, arms_required = 0, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 100, maintenance_per_turn = 25, prerequisite_tech = "Modern Warfare" },

  -- ── Skirmisher infantry ─────────────────────────────────
  { name = "Skirmishers",    category = "Infantry",  era = 1,
    firepower =  5, firepower_mounted = 5,  defense =  7, defense_terrain_bonus = 1,
    range = 1, movement = 4, arms_required = 1, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 100, maintenance_per_turn = 25, prerequisite_tech = nil },
  { name = "Sharpshooters",  category = "Infantry",  era = 2,
    firepower = 10, firepower_mounted = 10, defense =  7, defense_terrain_bonus = 1,
    range = 3, movement = 4, arms_required = 2, requires_horse = false,
    fuel_required = 0, recruit_tier = "Trained",
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = "Sharpshooter Training" },
  { name = "Rangers",        category = "Infantry",  era = 3,
    firepower = 15, firepower_mounted = 15, defense = 20, defense_terrain_bonus = 5,
    range = 5, movement = 4, arms_required = 4, requires_horse = false,
    fuel_required = 0, recruit_tier = "Expert",
    cost = 250, maintenance_per_turn = 50, prerequisite_tech = "Ranger Training" },

  -- ── Line infantry ───────────────────────────────────────
  { name = "Regulars",       category = "Infantry",  era = 1,
    firepower = 10, firepower_mounted = 10, defense =  5, defense_terrain_bonus = 1,
    range = 1, movement = 4, arms_required = 1, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 100, maintenance_per_turn = 25, prerequisite_tech = nil },
  { name = "RifleInfantry",  category = "Infantry",  era = 2,
    firepower = 15, firepower_mounted = 15, defense =  7, defense_terrain_bonus = 1,
    range = 2, movement = 4, arms_required = 2, requires_horse = false,
    fuel_required = 0, recruit_tier = "Trained",
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = "Rifling" },
  { name = "Infantry",       category = "Infantry",  era = 3,
    firepower = 22, firepower_mounted = 22, defense = 20, defense_terrain_bonus = 5,
    range = 2, movement = 4, arms_required = 4, requires_horse = false,
    fuel_required = 0, recruit_tier = "Expert",
    cost = 300, maintenance_per_turn = 75, prerequisite_tech = "Modern Warfare" },

  -- ── Elite/heavy infantry ────────────────────────────────
  { name = "Grenadiers",     category = "Infantry",  era = 1,
    firepower = 12, firepower_mounted = 12, defense =  5, defense_terrain_bonus = 1,
    range = 1, movement = 4, arms_required = 1, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 150, maintenance_per_turn = 50, prerequisite_tech = "Grenadier Tactics" },
  { name = "Guards",         category = "Infantry",  era = 2,
    firepower = 17, firepower_mounted = 17, defense =  7, defense_terrain_bonus = 1,
    range = 2, movement = 4, arms_required = 2, requires_horse = false,
    fuel_required = 0, recruit_tier = "Trained",
    cost = 250, maintenance_per_turn = 75, prerequisite_tech = "Professional Army" },
  { name = "MachineGunners", category = "Infantry",  era = 3,
    firepower = 25, firepower_mounted = 25, defense = 20, defense_terrain_bonus = 5,
    range = 2, movement = 4, arms_required = 4, requires_horse = false,
    fuel_required = 0, recruit_tier = "Expert",
    cost = 400, maintenance_per_turn = 100, prerequisite_tech = "Machine Guns" },

  -- ── Light cavalry ───────────────────────────────────────
  { name = "Hussars",        category = "Cavalry",   era = 1,
    firepower =  7, firepower_mounted = 10, defense =  7, defense_terrain_bonus = 0,
    range = 1, movement = 7, arms_required = 1, requires_horse = true,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 150, maintenance_per_turn = 25, prerequisite_tech = nil },
  { name = "Scouts",         category = "Cavalry",   era = 2,
    firepower = 10, firepower_mounted = 13, defense =  7, defense_terrain_bonus = 0,
    range = 1, movement = 7, arms_required = 2, requires_horse = true,
    fuel_required = 0, recruit_tier = "Trained",
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = "Breech-Loading Rifles" },
  { name = "Carbineers",     category = "Cavalry",   era = 2,
    firepower = 20, firepower_mounted = 26, defense =  5, defense_terrain_bonus = 0,
    range = 2, movement = 7, arms_required = 2, requires_horse = true,
    fuel_required = 0, recruit_tier = "Trained",
    cost = 250, maintenance_per_turn = 50, prerequisite_tech = "Carbines" },
  { name = "Mechanised",     category = "Cavalry",   era = 3,
    firepower = 22, firepower_mounted = 28, defense = 10, defense_terrain_bonus = 2,
    range = 4, movement = 6, arms_required = 4, requires_horse = false,
    fuel_required = 1, recruit_tier = "Expert",
    cost = 400, maintenance_per_turn = 75, prerequisite_tech = "Mechanisation" },

  -- ── Heavy cavalry ───────────────────────────────────────
  { name = "Cuirassiers",    category = "Cavalry",   era = 1,
    firepower = 15, firepower_mounted = 19, defense =  5, defense_terrain_bonus = 0,
    range = 1, movement = 7, arms_required = 1, requires_horse = true,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = nil },
  { name = "Armour",         category = "Cavalry",   era = 3,
    firepower = 45, firepower_mounted = 60, defense = 20, defense_terrain_bonus = 5,
    range = 6, movement = 6, arms_required = 10, requires_horse = false,
    fuel_required = 1, recruit_tier = "Expert",
    cost = 500, maintenance_per_turn = 100, prerequisite_tech = "Armoured Vehicles" },

  -- ── Light artillery ─────────────────────────────────────
  { name = "LightArtillery", category = "Artillery", era = 1,
    firepower = 10, firepower_mounted = 3,  defense =  3, defense_terrain_bonus = 1,
    range = 3, movement = 3, arms_required = 2, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = nil },
  { name = "HorseArtillery", category = "Artillery", era = 2,
    firepower = 13, firepower_mounted = 4,  defense =  4, defense_terrain_bonus = 1,
    range = 4, movement = 5, arms_required = 2, requires_horse = true,
    fuel_required = 0, recruit_tier = "Trained",
    cost = 300, maintenance_per_turn = 75, prerequisite_tech = "Rifled Artillery" },
  { name = "FieldArtillery", category = "Artillery", era = 2,
    firepower = 17, firepower_mounted = 5,  defense =  3, defense_terrain_bonus = 1,
    range = 5, movement = 3, arms_required = 4, requires_horse = false,
    fuel_required = 0, recruit_tier = "Trained",
    cost = 350, maintenance_per_turn = 75, prerequisite_tech = "Field Artillery" },
  { name = "MobileArtillery",category = "Artillery", era = 3,
    firepower = 25, firepower_mounted = 8,  defense = 20, defense_terrain_bonus = 5,
    range = 5, movement = 4, arms_required = 6, requires_horse = false,
    fuel_required = 1, recruit_tier = "Expert",
    cost = 450, maintenance_per_turn = 100, prerequisite_tech = "Mobile Artillery" },

  -- ── Heavy artillery ─────────────────────────────────────
  { name = "Artillery",      category = "Artillery", era = 1,
    firepower = 16, firepower_mounted = 4,  defense =  2, defense_terrain_bonus = 1,
    range = 4, movement = 2, arms_required = 2, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 300, maintenance_per_turn = 75, prerequisite_tech = "Improved Artillery" },
  { name = "SiegeArtillery", category = "Artillery", era = 2,
    firepower = 30, firepower_mounted = 8,  defense =  3, defense_terrain_bonus = 1,
    range = 6, movement = 2, arms_required = 4, requires_horse = false,
    fuel_required = 0, recruit_tier = "Trained",
    cost = 500, maintenance_per_turn = 100, prerequisite_tech = "Siege Warfare" },
  { name = "RailroadGuns",   category = "Artillery", era = 3,
    firepower = 50, firepower_mounted = 12, defense = 20, defense_terrain_bonus = 5,
    range = 17, movement = 0, arms_required = 8, requires_horse = false,
    fuel_required = 0, recruit_tier = "Expert",
    cost = 600, maintenance_per_turn = 125, prerequisite_tech = "Railroad Artillery" },

  -- ── Engineer ────────────────────────────────────────────
  { name = "Sapper",         category = "Special",   era = 1,
    firepower =  0, firepower_mounted = 0,  defense =  3, defense_terrain_bonus = 1,
    range = 1, movement = 4, arms_required = 2, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 150, maintenance_per_turn = 25, prerequisite_tech = "Engineering" },
  { name = "CombatEngineer", category = "Special",   era = 2,
    firepower =  0, firepower_mounted = 0,  defense =  4, defense_terrain_bonus = 1,
    range = 2, movement = 4, arms_required = 2, requires_horse = false,
    fuel_required = 0, recruit_tier = "Trained",
    cost = 200, maintenance_per_turn = 50, prerequisite_tech = "Engineering" },
  { name = "Commandos",      category = "Special",   era = 3,
    firepower = 15, firepower_mounted = 15, defense = 15, defense_terrain_bonus = 3,
    range = 2, movement = 6, arms_required = 3, requires_horse = false,
    fuel_required = 0, recruit_tier = "Expert",
    cost = 400, maintenance_per_turn = 100, prerequisite_tech = "Modern Warfare" },
  { name = "Saboteur",       category = "Special",   era = 3,
    firepower =  0, firepower_mounted = 0,  defense = 10, defense_terrain_bonus = 2,
    range = 1, movement = 4, arms_required = 3, requires_horse = false,
    fuel_required = 0, recruit_tier = "Expert",
    cost = 250, maintenance_per_turn = 50, prerequisite_tech = "Modern Warfare" },

  -- ── Special ─────────────────────────────────────────────
  { name = "General",        category = "Special",   era = 1,
    firepower =  0, firepower_mounted = 0,  defense =  0, defense_terrain_bonus = 0,
    range = 0, movement = 8, arms_required = 0, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 0, maintenance_per_turn = 0, prerequisite_tech = nil },

  -- ── Project-specific: minor-nation capital defense ──────
  -- Not in the original manual roster. Auto-spawned at minor-nation capitals;
  -- never recruited by the player.
  { name = "GarrisonArtillery", category = "Garrison", era = 1,
    firepower =  4, firepower_mounted = 0,  defense =  0, defense_terrain_bonus = 0,
    range = 3, movement = 0, arms_required = 0, requires_horse = false,
    fuel_required = 0, recruit_tier = "Untrained",
    cost = 0, maintenance_per_turn = 0, prerequisite_tech = nil },
}
