use crate::events::TreatyType;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tracks the diplomatic relationship between two nations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiplomaticRelation {
    pub nation_a: NationId,
    pub nation_b: NationId,
    pub score: i32,          // -100 to +100
    pub has_consulate: bool, // trade consulate established (GP->MN only)
    pub has_embassy: bool,   // embassy established (GP->MN, costs $5000)
    pub active_treaties: Vec<TreatyType>,
    pub at_war: bool,
}

impl DiplomaticRelation {
    /// Create a new diplomatic relation between two nations with default values.
    pub fn new(a: NationId, b: NationId) -> Self {
        Self {
            nation_a: a,
            nation_b: b,
            score: 0,
            has_consulate: false,
            has_embassy: false,
            active_treaties: Vec::new(),
            at_war: false,
        }
    }

    /// Improve the diplomatic score by the given amount, clamping to [-100, 100].
    pub fn improve_score(&mut self, amount: i32) {
        self.score = (self.score + amount).clamp(-100, 100);
    }

    /// Reduce the diplomatic score by the given amount, clamping to [-100, 100].
    pub fn reduce_score(&mut self, amount: i32) {
        self.score = (self.score - amount).clamp(-100, 100);
    }

    /// Add a treaty to this relation.
    pub fn add_treaty(&mut self, treaty_type: TreatyType) {
        if !self.active_treaties.contains(&treaty_type) {
            self.active_treaties.push(treaty_type);
        }
    }

    /// Remove a treaty from this relation.
    pub fn remove_treaty(&mut self, treaty_type: TreatyType) {
        self.active_treaties.retain(|t| *t != treaty_type);
    }

    /// Check whether a specific treaty is active.
    pub fn has_treaty(&self, treaty_type: TreatyType) -> bool {
        self.active_treaties.contains(&treaty_type)
    }
}

/// Manages all diplomatic relationships in the game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiplomacyState {
    #[serde(
        serialize_with = "serialize_relations",
        deserialize_with = "deserialize_relations"
    )]
    relations: HashMap<(NationId, NationId), DiplomaticRelation>,
    /// Per-nation diplomatic standing (global reputation).
    pub standing: HashMap<NationId, i32>,
}

/// Serialize HashMap<(NationId, NationId), DiplomaticRelation> as a Vec of pairs
/// because tuple keys cannot be used directly as JSON object keys.
fn serialize_relations<S>(
    relations: &HashMap<(NationId, NationId), DiplomaticRelation>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let entries: Vec<(&(NationId, NationId), &DiplomaticRelation)> = relations.iter().collect();
    entries.serialize(serializer)
}

/// Deserialize Vec of ((NationId, NationId), DiplomaticRelation) pairs back into HashMap.
fn deserialize_relations<'de, D>(
    deserializer: D,
) -> Result<HashMap<(NationId, NationId), DiplomaticRelation>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let entries: Vec<((NationId, NationId), DiplomaticRelation)> = Vec::deserialize(deserializer)?;
    Ok(entries.into_iter().collect())
}

/// Normalize a pair of NationIds to a canonical order so (a,b) and (b,a) map to the same key.
fn ordered_key(a: NationId, b: NationId) -> (NationId, NationId) {
    if a.0 <= b.0 { (a, b) } else { (b, a) }
}

impl DiplomacyState {
    /// Create an empty diplomacy state.
    pub fn new() -> Self {
        Self {
            relations: HashMap::new(),
            standing: HashMap::new(),
        }
    }

    /// Initialize relations between all Great Power pairs with embassies already established.
    pub fn initialize_great_powers(&mut self, gp_ids: &[NationId]) {
        for (i, &a) in gp_ids.iter().enumerate() {
            for &b in &gp_ids[i + 1..] {
                let key = ordered_key(a, b);
                let mut rel = DiplomaticRelation::new(key.0, key.1);
                rel.has_embassy = true;
                self.relations.insert(key, rel);
            }
        }
    }

    /// Get an immutable reference to the relation between two nations (order-independent).
    pub fn get_relation(&self, a: NationId, b: NationId) -> Option<&DiplomaticRelation> {
        let key = ordered_key(a, b);
        self.relations.get(&key)
    }

    /// Get a mutable reference to the relation between two nations (order-independent).
    pub fn get_relation_mut(
        &mut self,
        a: NationId,
        b: NationId,
    ) -> Option<&mut DiplomaticRelation> {
        let key = ordered_key(a, b);
        self.relations.get_mut(&key)
    }

    /// Get a mutable reference to the relation, creating it if it doesn't exist.
    pub fn ensure_relation(&mut self, a: NationId, b: NationId) -> &mut DiplomaticRelation {
        let key = ordered_key(a, b);
        self.relations
            .entry(key)
            .or_insert_with(|| DiplomaticRelation::new(key.0, key.1))
    }

    /// Build a trade consulate from a Great Power to a Minor Nation.
    /// Costs $500. Returns the cost on success.
    pub fn build_consulate(&mut self, gp: NationId, mn: NationId) -> Result<Money, String> {
        let rel = self.ensure_relation(gp, mn);
        if rel.has_consulate {
            return Err("Consulate already established".to_string());
        }
        rel.has_consulate = true;
        Ok(Money::dollars(500))
    }

    /// Build an embassy from a Great Power to a Minor Nation.
    /// Costs $5,000. Requires a consulate to be established first.
    /// Returns the cost on success.
    pub fn build_embassy(&mut self, gp: NationId, mn: NationId) -> Result<Money, String> {
        let rel = self.ensure_relation(gp, mn);
        if !rel.has_consulate {
            return Err("Must build consulate before embassy".to_string());
        }
        if rel.has_embassy {
            return Err("Embassy already established".to_string());
        }
        rel.has_embassy = true;
        Ok(Money::dollars(5000))
    }

    /// Propose a non-aggression pact between a Great Power and a Minor Nation.
    /// Requires an embassy to be established.
    /// `from` must be a Great Power and `to` must be a Minor Nation.
    /// The caller is responsible for verifying nation types before calling this.
    pub fn propose_pact(&mut self, from: NationId, to: NationId) -> Result<(), String> {
        let rel = self.ensure_relation(from, to);
        if !rel.has_embassy {
            return Err("Embassy required before proposing a non-aggression pact".to_string());
        }
        if rel.at_war {
            return Err("Cannot propose pact while at war".to_string());
        }
        if rel.has_treaty(TreatyType::NonAggressionPact) {
            return Err("Non-aggression pact already active".to_string());
        }
        rel.add_treaty(TreatyType::NonAggressionPact);
        rel.improve_score(10);
        Ok(())
    }

    /// Propose an alliance between two Great Powers.
    /// Requires embassy (GP pairs have embassies from game start).
    /// Both nations must be Great Powers — the caller is responsible for verifying this.
    pub fn propose_alliance(&mut self, from: NationId, to: NationId) -> Result<(), String> {
        let rel = self.ensure_relation(from, to);
        if !rel.has_embassy {
            return Err("Embassy required before proposing an alliance".to_string());
        }
        if rel.at_war {
            return Err("Cannot propose alliance while at war".to_string());
        }
        if rel.has_treaty(TreatyType::Alliance) {
            return Err("Alliance already active".to_string());
        }
        rel.add_treaty(TreatyType::Alliance);
        rel.improve_score(15);
        Ok(())
    }

    /// Check whether a specific treaty is active between two nations.
    pub fn has_treaty(&self, a: NationId, b: NationId, treaty: TreatyType) -> bool {
        self.get_relation(a, b)
            .map(|rel| rel.has_treaty(treaty))
            .unwrap_or(false)
    }

    /// Break a specific treaty between two nations.
    /// Removes the treaty and reduces the breaking nation's standing.
    pub fn break_treaty(&mut self, a: NationId, b: NationId, treaty: TreatyType) {
        let had_treaty = {
            let rel = self.ensure_relation(a, b);
            let had = rel.has_treaty(treaty);
            if had {
                rel.remove_treaty(treaty);
                rel.reduce_score(20);
            }
            had
        };
        if had_treaty {
            self.reduce_standing(a, 15);
        }
    }

    /// Send a cash grant from one nation to another, improving the relationship score.
    /// The improvement is amount_in_dollars / 100 (minimum 1 for any non-zero grant).
    /// The caller is responsible for deducting money from the sending nation's treasury.
    pub fn send_grant(&mut self, from: NationId, to: NationId, amount: Money) {
        let improvement = (amount.as_dollars() / 100).max(1) as i32;
        let rel = self.ensure_relation(from, to);
        rel.improve_score(improvement);
    }

    /// Declare war between attacker and defender.
    /// Sets at_war flag, reduces the diplomatic score to minimum,
    /// and breaks all active treaties between the two nations.
    pub fn declare_war(&mut self, attacker: NationId, defender: NationId) {
        // Break all treaties first
        let treaties_to_break: Vec<TreatyType> = self
            .get_relation(attacker, defender)
            .map(|rel| rel.active_treaties.clone())
            .unwrap_or_default();
        for treaty in &treaties_to_break {
            // Use ensure_relation to remove treaties without the standing penalty
            // since war declaration itself is the cause
            let rel = self.ensure_relation(attacker, defender);
            rel.remove_treaty(*treaty);
        }
        if !treaties_to_break.is_empty() {
            self.reduce_standing(attacker, 10);
        }

        let rel = self.ensure_relation(attacker, defender);
        rel.at_war = true;
        rel.score = -100;
    }

    /// Make peace between two nations. Clears the at_war flag.
    pub fn make_peace(&mut self, a: NationId, b: NationId) {
        let rel = self.ensure_relation(a, b);
        rel.at_war = false;
    }

    /// Get the diplomatic standing of a nation. Defaults to 100 for new nations.
    pub fn get_standing(&self, nation: NationId) -> i32 {
        self.standing.get(&nation).copied().unwrap_or(100)
    }

    /// Reduce a nation's standing by the given amount (e.g., for breaking alliances).
    pub fn reduce_standing(&mut self, nation: NationId, amount: i32) {
        let standing = self.standing.entry(nation).or_insert(100);
        *standing -= amount;
    }

    /// Get all relations involving a specific nation.
    pub fn relations_for(
        &self,
        nation: NationId,
    ) -> Vec<(&(NationId, NationId), &DiplomaticRelation)> {
        self.relations
            .iter()
            .filter(|((a, b), _)| *a == nation || *b == nation)
            .collect()
    }

    /// Get all nations that have an Alliance treaty with the given nation.
    pub fn get_allies(&self, nation: NationId) -> Vec<NationId> {
        self.relations
            .iter()
            .filter(|(_, rel)| rel.has_treaty(TreatyType::Alliance))
            .filter_map(|((a, b), _)| {
                if *a == nation {
                    Some(*b)
                } else if *b == nation {
                    Some(*a)
                } else {
                    None
                }
            })
            .collect()
    }
}

impl Default for DiplomacyState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_creation_defaults() {
        let rel = DiplomaticRelation::new(NationId(1), NationId(2));
        assert_eq!(rel.nation_a, NationId(1));
        assert_eq!(rel.nation_b, NationId(2));
        assert_eq!(rel.score, 0);
        assert!(!rel.has_consulate);
        assert!(!rel.has_embassy);
        assert!(rel.active_treaties.is_empty());
        assert!(!rel.at_war);
    }

    #[test]
    fn improve_score_clamps_to_100() {
        let mut rel = DiplomaticRelation::new(NationId(1), NationId(2));
        rel.improve_score(50);
        assert_eq!(rel.score, 50);
        rel.improve_score(80);
        assert_eq!(rel.score, 100); // clamped
    }

    #[test]
    fn reduce_score_clamps_to_neg_100() {
        let mut rel = DiplomaticRelation::new(NationId(1), NationId(2));
        rel.reduce_score(50);
        assert_eq!(rel.score, -50);
        rel.reduce_score(80);
        assert_eq!(rel.score, -100); // clamped
    }

    #[test]
    fn add_and_remove_treaty() {
        let mut rel = DiplomaticRelation::new(NationId(1), NationId(2));
        rel.add_treaty(TreatyType::Alliance);
        assert!(rel.has_treaty(TreatyType::Alliance));

        // Adding the same treaty again should not duplicate.
        rel.add_treaty(TreatyType::Alliance);
        assert_eq!(rel.active_treaties.len(), 1);

        rel.remove_treaty(TreatyType::Alliance);
        assert!(!rel.has_treaty(TreatyType::Alliance));
        assert!(rel.active_treaties.is_empty());
    }

    #[test]
    fn has_treaty_returns_false_for_missing() {
        let rel = DiplomaticRelation::new(NationId(1), NationId(2));
        assert!(!rel.has_treaty(TreatyType::NonAggressionPact));
    }

    #[test]
    fn great_power_initialization() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2), NationId(3)];
        state.initialize_great_powers(&gps);

        // All pairs should have embassies.
        let rel_12 = state.get_relation(NationId(1), NationId(2)).unwrap();
        assert!(rel_12.has_embassy);

        let rel_13 = state.get_relation(NationId(1), NationId(3)).unwrap();
        assert!(rel_13.has_embassy);

        let rel_23 = state.get_relation(NationId(2), NationId(3)).unwrap();
        assert!(rel_23.has_embassy);
    }

    #[test]
    fn great_power_initialization_pair_count() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2), NationId(3), NationId(4)];
        state.initialize_great_powers(&gps);

        // 4 nations => C(4,2) = 6 pairs
        assert_eq!(state.relations.len(), 6);
    }

    #[test]
    fn consulate_building() {
        let mut state = DiplomacyState::new();
        let cost = state.build_consulate(NationId(1), NationId(10)).unwrap();
        assert_eq!(cost, Money::dollars(500));

        let rel = state.get_relation(NationId(1), NationId(10)).unwrap();
        assert!(rel.has_consulate);
    }

    #[test]
    fn consulate_already_exists() {
        let mut state = DiplomacyState::new();
        state.build_consulate(NationId(1), NationId(10)).unwrap();
        let result = state.build_consulate(NationId(1), NationId(10));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Consulate already established");
    }

    #[test]
    fn embassy_requires_consulate() {
        let mut state = DiplomacyState::new();
        let result = state.build_embassy(NationId(1), NationId(10));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Must build consulate before embassy");
    }

    #[test]
    fn embassy_building_after_consulate() {
        let mut state = DiplomacyState::new();
        state.build_consulate(NationId(1), NationId(10)).unwrap();
        let cost = state.build_embassy(NationId(1), NationId(10)).unwrap();
        assert_eq!(cost, Money::dollars(5000));

        let rel = state.get_relation(NationId(1), NationId(10)).unwrap();
        assert!(rel.has_embassy);
    }

    #[test]
    fn embassy_already_exists() {
        let mut state = DiplomacyState::new();
        state.build_consulate(NationId(1), NationId(10)).unwrap();
        state.build_embassy(NationId(1), NationId(10)).unwrap();
        let result = state.build_embassy(NationId(1), NationId(10));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Embassy already established");
    }

    #[test]
    fn war_declaration() {
        let mut state = DiplomacyState::new();
        state.declare_war(NationId(1), NationId(2));

        let rel = state.get_relation(NationId(1), NationId(2)).unwrap();
        assert!(rel.at_war);
        assert_eq!(rel.score, -100);
    }

    #[test]
    fn make_peace() {
        let mut state = DiplomacyState::new();
        state.declare_war(NationId(1), NationId(2));
        state.make_peace(NationId(1), NationId(2));

        let rel = state.get_relation(NationId(1), NationId(2)).unwrap();
        assert!(!rel.at_war);
        // Score remains at -100 after peace; it doesn't automatically recover.
        assert_eq!(rel.score, -100);
    }

    #[test]
    fn standing_default() {
        let state = DiplomacyState::new();
        assert_eq!(state.get_standing(NationId(1)), 100);
    }

    #[test]
    fn standing_reduction() {
        let mut state = DiplomacyState::new();
        state.reduce_standing(NationId(1), 30);
        assert_eq!(state.get_standing(NationId(1)), 70);

        state.reduce_standing(NationId(1), 50);
        assert_eq!(state.get_standing(NationId(1)), 20);
    }

    #[test]
    fn order_independence_get_relation() {
        let mut state = DiplomacyState::new();
        state.declare_war(NationId(1), NationId(2));

        // Getting the relation in either order should return the same data.
        let rel_ab = state.get_relation(NationId(1), NationId(2)).unwrap();
        let score_ab = rel_ab.score;
        let at_war_ab = rel_ab.at_war;

        let rel_ba = state.get_relation(NationId(2), NationId(1)).unwrap();
        let score_ba = rel_ba.score;
        let at_war_ba = rel_ba.at_war;

        assert_eq!(score_ab, score_ba);
        assert_eq!(at_war_ab, at_war_ba);
    }

    #[test]
    fn order_independence_ensure_relation() {
        let mut state = DiplomacyState::new();

        // Create via (2,1) order.
        state.ensure_relation(NationId(2), NationId(1)).score = 42;

        // Access via (1,2) order — should find the same relation.
        let rel = state.get_relation(NationId(1), NationId(2)).unwrap();
        assert_eq!(rel.score, 42);
    }

    #[test]
    fn order_independence_consulate() {
        let mut state = DiplomacyState::new();
        state.build_consulate(NationId(5), NationId(3)).unwrap();

        // Should find the consulate regardless of argument order.
        let rel = state.get_relation(NationId(3), NationId(5)).unwrap();
        assert!(rel.has_consulate);
    }

    // ── Treaty proposal tests ────────────────────────────────────

    #[test]
    fn pact_requires_embassy() {
        let mut state = DiplomacyState::new();
        // No embassy established
        let result = state.propose_pact(NationId(1), NationId(10));
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Embassy required before proposing a non-aggression pact"
        );
    }

    #[test]
    fn pact_succeeds_with_embassy() {
        let mut state = DiplomacyState::new();
        state.build_consulate(NationId(1), NationId(10)).unwrap();
        state.build_embassy(NationId(1), NationId(10)).unwrap();

        let result = state.propose_pact(NationId(1), NationId(10));
        assert!(result.is_ok());
        assert!(state.has_treaty(NationId(1), NationId(10), TreatyType::NonAggressionPact));
    }

    #[test]
    fn alliance_requires_embassy() {
        let mut state = DiplomacyState::new();
        // No embassy between these two GPs (not initialized)
        let result = state.propose_alliance(NationId(1), NationId(2));
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Embassy required before proposing an alliance"
        );
    }

    #[test]
    fn alliance_succeeds_between_gps_with_embassy() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);

        let result = state.propose_alliance(NationId(1), NationId(2));
        assert!(result.is_ok());
        assert!(state.has_treaty(NationId(1), NationId(2), TreatyType::Alliance));
    }

    #[test]
    fn duplicate_pact_rejected() {
        let mut state = DiplomacyState::new();
        state.build_consulate(NationId(1), NationId(10)).unwrap();
        state.build_embassy(NationId(1), NationId(10)).unwrap();
        state.propose_pact(NationId(1), NationId(10)).unwrap();

        let result = state.propose_pact(NationId(1), NationId(10));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Non-aggression pact already active");
    }

    #[test]
    fn duplicate_alliance_rejected() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);
        state.propose_alliance(NationId(1), NationId(2)).unwrap();

        let result = state.propose_alliance(NationId(1), NationId(2));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Alliance already active");
    }

    // ── War breaks all treaties ──────────────────────────────────

    #[test]
    fn war_breaks_all_treaties() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);

        // Establish alliance
        state.propose_alliance(NationId(1), NationId(2)).unwrap();
        assert!(state.has_treaty(NationId(1), NationId(2), TreatyType::Alliance));

        // Declare war
        state.declare_war(NationId(1), NationId(2));

        // Alliance should be broken
        assert!(!state.has_treaty(NationId(1), NationId(2), TreatyType::Alliance));
        let rel = state.get_relation(NationId(1), NationId(2)).unwrap();
        assert!(rel.at_war);
        assert!(rel.active_treaties.is_empty());
    }

    #[test]
    fn war_breaks_pact() {
        let mut state = DiplomacyState::new();
        state.build_consulate(NationId(1), NationId(10)).unwrap();
        state.build_embassy(NationId(1), NationId(10)).unwrap();
        state.propose_pact(NationId(1), NationId(10)).unwrap();
        assert!(state.has_treaty(NationId(1), NationId(10), TreatyType::NonAggressionPact));

        state.declare_war(NationId(1), NationId(10));
        assert!(!state.has_treaty(NationId(1), NationId(10), TreatyType::NonAggressionPact));
    }

    // ── Break treaty ─────────────────────────────────────────────

    #[test]
    fn break_treaty_reduces_standing() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);
        state.propose_alliance(NationId(1), NationId(2)).unwrap();

        let standing_before = state.get_standing(NationId(1));
        state.break_treaty(NationId(1), NationId(2), TreatyType::Alliance);

        assert!(!state.has_treaty(NationId(1), NationId(2), TreatyType::Alliance));
        assert!(state.get_standing(NationId(1)) < standing_before);
    }

    // ── Cash grant ───────────────────────────────────────────────

    #[test]
    fn cash_grant_improves_relationship() {
        let mut state = DiplomacyState::new();
        state.build_consulate(NationId(1), NationId(10)).unwrap();

        let score_before = state.get_relation(NationId(1), NationId(10)).unwrap().score;
        state.send_grant(NationId(1), NationId(10), Money::dollars(500));
        let score_after = state.get_relation(NationId(1), NationId(10)).unwrap().score;

        assert!(score_after > score_before);
        assert_eq!(score_after, score_before + 5); // 500/100 = 5
    }

    #[test]
    fn cash_grant_large_amount() {
        let mut state = DiplomacyState::new();
        state.build_consulate(NationId(1), NationId(10)).unwrap();

        state.send_grant(NationId(1), NationId(10), Money::dollars(2000));
        let score = state.get_relation(NationId(1), NationId(10)).unwrap().score;
        assert_eq!(score, 20); // 2000/100 = 20
    }

    // ── Get allies ───────────────────────────────────────────────

    #[test]
    fn get_allies_returns_allied_nations() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2), NationId(3)];
        state.initialize_great_powers(&gps);
        state.propose_alliance(NationId(1), NationId(2)).unwrap();

        let allies = state.get_allies(NationId(1));
        assert_eq!(allies.len(), 1);
        assert!(allies.contains(&NationId(2)));

        // Nation 3 is not an ally
        let allies_3 = state.get_allies(NationId(3));
        assert!(allies_3.is_empty());
    }

    #[test]
    fn has_treaty_returns_false_for_non_existent_relation() {
        let state = DiplomacyState::new();
        assert!(!state.has_treaty(NationId(1), NationId(2), TreatyType::Alliance));
    }

    // ── Alliance breaks on war declaration ──────────────────────

    #[test]
    fn alliance_breaks_on_war_declaration() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);

        // Form alliance
        state.propose_alliance(NationId(1), NationId(2)).unwrap();
        assert!(state.has_treaty(NationId(1), NationId(2), TreatyType::Alliance));

        // Declare war — alliance should be removed
        state.declare_war(NationId(1), NationId(2));
        assert!(
            !state.has_treaty(NationId(1), NationId(2), TreatyType::Alliance),
            "Alliance should be broken when war is declared"
        );
        let rel = state.get_relation(NationId(1), NationId(2)).unwrap();
        assert!(rel.at_war);
        assert!(rel.active_treaties.is_empty());
    }

    // ── Standing decreases on treaty break ──────────────────────

    #[test]
    fn standing_decreases_on_treaty_break() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);
        state.propose_alliance(NationId(1), NationId(2)).unwrap();

        let standing_before = state.get_standing(NationId(1));
        state.break_treaty(NationId(1), NationId(2), TreatyType::Alliance);
        let standing_after = state.get_standing(NationId(1));

        assert!(
            standing_after < standing_before,
            "Standing should decrease after breaking a treaty: {} -> {}",
            standing_before,
            standing_after
        );
        // break_treaty reduces standing by 15
        assert_eq!(standing_after, standing_before - 15);
    }

    // ── Grant improves score proportional to amount ─────────────

    #[test]
    fn grant_improves_score_proportional_to_amount() {
        let mut state = DiplomacyState::new();
        state.build_consulate(NationId(1), NationId(10)).unwrap();

        let score_before = state.get_relation(NationId(1), NationId(10)).unwrap().score;
        state.send_grant(NationId(1), NationId(10), Money::dollars(1000));
        let score_after = state.get_relation(NationId(1), NationId(10)).unwrap().score;

        // $1000 grant => 1000/100 = +10 improvement
        assert_eq!(
            score_after,
            score_before + 10,
            "$1000 grant should give +10 diplomatic score"
        );
    }
}
