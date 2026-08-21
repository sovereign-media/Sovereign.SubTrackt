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

use std::path::PathBuf;

use anyhow::{Context as _, bail};
use fontdue::{Font, FontSettings};
use subtrackt_core::Rect;
use subtrackt_glyph::binarize::BinaryMask;
use subtrackt_glyph::feature::{AspectPolicy, vectorize};
use subtrackt_glyph::reference::{ReferenceEntry, ReferenceSet, Style};

/// Pixel size glyphs are rasterised at.
///
/// Larger than any real subtitle glyph on purpose. Normalisation is scale-invariant, so the only
/// thing size buys here is a cleaner rasterisation to normalise *from*.
const RENDER_PX: f32 = 96.0;

/// Coverage above which a rasterised pixel counts as ink.
///
/// Matches the binarizer's default of half, so a reference glyph is thresholded the same way a
/// decoded one is.
const INK: u8 = 128;

/// What a reference set covers.
///
/// ASCII printable, plus the Latin-1 letters that carry the accents #6 works to preserve. There is
/// no point including a character the segmenter cannot deliver as one glyph.
fn charset() -> Vec<char> {
    let mut chars: Vec<char> = (0x21u8..0x7F).map(char::from).collect();
    chars.extend("\u{c0}\u{c1}\u{c2}\u{c4}\u{c7}\u{c8}\u{c9}\u{ca}\u{cb}".chars());
    chars.extend("\u{cc}\u{cd}\u{ce}\u{cf}\u{d1}\u{d2}\u{d3}\u{d4}\u{d6}".chars());
    chars.extend("\u{d9}\u{da}\u{db}\u{dc}\u{df}\u{e0}\u{e1}\u{e2}\u{e4}".chars());
    chars.extend("\u{e7}\u{e8}\u{e9}\u{ea}\u{eb}\u{ec}\u{ed}\u{ee}\u{ef}".chars());
    chars.extend("\u{f1}\u{f2}\u{f3}\u{f4}\u{f6}\u{f9}\u{fa}\u{fb}\u{fc}".chars());
    chars
}

/// Rasterise one character and normalise it exactly as the runtime would.
fn vector_for(font: &Font, ch: char) -> Option<subtrackt_core::FeatureVector> {
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
    vectorize(&mask, Rect::new(0, 0, width, height), AspectPolicy::Letterbox).ok()
}

fn gen_reference(args: &[String]) -> anyhow::Result<()> {
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

    let bytes = std::fs::read(font_path).with_context(|| format!("reading {font_path}"))?;
    let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
        .map_err(|e| anyhow::anyhow!("{font_path} is not a usable font: {e}"))?;

    let mut entries = Vec::new();
    let mut missing = Vec::new();
    for ch in charset() {
        match vector_for(&font, ch) {
            Some(features) => {
                entries.push(ReferenceEntry { character: ch, style: Style::Regular, features });
            }
            None => missing.push(ch),
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
    if args.first().map(String::as_str) == Some("gen-reference") {
        return gen_reference(&args[1..]);
    }
    eprintln!("usage: xtask gen-reference <font.ttf> <out.subtref> [--name NAME]");
    std::process::exit(2);
}
