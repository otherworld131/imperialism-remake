//! Procedural flag generator.
//!
//! Produces a `FlagDesign` — a structured description of a 19th-century-style
//! flag — and a matching SVG string. The design is kept as data so a custom
//! frontend can render it however it wants (pixel art, fabric shader, etc.)
//! without reparsing SVG.
//!
//! Flag generation is *config-driven*: a `FlagRules` value specifies which
//! patterns/emblems are typical for which governments and which color or
//! emblem combinations should be excluded. Ships with `FlagRules::default()`
//! — a sane 19th-century set — and lets any caller override it.
//!
//! Flags use a 60×40 viewBox (3:2 aspect) to match the common historical
//! naval standard. An emblem, when present, is placed uniformly in one of
//! five positions: the center or one of the four corners (`EmblemPosition`).
//!
//! # Adding a new emblem
//!
//! Emblems are enumerated by the `Emblem` variant. To add one:
//!
//! 1. **Declare the variant.** Add it to `enum Emblem` below and — if you
//!    intend for serde to deserialize old save files — consider a
//!    `#[serde(alias = "…")]` for legacy names.
//! 2. **Draw it.** Extend the `match` in `emblem_svg` with a rendering
//!    arm that returns an SVG fragment. The function receives `(cx, cy,
//!    size, color)` in canvas space; keep your drawing inside a bounding
//!    box of `size × size` centered on `(cx, cy)` so corner placement
//!    doesn't clip. Use the `svg!`-free helpers (`rect`, inline
//!    `<circle>`, `<path>`) already used for other emblems.
//! 3. **Wire it into the default rules.** In `FlagRules::default()`, add
//!    your emblem (with a weight) to `default_emblem_weights` and to
//!    any per-government `emblem_weights` entries where it belongs.
//!    Monarchies lean heraldic, republics lean civic, theocracies lean
//!    religious — pick weights that match the cultural register.
//! 4. **(Optional) Ban awkward combinations.** Add
//!    `FlagExclusion::EmblemOnGovernment` or
//!    `FlagExclusion::EmblemWithPattern` entries to
//!    `forbidden_pairings` in `FlagRules::default()`. For example,
//!    `Emblem::Cross` is banned on `Republic` by default.
//! 5. **(Optional) Test it.** The tests in `#[cfg(test)]` at the bottom
//!    of this file iterate over random flags and assert structural
//!    invariants. If your emblem requires special-case drawing (e.g. a
//!    polygon generator), add a targeted test there.
//!
//! The same five-step recipe applies to new flag patterns
//! (`FlagPattern::*`) and to new flag colors (`FlagColor::*`) — each
//! enum is the single source of truth and all rendering / selection /
//! rule code branches on it exhaustively.

use std::collections::HashMap;

use crate::government::GovernmentForm;
use crate::rng::Rng;

pub const FLAG_WIDTH: u32 = 60;
pub const FLAG_HEIGHT: u32 = 40;

/// A named color from the historical flag palette. Kept as an enum (rather
/// than raw RGB) so saves/diffing stay readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FlagColor {
    Red,
    White,
    Blue,
    Navy,
    Green,
    DarkGreen,
    Yellow,
    Gold,
    Black,
    Orange,
    Maroon,
    Cyan,
}

impl FlagColor {
    pub fn hex(self) -> &'static str {
        match self {
            FlagColor::Red => "#C8102E",
            FlagColor::White => "#FFFFFF",
            FlagColor::Blue => "#1F4E9D",
            FlagColor::Navy => "#0A1B44",
            FlagColor::Green => "#1B8F3A",
            FlagColor::DarkGreen => "#0D5B2A",
            FlagColor::Yellow => "#F5D44A",
            FlagColor::Gold => "#D4A61A",
            FlagColor::Black => "#111111",
            FlagColor::Orange => "#E8852A",
            FlagColor::Maroon => "#6E1E2B",
            FlagColor::Cyan => "#3FB4C4",
        }
    }
}

/// Simple central emblems. The SVG renderer draws them as basic shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Emblem {
    None,
    Star,
    Sun,
    Crescent,
    Cross,
    Disk,
    Wheel,
    Anchor,
    Saltire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FlagPattern {
    HorizontalBicolor,
    HorizontalTricolor,
    VerticalBicolor,
    VerticalTricolor,
    Quartered,
    NordicCross,
    CantonStripes,
    Solid,
}

/// Where the emblem sits on the flag. Corners are inset ~10px from the
/// edge of the 60×40 canvas so the emblem always sits entirely on-flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EmblemPosition {
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl EmblemPosition {
    pub const ALL: &'static [EmblemPosition] = &[
        EmblemPosition::Center,
        EmblemPosition::TopLeft,
        EmblemPosition::TopRight,
        EmblemPosition::BottomLeft,
        EmblemPosition::BottomRight,
    ];

    /// Canvas-space `(cx, cy, size)` for this position on the standard
    /// 60×40 flag. Corner emblems are ~30% smaller than the centered one so
    /// they read as a badge rather than dominating the field.
    fn layout(self) -> (i32, i32, i32) {
        const CORNER_INSET: i32 = 10;
        const CORNER_SIZE: i32 = 10;
        const CENTER_SIZE: i32 = 14;
        let w = FLAG_WIDTH as i32;
        let h = FLAG_HEIGHT as i32;
        match self {
            EmblemPosition::Center => (w / 2, h / 2, CENTER_SIZE),
            EmblemPosition::TopLeft => (CORNER_INSET, CORNER_INSET, CORNER_SIZE),
            EmblemPosition::TopRight => (w - CORNER_INSET, CORNER_INSET, CORNER_SIZE),
            EmblemPosition::BottomLeft => (CORNER_INSET, h - CORNER_INSET, CORNER_SIZE),
            EmblemPosition::BottomRight => (w - CORNER_INSET, h - CORNER_INSET, CORNER_SIZE),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlagDesign {
    pub pattern: FlagPattern,
    /// Colors used by the pattern. Length depends on pattern: 1 for Solid,
    /// 2 for bi-color/NordicCross, 3 for tricolors, 4 for Quartered, 3 for
    /// CantonStripes `[canton_bg, stripe_a, stripe_b]`.
    pub colors: Vec<FlagColor>,
    pub emblem: Emblem,
    pub emblem_color: FlagColor,
    #[serde(default = "default_emblem_position")]
    pub emblem_position: EmblemPosition,
}

fn default_emblem_position() -> EmblemPosition {
    EmblemPosition::Center
}

impl Default for FlagDesign {
    fn default() -> Self {
        // A neutral white-field-with-red-cross as the fallback.
        Self {
            pattern: FlagPattern::Solid,
            colors: vec![FlagColor::White],
            emblem: Emblem::Cross,
            emblem_color: FlagColor::Red,
            emblem_position: EmblemPosition::Center,
        }
    }
}

// ── Palette helpers ─────────────────────────────────────────────

const PALETTE: &[FlagColor] = &[
    FlagColor::Red,
    FlagColor::White,
    FlagColor::Blue,
    FlagColor::Navy,
    FlagColor::Green,
    FlagColor::DarkGreen,
    FlagColor::Yellow,
    FlagColor::Gold,
    FlagColor::Black,
    FlagColor::Orange,
    FlagColor::Maroon,
    FlagColor::Cyan,
];

/// Structural exclusions that `FlagRules` enforces when generating a flag.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FlagExclusion {
    /// "Never put this emblem on a flag for this government."
    EmblemOnGovernment(Emblem, GovernmentForm),
    /// "Never use this pattern for this government."
    PatternOnGovernment(FlagPattern, GovernmentForm),
    /// "This emblem never appears on a flag with this pattern."
    EmblemWithPattern(Emblem, FlagPattern),
}

/// Config bundle controlling procedural flag generation. The shipping
/// defaults encode mild 19th-century conventions (tricolors for republics,
/// crosses for monarchies, no religious emblems on secular republics, etc.)
/// but callers can override any field. Unlisted governments fall through to
/// the `default_*` pools.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlagRules {
    pub pattern_weights: HashMap<GovernmentForm, Vec<(FlagPattern, u32)>>,
    pub default_pattern_weights: Vec<(FlagPattern, u32)>,
    pub emblem_weights: HashMap<GovernmentForm, Vec<(Emblem, u32)>>,
    pub default_emblem_weights: Vec<(Emblem, u32)>,
    /// Unordered color pairs that must not coexist on the same flag.
    pub forbidden_color_pairs: Vec<(FlagColor, FlagColor)>,
    pub forbidden_pairings: Vec<FlagExclusion>,
}

impl Default for FlagRules {
    fn default() -> Self {
        use Emblem::*;
        use FlagPattern::*;
        use GovernmentForm::*;

        let mut pattern_weights: HashMap<GovernmentForm, Vec<(FlagPattern, u32)>> = HashMap::new();
        // Monarchies: classical & heraldic feel.
        for m in [AbsoluteMonarchy, ConstitutionalMonarchy, Empire, Kingdom, GrandDuchy] {
            pattern_weights.insert(
                m,
                vec![
                    (NordicCross, 3),
                    (Quartered, 3),
                    (CantonStripes, 2),
                    (HorizontalBicolor, 2),
                    (HorizontalTricolor, 2),
                    (Solid, 1),
                ],
            );
        }
        // Republics & federal systems: tricolors dominate.
        for r in [Republic, FederalRepublic, Confederation] {
            pattern_weights.insert(
                r,
                vec![
                    (HorizontalTricolor, 5),
                    (VerticalTricolor, 5),
                    (HorizontalBicolor, 2),
                    (VerticalBicolor, 2),
                    (Solid, 1),
                ],
            );
        }
        // Eastern realms: solid field + central emblem is traditional.
        for e in [Sultanate, Emirate, Khanate, Shogunate] {
            pattern_weights.insert(
                e,
                vec![
                    (Solid, 5),
                    (HorizontalBicolor, 3),
                    (HorizontalTricolor, 2),
                    (Quartered, 1),
                ],
            );
        }

        let mut emblem_weights: HashMap<GovernmentForm, Vec<(Emblem, u32)>> = HashMap::new();
        // Republics eschew religious symbols.
        for r in [Republic, FederalRepublic, Confederation] {
            emblem_weights.insert(
                r,
                vec![(None, 6), (Star, 3), (Sun, 2), (Wheel, 1), (Disk, 1)],
            );
        }
        // Monarchies prefer regal motifs.
        for m in [AbsoluteMonarchy, ConstitutionalMonarchy, Empire, Kingdom, GrandDuchy] {
            emblem_weights.insert(
                m,
                vec![(Cross, 4), (None, 4), (Star, 2), (Sun, 2), (Disk, 1), (Anchor, 1)],
            );
        }
        // Theocracies freely use religious emblems.
        emblem_weights.insert(
            Theocracy,
            vec![(Cross, 3), (Crescent, 3), (Sun, 2), (Star, 2), (None, 1)],
        );
        // Islamic realms prefer crescent/star.
        for e in [Sultanate, Emirate] {
            emblem_weights.insert(
                e,
                vec![(Crescent, 4), (Star, 3), (Sun, 1), (None, 2)],
            );
        }

        // Structural pairings we disallow.
        let forbidden_pairings = vec![
            // Secular republics shouldn't carry religious emblems.
            FlagExclusion::EmblemOnGovernment(Crescent, Republic),
            FlagExclusion::EmblemOnGovernment(Crescent, FederalRepublic),
            FlagExclusion::EmblemOnGovernment(Cross, Republic),
            FlagExclusion::EmblemOnGovernment(Cross, FederalRepublic),
            // A NordicCross flag already *is* a cross — don't pile on more.
            FlagExclusion::EmblemWithPattern(Cross, NordicCross),
            FlagExclusion::EmblemWithPattern(Saltire, NordicCross),
        ];

        // One obvious color clash worth banning by default. The whole list is
        // short on purpose — we surface the knob without being prescriptive.
        let forbidden_color_pairs = vec![(FlagColor::Orange, FlagColor::Red)];

        Self {
            pattern_weights,
            default_pattern_weights: vec![
                (HorizontalTricolor, 3),
                (VerticalTricolor, 3),
                (HorizontalBicolor, 2),
                (VerticalBicolor, 2),
                (Quartered, 1),
                (NordicCross, 1),
                (CantonStripes, 1),
                (Solid, 1),
            ],
            emblem_weights,
            default_emblem_weights: vec![
                (None, 3),
                (Star, 2),
                (Sun, 1),
                (Cross, 1),
                (Disk, 1),
                (Wheel, 1),
                (Anchor, 1),
                (Saltire, 1),
                (Crescent, 1),
            ],
            forbidden_color_pairs,
            forbidden_pairings,
        }
    }
}

impl FlagRules {
    fn pattern_pool(&self, form: GovernmentForm) -> &[(FlagPattern, u32)] {
        self.pattern_weights
            .get(&form)
            .map(|v| v.as_slice())
            .unwrap_or(&self.default_pattern_weights)
    }

    fn emblem_pool(&self, form: GovernmentForm) -> &[(Emblem, u32)] {
        self.emblem_weights
            .get(&form)
            .map(|v| v.as_slice())
            .unwrap_or(&self.default_emblem_weights)
    }

    fn color_pair_forbidden(&self, a: FlagColor, b: FlagColor) -> bool {
        self.forbidden_color_pairs
            .iter()
            .any(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a))
    }

    fn emblem_forbidden_on(&self, emblem: Emblem, form: GovernmentForm, pattern: FlagPattern) -> bool {
        self.forbidden_pairings.iter().any(|ex| match *ex {
            FlagExclusion::EmblemOnGovernment(e, f) => e == emblem && f == form,
            FlagExclusion::EmblemWithPattern(e, p) => e == emblem && p == pattern,
            FlagExclusion::PatternOnGovernment(_, _) => false,
        })
    }

    fn pattern_forbidden_on(&self, pattern: FlagPattern, form: GovernmentForm) -> bool {
        self.forbidden_pairings.iter().any(|ex| match *ex {
            FlagExclusion::PatternOnGovernment(p, f) => p == pattern && f == form,
            _ => false,
        })
    }
}

fn weighted_pick<T: Copy>(rng: &mut Rng, pool: &[(T, u32)]) -> T {
    let total: u32 = pool.iter().map(|(_, w)| *w).sum();
    debug_assert!(total > 0, "weighted pool is empty");
    let mut roll = rng.range(0, total - 1);
    for (item, w) in pool {
        if roll < *w {
            return *item;
        }
        roll -= *w;
    }
    pool[pool.len() - 1].0
}

fn pick_distinct_colors_filtered(
    rng: &mut Rng,
    n: usize,
    rules: &FlagRules,
) -> Vec<FlagColor> {
    let mut out: Vec<FlagColor> = Vec::with_capacity(n);
    let mut tries = 0;
    while out.len() < n && tries < 200 {
        let c = *rng.pick(PALETTE);
        if !out.contains(&c) && !out.iter().any(|&prev| rules.color_pair_forbidden(prev, c)) {
            out.push(c);
        }
        tries += 1;
    }
    // Pathological seed safety net — pad from PALETTE in order, still
    // respecting distinctness and the forbidden-pair list.
    for c in PALETTE {
        if out.len() >= n {
            break;
        }
        if !out.contains(c) && !out.iter().any(|&prev| rules.color_pair_forbidden(prev, *c)) {
            out.push(*c);
        }
    }
    // If the rules are so restrictive that we still can't fill, fall back to
    // any distinct color — better a rule violation than a broken flag.
    for c in PALETTE {
        if out.len() >= n {
            break;
        }
        if !out.contains(c) {
            out.push(*c);
        }
    }
    out
}

/// Generate a flag for a given government form. Honors the per-government
/// pools and exclusion rules in `rules`.
pub fn random_for(rng: &mut Rng, form: GovernmentForm, rules: &FlagRules) -> FlagDesign {
    // Pick a pattern, redrawing up to 10 times if the pattern is explicitly
    // forbidden for this government. If every draw fails we fall back to the
    // default pool to avoid spinning forever.
    let mut pattern = weighted_pick(rng, rules.pattern_pool(form));
    for _ in 0..10 {
        if !rules.pattern_forbidden_on(pattern, form) {
            break;
        }
        pattern = weighted_pick(rng, &rules.default_pattern_weights);
    }

    let colors = match pattern {
        FlagPattern::Solid => pick_distinct_colors_filtered(rng, 1, rules),
        FlagPattern::HorizontalBicolor | FlagPattern::VerticalBicolor => {
            pick_distinct_colors_filtered(rng, 2, rules)
        }
        FlagPattern::NordicCross => pick_distinct_colors_filtered(rng, 2, rules),
        FlagPattern::HorizontalTricolor | FlagPattern::VerticalTricolor => {
            pick_distinct_colors_filtered(rng, 3, rules)
        }
        FlagPattern::Quartered => pick_distinct_colors_filtered(rng, 4, rules),
        FlagPattern::CantonStripes => pick_distinct_colors_filtered(rng, 3, rules),
    };

    // Pick an emblem honoring emblem-level and pattern-level exclusions.
    let mut emblem = weighted_pick(rng, rules.emblem_pool(form));
    for _ in 0..10 {
        if !rules.emblem_forbidden_on(emblem, form, pattern) {
            break;
        }
        emblem = weighted_pick(rng, &rules.default_emblem_weights);
    }
    if rules.emblem_forbidden_on(emblem, form, pattern) {
        emblem = Emblem::None;
    }

    let emblem_color = {
        let mut candidate = *rng.pick(PALETTE);
        for _ in 0..10 {
            if !colors.contains(&candidate)
                && !colors.iter().any(|&c| rules.color_pair_forbidden(c, candidate))
            {
                break;
            }
            candidate = *rng.pick(PALETTE);
        }
        candidate
    };

    // Pick the emblem's position uniformly from the 5 options (center + 4
    // corners). No emblem -> Center is still stored as a harmless default.
    let emblem_position = if emblem == Emblem::None {
        EmblemPosition::Center
    } else {
        *rng.pick(EmblemPosition::ALL)
    };

    FlagDesign {
        pattern,
        colors,
        emblem,
        emblem_color,
        emblem_position,
    }
}

/// Generate a flag without a government context — uses the default rule pools.
pub fn random(rng: &mut Rng) -> FlagDesign {
    let rules = FlagRules::default();
    random_for(rng, GovernmentForm::Republic, &rules)
}

// ── SVG rendering ───────────────────────────────────────────────

fn rect(x: i32, y: i32, w: i32, h: i32, color: FlagColor) -> String {
    format!(
        r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}"/>"#,
        fill = color.hex()
    )
}

fn emblem_svg(emblem: Emblem, color: FlagColor, cx: i32, cy: i32, size: i32) -> String {
    let c = color.hex();
    match emblem {
        Emblem::None => String::new(),
        Emblem::Disk => format!(r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{c}"/>"#, r = size / 2),
        Emblem::Star => {
            // 5-point star as a polygon.
            let r = size as f32 / 2.0;
            let mut points = String::new();
            for i in 0..10 {
                let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                let radius = if i % 2 == 0 { r } else { r * 0.45 };
                let px = cx as f32 + angle.cos() * radius;
                let py = cy as f32 + angle.sin() * radius;
                if i > 0 {
                    points.push(' ');
                }
                points.push_str(&format!("{px:.2},{py:.2}"));
            }
            format!(r#"<polygon points="{points}" fill="{c}"/>"#)
        }
        Emblem::Sun => {
            let r = size as f32 / 2.0;
            let mut rays = String::new();
            for i in 0..12 {
                let angle = i as f32 * std::f32::consts::PI / 6.0;
                let x1 = cx as f32 + angle.cos() * r * 0.55;
                let y1 = cy as f32 + angle.sin() * r * 0.55;
                let x2 = cx as f32 + angle.cos() * r;
                let y2 = cy as f32 + angle.sin() * r;
                rays.push_str(&format!(
                    r#"<line x1="{x1:.2}" y1="{y1:.2}" x2="{x2:.2}" y2="{y2:.2}" stroke="{c}" stroke-width="1"/>"#
                ));
            }
            format!(
                r#"<g>{rays}<circle cx="{cx}" cy="{cy}" r="{r:.2}" fill="{c}"/></g>"#,
                r = r * 0.4
            )
        }
        Emblem::Crescent => {
            // Build a crescent as a filled path — an outer arc minus an inner
            // arc offset to the right. Avoids needing a background color.
            let r = size / 2;
            let off = size / 4;
            format!(
                r#"<path d="M {xa} {cy} A {r} {r} 0 1 1 {xa} {cyp} A {ri} {ri} 0 1 0 {xb} {cy}" fill="{c}"/>"#,
                xa = cx - r,
                cyp = cy + 1,
                ri = r - 1,
                xb = cx - r + off,
            )
        }
        Emblem::Cross => {
            let arm = size / 3;
            let thick = (size / 5).max(2);
            format!(
                "{}{}",
                rect(cx - thick / 2, cy - arm, thick, arm * 2, color),
                rect(cx - arm, cy - thick / 2, arm * 2, thick, color),
            )
        }
        Emblem::Wheel => {
            let r = size / 2;
            let mut spokes = String::new();
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::PI / 4.0;
                let x2 = cx as f32 + angle.cos() * r as f32;
                let y2 = cy as f32 + angle.sin() * r as f32;
                spokes.push_str(&format!(
                    r#"<line x1="{cx}" y1="{cy}" x2="{x2:.2}" y2="{y2:.2}" stroke="{c}" stroke-width="1"/>"#
                ));
            }
            format!(
                r#"<g>{spokes}<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{c}" stroke-width="1.5"/></g>"#
            )
        }
        Emblem::Anchor => {
            let r = size / 2;
            format!(
                r#"<g stroke="{c}" stroke-width="1.5" fill="none"><line x1="{cx}" y1="{y0}" x2="{cx}" y2="{y1}"/><path d="M {x0} {y1} Q {cx} {y2} {x1} {y1}"/><circle cx="{cx}" cy="{y0}" r="1.5" fill="{c}"/></g>"#,
                y0 = cy - r,
                y1 = cy + r / 2,
                y2 = cy + r,
                x0 = cx - r / 2,
                x1 = cx + r / 2,
            )
        }
        Emblem::Saltire => {
            let r = size / 2;
            format!(
                r#"<g stroke="{c}" stroke-width="2" fill="none"><line x1="{x0}" y1="{y0}" x2="{x1}" y2="{y1}"/><line x1="{x0}" y1="{y1}" x2="{x1}" y2="{y0}"/></g>"#,
                x0 = cx - r,
                x1 = cx + r,
                y0 = cy - r,
                y1 = cy + r,
            )
        }
    }
}

/// Render a flag as a self-contained SVG string (3:2, 60×40 viewBox).
pub fn svg_for(design: &FlagDesign) -> String {
    let w = FLAG_WIDTH as i32;
    let h = FLAG_HEIGHT as i32;
    let mut body = String::new();

    match design.pattern {
        FlagPattern::Solid => {
            let c = *design.colors.first().unwrap_or(&FlagColor::White);
            body.push_str(&rect(0, 0, w, h, c));
        }
        FlagPattern::HorizontalBicolor => {
            let c = &design.colors;
            body.push_str(&rect(0, 0, w, h / 2, c[0]));
            body.push_str(&rect(0, h / 2, w, h / 2, c[1]));
        }
        FlagPattern::VerticalBicolor => {
            let c = &design.colors;
            body.push_str(&rect(0, 0, w / 2, h, c[0]));
            body.push_str(&rect(w / 2, 0, w / 2, h, c[1]));
        }
        FlagPattern::HorizontalTricolor => {
            let c = &design.colors;
            let band = h / 3;
            body.push_str(&rect(0, 0, w, band, c[0]));
            body.push_str(&rect(0, band, w, band, c[1]));
            body.push_str(&rect(0, band * 2, w, h - band * 2, c[2]));
        }
        FlagPattern::VerticalTricolor => {
            let c = &design.colors;
            let band = w / 3;
            body.push_str(&rect(0, 0, band, h, c[0]));
            body.push_str(&rect(band, 0, band, h, c[1]));
            body.push_str(&rect(band * 2, 0, w - band * 2, h, c[2]));
        }
        FlagPattern::Quartered => {
            let c = &design.colors;
            body.push_str(&rect(0, 0, w / 2, h / 2, c[0]));
            body.push_str(&rect(w / 2, 0, w / 2, h / 2, c[1]));
            body.push_str(&rect(0, h / 2, w / 2, h / 2, c[2]));
            body.push_str(&rect(w / 2, h / 2, w / 2, h / 2, c[3]));
        }
        FlagPattern::NordicCross => {
            let c = &design.colors;
            body.push_str(&rect(0, 0, w, h, c[0]));
            // Vertical stripe offset to the left (Nordic placement).
            let cross_thick = 6;
            let vx = (w / 3) - cross_thick / 2;
            let hy = (h / 2) - cross_thick / 2;
            body.push_str(&rect(vx, 0, cross_thick, h, c[1]));
            body.push_str(&rect(0, hy, w, cross_thick, c[1]));
        }
        FlagPattern::CantonStripes => {
            let c = &design.colors;
            let stripe_h = h / 5;
            for i in 0..5 {
                let color = if i % 2 == 0 { c[1] } else { c[2] };
                body.push_str(&rect(0, i * stripe_h, w, stripe_h, color));
            }
            // Canton.
            body.push_str(&rect(0, 0, w * 2 / 5, h * 3 / 5, c[0]));
        }
    }

    let (ex, ey, esize) = design.emblem_position.layout();
    body.push_str(&emblem_svg(design.emblem, design.emblem_color, ex, ey, esize));

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">{body}</svg>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_enough_colors_for_any_pattern() {
        assert!(PALETTE.len() >= 4);
    }

    #[test]
    fn svg_contains_svg_tag() {
        let mut rng = Rng::from_seed(1);
        for _ in 0..20 {
            let d = random(&mut rng);
            let svg = svg_for(&d);
            assert!(svg.starts_with("<svg"));
            assert!(svg.ends_with("</svg>"));
        }
    }

    #[test]
    fn random_is_deterministic() {
        let mut a = Rng::from_seed(99);
        let mut b = Rng::from_seed(99);
        for _ in 0..10 {
            let da = random(&mut a);
            let db = random(&mut b);
            assert_eq!(da.pattern, db.pattern);
            assert_eq!(da.colors, db.colors);
            assert_eq!(da.emblem, db.emblem);
        }
    }

    #[test]
    fn colors_are_distinct() {
        let mut rng = Rng::from_seed(2);
        for _ in 0..20 {
            let d = random(&mut rng);
            for i in 0..d.colors.len() {
                for j in (i + 1)..d.colors.len() {
                    assert_ne!(d.colors[i], d.colors[j]);
                }
            }
        }
    }

    #[test]
    fn emblem_positions_cover_all_corners_and_center() {
        // Across enough samples with emblem != None, every position should
        // be hit at least once — otherwise placement is biased.
        let mut rng = Rng::from_seed(1234);
        let rules = FlagRules::default();
        let mut positions = std::collections::HashSet::new();
        for _ in 0..2_000 {
            let d = random_for(&mut rng, GovernmentForm::Kingdom, &rules);
            if d.emblem != Emblem::None {
                positions.insert(d.emblem_position);
            }
            if positions.len() == EmblemPosition::ALL.len() {
                break;
            }
        }
        assert_eq!(positions.len(), EmblemPosition::ALL.len());
    }

    #[test]
    fn emblem_layout_stays_in_bounds() {
        // Every layout must keep the emblem's bounding box inside the
        // canvas. (cx ± size/2) must be within [0, w] and (cy ± size/2)
        // within [0, h].
        let w = FLAG_WIDTH as i32;
        let h = FLAG_HEIGHT as i32;
        for pos in EmblemPosition::ALL {
            let (cx, cy, size) = pos.layout();
            assert!(cx - size / 2 >= 0 && cx + size / 2 <= w);
            assert!(cy - size / 2 >= 0 && cy + size / 2 <= h);
        }
    }
}
