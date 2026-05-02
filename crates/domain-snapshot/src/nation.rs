use crate::economy::{CashSink, CashSource, NationEconomy};
use crate::events::TechId;
use crate::military::NationMilitary;
use crate::types::{NationId, NationType, ProvinceId};
use domain::ai;
use domain::nation as dn;

// ── NationColor ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NationColor {
    Yellow,
    Orange,
    LightBlue,
    Red,
    Green,
    Purple,
    Blue,
    Crimson,
    Magenta,
    Forest,
    Gold,
    Aqua,
    Violet,
    BurntOrange,
    HotPink,
    Turquoise,
    Slate,
    Mauve,
    Sage,
    Mustard,
    Gray,
    Brown,
    Pink,
    Teal,
    Olive,
    Maroon,
    Navy,
    Cyan,
    Lime,
    Coral,
    Lavender,
    Tan,
    Salmon,
    Khaki,
    Indigo,
}

// ── AiPersonality ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AiPersonality {
    Aggressive,
    Diplomatic,
    Economic,
    Balanced,
}

// ── SpendingCategory ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SpendingCategory {
    Military,
    Infrastructure,
    Consulate,
    Embassy,
    HireEngineer,
    HireImprover,
    Warship,
}

// ── CommittedInfraTarget ─────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommittedInfraTarget {
    pub candidate: crate::hex::HexCoord,
    pub origin_capital: crate::hex::HexCoord,
    pub turn_committed: u32,
}

// ── AiPriorityState ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AiPriorityState {
    pub priority_minor_targets: Vec<NationId>,
    /// Stored as Vec instead of HashMap<SpendingCategory, u32> to avoid non-string key issues.
    pub last_invest_turn: Vec<(SpendingCategory, u32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_infra_target: Option<CommittedInfraTarget>,
}

// ── NationDiplomacy ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NationDiplomacy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_personality: Option<AiPersonality>,
    /// Stored as Vec<(nation_id_raw, cents)> to avoid non-string map keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trade_subsidies: Vec<(u32, i64)>,
    #[serde(default)]
    pub is_in_anarchy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrated_by: Option<NationId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub player_sell_orders: Vec<crate::economy::PlayerSellOrder>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub player_buy_orders: Vec<crate::economy::PlayerBuyOrder>,
    #[serde(default)]
    pub ai_priority_state: AiPriorityState,
}

// ── NationArchives ───────────────────────────────────────────────────

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NationArchives {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trade_history: Vec<crate::economy::TradeHistoryEntry>,
    /// Stored as Vec<(CashSource, i64)> to avoid non-string map key issues.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cash_income_totals: Vec<(CashSource, i64)>,
    /// Stored as Vec<(CashSink, i64)> to avoid non-string map key issues.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cash_expense_totals: Vec<(CashSink, i64)>,
    #[serde(default)]
    pub goods_sales_revenue_dollars: i64,
    #[serde(default)]
    pub adjective: String,
    #[serde(default)]
    pub demonym_singular: String,
    #[serde(default)]
    pub demonym_plural: String,
    #[serde(default)]
    pub government_title: String,
    #[serde(default)]
    pub flag_svg: String,
}

// ── Nation ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Nation {
    pub id: NationId,
    pub name: String,
    pub color: NationColor,
    pub nation_type: NationType,
    pub province_ids: Vec<ProvinceId>,
    pub capital_province_id: ProvinceId,
    pub economy: NationEconomy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub researched_techs: Vec<TechId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub researched_tech_years: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_tech_research: Option<TechId>,
    #[serde(default)]
    pub military: NationMilitary,
    #[serde(default)]
    pub diplomacy: NationDiplomacy,
    #[serde(default)]
    pub archives: NationArchives,
}

// ═══════════════════════════════════════════════════════════════════════
// From impls
// ═══════════════════════════════════════════════════════════════════════

impl From<dn::NationColor> for NationColor {
    fn from(v: dn::NationColor) -> Self {
        match v {
            dn::NationColor::Yellow => Self::Yellow,
            dn::NationColor::Orange => Self::Orange,
            dn::NationColor::LightBlue => Self::LightBlue,
            dn::NationColor::Red => Self::Red,
            dn::NationColor::Green => Self::Green,
            dn::NationColor::Purple => Self::Purple,
            dn::NationColor::Blue => Self::Blue,
            dn::NationColor::Crimson => Self::Crimson,
            dn::NationColor::Magenta => Self::Magenta,
            dn::NationColor::Forest => Self::Forest,
            dn::NationColor::Gold => Self::Gold,
            dn::NationColor::Aqua => Self::Aqua,
            dn::NationColor::Violet => Self::Violet,
            dn::NationColor::BurntOrange => Self::BurntOrange,
            dn::NationColor::HotPink => Self::HotPink,
            dn::NationColor::Turquoise => Self::Turquoise,
            dn::NationColor::Slate => Self::Slate,
            dn::NationColor::Mauve => Self::Mauve,
            dn::NationColor::Sage => Self::Sage,
            dn::NationColor::Mustard => Self::Mustard,
            dn::NationColor::Gray => Self::Gray,
            dn::NationColor::Brown => Self::Brown,
            dn::NationColor::Pink => Self::Pink,
            dn::NationColor::Teal => Self::Teal,
            dn::NationColor::Olive => Self::Olive,
            dn::NationColor::Maroon => Self::Maroon,
            dn::NationColor::Navy => Self::Navy,
            dn::NationColor::Cyan => Self::Cyan,
            dn::NationColor::Lime => Self::Lime,
            dn::NationColor::Coral => Self::Coral,
            dn::NationColor::Lavender => Self::Lavender,
            dn::NationColor::Tan => Self::Tan,
            dn::NationColor::Salmon => Self::Salmon,
            dn::NationColor::Khaki => Self::Khaki,
            dn::NationColor::Indigo => Self::Indigo,
        }
    }
}
impl From<NationColor> for dn::NationColor {
    fn from(v: NationColor) -> Self {
        match v {
            NationColor::Yellow => Self::Yellow,
            NationColor::Orange => Self::Orange,
            NationColor::LightBlue => Self::LightBlue,
            NationColor::Red => Self::Red,
            NationColor::Green => Self::Green,
            NationColor::Purple => Self::Purple,
            NationColor::Blue => Self::Blue,
            NationColor::Crimson => Self::Crimson,
            NationColor::Magenta => Self::Magenta,
            NationColor::Forest => Self::Forest,
            NationColor::Gold => Self::Gold,
            NationColor::Aqua => Self::Aqua,
            NationColor::Violet => Self::Violet,
            NationColor::BurntOrange => Self::BurntOrange,
            NationColor::HotPink => Self::HotPink,
            NationColor::Turquoise => Self::Turquoise,
            NationColor::Slate => Self::Slate,
            NationColor::Mauve => Self::Mauve,
            NationColor::Sage => Self::Sage,
            NationColor::Mustard => Self::Mustard,
            NationColor::Gray => Self::Gray,
            NationColor::Brown => Self::Brown,
            NationColor::Pink => Self::Pink,
            NationColor::Teal => Self::Teal,
            NationColor::Olive => Self::Olive,
            NationColor::Maroon => Self::Maroon,
            NationColor::Navy => Self::Navy,
            NationColor::Cyan => Self::Cyan,
            NationColor::Lime => Self::Lime,
            NationColor::Coral => Self::Coral,
            NationColor::Lavender => Self::Lavender,
            NationColor::Tan => Self::Tan,
            NationColor::Salmon => Self::Salmon,
            NationColor::Khaki => Self::Khaki,
            NationColor::Indigo => Self::Indigo,
        }
    }
}

impl From<ai::AiPersonality> for AiPersonality {
    fn from(v: ai::AiPersonality) -> Self {
        match v {
            ai::AiPersonality::Aggressive => Self::Aggressive,
            ai::AiPersonality::Diplomatic => Self::Diplomatic,
            ai::AiPersonality::Economic => Self::Economic,
            ai::AiPersonality::Balanced => Self::Balanced,
        }
    }
}
impl From<AiPersonality> for ai::AiPersonality {
    fn from(v: AiPersonality) -> Self {
        match v {
            AiPersonality::Aggressive => Self::Aggressive,
            AiPersonality::Diplomatic => Self::Diplomatic,
            AiPersonality::Economic => Self::Economic,
            AiPersonality::Balanced => Self::Balanced,
        }
    }
}

impl From<ai::SpendingCategory> for SpendingCategory {
    fn from(v: ai::SpendingCategory) -> Self {
        match v {
            ai::SpendingCategory::Military => Self::Military,
            ai::SpendingCategory::Infrastructure => Self::Infrastructure,
            ai::SpendingCategory::Consulate => Self::Consulate,
            ai::SpendingCategory::Embassy => Self::Embassy,
            ai::SpendingCategory::HireEngineer => Self::HireEngineer,
            ai::SpendingCategory::HireImprover => Self::HireImprover,
            ai::SpendingCategory::Warship => Self::Warship,
        }
    }
}
impl From<SpendingCategory> for ai::SpendingCategory {
    fn from(v: SpendingCategory) -> Self {
        match v {
            SpendingCategory::Military => Self::Military,
            SpendingCategory::Infrastructure => Self::Infrastructure,
            SpendingCategory::Consulate => Self::Consulate,
            SpendingCategory::Embassy => Self::Embassy,
            SpendingCategory::HireEngineer => Self::HireEngineer,
            SpendingCategory::HireImprover => Self::HireImprover,
            SpendingCategory::Warship => Self::Warship,
        }
    }
}

impl From<&dn::CommittedInfraTarget> for CommittedInfraTarget {
    fn from(v: &dn::CommittedInfraTarget) -> Self {
        Self {
            candidate: v.candidate.into(),
            origin_capital: v.origin_capital.into(),
            turn_committed: v.turn_committed,
        }
    }
}
impl From<CommittedInfraTarget> for dn::CommittedInfraTarget {
    fn from(v: CommittedInfraTarget) -> Self {
        Self {
            candidate: v.candidate.into(),
            origin_capital: v.origin_capital.into(),
            turn_committed: v.turn_committed,
        }
    }
}

impl From<&dn::AiPriorityState> for AiPriorityState {
    fn from(v: &dn::AiPriorityState) -> Self {
        Self {
            priority_minor_targets: v
                .priority_minor_targets
                .iter()
                .copied()
                .map(Into::into)
                .collect(),
            last_invest_turn: v
                .last_invest_turn
                .iter()
                .map(|(k, val)| ((*k).into(), *val))
                .collect(),
            committed_infra_target: v.committed_infra_target.as_ref().map(Into::into),
        }
    }
}
impl From<AiPriorityState> for dn::AiPriorityState {
    fn from(v: AiPriorityState) -> Self {
        Self {
            priority_minor_targets: v
                .priority_minor_targets
                .into_iter()
                .map(Into::into)
                .collect(),
            last_invest_turn: v
                .last_invest_turn
                .into_iter()
                .map(|(k, val)| (k.into(), val))
                .collect(),
            committed_infra_target: v.committed_infra_target.map(Into::into),
        }
    }
}

impl From<&dn::NationDiplomacy> for NationDiplomacy {
    fn from(v: &dn::NationDiplomacy) -> Self {
        Self {
            ai_personality: v.ai_personality.map(Into::into),
            trade_subsidies: v
                .trade_subsidies
                .iter()
                .map(|(k, m)| (k.0, m.cents()))
                .collect(),
            is_in_anarchy: v.is_in_anarchy,
            integrated_by: v.integrated_by.map(Into::into),
            player_sell_orders: v.player_sell_orders.iter().map(Into::into).collect(),
            player_buy_orders: v.player_buy_orders.iter().map(Into::into).collect(),
            ai_priority_state: (&v.ai_priority_state).into(),
        }
    }
}
impl From<NationDiplomacy> for dn::NationDiplomacy {
    fn from(v: NationDiplomacy) -> Self {
        use domain::types::{Money, NationId as DN};
        Self {
            ai_personality: v.ai_personality.map(Into::into),
            trade_subsidies: v
                .trade_subsidies
                .into_iter()
                .map(|(k, cents)| (DN(k), Money::from_cents(cents)))
                .collect(),
            is_in_anarchy: v.is_in_anarchy,
            integrated_by: v.integrated_by.map(Into::into),
            player_sell_orders: v.player_sell_orders.into_iter().map(Into::into).collect(),
            player_buy_orders: v.player_buy_orders.into_iter().map(Into::into).collect(),
            ai_priority_state: v.ai_priority_state.into(),
        }
    }
}

impl From<&dn::NationArchives> for NationArchives {
    fn from(v: &dn::NationArchives) -> Self {
        Self {
            trade_history: v.trade_history.iter().map(Into::into).collect(),
            cash_income_totals: v
                .cash_income_totals
                .iter()
                .map(|(k, val)| ((*k).into(), *val))
                .collect(),
            cash_expense_totals: v
                .cash_expense_totals
                .iter()
                .map(|(k, val)| ((*k).into(), *val))
                .collect(),
            goods_sales_revenue_dollars: v.goods_sales_revenue_dollars,
            adjective: v.adjective.clone(),
            demonym_singular: v.demonym_singular.clone(),
            demonym_plural: v.demonym_plural.clone(),
            government_title: v.government_title.clone(),
            flag_svg: v.flag_svg.clone(),
        }
    }
}
impl From<NationArchives> for dn::NationArchives {
    fn from(v: NationArchives) -> Self {
        use domain::economy::CashSink as DCSink;
        use domain::economy::CashSource as DCS;
        Self {
            trade_history: v.trade_history.into_iter().map(Into::into).collect(),
            cash_income_totals: v
                .cash_income_totals
                .into_iter()
                .map(|(k, val)| (DCS::from(k), val))
                .collect(),
            cash_expense_totals: v
                .cash_expense_totals
                .into_iter()
                .map(|(k, val)| (DCSink::from(k), val))
                .collect(),
            goods_sales_revenue_dollars: v.goods_sales_revenue_dollars,
            adjective: v.adjective,
            demonym_singular: v.demonym_singular,
            demonym_plural: v.demonym_plural,
            government_title: v.government_title,
            flag_svg: v.flag_svg,
        }
    }
}

impl From<&dn::Nation> for Nation {
    fn from(v: &dn::Nation) -> Self {
        Self {
            id: v.id.into(),
            name: v.name.clone(),
            color: v.color.into(),
            nation_type: v.nation_type.into(),
            province_ids: v.province_ids.iter().copied().map(Into::into).collect(),
            capital_province_id: v.capital_province_id.into(),
            economy: (&v.economy).into(),
            researched_techs: v.researched_techs.iter().copied().map(Into::into).collect(),
            researched_tech_years: v.researched_tech_years.clone(),
            pending_tech_research: v.pending_tech_research.map(|t| TechId(t.0)),
            military: (&v.military).into(),
            diplomacy: (&v.diplomacy).into(),
            archives: (&v.archives).into(),
        }
    }
}
impl From<Nation> for dn::Nation {
    fn from(v: Nation) -> Self {
        use domain::types::ProvinceId as DP;
        let mut n = dn::Nation::new(
            v.id.into(),
            v.name,
            v.color.into(),
            v.nation_type.into(),
            v.capital_province_id.into(),
        );
        n.province_ids = v
            .province_ids
            .into_iter()
            .map(|p: ProvinceId| DP(p.0))
            .collect();
        n.economy = v.economy.into();
        n.researched_techs = v
            .researched_techs
            .into_iter()
            .map(|t: TechId| domain::events::TechId(t.0))
            .collect();
        n.researched_tech_years = v.researched_tech_years;
        n.pending_tech_research = v.pending_tech_research.map(|t| domain::events::TechId(t.0));
        n.military = v.military.into();
        n.diplomacy = v.diplomacy.into();
        n.archives = v.archives.into();
        n
    }
}
