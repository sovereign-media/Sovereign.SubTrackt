//! Development tooling. Not shipped, and not part of the binary.
//!
//! `gen-reference` renders a font into a reference glyph set. The important property is that it
//! rasterises the glyph and then hands it to `subtrackt_glyph::feature::vectorize` — the *same*
//! normalisation the runtime uses on decoded subtitle bitmaps. Reference vectors produced any
//! other way would be comparing against a subtly different transform, and the resulting distances
//! would mean nothing.
//!
//! ```console
//! $ cargo run -p xtask -- gen-reference C:/Windows/Fonts/arial.ttf arial.subtref --name arial
//! ```

mod accuracy;
mod disc;
mod fit;
mod fixture;
mod mark;
mod pairs;
mod select;
mod separability;
mod spacing;
mod stability;
mod sweep;

use std::path::PathBuf;

use anyhow::{Context as _, bail};
use fontdue::{Font, FontSettings};
use subtrackt_core::{MarkSlope, Rect};
use subtrackt_glyph::binarize::{BinaryMask, CoverageMask};
use subtrackt_glyph::ccl::{self, ComponentFilter};
use subtrackt_glyph::feature::{AspectPolicy, vectorize, vectorize_coverage};
use subtrackt_glyph::group::GroupedGlyph;
use subtrackt_glyph::reference::{ReferenceEntry, ReferenceSet, Style};

/// Pixel size glyphs are rasterised at.
///
/// Larger than any real subtitle glyph on purpose. Normalisation is scale-invariant, so the only
/// thing size buys here is a cleaner rasterisation to normalise *from*.
pub(crate) const RENDER_PX: f32 = 96.0;

/// Coverage above which a rasterised pixel counts as ink.
///
/// Matches the binarizer's default of half, so a reference glyph is thresholded the same way a
/// decoded one is.
const INK: u8 = 128;

/// What a reference set covers.
///
/// ASCII printable, plus the Latin-1 letters that carry the accents #6 works to preserve. There is
/// no point including a character the segmenter cannot deliver as one glyph.
pub(crate) fn charset() -> Vec<char> {
    let mut chars: Vec<char> = (0x21u8..0x7F).map(char::from).collect();
    chars.extend("\u{c0}\u{c1}\u{c2}\u{c4}\u{c7}\u{c8}\u{c9}\u{ca}\u{cb}".chars());
    chars.extend("\u{cc}\u{cd}\u{ce}\u{cf}\u{d1}\u{d2}\u{d3}\u{d4}\u{d6}".chars());
    chars.extend("\u{d9}\u{da}\u{db}\u{dc}\u{df}\u{e0}\u{e1}\u{e2}\u{e4}".chars());
    chars.extend("\u{e7}\u{e8}\u{e9}\u{ea}\u{eb}\u{ec}\u{ed}\u{ee}\u{ef}".chars());
    chars.extend("\u{f1}\u{f2}\u{f3}\u{f4}\u{f6}\u{f9}\u{fa}\u{fb}\u{fc}".chars());
    chars
}

/// Rasterise one character and normalise it exactly as the runtime would.
///
/// `grey` must match the pipeline's `grey_coverage` setting. A reference built through a different
/// normalisation than the runtime uses would be compared against a subtly different transform, and
/// every distance it produced would be meaningless.
pub(crate) fn vector_for(
    font: &Font,
    ch: char,
    grey: bool,
) -> Option<subtrackt_core::FeatureVector> {
    let (metrics, coverage) = font.rasterize(ch, RENDER_PX);
    if metrics.width == 0 || metrics.height == 0 {
        return None;
    }

    let width = u32::try_from(metrics.width).ok()?;
    let height = u32::try_from(metrics.height).ok()?;
    let bits: Vec<bool> = coverage.iter().map(|c| *c >= INK).collect();
    let mask = BinaryMask::from_bits(width, height, bits).ok()?;

    // A glyph rendered by itself is already tightly cropped, which is what connected components
    // hand the runtime vectorizer.
    if mask.foreground_count() == 0 {
        return None;
    }

    let bounds = Rect::new(0, 0, width, height);
    if grey {
        // fontdue hands back per-pixel coverage already, which is exactly what the runtime derives
        // from a subtitle palette. No thresholding step at all on this path.
        let plane = CoverageMask::from_values(width, height, coverage).ok()?;
        return vectorize_coverage(&plane, bounds, AspectPolicy::Letterbox).ok();
    }
    vectorize(&mask, bounds, AspectPolicy::Letterbox).ok()
}

/// Where a character stands in a line of text, from the font's own metrics.
///
/// This has to mean the same thing as `subtrackt_glyph::metrics`, which derives its anchors from a
/// rendered line's ink. So the unit here is the *ink* height of a capital H rather than a figure
/// from a font table: the runtime's cap height is the row the tall glyphs actually reach, and a
/// table value would include margins the pixels never show.
fn metrics_for(font: &Font, ch: char, cap_height: i32) -> subtrackt_core::LineMetrics {
    if cap_height <= 0 {
        return subtrackt_core::LineMetrics::UNKNOWN;
    }
    let metrics = font.metrics(ch, RENDER_PX);
    if metrics.height == 0 {
        return subtrackt_core::LineMetrics::UNKNOWN;
    }

    let height = i32::try_from(metrics.height).unwrap_or(0) * 100 / cap_height;
    // fontdue reports ymin as the offset of the bitmap's bottom from the baseline: negative for a
    // descender, positive for a mark floating clear of the baseline. The runtime measures downwards
    // from the baseline, so the sign flips.
    let descent = -metrics.ymin * 100 / cap_height;

    subtrackt_core::LineMetrics::new(u32::try_from(height).unwrap_or(0), descent)
}

/// Which way a character's diacritic leans, from an isolated render.
///
/// Runs the *shipped* `mark::slope` over the character's own connected components rather than
/// reimplementing the rule, so a reference entry and a decoded glyph are measured by the same code.
/// `group` is skipped because there is nothing to skip: a character rendered on its own is already
/// one glyph, and its components are exactly the parts `group` would hand over. (It could not be
/// run here anyway — a lone `é` has a blank row between its accent and its body, so `line_bands`
/// would band it as two lines and never attach the mark. See `docs/glyph-stability.md`.)
pub(crate) fn mark_for(font: &Font, ch: char) -> MarkSlope {
    let (metrics, coverage) = font.rasterize(ch, RENDER_PX);
    let (Ok(width), Ok(height)) = (u32::try_from(metrics.width), u32::try_from(metrics.height))
    else {
        return MarkSlope::NONE;
    };
    if width == 0 || height == 0 {
        return MarkSlope::NONE;
    }
    let bits: Vec<bool> = coverage.iter().map(|c| *c >= INK).collect();
    let Ok(mask) = BinaryMask::from_bits(width, height, bits) else {
        return MarkSlope::NONE;
    };
    let Ok(parts) = ccl::label(&mask, ComponentFilter::permissive()) else {
        return MarkSlope::NONE;
    };
    subtrackt_glyph::mark::slope(&mask, &GroupedGlyph { parts, line: 0 })
}

pub(crate) fn gen_reference(args: &[String]) -> anyhow::Result<()> {
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
    let mut faces = vec![(font_path.clone(), Style::Regular)];
    for (flag, style) in [("--italic", Style::Italic), ("--bold", Style::Bold)] {
        if let Some(at) = args.iter().position(|a| a == flag) {
            let path = args
                .get(at + 1)
                .cloned()
                .with_context(|| format!("{flag} needs a font"))?;
            faces.push((path, style));
        }
    }

    let mut entries = Vec::new();
    let mut missing = Vec::new();
    for (path, style) in &faces {
        let bytes = std::fs::read(path).with_context(|| format!("reading {path}"))?;
        let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
            .map_err(|e| anyhow::anyhow!("{path} is not a usable font: {e}"))?;

        // The unit every metric is a fraction of, taken per face: an italic and an upright cut of
        // one typeface do not share a cap height exactly, and scaling one against the other's would
        // make every metric slightly wrong in a way nothing would report.
        let cap_height = i32::try_from(font.metrics('H', RENDER_PX).height).unwrap_or(0);
        if cap_height <= 0 {
            eprintln!("  {path} rasterises no capital H; entries will carry no line metrics");
        }

        for ch in charset() {
            match vector_for(&font, ch, grey) {
                Some(features) => {
                    entries.push(ReferenceEntry {
                        character: ch,
                        style: *style,
                        features,
                        metrics: metrics_for(&font, ch, cap_height),
                        mark: mark_for(&font, ch),
                    });
                }
                None if *style == Style::Regular => missing.push(ch),
                None => {}
            }
        }
    }

    if entries.is_empty() {
        bail!("{font_path} produced no glyphs at all");
    }

    let set = ReferenceSet::new(name, entries);
    let encoded = set.encode();
    std::fs::write(&out, &encoded).with_context(|| format!("writing {}", out.display()))?;

    eprintln!(
        "{}: {} glyphs, {} bytes -> {}",
        set.name(),
        set.len(),
        encoded.len(),
        out.display()
    );
    if !missing.is_empty() {
        // Space and a few marks legitimately rasterise to nothing; anything else is worth knowing.
        eprintln!("  no outline for {} characters: {:?}", missing.len(), missing);
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("gen-reference") => return gen_reference(&args[1..]),
        Some("measure-stability") => return stability::measure(&args[1..]),
        Some("make-fixture") => return fixture::make(&args[1..]),
        Some("accuracy") => return accuracy::run(&args[1..]),
        Some("reference-fit") => return fit::run(&args[1..]),
        Some("fit-select") => return select::run(&args[1..]),
        Some("cluster-sweep") => return sweep::run(&args[1..]),
        Some("metric-sweep") => return sweep::run_metric(&args[1..]),
        Some("mark-sweep") => return sweep::run_mark(&args[1..]),
        Some("separability") => return separability::run(&args[1..]),
        Some("set-pairs") => return pairs::run(&args[1..]),
        Some("spacing-margin") => return spacing::run(&args[1..]),
        Some("srt-score") => return disc::run(&args[1..]),
        _ => {}
    }
    eprintln!("usage:");
    eprintln!(
        "  xtask gen-reference <font.ttf> <out.subtref> [--name NAME] [--italic F] [--bold F]"
    );
    eprintln!("  xtask measure-stability <regular.ttf> [bold] [italic] [bold-italic]");
    eprintln!("  xtask make-fixture <font.ttf> <out-dir> [--px N]");
    eprintln!("  xtask accuracy [font.ttf]");
    eprintln!("  xtask reference-fit <material.ttf> <candidate.ttf>...");
    eprintln!("  xtask fit-select <font.ttf>...");
    eprintln!("  xtask set-pairs <set.subtref>...");
    eprintln!("  xtask spacing-margin [font.ttf]...");
    eprintln!("  xtask mark-sweep [font.ttf]");
    eprintln!("  xtask srt-score <extracted.srt> <release.srt> [--compare <other.srt>]");
    std::process::exit(2);
}
