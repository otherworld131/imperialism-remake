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
    /// Nation IDs involved in this headline. Used by the newspaper filter to
    /// match by ID instead of text substring.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nation_ids: Vec<crate::types::NationId>,
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
            nation_ids: Vec::new(),
        }
    }

    pub fn with_reason(text: String, category: HeadlineCategory, reason: String) -> Self {
        Self {
            text,
            category,
            reason: Some(reason),
            is_non_action: false,
            nation_ids: Vec::new(),
        }
    }

    pub fn non_action(text: String, category: HeadlineCategory, reason: String) -> Self {
        Self {
            text,
            category,
            reason: Some(reason),
            is_non_action: true,
            nation_ids: Vec::new(),
        }
    }

    pub fn for_nation(mut self, id: crate::types::NationId) -> Self {
        self.nation_ids.push(id);
        self
    }

    pub fn for_nations(mut self, ids: &[crate::types::NationId]) -> Self {
        self.nation_ids.extend_from_slice(ids);
        self
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

/// A previously integrated minor nation regained its independence because
/// the overlord great power fell into anarchy (card #79).
#[derive(Debug, Clone, PartialEq)]
pub struct MinorRegainedIndependence {
    pub minor: NationId,
    pub former_overlord: NationId,
}

// ── Persistent history events ──────────────────────────────────

/// Reason a minor nation became part of a great power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IncorporationReason {
    /// Forced via pact-defense protection cascade.
    JoinedEmpire,
    /// Voluntary incorporation due to high diplomatic score.
    VoluntarilyJoinedEmpire,
}

/// A typed entry in `GameState::history`.
///
/// AI logic pattern-matches these to reason about past events instead of
/// grepping strings. UI/CLI rendering goes through `render(&GameState)` so
/// the player-facing wording stays consistent and nation/province names are
/// looked up live (a nation rename or province change would otherwise leave
/// stale text in the log).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HistoryEvent {
    /// "{attacker} declared war on {defender}", optionally "to protect {protectee}".
    WarDeclared {
        attacker: NationId,
        defender: NationId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protectee: Option<NationId>,
    },
    /// "{joiner} joined war against {target} (alliance obligation)".
    JoinedWar {
        joiner: NationId,
        target: NationId,
    },
    /// "{a} made peace with {b}" — AI auto-accept against a passive minor.
    PeaceMade { a: NationId, b: NationId },
    /// "{a} signed peace with {b}" — human-driven CLI peace command.
    PeaceSigned { a: NationId, b: NationId },
    /// "{a} and {b} agreed to mutual peace" — both sides voluntarily ended a war.
    MutualPeace { a: NationId, b: NationId },
    /// "{conqueror} conquered {province} from {loser}".
    ProvinceConquered {
        conqueror: NationId,
        loser: NationId,
        province: ProvinceId,
    },
    /// "{researcher} researched {tech_name}". Tech name is stored verbatim
    /// because the tech tree may not be loaded when rendering.
    TechnologyResearched {
        researcher: NationId,
        tech_name: String,
    },
    /// "{signer} signed a non-aggression pact with {partner}".
    NonAggressionPactSigned {
        signer: NationId,
        partner: NationId,
    },
    /// "{signer} formed an alliance with {partner}".
    AllianceFormed {
        signer: NationId,
        partner: NationId,
    },
    /// "{acceptor} accepted {proposer}'s {treaty_type} proposal".
    TreatyProposalAccepted {
        acceptor: NationId,
        proposer: NationId,
        treaty_type: TreatyType,
    },
    /// "{nation} fell into anarchy".
    FellIntoAnarchy { nation: NationId },
    /// "{minor} regained independence after {former_overlord} fell into anarchy".
    RegainedIndependence {
        minor: NationId,
        former_overlord: NationId,
    },
    /// "{minor} joined the empire of {overlord}" or "voluntarily joined the empire of".
    MinorJoinedEmpire {
        minor: NationId,
        overlord: NationId,
        reason: IncorporationReason,
    },
    /// "Trade consulate built with {target}".
    ConsulateBuilt {
        player: NationId,
        target: NationId,
    },
    /// "Embassy built with {target}".
    EmbassyBuilt {
        player: NationId,
        target: NationId,
    },
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
    MinorRegainedIndependence(MinorRegainedIndependence),
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
