#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WorkerType {
    Untrained,
    Trained,
    Expert,
}

/// Per-turn penalty applied to a tier (famine, plague, unrest, etc.).
/// Reduces effective labor output but doesn't remove workers from the pool.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TemporaryPenalty {
    /// Fraction of the tier's labor that is suppressed (0.0–1.0).
    pub fraction: f32,
    /// Turn on which the penalty expires.
    pub expires: crate::types::TurnNumber,
}

/// Rich per-tier state attached to a `LaborPool`.
///
/// `healthy + sick` equals the tier's worker count in `LaborPool`.
/// Sick workers consume food but do not contribute labor units.
/// Fields are `Option`-gated so they default to zero / absent and add no
/// allocation overhead until a feature actually populates them.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TierState {
    /// Workers currently able to work.
    pub healthy: u32,
    /// Workers incapacitated (famine, plague). They still eat.
    pub sick: u32,
    /// If set, this tier has workers queued to advance: `(target_tier, completion_turn)`.
    pub training_to: Option<(WorkerType, crate::types::TurnNumber)>,
    /// Nation of origin for the most recent immigration wave into this tier.
    pub recent_origin: Option<crate::types::NationId>,
    /// Active temporary penalty on this tier's output.
    pub temporary_penalty: Option<TemporaryPenalty>,
}

impl TierState {
    /// Total workers in this tier (healthy + sick).
    pub fn total(&self) -> u32 {
        self.healthy + self.sick
    }

    /// Effective labor contribution (sick workers contribute nothing).
    pub fn effective_workers(&self) -> u32 {
        self.healthy
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LaborPool {
    /// Healthy-and-active count for each tier.
    /// These are the authoritative worker counts used by all production logic.
    pub untrained: u32,
    pub trained: u32,
    pub expert: u32,
    /// Optional rich per-tier metadata. Absent entries are implicitly all-healthy
    /// with no training queue or penalties.
    #[serde(default)]
    pub tier_meta: std::collections::HashMap<WorkerType, TierState>,
}

impl LaborPool {
    /// Create a new empty labor pool.
    pub fn new() -> Self {
        Self {
            untrained: 0,
            trained: 0,
            expert: 0,
            tier_meta: std::collections::HashMap::new(),
        }
    }

    /// Return a view of the rich per-tier state for a given tier, or a default
    /// all-healthy state if no metadata has been recorded yet.
    pub fn tier_state(&self, tier: WorkerType) -> std::borrow::Cow<'_, TierState> {
        match self.tier_meta.get(&tier) {
            Some(s) => std::borrow::Cow::Borrowed(s),
            None => std::borrow::Cow::Owned(TierState {
                healthy: match tier {
                    WorkerType::Untrained => self.untrained,
                    WorkerType::Trained => self.trained,
                    WorkerType::Expert => self.expert,
                },
                ..TierState::default()
            }),
        }
    }

    /// Mutable access to rich tier state, creating an entry if absent.
    pub fn tier_state_mut(&mut self, tier: WorkerType) -> &mut TierState {
        let healthy = match tier {
            WorkerType::Untrained => self.untrained,
            WorkerType::Trained => self.trained,
            WorkerType::Expert => self.expert,
        };
        self.tier_meta.entry(tier).or_insert_with(|| TierState {
            healthy,
            ..TierState::default()
        })
    }

    /// Effective worker count for a tier: healthy workers only (sick do not produce).
    /// Falls back to the flat count when no rich state is recorded.
    pub fn effective_workers(&self, tier: WorkerType) -> u32 {
        self.tier_meta
            .get(&tier)
            .map(|s| s.healthy)
            .unwrap_or_else(|| match tier {
                WorkerType::Untrained => self.untrained,
                WorkerType::Trained => self.trained,
                WorkerType::Expert => self.expert,
            })
    }

    /// Total number of workers in the pool.
    pub fn total_workers(&self) -> u32 {
        self.untrained + self.trained + self.expert
    }

    /// Total labor units available using default multipliers (untrained=1, trained=2, expert=4).
    pub fn total_labor_units(&self) -> u32 {
        self.untrained + self.trained * 2 + self.expert * 4
    }

    /// Total labor units with custom multipliers from game config.
    pub fn total_labor_units_with(
        &self,
        untrained_mult: u32,
        trained_mult: u32,
        expert_mult: u32,
    ) -> u32 {
        self.untrained * untrained_mult + self.trained * trained_mult + self.expert * expert_mult
    }

    /// Train one untrained worker, converting them to trained.
    /// Returns `true` if successful, `false` if no untrained workers are available.
    /// Note: the paper cost check is external to this method.
    pub fn train_worker(&mut self) -> bool {
        if self.untrained > 0 {
            self.untrained -= 1;
            self.trained += 1;
            self.sync_tier_healthy(WorkerType::Untrained);
            self.sync_tier_healthy(WorkerType::Trained);
            true
        } else {
            false
        }
    }

    /// Promote one trained worker to expert.
    /// Returns `true` if successful, `false` if no trained workers are available.
    pub fn promote_worker(&mut self) -> bool {
        if self.trained > 0 {
            self.trained -= 1;
            self.expert += 1;
            self.sync_tier_healthy(WorkerType::Trained);
            self.sync_tier_healthy(WorkerType::Expert);
            true
        } else {
            false
        }
    }

    /// Add one untrained immigrant worker to the pool.
    pub fn recruit_immigrant(&mut self) {
        self.untrained += 1;
        self.sync_tier_healthy(WorkerType::Untrained);
    }

    /// Remove one worker due to starvation or other attrition.
    /// Removes untrained first, then trained, then expert.
    /// Returns `true` if a worker was removed, `false` if pool is empty.
    pub fn remove_worker(&mut self) -> bool {
        if self.untrained > 0 {
            self.untrained -= 1;
            self.sync_tier_healthy(WorkerType::Untrained);
            true
        } else if self.trained > 0 {
            self.trained -= 1;
            self.sync_tier_healthy(WorkerType::Trained);
            true
        } else if self.expert > 0 {
            self.expert -= 1;
            self.sync_tier_healthy(WorkerType::Expert);
            true
        } else {
            false
        }
    }

    /// Keep `tier_meta[tier].healthy` in sync with the authoritative flat count.
    /// Only updates if an entry already exists (no-op if absent — lazy init is fine).
    /// Enforces `healthy + sick == flat` by clamping sick down first if needed,
    /// then setting healthy to the remainder. This preserves the invariant even
    /// when attrition reduces flat below the previously-tracked sick count.
    fn sync_tier_healthy(&mut self, tier: WorkerType) {
        let flat = match tier {
            WorkerType::Untrained => self.untrained,
            WorkerType::Trained => self.trained,
            WorkerType::Expert => self.expert,
        };
        if let Some(meta) = self.tier_meta.get_mut(&tier) {
            meta.sick = meta.sick.min(flat);
            meta.healthy = flat - meta.sick;
        }
    }
}

impl Default for LaborPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ──────────────────────────────────────────────

    #[test]
    fn new_pool_is_empty() {
        let pool = LaborPool::new();
        assert_eq!(pool.untrained, 0);
        assert_eq!(pool.trained, 0);
        assert_eq!(pool.expert, 0);
        assert_eq!(pool.total_workers(), 0);
    }

    #[test]
    fn default_pool_is_empty() {
        let pool = LaborPool::default();
        assert_eq!(pool.total_workers(), 0);
    }

    // ── total_workers / total_labor_units ──────────────────────────

    #[test]
    fn total_workers_counts_all_types() {
        let pool = LaborPool { untrained: 3, trained: 2, expert: 1, ..LaborPool::new() };
        assert_eq!(pool.total_workers(), 6);
    }

    #[test]
    fn total_labor_units_uses_training_multipliers() {
        let pool = LaborPool { untrained: 5, trained: 3, expert: 2, ..LaborPool::new() };
        // 5*1 + 3*2 + 2*4 = 5 + 6 + 8 = 19
        assert_eq!(pool.total_labor_units(), 19);
        assert_eq!(pool.total_workers(), 10);
    }

    #[test]
    fn total_labor_units_with_custom_multipliers() {
        let pool = LaborPool { untrained: 4, trained: 2, expert: 1, ..LaborPool::new() };
        // Custom: 4*1 + 2*3 + 1*5 = 4 + 6 + 5 = 15
        assert_eq!(pool.total_labor_units_with(1, 3, 5), 15);
        // Default: 4*1 + 2*2 + 1*4 = 4 + 4 + 4 = 12
        assert_eq!(pool.total_labor_units(), 12);
    }

    // ── recruit_immigrant ─────────────────────────────────────────

    #[test]
    fn recruit_immigrant_adds_untrained() {
        let mut pool = LaborPool::new();
        pool.recruit_immigrant();
        assert_eq!(pool.untrained, 1);
        assert_eq!(pool.total_workers(), 1);
    }

    #[test]
    fn recruit_multiple_immigrants() {
        let mut pool = LaborPool::new();
        pool.recruit_immigrant();
        pool.recruit_immigrant();
        pool.recruit_immigrant();
        assert_eq!(pool.untrained, 3);
    }

    // ── train_worker ──────────────────────────────────────────────

    #[test]
    fn train_worker_converts_untrained_to_trained() {
        let mut pool = LaborPool::new();
        pool.recruit_immigrant();
        pool.recruit_immigrant();

        let result = pool.train_worker();
        assert!(result);
        assert_eq!(pool.untrained, 1);
        assert_eq!(pool.trained, 1);
        assert_eq!(pool.total_workers(), 2); // total unchanged
    }

    #[test]
    fn train_worker_fails_when_no_untrained() {
        let mut pool = LaborPool::new();
        let result = pool.train_worker();
        assert!(!result);
        assert_eq!(pool.trained, 0);
    }

    #[test]
    fn train_worker_fails_when_only_trained_and_expert() {
        let mut pool = LaborPool { untrained: 0, trained: 5, expert: 3, ..LaborPool::new() };
        let result = pool.train_worker();
        assert!(!result);
        assert_eq!(pool.trained, 5); // unchanged
    }

    // ── promote_worker ────────────────────────────────────────────

    #[test]
    fn promote_worker_converts_trained_to_expert() {
        let mut pool = LaborPool { untrained: 0, trained: 3, expert: 1, ..LaborPool::new() };
        let result = pool.promote_worker();
        assert!(result);
        assert_eq!(pool.trained, 2);
        assert_eq!(pool.expert, 2);
        assert_eq!(pool.total_workers(), 4); // total unchanged
    }

    #[test]
    fn promote_worker_fails_when_no_trained() {
        let mut pool = LaborPool { untrained: 5, trained: 0, expert: 2, ..LaborPool::new() };
        let result = pool.promote_worker();
        assert!(!result);
        assert_eq!(pool.expert, 2); // unchanged
    }

    // ── Full lifecycle ────────────────────────────────────────────

    #[test]
    fn full_worker_lifecycle() {
        let mut pool = LaborPool::new();

        // Recruit
        pool.recruit_immigrant();
        assert_eq!(pool.untrained, 1);

        // Train
        assert!(pool.train_worker());
        assert_eq!(pool.untrained, 0);
        assert_eq!(pool.trained, 1);

        // Promote
        assert!(pool.promote_worker());
        assert_eq!(pool.trained, 0);
        assert_eq!(pool.expert, 1);

        // Total unchanged throughout
        assert_eq!(pool.total_workers(), 1);
    }

    #[test]
    fn cannot_promote_untrained_directly() {
        let mut pool = LaborPool::new();
        pool.recruit_immigrant();
        // Cannot promote — no trained workers
        assert!(!pool.promote_worker());
        assert_eq!(pool.untrained, 1);
        assert_eq!(pool.expert, 0);
    }

    // ── TierState / rich metadata ─────────────────────────────────

    #[test]
    fn tier_state_defaults_to_all_healthy() {
        let mut pool = LaborPool::new();
        pool.untrained = 3;
        pool.trained = 2;
        // No tier_meta recorded — tier_state() synthesises a healthy default.
        let s = pool.tier_state(WorkerType::Untrained);
        assert_eq!(s.healthy, 3);
        assert_eq!(s.sick, 0);
        assert!(s.training_to.is_none());
        assert!(s.temporary_penalty.is_none());
    }

    #[test]
    fn sick_workers_counted_in_tier_state_but_not_in_effective_workers() {
        let mut pool = LaborPool::new();
        pool.untrained = 5;
        let meta = pool.tier_state_mut(WorkerType::Untrained);
        meta.healthy = 3;
        meta.sick = 2;
        // flat count is still 5 (unchanged — sick are tracked in tier_meta)
        assert_eq!(pool.untrained, 5);
        // effective_workers returns only healthy
        assert_eq!(pool.effective_workers(WorkerType::Untrained), 3);
        // tier_state total() = healthy + sick
        assert_eq!(pool.tier_state(WorkerType::Untrained).total(), 5);
    }

    #[test]
    fn training_queue_stored_in_tier_state() {
        use crate::types::TurnNumber;
        let mut pool = LaborPool::new();
        pool.untrained = 4;
        let meta = pool.tier_state_mut(WorkerType::Untrained);
        meta.training_to = Some((WorkerType::Trained, TurnNumber::new(10)));
        let s = pool.tier_state(WorkerType::Untrained);
        let (target, completion) = s.training_to.unwrap();
        assert_eq!(target, WorkerType::Trained);
        assert_eq!(completion, TurnNumber::new(10));
    }

    #[test]
    fn tier_state_mut_initialises_healthy_from_flat_count() {
        let mut pool = LaborPool::new();
        pool.trained = 7;
        // First access to tier_state_mut should seed healthy from the flat count.
        let meta = pool.tier_state_mut(WorkerType::Trained);
        assert_eq!(meta.healthy, 7);
        assert_eq!(meta.sick, 0);
    }

    #[test]
    fn effective_workers_falls_back_to_flat_count_without_tier_meta() {
        let pool = LaborPool { expert: 4, ..LaborPool::new() };
        assert_eq!(pool.effective_workers(WorkerType::Expert), 4);
    }

    #[test]
    fn sync_tier_healthy_clamps_sick_when_attrition_reduces_flat_below_sick() {
        // Regression: repeated remove_worker on a tier with existing sick workers
        // must not let healthy + sick exceed the flat count.
        let mut pool = LaborPool::new();
        pool.untrained = 5;
        // Seed tier_meta: 2 healthy + 3 sick = 5 total
        let meta = pool.tier_state_mut(WorkerType::Untrained);
        meta.healthy = 2;
        meta.sick = 3;

        // Remove 2 workers via attrition — flat goes from 5 to 3
        pool.remove_worker();
        pool.remove_worker();

        assert_eq!(pool.untrained, 3);
        let s = pool.tier_state(WorkerType::Untrained);
        // Invariant: healthy + sick == flat
        assert_eq!(s.healthy + s.sick, 3, "healthy + sick must equal flat count");
        // Sick clamped to at most flat
        assert!(s.sick <= 3);
    }

    #[test]
    fn sync_tier_healthy_preserves_invariant_across_multiple_removals() {
        let mut pool = LaborPool::new();
        pool.trained = 4;
        let meta = pool.tier_state_mut(WorkerType::Trained);
        meta.healthy = 1;
        meta.sick = 3;

        // Remove all 4 workers one by one
        for _ in 0..4 {
            pool.remove_worker();
        }
        assert_eq!(pool.trained, 0);
        let s = pool.tier_state(WorkerType::Trained);
        assert_eq!(s.healthy, 0);
        assert_eq!(s.sick, 0);
        assert_eq!(s.healthy + s.sick, 0);
    }
}
