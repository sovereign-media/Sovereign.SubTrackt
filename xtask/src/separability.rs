//! The falsification bench: does a proposed feature separate what the shape vector cannot?
//!
//! Built as the cheap check for
//! [#37](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/37), run before building
//! anything. #10 established that `I`, `l` and `|` are the *same* 256-bit vector, and proposed that
//! a glyph's height and baseline offset relative to its text line would separate the confusions
//! that letterboxing collapses. That proposal was cheap to check and expensive to build, so it got
//! checked first. It held, it shipped, and the bench earned a second use.
//!
//! The second question is **hole count**. Every remaining confusion in the reference set is a pair
//! of shapes the vector places close together, and a topological count — how many enclosed
//! background regions a glyph carries — is exact at any resolution, orthogonal to the shape vector,
//! and immune to the ±1px edge term that `docs/glyph-stability.md` measured as the dominant source
//! of variance. That is the case *for* it. The case against is that at the 21px glyph heights the
//! library survey measured, the counter of an `e` is a handful of pixels and a threshold can close
//! it outright — and a feature that is confidently wrong some of the time is worse than none.
//!
//! So both halves are measured here, and **both have to pass**:
//!
//! - **Separation.** Of the pairs the shipped matcher would call ambiguous, how many do holes tell
//!   apart? A feature that separates nothing that is actually confused is not worth its bytes.
//! - **Stability.** Does one character produce one hole count across the sizes and ink thresholds
//!   real material varies over? A count that flips is a liar, and it lies in the direction that
//!   matters — asserting a difference between two renderings of the same letter.
//!
//! Everything comes from font renders alone — no pipeline, no fixture. That is enough to answer the
//! necessary condition: a feature that cannot separate the confusable pairs *here*, where the
//! rasterisation is clean and the typeface is the reference's own, will not separate them on a
//! decoded subtitle bitmap.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, bail};
use fontdue::{Font, FontSettings};
use subtrackt_core::FeatureVector;
use subtrackt_glyph::matcher::MatchThresholds;

/// Rendering sizes the hole count is checked for stability across.
///
/// These are the range `docs/library-survey.md` measured real subtitle glyphs at — 21 to 50 px —
/// rather than the 96px this harness rasterises references at. The whole risk being tested is that
/// a counter closes at the small end, so testing at the comfortable end would answer nothing.
const SURVEY_SIZES: [f32; 6] = [21.0, 24.0, 30.0, 36.0, 42.0, 50.0];

/// Ink thresholds the count is checked across, bracketing the binarizer's default of half.
///
/// This is the ±1px edge variation of `docs/glyph-stability.md` expressed at its source: moving the
/// threshold is what thickens or thins a stroke, and a thickened stroke is what closes a counter.
const INK_LEVELS: [u8; 3] = [96, 128, 160];

/// Smallest enclosed region that counts as a hole, in permille of the glyph's bounding box.
///
/// A fraction rather than a pixel count, for the reason every threshold in this project is: the
/// same glyph arrives at several resolutions. Anti-aliasing leaves single-pixel gaps that are not
/// counters, and at 21px a real counter runs to around 2% of the box, so there is room between them.
const HOLE_MIN_PERMILLE: u64 = 5;

/// One character's shape, where it sits in its line, and how many holes it carries.
struct Measured {
    character: char,
    shape: FeatureVector,
    /// Height as a percentage of the font's cap height.
    height: i32,
    /// How far the glyph's bottom sits below the baseline, as a percentage of cap height.
    descent: i32,
    /// Enclosed background regions at the reference rendering size.
    holes: u32,
}

/// Combined distance in cells: shape distance plus a weighted metric penalty.
///
/// This is what the shipped matcher computes — `MatchThresholds::distance` — so the pairs it calls
/// close are the pairs that are actually confused today.
fn combined(a: &Measured, b: &Measured, weight_percent: u32) -> u32 {
    let metric = a.height.abs_diff(b.height) + a.descent.abs_diff(b.descent);
    a.shape.distance(&b.shape) + metric * weight_percent / 100
}

/// The four-connected neighbours of a pixel index, `usize::MAX` where the edge cuts one off.
fn neighbours(index: usize, width: usize, height: usize) -> [usize; 4] {
    let (x, y) = (index % width, index / width);
    let mut out = [usize::MAX; 4];
    if x > 0 {
        out[0] = index - 1;
    }
    if x + 1 < width {
        out[1] = index + 1;
    }
    if y > 0 {
        out[2] = index - width;
    }
    if y + 1 < height {
        out[3] = index + width;
    }
    out
}

/// Count the enclosed background regions of a rasterised glyph.
///
/// Connectivity is the **dual** of the segmenter's. `subtrackt_glyph::ccl` labels foreground
/// 8-connected, so the background must be walked 4-connected: two strokes touching at a corner are
/// joined by the foreground pass, and an 8-connected background walk would leak a counter out
/// through that same corner. Getting the duality backwards under-counts precisely the tight
/// counters this measurement exists to interrogate.
fn count_holes(coverage: &[u8], width: usize, height: usize, ink: u8) -> u32 {
    if width == 0 || height == 0 {
        return 0;
    }
    let area = u64::try_from(width).unwrap_or(0) * u64::try_from(height).unwrap_or(0);
    let min_area = (area * HOLE_MIN_PERMILLE).div_ceil(1000).max(1);

    let is_background = |index: usize| coverage[index] < ink;
    let mut seen = vec![false; width * height];
    let mut stack: Vec<usize> = Vec::new();

    // Flood the outside inwards from every border pixel. Whatever background survives is enclosed.
    for x in 0..width {
        for index in [x, (height - 1) * width + x] {
            if !seen[index] && is_background(index) {
                seen[index] = true;
                stack.push(index);
            }
        }
    }
    for y in 0..height {
        for index in [y * width, y * width + width - 1] {
            if !seen[index] && is_background(index) {
                seen[index] = true;
                stack.push(index);
            }
        }
    }
    while let Some(index) = stack.pop() {
        for next in neighbours(index, width, height) {
            if next != usize::MAX && !seen[next] && is_background(next) {
                seen[next] = true;
                stack.push(next);
            }
        }
    }

    let mut holes = 0;
    for start in 0..seen.len() {
        if seen[start] || !is_background(start) {
            continue;
        }
        let mut size: u64 = 0;
        seen[start] = true;
        stack.push(start);
        while let Some(index) = stack.pop() {
            size += 1;
            for next in neighbours(index, width, height) {
                if next != usize::MAX && !seen[next] && is_background(next) {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        if size >= min_area {
            holes += 1;
        }
    }
    holes
}

/// Holes in one character at one rendering size and ink threshold.
fn holes_at(font: &Font, character: char, size: f32, ink: u8) -> Option<u32> {
    let (metrics, coverage) = font.rasterize(character, size);
    if metrics.width == 0 || metrics.height == 0 {
        return None;
    }
    Some(count_holes(&coverage, metrics.width, metrics.height, ink))
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
        let holes = holes_at(font, character, crate::RENDER_PX, 128).unwrap_or(0);
        out.push(Measured { character, shape, height, descent, holes });
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

/// Does the hole count say anything about the pairs that are still confused?
///
/// "Still confused" is not a judgement call: it is what `MatchThresholds` calls ambiguous, so the
/// set of pairs examined here is the set post-correction actually receives.
fn report_hole_separation(measured: &[Measured], weight_percent: u32) {
    let margin = MatchThresholds::default().ambiguity_margin();

    println!("\n--- hole census, as a check on the measurement itself ---");
    let mut census: BTreeMap<u32, Vec<char>> = BTreeMap::new();
    for m in measured {
        census.entry(m.holes).or_default().push(m.character);
    }
    for (holes, chars) in &census {
        let listed: String = chars.iter().collect();
        println!("  {holes} hole(s): {listed}");
    }

    let mut pairs: Vec<(u32, &Measured, &Measured)> = Vec::new();
    for (index, a) in measured.iter().enumerate() {
        for b in &measured[index + 1..] {
            pairs.push((combined(a, b, weight_percent), a, b));
        }
    }
    pairs.sort_unstable_by_key(|(with, a, b)| (*with, a.character, b.character));

    println!("\n--- do holes separate what shape and metrics cannot? ---");
    println!("  the pairs the shipped matcher would call ambiguous, closest first");
    println!("  pair    combined   holes    verdict");
    let ambiguous: Vec<_> = pairs
        .iter()
        .filter(|(with, _, _)| *with <= margin)
        .collect();
    for (with, a, b) in ambiguous.iter().take(24) {
        let verdict = if a.holes == b.holes {
            "no help"
        } else {
            "SEPARATES"
        };
        println!(
            "  {} / {}  {with:>8}   {} / {}    {verdict}",
            a.character, b.character, a.holes, b.holes
        );
    }

    let separated = ambiguous
        .iter()
        .filter(|(_, a, b)| a.holes != b.holes)
        .count();
    let total = ambiguous.len();
    let percent = (separated * 100).checked_div(total).unwrap_or(0);
    println!("\n  ambiguous pairs (combined <= {margin} cells): {total}");
    println!("  of those, holes differ on: {separated} ({percent}%)");

    // The margin that ships is tight, and a mismatched typeface inflates every distance — so a
    // feature that is useless at 7 cells could still earn its place at 20. Widening the band asks
    // whether that is true, rather than leaving it as an objection nobody measured.
    println!("\n  the same question over wider bands, since a near-miss typeface inflates every");
    println!("  distance and pushes more pairs into contention:");
    println!("  band (cells)   pairs   holes differ");
    for (low, high) in [(0, 7), (8, 15), (16, 23), (24, 31), (32, 47)] {
        let band: Vec<_> = pairs
            .iter()
            .filter(|(with, _, _)| *with >= low && *with <= high)
            .collect();
        let differ = band.iter().filter(|(_, a, b)| a.holes != b.holes).count();
        let share = (differ * 100).checked_div(band.len()).unwrap_or(0);
        println!("  {low:>4} - {high:<5}   {:>5}   {differ:>5} ({share}%)", band.len());
    }
}

/// Does a character carry the same number of holes in a typeface that is not the reference's?
///
/// The question #9 forces. An embedded reference set is by definition built from a typeface the
/// disc was not authored in, so a feature is only worth storing if it survives that gap. Hole count
/// looks resolution-proof, but it is a property of the *letterform*, and letterforms differ: a
/// double-storey `g` carries two counters where a single-storey `g` carries one.
fn report_hole_portability(reference: &Font, reference_name: &str, others: &[(String, Font)]) {
    println!("\n--- does a hole count survive a change of typeface? ---");
    if others.is_empty() {
        println!("  no comparison typefaces given; pass more fonts to check this");
        return;
    }

    for (name, font) in others {
        let mut disagreements: Vec<(char, u32, u32)> = Vec::new();
        let mut compared = 0u32;
        for character in crate::charset() {
            let (Some(here), Some(there)) = (
                holes_at(reference, character, crate::RENDER_PX, 128),
                holes_at(font, character, crate::RENDER_PX, 128),
            ) else {
                continue;
            };
            compared += 1;
            if here != there {
                disagreements.push((character, here, there));
            }
        }
        let listed: Vec<String> = disagreements
            .iter()
            .map(|(c, here, there)| format!("{c} {here}->{there}"))
            .collect();
        println!(
            "  {reference_name} vs {name}: {} of {compared} characters disagree{}{}",
            disagreements.len(),
            if listed.is_empty() { "" } else { "   " },
            listed.join("  ")
        );
    }
}

/// Is one character's hole count the same number every time it is rendered?
///
/// The kill criterion. A count that varies with rendering size or ink threshold does not report a
/// property of the character, it reports a property of the rasterisation — and it would assert a
/// difference between two renderings of one letter, which is the most damaging thing a matching
/// feature can do.
fn report_hole_stability(font: &Font, measured: &[Measured]) {
    println!("\n--- is a hole count stable across the sizes real material ships at? ---");
    println!(
        "  {} sizes ({}-{}px) x {} ink thresholds, per character",
        SURVEY_SIZES.len(),
        SURVEY_SIZES[0],
        SURVEY_SIZES[SURVEY_SIZES.len() - 1],
        INK_LEVELS.len()
    );

    let mut renderings: u64 = 0;
    let mut agreeing: u64 = 0;
    let mut unstable: Vec<(char, u32, BTreeMap<u32, u64>)> = Vec::new();

    for m in measured {
        let mut counts: BTreeMap<u32, u64> = BTreeMap::new();
        for size in SURVEY_SIZES {
            for ink in INK_LEVELS {
                if let Some(holes) = holes_at(font, m.character, size, ink) {
                    *counts.entry(holes).or_default() += 1;
                }
            }
        }
        let Some((&mode, &hits)) = counts.iter().max_by_key(|(_, count)| **count) else {
            continue;
        };
        renderings += counts.values().sum::<u64>();
        agreeing += hits;
        if counts.len() > 1 {
            unstable.push((m.character, mode, counts));
        }
    }

    let percent = (agreeing * 100).checked_div(renderings).unwrap_or(0);
    println!("  renderings: {renderings}, agreeing with their modal count: {percent}%");
    println!(
        "  characters whose count is not constant: {} of {}",
        unstable.len(),
        measured.len()
    );
    if !unstable.is_empty() {
        println!("\n  character   at 96px   what it reports across the survey range");
        for (character, mode, counts) in &unstable {
            let spread: Vec<String> = counts
                .iter()
                .map(|(holes, hits)| format!("{holes} x{hits}"))
                .collect();
            println!("  {character:>9}   {mode:>7}   {}", spread.join("   "));
        }
    }
}

/// Run the separability check.
///
/// # Errors
/// Fails if no usable font can be found or the font carries no capital H to measure against.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let weight_percent: u32 = match args.iter().position(|a| a == "--weight") {
        Some(at) => args
            .get(at + 1)
            .context("--weight needs a value")?
            .parse()?,
        None => 50,
    };

    // Positional arguments are fonts: the first is the reference, any others are typefaces to check
    // the hole count's portability against.
    let weight_value = args.iter().position(|a| a == "--weight").map(|at| at + 1);
    let fonts: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(index, arg)| !arg.starts_with("--") && Some(*index) != weight_value)
        .map(|(_, arg)| arg)
        .collect();

    let font_path = crate::accuracy::find_font(fonts.first().copied()).context(
        "no font found; pass one explicitly, e.g. xtask separability C:/Windows/Fonts/arial.ttf",
    )?;
    println!("font: {}", font_path.display());

    let bytes = std::fs::read(Path::new(&font_path))
        .with_context(|| format!("reading {}", font_path.display()))?;
    let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
        .map_err(|e| anyhow::anyhow!("{}: {e}", font_path.display()))?;

    let mut others: Vec<(String, Font)> = Vec::new();
    for path in fonts.iter().skip(1) {
        let bytes = std::fs::read(Path::new(path)).with_context(|| format!("reading {path}"))?;
        let other = Font::from_bytes(bytes.as_slice(), FontSettings::default())
            .map_err(|e| anyhow::anyhow!("{path}: {e}"))?;
        let name = Path::new(path)
            .file_stem()
            .map_or_else(|| (*path).clone(), |s| s.to_string_lossy().into_owned());
        others.push((name, other));
    }

    let reference_name = font_path
        .file_stem()
        .map_or_else(|| "reference".to_owned(), |s| s.to_string_lossy().into_owned());

    let measured = measure(&font)?;
    println!(
        "{} characters measured, metric weight {weight_percent}%",
        measured.len()
    );
    report(&measured, weight_percent);
    report_hole_separation(&measured, weight_percent);
    report_hole_stability(&font, &measured);
    report_hole_portability(&font, &reference_name, &others);
    Ok(())
}
