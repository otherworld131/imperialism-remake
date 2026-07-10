//! Asset generation pipeline (milestone M4).
//!
//! Walks `assets-src/icons/**/*.svg`, rasterizes each source at 64x64 with a
//! transparent background, and writes the result to
//! `crates/presentation/assets/icons/<group>/<name>.png`.
//!
//! The tool is idempotent: re-running it simply overwrites the PNGs with
//! identical content. Each written PNG is re-decoded to verify it is a valid
//! 64x64 image before being counted as a success.
//!
//! Run with: `cargo run -p gen_assets`

use std::path::{Path, PathBuf};

/// Output raster size in pixels (square).
const SIZE: u32 = 64;

fn main() {
    // tools/gen_assets -> repo root is two levels up.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf();
    let src_root = root.join("assets-src/icons");
    let out_root = root.join("crates/presentation/assets/icons");

    let mut svgs = Vec::new();
    collect_svgs(&src_root, &mut svgs);
    svgs.sort();

    if svgs.is_empty() {
        eprintln!("no SVG sources found under {}", src_root.display());
        std::process::exit(1);
    }

    let mut written = 0usize;
    let mut failed = 0usize;
    let mut groups: Vec<(String, usize)> = Vec::new();

    for svg_path in &svgs {
        let rel = svg_path
            .strip_prefix(&src_root)
            .expect("svg under source root");
        let out_path = out_root.join(rel.with_extension("png"));
        let native = rel.starts_with("splash");
        match rasterize(svg_path, &out_path, native) {
            Ok(()) => {
                written += 1;
                let group = rel
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match groups.iter_mut().find(|(g, _)| *g == group) {
                    Some((_, n)) => *n += 1,
                    None => groups.push((group, 1)),
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("FAIL {}: {}", rel.display(), e);
            }
        }
    }

    println!("gen_assets: {written} icons rasterized at {SIZE}x{SIZE} ({failed} failed)");
    for (group, n) in &groups {
        println!("  {group:<16} {n:>3}");
    }
    println!("  output: {}", out_root.display());

    if failed > 0 {
        std::process::exit(1);
    }
}

/// Recursively collect every `.svg` file under `dir`.
fn collect_svgs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_svgs(&path, out);
        } else if path.extension().is_some_and(|e| e == "svg") {
            out.push(path);
        }
    }
}

/// Rasterize one SVG to a transparent-background PNG, then verify the
/// written file decodes back to the expected dimensions. Icons render at
/// 64x64; the `splash/` group renders at the SVG's native pixel grid
/// (one output pixel per authored pixel — the game upscales with
/// nearest-neighbor at draw time).
fn rasterize(svg_path: &Path, out_path: &Path, native: bool) -> Result<(), String> {
    let data = std::fs::read(svg_path).map_err(|e| format!("read: {e}"))?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default())
        .map_err(|e| format!("parse: {e}"))?;

    let size = tree.size();
    let (out_w, out_h) = if native {
        (size.width().round() as u32, size.height().round() as u32)
    } else {
        (SIZE, SIZE)
    };
    let mut pixmap = tiny_skia::Pixmap::new(out_w, out_h).ok_or("pixmap alloc")?;
    let transform =
        tiny_skia::Transform::from_scale(out_w as f32 / size.width(), out_h as f32 / size.height());
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    pixmap
        .save_png(out_path)
        .map_err(|e| format!("write png: {e}"))?;

    // Verification pass: the PNG on disk must decode at the expected size.
    let bytes = std::fs::read(out_path).map_err(|e| format!("re-read: {e}"))?;
    let decoded = tiny_skia::Pixmap::decode_png(&bytes).map_err(|e| format!("decode: {e}"))?;
    if decoded.width() != out_w || decoded.height() != out_h {
        return Err(format!(
            "decoded size {}x{}, expected {out_w}x{out_h}",
            decoded.width(),
            decoded.height()
        ));
    }
    Ok(())
}
