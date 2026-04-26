//! Typed commands — the "command" side of CQRS.
//! Every mutation the frontend can request is represented here.
//! Commands are dispatched via `apply_command` in `session.rs` (wasm-bridge).

/// A mutation the frontend sends to the application layer.
/// Tagged union serialized with `"type"` discriminant for easy JSON dispatch.
///
/// Domain IDs (NationId, ProvinceId) are represented as raw u32 here to keep
/// serde out of the domain crate. Handlers wrap them back into typed IDs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FrontendCommand {
    // ── Turn progression ──────────────────────────────────────────────────
    EndTurn,

    // ── Military ─────────────────────────────────────────────────────────
    QueueUnitMove {
        unit_id: u32,
        target_province: u32,
    },
    CancelUnitMove {
        unit_id: u32,
    },
    DisbandUnit {
        unit_id: u32,
    },
    RecruitArmyUnit {
        nation_id: u32,
        unit_type: String,
    },
    AssignBeachhead {
        nation_id: u32,
        target_province: u32,
    },
    SetShipOperation {
        ship_id: u32,
        operation: String,
    },
    BuildShip {
        nation_id: u32,
        ship_type: String,
    },

    // ── Civilians ─────────────────────────────────────────────────────────
    HireCivilian {
        nation_id: u32,
        civilian_type: String,
    },
    DeployCivilian {
        civilian_id: u32,
        hex_q: i32,
        hex_r: i32,
    },
    RecallCivilian {
        civilian_id: u32,
    },
    EngineerBuild {
        civilian_id: u32,
        build_kind: String,
    },

    // ── Economy ───────────────────────────────────────────────────────────
    ExpandBuilding {
        nation_id: u32,
        building_type: String,
    },
    BuildFreightCar {
        nation_id: u32,
    },
    SetTransportAllocation {
        nation_id: u32,
        commodity: String,
        amount: u32,
    },
    SetPlayerSellOrder {
        nation_id: u32,
        commodity: String,
        quantity: u32,
        price_cents: u32,
    },
    SetPlayerBuyOrder {
        nation_id: u32,
        commodity: String,
        quantity: u32,
        price_cents: u32,
    },
    SetTradeSubsidy {
        from_nation: u32,
        to_nation: u32,
        subsidy_dollars: u32,
    },

    // ── Technology ───────────────────────────────────────────────────────
    ResearchTech {
        tech_name: String,
    },

    // ── Diplomacy ────────────────────────────────────────────────────────
    DiplomacyBuildConsulate {
        player: u32,
        target: u32,
    },
    DiplomacyBuildEmbassy {
        player: u32,
        target: u32,
    },
    DiplomacyProposeNap {
        from: u32,
        to: u32,
    },
    DiplomacyProposeAlliance {
        from: u32,
        to: u32,
    },
    DiplomacyDeclareWar {
        from: u32,
        to: u32,
    },
    DiplomacySendGrant {
        from: u32,
        to: u32,
        amount_dollars: u32,
    },
    DiplomacyBreakTreaty {
        from: u32,
        to: u32,
    },
    DiplomacyProposePeace {
        from: u32,
        to: u32,
    },
    AcceptProposal {
        nation_id: u32,
        proposal_index: u32,
    },
    RejectProposal {
        nation_id: u32,
        proposal_index: u32,
    },
}

/// Result returned after applying a `FrontendCommand`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl CommandResult {
    pub fn success() -> Self {
        Self { ok: true, message: None }
    }
    pub fn error(msg: impl Into<String>) -> Self {
        Self { ok: false, message: Some(msg.into()) }
    }
}
