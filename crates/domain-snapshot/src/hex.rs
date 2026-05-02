use domain::hex::HexCoord as D;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct HexCoord {
    pub q: i32,
    pub r: i32,
}

impl From<D> for HexCoord {
    fn from(v: D) -> Self {
        Self { q: v.q, r: v.r }
    }
}
impl From<HexCoord> for D {
    fn from(v: HexCoord) -> Self {
        D::new(v.q, v.r)
    }
}
