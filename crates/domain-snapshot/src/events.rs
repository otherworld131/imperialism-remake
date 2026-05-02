use crate::types::{NationId, ProvinceId};
use domain::events as d;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TechId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeadlineCategory {
    War,
    Battle,
    Diplomacy,
    Growth,
    Trade,
    Crisis,
    Politics,
    Military,
    Default,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Headline {
    pub text: String,
    pub category: HeadlineCategory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_non_action: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nation_ids: Vec<NationId>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TreatyType {
    NonAggressionPact,
    Alliance,
    RequestToJoinEmpire,
    PeaceTreaty,
    WarDeclaration,
    PactDefenseRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IncorporationReason {
    JoinedEmpire,
    VoluntarilyJoinedEmpire,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HistoryEvent {
    WarDeclared {
        attacker: NationId,
        defender: NationId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protectee: Option<NationId>,
    },
    JoinedWar {
        joiner: NationId,
        target: NationId,
    },
    PeaceMade {
        a: NationId,
        b: NationId,
    },
    PeaceSigned {
        a: NationId,
        b: NationId,
    },
    MutualPeace {
        a: NationId,
        b: NationId,
    },
    ProvinceConquered {
        conqueror: NationId,
        loser: NationId,
        province: ProvinceId,
    },
    TechnologyResearched {
        researcher: NationId,
        tech_name: String,
    },
    NonAggressionPactSigned {
        signer: NationId,
        partner: NationId,
    },
    AllianceFormed {
        signer: NationId,
        partner: NationId,
    },
    TreatyProposalAccepted {
        acceptor: NationId,
        proposer: NationId,
        treaty_type: TreatyType,
    },
    FellIntoAnarchy {
        nation: NationId,
    },
    RegainedIndependence {
        minor: NationId,
        former_overlord: NationId,
    },
    MinorJoinedEmpire {
        minor: NationId,
        overlord: NationId,
        reason: IncorporationReason,
    },
    ConsulateBuilt {
        player: NationId,
        target: NationId,
    },
    EmbassyBuilt {
        player: NationId,
        target: NationId,
    },
}

// ── From impls ────────────────────────────────────────────────────

impl From<d::TechId> for TechId {
    fn from(v: d::TechId) -> Self {
        Self(v.0)
    }
}
impl From<TechId> for d::TechId {
    fn from(v: TechId) -> Self {
        Self(v.0)
    }
}

impl From<d::HeadlineCategory> for HeadlineCategory {
    fn from(v: d::HeadlineCategory) -> Self {
        match v {
            d::HeadlineCategory::War => Self::War,
            d::HeadlineCategory::Battle => Self::Battle,
            d::HeadlineCategory::Diplomacy => Self::Diplomacy,
            d::HeadlineCategory::Growth => Self::Growth,
            d::HeadlineCategory::Trade => Self::Trade,
            d::HeadlineCategory::Crisis => Self::Crisis,
            d::HeadlineCategory::Politics => Self::Politics,
            d::HeadlineCategory::Military => Self::Military,
            d::HeadlineCategory::Default => Self::Default,
        }
    }
}
impl From<HeadlineCategory> for d::HeadlineCategory {
    fn from(v: HeadlineCategory) -> Self {
        match v {
            HeadlineCategory::War => Self::War,
            HeadlineCategory::Battle => Self::Battle,
            HeadlineCategory::Diplomacy => Self::Diplomacy,
            HeadlineCategory::Growth => Self::Growth,
            HeadlineCategory::Trade => Self::Trade,
            HeadlineCategory::Crisis => Self::Crisis,
            HeadlineCategory::Politics => Self::Politics,
            HeadlineCategory::Military => Self::Military,
            HeadlineCategory::Default => Self::Default,
        }
    }
}

impl From<&d::Headline> for Headline {
    fn from(v: &d::Headline) -> Self {
        Self {
            text: v.text.clone(),
            category: v.category.into(),
            reason: v.reason.clone(),
            is_non_action: v.is_non_action,
            nation_ids: v.nation_ids.iter().copied().map(Into::into).collect(),
        }
    }
}
impl From<Headline> for d::Headline {
    fn from(v: Headline) -> Self {
        Self {
            text: v.text,
            category: v.category.into(),
            reason: v.reason,
            is_non_action: v.is_non_action,
            nation_ids: v.nation_ids.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<d::TreatyType> for TreatyType {
    fn from(v: d::TreatyType) -> Self {
        match v {
            d::TreatyType::NonAggressionPact => Self::NonAggressionPact,
            d::TreatyType::Alliance => Self::Alliance,
            d::TreatyType::RequestToJoinEmpire => Self::RequestToJoinEmpire,
            d::TreatyType::PeaceTreaty => Self::PeaceTreaty,
            d::TreatyType::WarDeclaration => Self::WarDeclaration,
            d::TreatyType::PactDefenseRequest => Self::PactDefenseRequest,
        }
    }
}
impl From<TreatyType> for d::TreatyType {
    fn from(v: TreatyType) -> Self {
        match v {
            TreatyType::NonAggressionPact => Self::NonAggressionPact,
            TreatyType::Alliance => Self::Alliance,
            TreatyType::RequestToJoinEmpire => Self::RequestToJoinEmpire,
            TreatyType::PeaceTreaty => Self::PeaceTreaty,
            TreatyType::WarDeclaration => Self::WarDeclaration,
            TreatyType::PactDefenseRequest => Self::PactDefenseRequest,
        }
    }
}

impl From<d::IncorporationReason> for IncorporationReason {
    fn from(v: d::IncorporationReason) -> Self {
        match v {
            d::IncorporationReason::JoinedEmpire => Self::JoinedEmpire,
            d::IncorporationReason::VoluntarilyJoinedEmpire => Self::VoluntarilyJoinedEmpire,
        }
    }
}
impl From<IncorporationReason> for d::IncorporationReason {
    fn from(v: IncorporationReason) -> Self {
        match v {
            IncorporationReason::JoinedEmpire => Self::JoinedEmpire,
            IncorporationReason::VoluntarilyJoinedEmpire => Self::VoluntarilyJoinedEmpire,
        }
    }
}

impl From<&d::HistoryEvent> for HistoryEvent {
    fn from(v: &d::HistoryEvent) -> Self {
        match v {
            d::HistoryEvent::WarDeclared {
                attacker,
                defender,
                protectee,
            } => Self::WarDeclared {
                attacker: (*attacker).into(),
                defender: (*defender).into(),
                protectee: protectee.map(Into::into),
            },
            d::HistoryEvent::JoinedWar { joiner, target } => Self::JoinedWar {
                joiner: (*joiner).into(),
                target: (*target).into(),
            },
            d::HistoryEvent::PeaceMade { a, b } => Self::PeaceMade {
                a: (*a).into(),
                b: (*b).into(),
            },
            d::HistoryEvent::PeaceSigned { a, b } => Self::PeaceSigned {
                a: (*a).into(),
                b: (*b).into(),
            },
            d::HistoryEvent::MutualPeace { a, b } => Self::MutualPeace {
                a: (*a).into(),
                b: (*b).into(),
            },
            d::HistoryEvent::ProvinceConquered {
                conqueror,
                loser,
                province,
            } => Self::ProvinceConquered {
                conqueror: (*conqueror).into(),
                loser: (*loser).into(),
                province: (*province).into(),
            },
            d::HistoryEvent::TechnologyResearched {
                researcher,
                tech_name,
            } => Self::TechnologyResearched {
                researcher: (*researcher).into(),
                tech_name: tech_name.clone(),
            },
            d::HistoryEvent::NonAggressionPactSigned { signer, partner } => {
                Self::NonAggressionPactSigned {
                    signer: (*signer).into(),
                    partner: (*partner).into(),
                }
            }
            d::HistoryEvent::AllianceFormed { signer, partner } => Self::AllianceFormed {
                signer: (*signer).into(),
                partner: (*partner).into(),
            },
            d::HistoryEvent::TreatyProposalAccepted {
                acceptor,
                proposer,
                treaty_type,
            } => Self::TreatyProposalAccepted {
                acceptor: (*acceptor).into(),
                proposer: (*proposer).into(),
                treaty_type: (*treaty_type).into(),
            },
            d::HistoryEvent::FellIntoAnarchy { nation } => Self::FellIntoAnarchy {
                nation: (*nation).into(),
            },
            d::HistoryEvent::RegainedIndependence {
                minor,
                former_overlord,
            } => Self::RegainedIndependence {
                minor: (*minor).into(),
                former_overlord: (*former_overlord).into(),
            },
            d::HistoryEvent::MinorJoinedEmpire {
                minor,
                overlord,
                reason,
            } => Self::MinorJoinedEmpire {
                minor: (*minor).into(),
                overlord: (*overlord).into(),
                reason: (*reason).into(),
            },
            d::HistoryEvent::ConsulateBuilt { player, target } => Self::ConsulateBuilt {
                player: (*player).into(),
                target: (*target).into(),
            },
            d::HistoryEvent::EmbassyBuilt { player, target } => Self::EmbassyBuilt {
                player: (*player).into(),
                target: (*target).into(),
            },
        }
    }
}
impl From<HistoryEvent> for d::HistoryEvent {
    fn from(v: HistoryEvent) -> Self {
        match v {
            HistoryEvent::WarDeclared {
                attacker,
                defender,
                protectee,
            } => Self::WarDeclared {
                attacker: attacker.into(),
                defender: defender.into(),
                protectee: protectee.map(Into::into),
            },
            HistoryEvent::JoinedWar { joiner, target } => Self::JoinedWar {
                joiner: joiner.into(),
                target: target.into(),
            },
            HistoryEvent::PeaceMade { a, b } => Self::PeaceMade {
                a: a.into(),
                b: b.into(),
            },
            HistoryEvent::PeaceSigned { a, b } => Self::PeaceSigned {
                a: a.into(),
                b: b.into(),
            },
            HistoryEvent::MutualPeace { a, b } => Self::MutualPeace {
                a: a.into(),
                b: b.into(),
            },
            HistoryEvent::ProvinceConquered {
                conqueror,
                loser,
                province,
            } => Self::ProvinceConquered {
                conqueror: conqueror.into(),
                loser: loser.into(),
                province: province.into(),
            },
            HistoryEvent::TechnologyResearched {
                researcher,
                tech_name,
            } => Self::TechnologyResearched {
                researcher: researcher.into(),
                tech_name,
            },
            HistoryEvent::NonAggressionPactSigned { signer, partner } => {
                Self::NonAggressionPactSigned {
                    signer: signer.into(),
                    partner: partner.into(),
                }
            }
            HistoryEvent::AllianceFormed { signer, partner } => Self::AllianceFormed {
                signer: signer.into(),
                partner: partner.into(),
            },
            HistoryEvent::TreatyProposalAccepted {
                acceptor,
                proposer,
                treaty_type,
            } => Self::TreatyProposalAccepted {
                acceptor: acceptor.into(),
                proposer: proposer.into(),
                treaty_type: treaty_type.into(),
            },
            HistoryEvent::FellIntoAnarchy { nation } => Self::FellIntoAnarchy {
                nation: nation.into(),
            },
            HistoryEvent::RegainedIndependence {
                minor,
                former_overlord,
            } => Self::RegainedIndependence {
                minor: minor.into(),
                former_overlord: former_overlord.into(),
            },
            HistoryEvent::MinorJoinedEmpire {
                minor,
                overlord,
                reason,
            } => Self::MinorJoinedEmpire {
                minor: minor.into(),
                overlord: overlord.into(),
                reason: reason.into(),
            },
            HistoryEvent::ConsulateBuilt { player, target } => Self::ConsulateBuilt {
                player: player.into(),
                target: target.into(),
            },
            HistoryEvent::EmbassyBuilt { player, target } => Self::EmbassyBuilt {
                player: player.into(),
                target: target.into(),
            },
        }
    }
}
