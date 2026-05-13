use crate::diplomacy::DiplomacyState;
use crate::economy::{CashFlow, MarketState, ResourceFlow};
use crate::events::{Headline, HistoryEvent, TreatyType};
use crate::map::{HexMap, Province};
use crate::military::{BattleResult, NavalBattleResult};
use crate::nation::Nation;
use crate::types::{Difficulty, Money, NationId, ProvinceId, ResourceType, TurnNumber};
use domain::game_state as dgs;

// ── MarketTurnRecord ─────────────────────────────────────────────────

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MarketTurnRecord {
    pub offers: Vec<MarketOfferRecord>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarketOfferRecord {
    pub seller: NationId,
    pub resource: ResourceType,
    pub offered: u32,
    pub price_per_unit: Money,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<MarketFillRecord>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarketFillRecord {
    pub buyer: NationId,
    pub quantity: u32,
    pub price_per_unit: Money,
}

impl From<&dgs::MarketTurnRecord> for MarketTurnRecord {
    fn from(v: &dgs::MarketTurnRecord) -> Self {
        Self {
            offers: v.offers.iter().map(Into::into).collect(),
        }
    }
}

impl From<MarketTurnRecord> for dgs::MarketTurnRecord {
    fn from(v: MarketTurnRecord) -> Self {
        Self {
            offers: v.offers.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<&dgs::MarketOfferRecord> for MarketOfferRecord {
    fn from(v: &dgs::MarketOfferRecord) -> Self {
        Self {
            seller: v.seller.into(),
            resource: v.resource.into(),
            offered: v.offered,
            price_per_unit: v.price_per_unit.into(),
            fills: v.fills.iter().map(Into::into).collect(),
        }
    }
}

impl From<MarketOfferRecord> for dgs::MarketOfferRecord {
    fn from(v: MarketOfferRecord) -> Self {
        Self {
            seller: v.seller.into(),
            resource: v.resource.into(),
            offered: v.offered,
            price_per_unit: v.price_per_unit.into(),
            fills: v.fills.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<&dgs::MarketFillRecord> for MarketFillRecord {
    fn from(v: &dgs::MarketFillRecord) -> Self {
        Self {
            buyer: v.buyer.into(),
            quantity: v.quantity,
            price_per_unit: v.price_per_unit.into(),
        }
    }
}

impl From<MarketFillRecord> for dgs::MarketFillRecord {
    fn from(v: MarketFillRecord) -> Self {
        Self {
            buyer: v.buyer.into(),
            quantity: v.quantity,
            price_per_unit: v.price_per_unit.into(),
        }
    }
}

// ── PoliticalSnapshot ────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PoliticalSnapshot {
    pub provinces: Vec<(ProvinceId, NationId, Option<NationId>)>,
    pub capitals: Vec<(NationId, ProvinceId)>,
}

// ── WorldState ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorldState {
    pub map_key: String,
    pub hex_map: HexMap,
    pub provinces: Vec<Province>,
    pub nations: Vec<Nation>,
    pub diplomacy: DiplomacyState,
    #[serde(default)]
    pub market_state: MarketState,
}

// ── GameArchive ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GameArchive {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<(TurnNumber, HistoryEvent)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub high_scores: Vec<(String, u32, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub newspaper_archive: Vec<(TurnNumber, Vec<Headline>)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub battle_archive: Vec<(TurnNumber, Vec<BattleResult>, Vec<NavalBattleResult>)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub political_archive: Vec<(TurnNumber, PoliticalSnapshot)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub market_archive: Vec<(TurnNumber, MarketTurnRecord)>,
}

// ── TransientState ───────────────────────────────────────────────────
/// `events`, `pending_ai_cash_spending`, and `pending_ai_cash_income` are
/// marked `#[serde(skip)]` in domain — they are not included in the snapshot.

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TransientState {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_attacks: Vec<(NationId, ProvinceId)>,
    /// (nation, unit_id_raw, destination_province)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_moves: Vec<(NationId, u32, ProvinceId)>,
    /// (nation, from_sea_zone_id, to_sea_zone_id) — queued fleet moves to
    /// resolve at end of turn (card #471).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_fleet_moves: Vec<(NationId, u32, u32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_landings: Vec<(NationId, ProvinceId, TurnNumber)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_diplomacy_actions: Vec<PendingDiplomacyAction>,
    /// Stored as Vec instead of HashMap to avoid non-string key issues.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub last_cash_flow: Vec<(NationId, CashFlow)>,
    /// Stored as Vec instead of HashMap to avoid non-string key issues.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub last_resource_flow: Vec<(NationId, ResourceFlow)>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PendingDiplomacyAction {
    BuildConsulate { player: NationId, target: NationId },
    BuildEmbassy { player: NationId, target: NationId },
    DeclareWar { from: NationId, to: NationId },
    SendGrant {
        from: NationId,
        to: NationId,
        amount: Money,
    },
    BreakTreaty {
        from: NationId,
        to: NationId,
        treaty_type: TreatyType,
    },
}

// ── GameState ────────────────────────────────────────────────────────
/// `ai_debug` and `game_data` are skipped — reconstructed by infrastructure on load.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GameState {
    pub turn: TurnNumber,
    pub difficulty: Difficulty,
    pub human_player_nation: NationId,
    #[serde(default)]
    pub observer_mode: bool,
    #[serde(default = "default_next_unit_id")]
    pub next_unit_id: u32,
    #[serde(default = "default_rng_state")]
    pub rng_state: u64,
    pub world: WorldState,
    #[serde(default)]
    pub archive: GameArchive,
    #[serde(default)]
    pub transient: TransientState,
}

fn default_next_unit_id() -> u32 {
    6_000_000
}

fn default_rng_state() -> u64 {
    domain::game_state::DEFAULT_RNG_STATE
}

// ═══════════════════════════════════════════════════════════════════════
// From impls
// ═══════════════════════════════════════════════════════════════════════

impl From<&dgs::PoliticalSnapshot> for PoliticalSnapshot {
    fn from(v: &dgs::PoliticalSnapshot) -> Self {
        Self {
            provinces: v
                .provinces
                .iter()
                .map(|(p, n, i)| ((*p).into(), (*n).into(), i.map(Into::into)))
                .collect(),
            capitals: v
                .capitals
                .iter()
                .map(|(n, p)| ((*n).into(), (*p).into()))
                .collect(),
        }
    }
}
impl From<PoliticalSnapshot> for dgs::PoliticalSnapshot {
    fn from(v: PoliticalSnapshot) -> Self {
        Self {
            provinces: v
                .provinces
                .into_iter()
                .map(|(p, n, i)| (p.into(), n.into(), i.map(Into::into)))
                .collect(),
            capitals: v
                .capitals
                .into_iter()
                .map(|(n, p)| (n.into(), p.into()))
                .collect(),
        }
    }
}

impl From<&dgs::WorldState> for WorldState {
    fn from(v: &dgs::WorldState) -> Self {
        Self {
            map_key: v.map_key.clone(),
            hex_map: (&v.hex_map).into(),
            provinces: v.provinces.iter().map(Into::into).collect(),
            nations: v.nations.iter().map(Into::into).collect(),
            diplomacy: (&v.diplomacy).into(),
            market_state: (&v.market_state).into(),
        }
    }
}
impl From<WorldState> for dgs::WorldState {
    fn from(v: WorldState) -> Self {
        let hex_map: domain::map::HexMap = v.hex_map.into();
        let mut provinces: Vec<domain::map::Province> =
            v.provinces.into_iter().map(Into::into).collect();
        // Recompute sea zones from the hex map (not serialized — derived from hex_map).
        let mut sea_zones = domain::map::compute_sea_zones(&hex_map);
        domain::map::assign_coastal_provinces(&mut sea_zones, &provinces, &hex_map);
        for province in &mut provinces {
            province.ocean_coastal = sea_zones
                .iter()
                .any(|z| !z.is_lake && z.coastal_provinces.contains(&province.id));
        }
        Self {
            map_key: v.map_key,
            hex_map,
            provinces,
            nations: v.nations.into_iter().map(Into::into).collect(),
            diplomacy: v.diplomacy.into(),
            market_state: v.market_state.into(),
            sea_zones,
        }
    }
}

impl From<&dgs::GameArchive> for GameArchive {
    fn from(v: &dgs::GameArchive) -> Self {
        Self {
            history: v
                .history
                .iter()
                .map(|(t, e)| ((*t).into(), e.into()))
                .collect(),
            high_scores: v.high_scores.clone(),
            newspaper_archive: v
                .newspaper_archive
                .iter()
                .map(|(t, hs)| ((*t).into(), hs.iter().map(Into::into).collect()))
                .collect(),
            battle_archive: v
                .battle_archive
                .iter()
                .map(|(t, lb, nb): &(_, Vec<_>, Vec<_>)| {
                    (
                        (*t).into(),
                        lb.iter().map(Into::into).collect(),
                        nb.iter().map(Into::into).collect(),
                    )
                })
                .collect(),
            political_archive: v
                .political_archive
                .iter()
                .map(|(t, ps)| ((*t).into(), ps.into()))
                .collect(),
            market_archive: v
                .market_archive
                .iter()
                .map(|(t, mr)| ((*t).into(), mr.into()))
                .collect(),
        }
    }
}
impl From<GameArchive> for dgs::GameArchive {
    fn from(v: GameArchive) -> Self {
        Self {
            history: v
                .history
                .into_iter()
                .map(|(t, e)| (t.into(), e.into()))
                .collect(),
            high_scores: v.high_scores,
            newspaper_archive: v
                .newspaper_archive
                .into_iter()
                .map(|(t, hs)| (t.into(), hs.into_iter().map(Into::into).collect()))
                .collect(),
            battle_archive: v
                .battle_archive
                .into_iter()
                .map(|(t, lb, nb)| {
                    (
                        t.into(),
                        lb.into_iter().map(Into::into).collect(),
                        nb.into_iter().map(Into::into).collect(),
                    )
                })
                .collect(),
            political_archive: v
                .political_archive
                .into_iter()
                .map(|(t, ps)| (t.into(), ps.into()))
                .collect(),
            market_archive: v
                .market_archive
                .into_iter()
                .map(|(t, mr)| (t.into(), mr.into()))
                .collect(),
        }
    }
}

impl From<&dgs::TransientState> for TransientState {
    fn from(v: &dgs::TransientState) -> Self {
        Self {
            pending_attacks: v
                .pending_attacks
                .iter()
                .map(|(n, p)| ((*n).into(), (*p).into()))
                .collect(),
            pending_moves: v
                .pending_moves
                .iter()
                .map(|(n, uid, p)| ((*n).into(), uid.0, (*p).into()))
                .collect(),
            pending_fleet_moves: v
                .pending_fleet_moves
                .iter()
                .map(|(n, fz, tz)| ((*n).into(), fz.0, tz.0))
                .collect(),
            pending_landings: v
                .pending_landings
                .iter()
                .map(|(n, p, t)| ((*n).into(), (*p).into(), (*t).into()))
                .collect(),
            pending_diplomacy_actions: v
                .pending_diplomacy_actions
                .iter()
                .map(|a| match a {
                    dgs::PendingDiplomacyAction::BuildConsulate { player, target } => {
                        PendingDiplomacyAction::BuildConsulate {
                            player: (*player).into(),
                            target: (*target).into(),
                        }
                    }
                    dgs::PendingDiplomacyAction::BuildEmbassy { player, target } => {
                        PendingDiplomacyAction::BuildEmbassy {
                            player: (*player).into(),
                            target: (*target).into(),
                        }
                    }
                    dgs::PendingDiplomacyAction::DeclareWar { from, to } => {
                        PendingDiplomacyAction::DeclareWar {
                            from: (*from).into(),
                            to: (*to).into(),
                        }
                    }
                    dgs::PendingDiplomacyAction::SendGrant { from, to, amount } => {
                        PendingDiplomacyAction::SendGrant {
                            from: (*from).into(),
                            to: (*to).into(),
                            amount: (*amount).into(),
                        }
                    }
                    dgs::PendingDiplomacyAction::BreakTreaty {
                        from,
                        to,
                        treaty_type,
                    } => PendingDiplomacyAction::BreakTreaty {
                        from: (*from).into(),
                        to: (*to).into(),
                        treaty_type: (*treaty_type).into(),
                    },
                })
                .collect(),
            last_cash_flow: v
                .last_cash_flow
                .iter()
                .map(|(n, cf)| ((*n).into(), cf.into()))
                .collect(),
            last_resource_flow: v
                .last_resource_flow
                .iter()
                .map(|(n, rf)| ((*n).into(), rf.into()))
                .collect(),
        }
    }
}
impl From<TransientState> for dgs::TransientState {
    fn from(v: TransientState) -> Self {
        use domain::map::UnitId;
        use domain::map::sea_zones::SeaZoneId;
        use domain::types::NationId as DN;
        use std::collections::HashMap;
        Self {
            events: Vec::new(),
            pending_attacks: v
                .pending_attacks
                .into_iter()
                .map(|(n, p)| (n.into(), p.into()))
                .collect(),
            pending_moves: v
                .pending_moves
                .into_iter()
                .map(|(n, uid, p)| (n.into(), UnitId(uid), p.into()))
                .collect(),
            pending_fleet_moves: v
                .pending_fleet_moves
                .into_iter()
                .map(|(n, fz, tz)| (n.into(), SeaZoneId(fz), SeaZoneId(tz)))
                .collect(),
            pending_landings: v
                .pending_landings
                .into_iter()
                .map(|(n, p, t)| (n.into(), p.into(), t.into()))
                .collect(),
            pending_diplomacy_actions: v
                .pending_diplomacy_actions
                .into_iter()
                .map(|a| match a {
                    PendingDiplomacyAction::BuildConsulate { player, target } => {
                        dgs::PendingDiplomacyAction::BuildConsulate {
                            player: player.into(),
                            target: target.into(),
                        }
                    }
                    PendingDiplomacyAction::BuildEmbassy { player, target } => {
                        dgs::PendingDiplomacyAction::BuildEmbassy {
                            player: player.into(),
                            target: target.into(),
                        }
                    }
                    PendingDiplomacyAction::DeclareWar { from, to } => {
                        dgs::PendingDiplomacyAction::DeclareWar {
                            from: from.into(),
                            to: to.into(),
                        }
                    }
                    PendingDiplomacyAction::SendGrant { from, to, amount } => {
                        dgs::PendingDiplomacyAction::SendGrant {
                            from: from.into(),
                            to: to.into(),
                            amount: amount.into(),
                        }
                    }
                    PendingDiplomacyAction::BreakTreaty {
                        from,
                        to,
                        treaty_type,
                    } => dgs::PendingDiplomacyAction::BreakTreaty {
                        from: from.into(),
                        to: to.into(),
                        treaty_type: treaty_type.into(),
                    },
                })
                .collect(),
            pending_ai_cash_spending: Vec::new(),
            pending_ai_cash_income: Vec::new(),
            pending_economy_orders: HashMap::new(),
            last_cash_flow: v
                .last_cash_flow
                .into_iter()
                .map(|(n, cf)| (DN::from(n), cf.into()))
                .collect::<HashMap<_, _>>(),
            last_resource_flow: v
                .last_resource_flow
                .into_iter()
                .map(|(n, rf)| (DN::from(n), rf.into()))
                .collect::<HashMap<_, _>>(),
            pending_ai_material_outflows: Vec::new(),
            pending_ai_goods_outflows: Vec::new(),
            pending_ai_material_inflows: Vec::new(),
        }
    }
}

impl From<&dgs::GameState> for GameState {
    fn from(v: &dgs::GameState) -> Self {
        Self {
            turn: v.turn.into(),
            difficulty: v.difficulty.into(),
            human_player_nation: v.human_player_nation.into(),
            observer_mode: v.observer_mode,
            next_unit_id: v.next_unit_id,
            rng_state: v.rng_state,
            world: (&v.world).into(),
            archive: (&v.archive).into(),
            transient: (&v.transient).into(),
        }
    }
}
impl From<GameState> for dgs::GameState {
    fn from(v: GameState) -> Self {
        Self {
            turn: v.turn.into(),
            difficulty: v.difficulty.into(),
            human_player_nation: v.human_player_nation.into(),
            ai_debug: false,
            observer_mode: v.observer_mode,
            next_unit_id: v.next_unit_id,
            rng_state: v.rng_state,
            game_data: domain::data::GameData::default(),
            world: v.world.into(),
            archive: v.archive.into(),
            transient: v.transient.into(),
        }
    }
}
