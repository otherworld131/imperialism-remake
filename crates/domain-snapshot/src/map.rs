use crate::hex::HexCoord;
use crate::types::{NationId, ProvinceId, ResourceType, TerrainType};
use domain::map as dm;

// ── Infrastructure ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Infrastructure {
    pub has_railroad: bool,
    pub has_depot: bool,
    pub has_port: bool,
    pub has_fort: bool,
    pub fort_level: u8,
}

// ── Tile ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Tile {
    pub terrain: TerrainType,
    pub resource_deposit: Option<ResourceType>,
    pub prospected: bool,
    pub improvement_level: u8,
    pub infrastructure: Infrastructure,
    pub assigned_civilian: Option<u32>,
    pub province_id: Option<ProvinceId>,
    pub is_capital: bool,
    pub is_country_capital: bool,
}

// ── HexMap ────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HexMap {
    pub tiles: Vec<(HexCoord, Tile)>,
    pub width: i32,
    pub height: i32,
}

// ── Province ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SettlementLevel { Hamlet, Village, Town }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Province {
    pub id: ProvinceId,
    pub name: String,
    pub owner: NationId,
    pub capital_tile: HexCoord,
    pub tiles: Vec<HexCoord>,
    pub garrison_count: u8,
    pub settlement_level: SettlementLevel,
    pub connected_to_capital: bool,
    pub industrialization_turns_remaining: Option<u8>,
    #[serde(default)]
    pub town_countdown: Option<u8>,
    #[serde(default)]
    pub coastal: bool,
    #[serde(default)]
    pub incorporated_from: Option<NationId>,
    #[serde(default)]
    pub conquest_origin: Option<NationId>,
}

// ═══════════════════════════════════════════════════════════════════
// From impls
// ═══════════════════════════════════════════════════════════════════

impl From<dm::Infrastructure> for Infrastructure {
    fn from(v: dm::Infrastructure) -> Self {
        Self {
            has_railroad: v.has_railroad,
            has_depot: v.has_depot,
            has_port: v.has_port,
            has_fort: v.has_fort,
            fort_level: v.fort_level,
        }
    }
}
impl From<Infrastructure> for dm::Infrastructure {
    fn from(v: Infrastructure) -> Self {
        Self {
            has_railroad: v.has_railroad,
            has_depot: v.has_depot,
            has_port: v.has_port,
            has_fort: v.has_fort,
            fort_level: v.fort_level,
        }
    }
}

impl From<&dm::Tile> for Tile {
    fn from(v: &dm::Tile) -> Self {
        Self {
            terrain: v.terrain().into(),
            resource_deposit: v.resource_deposit().map(Into::into),
            prospected: v.is_prospected(),
            improvement_level: v.improvement_level(),
            infrastructure: v.infrastructure.into(),
            assigned_civilian: v.assigned_civilian.map(|u| u.0),
            province_id: v.province_id.map(Into::into),
            is_capital: v.is_capital,
            is_country_capital: v.is_country_capital,
        }
    }
}
impl From<Tile> for dm::Tile {
    fn from(v: Tile) -> Self {
        use domain::map::UnitId;
        let terrain: domain::types::TerrainType = v.terrain.into();
        let mut tile = dm::Tile::new(terrain);
        if let Some(res) = v.resource_deposit {
            let res_d: domain::types::ResourceType = res.into();
            if v.prospected {
                tile.reveal_deposit(res_d);
            } else {
                tile.set_resource(res_d);
            }
        } else if v.prospected {
            // Tile was prospected and found to have no deposit — must preserve this
            // so the player can't re-prospect the same tile after a save/load.
            tile.reveal_no_deposit();
        }
        tile.set_improvement_level(v.improvement_level);
        tile.infrastructure = v.infrastructure.into();
        tile.assigned_civilian = v.assigned_civilian.map(UnitId);
        tile.province_id = v.province_id.map(Into::into);
        tile.is_capital = v.is_capital;
        tile.is_country_capital = v.is_country_capital;
        tile
    }
}

impl From<&dm::HexMap> for HexMap {
    fn from(v: &dm::HexMap) -> Self {
        Self {
            tiles: v.all_tiles().map(|(coord, tile)| (coord.into(), tile.into())).collect(),
            width: v.width(),
            height: v.height(),
        }
    }
}
impl From<HexMap> for dm::HexMap {
    fn from(v: HexMap) -> Self {
        let mut map = dm::HexMap::new(v.width, v.height);
        for (coord, tile) in v.tiles {
            map.set_tile(coord.into(), tile.into());
        }
        map
    }
}

impl From<domain::map::SettlementLevel> for SettlementLevel {
    fn from(v: domain::map::SettlementLevel) -> Self {
        match v {
            domain::map::SettlementLevel::Hamlet => Self::Hamlet,
            domain::map::SettlementLevel::Village => Self::Village,
            domain::map::SettlementLevel::Town => Self::Town,
        }
    }
}
impl From<SettlementLevel> for domain::map::SettlementLevel {
    fn from(v: SettlementLevel) -> Self {
        match v {
            SettlementLevel::Hamlet => Self::Hamlet,
            SettlementLevel::Village => Self::Village,
            SettlementLevel::Town => Self::Town,
        }
    }
}

impl From<&dm::Province> for Province {
    fn from(v: &dm::Province) -> Self {
        Self {
            id: v.id.into(),
            name: v.name.clone(),
            owner: v.owner.into(),
            capital_tile: v.capital_tile.into(),
            tiles: v.tiles.iter().copied().map(Into::into).collect(),
            garrison_count: v.garrison_count,
            settlement_level: v.settlement_level.into(),
            connected_to_capital: v.connected_to_capital,
            industrialization_turns_remaining: v.industrialization_turns_remaining,
            town_countdown: v.town_countdown,
            coastal: v.coastal,
            incorporated_from: v.incorporated_from.map(Into::into),
            conquest_origin: v.conquest_origin.map(Into::into),
        }
    }
}
impl From<Province> for dm::Province {
    fn from(v: Province) -> Self {
        let tiles: Vec<domain::hex::HexCoord> = v.tiles.into_iter().map(Into::into).collect();
        let mut p = dm::Province::new(
            v.id.into(),
            v.name,
            v.owner.into(),
            v.capital_tile.into(),
            tiles,
            v.garrison_count,
        );
        p.settlement_level = v.settlement_level.into();
        p.connected_to_capital = v.connected_to_capital;
        p.industrialization_turns_remaining = v.industrialization_turns_remaining;
        p.town_countdown = v.town_countdown;
        p.coastal = v.coastal;
        p.incorporated_from = v.incorporated_from.map(Into::into);
        p.conquest_origin = v.conquest_origin.map(Into::into);
        p
    }
}
