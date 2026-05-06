//! Economy phase of the turn pipeline.
//!
//! Implements the plan → reserve → execute structure (Trello #161).
//! `processor.rs` calls `collect_economic_orders` → `validate_and_reserve` →
//! `execute_reserved_economy` three times (collect, production, trade) interleaved
//! with non-economy steps, then calls `release_all_reservations` once at the end
//! of all three batches to honour the documented reservation lifetime contract.
//!
//! Lighter helpers (building tick, blockade capacity, maintenance, blockade
//! headlines) live here alongside the phase types.
//! The heavy execution functions (`collect_resources`, `run_production`)
//! remain in `processor.rs`; trade resolution now lives in `trade_phase.rs`.

use crate::economy::buildings::BuildingType;
use crate::economy::labor::WorkerType;
use crate::economy::observability::{PendingEconomyOrder, PendingOrderPhase};
use crate::economy::production::{
    ProductionChain, calculate_armory_production, calculate_canned_food_production,
    calculate_factory_production, calculate_mill_production, calculate_paper_production,
};
use crate::economy::trade::{self, Commodity};
use crate::events::{Headline, HeadlineCategory};
use crate::game_state::GameState;
use crate::military::naval::calculate_blockade_effect;
use crate::turn::processor::TurnReport;
use crate::types::*;
use std::collections::{BTreeMap, HashMap};

// ── Plan / Reserve / Execute types (#161) ────────────────────────────────────

/// An economic action gathered during the collect phase.
/// No game state is mutated during collection — this represents *intent*.
#[derive(Debug, Clone)]
pub struct EconomicOrder {
    pub kind: EconomicOrderKind,
}

/// The kind of economy-phase work each order represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EconomicOrderKind {
    /// Collect raw resources from owned, connected tiles.
    CollectTileResources,
    /// Run production chains (mills → materials, factories → goods).
    RunProduction,
    /// Execute the trade session with minor nations and the world market.
    ExecuteTrade,
}

#[derive(Debug, Clone)]
pub struct NationReservation {
    pub nation_id: NationId,
    pub inventory_reservations: Vec<ReservationId>,
    pub inventory_summary: BTreeMap<Commodity, u32>,
    pub treasury_reserved: Money,
    pub labor_reserved: HashMap<WorkerType, u32>,
}

impl NationReservation {
    fn commitment_count(&self) -> usize {
        let _ = self.nation_id;
        self.inventory_reservations.len()
            + self.inventory_summary.len()
            + self.labor_reserved.len()
            + usize::from(self.treasury_reserved > Money::ZERO)
    }
}

/// An economic order that has been validated and whose resource requirements
/// have been pre-committed.
#[derive(Debug, Clone)]
pub struct ReservedAction {
    pub order: EconomicOrder,
    pub reservations: Vec<NationReservation>,
}

impl ReservedAction {
    fn commitment_count(&self) -> usize {
        self.reservations
            .iter()
            .map(NationReservation::commitment_count)
            .sum()
    }
}

/// Gather the standard economy orders for this turn.
///
/// **No mutation.** Reads game state to decide which standard operations
/// should run; the actual work happens in the matching `run_*_phase` entry points.
pub(super) fn collect_economic_orders(_game: &GameState) -> Vec<EconomicOrder> {
    vec![
        EconomicOrder {
            kind: EconomicOrderKind::CollectTileResources,
        },
        EconomicOrder {
            kind: EconomicOrderKind::RunProduction,
        },
        EconomicOrder {
            kind: EconomicOrderKind::ExecuteTrade,
        },
    ]
}

/// Validate orders and register the reservations needed to execute them.
///
/// Returns reserved actions ready for execution in the current economy batch.
/// Reservations and pending-order summaries are batch-local and must be
/// cleared after the matching batch executes so later batches observe fresh
/// `available()` state.
pub(super) fn validate_and_reserve(
    game: &mut GameState,
    orders: Vec<EconomicOrder>,
) -> Vec<ReservedAction> {
    orders
        .into_iter()
        .map(|order| {
            let reservations = match order.kind {
                EconomicOrderKind::CollectTileResources => Vec::new(),
                EconomicOrderKind::RunProduction => reserve_production_phase(game),
                EconomicOrderKind::ExecuteTrade => reserve_trade_phase(game),
            };
            ReservedAction {
                order,
                reservations,
            }
        })
        .collect()
}

fn reserve_production_phase(game: &mut GameState) -> Vec<NationReservation> {
    let untrained_mult = game.game_data.game_config.untrained_labor;
    let trained_mult = game.game_data.game_config.trained_labor;
    let expert_mult = game.game_data.game_config.expert_labor;
    let armory_steel_per_arm = game.game_data.game_config.armory_steel_per_arm;
    let armory_labor_per_arm = game.game_data.game_config.armory_labor_per_arm;
    let nation_ids: Vec<NationId> = game.world.nations.iter().map(|n| n.id).collect();
    let mut out = Vec::new();

    for nation_id in nation_ids {
        let Some(nation) = game.get_nation(nation_id) else {
            continue;
        };
        if nation.diplomacy.is_in_anarchy {
            continue;
        }

        let resources: Vec<(ResourceType, u32)> = nation
            .economy
            .warehouse
            .iter()
            .map(|(r, q)| (*r, *q))
            .collect();
        let starting_materials: HashMap<MaterialType, u32> = nation
            .economy
            .materials
            .iter()
            .map(|(m, q)| (*m, *q))
            .collect();

        // DEBUG (do not revert until requested): the LaborPool::total_labor_units_with
        // method returns a fixed 200; this call site is unchanged for clarity.
        let total_labor =
            nation
                .economy
                .labor
                .total_labor_units_with(untrained_mult, trained_mult, expert_mult);

        let timber_cap = building_capacity(nation, BuildingType::LumberMill);
        let steel_cap = building_capacity(nation, BuildingType::SteelMill);
        let textile_cap = building_capacity(nation, BuildingType::TextileMill);
        let furniture_cap = building_capacity(nation, BuildingType::FurnitureFactory);
        let hardware_cap = building_capacity(nation, BuildingType::HardwareFactory);
        let clothing_cap = building_capacity(nation, BuildingType::ClothingFactory);
        let armory_cap = building_capacity(nation, BuildingType::Armory);
        let paper_cap = building_capacity(nation, BuildingType::PaperFactory);
        let canned_food_cap = building_capacity(nation, BuildingType::FoodProcessing);

        let targets = nation.economy.chain_targets.clone();

        // Distribute labor proportionally across active steps using labor weights.
        let labor_budgets = allocate_labor(
            total_labor,
            &targets,
            BuildingCapacities {
                timber: timber_cap,
                metal: steel_cap,
                textile: textile_cap,
                furniture: furniture_cap,
                hardware: hardware_cap,
                clothing: clothing_cap,
                armory: armory_cap,
                paper: paper_cap,
                canned_food: canned_food_cap,
            },
        );

        // Apply feed% to resources before passing to mill production functions.
        let fed_resources = apply_feed_to_resources(&resources, &targets);
        let mut resource_needs: BTreeMap<ResourceType, u32> = BTreeMap::new();

        let timber_result = if timber_cap > 0 {
            let result = calculate_mill_production(
                ProductionChain::Timber,
                &fed_resources,
                timber_cap,
                labor_budgets.timber_mill,
            );
            for (resource, qty) in &result.resources_consumed {
                *resource_needs.entry(*resource).or_insert(0) += *qty;
            }
            Some(result)
        } else {
            None
        };

        let metal_result = if steel_cap > 0 {
            let result = calculate_mill_production(
                ProductionChain::Metal,
                &fed_resources,
                steel_cap,
                labor_budgets.metal_mill,
            );
            for (resource, qty) in &result.resources_consumed {
                *resource_needs.entry(*resource).or_insert(0) += *qty;
            }
            Some(result)
        } else {
            None
        };

        let textile_result = if textile_cap > 0 {
            let result = calculate_mill_production(
                ProductionChain::Textile,
                &fed_resources,
                textile_cap,
                labor_budgets.textile_mill,
            );
            for (resource, qty) in &result.resources_consumed {
                *resource_needs.entry(*resource).or_insert(0) += *qty;
            }
            Some(result)
        } else {
            None
        };

        // Combine warehouse materials + this turn's mill output for factory inputs.
        let mut materials_inventory = starting_materials.clone();
        for result in [&timber_result, &metal_result, &textile_result]
            .into_iter()
            .flatten()
        {
            for (material, qty) in &result.materials_produced {
                *materials_inventory.entry(*material).or_insert(0) += *qty;
            }
        }

        // Apply feed% to materials (warehouse + mill output combined).
        let fed_materials = apply_feed_to_materials(
            &materials_inventory
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect::<Vec<_>>(),
            &targets,
        );

        let furniture_result = if furniture_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Timber,
                &fed_materials,
                furniture_cap,
                labor_budgets.lumber_factory,
            ))
        } else {
            None
        };
        let hardware_result = if hardware_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Metal,
                &fed_materials,
                hardware_cap,
                labor_budgets.steel_factory,
            ))
        } else {
            None
        };
        let clothing_result = if clothing_cap > 0 {
            Some(calculate_factory_production(
                ProductionChain::Textile,
                &fed_materials,
                clothing_cap,
                labor_budgets.garment_factory,
            ))
        } else {
            None
        };

        let steel_consumed_by_factory = hardware_result
            .as_ref()
            .and_then(|r| {
                r.materials_consumed
                    .iter()
                    .find(|(m, _)| *m == MaterialType::Steel)
            })
            .map(|(_, qty)| *qty)
            .unwrap_or(0);
        let armory_result = if armory_cap > 0 {
            let steel_for_armory = materials_inventory
                .get(&MaterialType::Steel)
                .copied()
                .unwrap_or(0)
                .saturating_sub(steel_consumed_by_factory)
                .min(targets.armory);
            Some(calculate_armory_production(
                steel_for_armory,
                armory_cap,
                labor_budgets.armory,
                armory_steel_per_arm,
                armory_labor_per_arm,
            ))
        } else {
            None
        };

        let lumber_consumed_by_furniture = furniture_result
            .as_ref()
            .and_then(|r| {
                r.materials_consumed
                    .iter()
                    .find(|(m, _)| *m == MaterialType::Lumber)
            })
            .map(|(_, qty)| *qty)
            .unwrap_or(0);
        let paper_result = if paper_cap > 0 {
            let lumber_for_paper = materials_inventory
                .get(&MaterialType::Lumber)
                .copied()
                .unwrap_or(0)
                .saturating_sub(lumber_consumed_by_furniture);
            let fed: Vec<(MaterialType, u32)> = vec![(MaterialType::Lumber, lumber_for_paper)];
            Some(calculate_paper_production(
                &fed,
                paper_cap,
                labor_budgets.paper_factory,
            ))
        } else {
            None
        };

        let canned_food_result = if canned_food_cap > 0 {
            let result = calculate_canned_food_production(
                &fed_resources,
                canned_food_cap,
                labor_budgets.canned_food_factory,
            );
            for (resource, qty) in &result.resources_consumed {
                *resource_needs.entry(*resource).or_insert(0) += *qty;
            }
            Some(result)
        } else {
            None
        };

        let mut material_needs: BTreeMap<MaterialType, u32> = BTreeMap::new();
        for result in [
            &furniture_result,
            &hardware_result,
            &clothing_result,
            &armory_result,
            &paper_result,
        ]
        .into_iter()
        .flatten()
        {
            for (material, qty) in &result.materials_consumed {
                let starting_stock = starting_materials.get(material).copied().unwrap_or(0);
                let already_reserved = material_needs.get(material).copied().unwrap_or(0);
                let reservable = starting_stock.saturating_sub(already_reserved).min(*qty);
                if reservable > 0 {
                    *material_needs.entry(*material).or_insert(0) += reservable;
                }
            }
        }

        let labor_used: u32 = [&timber_result, &metal_result, &textile_result]
            .into_iter()
            .flatten()
            .map(|r| r.labor_used)
            .sum::<u32>()
            + [
                &furniture_result,
                &hardware_result,
                &clothing_result,
                &armory_result,
                &paper_result,
                &canned_food_result,
            ]
            .into_iter()
            .flatten()
            .map(|r| r.labor_used)
            .sum::<u32>();

        let Some(nation) = game.get_nation_mut(nation_id) else {
            continue;
        };
        let mut inventory_reservations = Vec::new();
        let mut inventory_summary = BTreeMap::new();
        for (resource, qty) in resource_needs {
            if qty == 0 {
                continue;
            }
            if let Ok(id) = nation.economy.reserve(Commodity::Resource(resource), qty) {
                inventory_reservations.push(id);
                inventory_summary.insert(Commodity::Resource(resource), qty);
            }
        }
        for (material, qty) in material_needs {
            if qty == 0 {
                continue;
            }
            if let Ok(id) = nation.economy.reserve(Commodity::Material(material), qty) {
                inventory_reservations.push(id);
                inventory_summary.insert(Commodity::Material(material), qty);
            }
        }
        let labor_reserved = nation
            .economy
            .reserve_labor_units_with(labor_used, untrained_mult, trained_mult, expert_mult)
            .unwrap_or_default();
        if !inventory_summary.is_empty() || !labor_reserved.is_empty() {
            register_pending_order(
                game,
                nation_id,
                PendingEconomyOrder {
                    phase: PendingOrderPhase::Produce,
                    inventory: inventory_summary.clone(),
                    treasury: Money::ZERO,
                    labor: labor_reserved.clone(),
                },
            );
        }
        out.push(NationReservation {
            nation_id,
            inventory_reservations,
            inventory_summary,
            treasury_reserved: Money::ZERO,
            labor_reserved,
        });
    }

    out
}

fn reserve_trade_phase(game: &mut GameState) -> Vec<NationReservation> {
    let human_id = game.human_player_nation;
    let blockade_capacity = compute_blockade_capacity(game);
    let gp_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power() && !n.diplomacy.is_in_anarchy)
        .map(|n| n.id)
        .collect();

    let offers = trade::generate_minor_nation_offers(
        &game.world.nations,
        &game.world.provinces,
        &game.world.hex_map,
    );
    let mut all_bids = Vec::new();
    for gp_id in &gp_ids {
        if *gp_id == human_id {
            if let Some(human) = game.get_nation(*gp_id) {
                for order in &human.diplomacy.player_buy_orders {
                    if order.quantity > 0 {
                        all_bids.push(trade::TradeBid {
                            buyer: *gp_id,
                            resource: order.resource,
                            quantity: order.quantity,
                            max_price_per_unit: order.max_price_per_unit,
                        });
                    }
                }
            }
            continue;
        }
        if let Some(nation) = game.get_nation(*gp_id) {
            let cargo_capacity = blockade_capacity
                .get(gp_id)
                .copied()
                .unwrap_or_else(|| nation.total_cargo_capacity(&game.game_data));
            all_bids.extend(trade::generate_smart_bids(
                nation,
                &offers,
                &game.world.diplomacy,
                cargo_capacity,
            ));
        }
    }

    let mut bid_spend: HashMap<NationId, Money> = HashMap::new();
    for bid in &all_bids {
        let cost = Money::dollars(bid.max_price_per_unit.as_dollars() * i64::from(bid.quantity));
        *bid_spend.entry(bid.buyer).or_insert(Money::ZERO) += cost;
    }

    let mut out = Vec::new();
    for nation_id in gp_ids {
        let Some(nation_ro) = game.get_nation(nation_id) else {
            continue;
        };
        let subsidy_total: Money = nation_ro
            .diplomacy
            .trade_subsidies
            .values()
            .copied()
            .fold(Money::ZERO, |acc, cost| acc + cost);
        let sell_orders = if nation_id == human_id {
            nation_ro.diplomacy.player_sell_orders.clone()
        } else {
            Vec::new()
        };
        let reserve_requests: Vec<(Commodity, u32)> = sell_orders
            .iter()
            .filter_map(|order| match order.commodity {
                Commodity::Resource(resource) => {
                    if order.quantity > 0 && nation_ro.resource_amount(resource) >= order.quantity {
                        Some((Commodity::Resource(resource), order.quantity))
                    } else {
                        None
                    }
                }
                Commodity::Material(material) => {
                    let stock = nation_ro
                        .economy
                        .materials
                        .get(&material)
                        .copied()
                        .unwrap_or(0);
                    let qty = stock.min(order.quantity);
                    (qty > 0).then_some((Commodity::Material(material), qty))
                }
                Commodity::Goods(goods) => {
                    let stock = nation_ro.economy.goods.get(&goods).copied().unwrap_or(0);
                    let qty = stock.min(order.quantity);
                    (qty > 0).then_some((Commodity::Goods(goods), qty))
                }
            })
            .collect();

        let mut inventory_summary = BTreeMap::new();
        let mut inventory_reservations = Vec::new();
        for (commodity, qty) in reserve_requests {
            if let Ok(id) = reserve_inventory(game, nation_id, commodity, qty) {
                inventory_reservations.push(id);
                inventory_summary.insert(commodity, qty);
            }
        }

        let target_bid_spend = bid_spend.get(&nation_id).copied().unwrap_or(Money::ZERO);
        let Some(nation) = game.get_nation_mut(nation_id) else {
            continue;
        };
        let mut treasury_reserved = Money::ZERO;
        if subsidy_total > Money::ZERO && nation.economy.reserve_treasury(subsidy_total).is_ok() {
            treasury_reserved += subsidy_total;
        }
        let bid_reserve = if target_bid_spend <= nation.economy.available_treasury() {
            target_bid_spend
        } else {
            nation.economy.available_treasury()
        };
        if bid_reserve > Money::ZERO && nation.economy.reserve_treasury(bid_reserve).is_ok() {
            treasury_reserved += bid_reserve;
        }

        if !inventory_summary.is_empty() || treasury_reserved > Money::ZERO {
            register_pending_order(
                game,
                nation_id,
                PendingEconomyOrder {
                    phase: PendingOrderPhase::Trade,
                    inventory: inventory_summary.clone(),
                    treasury: treasury_reserved,
                    labor: HashMap::new(),
                },
            );
        }
        out.push(NationReservation {
            nation_id,
            inventory_reservations,
            inventory_summary,
            treasury_reserved,
            labor_reserved: HashMap::new(),
        });
    }
    out
}

fn building_capacity(nation: &crate::nation::Nation, building_type: BuildingType) -> u32 {
    nation
        .economy
        .buildings
        .iter()
        .find(|b| b.building_type == building_type)
        .map(|b| b.effective_capacity())
        .unwrap_or(0)
}

pub struct LaborBudgets {
    pub timber_mill: u32,
    pub metal_mill: u32,
    pub textile_mill: u32,
    pub lumber_factory: u32,
    pub steel_factory: u32,
    pub garment_factory: u32,
    pub armory: u32,
    pub paper_factory: u32,
    pub canned_food_factory: u32,
}

/// Building capacity inputs for labor allocation.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuildingCapacities {
    pub timber: u32,
    pub metal: u32,
    pub textile: u32,
    pub furniture: u32,
    pub hardware: u32,
    pub clothing: u32,
    pub armory: u32,
    pub paper: u32,
    pub canned_food: u32,
}

/// Distribute total labor across production steps using output targets as weights.
///
/// Each step's desired labor = min(target, capacity) × 2. If total_labor ≥ total
/// desired, each step gets exactly what it needs. Otherwise the Hamilton
/// (largest-remainder) method distributes the available labor proportionally,
/// guaranteeing sum(budgets) == min(total_labor, total_desired).
/// Steps with zero capacity always get zero.
pub fn allocate_labor(
    total_labor: u32,
    targets: &crate::nation::ChainOutputTargets,
    caps: BuildingCapacities,
) -> LaborBudgets {
    let timber_cap = caps.timber;
    let metal_cap = caps.metal;
    let textile_cap = caps.textile;
    let furniture_cap = caps.furniture;
    let hardware_cap = caps.hardware;
    let clothing_cap = caps.clothing;
    let armory_cap = caps.armory;
    let paper_cap = caps.paper;
    let canned_food_cap = caps.canned_food;
    let desired = [
        if timber_cap > 0 {
            targets.timber_mill.min(timber_cap).saturating_mul(2)
        } else {
            0
        },
        if metal_cap > 0 {
            targets.metal_mill.min(metal_cap).saturating_mul(2)
        } else {
            0
        },
        if textile_cap > 0 {
            targets.textile_mill.min(textile_cap).saturating_mul(2)
        } else {
            0
        },
        if furniture_cap > 0 {
            targets.lumber_factory.min(furniture_cap).saturating_mul(2)
        } else {
            0
        },
        if hardware_cap > 0 {
            targets.steel_factory.min(hardware_cap).saturating_mul(2)
        } else {
            0
        },
        if clothing_cap > 0 {
            targets.garment_factory.min(clothing_cap).saturating_mul(2)
        } else {
            0
        },
        if armory_cap > 0 {
            targets.armory.min(armory_cap).saturating_mul(2)
        } else {
            0
        },
        if paper_cap > 0 {
            targets.paper_factory.min(paper_cap).saturating_mul(2)
        } else {
            0
        },
        if canned_food_cap > 0 {
            targets
                .canned_food_factory
                .min(canned_food_cap)
                .saturating_mul(2)
        } else {
            0
        },
    ];
    let total_desired: u32 = desired
        .iter()
        .copied()
        .fold(0u32, |a, b| a.saturating_add(b));
    if total_desired == 0 || total_labor == 0 {
        return LaborBudgets {
            timber_mill: 0,
            metal_mill: 0,
            textile_mill: 0,
            lumber_factory: 0,
            steel_factory: 0,
            garment_factory: 0,
            armory: 0,
            paper_factory: 0,
            canned_food_factory: 0,
        };
    }
    if total_labor >= total_desired {
        return LaborBudgets {
            timber_mill: desired[0],
            metal_mill: desired[1],
            textile_mill: desired[2],
            lumber_factory: desired[3],
            steel_factory: desired[4],
            garment_factory: desired[5],
            armory: desired[6],
            paper_factory: desired[7],
            canned_food_factory: desired[8],
        };
    }
    // Hamilton: distribute total_labor proportionally to desired weights
    let mut floors = [0u32; 9];
    let mut remainders = [0u32; 9];
    let mut sum = 0u32;
    for (i, &w) in desired.iter().enumerate() {
        if w > 0 {
            floors[i] = total_labor * w / total_desired;
            remainders[i] = (total_labor * w) % total_desired;
            sum += floors[i];
        }
    }
    let leftover = total_labor.saturating_sub(sum);
    if leftover > 0 {
        let mut order: Vec<usize> = (0..9).filter(|&i| desired[i] > 0).collect();
        order.sort_by(|&a, &b| remainders[b].cmp(&remainders[a]));
        for &idx in order.iter().take(leftover as usize) {
            floors[idx] += 1;
        }
    }
    LaborBudgets {
        timber_mill: floors[0],
        metal_mill: floors[1],
        textile_mill: floors[2],
        lumber_factory: floors[3],
        steel_factory: floors[4],
        garment_factory: floors[5],
        armory: floors[6],
        paper_factory: floors[7],
        canned_food_factory: floors[8],
    }
}

/// Cap resources to the amount required by target output, so each step
/// consumes no more than needed to hit its target.
pub fn apply_feed_to_resources(
    resources: &[(ResourceType, u32)],
    targets: &crate::nation::ChainOutputTargets,
) -> Vec<(ResourceType, u32)> {
    resources
        .iter()
        .map(|(r, qty)| {
            let cap = match r {
                ResourceType::Timber => targets.timber_mill.saturating_mul(2),
                ResourceType::Coal | ResourceType::Iron => targets.metal_mill,
                ResourceType::Cotton | ResourceType::Wool => targets.textile_mill.saturating_mul(2),
                ResourceType::Grain
                | ResourceType::Fruit
                | ResourceType::Fish
                | ResourceType::Livestock => targets.canned_food_factory,
                _ => u32::MAX,
            };
            (*r, (*qty).min(cap))
        })
        .collect()
}

/// Cap materials to the amount required by target output.
pub fn apply_feed_to_materials(
    materials: &[(MaterialType, u32)],
    targets: &crate::nation::ChainOutputTargets,
) -> Vec<(MaterialType, u32)> {
    materials
        .iter()
        .map(|(m, qty)| {
            let cap = match m {
                MaterialType::Lumber => targets
                    .lumber_factory
                    .saturating_mul(2)
                    .saturating_add(targets.paper_factory.saturating_mul(2)),
                MaterialType::Steel => targets
                    .steel_factory
                    .saturating_mul(2)
                    .saturating_add(targets.armory),
                MaterialType::Fabric => targets.garment_factory.saturating_mul(2),
                _ => u32::MAX,
            };
            (*m, (*qty).min(cap))
        })
        .collect()
}

fn reserve_inventory(
    game: &mut GameState,
    nation_id: NationId,
    commodity: Commodity,
    qty: u32,
) -> Result<ReservationId, crate::DomainError> {
    let nation = game
        .get_nation_mut(nation_id)
        .ok_or(crate::DomainError::NationNotFound(nation_id))?;
    nation.economy.reserve(commodity, qty)
}

fn register_pending_order(game: &mut GameState, nation_id: NationId, order: PendingEconomyOrder) {
    game.transient
        .pending_economy_orders
        .entry(nation_id)
        .or_default()
        .push(order);
}

/// Execute a set of reserved economy actions.
///
/// Calls into `processor.rs` for the heavy execution logic. Does NOT release
/// reservations — the caller (`processor.rs`) is responsible for calling
/// `release_all_reservations` once after all economy batches complete so that
/// reservations survive across collect → production → trade within a single turn.
pub(super) fn execute_reserved_economy(
    game: &mut GameState,
    report: &mut TurnReport,
    reserved: Vec<ReservedAction>,
) {
    for action in reserved {
        let _ = action.commitment_count();
        match action.order.kind {
            EconomicOrderKind::CollectTileResources => {
                super::processor::collect_resources(game, report);
            }
            EconomicOrderKind::RunProduction => {
                super::processor::run_production(game, report);
            }
            EconomicOrderKind::ExecuteTrade => {
                let blockade_capacity = compute_blockade_capacity(game);
                super::trade_phase::resolve_trade_session(game, report, &blockade_capacity);
            }
        }
    }
}

/// Bankruptcy floor: treasury cannot go below $0.
const BANKRUPTCY_FLOOR: Money = Money::ZERO;

/// Tick all buildings for all nations, advancing expansion timers.
pub(super) fn tick_buildings(game: &mut GameState) {
    for nation in &mut game.world.nations {
        for building in &mut nation.economy.buildings {
            building.tick();
        }
    }
}

/// Apply army maintenance, bankruptcy clamp, and bankruptcy headline.
pub(super) fn apply_maintenance(game: &mut GameState, report: &mut TurnReport) {
    let cents_per_arm = game.game_data.game_config.army_maintenance_cents_per_arm;
    for nation in &mut game.world.nations {
        if nation.diplomacy.is_in_anarchy {
            continue;
        }
        let total_cost: Money = nation
            .military
            .army
            .iter()
            .map(|u| u.maintenance_cost(cents_per_arm))
            .fold(Money::ZERO, |acc, c| acc + c);
        if total_cost != Money::ZERO {
            nation.economy.treasury -= total_cost;
            report.maintenance_costs.push((nation.id, total_cost));
        }

        // Track negativity BEFORE clamping — is_bankrupt() checks treasury < 0,
        // which is always false after the floor is applied (F-017 fix).
        let went_bankrupt = nation.economy.treasury < Money::ZERO;

        // Bankruptcy protection: treasury cannot go below $0. The clamp
        // represents debt forgiven mid-turn — we surface it as income in the
        // cash-flow ledger so the reconciliation invariant closes.
        if nation.economy.treasury < BANKRUPTCY_FLOOR {
            let writeoff =
                Money::from_cents(BANKRUPTCY_FLOOR.cents() - nation.economy.treasury.cents());
            nation.economy.treasury = BANKRUPTCY_FLOOR;
            if writeoff > Money::ZERO {
                report.bankruptcy_writeoff.push((nation.id, writeoff));
            }
        }

        if went_bankrupt {
            report.newspaper_headlines.push(
                Headline::new(
                    format!("FINANCIAL CRISIS: {} faces bankruptcy!", nation.name),
                    HeadlineCategory::Crisis,
                )
                .for_nation(nation.id),
            );
        }
    }
}

/// Compute blockade-adjusted cargo capacity for all Great Powers.
///
/// For each GP at war with an enemy that has warships, reduce their effective
/// cargo capacity using `calculate_blockade_effect`. This map is passed to the
/// trade session so blockades actually reduce trade volume.
pub(super) fn compute_blockade_capacity(game: &GameState) -> HashMap<NationId, u32> {
    use crate::map::sea_zones::SeaZoneId;

    // Only consider active Great Powers (not anarchic, not eliminated)
    let active_gp_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power() && !n.diplomacy.is_in_anarchy && !n.province_ids.is_empty())
        .map(|n| n.id)
        .collect();

    let mut capacity_map = HashMap::new();

    for &nation_id in &active_gp_ids {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => continue,
        };
        let raw_cargo = nation.total_cargo_capacity(&game.game_data);

        // Find all ocean sea zones adjacent to this nation's coastal provinces.
        // Zone-local blockade: enemy warships only threaten ports they can reach.
        // Warships with no zone assigned (sea_zone == None) count globally as a
        // fallback (preserves behaviour when zones haven't been computed yet).
        let adjacent_zones: std::collections::HashSet<SeaZoneId> =
            if game.world.sea_zones.is_empty() {
                std::collections::HashSet::new()
            } else {
                nation
                    .province_ids
                    .iter()
                    .filter_map(|&pid| game.get_province(pid))
                    .filter(|p| p.coastal)
                    .flat_map(|prov| {
                        crate::map::sea_zones::ocean_zones_adjacent_to_province(
                            &game.world.sea_zones,
                            prov,
                            &game.world.hex_map,
                        )
                    })
                    .collect()
            };
        let zones_computed = !game.world.sea_zones.is_empty();

        // Only count warships from active enemy nations, and only if the war
        // is past its one-turn grace period (card #104: blockade, like every
        // other hostile action, doesn't fire on the declaration turn).
        let mut enemy_warship_count: u32 = 0;
        for &other_id in &active_gp_ids {
            if other_id == nation_id {
                continue;
            }
            let hostile = game
                .world
                .diplomacy
                .get_relation(nation_id, other_id)
                .is_some_and(|r| r.hostilities_active_on(game.turn));
            if !hostile {
                continue;
            }
            if let Some(other) = game.get_nation(other_id) {
                for ship in &other.military.warships {
                    let counts = match ship.sea_zone {
                        // Zones not computed: count all ships globally (legacy / test mode)
                        None if !zones_computed => true,
                        // Zones computed but ship undeployed: no blockade effect
                        None => false,
                        // Ship in an adjacent ocean zone: blockades this nation
                        Some(zone_id) if adjacent_zones.contains(&zone_id) => true,
                        // Ship in a non-adjacent zone: no effect
                        _ => false,
                    };
                    if counts {
                        enemy_warship_count += 1;
                    }
                }
            }
        }

        let effective = if enemy_warship_count > 0 {
            calculate_blockade_effect(raw_cargo, enemy_warship_count)
        } else {
            raw_cargo
        };
        capacity_map.insert(nation_id, effective);
    }

    capacity_map
}

/// Apply blockade effects: emit headlines for nations whose enemy warships
/// are reducing trade cargo capacity.
///
/// Trade resolution itself already consumed the blockade-adjusted capacity
/// from `compute_blockade_capacity`; this pass only generates the player-
/// facing newspaper line.
pub(super) fn apply_blockade_effects(game: &GameState, report: &mut TurnReport) {
    let gp_ids: Vec<NationId> = game
        .world
        .nations
        .iter()
        .filter(|n| n.is_great_power())
        .map(|n| n.id)
        .collect();

    for &nation_id in &gp_ids {
        let nation = match game.get_nation(nation_id) {
            Some(n) => n,
            None => continue,
        };

        let cargo = nation.total_cargo_capacity(&game.game_data);
        if cargo == 0 {
            continue;
        }

        // Sum up enemy warship counts, gated by the one-turn grace period
        // (card #104: blockade effects don't apply on the declaration turn).
        let mut enemy_warship_count: u32 = 0;
        for &other_id in &gp_ids {
            if other_id == nation_id {
                continue;
            }
            let hostile = game
                .world
                .diplomacy
                .get_relation(nation_id, other_id)
                .is_some_and(|r| r.hostilities_active_on(game.turn));
            if hostile && let Some(other) = game.get_nation(other_id) {
                enemy_warship_count += other.warship_count() as u32;
            }
        }

        if enemy_warship_count > 0 {
            let effective_cargo = calculate_blockade_effect(cargo, enemy_warship_count);
            let blocked = cargo - effective_cargo;
            if blocked > 0 {
                let nation_name = &nation.name;
                report.newspaper_headlines.push(
                    Headline::new(
                        format!(
                            "BLOCKADE: {} merchant fleet loses {} cargo capacity to enemy warships",
                            nation_name, blocked
                        ),
                        HeadlineCategory::Battle,
                    )
                    .for_nation(nation_id),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nation::ChainOutputTargets;

    fn targets_max() -> ChainOutputTargets {
        ChainOutputTargets {
            timber_mill: u32::MAX,
            metal_mill: u32::MAX,
            textile_mill: u32::MAX,
            lumber_factory: u32::MAX,
            steel_factory: u32::MAX,
            garment_factory: u32::MAX,
            armory: 0,
            paper_factory: 0,
            canned_food_factory: 0,
        }
    }
    fn targets_zero() -> ChainOutputTargets {
        ChainOutputTargets {
            timber_mill: 0,
            metal_mill: 0,
            textile_mill: 0,
            lumber_factory: 0,
            steel_factory: 0,
            garment_factory: 0,
            armory: 0,
            paper_factory: 0,
            canned_food_factory: 0,
        }
    }
    fn targets_n(n: u32) -> ChainOutputTargets {
        ChainOutputTargets {
            timber_mill: n,
            metal_mill: n,
            textile_mill: n,
            lumber_factory: n,
            steel_factory: n,
            garment_factory: n,
            armory: 0,
            paper_factory: 0,
            canned_food_factory: 0,
        }
    }

    #[test]
    fn allocate_labor_proportional_by_desired() {
        // timber target=75, metal target=25, textile target=0 (no cap)
        // desired: timber 75*2=150, metal 25*2=50; 100 labor → split 3:1
        let mut t = targets_max();
        t.timber_mill = 75;
        t.metal_mill = 25;
        t.textile_mill = 0;
        let budgets = allocate_labor(
            100,
            &t,
            BuildingCapacities {
                timber: 75,
                metal: 25,
                textile: 0,
                furniture: 0,
                hardware: 0,
                clothing: 0,
                armory: 0,
                paper: 0,
                canned_food: 0,
            },
        );
        assert_eq!(budgets.timber_mill, 75);
        assert_eq!(budgets.metal_mill, 25);
        assert_eq!(budgets.textile_mill, 0);
    }

    #[test]
    fn allocate_labor_zero_target_stops_step() {
        let t = targets_zero();
        let budgets = allocate_labor(
            100,
            &t,
            BuildingCapacities {
                timber: 1,
                metal: 1,
                textile: 1,
                furniture: 1,
                hardware: 1,
                clothing: 1,
                armory: 0,
                paper: 0,
                canned_food: 0,
            },
        );
        assert_eq!(budgets.timber_mill, 0);
        assert_eq!(budgets.metal_mill, 0);
        assert_eq!(budgets.textile_mill, 0);
        assert_eq!(budgets.lumber_factory, 0);
        assert_eq!(budgets.steel_factory, 0);
        assert_eq!(budgets.garment_factory, 0);
    }

    #[test]
    fn allocate_labor_zero_cap_gets_zero() {
        let t = ChainOutputTargets {
            timber_mill: u32::MAX,
            metal_mill: u32::MAX,
            textile_mill: 0,
            lumber_factory: 0,
            steel_factory: 0,
            garment_factory: 0,
            armory: 0,
            paper_factory: 0,
            canned_food_factory: 0,
        };
        // cap=0 for timber_mill → zero, all labor to metal_mill (cap=1)
        let budgets = allocate_labor(
            100,
            &t,
            BuildingCapacities {
                timber: 0,
                metal: 1,
                textile: 0,
                furniture: 0,
                hardware: 0,
                clothing: 0,
                armory: 0,
                paper: 0,
                canned_food: 0,
            },
        );
        assert_eq!(budgets.timber_mill, 0);
        assert_eq!(budgets.metal_mill, 2); // min(MAX,1)*2 = 2; total_labor=100 >= 2 → exact
    }

    #[test]
    fn allocate_labor_exact_when_enough() {
        // cap=10 each, target=MAX → desired = 20 each; total=120 → exact
        let t = ChainOutputTargets {
            timber_mill: u32::MAX,
            metal_mill: u32::MAX,
            textile_mill: u32::MAX,
            lumber_factory: 0,
            steel_factory: 0,
            garment_factory: 0,
            armory: 0,
            paper_factory: 0,
            canned_food_factory: 0,
        };
        let budgets = allocate_labor(
            120,
            &t,
            BuildingCapacities {
                timber: 10,
                metal: 10,
                textile: 10,
                furniture: 0,
                hardware: 0,
                clothing: 0,
                armory: 0,
                paper: 0,
                canned_food: 0,
            },
        );
        assert_eq!(budgets.timber_mill, 20);
        assert_eq!(budgets.metal_mill, 20);
        assert_eq!(budgets.textile_mill, 20);
    }

    #[test]
    fn apply_feed_zero_target_yields_zero() {
        let mut t = targets_max();
        t.timber_mill = 0;
        let resources = vec![(ResourceType::Timber, 200u32)];
        let fed = apply_feed_to_resources(&resources, &t);
        assert_eq!(fed[0].1, 0);
    }

    #[test]
    fn apply_feed_max_target_is_passthrough() {
        let t = ChainOutputTargets {
            timber_mill: u32::MAX,
            metal_mill: u32::MAX,
            textile_mill: u32::MAX,
            lumber_factory: u32::MAX,
            steel_factory: u32::MAX,
            garment_factory: u32::MAX,
            armory: 0,
            paper_factory: 0,
            canned_food_factory: 0,
        };
        let resources = vec![(ResourceType::Timber, 300u32), (ResourceType::Coal, 150u32)];
        let fed = apply_feed_to_resources(&resources, &t);
        assert_eq!(fed[0].1, 300);
        assert_eq!(fed[1].1, 150);
    }

    #[test]
    fn apply_material_cap_by_target() {
        // lumber_factory target=30 → lumber capped to 60 (30*2)
        let mut t = targets_max();
        t.lumber_factory = 30;
        let materials = vec![(MaterialType::Lumber, 200u32)];
        let fed = apply_feed_to_materials(&materials, &t);
        assert_eq!(fed[0].1, 60);
    }

    #[test]
    fn allocate_labor_does_not_exceed_total() {
        // caps all 6 at 10, MAX targets → desired = 20 each = 120; labor = 97 < 120
        let t = targets_n(u32::MAX);
        let total = 97u32;
        let budgets = allocate_labor(
            total,
            &t,
            BuildingCapacities {
                timber: 10,
                metal: 10,
                textile: 10,
                furniture: 10,
                hardware: 10,
                clothing: 10,
                armory: 0,
                paper: 0,
                canned_food: 0,
            },
        );
        let sum = budgets.timber_mill
            + budgets.metal_mill
            + budgets.textile_mill
            + budgets.lumber_factory
            + budgets.steel_factory
            + budgets.garment_factory;
        assert!(sum <= total, "allocated {sum} exceeds total {total}");
    }
}
