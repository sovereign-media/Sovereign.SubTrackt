//! Development tooling. Not shipped, and not part of the binary.
//!
//! `gen-reference` renders a font into a reference glyph set. The rendering itself moved to
//! `subtrackt_glyph::font` behind the `font` feature, so a downloaded release binary can make its
//! own sets without the repository — #80. xtask keeps the subcommand and calls that same code, so
//! the two cannot drift into producing sets that disagree.
//!
//! ```console
//! $ cargo run -p xtask -- gen-reference C:/Windows/Fonts/arial.ttf arial.subtref --name arial
//! ```

mod accuracy;
mod bigram;
mod bodybox;
mod disc;
mod dump;
mod fit;
mod fixture;
mod fontid;
mod geometry;
mod mark;
mod pairs;
mod refmatch;
mod rendersweep;
mod select;
mod separability;
mod spacing;
mod stability;
mod sweep;
mod unread;
mod voting;
mod widthsweep;

use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt_glyph::font::{Crop, Face, Rendering, generate_under};
use subtrackt_glyph::reference::Style;

/// Parse `--renderings 21:160,50:160` into a rendering list.
///
/// xtask only, and deliberately not on the shipped CLI. #99 chose `font::RENDERINGS` by sweeping
/// candidates against a real disc, and that sweep needs to generate sets the tool would never write
/// — including the pre-#99 rendering, spelled `96:128:raster`. A user has no such question; a user
/// wants the set the measurement chose.
fn parse_renderings(spec: &str) -> anyhow::Result<Vec<Rendering>> {
    spec.split(',')
        .map(|item| {
            let mut parts = item.split(':');
            let px: f32 = parts
                .next()
                .context("a rendering needs a size")?
                .trim()
                .parse()
                .with_context(|| format!("{item:?} is not px[:ink[:raster|ink]]"))?;
            let ink: u8 = match parts.next() {
                Some(value) => value.trim().parse()?,
                None => 160,
            };
            let crop = match parts.next() {
                Some("raster") => Crop::Raster,
                Some("ink") | None => Crop::Ink,
                Some(other) => anyhow::bail!("unknown crop {other:?}; use `raster` or `ink`"),
            };
            Ok(Rendering { px, ink, crop })
        })
        .collect()
}

// Re-exported so the benches keep spelling these `crate::charset()` and `crate::RENDER_PX`.
// They are the same items the CLI uses; there is no second copy to keep in step.
pub(crate) use subtrackt_glyph::font::{RENDER_PX, charset, vector_for};

fn gen_reference(args: &[String]) -> anyhow::Result<()> {
    let grey = args.iter().any(|a| a == "--grey-coverage");
    let font_path = args
        .first()
        .context("usage: gen-reference <font.ttf> <out> [--name N]")?;
    let out: PathBuf = args.get(1).context("missing output path")?.into();

    let name = match args.iter().position(|a| a == "--name") {
        Some(at) => args.get(at + 1).cloned().context("--name needs a value")?,
        None => PathBuf::from(font_path)
            .file_stem()
            .map_or_else(|| "unnamed".to_owned(), |s| s.to_string_lossy().into_owned()),
    };

    // Additional faces of the same typeface, each contributing its own vector for every character.
    // #66 is the measurement behind this: a track that changes style mid-film is read by whichever
    // face is closer to the ink, and the style byte exists so both can be present at once.
    let mut paths = vec![(font_path.clone(), Style::Regular)];
    for (flag, style) in [("--italic", Style::Italic), ("--bold", Style::Bold)] {
        if let Some(at) = args.iter().position(|a| a == flag) {
            let path = args
                .get(at + 1)
                .cloned()
                .with_context(|| format!("{flag} needs a font"))?;
            paths.push((path, style));
        }
    }

    let mut loaded = Vec::new();
    for (path, style) in &paths {
        let bytes = std::fs::read(path).with_context(|| format!("reading {path}"))?;
        loaded.push((path.clone(), *style, bytes));
    }
    let faces: Vec<Face<'_>> = loaded
        .iter()
        .map(|(_, style, bytes)| Face { bytes, style: *style })
        .collect();

    let renderings = match args.iter().position(|a| a == "--renderings") {
        Some(at) => parse_renderings(args.get(at + 1).context("--renderings needs a value")?)?,
        None => subtrackt_glyph::font::RENDERINGS.to_vec(),
    };
    let generated = generate_under(name, &faces, grey, &renderings)
        .with_context(|| format!("generating a reference set from {font_path}"))?;
    for style in &generated.without_cap_height {
        eprintln!(
            "  the {style:?} face rasterises no capital H; its entries carry no line metrics"
        );
    }

    let encoded = generated.set.encode();
    std::fs::write(&out, &encoded).with_context(|| format!("writing {}", out.display()))?;

    eprintln!(
        "{}: {} glyphs, {} bytes -> {}",
        generated.set.name(),
        generated.set.len(),
        encoded.len(),
        out.display()
    );
    if !generated.missing.is_empty() {
        // Space and a few marks legitimately rasterise to nothing; anything else is worth knowing.
        eprintln!(
            "  no outline for {} characters: {:?}",
            generated.missing.len(),
            generated.missing
        );
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("gen-reference") => return gen_reference(&args[1..]),
        Some("measure-stability") => return stability::measure(&args[1..]),
        Some("reference-render") => return refmatch::run(&args[1..]),
        Some("render-sweep") => return rendersweep::run(&args[1..]),
        Some("make-fixture") => return fixture::make(&args[1..]),
        Some("accuracy") => return accuracy::run(&args[1..]),
        Some("reference-fit") => return fit::run(&args[1..]),
        Some("fit-select") => return select::run(&args[1..]),
        Some("font-id") => return fontid::run(&args[1..]),
        Some("cluster-sweep") => return sweep::run(&args[1..]),
        Some("metric-sweep") => return sweep::run_metric(&args[1..]),
        Some("mark-sweep") => return sweep::run_mark(&args[1..]),
        Some("separability") => return separability::run(&args[1..]),
        Some("body-box") => return bodybox::run(&args[1..]),
        Some("set-pairs") => return pairs::run(&args[1..]),
        Some("spacing-margin") => return spacing::run(&args[1..]),
        Some("srt-score") => return disc::run(&args[1..]),
        Some("dump-sup") => return dump::run(&args[1..]),
        Some("glyph-geometry") => return geometry::run(&args[1..]),
        Some("unread") => return unread::run(&args[1..]),
        Some("shape-votes") => return voting::run(&args[1..]),
        Some("width-sweep") => return widthsweep::run(&args[1..]),
        _ => {}
    }
    eprintln!("usage:");
    eprintln!(
        "  xtask gen-reference <font.ttf> <out.subtref> [--name NAME] [--italic F] [--bold F] [--renderings px:ink:crop,...]"
    );
    eprintln!("  xtask measure-stability <regular.ttf> [bold] [italic] [bold-italic]");
    eprintln!("  xtask reference-render [font.ttf]");
    eprintln!("  xtask render-sweep [font.ttf]");
    eprintln!("  xtask make-fixture <font.ttf> <out-dir> [--px N]");
    eprintln!("  xtask accuracy [font.ttf]");
    eprintln!("  xtask reference-fit <material.ttf> <candidate.ttf>...");
    eprintln!("  xtask fit-select <font.ttf>...");
    eprintln!("  xtask font-id <font.ttf>... [--continue]");
    eprintln!("  xtask set-pairs <set.subtref>...");
    eprintln!("  xtask spacing-margin [font.ttf]...");
    eprintln!("  xtask mark-sweep [font.ttf]");
    eprintln!("  xtask body-box [font.ttf]");
    eprintln!("  xtask srt-score <extracted.srt> <release.srt> [--compare <other.srt>]");
    eprintln!("  xtask dump-sup <media> <out.sup> [--stream N]");
    eprintln!("  xtask glyph-geometry <media> <reference.subtref> <release.srt> [--pair lI]");
    eprintln!("  xtask unread <media> <reference.subtref>");
    eprintln!("  xtask shape-votes <media> <reference.subtref>");
    eprintln!("  xtask width-sweep <media> <reference.subtref> <release.srt> [--post-correct]");
    std::process::exit(2);
}
