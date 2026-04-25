use crate::events::TreatyType;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// A diplomatic proposal awaiting evaluation by the target nation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiplomaticProposal {
    pub from: NationId,
    pub to: NationId,
    pub proposal_type: TreatyType,
    pub turn_proposed: TurnNumber,
    /// For PactDefenseRequest: the nation that attacked the minor.
    #[serde(default)]
    pub attacker: Option<NationId>,
    /// For PactDefenseRequest: remaining candidate protectors if this one declines.
    #[serde(default)]
    pub cascade_remaining: Option<Vec<NationId>>,
}

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
    /// Turn the current war was declared, if any. Used to grant a one-turn
    /// grace period before naval combat begins (card #104). `None` means
    /// either there is no war, or the war is older than the just-declared
    /// turn — combat resolves normally.
    #[serde(default)]
    pub turn_war_declared: Option<TurnNumber>,
}

/// Records an alliance that was broken because one side made separate peace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokenAlliance {
    pub peacemaker: NationId,
    pub former_ally: NationId,
    pub enemy: NationId,
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
            turn_war_declared: None,
        }
    }

    /// Whether hostile actions (naval combat, blockade, …) should apply on
    /// `current_turn` for this war. Returns false during the one-turn grace
    /// period after declaration (card #104): the defender always gets one
    /// turn before any enemy action takes effect.
    pub fn hostilities_active_on(&self, current_turn: TurnNumber) -> bool {
        self.at_war && self.turn_war_declared != Some(current_turn)
    }

    /// Improve the diplomatic score by the given amount, clamping to [-100, 100].
    pub fn improve_score(&mut self, amount: i32) {
        self.score = (self.score + amount).clamp(-100, 100);
    }

    /// Reduce the diplomatic score by the given amount, clamping to [-100, 100].
    pub fn reduce_score(&mut self, amount: i32) {
        self.score = (self.score - amount).clamp(-100, 100);
    }

    /// Add a treaty to this relation, enforcing mutual exclusion:
    /// - Alliance supersedes NonAggressionPact (NAP is auto-removed)
    /// - NAP cannot be added if Alliance is already active
    pub fn add_treaty(&mut self, treaty_type: TreatyType) {
        if self.active_treaties.contains(&treaty_type) {
            return;
        }
        match treaty_type {
            TreatyType::Alliance => {
                // Alliance supersedes NAP
                self.active_treaties
                    .retain(|t| *t != TreatyType::NonAggressionPact);
                self.active_treaties.push(TreatyType::Alliance);
            }
            TreatyType::NonAggressionPact => {
                // NAP cannot coexist with Alliance
                if !self.active_treaties.contains(&TreatyType::Alliance) {
                    self.active_treaties.push(TreatyType::NonAggressionPact);
                }
            }
            other => {
                self.active_treaties.push(other);
            }
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
    relations: BTreeMap<(NationId, NationId), DiplomaticRelation>,
    /// Per-nation diplomatic standing (global reputation).
    pub standing: HashMap<NationId, i32>,
    /// Proposals awaiting evaluation by the target nation.
    #[serde(default)]
    pub pending_proposals: Vec<DiplomaticProposal>,
    /// Separate-peace alliance breaks recorded this turn and finalized after all
    /// peace outcomes for the turn are known.
    #[serde(default)]
    pending_separate_peace_breaks: Vec<BrokenAlliance>,
    /// (attacker, minor) pairs for which a pact-defense protection request
    /// has already been raised in the current war. Prevents re-triggering
    /// the cascade every combat (card #68). Cleared when the attacker/minor
    /// war ends (peace, incorporation, anarchy).
    #[serde(default, with = "pact_defense_set_serde")]
    pact_defense_requested: HashSet<(NationId, NationId)>,
}

mod pact_defense_set_serde {
    use super::NationId;
    use serde::{Deserialize, Serialize};
    use std::collections::HashSet;

    pub fn serialize<S>(
        set: &HashSet<(NationId, NationId)>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let v: Vec<(NationId, NationId)> = set.iter().copied().collect();
        v.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashSet<(NationId, NationId)>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v: Vec<(NationId, NationId)> = Vec::deserialize(deserializer)?;
        Ok(v.into_iter().collect())
    }
}

/// Serialize BTreeMap<(NationId, NationId), DiplomaticRelation> as a Vec of pairs
/// because tuple keys cannot be used directly as JSON object keys.
fn serialize_relations<S>(
    relations: &BTreeMap<(NationId, NationId), DiplomaticRelation>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let entries: Vec<(&(NationId, NationId), &DiplomaticRelation)> = relations.iter().collect();
    entries.serialize(serializer)
}

/// Deserialize Vec of ((NationId, NationId), DiplomaticRelation) pairs back into BTreeMap.
fn deserialize_relations<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<(NationId, NationId), DiplomaticRelation>, D::Error>
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
    fn has_pending_separate_peace_break(&self, a: NationId, b: NationId) -> bool {
        self.pending_separate_peace_breaks.iter().any(|broken| {
            (broken.peacemaker == a && broken.former_ally == b)
                || (broken.peacemaker == b && broken.former_ally == a)
        })
    }

    /// Create an empty diplomacy state.
    pub fn new() -> Self {
        Self {
            relations: BTreeMap::new(),
            standing: HashMap::new(),
            pending_proposals: Vec::new(),
            pending_separate_peace_breaks: Vec::new(),
            pact_defense_requested: HashSet::new(),
        }
    }

    /// Has a pact-defense protection request already been raised for the given
    /// (attacker, minor) pair in the current war? Card #68.
    pub fn is_pact_defense_requested(&self, attacker: NationId, minor: NationId) -> bool {
        self.pact_defense_requested.contains(&(attacker, minor))
    }

    /// Mark that a pact-defense protection request has been raised for the
    /// given (attacker, minor) pair. Future attacks by the same attacker on
    /// the same minor will not re-trigger the cascade until the war ends
    /// (see `clear_pact_defense_for_war` / `clear_pact_defense_for_nation`).
    pub fn mark_pact_defense_requested(&mut self, attacker: NationId, minor: NationId) {
        self.pact_defense_requested.insert((attacker, minor));
    }

    /// Clear pact-defense dedup entries for any combination of the two
    /// nations when the war between them ends (peace treaty, mutual peace,
    /// one side annexed). Handles both role orderings so the caller does not
    /// need to know which is attacker and which is minor.
    pub fn clear_pact_defense_for_war(&mut self, a: NationId, b: NationId) {
        self.pact_defense_requested
            .retain(|&(att, min)| !((att == a && min == b) || (att == b && min == a)));
    }

    /// Clear every pact-defense dedup entry involving the given nation.
    /// Use when the nation is incorporated, destroyed, or enters anarchy —
    /// any war it was part of has effectively ended for dedup purposes.
    pub fn clear_pact_defense_for_nation(&mut self, nation: NationId) {
        self.pact_defense_requested
            .retain(|&(att, min)| att != nation && min != nation);
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
    /// Rejects if the proposer's standing is below 30.
    pub fn propose_pact(&mut self, from: NationId, to: NationId) -> Result<(), String> {
        if !self.would_accept_treaty(from) {
            return Err("Standing too low to propose treaties".to_string());
        }
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
        if rel.has_treaty(TreatyType::Alliance) {
            return Err("Alliance already active — NAP is redundant".to_string());
        }
        rel.add_treaty(TreatyType::NonAggressionPact);
        rel.improve_score(10);
        Ok(())
    }

    /// Propose an alliance between two Great Powers.
    /// Requires embassy (GP pairs have embassies from game start).
    /// Both nations must be Great Powers — the caller is responsible for verifying this.
    /// Rejects if the proposer's standing is below 30.
    pub fn propose_alliance(&mut self, from: NationId, to: NationId) -> Result<(), String> {
        if !self.would_accept_treaty(from) {
            return Err("Standing too low to propose treaties".to_string());
        }
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
    ///
    /// Production callers should prefer [`declare_war_at`] to record the
    /// declaration turn and enable the one-turn naval combat grace period.
    /// This 2-arg variant leaves `turn_war_declared = None`, so naval
    /// combat begins immediately — appropriate for tests that don't want
    /// to model the grace period.
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
        // Explicitly clear any stale grace-period stamp left over from a
        // prior war on this same relation. Production callers should use
        // `declare_war_at` to re-stamp; this keeps the legacy 2-arg form
        // deterministic for tests that do not care about the grace turn.
        rel.turn_war_declared = None;
    }

    /// Declare war and stamp the current turn so naval combat is deferred
    /// by one turn (card #104). Production callers should always use this
    /// variant; tests that don't care about the grace period may use the
    /// 2-arg [`declare_war`].
    pub fn declare_war_at(
        &mut self,
        attacker: NationId,
        defender: NationId,
        current_turn: TurnNumber,
    ) {
        self.declare_war(attacker, defender);
        let rel = self.ensure_relation(attacker, defender);
        rel.turn_war_declared = Some(current_turn);
    }

    /// Queue peace between two nations without finalizing any same-turn
    /// separate-peace alliance penalties yet.
    pub fn queue_peace(&mut self, a: NationId, b: NationId) {
        if !self.is_at_war(a, b) {
            return;
        }

        let mut broken_alliances: Vec<BrokenAlliance> = self
            .get_allies(a)
            .into_iter()
            .filter(|ally| self.is_at_war(*ally, b))
            .map(|ally| BrokenAlliance {
                peacemaker: a,
                former_ally: ally,
                enemy: b,
            })
            .collect();
        broken_alliances.extend(
            self.get_allies(b)
                .into_iter()
                .filter(|ally| self.is_at_war(*ally, a))
                .map(|ally| BrokenAlliance {
                    peacemaker: b,
                    former_ally: ally,
                    enemy: a,
                }),
        );

        let rel = self.ensure_relation(a, b);
        rel.at_war = false;
        // Clear the declaration timestamp so a future war (which will be
        // declared via `declare_war_at` again) doesn't inherit a stale
        // grace-period stamp from the prior conflict.
        rel.turn_war_declared = None;

        for broken in broken_alliances {
            let already_pending = self.pending_separate_peace_breaks.contains(&broken);
            if !already_pending {
                self.pending_separate_peace_breaks.push(broken);
            }
        }

        // Peace between a and b ends any ongoing pact-defense dedup for this
        // pair so a fresh request can be raised in a future war (card #68).
        self.clear_pact_defense_for_war(a, b);
    }

    /// Finalize any alliance breaks caused by separate peace after all peace
    /// outcomes for the current turn are known.
    pub fn finalize_pending_separate_peace_breaks(&mut self) -> Vec<BrokenAlliance> {
        let pending = std::mem::take(&mut self.pending_separate_peace_breaks);
        let mut finalized = Vec::new();

        for broken in pending {
            let alliance_still_active =
                self.has_treaty(broken.peacemaker, broken.former_ally, TreatyType::Alliance);
            let ally_still_at_war = self.is_at_war(broken.former_ally, broken.enemy);
            if alliance_still_active && ally_still_at_war {
                self.break_treaty(broken.peacemaker, broken.former_ally, TreatyType::Alliance);
                finalized.push(broken);
            }
        }

        finalized
    }

    /// Make peace between two nations and immediately finalize any separate-peace
    /// alliance breaks. Use `queue_peace` during turn resolution when multiple
    /// same-turn peaces may need to be reconciled together.
    pub fn make_peace(&mut self, a: NationId, b: NationId) -> Vec<BrokenAlliance> {
        self.queue_peace(a, b);
        self.finalize_pending_separate_peace_breaks()
    }

    /// Get the diplomatic standing of a nation. Defaults to 100 for new nations.
    pub fn get_standing(&self, nation: NationId) -> i32 {
        self.standing.get(&nation).copied().unwrap_or(100)
    }

    /// Reduce a nation's standing by the given amount (e.g., for breaking alliances).
    /// Standing is clamped to a minimum of -100.
    pub fn reduce_standing(&mut self, nation: NationId, amount: i32) {
        let standing = self.standing.entry(nation).or_insert(100);
        *standing = (*standing - amount).max(-100);
    }

    /// Propose peace between two nations at war. Creates a pending proposal.
    pub fn propose_peace(
        &mut self,
        from: NationId,
        to: NationId,
        turn: TurnNumber,
    ) -> Result<(), String> {
        if !self.is_at_war(from, to) {
            return Err("Not at war".to_string());
        }
        // No duplicate pending peace proposals between these two nations
        let already_pending = self.pending_proposals.iter().any(|p| {
            p.proposal_type == TreatyType::PeaceTreaty
                && ((p.from == from && p.to == to) || (p.from == to && p.to == from))
        });
        if already_pending {
            return Err("Peace proposal already pending".to_string());
        }
        self.pending_proposals.push(DiplomaticProposal {
            from,
            to,
            proposal_type: TreatyType::PeaceTreaty,
            turn_proposed: turn,
            attacker: None,
            cascade_remaining: None,
        });
        Ok(())
    }

    /// Create a general treaty proposal (NAP, Alliance, etc.). Creates a pending proposal.
    /// Validates preconditions based on treaty type (embassy, standing, no duplicates).
    pub fn propose_treaty(
        &mut self,
        from: NationId,
        to: NationId,
        treaty_type: TreatyType,
        turn: TurnNumber,
    ) -> Result<(), String> {
        // Reject unsupported treaty types first (before any state checks)
        match treaty_type {
            TreatyType::NonAggressionPact | TreatyType::Alliance => {}
            _ => {
                return Err(format!(
                    "{:?} cannot be proposed via propose_treaty — use the dedicated method",
                    treaty_type
                ));
            }
        }

        if self.is_at_war(from, to) {
            return Err("Cannot propose treaty while at war".to_string());
        }

        // Treaty-type-specific preconditions
        match treaty_type {
            TreatyType::NonAggressionPact => {
                if !self.would_accept_treaty(from) {
                    return Err("Standing too low to propose treaties".to_string());
                }
                let rel = self.get_relation(from, to);
                if !rel.map(|r| r.has_embassy).unwrap_or(false) {
                    return Err(
                        "Embassy required before proposing a non-aggression pact".to_string()
                    );
                }
                if rel
                    .map(|r| r.has_treaty(TreatyType::NonAggressionPact))
                    .unwrap_or(false)
                {
                    return Err("Non-aggression pact already active".to_string());
                }
                if rel
                    .map(|r| r.has_treaty(TreatyType::Alliance))
                    .unwrap_or(false)
                {
                    return Err("Alliance already active — NAP is redundant".to_string());
                }
            }
            TreatyType::Alliance => {
                if !self.would_accept_treaty(from) {
                    return Err("Standing too low to propose treaties".to_string());
                }
                let rel = self.get_relation(from, to);
                if !rel.map(|r| r.has_embassy).unwrap_or(false) {
                    return Err("Embassy required before proposing an alliance".to_string());
                }
                if rel
                    .map(|r| r.has_treaty(TreatyType::Alliance))
                    .unwrap_or(false)
                {
                    return Err("Alliance already active".to_string());
                }
            }
            // Unreachable — unsupported types are rejected above
            _ => unreachable!(),
        }

        // No duplicate pending proposals of same type
        let already_pending = self.pending_proposals.iter().any(|p| {
            p.proposal_type == treaty_type
                && ((p.from == from && p.to == to) || (p.from == to && p.to == from))
        });
        if already_pending {
            return Err("Proposal already pending".to_string());
        }
        self.pending_proposals.push(DiplomaticProposal {
            from,
            to,
            proposal_type: treaty_type,
            turn_proposed: turn,
            attacker: None,
            cascade_remaining: None,
        });
        Ok(())
    }

    /// Drain all pending proposals for evaluation. Returns ownership.
    pub fn drain_proposals(&mut self) -> Vec<DiplomaticProposal> {
        std::mem::take(&mut self.pending_proposals)
    }

    /// Remove proposals older than `max_age` turns.
    pub fn expire_proposals(&mut self, current_turn: TurnNumber, max_age: u32) {
        self.pending_proposals
            .retain(|p| current_turn.0.saturating_sub(p.turn_proposed.0) <= max_age);
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
                if *a == nation && !self.has_pending_separate_peace_break(*a, *b) {
                    Some(*b)
                } else if *b == nation && !self.has_pending_separate_peace_break(*a, *b) {
                    Some(*a)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all nations that have a Non-Aggression Pact with the given nation.
    /// Used to identify potential protectors for minor nations (pact defense).
    pub fn get_pact_holders(&self, nation: NationId) -> Vec<NationId> {
        self.relations
            .iter()
            .filter(|(_, rel)| rel.has_treaty(TreatyType::NonAggressionPact))
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

    /// Check if a treaty would be accepted based on the proposer's standing.
    /// Nations with standing below 30 get rejected.
    pub fn would_accept_treaty(&self, proposer: NationId) -> bool {
        let standing = self.get_standing(proposer);
        standing >= 30
    }

    /// Check whether a consulate exists between two nations.
    pub fn has_consulate(&self, a: NationId, b: NationId) -> bool {
        self.get_relation(a, b)
            .map(|rel| rel.has_consulate)
            .unwrap_or(false)
    }

    /// Check whether an embassy exists between two nations.
    pub fn has_embassy(&self, a: NationId, b: NationId) -> bool {
        self.get_relation(a, b)
            .map(|rel| rel.has_embassy)
            .unwrap_or(false)
    }

    /// Check whether two nations are at war.
    pub fn is_at_war(&self, a: NationId, b: NationId) -> bool {
        self.get_relation(a, b)
            .map(|rel| rel.at_war)
            .unwrap_or(false)
    }

    /// Check whether a nation is at war with any other nation.
    pub fn is_at_war_with_anyone(&self, nation: NationId) -> bool {
        self.relations
            .values()
            .any(|rel| rel.at_war && (rel.nation_a == nation || rel.nation_b == nation))
    }

    /// Check whether `nation` is at war with someone other than `excluded`.
    /// Used to detect target-side allies that are tied up in another conflict.
    pub fn is_at_war_with_anyone_except(&self, nation: NationId, excluded: NationId) -> bool {
        self.relations.values().any(|rel| {
            if !rel.at_war {
                return false;
            }
            let other = if rel.nation_a == nation {
                Some(rel.nation_b)
            } else if rel.nation_b == nation {
                Some(rel.nation_a)
            } else {
                None
            };
            matches!(other, Some(o) if o != excluded)
        })
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
        let broken = state.make_peace(NationId(1), NationId(2));

        let rel = state.get_relation(NationId(1), NationId(2)).unwrap();
        assert!(!rel.at_war);
        // Score remains at -100 after peace; it doesn't automatically recover.
        assert_eq!(rel.score, -100);
        assert!(broken.is_empty());
    }

    #[test]
    fn make_peace_breaks_alliance_for_separate_peace() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2), NationId(3)];
        state.initialize_great_powers(&gps);

        state.propose_alliance(NationId(1), NationId(3)).unwrap();
        state.declare_war(NationId(1), NationId(2));
        state.declare_war(NationId(3), NationId(2));

        let standing_before = state.get_standing(NationId(1));
        let relation_before = state.get_relation(NationId(1), NationId(3)).unwrap().score;

        let broken = state.make_peace(NationId(1), NationId(2));

        assert_eq!(
            broken,
            vec![BrokenAlliance {
                peacemaker: NationId(1),
                former_ally: NationId(3),
                enemy: NationId(2),
            }]
        );
        assert!(!state.is_at_war(NationId(1), NationId(2)));
        assert!(state.is_at_war(NationId(3), NationId(2)));
        assert!(!state.has_treaty(NationId(1), NationId(3), TreatyType::Alliance));
        assert_eq!(state.get_standing(NationId(1)), standing_before - 15);
        assert_eq!(
            state.get_relation(NationId(1), NationId(3)).unwrap().score,
            relation_before - 20
        );
    }

    #[test]
    fn queued_same_turn_coalition_peaces_do_not_break_alliance() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2), NationId(3)];
        state.initialize_great_powers(&gps);

        state.propose_alliance(NationId(1), NationId(3)).unwrap();
        state.declare_war(NationId(1), NationId(2));
        state.declare_war(NationId(3), NationId(2));

        let standing_before = state.get_standing(NationId(1));
        let relation_before = state.get_relation(NationId(1), NationId(3)).unwrap().score;

        state.queue_peace(NationId(1), NationId(2));
        state.queue_peace(NationId(3), NationId(2));
        let broken = state.finalize_pending_separate_peace_breaks();

        assert!(broken.is_empty());
        assert!(state.has_treaty(NationId(1), NationId(3), TreatyType::Alliance));
        assert_eq!(state.get_standing(NationId(1)), standing_before);
        assert_eq!(
            state.get_relation(NationId(1), NationId(3)).unwrap().score,
            relation_before
        );
    }

    #[test]
    fn make_peace_keeps_alliance_when_ally_is_not_in_same_war() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2), NationId(3)];
        state.initialize_great_powers(&gps);

        state.propose_alliance(NationId(1), NationId(3)).unwrap();
        state.declare_war(NationId(1), NationId(2));

        let standing_before = state.get_standing(NationId(1));
        let broken = state.make_peace(NationId(1), NationId(2));

        assert!(broken.is_empty());
        assert!(state.has_treaty(NationId(1), NationId(3), TreatyType::Alliance));
        assert_eq!(state.get_standing(NationId(1)), standing_before);
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

    // ── Treaty mutual exclusion ─────────────────────────────────

    #[test]
    fn alliance_supersedes_nap() {
        let mut rel = DiplomaticRelation::new(NationId(1), NationId(2));
        rel.add_treaty(TreatyType::NonAggressionPact);
        assert!(rel.has_treaty(TreatyType::NonAggressionPact));

        // Adding Alliance should auto-remove NAP
        rel.add_treaty(TreatyType::Alliance);
        assert!(rel.has_treaty(TreatyType::Alliance));
        assert!(!rel.has_treaty(TreatyType::NonAggressionPact));
        assert_eq!(rel.active_treaties.len(), 1);
    }

    #[test]
    fn nap_rejected_when_alliance_active() {
        let mut rel = DiplomaticRelation::new(NationId(1), NationId(2));
        rel.add_treaty(TreatyType::Alliance);

        // Adding NAP while Alliance is active should be a no-op
        rel.add_treaty(TreatyType::NonAggressionPact);
        assert!(rel.has_treaty(TreatyType::Alliance));
        assert!(!rel.has_treaty(TreatyType::NonAggressionPact));
        assert_eq!(rel.active_treaties.len(), 1);
    }

    #[test]
    fn propose_pact_rejected_when_alliance_active() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);
        state.propose_alliance(NationId(1), NationId(2)).unwrap();

        let result = state.propose_pact(NationId(1), NationId(2));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Alliance already active"));
    }

    // ── propose_treaty validation ─────────────────────────────────

    #[test]
    fn propose_treaty_nap_requires_embassy() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);
        // Drop embassy to test precondition
        state.ensure_relation(NationId(1), NationId(2)).has_embassy = false;

        let result = state.propose_treaty(
            NationId(1),
            NationId(2),
            TreatyType::NonAggressionPact,
            TurnNumber::new(1),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Embassy required"));
    }

    #[test]
    fn propose_treaty_nap_rejects_duplicate() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);

        state
            .propose_treaty(
                NationId(1),
                NationId(2),
                TreatyType::NonAggressionPact,
                TurnNumber::new(1),
            )
            .unwrap();
        let result = state.propose_treaty(
            NationId(1),
            NationId(2),
            TreatyType::NonAggressionPact,
            TurnNumber::new(1),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already pending"));
    }

    #[test]
    fn propose_treaty_queues_pending_proposal() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);

        state
            .propose_treaty(
                NationId(1),
                NationId(2),
                TreatyType::Alliance,
                TurnNumber::new(5),
            )
            .unwrap();
        assert_eq!(state.pending_proposals.len(), 1);
        assert_eq!(
            state.pending_proposals[0].proposal_type,
            TreatyType::Alliance
        );
        assert_eq!(state.pending_proposals[0].from, NationId(1));
        assert_eq!(state.pending_proposals[0].to, NationId(2));
    }

    #[test]
    fn propose_treaty_rejects_unsupported_types() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2)];
        state.initialize_great_powers(&gps);

        let result = state.propose_treaty(
            NationId(1),
            NationId(2),
            TreatyType::PeaceTreaty,
            TurnNumber::new(1),
        );
        assert!(result.is_err());

        let result = state.propose_treaty(
            NationId(1),
            NationId(2),
            TreatyType::WarDeclaration,
            TurnNumber::new(1),
        );
        assert!(result.is_err());

        let result = state.propose_treaty(
            NationId(1),
            NationId(2),
            TreatyType::RequestToJoinEmpire,
            TurnNumber::new(1),
        );
        assert!(result.is_err());
    }

    // ── Standing floor ──────────────────────────────────────────

    #[test]
    fn standing_floors_at_negative_100() {
        let mut state = DiplomacyState::new();
        // Starting standing is 100, reduce by 250
        state.reduce_standing(NationId(1), 250);
        assert_eq!(state.get_standing(NationId(1)), -100);

        // Further reduction should not go below -100
        state.reduce_standing(NationId(1), 50);
        assert_eq!(state.get_standing(NationId(1)), -100);
    }

    // ── At war with anyone ──────────────────────────────────────

    #[test]
    fn is_at_war_with_anyone_returns_false_when_peaceful() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2), NationId(3)];
        state.initialize_great_powers(&gps);

        assert!(!state.is_at_war_with_anyone(NationId(1)));
    }

    #[test]
    fn is_at_war_with_anyone_returns_true_when_at_war() {
        let mut state = DiplomacyState::new();
        let gps = vec![NationId(1), NationId(2), NationId(3)];
        state.initialize_great_powers(&gps);

        state.declare_war(NationId(1), NationId(2));
        assert!(state.is_at_war_with_anyone(NationId(1)));
        assert!(state.is_at_war_with_anyone(NationId(2)));
        assert!(!state.is_at_war_with_anyone(NationId(3)));
    }
}
