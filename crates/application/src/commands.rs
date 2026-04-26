//! Typed commands — the "command" side of CQRS.
//! Every mutation the frontend can request is represented here.
//! Commands are dispatched via `apply_command` in `session.rs` (wasm-bridge).

use domain::types::{NationId, ProvinceId};

/// A mutation the frontend sends to the application layer.
/// Tagged union serialized with `"type"` discriminant for easy JSON dispatch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FrontendCommand {
    // ── Turn progression ──────────────────────────────────────────────────
    EndTurn,

    // ── Military ─────────────────────────────────────────────────────────
    QueueUnitMove {
        unit_id: u32,
        target_province: ProvinceId,
    },
    CancelUnitMove {
        unit_id: u32,
    },
    DisbandUnit {
        unit_id: u32,
    },
    RecruitArmyUnit {
        nation_id: NationId,
        unit_type: String,
    },
    AssignBeachhead {
        nation_id: NationId,
        target_province: ProvinceId,
    },
    SetShipOperation {
        ship_id: u32,
        operation: String,
    },
    BuildShip {
        nation_id: NationId,
        ship_type: String,
    },

    // ── Civilians ─────────────────────────────────────────────────────────
    HireCivilian {
        nation_id: NationId,
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
        nation_id: NationId,
        building_type: String,
    },
    BuildFreightCar {
        nation_id: NationId,
    },
    SetTransportAllocation {
        nation_id: NationId,
        commodity: String,
        amount: u32,
    },
    SetPlayerSellOrder {
        nation_id: NationId,
        commodity: String,
        quantity: u32,
        price_cents: u32,
    },
    SetPlayerBuyOrder {
        nation_id: NationId,
        commodity: String,
        quantity: u32,
        price_cents: u32,
    },
    SetTradeSubsidy {
        from_nation: NationId,
        to_nation: NationId,
        subsidy_dollars: u32,
    },

    // ── Technology ───────────────────────────────────────────────────────
    ResearchTech {
        tech_name: String,
    },

    // ── Diplomacy ────────────────────────────────────────────────────────
    DiplomacyBuildConsulate {
        player: NationId,
        target: NationId,
    },
    DiplomacyBuildEmbassy {
        player: NationId,
        target: NationId,
    },
    DiplomacyProposeNap {
        from: NationId,
        to: NationId,
    },
    DiplomacyProposeAlliance {
        from: NationId,
        to: NationId,
    },
    DiplomacyDeclareWar {
        from: NationId,
        to: NationId,
    },
    DiplomacySendGrant {
        from: NationId,
        to: NationId,
        amount_dollars: u32,
    },
    DiplomacyBreakTreaty {
        from: NationId,
        to: NationId,
    },
    DiplomacyProposePeace {
        from: NationId,
        to: NationId,
    },
    AcceptProposal {
        nation_id: NationId,
        proposal_index: u32,
    },
    RejectProposal {
        nation_id: NationId,
        proposal_index: u32,
    },
}

/// Result returned after applying a `FrontendCommand`.
/// A successful command may carry an optional message (e.g., error details for
/// `ok: false`, or a human-readable confirmation for `ok: true`).
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
