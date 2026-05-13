use domain::types as d;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct NationId(pub u32);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ProvinceId(pub u32);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct TurnNumber(pub u32);

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct Money(pub i64);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ReservationId(pub u64);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ResourceType {
    Timber,
    Coal,
    Iron,
    Cotton,
    Wool,
    Grain,
    Fruit,
    Livestock,
    Horses,
    Oil,
    Gold,
    Gems,
    Fish,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum MaterialType {
    Lumber,
    Steel,
    Fabric,
    Paper,
    CannedFood,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum GoodsType {
    Furniture,
    Hardware,
    Clothing,
    Arms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TerrainType {
    Grassland,
    Forest,
    Hills,
    Mountain,
    Desert,
    Swamp,
    Tundra,
    Sea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NationType {
    GreatPower,
    MinorNation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Difficulty {
    Introductory,
    Easy,
    Normal,
    Hard,
    NighOnImpossible,
}

// ── From impls ────────────────────────────────────────────────────

impl From<d::NationId> for NationId {
    fn from(v: d::NationId) -> Self {
        Self(v.0)
    }
}
impl From<NationId> for d::NationId {
    fn from(v: NationId) -> Self {
        Self(v.0)
    }
}

impl From<d::ProvinceId> for ProvinceId {
    fn from(v: d::ProvinceId) -> Self {
        Self(v.0)
    }
}
impl From<ProvinceId> for d::ProvinceId {
    fn from(v: ProvinceId) -> Self {
        Self(v.0)
    }
}

impl From<d::TurnNumber> for TurnNumber {
    fn from(v: d::TurnNumber) -> Self {
        Self(v.0)
    }
}
impl From<TurnNumber> for d::TurnNumber {
    fn from(v: TurnNumber) -> Self {
        Self::new(v.0)
    }
}

impl From<d::Money> for Money {
    fn from(v: d::Money) -> Self {
        Self(v.cents())
    }
}
impl From<Money> for d::Money {
    fn from(v: Money) -> Self {
        Self::from_cents(v.0)
    }
}

impl From<d::ReservationId> for ReservationId {
    fn from(v: d::ReservationId) -> Self {
        Self(v.0)
    }
}
impl From<ReservationId> for d::ReservationId {
    fn from(v: ReservationId) -> Self {
        Self(v.0)
    }
}

impl From<d::ResourceType> for ResourceType {
    fn from(v: d::ResourceType) -> Self {
        match v {
            d::ResourceType::Timber => Self::Timber,
            d::ResourceType::Coal => Self::Coal,
            d::ResourceType::Iron => Self::Iron,
            d::ResourceType::Cotton => Self::Cotton,
            d::ResourceType::Wool => Self::Wool,
            d::ResourceType::Grain => Self::Grain,
            d::ResourceType::Fruit => Self::Fruit,
            d::ResourceType::Livestock => Self::Livestock,
            d::ResourceType::Horses => Self::Horses,
            d::ResourceType::Oil => Self::Oil,
            d::ResourceType::Gold => Self::Gold,
            d::ResourceType::Gems => Self::Gems,
            d::ResourceType::Fish => Self::Fish,
        }
    }
}
impl From<ResourceType> for d::ResourceType {
    fn from(v: ResourceType) -> Self {
        match v {
            ResourceType::Timber => Self::Timber,
            ResourceType::Coal => Self::Coal,
            ResourceType::Iron => Self::Iron,
            ResourceType::Cotton => Self::Cotton,
            ResourceType::Wool => Self::Wool,
            ResourceType::Grain => Self::Grain,
            ResourceType::Fruit => Self::Fruit,
            ResourceType::Livestock => Self::Livestock,
            ResourceType::Horses => Self::Horses,
            ResourceType::Oil => Self::Oil,
            ResourceType::Gold => Self::Gold,
            ResourceType::Gems => Self::Gems,
            ResourceType::Fish => Self::Fish,
        }
    }
}

impl From<d::MaterialType> for MaterialType {
    fn from(v: d::MaterialType) -> Self {
        match v {
            d::MaterialType::Lumber => Self::Lumber,
            d::MaterialType::Steel => Self::Steel,
            d::MaterialType::Fabric => Self::Fabric,
            d::MaterialType::Paper => Self::Paper,
            d::MaterialType::CannedFood => Self::CannedFood,
        }
    }
}
impl From<MaterialType> for d::MaterialType {
    fn from(v: MaterialType) -> Self {
        match v {
            MaterialType::Lumber => Self::Lumber,
            MaterialType::Steel => Self::Steel,
            MaterialType::Fabric => Self::Fabric,
            MaterialType::Paper => Self::Paper,
            MaterialType::CannedFood => Self::CannedFood,
        }
    }
}

impl From<d::GoodsType> for GoodsType {
    fn from(v: d::GoodsType) -> Self {
        match v {
            d::GoodsType::Furniture => Self::Furniture,
            d::GoodsType::Hardware => Self::Hardware,
            d::GoodsType::Clothing => Self::Clothing,
            d::GoodsType::Arms => Self::Arms,
        }
    }
}
impl From<GoodsType> for d::GoodsType {
    fn from(v: GoodsType) -> Self {
        match v {
            GoodsType::Furniture => Self::Furniture,
            GoodsType::Hardware => Self::Hardware,
            GoodsType::Clothing => Self::Clothing,
            GoodsType::Arms => Self::Arms,
        }
    }
}

impl From<d::TerrainType> for TerrainType {
    fn from(v: d::TerrainType) -> Self {
        match v {
            d::TerrainType::Grassland => Self::Grassland,
            d::TerrainType::Forest => Self::Forest,
            d::TerrainType::Hills => Self::Hills,
            d::TerrainType::Mountain => Self::Mountain,
            d::TerrainType::Desert => Self::Desert,
            d::TerrainType::Swamp => Self::Swamp,
            d::TerrainType::Tundra => Self::Tundra,
            d::TerrainType::Sea => Self::Sea,
        }
    }
}
impl From<TerrainType> for d::TerrainType {
    fn from(v: TerrainType) -> Self {
        match v {
            TerrainType::Grassland => Self::Grassland,
            TerrainType::Forest => Self::Forest,
            TerrainType::Hills => Self::Hills,
            TerrainType::Mountain => Self::Mountain,
            TerrainType::Desert => Self::Desert,
            TerrainType::Swamp => Self::Swamp,
            TerrainType::Tundra => Self::Tundra,
            TerrainType::Sea => Self::Sea,
        }
    }
}

impl From<d::NationType> for NationType {
    fn from(v: d::NationType) -> Self {
        match v {
            d::NationType::GreatPower => Self::GreatPower,
            d::NationType::MinorNation => Self::MinorNation,
        }
    }
}
impl From<NationType> for d::NationType {
    fn from(v: NationType) -> Self {
        match v {
            NationType::GreatPower => Self::GreatPower,
            NationType::MinorNation => Self::MinorNation,
        }
    }
}

impl From<d::Difficulty> for Difficulty {
    fn from(v: d::Difficulty) -> Self {
        match v {
            d::Difficulty::Introductory => Self::Introductory,
            d::Difficulty::Easy => Self::Easy,
            d::Difficulty::Normal => Self::Normal,
            d::Difficulty::Hard => Self::Hard,
            d::Difficulty::NighOnImpossible => Self::NighOnImpossible,
        }
    }
}
impl From<Difficulty> for d::Difficulty {
    fn from(v: Difficulty) -> Self {
        match v {
            Difficulty::Introductory => Self::Introductory,
            Difficulty::Easy => Self::Easy,
            Difficulty::Normal => Self::Normal,
            Difficulty::Hard => Self::Hard,
            Difficulty::NighOnImpossible => Self::NighOnImpossible,
        }
    }
}
