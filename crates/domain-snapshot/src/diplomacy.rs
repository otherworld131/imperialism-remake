use crate::events::TreatyType;
use crate::types::{NationId, TurnNumber};
use domain::diplomacy as d;

// ── DiplomaticProposal ───────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiplomaticProposal {
    pub from: NationId,
    pub to: NationId,
    pub proposal_type: TreatyType,
    pub turn_proposed: TurnNumber,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attacker: Option<NationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cascade_remaining: Option<Vec<NationId>>,
}

// ── DiplomaticRelation ───────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiplomaticRelation {
    pub nation_a: NationId,
    pub nation_b: NationId,
    pub score: i32,
    pub has_consulate: bool,
    pub has_embassy: bool,
    pub active_treaties: Vec<TreatyType>,
    pub at_war: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_war_declared: Option<TurnNumber>,
}

// ── DiplomacyState ───────────────────────────────────────────────────

/// Tuple key `(NationId, NationId)` can't be a JSON object key, so relations
/// are stored as a flat Vec of ((a, b), relation) pairs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiplomacyState {
    pub relations: Vec<((u32, u32), DiplomaticRelation)>,
    pub standing: Vec<(u32, i32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_proposals: Vec<DiplomaticProposal>,
    /// (attacker, minor) pairs — serialized as flat Vec of (u32, u32).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pact_defense_requested: Vec<(u32, u32)>,
}

// ═══════════════════════════════════════════════════════════════════════
// From impls
// ═══════════════════════════════════════════════════════════════════════

impl From<&d::DiplomaticProposal> for DiplomaticProposal {
    fn from(v: &d::DiplomaticProposal) -> Self {
        Self {
            from: v.from.into(),
            to: v.to.into(),
            proposal_type: v.proposal_type.into(),
            turn_proposed: v.turn_proposed.into(),
            attacker: v.attacker.map(Into::into),
            cascade_remaining: v
                .cascade_remaining
                .as_ref()
                .map(|list| list.iter().copied().map(Into::into).collect()),
        }
    }
}
impl From<DiplomaticProposal> for d::DiplomaticProposal {
    fn from(v: DiplomaticProposal) -> Self {
        use domain::types::NationId as DN;
        Self {
            from: v.from.into(),
            to: v.to.into(),
            proposal_type: v.proposal_type.into(),
            turn_proposed: v.turn_proposed.into(),
            attacker: v.attacker.map(Into::into),
            cascade_remaining: v
                .cascade_remaining
                .map(|list| list.into_iter().map(|n: NationId| DN(n.0)).collect()),
        }
    }
}

impl From<&d::DiplomaticRelation> for DiplomaticRelation {
    fn from(v: &d::DiplomaticRelation) -> Self {
        Self {
            nation_a: v.nation_a.into(),
            nation_b: v.nation_b.into(),
            score: v.score,
            has_consulate: v.has_consulate,
            has_embassy: v.has_embassy,
            active_treaties: v.active_treaties.iter().copied().map(Into::into).collect(),
            at_war: v.at_war,
            turn_war_declared: v.turn_war_declared.map(Into::into),
        }
    }
}
impl From<DiplomaticRelation> for d::DiplomaticRelation {
    fn from(v: DiplomaticRelation) -> Self {
        Self {
            nation_a: v.nation_a.into(),
            nation_b: v.nation_b.into(),
            score: v.score,
            has_consulate: v.has_consulate,
            has_embassy: v.has_embassy,
            active_treaties: v.active_treaties.into_iter().map(Into::into).collect(),
            at_war: v.at_war,
            turn_war_declared: v.turn_war_declared.map(Into::into),
        }
    }
}

impl From<&d::DiplomacyState> for DiplomacyState {
    fn from(v: &d::DiplomacyState) -> Self {
        Self {
            relations: v
                .all_relations()
                .map(|((a, b), rel)| ((a.0, b.0), rel.into()))
                .collect(),
            standing: v.standing.iter().map(|(k, val)| (k.0, *val)).collect(),
            pending_proposals: v.pending_proposals.iter().map(Into::into).collect(),
            pact_defense_requested: v.pact_defense_pairs().map(|(a, b)| (a.0, b.0)).collect(),
        }
    }
}
impl From<DiplomacyState> for d::DiplomacyState {
    fn from(v: DiplomacyState) -> Self {
        use domain::types::NationId as DN;
        use std::collections::{BTreeMap, HashMap, HashSet};
        let relations: BTreeMap<(DN, DN), d::DiplomaticRelation> = v
            .relations
            .into_iter()
            .map(|((a, b), rel)| ((DN(a), DN(b)), rel.into()))
            .collect();
        let standing: HashMap<DN, i32> = v
            .standing
            .into_iter()
            .map(|(k, val)| (DN(k), val))
            .collect();
        let pending_proposals = v.pending_proposals.into_iter().map(Into::into).collect();
        let pact_defense_requested: HashSet<(DN, DN)> = v
            .pact_defense_requested
            .into_iter()
            .map(|(a, b)| (DN(a), DN(b)))
            .collect();
        d::DiplomacyState::from_raw(
            relations,
            standing,
            pending_proposals,
            pact_defense_requested,
        )
    }
}
