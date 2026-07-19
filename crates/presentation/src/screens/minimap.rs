//! CPU-rasterized mini hex maps for the battle screen's province inset and
//! the newspaper archive's political-map modal. The web frontend draws these
//! on a `<canvas>`; here each panel renders once into a Bevy [`Image`]
//! (pixel → cube-rounded hex lookup), with nation labels overlaid as
//! absolutely-positioned text nodes by the calling screen.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::collections::{HashMap, HashSet, VecDeque};

const SQRT3: f32 = 1.732_050_8;

/// Pointy-top axial → pixel, matching the web `hexToPixel`.
pub fn hex_to_px(q: i32, r: i32, size: f32) -> Vec2 {
    Vec2::new(
        size * (SQRT3 * q as f32 + SQRT3 / 2.0 * r as f32),
        size * (1.5 * r as f32),
    )
}

/// Pixel → axial hex via fractional axial coords + cube rounding.
fn px_to_hex(x: f32, y: f32, size: f32) -> (i32, i32) {
    let qf = (SQRT3 / 3.0 * x - y / 3.0) / size;
    let rf = (2.0 / 3.0 * y) / size;
    cube_round(qf, rf)
}

fn cube_round(qf: f32, rf: f32) -> (i32, i32) {
    let sf = -qf - rf;
    let mut q = qf.round();
    let mut r = rf.round();
    let s = sf.round();
    let dq = (q - qf).abs();
    let dr = (r - rf).abs();
    let ds = (s - sf).abs();
    if dq > dr && dq > ds {
        q = -r - s;
    } else if dr > ds {
        r = -q - s;
    }
    (q as i32, r as i32)
}

/// One hex's render data. `country` / `province` are arbitrary group ids;
/// neighbors with different `country` draw a thick dark border, same country
/// but different `province` a medium border, otherwise a faint grid line.
#[derive(Clone, Copy)]
pub struct HexCell {
    pub fill: [u8; 3],
    pub country: u32,
    pub province: u32,
}

pub struct RasterParams {
    pub width: u32,
    pub height: u32,
    pub hex_size: f32,
    /// Added to `hex_to_px` results — position the content in the image.
    pub offset: Vec2,
    pub background: [u8; 3],
}

/// Pixel buffer being composed; wrap into an [`Image`] with [`into_image`].
pub struct Raster {
    pub params: RasterParams,
    data: Vec<u8>,
}

impl Raster {
    /// Rasterize the hex fills and borders.
    pub fn new(params: RasterParams, cells: &HashMap<(i32, i32), HexCell>) -> Self {
        let (w, h) = (params.width as usize, params.height as usize);
        let mut data = vec![0u8; w * h * 4];
        // Per-pixel hex id (for border detection) and base fill.
        let mut hexes: Vec<(i32, i32)> = vec![(i32::MIN, i32::MIN); w * h];
        for y in 0..h {
            for x in 0..w {
                let hex = px_to_hex(
                    x as f32 + 0.5 - params.offset.x,
                    y as f32 + 0.5 - params.offset.y,
                    params.hex_size,
                );
                let i = y * w + x;
                hexes[i] = hex;
                let fill = cells.get(&hex).map(|c| c.fill).unwrap_or(params.background);
                data[i * 4] = fill[0];
                data[i * 4 + 1] = fill[1];
                data[i * 4 + 2] = fill[2];
                data[i * 4 + 3] = 255;
            }
        }
        // Border tiers: 0 none, 1 grid, 2 province, 3 country.
        let mut tier = vec![0u8; w * h];
        let edge_tier = |a: (i32, i32), b: (i32, i32)| -> u8 {
            let ca = cells.get(&a);
            let cb = cells.get(&b);
            match (ca, cb) {
                (Some(ca), Some(cb)) => {
                    if ca.country != cb.country {
                        3
                    } else if ca.country != u32::MAX && ca.province != cb.province {
                        2
                    } else {
                        1
                    }
                }
                (Some(ca), None) | (None, Some(ca)) => {
                    if ca.country != u32::MAX {
                        3
                    } else {
                        1
                    }
                }
                (None, None) => 0,
            }
        };
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let here = hexes[i];
                if x > 0 && hexes[i - 1] != here {
                    tier[i] = tier[i].max(edge_tier(here, hexes[i - 1]));
                }
                if y > 0 && hexes[i - w] != here {
                    tier[i] = tier[i].max(edge_tier(here, hexes[i - w]));
                }
            }
        }
        // Dilate country borders one pixel so they read as thick lines.
        let mut thick = tier.clone();
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if tier[i] == 3 {
                    if x + 1 < w {
                        thick[i + 1] = thick[i + 1].max(3);
                    }
                    if y + 1 < h {
                        thick[i + w] = thick[i + w].max(3);
                    }
                }
            }
        }
        for (i, tier) in thick.iter().enumerate() {
            match tier {
                3 => blend(&mut data, i, [20, 10, 0], 0.9),
                2 => blend(&mut data, i, [40, 30, 20], 0.55),
                1 => blend(&mut data, i, [0, 0, 0], 0.12),
                _ => {}
            }
        }
        Self { params, data }
    }

    /// Blend `color` at `alpha` over every pixel of `hex`'s area (e.g. the
    /// battle-province highlight). One pass over the buffer.
    pub fn tint_hexes(&mut self, hexes: &HashSet<(i32, i32)>, color: [u8; 3], alpha: f32) {
        let (w, h) = (self.params.width as usize, self.params.height as usize);
        for y in 0..h {
            for x in 0..w {
                let hex = px_to_hex(
                    x as f32 + 0.5 - self.params.offset.x,
                    y as f32 + 0.5 - self.params.offset.y,
                    self.params.hex_size,
                );
                if hexes.contains(&hex) {
                    blend(&mut self.data, y * w + x, color, alpha);
                }
            }
        }
    }

    /// Solid line of the given thickness between two pixel positions.
    pub fn draw_line(&mut self, from: Vec2, to: Vec2, color: [u8; 3], thickness: f32) {
        let steps = (to - from).length().ceil().max(1.0) as usize;
        let radius = (thickness / 2.0).max(0.5);
        for step in 0..=steps {
            let p = from.lerp(to, step as f32 / steps as f32);
            self.fill_disc(p, radius, color);
        }
    }

    /// Arrow from `from` to `to` (attack arrows): a line plus two head barbs.
    pub fn draw_arrow(&mut self, from: Vec2, to: Vec2, color: [u8; 3], thickness: f32) {
        self.draw_line(from, to, color, thickness);
        let dir = (to - from).normalize_or_zero();
        if dir == Vec2::ZERO {
            return;
        }
        let angle = dir.y.atan2(dir.x);
        let len = 8.0;
        for da in [-0.4f32, 0.4] {
            let barb = to - Vec2::new((angle + da).cos(), (angle + da).sin()) * len;
            self.draw_line(to, barb, color, thickness);
        }
    }

    /// Filled circle with a 1px ring (capital markers on the political map).
    pub fn draw_dot(&mut self, center: Vec2, radius: f32, fill: [u8; 3], ring: [u8; 3]) {
        self.fill_disc(center, radius + 1.0, ring);
        self.fill_disc(center, radius, fill);
    }

    fn fill_disc(&mut self, center: Vec2, radius: f32, color: [u8; 3]) {
        let w = self.params.width as i32;
        let h = self.params.height as i32;
        let r = radius.ceil() as i32;
        let (cx, cy) = (center.x, center.y);
        for dy in -r..=r {
            for dx in -r..=r {
                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if x < 0 || y < 0 || x >= w || y >= h {
                    continue;
                }
                let px = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                if px.distance(center) <= radius {
                    blend(&mut self.data, (y * w + x) as usize, color, 1.0);
                }
            }
        }
    }

    pub fn into_image(self) -> Image {
        Image::new(
            Extent3d {
                width: self.params.width,
                height: self.params.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            self.data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        )
    }
}

fn blend(data: &mut [u8], i: usize, color: [u8; 3], alpha: f32) {
    for c in 0..3 {
        let base = f32::from(data[i * 4 + c]);
        data[i * 4 + c] = (base + (f32::from(color[c]) - base) * alpha).round() as u8;
    }
}

/// `Color` → sRGB bytes for the raster buffer.
pub fn rgb(color: Color) -> [u8; 3] {
    let c = color.to_srgba();
    [
        (c.red * 255.0).round() as u8,
        (c.green * 255.0).round() as u8,
        (c.blue * 255.0).round() as u8,
    ]
}

// ── Nation labels ────────────────────────────────────────────────────────

/// A nation-name label positioned in image pixel space (web
/// `computeNationLabels` port: BFS-connected components per border group,
/// centroid + component size).
pub struct NationLabel {
    pub name: String,
    /// Pixel position inside the raster (offset already applied).
    pub pos: Vec2,
    pub size: usize,
}

/// A label ready to render: clamped inside the raster and guaranteed not to
/// overlap any other placed label.
pub struct PlacedLabel {
    /// Uppercased display name.
    pub name: String,
    pub pos: Vec2,
    pub font_size: f32,
}

/// De-collide + clamp nation labels for a `bounds`-sized raster. Labels are
/// placed largest-component first; a label whose estimated text box would
/// overlap an already-placed one is dropped (better absent than illegible).
/// `font_mult` / `font_min` / `font_max` map component size → font size.
pub fn place_nation_labels(
    mut labels: Vec<NationLabel>,
    bounds: Vec2,
    font_mult: f32,
    font_min: f32,
    font_max: f32,
) -> Vec<PlacedLabel> {
    labels.sort_by(|a, b| b.size.cmp(&a.size));
    let mut placed: Vec<(Vec2, Vec2)> = Vec::new();
    let mut out = Vec::new();
    for label in labels {
        let font_size = ((label.size as f32).sqrt() * font_mult).clamp(font_min, font_max);
        let name = label.name.to_uppercase();
        // Clamp the label center so the rendered text stays inside the
        // clipped frame instead of getting cut mid-word.
        let half_text = name.chars().count() as f32 * font_size * 0.30;
        let pos = Vec2::new(
            label.pos.x.clamp(
                half_text.min(bounds.x / 2.0),
                (bounds.x - half_text).max(bounds.x / 2.0),
            ),
            label.pos.y.clamp(12.0, bounds.y - 12.0),
        );
        let half = Vec2::new(half_text, font_size * 0.65);
        if placed
            .iter()
            .any(|(p, ph)| (pos - *p).abs().cmplt(half + *ph).all())
        {
            continue;
        }
        placed.push((pos, half));
        out.push(PlacedLabel {
            name,
            pos,
            font_size,
        });
    }
    out
}

/// `tiles`: (q, r, group-name) for every labelable land tile.
pub fn compute_nation_labels(
    tiles: &[(i32, i32, &str)],
    min_size: usize,
    hex_size: f32,
    offset: Vec2,
) -> Vec<NationLabel> {
    let mut groups: HashMap<&str, HashSet<(i32, i32)>> = HashMap::new();
    for &(q, r, name) in tiles {
        if name.is_empty() {
            continue;
        }
        groups.entry(name).or_default().insert((q, r));
    }
    let mut names: Vec<&str> = groups.keys().copied().collect();
    names.sort_unstable();

    let mut labels = Vec::new();
    for name in names {
        let tiles = &groups[name];
        let mut visited: HashSet<(i32, i32)> = HashSet::new();
        let mut starts: Vec<(i32, i32)> = tiles.iter().copied().collect();
        starts.sort_unstable();
        for start in starts {
            if visited.contains(&start) {
                continue;
            }
            let mut component = Vec::new();
            let mut queue = VecDeque::from([start]);
            visited.insert(start);
            while let Some((q, r)) = queue.pop_front() {
                component.push((q, r));
                for (nq, nr) in [
                    (q + 1, r),
                    (q, r + 1),
                    (q - 1, r + 1),
                    (q - 1, r),
                    (q, r - 1),
                    (q + 1, r - 1),
                ] {
                    if tiles.contains(&(nq, nr)) && visited.insert((nq, nr)) {
                        queue.push_back((nq, nr));
                    }
                }
            }
            if component.len() < min_size {
                continue;
            }
            let mut sum = Vec2::ZERO;
            for &(q, r) in &component {
                sum += hex_to_px(q, r, hex_size);
            }
            labels.push(NationLabel {
                name: name.to_string(),
                pos: sum / component.len() as f32 + offset,
                size: component.len(),
            });
        }
    }
    labels
}
