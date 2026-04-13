#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WorkerType {
    Untrained,
    Trained,
    Expert,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LaborPool {
    pub untrained: u32,
    pub trained: u32,
    pub expert: u32,
}

impl LaborPool {
    /// Create a new empty labor pool.
    pub fn new() -> Self {
        Self {
            untrained: 0,
            trained: 0,
            expert: 0,
        }
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
            true
        } else {
            false
        }
    }

    /// Add one untrained immigrant worker to the pool.
    pub fn recruit_immigrant(&mut self) {
        self.untrained += 1;
    }

    /// Remove one worker due to starvation or other attrition.
    /// Removes untrained first, then trained, then expert.
    /// Returns `true` if a worker was removed, `false` if pool is empty.
    pub fn remove_worker(&mut self) -> bool {
        if self.untrained > 0 {
            self.untrained -= 1;
            true
        } else if self.trained > 0 {
            self.trained -= 1;
            true
        } else if self.expert > 0 {
            self.expert -= 1;
            true
        } else {
            false
        }
    }

    /// Workers available for production. For now, equals total workers.
    pub fn available_for_production(&self) -> u32 {
        self.total_workers()
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
        let pool = LaborPool {
            untrained: 3,
            trained: 2,
            expert: 1,
        };
        assert_eq!(pool.total_workers(), 6);
    }

    #[test]
    fn total_labor_units_uses_training_multipliers() {
        let pool = LaborPool {
            untrained: 5,
            trained: 3,
            expert: 2,
        };
        // 5*1 + 3*2 + 2*4 = 5 + 6 + 8 = 19
        assert_eq!(pool.total_labor_units(), 19);
        assert_eq!(pool.total_workers(), 10);
    }

    #[test]
    fn total_labor_units_with_custom_multipliers() {
        let pool = LaborPool {
            untrained: 4,
            trained: 2,
            expert: 1,
        };
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
        let mut pool = LaborPool {
            untrained: 0,
            trained: 5,
            expert: 3,
        };
        let result = pool.train_worker();
        assert!(!result);
        assert_eq!(pool.trained, 5); // unchanged
    }

    // ── promote_worker ────────────────────────────────────────────

    #[test]
    fn promote_worker_converts_trained_to_expert() {
        let mut pool = LaborPool {
            untrained: 0,
            trained: 3,
            expert: 1,
        };
        let result = pool.promote_worker();
        assert!(result);
        assert_eq!(pool.trained, 2);
        assert_eq!(pool.expert, 2);
        assert_eq!(pool.total_workers(), 4); // total unchanged
    }

    #[test]
    fn promote_worker_fails_when_no_trained() {
        let mut pool = LaborPool {
            untrained: 5,
            trained: 0,
            expert: 2,
        };
        let result = pool.promote_worker();
        assert!(!result);
        assert_eq!(pool.expert, 2); // unchanged
    }

    // ── available_for_production ───────────────────────────────────

    #[test]
    fn available_for_production_equals_total() {
        let pool = LaborPool {
            untrained: 4,
            trained: 3,
            expert: 2,
        };
        assert_eq!(pool.available_for_production(), 9);
        assert_eq!(pool.available_for_production(), pool.total_workers());
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
}
