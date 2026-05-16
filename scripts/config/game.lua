-- Game Configuration
--
-- Global game-rule constants. These define fundamental mechanics, NOT personality
-- preferences. All AI personalities and human players use the same values.
--
-- To mod the game, change values here. For AI behavior tuning, see scripts/ai/*.lua.

game_config = {
    -- Free-form debug string. Surfaced to the browser via
    -- `wasm.wasm_debug_marker()`. Edit this, rebuild WASM, and call the JS
    -- function from the console to verify the Lua → WASM pipeline is live.
    debug_marker = "lua-marker-2026-05-08-A",

    -- Labor output per worker type
    untrained_labor = 1,
    trained_labor = 2,
    expert_labor = 4,

    -- Labor cost per unit of mill/factory output
    labor_per_production = 2,

    -- Building a civilian unit permanently removes one expert worker
    civilian_costs_expert = true,

    -- Gate AI civilian hires and army recruitment on workforce capacity.
    -- Required labor = Σ over { LumberMill, FurnitureFactory, SteelMill,
    -- HardwareFactory, TextileMill, ClothingFactory } of
    --   effective_capacity(b) * labor_per_production.
    -- Total labor from current workers must be ≥ this ratio × required labor
    -- before the AI is allowed to spawn a civilian or recruit an army unit.
    chain_labor_gate_ratio = 0.66,
    armory_steel_per_arm  = 1,   -- Steel consumed per Arm produced at the Armory
    armory_labor_per_arm  = 2,   -- Labor units required per Arm produced

    -- Worker training costs (paper material + labor per worker trained)
    train_to_trained_paper_cost = 1,  -- paper per Untrained->Trained promotion
    train_to_trained_labor_cost = 4,  -- labor per Untrained->Trained promotion
    train_to_expert_paper_cost  = 2,  -- paper per Trained->Expert promotion
    train_to_expert_labor_cost  = 8,  -- labor per Trained->Expert promotion

    -- Production ratios (resource -> material -> good)
    resources_per_material = 2,    -- 2 timber -> 1 lumber, 2 cotton/wool -> 1 fabric
    materials_per_good = 2,        -- 2 lumber -> 1 furniture, 2 steel -> 1 hardware
    coal_iron_ratio = 1,           -- 1 coal + 1 iron -> 1 steel (special case)

    -- Food
    food_per_worker = 1,           -- each worker eats 1 food per turn
    starvation_cap = 2,            -- max workers lost to starvation per turn
    canned_food_ratio = 2,         -- 2 raw food -> 1 canned food

    -- Immigration requirements (per immigrant)
    immigration_canned_food = 1,
    immigration_clothing = 1,
    immigration_furniture = 1,
    provinces_per_immigrant = 4,           -- 1 immigrant per N provinces
    provinces_per_immigrant_upgraded = 3,  -- with upgraded Capitol

    -- Monetary conversion
    gold_value = 500,              -- cash per unit of gold transported
    gems_value = 1000,             -- cash per unit of gems transported

    -- Buildings
    expansion_delay_turns = 2,     -- turns for building expansion to complete
    use_tier_expansion = true,     -- use capacity tier progression (2->4->8->12->...)

    -- Starting conditions
    starting_freight_cars = 15,    -- freight cars each Great Power starts with
    starting_engineers = 1,        -- Engineer civilians each Great Power starts with
    freight_car_cost = 200,        -- $ to build one freight car (BuildFreightCar command)
    -- Per-turn army maintenance, in cents per arms_required slot. $2.50/arm
    -- (1/10 of the original $25/arm) per card #216. Garrison units (militia
    -- and garrison artillery) are exempt regardless of this value.
    army_maintenance_cents_per_arm = 250,
    starting_prospectors = 1,      -- Prospector civilians each Great Power starts with
    starting_miners = 1,           -- Miner civilians each Great Power starts with

    -- Tech prerequisites for hiring civilians. nil = available from turn 1.
    -- Per the original Imperialism manual (p.27–28).
    civilian_rancher_tech = "Feed Grasses",
    civilian_forester_tech = "Iron Railroad Bridge",
    civilian_driller_tech = "Oil Drilling",

    -- AI prospector hire pacing: target one Prospector per N undiscovered
    -- deposit-eligible hexes the nation owns. 0 disables hiring.
    ai_prospector_per_hexes = 10,

    -- Civilian hire costs ($)
    engineer_cost = 500,
    prospector_cost = 100,
    miner_cost = 1500,
    farmer_cost = 100,
    rancher_cost = 100,
    forester_cost = 100,
    driller_cost = 2000,

    -- Infrastructure build costs ($)
    depot_cost = 2000,
    port_cost = 3000,
    railroad_cost_grassland = 100,
    railroad_cost_forest = 100,
    railroad_cost_desert = 150,
    railroad_cost_tundra = 150,
    railroad_cost_swamp = 300,
    railroad_cost_hills = 200,
    railroad_cost_mountain = 500,
    fort_cost_level_1 = 5000,
    fort_cost_level_2 = 7500,
    fort_cost_level_3 = 10000,

    -- Engineer build-task durations (turns to complete)
    build_turns_railroad = 1,
    build_turns_depot = 2,
    build_turns_port = 3,

    -- Tech prerequisites for laying railroad on each land terrain.
    -- nil = always buildable. Reference: tech/tree.rs.
    railroad_tech_grassland = nil,
    railroad_tech_forest = nil,
    railroad_tech_desert = nil,
    railroad_tech_tundra = nil,
    railroad_tech_hills = nil,
    railroad_tech_swamp = "Iron Railroad Bridge",
    railroad_tech_mountain = "Compound Steam Engine",

    -- Number of turns over which the AI amortises a depot's yield when
    -- comparing candidate placements vs. the railroad + depot build cost.
    -- Depots are long-lived infrastructure; a 50-turn window reflects that a
    -- nation should always be investing in development when an opportunity
    -- exists, not hoarding cash.
    infrastructure_horizon_turns = 50,

    -- Depot-planner scoring weights (card #132).
    --   net_score = coverage * horizon * infra_coverage_weight
    --             - path_cost * infra_path_cost_weight - depot_cost
    -- Raise infra_path_cost_weight above 1.0 to prefer short routes over
    -- high-coverage remote candidates.
    infra_coverage_weight = 1.0,
    infra_path_cost_weight = 1.0,
    -- Card #217: weight on a tile's "eventual" yield (improvable tiers above
    -- current improvement_level, capped by tech) when scoring depot coverage.
    -- 0 = current yield only (legacy). 1.0 = 1 demand-weighted point per
    -- unimproved tier. Lets the depot planner prefer tiles that *will* produce
    -- a lot once worked, not just tiles that produce a lot today.
    infra_improvability_weight = 0.5,
    -- Card #217: early-game bias toward laying rail before hiring more
    -- improvers. For the first N turns, multiply score_infrastructure by
    -- early_game_bias. Connecting an L0 tile produces yield immediately;
    -- improving a disconnected tile produces nothing until rail catches up.
    infra_early_game_bias_turns = 5,
    infra_early_game_bias = 1.5,

    -- Trade-aware demand (cards #131 / #132).
    --   trade_lookback_turns             — window for "recently imported"
    --   trade_discount_weight            — master switch (0 disables)
    --   trade_history_weight             — weight on avg-per-turn imports
    --                                      from trade_history (rate = sum /
    --                                      lookback_turns so a one-off buy
    --                                      N turns ago counts as 1/N this turn)
    --   trade_consulate_potential_weight — weight on tradeable tile yield
    --                                      owned by consulated minors (lower
    --                                      than history because potential is
    --                                      softer signal than actual imports)
    trade_lookback_turns = 8,
    trade_discount_weight = 0.5,
    trade_history_weight = 1.0,
    trade_consulate_potential_weight = 0.25,

    -- Engineer-hire scoring (drives `score_hire_engineer`).
    -- Score = base + path_len × path_coeff, capped, returns None at hire_max.
    engineer_hire_max = 3,
    engineer_hire_base = 100,
    engineer_hire_path_coeff = 30,
    engineer_hire_cap = 250,

    -- Improver-civilian hire scoring (drives `score_civilian`).
    -- Replaces the old fixed 4-step coverage ladder with a continuous
    -- saturation formula that scales with empire size:
    --   each existing improver "covers" ~target_tiles_per_worker improvable
    --   tiles; every unmet tile beyond that capacity adds
    --   coverage_per_unmet to the score.
    --   hire_bootstrap is added when civilian_count = 0 and there is at
    --   least one improvable tile (gets the first civilian hired).
    --   idle_penalty is subtracted per idle improver already on the roster.
    -- Each civilian "covers" this many improvable tiles' worth of work; the
    -- saturation picker stops adding more of a type once
    -- `workers >= ceil(demand / coverage)`. Lower = more civilians (lots of
    -- 1-tile-each Farmers); higher = fewer, harder-working civilians. The
    -- original game has ~1–3 of each type per nation, so 8 is a closer match.
    civilian_target_tiles_per_worker = 3,
    -- Card #217 follow-up: replaces the single `civilian_coverage_per_unmet`
    -- with per-tile weights that depend on the tile's connectivity. An
    -- improvable tile only produces yield once collected, so a depot-
    -- connected tile pulls a stronger "we should hire someone for this"
    -- signal than an isolated tile that won't yield until rail catches up.
    --   collectable      visible-improvable tile inside a connected depot's
    --                    1-hex collection radius (or in the capital province).
    --   rail_adjacent    visible-improvable tile next to existing owned
    --                    rail/depot — easy to extend to it next.
    --   unconnected      visible-improvable tile not in either set; speculative.
    --   undiscovered     un-prospected deposit-eligible hex (Prospector pull).
    -- Saturation cap stays the same: each existing improver "covers"
    -- target_tiles_per_worker tiles' worth of weighted demand.
    civilian_coverage_collectable = 3.0,
    civilian_coverage_rail_adjacent = 1.5,
    civilian_coverage_unconnected = 0.5,
    civilian_coverage_undiscovered = 1.5,
    civilian_hire_bootstrap = 15.0,
    civilian_idle_penalty = 8.0,

    -- Card #217: improver-deployment connectivity buckets. When picking which
    -- tile an idle improver should work next, the AI prefers (lower score
    -- wins, ties broken by lowest current improvement_level):
    --   0  collectable now (already in the rail/depot harvest set)
    --   planned_weight  on the AI's current depot plan (path or 1-hex radius
    --                   around the planned candidate) — will be collectable
    --                   within a few turns
    --   adjacent_weight adjacent to existing rail/depot — easily reached by
    --                   a small rail extension later
    --   unconnected_weight not in any plan and far from rail — speculative
    --
    -- Cash-rich softening: when the AI's treasury surplus over its spending
    -- reserve exceeds `softening_treasury_threshold` dollars, the unconnected
    -- bucket weights are scaled by `1 / (1 + surplus / threshold)`, so a rich
    -- AI doesn't sit on idle improvers when only disconnected tiles remain.
    civilian_connectivity_planned_weight = 30.0,
    civilian_connectivity_adjacent_weight = 60.0,
    civilian_connectivity_unconnected_weight = 100.0,
    civilian_connectivity_softening_threshold = 20000,

    -- Backlog scoring weights — points added per turn the spending category
    -- has been neglected. Each personality has a different sensitivity per
    -- category, encoding what they grow "impatient" about.
    --     Aggressive  → impatient about military, slow on diplomacy
    --     Economic    → impatient about infrastructure
    --     Diplomatic  → impatient about diplomacy
    --     Balanced    → mid on everything
    -- These are added to the raw category score before sorting, so neglected
    -- categories naturally climb the priority ladder until they fire.
    backlog_weight_aggressive_military = 50,
    backlog_weight_aggressive_infra = 25,
    backlog_weight_aggressive_diplomacy = 5,
    backlog_weight_aggressive_hire_engineer = 20,
    backlog_weight_aggressive_hire_improver = 15,
    backlog_weight_balanced_military = 30,
    backlog_weight_balanced_infra = 30,
    backlog_weight_balanced_diplomacy = 20,
    backlog_weight_balanced_hire_engineer = 25,
    backlog_weight_balanced_hire_improver = 20,
    backlog_weight_economic_military = 15,
    backlog_weight_economic_infra = 50,
    backlog_weight_economic_diplomacy = 20,
    backlog_weight_economic_hire_engineer = 30,
    backlog_weight_economic_hire_improver = 25,
    backlog_weight_diplomatic_military = 10,
    backlog_weight_diplomatic_infra = 35,
    backlog_weight_diplomatic_diplomacy = 40,
    backlog_weight_diplomatic_hire_engineer = 25,
    backlog_weight_diplomatic_hire_improver = 20,
    -- Cap on day-1 backlog: prevents starting-state backlog (no category yet
    -- invested in) from completely dominating the first ~20 turns.
    backlog_initial_cap = 20,

    -- Priority diplomacy targets: at game start every Great Power picks N
    -- minor nations whose visible exports best fill its resource deficits.
    -- Until consulate+embassy are established with all priority targets, the
    -- diplomacy score is huge for those targets (and zero for non-priority
    -- minors). Once secured, diplomacy spending drops to near zero.
    -- N depends on personality.
    priority_minor_target_score = 1000.0,
    priority_minor_targets_aggressive = 3,
    priority_minor_targets_balanced = 4,
    priority_minor_targets_economic = 4,
    priority_minor_targets_diplomatic = 5,

    -- Diplomacy costs
    consulate_cost = 500,
    embassy_cost = 5000,
    -- Minimum relationship score (-100..=100) before AI upgrades a consulate
    -- to an embassy. Consulates already give a relationship bonus, so the
    -- expensive embassy should wait for real warmth. Priority-minor targets
    -- bypass this gate.
    ai_embassy_min_relation = 25,

    -- Diplomatic relationship tuning
    -- Minor nations voluntarily join a Great Power's empire when their relation
    -- score reaches this value. Range is [-100, 100], so 95 = near-max trust.
    voluntary_incorporation_threshold = 90,
    -- Per-turn cap on relationship improvement from trade with a consulate.
    -- The raw improvement is the number of distinct resources traded;
    -- capping prevents broad trade portfolios from trivially maxing relations.
    trade_relation_improvement_cap = 15,
    -- Relationship improvement per distinct resource traded (per interval).
    -- Total improvement = resources * per_resource, capped at improvement_cap.
    trade_relation_improvement_per_resource = 2,
    -- Only apply the trade relationship improvement once every N turns.
    -- Combined with the cap above, this controls how fast a GP can befriend
    -- a minor nation through passive trade alone. Set to 1 for every turn.
    trade_relation_turn_interval = 3,

    -- Minor nation trade behaviour
    -- Chance (0–100) that a minor nation withholds one random resource offer each turn.
    -- This adds variety: a minor is not always a perfectly predictable supplier.
    minor_resource_withhold_chance = 20,
    -- Price ($/unit) minor nations pay when purchasing manufactured goods each turn.
    minor_goods_buy_price = 150,
    -- Chance (0–100) that an individual minor nation declines to buy a given
    -- manufactured-goods offer from a GP this turn. Minors are tried in order
    -- of descending relationship with the seller; if every minor skips, the
    -- offer goes unfilled and the surplus stays in the GP's stockpile.
    minor_goods_skip_chance = 20,

    -- AI trade behaviour
    ai_consulate_target = 4,                  -- AI GPs aim for this many consulates in minor nations
    ai_consulate_priority_score = 30.0,        -- scoring weight per missing consulate below target
    ai_consulate_beyond_target_score = 3.0,    -- base scoring weight per available MN beyond target
    ai_consulate_beyond_target_decay = 4.0,    -- penalty per extra consulate above target

    -- Map generation
    min_food_tile_percent = 20,  -- at least 20% of land tiles must produce food
    food_cluster_chance = 40,    -- % chance food terrain spreads to adjacent tile

    -- Garrison militia (per-province local defence — manual page 36)
    -- Default size of a Great Power province's garrison; fresh provinces and
    -- provinces that lose militia regenerate up to this cap.
    default_garrison_per_province = 4,
    -- Default garrison size for minor-nation provinces.
    minor_default_garrison = 3,
    -- Hard upper bound after retreats pile overflow into neighbors.
    max_garrison_per_province = 8,
    -- Turn cadence at which each under-strength province spawns +1 militia.
    garrison_regen_interval_turns = 2,
    -- HP recovered per turn by a unit that neither moved nor fought (card #20).
    -- 35/turn brings a 0-HP unit back to full in ~3 turns of rest.
    rest_heal_amount = 35,

    -- AI naval scoring coefficients (card #112).
    -- spending_naval_base: peacetime score floor (even without war, navy drips in).
    -- spending_naval_war_bonus: bonus when AI is at war with any nation.
    -- spending_naval_gap_coeff: points per unit of firepower gap vs strongest enemy fleet.
    spending_naval_base = 2.0,
    spending_naval_war_bonus = 10.0,
    spending_naval_gap_coeff = 1.5,

    -- D-4: Pact-defense evaluation (evaluate_pact_defense).
    -- A nation's global "standing" must exceed this gate before pact defense is considered.
    pact_defense_standing_gate = 30,
    -- Weight of the minor-nation relationship score in the combined factor.
    pact_defense_relationship_weight = 0.4,
    -- Weight of the hypothetical-war win-likelihood in the combined factor.
    pact_defense_military_weight = 0.4,
    -- Per-personality additive bias to the combined factor.
    pact_defense_bias_aggressive = 0.2,
    pact_defense_bias_diplomatic = 0.1,
    pact_defense_bias_balanced = 0.0,
    pact_defense_bias_economic = -0.15,
    -- Per-personality minimum combined threshold to trigger pact defense.
    pact_defense_threshold_aggressive = 0.2,
    pact_defense_threshold_diplomatic = 0.3,
    pact_defense_threshold_balanced = 0.35,
    pact_defense_threshold_economic = 0.5,

    -- D-5: Terrain and fort defense bonuses (fraction added to defender FP).
    --
    -- Card #478 follow-up: terrain bonuses are zeroed out and the per-unit
    -- `defense` stat multiplier was dropped from the resolver. The only
    -- defender multiplier left is the fort, which scales linearly to a
    -- max of +75% at L3 (so a forted province roughly negates an attacker's
    -- numeric edge, but no defensive setup is multiplicatively decisive).
    terrain_defense_mountain = 0.0,
    terrain_defense_hills = 0.0,
    terrain_defense_forest = 0.0,
    terrain_defense_swamp = 0.0,
    fort_defense_level1 = 0.25,
    fort_defense_level2 = 0.50,
    fort_defense_level3 = 0.75,
    -- Flat raw FP added per defending Garrison unit (Minutemen / Militia /
    -- Conscript / GarrisonArtillery) that has been at the province for at
    -- least one turn (`arrived_turn < current_turn`). Intent: established
    -- garrisons "dig in" and gain a small entrenchment kicker that fresh
    -- arrivals don't get yet. Was 8 (Minutemen-only) before card #478.
    garrison_entrenchment_fp = 3.0,
    -- Fraction of starting FP lost that triggers a mid-battle retreat for each side.
    battle_attacker_fp_loss_ratio = 0.60,
    battle_defender_fp_loss_ratio = 2.0,

    -- ── Role-aware combat (Trello card #478) ─────────────────────────
    -- These knobs feed both the resolver and the AI strength estimator
    -- so the AI's pre-attack guess matches what the resolver actually does.
    --
    -- combat_first_strike_enabled
    --   When the longer-ranged side's max range exceeds the shorter-ranged
    --   side's max range, that side fires one *free* volley before round 1
    --   from only its over-range units. Captures the "artillery shoots
    --   first" feel the original game implied via FPN/FPM split.
    --   Alternatives we considered:
    --     (b) free volley at 50% damage  — too forgiving for siege/RR guns
    --     (c) no free volley but +30% bombardment dmg every round for the
    --         longer-ranged side — closer to a hit-and-run feel, but
    --         bleeds the headline "artillery gets to shoot first" effect.
    --   We picked (a). One free volley per battle, capped to over-range
    --   units (range > opponent_max_range), full damage.
    combat_first_strike_enabled = true,
    combat_first_strike_damage_multiplier = 1.0,

    -- combat_cavalry_charge_bonus
    --   Multiplies attacking-cavalry firepower in round 1 only.
    --   Spec calls for ×1.25 vs non-cavalry targets, but the resolver pools
    --   damage at the side level so we can't slice "vs non-cavalry only"
    --   without per-target accounting. We apply the bonus unconditionally
    --   in round 1 (one-shot charge) and document the per-target variant.
    --   Alternative: every round it stays in melee — historically wrong,
    --   a charge is a one-shot.
    combat_cavalry_charge_bonus = 0.25,

    -- "Screen your guns" emerges from the per-shot 1v1 targeting model:
    -- front-line shooters target enemy front-line first and only fall through
    -- to artillery once the screen is gone, so a stack with infantry up
    -- front naturally protects its artillery without an explicit penalty.

    -- combat_ai_strength_*  — AI estimator only, never used by the resolver.
    --   The estimator computes per-unit "effective strength" as
    --     s(u) = fp_phase(u) * sqrt(def_eff(u)) * range_factor(u) * health(u)
    --   so that under Lanchester square-law dynamics a force's expected
    --   contribution is linearly summable.
    --   sqrt(defense): doubling defense ≈ √2× the strength contribution.
    --   Alternative we considered: linear `def_eff(u)` (more intuitive when
    --   reading numbers — "20 def is 5× as tough as 4 def" — but inflates
    --   the apparent gap between Era-3 elite and Era-1 garrisons more than
    --   battle outcomes warrant). We picked sqrt; flip the flag below to
    --   compare empirically.
    combat_ai_strength_lanchester = true,
    --   range_advantage = 1 + coeff * max(0, my_range - enemy_max_range)
    --   capped at +cap. Lets the AI value a 6-range Armour stack over a
    --   1-range Conscript stack of equivalent FP.
    combat_ai_strength_range_advantage_coeff = 0.10,
    combat_ai_strength_range_advantage_cap = 0.50,

    -- D-6: AI worker/civilian hiring thresholds.
    -- Workers recruited per province for normal vs. wealthy nations.
    labor_workers_per_province_base = 2,
    labor_workers_per_province_wealthy = 3,
    -- Treasury threshold (dollars) above which a nation is considered "wealthy" for labor.
    labor_wealthy_treasury_threshold = 20000,
    -- Minimum total worker target regardless of province count.
    labor_min_workers_floor = 5,
    -- Treasury thresholds and civilian-count caps for each hiring tier.
    labor_hire_civilian_tier1_treasury = 1000,
    labor_hire_civilian_tier1_max = 2,
    labor_hire_civilian_tier2_treasury = 2000,
    labor_hire_civilian_tier2_max = 4,

    -- D-7: Minimum treasury required before the AI will build consulates.
    ai_consulate_treasury_threshold = 2000,
}

return game_config
