//! Does line-relative size actually separate the pairs the shape vector cannot?
//!
//! The falsification step for
//! [#37](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/37), run before building
//! anything. #10 established that `I`, `l` and `|` are the *same* 256-bit vector, and proposed that
//! a glyph's height and baseline offset relative to its text line would separate the confusions
//! that letterboxing collapses. That proposal is cheap to check and expensive to build, so it gets
//! checked first.
//!
//! Everything here comes from font metrics alone — no pipeline, no fixture. That is enough to
//! answer the necessary condition: if two characters carry the same shape *and* the same
//! line-relative metrics, no combination of the two will ever tell them apart, and the idea is dead
//! before any code is written for it.
//!
//! The prediction on record in #37 is specific, so it can be wrong: the case pairs (`o`/`O`,
//! `c`/`C`, `u`/`U`) separate cleanly at roughly 52 against 100, and `I`/`l` does *not*, because
//! Arial's lowercase ascender and cap height differ by under 2%.

use std::path::Path;

use anyhow::{Context as _, bail};
use fontdue::{Font, FontSettings};
use subtrackt_core::FeatureVector;

/// One character's shape and where it sits in its line.
struct Measured {
    character: char,
    shape: FeatureVector,
    /// Height as a percentage of the font's cap height.
    height: i32,
    /// How far the glyph's bottom sits below the baseline, as a percentage of cap height.
    descent: i32,
}

/// Combined distance in cells: shape distance plus a weighted metric penalty.
///
/// The weight is what a future implementation would have to choose by measurement. Half is used
/// here only to make the diagnostic readable — a 48-point height difference becomes 24 cells, which
/// is comparable to the median distance between two genuinely different shapes.
fn combined(a: &Measured, b: &Measured, weight_percent: u32) -> u32 {
    let metric = a.height.abs_diff(b.height) + a.descent.abs_diff(b.descent);
    a.shape.distance(&b.shape) + metric * weight_percent / 100
}

/// Measure every character in the reference charset.
fn measure(font: &Font) -> anyhow::Result<Vec<Measured>> {
    // Cap height, taken from a capital H rather than from a table, because it has to mean the same
    // thing as the runtime's anchor — and the runtime's anchor is the tallest ink on the line.
    let unit = font.metrics('H', crate::RENDER_PX).height;
    if unit == 0 {
        bail!("the font rasterises no capital H, so there is no cap height to measure against");
    }
    let unit = i32::try_from(unit).unwrap_or(1);

    let mut out = Vec::new();
    for character in crate::charset() {
        let Some(shape) = crate::vector_for(font, character, false) else {
            continue;
        };
        let metrics = font.metrics(character, crate::RENDER_PX);
        let height = i32::try_from(metrics.height).unwrap_or(0) * 100 / unit;
        // fontdue reports ymin as the offset of the bitmap's bottom from the baseline, negative
        // when a glyph descends.
        let descent = -metrics.ymin * 100 / unit;
        out.push(Measured { character, shape, height, descent });
    }
    Ok(out)
}

/// Print the closest pairs under shape alone and under shape plus metrics.
fn report(measured: &[Measured], weight_percent: u32) {
    let mut pairs: Vec<(u32, u32, &Measured, &Measured)> = Vec::new();
    for (index, a) in measured.iter().enumerate() {
        for b in &measured[index + 1..] {
            pairs.push((a.shape.distance(&b.shape), combined(a, b, weight_percent), a, b));
        }
    }

    pairs.sort_unstable_by_key(|(shape, _, a, b)| (*shape, a.character, b.character));
    println!("\n--- the pairs the shape vector cannot separate ---");
    println!("  pair    shape   +metrics   heights      descents");
    for (shape, with, a, b) in pairs.iter().take(16) {
        let verdict = if *with == *shape {
            "  <-- STILL TIED"
        } else {
            ""
        };
        println!(
            "  {} / {}  {shape:>5}   {with:>8}   {:>3} / {:<3}   {:>3} / {:<3}{verdict}",
            a.character, b.character, a.height, b.height, a.descent, b.descent
        );
    }

    let tied_before = pairs.iter().filter(|(shape, _, _, _)| *shape == 0).count();
    let tied_after = pairs.iter().filter(|(_, with, _, _)| *with == 0).count();
    println!("\n  pairs at distance zero: {tied_before} by shape, {tied_after} with metrics");

    // The specific prediction from #37, checked rather than asserted.
    println!("\n--- the prediction in #37, checked ---");
    let find = |c: char| measured.iter().find(|m| m.character == c);
    for (a, b, expectation) in [
        ('o', 'O', "should separate"),
        ('c', 'C', "should separate"),
        ('u', 'U', "should separate"),
        ('s', 'S', "should separate"),
        ('I', 'l', "should NOT separate"),
        ('1', 'l', "should NOT separate"),
    ] {
        let (Some(x), Some(y)) = (find(a), find(b)) else {
            continue;
        };
        let (shape, with) = (x.shape.distance(&y.shape), combined(x, y, weight_percent));
        println!(
            "  {a} / {b}   shape {shape:>3} -> {with:>3}   (heights {:>3} / {:<3})   {expectation}",
            x.height, y.height
        );
    }
}

/// Run the separability check.
///
/// # Errors
/// Fails if no usable font can be found or the font carries no capital H to measure against.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let font_path = crate::accuracy::find_font(args.first()).context(
        "no font found; pass one explicitly, e.g. xtask separability C:/Windows/Fonts/arial.ttf",
    )?;
    println!("font: {}", font_path.display());

    let bytes = std::fs::read(Path::new(&font_path))
        .with_context(|| format!("reading {}", font_path.display()))?;
    let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
        .map_err(|e| anyhow::anyhow!("{}: {e}", font_path.display()))?;

    let weight_percent: u32 = match args.iter().position(|a| a == "--weight") {
        Some(at) => args
            .get(at + 1)
            .context("--weight needs a value")?
            .parse()?,
        None => 50,
    };

    let measured = measure(&font)?;
    println!(
        "{} characters measured, metric weight {weight_percent}%",
        measured.len()
    );
    report(&measured, weight_percent);
    Ok(())
}
