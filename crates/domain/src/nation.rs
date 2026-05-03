use crate::ai::AiPersonality;
use crate::economy::buildings::{Building, BuildingType};
use crate::economy::civilians::{Civilian, CivilianType};
use crate::economy::labor::{LaborPool, WorkerType};
use crate::economy::ledger::{CashSink, CashSource};
use crate::economy::observability::BlockReason;
use crate::economy::trade::Commodity;
use crate::economy::transport::{LogisticsState, TransportSystem};
use crate::events::TechId;
use crate::military::ships::Ship;
use crate::military::units::{ArmyUnit, ArmyUnitType};
use crate::types::*;
use std::collections::{BTreeMap, HashMap};

/// Colors used to distinguish nations on the map and in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NationColor {
    // Great Power colors (first 7 match the original game)
    Yellow,
    Orange,
    LightBlue,
    Red,
    Green,
    Purple,
    Blue,
    // Additional GP colors — used when num_great_powers > 7 so each GP still
    // gets a visually distinct hue (every-frame color matters for tile fill).
    Crimson,
    Magenta,
    Forest,
    Gold,
    Aqua,
    Violet,
    BurntOrange,
    HotPink,
    Turquoise,
    Slate,
    Mauve,
    Sage,
    Mustard,
    // Minor nation colors
    Gray,
    Brown,
    Pink,
    Teal,
    Olive,
    Maroon,
    Navy,
    Cyan,
    Lime,
    Coral,
    Lavender,
    Tan,
    Salmon,
    Khaki,
    Indigo,
}

/// Economy substruct — owns inventory, labor, treasury, and buildings.
///
/// Extracted from `Nation` (KEYSTONE refactor). Future economy work
/// (reservations, snapshots, plan/reserve/execute) adds fields here, not
/// to `Nation`.
pub struct ReservationStateSnapshot {
    pub reserved_treasury: Money,
    pub reserved_warehouse: BTreeMap<ResourceType, u32>,
    pub reserved_materials: BTreeMap<MaterialType, u32>,
    pub reserved_goods: BTreeMap<GoodsType, u32>,
    pub reservation_ledger: BTreeMap<ReservationId, (Commodity, u32)>,
    pub next_reservation_id: u64,
    pub reserved_labor: HashMap<WorkerType, u32>,
}

/// Player-controlled output targets for each production chain step.
///
/// Each value is a desired output quantity (in units) for the step this turn.
/// `u32::MAX` means "produce as much as possible" (no target cap applied).
/// Setting a value to 0 stops production for that step entirely.
/// AI nations use defaults (all `u32::MAX`) from `NationEconomy::new()`.
#[derive(Debug, Clone)]
pub struct ChainOutputTargets {
    pub timber_mill: u32,     // target lumber units
    pub metal_mill: u32,      // target steel units
    pub textile_mill: u32,    // target fabric units
    pub lumber_factory: u32,  // target furniture units
    pub steel_factory: u32,   // target hardware units
    pub garment_factory: u32, // target clothing units
}

impl Default for ChainOutputTargets {
    fn default() -> Self {
        Self {
            timber_mill: u32::MAX,
            metal_mill: u32::MAX,
            textile_mill: u32::MAX,
            lumber_factory: u32::MAX,
            steel_factory: u32::MAX,
            garment_factory: u32::MAX,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NationEconomy {
    pub treasury: Money,
    /// Resource warehouse — stores raw resources.
    pub warehouse: BTreeMap<ResourceType, u32>,
    /// Processed materials warehouse.
    pub materials: BTreeMap<MaterialType, u32>,
    /// Finished goods warehouse.
    pub goods: BTreeMap<GoodsType, u32>,
    /// Buildings owned by this nation.
    pub buildings: Vec<Building>,
    /// Labor pool (workers available for production).
    pub labor: LaborPool,

    // ── Logistics state (Trello #165) ────────────────────────────────
    /// Freight usage snapshot from the most recently processed turn.
    /// Updated by the turn processor after `calculate_deliveries` completes.
    pub logistics: LogisticsState,

    /// Player-controlled production chain output targets.
    pub chain_targets: ChainOutputTargets,

    /// Civilians queued for hiring at end of turn (civilian_type → count).
    pub pending_civilian_hires: HashMap<CivilianType, u32>,

    /// Workers queued for Untrained→Trained advancement at end of turn.
    pub pending_train_to_trained: u32,

    /// Workers queued for Trained→Expert advancement at end of turn.
    pub pending_train_to_expert: u32,

    // ── Reservation accounting (Trello #162 / #169) ──────────────────
    /// Reserved treasury amount (sum of active treasury reservations).
    pub(crate) reserved_treasury: Money,
    /// Per-resource reserved amounts (sum of active reservations).
    pub(crate) reserved_warehouse: BTreeMap<ResourceType, u32>,
    /// Per-material reserved amounts.
    pub(crate) reserved_materials: BTreeMap<MaterialType, u32>,
    /// Per-goods reserved amounts.
    pub(crate) reserved_goods: BTreeMap<GoodsType, u32>,
    /// Active reservation ledger: id → (commodity, quantity).
    pub(crate) reservation_ledger: BTreeMap<ReservationId, (Commodity, u32)>,
    /// Monotonically increasing counter for generating unique ReservationIds.
    pub(crate) next_reservation_id: u64,
    /// Labor reserved for in-flight economy actions (per worker tier).
    pub(crate) reserved_labor: HashMap<WorkerType, u32>,
}

impl NationEconomy {
    pub fn new() -> Self {
        Self {
            treasury: Money::ZERO,
            warehouse: BTreeMap::new(),
            materials: BTreeMap::new(),
            goods: BTreeMap::new(),
            buildings: Vec::new(),
            labor: LaborPool::new(),
            logistics: LogisticsState::new(),
            chain_targets: ChainOutputTargets::default(),
            pending_civilian_hires: HashMap::new(),
            pending_train_to_trained: 0,
            pending_train_to_expert: 0,
            reserved_treasury: Money::ZERO,
            reserved_warehouse: BTreeMap::new(),
            reserved_materials: BTreeMap::new(),
            reserved_goods: BTreeMap::new(),
            reservation_ledger: BTreeMap::new(),
            next_reservation_id: 0,
            reserved_labor: HashMap::new(),
        }
    }

    // ── Unified commodity API (#160) ──────────────────────────────────

    /// Total stored quantity of a commodity (reserved + free).
    pub fn amount(&self, key: Commodity) -> u32 {
        match key {
            Commodity::Resource(r) => self.warehouse.get(&r).copied().unwrap_or(0),
            Commodity::Material(m) => self.materials.get(&m).copied().unwrap_or(0),
            Commodity::Goods(g) => self.goods.get(&g).copied().unwrap_or(0),
        }
    }

    /// Add a quantity of a commodity to this nation's inventory.
    pub fn add(&mut self, key: Commodity, qty: u32) {
        match key {
            Commodity::Resource(r) => *self.warehouse.entry(r).or_insert(0) += qty,
            Commodity::Material(m) => *self.materials.entry(m).or_insert(0) += qty,
            Commodity::Goods(g) => *self.goods.entry(g).or_insert(0) += qty,
        }
    }

    /// Consume a quantity of a commodity. Returns `false` if insufficient
    /// total quantity is held (no mutation on failure).
    pub fn consume(&mut self, key: Commodity, qty: u32) -> bool {
        match key {
            Commodity::Resource(r) => {
                if let Some(cur) = self.warehouse.get_mut(&r)
                    && *cur >= qty
                {
                    *cur -= qty;
                    return true;
                }
                false
            }
            Commodity::Material(m) => {
                if let Some(cur) = self.materials.get_mut(&m)
                    && *cur >= qty
                {
                    *cur -= qty;
                    return true;
                }
                false
            }
            Commodity::Goods(g) => {
                if let Some(cur) = self.goods.get_mut(&g)
                    && *cur >= qty
                {
                    *cur -= qty;
                    return true;
                }
                false
            }
        }
    }

    /// Iterate over all commodities with non-zero total quantity.
    pub fn iter_all(&self) -> impl Iterator<Item = (Commodity, u32)> + '_ {
        let resources = self
            .warehouse
            .iter()
            .filter(|&(_, v)| *v > 0)
            .map(|(&k, &v)| (Commodity::Resource(k), v));
        let materials = self
            .materials
            .iter()
            .filter(|&(_, v)| *v > 0)
            .map(|(&k, &v)| (Commodity::Material(k), v));
        let goods = self
            .goods
            .iter()
            .filter(|&(_, v)| *v > 0)
            .map(|(&k, &v)| (Commodity::Goods(k), v));
        resources.chain(materials).chain(goods)
    }

    // ── Reservation API (#162) ────────────────────────────────────────

    /// How much of a commodity is currently reserved by in-flight reservations.
    pub fn reserved(&self, key: Commodity) -> u32 {
        match key {
            Commodity::Resource(r) => self.reserved_warehouse.get(&r).copied().unwrap_or(0),
            Commodity::Material(m) => self.reserved_materials.get(&m).copied().unwrap_or(0),
            Commodity::Goods(g) => self.reserved_goods.get(&g).copied().unwrap_or(0),
        }
    }

    /// How much of a commodity is free (not reserved) and available for use.
    pub fn available(&self, key: Commodity) -> u32 {
        self.amount(key).saturating_sub(self.reserved(key))
    }

    /// Reserve `qty` units of `key`, returning an opaque `ReservationId`.
    ///
    /// Fails with `DomainError::InsufficientInventory` if `available(key) < qty`.
    /// Reservations must be committed or released before end-of-turn; any
    /// uncommitted reservations are auto-released by `release_all_reservations`.
    pub fn reserve(
        &mut self,
        key: Commodity,
        qty: u32,
    ) -> Result<ReservationId, crate::DomainError> {
        if qty == 0 {
            return Err(crate::DomainError::InvalidOperation(
                "reserve quantity must be positive".into(),
            ));
        }
        let avail = self.available(key);
        if avail < qty {
            return Err(crate::DomainError::InsufficientInventory {
                requested: qty,
                available: avail,
            });
        }
        let id = ReservationId(self.next_reservation_id);
        self.next_reservation_id += 1;
        match key {
            Commodity::Resource(r) => *self.reserved_warehouse.entry(r).or_insert(0) += qty,
            Commodity::Material(m) => *self.reserved_materials.entry(m).or_insert(0) += qty,
            Commodity::Goods(g) => *self.reserved_goods.entry(g).or_insert(0) += qty,
        }
        self.reservation_ledger.insert(id, (key, qty));
        Ok(id)
    }

    /// Commit a reservation: deduct the reserved quantity from total inventory
    /// and remove the reservation entry.
    ///
    /// Fails with `DomainError::ReservationNotFound` if `id` is unknown.
    /// Fails with `DomainError::InsufficientInventory` if inventory was externally
    /// depleted below the reserved amount after the reservation was made.
    pub fn commit(&mut self, id: ReservationId) -> Result<(), crate::DomainError> {
        // Peek before mutating the ledger so we can return an error cleanly.
        let &(key, qty) = self
            .reservation_ledger
            .get(&id)
            .ok_or(crate::DomainError::ReservationNotFound(id))?;
        let available = self.amount(key);
        if available < qty {
            return Err(crate::DomainError::InsufficientInventory {
                requested: qty,
                available,
            });
        }
        // Now safe: remove from ledger, decrement reserved counter, consume inventory.
        self.reservation_ledger.remove(&id);
        match key {
            Commodity::Resource(r) => {
                let e = self.reserved_warehouse.entry(r).or_insert(0);
                debug_assert!(
                    *e >= qty,
                    "reservation counter underflow for Resource({r:?})"
                );
                *e = e.saturating_sub(qty);
            }
            Commodity::Material(m) => {
                let e = self.reserved_materials.entry(m).or_insert(0);
                debug_assert!(
                    *e >= qty,
                    "reservation counter underflow for Material({m:?})"
                );
                *e = e.saturating_sub(qty);
            }
            Commodity::Goods(g) => {
                let e = self.reserved_goods.entry(g).or_insert(0);
                debug_assert!(*e >= qty, "reservation counter underflow for Goods({g:?})");
                *e = e.saturating_sub(qty);
            }
        }
        self.consume(key, qty);
        Ok(())
    }

    /// Release a reservation without consuming inventory.
    ///
    /// Fails with `DomainError::ReservationNotFound` if `id` is unknown.
    pub fn release(&mut self, id: ReservationId) -> Result<(), crate::DomainError> {
        let (key, qty) = self
            .reservation_ledger
            .remove(&id)
            .ok_or(crate::DomainError::ReservationNotFound(id))?;
        match key {
            Commodity::Resource(r) => {
                let e = self.reserved_warehouse.entry(r).or_insert(0);
                debug_assert!(
                    *e >= qty,
                    "reservation counter underflow for Resource({r:?})"
                );
                *e = e.saturating_sub(qty);
            }
            Commodity::Material(m) => {
                let e = self.reserved_materials.entry(m).or_insert(0);
                debug_assert!(
                    *e >= qty,
                    "reservation counter underflow for Material({m:?})"
                );
                *e = e.saturating_sub(qty);
            }
            Commodity::Goods(g) => {
                let e = self.reserved_goods.entry(g).or_insert(0);
                debug_assert!(*e >= qty, "reservation counter underflow for Goods({g:?})");
                *e = e.saturating_sub(qty);
            }
        }
        Ok(())
    }

    // ── Snapshot access (used by domain-snapshot for save/load) ─────────

    pub fn snapshot_reserved_treasury(&self) -> Money {
        self.reserved_treasury
    }
    pub fn snapshot_reserved_warehouse(&self) -> &BTreeMap<ResourceType, u32> {
        &self.reserved_warehouse
    }
    pub fn snapshot_reserved_materials(&self) -> &BTreeMap<MaterialType, u32> {
        &self.reserved_materials
    }
    pub fn snapshot_reserved_goods(&self) -> &BTreeMap<GoodsType, u32> {
        &self.reserved_goods
    }
    pub fn snapshot_reservation_ledger(&self) -> &BTreeMap<ReservationId, (Commodity, u32)> {
        &self.reservation_ledger
    }
    pub fn snapshot_next_reservation_id(&self) -> u64 {
        self.next_reservation_id
    }
    pub fn snapshot_reserved_labor(&self) -> &HashMap<WorkerType, u32> {
        &self.reserved_labor
    }

    pub fn restore_reservation_state(&mut self, state: ReservationStateSnapshot) {
        self.reserved_treasury = state.reserved_treasury;
        self.reserved_warehouse = state.reserved_warehouse;
        self.reserved_materials = state.reserved_materials;
        self.reserved_goods = state.reserved_goods;
        self.reservation_ledger = state.reservation_ledger;
        self.next_reservation_id = state.next_reservation_id;
        self.reserved_labor = state.reserved_labor;
    }

    /// Release all active reservations for this nation (end-of-turn safety net).
    pub fn release_all_reservations(&mut self) {
        self.reservation_ledger.clear();
        self.reserved_warehouse.clear();
        self.reserved_materials.clear();
        self.reserved_goods.clear();
        self.reserved_treasury = Money::ZERO;
        self.reserved_labor.clear();
    }

    // ── Treasury reservation API (#169) ──────────────────────────────

    /// Mark `amount` of treasury as reserved for a pending action.
    ///
    /// Returns `Err` if `available_treasury() < amount` or if `amount <= Money::ZERO`.
    /// Unlike commodity reservations, treasury reservations are tracked as a
    /// running total (no per-reservation ID) because the amounts are committed
    /// or released in the same call frame where they're reserved.
    pub fn reserve_treasury(&mut self, amount: Money) -> Result<(), crate::DomainError> {
        if amount <= Money::ZERO {
            return Err(crate::DomainError::InvalidOperation(
                "reserve_treasury amount must be positive".into(),
            ));
        }
        let avail = self.available_treasury();
        if avail < amount {
            return Err(crate::DomainError::InsufficientInventory {
                requested: amount.as_dollars() as u32,
                available: avail.as_dollars() as u32,
            });
        }
        self.reserved_treasury += amount;
        Ok(())
    }

    /// Commit a treasury reservation: deduct `amount` from the treasury and release the hold.
    ///
    /// Returns `Err` if `amount <= Money::ZERO` or `amount > reserved_treasury`.
    pub fn commit_treasury(&mut self, amount: Money) -> Result<(), crate::DomainError> {
        if amount <= Money::ZERO {
            return Err(crate::DomainError::InvalidOperation(
                "commit_treasury amount must be positive".into(),
            ));
        }
        if amount > self.reserved_treasury {
            return Err(crate::DomainError::InvalidOperation(
                "commit_treasury over-commit: amount exceeds reserved balance".into(),
            ));
        }
        self.reserved_treasury -= amount;
        self.treasury -= amount;
        Ok(())
    }

    /// Release a treasury reservation without deducting from the treasury.
    ///
    /// Returns `Err` if `amount <= Money::ZERO` or `amount > reserved_treasury`.
    pub fn release_treasury(&mut self, amount: Money) -> Result<(), crate::DomainError> {
        if amount <= Money::ZERO {
            return Err(crate::DomainError::InvalidOperation(
                "release_treasury amount must be positive".into(),
            ));
        }
        if amount > self.reserved_treasury {
            return Err(crate::DomainError::InvalidOperation(
                "release_treasury over-release: amount exceeds reserved balance".into(),
            ));
        }
        self.reserved_treasury -= amount;
        Ok(())
    }

    /// How much treasury is currently reserved for pending actions.
    pub fn reserved_treasury_amount(&self) -> Money {
        self.reserved_treasury
    }

    /// How much treasury is free (not reserved) and available for use.
    pub fn available_treasury(&self) -> Money {
        if self.treasury > self.reserved_treasury {
            self.treasury - self.reserved_treasury
        } else {
            Money::ZERO
        }
    }

    // ── Pre-execution observability query methods (#169) ──────────────

    /// Snapshot of all currently reserved commodity quantities.
    ///
    /// Returns only commodities with non-zero reserved amounts.
    pub fn reserved_inventory(&self) -> std::collections::BTreeMap<Commodity, u32> {
        let mut out = std::collections::BTreeMap::new();
        for (r, &qty) in &self.reserved_warehouse {
            if qty > 0 {
                out.insert(Commodity::Resource(*r), qty);
            }
        }
        for (m, &qty) in &self.reserved_materials {
            if qty > 0 {
                out.insert(Commodity::Material(*m), qty);
            }
        }
        for (g, &qty) in &self.reserved_goods {
            if qty > 0 {
                out.insert(Commodity::Goods(*g), qty);
            }
        }
        out
    }

    /// Snapshot of all currently reserved labor counts by tier.
    pub fn reserved_labor(&self) -> &HashMap<WorkerType, u32> {
        &self.reserved_labor
    }

    /// How much labor (by tier) is currently available for production.
    pub fn available_labor(&self) -> std::collections::HashMap<WorkerType, u32> {
        let mut out = std::collections::HashMap::new();
        out.insert(
            WorkerType::Untrained,
            self.labor.untrained.saturating_sub(
                self.reserved_labor
                    .get(&WorkerType::Untrained)
                    .copied()
                    .unwrap_or(0),
            ),
        );
        out.insert(
            WorkerType::Trained,
            self.labor.trained.saturating_sub(
                self.reserved_labor
                    .get(&WorkerType::Trained)
                    .copied()
                    .unwrap_or(0),
            ),
        );
        out.insert(
            WorkerType::Expert,
            self.labor.expert.saturating_sub(
                self.reserved_labor
                    .get(&WorkerType::Expert)
                    .copied()
                    .unwrap_or(0),
            ),
        );
        out
    }

    /// Available labor units after reservations are applied.
    pub fn available_labor_units_with(
        &self,
        untrained_mult: u32,
        trained_mult: u32,
        expert_mult: u32,
    ) -> u32 {
        let avail = self.available_labor();
        avail.get(&WorkerType::Untrained).copied().unwrap_or(0) * untrained_mult
            + avail.get(&WorkerType::Trained).copied().unwrap_or(0) * trained_mult
            + avail.get(&WorkerType::Expert).copied().unwrap_or(0) * expert_mult
    }

    /// Reserve an exact number of workers of a single tier.
    pub fn reserve_labor(&mut self, tier: WorkerType, qty: u32) -> Result<(), crate::DomainError> {
        if qty == 0 {
            return Ok(());
        }
        if let Some(reason) = self.block_reason_for_labor(tier, qty) {
            return Err(crate::DomainError::illegal(reason));
        }
        *self.reserved_labor.entry(tier).or_insert(0) += qty;
        Ok(())
    }

    /// Release a labor reservation for a single tier.
    pub fn release_labor(&mut self, tier: WorkerType, qty: u32) -> Result<(), crate::DomainError> {
        if qty == 0 {
            return Ok(());
        }
        let reserved = self.reserved_labor.entry(tier).or_insert(0);
        if *reserved < qty {
            return Err(crate::DomainError::InvalidOperation(
                "release_labor over-release: amount exceeds reserved labor".into(),
            ));
        }
        *reserved -= qty;
        if *reserved == 0 {
            self.reserved_labor.remove(&tier);
        }
        Ok(())
    }

    /// Reserve enough workers to cover `required_units` under the provided labor multipliers.
    ///
    /// Workers are reserved from the highest-output tier first so the hold uses the
    /// fewest heads for a given amount of labor.
    pub fn reserve_labor_units_with(
        &mut self,
        required_units: u32,
        untrained_mult: u32,
        trained_mult: u32,
        expert_mult: u32,
    ) -> Result<HashMap<WorkerType, u32>, crate::DomainError> {
        if required_units == 0 {
            return Ok(HashMap::new());
        }
        let available_units =
            self.available_labor_units_with(untrained_mult, trained_mult, expert_mult);
        if available_units < required_units {
            return Err(crate::DomainError::illegal(format!(
                "Need {required_units} labor units but only {available_units} available"
            )));
        }

        let available = self.available_labor();
        let mut plan = HashMap::new();
        let mut remaining_units = required_units;
        for (tier, mult) in [
            (WorkerType::Expert, expert_mult),
            (WorkerType::Trained, trained_mult),
            (WorkerType::Untrained, untrained_mult),
        ] {
            if remaining_units == 0 || mult == 0 {
                continue;
            }
            let have = available.get(&tier).copied().unwrap_or(0);
            if have == 0 {
                continue;
            }
            let needed_workers = remaining_units.div_ceil(mult);
            let reserve = have.min(needed_workers);
            if reserve > 0 {
                self.reserve_labor(tier, reserve)?;
                plan.insert(tier, reserve);
                remaining_units = remaining_units.saturating_sub(reserve * mult);
            }
        }

        debug_assert_eq!(
            remaining_units, 0,
            "labor reservation should fully cover required units"
        );
        Ok(plan)
    }

    /// Why a commodity reservation of `qty` would fail, or `None` if it would succeed.
    pub fn block_reason_for_commodity(&self, key: Commodity, qty: u32) -> Option<BlockReason> {
        let avail = self.available(key);
        if avail < qty {
            Some(BlockReason::InsufficientInventory {
                commodity: key,
                needed: qty,
                available: avail,
            })
        } else {
            None
        }
    }

    /// Why a treasury reservation of `amount` would fail, or `None` if it would succeed.
    pub fn block_reason_for_treasury(&self, amount: Money) -> Option<BlockReason> {
        let avail = self.available_treasury();
        if avail < amount {
            Some(BlockReason::InsufficientTreasury {
                needed: amount,
                available: avail,
            })
        } else {
            None
        }
    }

    /// Why a labor request of `qty` workers of `tier` would fail, or `None` if feasible.
    pub fn block_reason_for_labor(&self, tier: WorkerType, qty: u32) -> Option<BlockReason> {
        let available = match tier {
            WorkerType::Untrained => self.labor.untrained.saturating_sub(
                self.reserved_labor
                    .get(&WorkerType::Untrained)
                    .copied()
                    .unwrap_or(0),
            ),
            WorkerType::Trained => self.labor.trained.saturating_sub(
                self.reserved_labor
                    .get(&WorkerType::Trained)
                    .copied()
                    .unwrap_or(0),
            ),
            WorkerType::Expert => self.labor.expert.saturating_sub(
                self.reserved_labor
                    .get(&WorkerType::Expert)
                    .copied()
                    .unwrap_or(0),
            ),
        };
        if available < qty {
            Some(BlockReason::InsufficientLabor {
                tier,
                needed: qty,
                available,
            })
        } else {
            None
        }
    }
}

impl Default for NationEconomy {
    fn default() -> Self {
        Self::new()
    }
}

/// Military forces, civilian improvers, and transport.
///
/// Extracted from `Nation` (Phase 5 Lesson 8 refactor).
#[derive(Debug, Clone, Default)]
pub struct NationMilitary {
    /// Field army and garrison units.
    pub army: Vec<ArmyUnit>,
    /// Civilian improver units (Farmers, Foresters, Miners, Engineers).
    pub civilians: Vec<Civilian>,
    /// Freight transport system.
    pub transport: TransportSystem,
    /// Merchant fleet — ships used for trade.
    pub merchant_fleet: Vec<Ship>,
    /// Warship fleet — military naval vessels.
    pub warships: Vec<Ship>,
    /// Total arms built (tracks General reward thresholds).
    pub total_arms_built: u32,
    /// Number of Generals earned.
    pub generals_earned: u32,
    /// Total warships built (naval telemetry).
    pub warships_built: u32,
    /// Total warships lost in combat (naval telemetry).
    pub warships_lost: u32,
    /// Total Ships-of-the-Line built (tracks Admiral reward thresholds).
    pub total_ships_of_the_line_built: u32,
    /// Number of Admirals earned.
    pub admirals_earned: u32,
    /// Conquest bonus from capturing GP capitals: each +1 improves worker rate.
    pub capitol_bonus_capacity: u32,
    /// Whether this nation has established its first colony.
    pub has_colony: bool,
    /// Expert worker rewards already earned (thresholds at 10 and 30 experts).
    pub expert_rewards_earned: u8,
    /// Remaining fleet movement budget per sea zone for the current turn.
    /// Key = zone where fleet currently is; value = moves remaining this turn.
    /// Reset at the start of each turn's resolution phase.
    pub fleet_moves_remaining: std::collections::HashMap<crate::map::sea_zones::SeaZoneId, u32>,
}

/// Diplomatic and political state.
///
/// Extracted from `Nation` (Phase 5 Lesson 8 refactor).
#[derive(Debug, Clone, Default)]
pub struct NationDiplomacy {
    /// AI personality for this nation (`None` for human player).
    pub ai_personality: Option<AiPersonality>,
    /// Per-Minor-Nation trade subsidy amounts (GP pays per turn).
    pub trade_subsidies: HashMap<NationId, Money>,
    /// Whether this nation has fallen into anarchy (lost its capital).
    pub is_in_anarchy: bool,
    /// Overlord GP if this minor nation has been diplomatically integrated.
    pub integrated_by: Option<NationId>,
    /// Player sell orders for this turn (cleared after turn resolution).
    pub player_sell_orders: Vec<crate::economy::trade::PlayerSellOrder>,
    /// Player buy orders for this turn (cleared after turn resolution).
    pub player_buy_orders: Vec<crate::economy::trade::PlayerBuyOrder>,
    /// AI scratch state for the scored-spending loop.
    pub ai_priority_state: AiPriorityState,
}

/// Historical records, telemetry, and display data.
///
/// Extracted from `Nation` (Phase 5 Lesson 8 refactor).
#[derive(Debug, Clone, Default)]
pub struct NationArchives {
    /// Records of past trade transactions for player reference.
    pub trade_history: Vec<crate::economy::trade::TradeHistoryEntry>,
    /// Cumulative per-source income totals (dollars).
    pub cash_income_totals: HashMap<CashSource, i64>,
    /// Cumulative per-sink expense totals (dollars).
    pub cash_expense_totals: HashMap<CashSink, i64>,
    /// Cumulative revenue from auto-sold materials/goods (dollars).
    pub goods_sales_revenue_dollars: i64,
    /// Adjective form of the nation name (e.g. "Devronian").
    pub adjective: String,
    /// Singular demonym ("a Devronian").
    pub demonym_singular: String,
    /// Plural demonym ("the Devronians").
    pub demonym_plural: String,
    /// Full formal title ("Empire of Devronia").
    pub government_title: String,
    /// Procedurally generated flag SVG (60×40 viewBox).
    pub flag_svg: String,
}

/// A nation in the game — either a Great Power (player-controlled or AI)
/// or a Minor Nation (AI-only, can be annexed or allied).
#[derive(Debug, Clone)]
pub struct Nation {
    pub id: NationId,
    pub name: String,
    pub color: NationColor,
    pub nation_type: NationType,
    pub province_ids: Vec<ProvinceId>,
    pub capital_province_id: ProvinceId,
    /// Inventory, labor, treasury, and buildings live here.
    pub economy: NationEconomy,
    /// Technologies that have been researched by this nation.
    pub researched_techs: Vec<TechId>,
    /// Year each tech was researched: parallel to `researched_techs`.
    pub researched_tech_years: Vec<u32>,
    /// Technology queued for research at the next end-of-turn.
    /// Set by the human player via the Tech screen; applied by the turn processor.
    pub pending_tech_research: Option<TechId>,
    /// Military forces, civilian improvers, and transport.
    pub military: NationMilitary,
    /// Diplomatic and political state.
    pub diplomacy: NationDiplomacy,
    /// Historical records, telemetry, and display data.
    pub archives: NationArchives,
}

/// Persistent per-nation AI state used by the scored-spending loop.
#[derive(Debug, Clone, Default)]
pub struct AiPriorityState {
    /// Minor-nation IDs the AI has selected as high-priority diplomacy
    /// targets (consulate + embassy). Picked once at game init based on
    /// trade complementarity with the GP's resource deficits.
    pub priority_minor_targets: Vec<NationId>,
    /// `last_invest_turn[Category]` — the absolute turn number we last
    /// picked this category in `ai_scored_spending`. Missing key means
    /// the category has never been picked; the backlog scorer treats
    /// that as "very stale" so it climbs the priority ladder fast.
    pub last_invest_turn: HashMap<crate::ai::SpendingCategory, u32>,
    /// Card #132: hard-committed depot target. While `Some(t)`, the
    /// planner must return a plan for `t.candidate` routed from
    /// `t.origin_capital` every turn until the depot is built there or
    /// the path becomes unreachable. Absence of a commitment means the
    /// planner is free to pick the best candidate this turn.
    pub committed_infra_target: Option<CommittedInfraTarget>,
}

/// A hard commitment to build a depot at `candidate`, routing rail from
/// `origin_capital`. Created by `plan_next_depot` when it selects a new
/// target, cleared by the spending loop when the planner reports
/// "reached" or "unreachable".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedInfraTarget {
    pub candidate: crate::hex::HexCoord,
    pub origin_capital: crate::hex::HexCoord,
    pub turn_committed: u32,
}

impl Nation {
    /// Create a new nation with an empty treasury and empty warehouses.
    pub fn new(
        id: NationId,
        name: String,
        color: NationColor,
        nation_type: NationType,
        capital_province_id: ProvinceId,
    ) -> Self {
        Self {
            id,
            name,
            color,
            nation_type,
            province_ids: vec![capital_province_id],
            capital_province_id,
            economy: NationEconomy::new(),
            researched_techs: Vec::new(),
            researched_tech_years: Vec::new(),
            pending_tech_research: None,
            military: NationMilitary::default(),
            diplomacy: NationDiplomacy::default(),
            archives: NationArchives::default(),
        }
    }

    /// Add a province to this nation's control.
    pub fn add_province(&mut self, province_id: ProvinceId) {
        if !self.province_ids.contains(&province_id) {
            self.province_ids.push(province_id);
        }
    }

    /// The number of provinces controlled by this nation.
    pub fn province_count(&self) -> usize {
        self.province_ids.len()
    }

    /// Add raw resources to the warehouse.
    pub fn add_resource(&mut self, resource: ResourceType, amount: u32) {
        *self.economy.warehouse.entry(resource).or_insert(0) += amount;
    }

    /// Remove raw resources from the warehouse.
    /// Returns `false` if the nation does not have enough of the resource
    /// (no resources are removed in that case).
    pub fn remove_resource(&mut self, resource: ResourceType, amount: u32) -> bool {
        let current = self.economy.warehouse.entry(resource).or_insert(0);
        if *current >= amount {
            *current -= amount;
            true
        } else {
            false
        }
    }

    /// The current amount of a raw resource in the warehouse.
    pub fn resource_amount(&self, resource: ResourceType) -> u32 {
        self.economy.warehouse.get(&resource).copied().unwrap_or(0)
    }

    /// Consume a material from the warehouse.
    /// Returns `false` if the nation does not have enough (no materials removed).
    pub fn consume_material(&mut self, material: MaterialType, amount: u32) -> bool {
        let current = self.economy.materials.entry(material).or_insert(0);
        if *current >= amount {
            *current -= amount;
            true
        } else {
            false
        }
    }

    /// Consume a finished good from the warehouse.
    /// Returns `false` if the nation does not have enough (no goods removed).
    pub fn consume_goods(&mut self, goods: GoodsType, amount: u32) -> bool {
        let current = self.economy.goods.entry(goods).or_insert(0);
        if *current >= amount {
            *current -= amount;
            true
        } else {
            false
        }
    }

    /// The current amount of a material in the warehouse.
    pub fn material_amount(&self, material: MaterialType) -> u32 {
        self.economy.materials.get(&material).copied().unwrap_or(0)
    }

    /// The current amount of a finished good in the warehouse.
    pub fn goods_amount(&self, goods: GoodsType) -> u32 {
        self.economy.goods.get(&goods).copied().unwrap_or(0)
    }

    /// Add materials to the warehouse.
    pub fn add_material(&mut self, material: MaterialType, amount: u32) {
        *self.economy.materials.entry(material).or_insert(0) += amount;
    }

    /// Add finished goods to the warehouse.
    pub fn add_goods(&mut self, goods: GoodsType, amount: u32) {
        *self.economy.goods.entry(goods).or_insert(0) += amount;
    }

    /// Returns true if the nation has enough treasury, arms, horses, oil, and
    /// labor to recruit the given unit type. Does NOT check tech prerequisites.
    pub fn can_recruit_unit(&self, unit_type: ArmyUnitType) -> bool {
        let stats = unit_type.stats();
        let labor_ok = {
            let available = match stats.recruit_tier {
                WorkerType::Untrained => self.economy.labor.untrained,
                WorkerType::Trained => self.economy.labor.trained,
                WorkerType::Expert => self.economy.labor.expert,
            };
            available >= 1
        };
        self.economy.treasury >= stats.cost
            && self.material_amount(MaterialType::Arms) >= stats.arms_required
            && (!stats.requires_horse || self.resource_amount(ResourceType::Horses) >= 1)
            && (stats.fuel_required == 0
                || self.resource_amount(ResourceType::Oil) >= stats.fuel_required)
            && labor_ok
    }

    /// Deducts all resources consumed by recruiting the given unit type:
    /// treasury, arms, horses (if required), oil (if required), and one
    /// labor unit of the appropriate tier. Caller must verify `can_recruit_unit`
    /// first; deductions saturate rather than panic.
    pub fn deduct_recruit_resources(&mut self, unit_type: ArmyUnitType) {
        let stats = unit_type.stats();
        self.economy.treasury -= stats.cost;
        self.consume_material(MaterialType::Arms, stats.arms_required);
        if stats.requires_horse {
            self.remove_resource(ResourceType::Horses, 1);
        }
        if stats.fuel_required > 0 {
            self.remove_resource(ResourceType::Oil, stats.fuel_required);
        }
        match stats.recruit_tier {
            WorkerType::Untrained => {
                self.economy.labor.untrained = self.economy.labor.untrained.saturating_sub(1);
            }
            WorkerType::Trained => {
                self.economy.labor.trained = self.economy.labor.trained.saturating_sub(1);
            }
            WorkerType::Expert => {
                self.economy.labor.expert = self.economy.labor.expert.saturating_sub(1);
            }
        }
    }

    /// Whether this nation is a Great Power.
    pub fn is_great_power(&self) -> bool {
        self.nation_type == NationType::GreatPower
    }

    /// Get a mutable reference to a building by its type.
    pub fn get_building_mut(&mut self, building_type: BuildingType) -> Option<&mut Building> {
        self.economy
            .buildings
            .iter_mut()
            .find(|b| b.building_type == building_type)
    }

    /// Check whether this nation has a building of the given type.
    pub fn has_building(&self, building_type: BuildingType) -> bool {
        self.economy
            .buildings
            .iter()
            .any(|b| b.building_type == building_type)
    }

    /// Whether this nation has researched a given technology.
    pub fn has_researched(&self, tech: TechId) -> bool {
        self.researched_techs.contains(&tech)
    }

    /// Returns all army units stationed in a given province.
    pub fn units_in_province(&self, province: ProvinceId) -> Vec<&ArmyUnit> {
        self.military
            .army
            .iter()
            .filter(|u| u.position == province)
            .collect()
    }

    /// Sum of effective_firepower() for all **projectable** army units
    /// (field army only — excludes Militia and GarrisonArtillery which are
    /// locked to their home province). Used by the coalition-strength
    /// assessment where "can I project force?" is the relevant question.
    pub fn total_military_firepower(&self) -> f64 {
        self.field_army_iter()
            .map(|u| u.effective_firepower())
            .sum()
    }

    /// Sum of effective_firepower() across every army unit, including
    /// garrison (Militia + GarrisonArtillery). Used where full defensive
    /// strength matters.
    pub fn total_firepower_including_garrison(&self) -> f64 {
        self.military
            .army
            .iter()
            .map(|u| u.effective_firepower())
            .sum()
    }

    /// Number of **field army** units (excludes stationary garrison units:
    /// Militia and GarrisonArtillery). Use this wherever the semantic is
    /// "units available to attack / move", not "raw entries in nation.military.army".
    pub fn field_army_count(&self) -> usize {
        self.military
            .army
            .iter()
            .filter(|u| u.unit_type.can_move())
            .count()
    }

    /// Iterator over field army units (movable — excludes garrison).
    pub fn field_army_iter(&self) -> impl Iterator<Item = &ArmyUnit> + '_ {
        self.military.army.iter().filter(|u| u.unit_type.can_move())
    }

    /// Count of Militia units stationed at a given province.
    pub fn militia_at(&self, province: ProvinceId) -> usize {
        self.military
            .army
            .iter()
            .filter(|u| {
                u.position == province
                    && u.unit_type == crate::military::units::ArmyUnitType::Minutemen
            })
            .count()
    }

    /// Whether the given province has this nation's GarrisonArtillery unit.
    pub fn has_garrison_artillery_at(&self, province: ProvinceId) -> bool {
        self.military.army.iter().any(|u| {
            u.position == province
                && u.unit_type == crate::military::units::ArmyUnitType::GarrisonArtillery
        })
    }

    /// Total cargo capacity of all merchant ships in the fleet.
    pub fn total_cargo_capacity(&self, data: &crate::data::GameData) -> u32 {
        self.military
            .merchant_fleet
            .iter()
            .map(|s| data.ship_stats(s.ship_type).cargo)
            .sum()
    }

    /// Number of merchant ships in the fleet.
    pub fn merchant_ship_count(&self) -> usize {
        self.military.merchant_fleet.len()
    }

    /// Sum of firepower for all warships in the fleet.
    pub fn total_naval_firepower(&self, data: &crate::data::GameData) -> u32 {
        self.military
            .warships
            .iter()
            .map(|s| data.ship_stats(s.ship_type).firepower)
            .sum()
    }

    /// Number of warships in the fleet.
    pub fn warship_count(&self) -> usize {
        self.military.warships.len()
    }

    /// Add a technology to this nation's researched list, recording the year.
    pub fn research_tech(&mut self, tech: TechId) {
        self.research_tech_in_year(tech, 0);
    }

    /// Add a technology to this nation's researched list with the given game year.
    pub fn research_tech_in_year(&mut self, tech: TechId, year: u32) {
        if !self.researched_techs.contains(&tech) {
            self.researched_techs.push(tech);
            self.researched_tech_years.push(year);
        }
    }

    /// Returns true if the nation's treasury is negative (in debt).
    pub fn is_bankrupt(&self) -> bool {
        self.economy.treasury < Money::ZERO
    }

    /// Whether this nation is in anarchy (lost its capital).
    pub fn is_in_anarchy(&self) -> bool {
        self.diplomacy.is_in_anarchy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a sample Great Power nation for testing.
    fn sample_great_power() -> Nation {
        Nation::new(
            NationId(1),
            "France".to_string(),
            NationColor::Blue,
            NationType::GreatPower,
            ProvinceId(10),
        )
    }

    /// Helper: build a sample Minor Nation for testing.
    fn sample_minor_nation() -> Nation {
        Nation::new(
            NationId(8),
            "Bavaria".to_string(),
            NationColor::Gray,
            NationType::MinorNation,
            ProvinceId(20),
        )
    }

    // ── Construction ───────────────────────────────────────────

    #[test]
    fn new_nation_has_correct_fields() {
        let n = sample_great_power();
        assert_eq!(n.id, NationId(1));
        assert_eq!(n.name, "France");
        assert_eq!(n.color, NationColor::Blue);
        assert_eq!(n.nation_type, NationType::GreatPower);
        assert_eq!(n.capital_province_id, ProvinceId(10));
    }

    #[test]
    fn new_nation_starts_with_zero_treasury() {
        let n = sample_great_power();
        assert_eq!(n.economy.treasury, Money::ZERO);
    }

    #[test]
    fn new_nation_has_capital_in_provinces() {
        let n = sample_great_power();
        assert!(n.province_ids.contains(&ProvinceId(10)));
        assert_eq!(n.province_count(), 1);
    }

    #[test]
    fn new_nation_has_empty_warehouses() {
        let n = sample_great_power();
        assert!(n.economy.warehouse.is_empty());
        assert!(n.economy.materials.is_empty());
        assert!(n.economy.goods.is_empty());
    }

    // ── is_great_power ────────────────────────────────────────

    #[test]
    fn great_power_returns_true() {
        let n = sample_great_power();
        assert!(n.is_great_power());
    }

    #[test]
    fn minor_nation_returns_false() {
        let n = sample_minor_nation();
        assert!(!n.is_great_power());
    }

    // ── Province management ───────────────────────────────────

    #[test]
    fn add_province_increases_count() {
        let mut n = sample_great_power();
        assert_eq!(n.province_count(), 1);
        n.add_province(ProvinceId(11));
        assert_eq!(n.province_count(), 2);
        n.add_province(ProvinceId(12));
        assert_eq!(n.province_count(), 3);
    }

    #[test]
    fn add_duplicate_province_does_not_increase_count() {
        let mut n = sample_great_power();
        n.add_province(ProvinceId(11));
        assert_eq!(n.province_count(), 2);
        n.add_province(ProvinceId(11));
        assert_eq!(n.province_count(), 2);
    }

    #[test]
    fn add_capital_province_again_does_not_duplicate() {
        let mut n = sample_great_power();
        n.add_province(ProvinceId(10)); // capital already present
        assert_eq!(n.province_count(), 1);
    }

    // ── Resource management ───────────────────────────────────

    #[test]
    fn add_resource_stores_amount() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Timber, 5);
        assert_eq!(n.resource_amount(ResourceType::Timber), 5);
    }

    #[test]
    fn add_resource_accumulates() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Iron, 3);
        n.add_resource(ResourceType::Iron, 7);
        assert_eq!(n.resource_amount(ResourceType::Iron), 10);
    }

    #[test]
    fn resource_amount_defaults_to_zero() {
        let n = sample_great_power();
        assert_eq!(n.resource_amount(ResourceType::Coal), 0);
    }

    #[test]
    fn remove_resource_sufficient() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Cotton, 10);
        let result = n.remove_resource(ResourceType::Cotton, 4);
        assert!(result);
        assert_eq!(n.resource_amount(ResourceType::Cotton), 6);
    }

    #[test]
    fn remove_resource_exact_amount() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Grain, 5);
        let result = n.remove_resource(ResourceType::Grain, 5);
        assert!(result);
        assert_eq!(n.resource_amount(ResourceType::Grain), 0);
    }

    #[test]
    fn remove_resource_insufficient() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Gold, 3);
        let result = n.remove_resource(ResourceType::Gold, 5);
        assert!(!result);
        // Amount should remain unchanged
        assert_eq!(n.resource_amount(ResourceType::Gold), 3);
    }

    #[test]
    fn remove_resource_not_present() {
        let mut n = sample_great_power();
        let result = n.remove_resource(ResourceType::Oil, 1);
        assert!(!result);
    }

    #[test]
    fn multiple_resource_types_independent() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Timber, 10);
        n.add_resource(ResourceType::Coal, 20);
        n.add_resource(ResourceType::Iron, 15);

        assert_eq!(n.resource_amount(ResourceType::Timber), 10);
        assert_eq!(n.resource_amount(ResourceType::Coal), 20);
        assert_eq!(n.resource_amount(ResourceType::Iron), 15);

        n.remove_resource(ResourceType::Coal, 5);
        assert_eq!(n.resource_amount(ResourceType::Coal), 15);
        // Others unchanged
        assert_eq!(n.resource_amount(ResourceType::Timber), 10);
        assert_eq!(n.resource_amount(ResourceType::Iron), 15);
    }

    // ── Tech research ─────────────────────────────────────────

    #[test]
    fn new_nation_has_no_researched_techs() {
        let n = sample_great_power();
        assert!(n.researched_techs.is_empty());
    }

    #[test]
    fn has_researched_returns_false_when_empty() {
        let n = sample_great_power();
        assert!(!n.has_researched(TechId(1)));
    }

    #[test]
    fn research_tech_adds_to_list() {
        let mut n = sample_great_power();
        n.research_tech(TechId(5));
        assert!(n.has_researched(TechId(5)));
        assert_eq!(n.researched_techs.len(), 1);
    }

    #[test]
    fn research_tech_does_not_duplicate() {
        let mut n = sample_great_power();
        n.research_tech(TechId(3));
        n.research_tech(TechId(3));
        assert_eq!(n.researched_techs.len(), 1);
    }

    #[test]
    fn research_multiple_techs() {
        let mut n = sample_great_power();
        n.research_tech(TechId(1));
        n.research_tech(TechId(2));
        n.research_tech(TechId(3));
        assert!(n.has_researched(TechId(1)));
        assert!(n.has_researched(TechId(2)));
        assert!(n.has_researched(TechId(3)));
        assert!(!n.has_researched(TechId(4)));
        assert_eq!(n.researched_techs.len(), 3);
    }

    // ── Material management ──────────────────────────────────

    #[test]
    fn add_material_stores_amount() {
        let mut n = sample_great_power();
        n.add_material(MaterialType::Lumber, 5);
        assert_eq!(n.material_amount(MaterialType::Lumber), 5);
    }

    #[test]
    fn add_material_accumulates() {
        let mut n = sample_great_power();
        n.add_material(MaterialType::Steel, 3);
        n.add_material(MaterialType::Steel, 7);
        assert_eq!(n.material_amount(MaterialType::Steel), 10);
    }

    #[test]
    fn material_amount_defaults_to_zero() {
        let n = sample_great_power();
        assert_eq!(n.material_amount(MaterialType::Fabric), 0);
    }

    #[test]
    fn consume_material_sufficient() {
        let mut n = sample_great_power();
        n.add_material(MaterialType::Lumber, 10);
        assert!(n.consume_material(MaterialType::Lumber, 4));
        assert_eq!(n.material_amount(MaterialType::Lumber), 6);
    }

    #[test]
    fn consume_material_insufficient() {
        let mut n = sample_great_power();
        n.add_material(MaterialType::Steel, 3);
        assert!(!n.consume_material(MaterialType::Steel, 5));
        assert_eq!(n.material_amount(MaterialType::Steel), 3);
    }

    // ── Goods management ─────────────────────────────────────

    #[test]
    fn add_goods_stores_amount() {
        let mut n = sample_great_power();
        n.add_goods(GoodsType::Furniture, 2);
        assert_eq!(n.goods_amount(GoodsType::Furniture), 2);
    }

    #[test]
    fn add_goods_accumulates() {
        let mut n = sample_great_power();
        n.add_goods(GoodsType::Clothing, 3);
        n.add_goods(GoodsType::Clothing, 4);
        assert_eq!(n.goods_amount(GoodsType::Clothing), 7);
    }

    #[test]
    fn goods_amount_defaults_to_zero() {
        let n = sample_great_power();
        assert_eq!(n.goods_amount(GoodsType::Hardware), 0);
    }

    #[test]
    fn consume_goods_sufficient() {
        let mut n = sample_great_power();
        n.add_goods(GoodsType::Furniture, 5);
        assert!(n.consume_goods(GoodsType::Furniture, 3));
        assert_eq!(n.goods_amount(GoodsType::Furniture), 2);
    }

    #[test]
    fn consume_goods_insufficient() {
        let mut n = sample_great_power();
        n.add_goods(GoodsType::Clothing, 2);
        assert!(!n.consume_goods(GoodsType::Clothing, 5));
        assert_eq!(n.goods_amount(GoodsType::Clothing), 2);
    }

    // ── Province count after adding/removing ─────────────────

    #[test]
    fn province_count_tracks_additions() {
        let mut n = sample_great_power();
        assert_eq!(n.province_count(), 1); // starts with capital
        n.add_province(ProvinceId(11));
        n.add_province(ProvinceId(12));
        n.add_province(ProvinceId(13));
        assert_eq!(n.province_count(), 4);
    }

    #[test]
    fn province_ids_contains_all_added() {
        let mut n = sample_great_power();
        n.add_province(ProvinceId(11));
        n.add_province(ProvinceId(12));
        assert!(n.province_ids.contains(&ProvinceId(10))); // capital
        assert!(n.province_ids.contains(&ProvinceId(11)));
        assert!(n.province_ids.contains(&ProvinceId(12)));
    }

    // ── Warehouse operations don't go negative ──────────────

    #[test]
    fn remove_resource_does_not_go_negative() {
        let mut n = sample_great_power();
        n.add_resource(ResourceType::Timber, 3);
        // Try to remove more than available
        let result = n.remove_resource(ResourceType::Timber, 10);
        assert!(!result);
        assert_eq!(n.resource_amount(ResourceType::Timber), 3);
    }

    #[test]
    fn consume_material_does_not_go_negative() {
        let mut n = sample_great_power();
        n.add_material(MaterialType::Steel, 2);
        let result = n.consume_material(MaterialType::Steel, 5);
        assert!(!result);
        assert_eq!(n.material_amount(MaterialType::Steel), 2);
    }

    #[test]
    fn consume_goods_does_not_go_negative() {
        let mut n = sample_great_power();
        n.add_goods(GoodsType::Furniture, 1);
        let result = n.consume_goods(GoodsType::Furniture, 3);
        assert!(!result);
        assert_eq!(n.goods_amount(GoodsType::Furniture), 1);
    }

    #[test]
    fn remove_resource_from_empty_warehouse() {
        let mut n = sample_great_power();
        let result = n.remove_resource(ResourceType::Coal, 1);
        assert!(!result);
        assert_eq!(n.resource_amount(ResourceType::Coal), 0);
    }

    // ── Military firepower calculation with medals ──────────

    #[test]
    fn total_military_firepower_with_no_units() {
        let n = sample_great_power();
        assert!((n.total_military_firepower() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn total_military_firepower_with_medals() {
        use crate::map::UnitId;
        use crate::military::units::{ArmyUnit, ArmyUnitType};

        let mut n = sample_great_power();

        // Add a Regulars unit with 2 medals
        let mut unit1 = ArmyUnit::new(
            UnitId(1),
            ArmyUnitType::Regulars,
            NationId(1),
            ProvinceId(10),
        );
        unit1.award_medal();
        unit1.award_medal();
        // Regulars FPN = 10 (per manual), 2 medals = 1.5× => 15.0
        n.military.army.push(unit1);

        // Add a Guards unit with 0 medals.
        let unit2 = ArmyUnit::new(UnitId(2), ArmyUnitType::Guards, NationId(1), ProvinceId(10));
        // Guards FPN = 17 (per manual), 0 medals = 1.0× => 17.0
        n.military.army.push(unit2);

        // Total: 15.0 + 17.0 = 32.0
        assert!((n.total_military_firepower() - 32.0).abs() < f64::EPSILON);
    }

    // ── Building ownership checks ────────────────────────────

    #[test]
    fn has_building_after_adding() {
        let mut n = sample_great_power();
        n.economy
            .buildings
            .push(Building::new(BuildingType::LumberMill, 2));
        assert!(n.has_building(BuildingType::LumberMill));
        assert!(!n.has_building(BuildingType::SteelMill));
    }

    #[test]
    fn get_building_mut_modifies_capacity() {
        let mut n = sample_great_power();
        n.economy
            .buildings
            .push(Building::new(BuildingType::SteelMill, 1));
        let mill = n.get_building_mut(BuildingType::SteelMill).unwrap();
        mill.start_expansion(3);
        assert!(
            n.economy
                .buildings
                .iter()
                .any(|b| b.building_type == BuildingType::SteelMill && b.pending_capacity == 3)
        );
    }

    // ── Trade history ────────────────────────────────────────────

    #[test]
    fn new_nation_has_empty_trade_history() {
        let n = sample_great_power();
        assert!(n.archives.trade_history.is_empty());
    }

    #[test]
    fn trade_history_can_be_appended() {
        use crate::economy::trade::TradeHistoryEntry;

        let mut n = sample_great_power();
        n.archives.trade_history.push(TradeHistoryEntry {
            turn: TurnNumber::new(3),
            partner: NationId(10),
            resource: ResourceType::Timber,
            commodity_label: "Timber".to_string(),
            quantity: 5,
            total_cost: Money::dollars(250),
            bought: true,
        });
        assert_eq!(n.archives.trade_history.len(), 1);
        assert_eq!(n.archives.trade_history[0].partner, NationId(10));
        assert_eq!(n.archives.trade_history[0].quantity, 5);
    }

    #[test]
    fn trade_history_stores_all_fields() {
        use crate::economy::trade::TradeHistoryEntry;

        let mut n = sample_great_power();
        n.archives.trade_history.push(TradeHistoryEntry {
            turn: TurnNumber::new(7),
            partner: NationId(5),
            resource: ResourceType::Iron,
            commodity_label: "Iron".to_string(),
            quantity: 3,
            total_cost: Money::dollars(225),
            bought: true,
        });

        let cloned = n.clone();
        assert_eq!(cloned.archives.trade_history.len(), 1);
        assert_eq!(cloned.archives.trade_history[0].turn, TurnNumber::new(7));
        assert_eq!(cloned.archives.trade_history[0].partner, NationId(5));
    }

    // ── Bankruptcy ──────────────────────────────────────────────

    #[test]
    fn is_bankrupt_false_at_zero() {
        let n = sample_great_power();
        assert!(!n.is_bankrupt());
    }

    #[test]
    fn is_bankrupt_false_when_positive() {
        let mut n = sample_great_power();
        n.economy.treasury = Money::dollars(1000);
        assert!(!n.is_bankrupt());
    }

    #[test]
    fn is_bankrupt_true_when_negative() {
        let mut n = sample_great_power();
        n.economy.treasury = Money::dollars(-1);
        assert!(n.is_bankrupt());
    }

    // ── NationEconomy unified commodity API (#160) ──────────────────

    #[test]
    fn economy_amount_returns_zero_when_empty() {
        let n = sample_great_power();
        assert_eq!(
            n.economy.amount(Commodity::Resource(ResourceType::Timber)),
            0
        );
        assert_eq!(
            n.economy.amount(Commodity::Material(MaterialType::Lumber)),
            0
        );
        assert_eq!(n.economy.amount(Commodity::Goods(GoodsType::Furniture)), 0);
    }

    #[test]
    fn economy_add_and_amount_round_trip() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Resource(ResourceType::Coal), 10);
        n.economy.add(Commodity::Material(MaterialType::Steel), 5);
        n.economy.add(Commodity::Goods(GoodsType::Clothing), 3);
        assert_eq!(
            n.economy.amount(Commodity::Resource(ResourceType::Coal)),
            10
        );
        assert_eq!(
            n.economy.amount(Commodity::Material(MaterialType::Steel)),
            5
        );
        assert_eq!(n.economy.amount(Commodity::Goods(GoodsType::Clothing)), 3);
    }

    #[test]
    fn economy_consume_returns_true_when_sufficient() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Resource(ResourceType::Iron), 8);
        assert!(
            n.economy
                .consume(Commodity::Resource(ResourceType::Iron), 5)
        );
        assert_eq!(n.economy.amount(Commodity::Resource(ResourceType::Iron)), 3);
    }

    #[test]
    fn economy_consume_returns_false_when_insufficient() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Material(MaterialType::Arms), 2);
        assert!(
            !n.economy
                .consume(Commodity::Material(MaterialType::Arms), 5)
        );
        assert_eq!(n.economy.amount(Commodity::Material(MaterialType::Arms)), 2);
    }

    #[test]
    fn economy_iter_all_yields_all_nonempty_commodities() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Resource(ResourceType::Grain), 7);
        n.economy.add(Commodity::Material(MaterialType::Fabric), 4);
        n.economy.add(Commodity::Goods(GoodsType::Hardware), 1);
        let all: Vec<(Commodity, u32)> = n.economy.iter_all().collect();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&(Commodity::Resource(ResourceType::Grain), 7)));
        assert!(all.contains(&(Commodity::Material(MaterialType::Fabric), 4)));
        assert!(all.contains(&(Commodity::Goods(GoodsType::Hardware), 1)));
    }

    // ── Reservation API (#162) ──────────────────────────────────────

    #[test]
    fn reserve_reduces_available_not_total() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Resource(ResourceType::Timber), 10);
        let id = n
            .economy
            .reserve(Commodity::Resource(ResourceType::Timber), 4)
            .unwrap();
        assert_eq!(
            n.economy.amount(Commodity::Resource(ResourceType::Timber)),
            10
        );
        assert_eq!(
            n.economy
                .reserved(Commodity::Resource(ResourceType::Timber)),
            4
        );
        assert_eq!(
            n.economy
                .available(Commodity::Resource(ResourceType::Timber)),
            6
        );
        let _ = n.economy.release(id);
    }

    #[test]
    fn reserve_fails_when_insufficient_available() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Material(MaterialType::Lumber), 3);
        let result = n
            .economy
            .reserve(Commodity::Material(MaterialType::Lumber), 5);
        assert!(matches!(
            result,
            Err(crate::DomainError::InsufficientInventory { .. })
        ));
        assert_eq!(
            n.economy.amount(Commodity::Material(MaterialType::Lumber)),
            3
        );
    }

    #[test]
    fn commit_deducts_from_total_and_clears_reservation() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Goods(GoodsType::Furniture), 6);
        let id = n
            .economy
            .reserve(Commodity::Goods(GoodsType::Furniture), 2)
            .unwrap();
        n.economy.commit(id).unwrap();
        assert_eq!(n.economy.amount(Commodity::Goods(GoodsType::Furniture)), 4);
        assert_eq!(
            n.economy.reserved(Commodity::Goods(GoodsType::Furniture)),
            0
        );
        assert!(n.economy.reservation_ledger.is_empty());
    }

    #[test]
    fn release_restores_available_without_consuming() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Resource(ResourceType::Gold), 5);
        let id = n
            .economy
            .reserve(Commodity::Resource(ResourceType::Gold), 3)
            .unwrap();
        n.economy.release(id).unwrap();
        assert_eq!(n.economy.amount(Commodity::Resource(ResourceType::Gold)), 5);
        assert_eq!(
            n.economy.reserved(Commodity::Resource(ResourceType::Gold)),
            0
        );
    }

    #[test]
    fn release_nonexistent_reservation_returns_error() {
        let mut n = sample_great_power();
        let fake_id = ReservationId(9999);
        let result = n.economy.release(fake_id);
        assert!(matches!(
            result,
            Err(crate::DomainError::ReservationNotFound(_))
        ));
    }

    #[test]
    fn release_all_reservations_clears_all() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Resource(ResourceType::Coal), 10);
        n.economy.add(Commodity::Material(MaterialType::Steel), 8);
        n.economy
            .reserve(Commodity::Resource(ResourceType::Coal), 5)
            .unwrap();
        n.economy
            .reserve(Commodity::Material(MaterialType::Steel), 3)
            .unwrap();
        n.economy.release_all_reservations();
        assert!(n.economy.reservation_ledger.is_empty());
        assert_eq!(
            n.economy.available(Commodity::Resource(ResourceType::Coal)),
            10
        );
        assert_eq!(
            n.economy
                .available(Commodity::Material(MaterialType::Steel)),
            8
        );
    }

    #[test]
    fn reservation_ledger_tracks_quantity() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Resource(ResourceType::Iron), 12);
        let id = n
            .economy
            .reserve(Commodity::Resource(ResourceType::Iron), 4)
            .unwrap();
        assert_eq!(
            n.economy.reservation_ledger.get(&id).map(|(_, q)| *q),
            Some(4)
        );
        assert_eq!(
            n.economy.reserved(Commodity::Resource(ResourceType::Iron)),
            4
        );
        let _ = n.economy.release(id);
    }

    #[test]
    fn reserved_leq_total_invariant() {
        let mut n = sample_great_power();
        n.economy.add(Commodity::Resource(ResourceType::Wool), 7);
        n.economy
            .reserve(Commodity::Resource(ResourceType::Wool), 4)
            .unwrap();
        assert!(
            n.economy.reserved(Commodity::Resource(ResourceType::Wool))
                <= n.economy.amount(Commodity::Resource(ResourceType::Wool))
        );
    }

    #[test]
    fn commit_returns_error_if_inventory_externally_depleted() {
        // F-009 regression: commit must fail if stock was depleted after reservation.
        let mut n = sample_great_power();
        n.economy.add(Commodity::Material(MaterialType::Lumber), 10);
        let id = n
            .economy
            .reserve(Commodity::Material(MaterialType::Lumber), 8)
            .unwrap();

        // External depletion via legacy mutator bypasses reservation invariants.
        n.consume_material(MaterialType::Lumber, 5);

        // Inventory is now 5, but we reserved 8 — commit must fail.
        let result = n.economy.commit(id);
        assert!(
            matches!(
                result,
                Err(crate::DomainError::InsufficientInventory { .. })
            ),
            "expected InsufficientInventory, got {:?}",
            result
        );
        // Reservation must not have been removed from the ledger.
        assert!(
            n.economy.reservation_ledger.contains_key(&id),
            "reservation should remain in ledger after failed commit"
        );
    }

    // ── Recruit gating ────────────────────────────────────────

    fn recruit_ready_nation() -> Nation {
        let mut n = sample_great_power();
        n.economy.treasury = Money::dollars(200);
        n.add_material(MaterialType::Arms, 2);
        n.economy.labor.untrained = 2;
        n.economy.labor.trained = 2;
        n.economy.labor.expert = 2;
        n.add_resource(ResourceType::Horses, 2);
        n.add_resource(ResourceType::Oil, 2);
        n
    }

    #[test]
    fn can_recruit_unit_returns_true_when_all_resources_available() {
        let n = recruit_ready_nation();
        assert!(n.can_recruit_unit(ArmyUnitType::Regulars));
    }

    #[test]
    fn can_recruit_unit_fails_when_treasury_insufficient() {
        let mut n = recruit_ready_nation();
        n.economy.treasury = Money::ZERO;
        assert!(!n.can_recruit_unit(ArmyUnitType::Regulars));
    }

    #[test]
    fn can_recruit_unit_fails_when_arms_insufficient() {
        let mut n = recruit_ready_nation();
        n.economy.materials.clear();
        assert!(!n.can_recruit_unit(ArmyUnitType::Regulars));
    }

    #[test]
    fn can_recruit_unit_fails_when_untrained_labor_exhausted() {
        let mut n = recruit_ready_nation();
        n.economy.labor.untrained = 0;
        assert!(!n.can_recruit_unit(ArmyUnitType::Regulars));
    }

    #[test]
    fn can_recruit_unit_fails_when_trained_labor_exhausted() {
        let mut n = recruit_ready_nation();
        n.economy.treasury = Money::dollars(10_000);
        n.add_material(MaterialType::Arms, 10);
        n.economy.labor.trained = 0;
        assert!(!n.can_recruit_unit(ArmyUnitType::RifleInfantry));
    }

    #[test]
    fn can_recruit_unit_fails_when_horse_unavailable() {
        let mut n = recruit_ready_nation();
        n.economy.warehouse.clear();
        assert!(!n.can_recruit_unit(ArmyUnitType::Hussars));
    }

    #[test]
    fn can_recruit_unit_fails_when_fuel_unavailable() {
        let mut n = recruit_ready_nation();
        n.economy.treasury = Money::dollars(10_000);
        n.add_material(MaterialType::Arms, 10);
        n.economy.labor.expert = 5;
        n.economy.warehouse.clear();
        assert!(!n.can_recruit_unit(ArmyUnitType::Mechanised));
    }

    #[test]
    fn failed_can_recruit_does_not_mutate_state() {
        let mut n = recruit_ready_nation();
        n.economy.labor.untrained = 0;
        let treasury_before = n.economy.treasury;
        let arms_before = n.material_amount(MaterialType::Arms);
        assert!(!n.can_recruit_unit(ArmyUnitType::Regulars));
        assert_eq!(n.economy.treasury, treasury_before);
        assert_eq!(n.material_amount(MaterialType::Arms), arms_before);
    }

    #[test]
    fn deduct_recruit_resources_consumes_treasury_arms_and_labor() {
        let mut n = recruit_ready_nation();
        let treasury_before = n.economy.treasury;
        let stats = ArmyUnitType::Regulars.stats();
        n.deduct_recruit_resources(ArmyUnitType::Regulars);
        assert_eq!(n.economy.treasury, treasury_before - stats.cost);
        assert_eq!(
            n.material_amount(MaterialType::Arms),
            2 - stats.arms_required
        );
        assert_eq!(n.economy.labor.untrained, 1);
    }

    #[test]
    fn deduct_recruit_resources_consumes_horse_when_required() {
        let mut n = recruit_ready_nation();
        n.economy.treasury = Money::dollars(10_000);
        n.add_material(MaterialType::Arms, 5);
        let horses_before = n.resource_amount(ResourceType::Horses);
        n.deduct_recruit_resources(ArmyUnitType::Hussars);
        assert_eq!(n.resource_amount(ResourceType::Horses), horses_before - 1);
    }
}
