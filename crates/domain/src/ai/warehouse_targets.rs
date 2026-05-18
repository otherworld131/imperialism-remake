//! Per-nation warehouse stock targets — the levels the AI is aiming to keep
//! in stock, surfaced for the debug overlay in the web UI.
//!
//! The formulas mirror two existing AI subsystems:
//! - **Resources** (raw): the buy-bid stockpile target used by
//!   `crate::economy::trade::generate_need_based_bids` — projected per-turn
//!   demand × `trade_buy_buffer_turns`.
//! - **Materials & Goods** (manufactured): the sell-side reserve held back
//!   in `crate::turn::trade_phase` so the AI doesn't liquidate stocks the
//!   chain (or the immigrant queue) is about to need.
//!
//! Keep these formulas in sync with the canonical sites. A regression test
//! covers the round-trip.

use crate::economy::trade;
use crate::game_state::GameState;
use crate::types::*;
use std::collections::BTreeMap;

/// Stock targets the AI is aiming to maintain per warehouse bucket.
#[derive(Debug, Clone, Default)]
pub struct WarehouseTargets {
    pub resources: BTreeMap<ResourceType, u32>,
    pub materials: BTreeMap<MaterialType, u32>,
    pub goods: BTreeMap<GoodsType, u32>,
}

/// Compute the warehouse targets for `nation_id` using the same formulas
/// the AI uses to drive buy bids and sell reserves.
pub fn compute_warehouse_targets(game: &GameState, nation_id: NationId) -> WarehouseTargets {
    let Some(nation) = game.get_nation(nation_id) else {
        return WarehouseTargets::default();
    };

    let cfg = &game.game_data.game_config;
    let personality = super::common::get_personality(game, nation_id);

    // ── Resources: chain-input stockpile target ────────────────────
    let buffer_turns = super::economy::trade_buy_buffer_turns(game, personality);
    let mut resources: BTreeMap<ResourceType, u32> = BTreeMap::new();
    for (r, per_turn) in trade::projected_resource_needs(nation) {
        resources.insert(r, per_turn.saturating_mul(buffer_turns));
    }

    // ── Materials & Goods: sell-side reserves ──────────────────────
    let per_turn = super::economy::expansions_per_turn_target(game, personality);
    let buildings_factor = super::economy::expansion_reserve_buildings_factor(game, personality);
    let (lumber_reserve, steel_reserve) =
        super::economy::reserve_for_expansion(game, nation_id, per_turn, buildings_factor);
    let (m_fabric_reserve, m_lumber_reserve, m_steel_reserve, _m_coal) =
        super::naval::merchant_navy_material_reserve(game, nation_id);
    let arms_reserve_total = nation
        .pending_recruits_arms_cost()
        .saturating_add(super::economy::arms_sell_reserve(game, personality));

    let pending_immig = nation.economy.pending_immigration;
    let immig_canned_food_reserve = pending_immig.saturating_mul(cfg.immigration_canned_food);
    let immig_clothing_reserve = pending_immig.saturating_mul(cfg.immigration_clothing);
    let immig_furniture_reserve = pending_immig.saturating_mul(cfg.immigration_furniture);

    let cap_of = |bt: crate::economy::BuildingType| -> u32 {
        nation
            .economy
            .buildings
            .iter()
            .find(|b| b.building_type == bt)
            .map(|b| b.effective_capacity())
            .unwrap_or(0)
    };
    let clothing_cap = cap_of(crate::economy::BuildingType::ClothingFactory);
    let furniture_cap_floor = cap_of(crate::economy::BuildingType::FurnitureFactory) * 2;
    let clothing_cap_floor = clothing_cap * 2;
    let armory_cap_floor = cap_of(crate::economy::BuildingType::Armory) * 2;
    let paper_cap_floor = cap_of(crate::economy::BuildingType::PaperFactory) * 2;

    let pending_train_paper = nation
        .economy
        .pending_train_to_trained
        .saturating_mul(cfg.train_to_trained_paper_cost)
        .saturating_add(
            nation
                .economy
                .pending_train_to_expert
                .saturating_mul(cfg.train_to_expert_paper_cost),
        );
    let paper_reserve = pending_train_paper
        .saturating_add(cfg.strategic_paper_reserve)
        .max(paper_cap_floor);

    let fabric_chain_reserve = nation
        .economy
        .chain_targets
        .garment_factory
        .min(clothing_cap)
        .saturating_mul(cfg.materials_per_good);

    let canned_food_stockpile_target = super::lua_bridge::get_personality_config(game, personality)
        .as_ref()
        .and_then(|c| c.canned_food_stockpile_target)
        .unwrap_or(20);
    let canned_food_reserve_total =
        immig_canned_food_reserve.saturating_add(canned_food_stockpile_target);

    let mut materials = BTreeMap::new();
    materials.insert(
        MaterialType::Lumber,
        lumber_reserve.saturating_add(m_lumber_reserve),
    );
    materials.insert(
        MaterialType::Steel,
        steel_reserve.saturating_add(m_steel_reserve),
    );
    materials.insert(
        MaterialType::Fabric,
        m_fabric_reserve.saturating_add(fabric_chain_reserve),
    );
    materials.insert(MaterialType::CannedFood, canned_food_reserve_total);
    materials.insert(MaterialType::Paper, paper_reserve);

    let mut goods = BTreeMap::new();
    goods.insert(GoodsType::Arms, arms_reserve_total.max(armory_cap_floor));
    goods.insert(
        GoodsType::Clothing,
        immig_clothing_reserve.max(clothing_cap_floor),
    );
    goods.insert(
        GoodsType::Furniture,
        immig_furniture_reserve.max(furniture_cap_floor),
    );
    goods.insert(GoodsType::Hardware, 0);

    WarehouseTargets {
        resources,
        materials,
        goods,
    }
}
