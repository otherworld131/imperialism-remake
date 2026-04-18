use crate::types::*;

// ── TechId ─────────────────────────────────────────────────────

/// Identifies a technology in the tech tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TechId(pub u32);

// ── Headline categories ─────────────────────────────────────────

/// Category for newspaper headlines, used for color-coded display.
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

// ── Headline struct ────────────────────────────────────────────

/// A newspaper headline with optional AI reasoning.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Headline {
    pub text: String,
    pub category: HeadlineCategory,
    /// AI decision rationale; `None` for non-AI headlines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// When `true`, this headline describes a decision the AI *declined* to make
    /// (e.g., "X did not declare war this turn"). Hidden from the newspaper by
    /// default; revealed by the "Show AI non-actions" debug toggle.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_non_action: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Headline {
    pub fn new(text: String, category: HeadlineCategory) -> Self {
        Self {
            text,
            category,
            reason: None,
            is_non_action: false,
        }
    }

    pub fn with_reason(text: String, category: HeadlineCategory, reason: String) -> Self {
        Self {
            text,
            category,
            reason: Some(reason),
            is_non_action: false,
        }
    }

    pub fn non_action(text: String, category: HeadlineCategory, reason: String) -> Self {
        Self {
            text,
            category,
            reason: Some(reason),
            is_non_action: true,
        }
    }
}

// ── Treaty & Victory enums ─────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TreatyType {
    NonAggressionPact,
    Alliance,
    RequestToJoinEmpire,
    PeaceTreaty,
    WarDeclaration,
    PactDefenseRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum VictoryType {
    CouncilVote,
    CouncilDefault,
    Conquest,
}

// ── Domain events ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TurnStarted {
    pub turn: TurnNumber,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TurnEnded {
    pub turn: TurnNumber,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TechnologyResearched {
    pub nation: NationId,
    pub tech: TechId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WarDeclared {
    pub attacker: NationId,
    pub defender: NationId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreatyProposed {
    pub from: NationId,
    pub to: NationId,
    pub treaty_type: TreatyType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreatyAccepted {
    pub from: NationId,
    pub to: NationId,
    pub treaty_type: TreatyType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreatyRejected {
    pub from: NationId,
    pub to: NationId,
    pub treaty_type: TreatyType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProvinceConquered {
    pub province: ProvinceId,
    pub old_owner: NationId,
    pub new_owner: NationId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnitCreated {
    pub nation: NationId,
    pub unit_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnitDestroyed {
    pub nation: NationId,
    pub unit_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradeCompleted {
    pub buyer: NationId,
    pub seller: NationId,
    pub amount: Money,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildingConstructed {
    pub nation: NationId,
    pub building_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VictoryAchieved {
    pub winner: NationId,
    pub victory_type: VictoryType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NationIncorporated {
    pub minor_nation: NationId,
    pub great_power: NationId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnitUpgraded {
    pub nation: NationId,
    pub from_type: String,
    pub to_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NationEnteredAnarchy {
    pub nation: NationId,
}

// ── Wrapper enum ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DomainEvent {
    TurnStarted(TurnStarted),
    TurnEnded(TurnEnded),
    TechnologyResearched(TechnologyResearched),
    WarDeclared(WarDeclared),
    TreatyProposed(TreatyProposed),
    TreatyAccepted(TreatyAccepted),
    TreatyRejected(TreatyRejected),
    ProvinceConquered(ProvinceConquered),
    UnitCreated(UnitCreated),
    UnitDestroyed(UnitDestroyed),
    TradeCompleted(TradeCompleted),
    BuildingConstructed(BuildingConstructed),
    VictoryAchieved(VictoryAchieved),
    NationIncorporated(NationIncorporated),
    UnitUpgraded(UnitUpgraded),
    NationEnteredAnarchy(NationEnteredAnarchy),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_started_event() {
        let event = TurnStarted {
            turn: TurnNumber::new(1),
        };
        let wrapped = DomainEvent::TurnStarted(event.clone());
        assert_eq!(event.turn, TurnNumber::new(1));
        assert_eq!(wrapped, DomainEvent::TurnStarted(event));
    }

    #[test]
    fn turn_ended_event() {
        let event = TurnEnded {
            turn: TurnNumber::new(5),
        };
        let wrapped = DomainEvent::TurnEnded(event.clone());
        assert_eq!(event.turn, TurnNumber::new(5));
        assert_eq!(wrapped, DomainEvent::TurnEnded(event));
    }

    #[test]
    fn technology_researched_event() {
        let event = TechnologyResearched {
            nation: NationId(1),
            tech: TechId(42),
        };
        let wrapped = DomainEvent::TechnologyResearched(event.clone());
        assert_eq!(event.nation, NationId(1));
        assert_eq!(event.tech, TechId(42));
        assert_eq!(wrapped, DomainEvent::TechnologyResearched(event));
    }

    #[test]
    fn war_declared_event() {
        let event = WarDeclared {
            attacker: NationId(1),
            defender: NationId(2),
        };
        let wrapped = DomainEvent::WarDeclared(event.clone());
        assert_eq!(event.attacker, NationId(1));
        assert_eq!(event.defender, NationId(2));
        assert_eq!(wrapped, DomainEvent::WarDeclared(event));
    }

    #[test]
    fn treaty_proposed_event() {
        let event = TreatyProposed {
            from: NationId(1),
            to: NationId(3),
            treaty_type: TreatyType::Alliance,
        };
        let wrapped = DomainEvent::TreatyProposed(event.clone());
        assert_eq!(event.treaty_type, TreatyType::Alliance);
        assert_eq!(wrapped, DomainEvent::TreatyProposed(event));
    }

    #[test]
    fn treaty_accepted_event() {
        let event = TreatyAccepted {
            from: NationId(2),
            to: NationId(4),
            treaty_type: TreatyType::NonAggressionPact,
        };
        let wrapped = DomainEvent::TreatyAccepted(event.clone());
        assert_eq!(event.treaty_type, TreatyType::NonAggressionPact);
        assert_eq!(wrapped, DomainEvent::TreatyAccepted(event));
    }

    #[test]
    fn treaty_rejected_event() {
        let event = TreatyRejected {
            from: NationId(5),
            to: NationId(6),
            treaty_type: TreatyType::RequestToJoinEmpire,
        };
        let wrapped = DomainEvent::TreatyRejected(event.clone());
        assert_eq!(event.treaty_type, TreatyType::RequestToJoinEmpire);
        assert_eq!(wrapped, DomainEvent::TreatyRejected(event));
    }

    #[test]
    fn province_conquered_event() {
        let event = ProvinceConquered {
            province: ProvinceId(10),
            old_owner: NationId(1),
            new_owner: NationId(3),
        };
        let wrapped = DomainEvent::ProvinceConquered(event.clone());
        assert_eq!(event.province, ProvinceId(10));
        assert_eq!(event.old_owner, NationId(1));
        assert_eq!(event.new_owner, NationId(3));
        assert_eq!(wrapped, DomainEvent::ProvinceConquered(event));
    }

    #[test]
    fn unit_created_event() {
        let event = UnitCreated {
            nation: NationId(1),
            unit_type: "Infantry".to_string(),
        };
        let wrapped = DomainEvent::UnitCreated(event.clone());
        assert_eq!(event.unit_type, "Infantry");
        assert_eq!(wrapped, DomainEvent::UnitCreated(event));
    }

    #[test]
    fn unit_destroyed_event() {
        let event = UnitDestroyed {
            nation: NationId(2),
            unit_type: "Cavalry".to_string(),
        };
        let wrapped = DomainEvent::UnitDestroyed(event.clone());
        assert_eq!(event.unit_type, "Cavalry");
        assert_eq!(wrapped, DomainEvent::UnitDestroyed(event));
    }

    #[test]
    fn trade_completed_event() {
        let event = TradeCompleted {
            buyer: NationId(1),
            seller: NationId(4),
            amount: Money::dollars(500),
        };
        let wrapped = DomainEvent::TradeCompleted(event.clone());
        assert_eq!(event.amount, Money::dollars(500));
        assert_eq!(wrapped, DomainEvent::TradeCompleted(event));
    }

    #[test]
    fn building_constructed_event() {
        let event = BuildingConstructed {
            nation: NationId(3),
            building_type: "SteelMill".to_string(),
        };
        let wrapped = DomainEvent::BuildingConstructed(event.clone());
        assert_eq!(event.building_type, "SteelMill");
        assert_eq!(wrapped, DomainEvent::BuildingConstructed(event));
    }

    #[test]
    fn victory_achieved_event() {
        let event = VictoryAchieved {
            winner: NationId(7),
            victory_type: VictoryType::CouncilVote,
        };
        let wrapped = DomainEvent::VictoryAchieved(event.clone());
        assert_eq!(event.victory_type, VictoryType::CouncilVote);
        assert_eq!(wrapped, DomainEvent::VictoryAchieved(event));
    }

    #[test]
    fn treaty_type_variants() {
        let variants = [
            TreatyType::NonAggressionPact,
            TreatyType::Alliance,
            TreatyType::RequestToJoinEmpire,
            TreatyType::PeaceTreaty,
            TreatyType::WarDeclaration,
        ];
        // All variants are distinct.
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn victory_type_variants() {
        let variants = [
            VictoryType::CouncilVote,
            VictoryType::CouncilDefault,
            VictoryType::Conquest,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn tech_id_equality() {
        assert_eq!(TechId(1), TechId(1));
        assert_ne!(TechId(1), TechId(2));
    }

    #[test]
    fn all_domain_event_types_can_be_created() {
        let events = [
            DomainEvent::TurnStarted(TurnStarted {
                turn: TurnNumber::new(1),
            }),
            DomainEvent::TurnEnded(TurnEnded {
                turn: TurnNumber::new(1),
            }),
            DomainEvent::TechnologyResearched(TechnologyResearched {
                nation: NationId(1),
                tech: TechId(1),
            }),
            DomainEvent::WarDeclared(WarDeclared {
                attacker: NationId(1),
                defender: NationId(2),
            }),
            DomainEvent::ProvinceConquered(ProvinceConquered {
                province: ProvinceId(1),
                old_owner: NationId(2),
                new_owner: NationId(1),
            }),
        ];
        assert_eq!(events.len(), 5);
    }

    // ── Headline struct ───────────────────────────────────────

    #[test]
    fn headline_new_has_no_reason() {
        let h = Headline::new("Trade flourishes".to_string(), HeadlineCategory::Trade);
        assert_eq!(h.text, "Trade flourishes");
        assert_eq!(h.category, HeadlineCategory::Trade);
        assert!(h.reason.is_none());
    }

    #[test]
    fn headline_with_reason_carries_reason() {
        let h = Headline::with_reason(
            "Scientists in X have discovered Y!".to_string(),
            HeadlineCategory::Default,
            "Aggressive personality selected military tech".to_string(),
        );
        assert_eq!(
            h.reason.as_deref(),
            Some("Aggressive personality selected military tech")
        );
    }

    #[test]
    fn headline_serializes_omits_reason_when_none() {
        let h = Headline::new("Plain headline".to_string(), HeadlineCategory::Default);
        let json = serde_json::to_string(&h).expect("serialize");
        // skip_serializing_if should omit the field entirely when None
        assert!(
            !json.contains("reason"),
            "Expected reason omitted, got: {}",
            json
        );
        assert!(json.contains("\"text\":\"Plain headline\""));
    }

    #[test]
    fn headline_serializes_includes_reason_when_some() {
        let h = Headline::with_reason(
            "Country X declared war".to_string(),
            HeadlineCategory::War,
            "Need score 2.3 > threshold 1.5".to_string(),
        );
        let json = serde_json::to_string(&h).expect("serialize");
        assert!(json.contains("\"reason\":\"Need score 2.3 > threshold 1.5\""));
    }

    #[test]
    fn headline_deserializes_without_reason_field() {
        // Saves produced by this codebase never emit the field when None; make sure
        // round-tripping a reason-less JSON still works.
        let json = r#"{"text":"Test","category":"default"}"#;
        let h: Headline = serde_json::from_str(json).expect("deserialize");
        assert_eq!(h.text, "Test");
        assert!(h.reason.is_none());
    }
}
