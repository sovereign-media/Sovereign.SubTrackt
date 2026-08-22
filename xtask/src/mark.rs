//! The falsification bench for [#48](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/48):
//! does carrying a diacritic separately separate the accent-direction pairs?
//!
//! #46 went looking for a hole count and turned up something else on the way past. Of the pairs the
//! shipped matcher calls ambiguous, sixteen are one base letter differing only in which way its
//! accent leans — `À`/`Á`, `è`/`é`, `ò`/`ó`. That is ordinary Spanish, French and Italian text, and
//! it is a larger residual than `l`/`I`, which is one pair.
//!
//! The vector cannot see it for a structural reason. Letterboxing scales the merged bounding box —
//! base plus mark — to fill the grid, so a mark occupying the top sixth of the glyph lands in one
//! or two rows of cells and its direction is largely sub-cell. Everything below it is identical
//! between the pair, so the distance is dominated by the part carrying no information.
//!
//! But the pipeline *knows* which pixels are the accent: [`subtrackt_glyph::group`] identifies the
//! component as a mark and attaches it to a body before [`subtrackt_glyph::feature`] merges the
//! boxes and throws that knowledge away. So three candidates, cheapest first, exactly as #48 lists
//! them:
//!
//! - **A, placement.** Where the mark's box sits relative to the body's, as four fractions.
//! - **B, shape.** The mark's own ink, letterboxed onto the same grid by the same `vectorize`.
//! - **C, slope.** The normalised second-moment cross term of the mark's ink — one signed number,
//!   positive for a stroke falling left to right and negative for one rising.
//!
//! Two things have to hold, and the second is the one that kills features. **Separation:** the two
//! members of a pair must land further apart than the margin. **Stability:** one character must
//! produce the same value across the sizes and ink thresholds real material varies over.
//! `docs/glyph-stability.md` records what happens when only the first is checked — two renderings
//! of one character sit a median 46 cells apart against 31 for two *different* characters, which is
//! why the matcher clusters before it matches.
//!
//! So separation is not measured against a fixed threshold here. It is measured against each
//! character's own spread across [`SURVEY_SIZES`] x [`INK_LEVELS`]: a candidate separates a pair
//! when the gap between the two characters is wider than the wobble either one shows on its own.
//! That is #14's comparison applied one level down, to the mark instead of the glyph.

use std::collections::BTreeMap;

use fontdue::Font;
use subtrackt_core::{FEATURE_BITS, FeatureVector, Rect};
use subtrackt_glyph::binarize::BinaryMask;
use subtrackt_glyph::ccl::{self, ComponentFilter};
use subtrackt_glyph::feature::{AspectPolicy, vectorize};
use subtrackt_glyph::group::{self, GroupingRules};

use crate::separability::{INK_LEVELS, SURVEY_SIZES};

/// Ink threshold the reference rendering is taken at, matching the binarizer's default of half.
const REFERENCE_INK: u8 = 128;

/// Sign agreement a candidate has to reach across the survey range to count as holding still.
///
/// A fraction rather than "always", because the smallest survey size renders some marks at three
/// pixels and one of those can round either way. What the number rules out is a feature that flips
/// often enough to assert a difference between two renderings of one letter.
const SIGN_AGREEMENT_PERCENT: u32 = 95;

/// Slope magnitude below which a mark is treated as having no direction at all.
///
/// A circumflex and a diaeresis are symmetric about their vertical axis, so their cross term is
/// zero and its sign is whichever way the rasterisation happened to round. Asking such a mark to
/// hold a sign is asking the wrong question; what it has to hold is a value near zero, which is
/// what the spread column reports. Set well below the 60-odd an acute or a grave measures and well
/// above the single digits a symmetric mark wanders through.
const LEANING_SLOPE: u32 = 20;

/// The three candidates, named so a table column and a verdict cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Candidate {
    Placement,
    Shape,
    Slope,
}

impl Candidate {
    const ALL: [Self; 3] = [Self::Placement, Self::Shape, Self::Slope];

    const fn label(self) -> &'static str {
        match self {
            Self::Placement => "A placement",
            Self::Shape => "B mark shape",
            Self::Slope => "C mark slope",
        }
    }

    const fn tag(self) -> &'static str {
        match self {
            Self::Placement => "A",
            Self::Shape => "B",
            Self::Slope => "C",
        }
    }
}

/// One rendering's mark, under all three candidates at once.
///
/// Measured together because they come from the same decomposition, and separating them would mean
/// rasterising every character three times to answer one question.
#[derive(Debug, Clone)]
struct MarkFeatures {
    /// Candidate A: mark width, mark height, centre-to-centre horizontal offset and the vertical
    /// gap, each as a percentage of the corresponding body dimension.
    ///
    /// Percentages of the body rather than of the line, because a mark is placed by the typeface
    /// relative to the letter it sits on, and the body is the part of that relationship both
    /// members of a pair share.
    placement: [i32; 4],
    /// Candidate B: the mark's own ink through the runtime's normalisation.
    shape: FeatureVector,
    /// Candidate C: `Cxy / sqrt(Cxx * Cyy)` over the mark's ink, in percent.
    ///
    /// Normalised by the two variances so it reports the *direction* of the ink and not its extent
    /// — a long accent and a short one that lean the same way have to agree, or the feature is
    /// reporting rendering size again.
    slope: i32,
}

impl MarkFeatures {
    /// Distance between two renderings under one candidate, in that candidate's own units.
    ///
    /// The units differ on purpose and are never mixed: every candidate is compared against its own
    /// spread, and only B is in cells of the feature vector.
    fn distance(&self, other: &Self, candidate: Candidate) -> u32 {
        match candidate {
            Candidate::Placement => self
                .placement
                .iter()
                .zip(other.placement)
                .map(|(a, b)| a.abs_diff(b))
                .sum(),
            Candidate::Shape => self.shape.distance(&other.shape),
            Candidate::Slope => self.slope.abs_diff(other.slope),
        }
    }
}

/// How high a character's ink reaches above the baseline, in pixels at `size`.
///
/// fontdue reports `ymin` as the offset of the bitmap's bottom from the baseline, so the top edge
/// is that plus the bitmap's height.
fn reach(font: &Font, character: char, size: f32) -> i32 {
    let metrics = font.metrics(character, size);
    if metrics.height == 0 {
        return i32::MIN;
    }
    metrics.ymin + i32::try_from(metrics.height).unwrap_or(0)
}

/// The character among `candidates` whose ink reaches highest above the baseline.
fn tallest(font: &Font, candidates: impl Iterator<Item = char>) -> char {
    candidates
        .max_by_key(|character| reach(font, *character, crate::RENDER_PX))
        .unwrap_or('l')
}

/// The two neighbours a subject gets rendered between, and what each stands for.
///
/// A mark attaches to a body only inside one text line, and `line_bands` cuts a line at any row
/// carrying no ink across its whole width. The accent on a capital sits *above* cap height, so
/// whether it reaches the letter under it depends on whether anything else on the line reaches that
/// high. That makes the neighbour a variable of the measurement rather than a detail of it:
///
/// - **any ASCII** is the best case. Whatever in this font overshoots furthest — typically `$` or a
///   bracket, which are drawn past cap height on purpose — so a failure here is conclusive.
/// - **letters only** is the ordinary case. Subtitle lines are mostly letters and spaces, and in a
///   typeface whose ascenders stop at cap height there is nothing above the caps at all.
///
/// Accented characters are excluded from both. An accented neighbour would put a mark of its own in
/// the row under scrutiny, which fills the gap for the projection while doing nothing for the
/// subject's mark — the measurement would report a line that groups when no letter grouped.
fn neighbours(font: &Font) -> [(&'static str, char); 2] {
    let ascii = tallest(font, (0x21u8..0x7f).map(char::from));
    let letter = tallest(
        font,
        (0x21u8..0x7f)
            .map(char::from)
            .filter(char::is_ascii_alphabetic),
    );
    [("any ASCII", ascii), ("letters only", letter)]
}

/// Blank border left around a rendered line, so no glyph touches the plane edge.
const MARGIN: u32 = 4;

/// The body and the mark of one rendering, as boxes into the mask they were labelled in.
struct Decomposed {
    mask: BinaryMask,
    body: Rect,
    mark: Rect,
}

/// Lay out a line of text as a coverage plane, the way `xtask fixture` does minus the outline.
///
/// The outline is left off on purpose: the rest of this bench thresholds bare fontdue coverage, and
/// a measurement that rendered its marks differently from its glyphs would be answering a different
/// question with each half.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn render_line(font: &Font, text: &str, size: f32) -> Option<(u32, u32, Vec<u8>)> {
    let mut placed = Vec::new();
    let mut pen = 0i32;
    let (mut top, mut bottom) = (i32::MAX, i32::MIN);

    for character in text.chars() {
        let (metrics, coverage) = font.rasterize(character, size);
        if metrics.width > 0 && metrics.height > 0 {
            let glyph_height = i32::try_from(metrics.height).ok()?;
            let x = pen + metrics.xmin;
            let y = -metrics.ymin - glyph_height;
            top = top.min(y);
            bottom = bottom.max(y + glyph_height);
            placed.push((x, y, metrics.width, metrics.height, coverage));
        }
        pen += metrics
            .advance_width
            .round()
            .clamp(0.0, f32::from(i16::MAX)) as i32;
    }
    if placed.is_empty() {
        return None;
    }

    let width = u32::try_from(pen).ok()? + MARGIN * 2;
    let height = u32::try_from(bottom - top).ok()? + MARGIN * 2;
    let mut plane = vec![0u8; (width as usize) * (height as usize)];
    for (x, y, glyph_width, glyph_height, coverage) in placed {
        for row in 0..glyph_height {
            for column in 0..glyph_width {
                let at_x = x + i32::try_from(column).ok()? + i32::try_from(MARGIN).ok()?;
                let at_y = y - top + i32::try_from(row).ok()? + i32::try_from(MARGIN).ok()?;
                if at_x < 0 || at_y < 0 {
                    continue;
                }
                let (at_x, at_y) = (at_x as u32, at_y as u32);
                if at_x >= width || at_y >= height {
                    continue;
                }
                let at = (at_y * width + at_x) as usize;
                plane[at] = plane[at].max(coverage[row * glyph_width + column]);
            }
        }
    }
    Some((width, height, plane))
}

/// Split one rendered character into a body and a mark, through the runtime's own grouping.
///
/// Deliberately not a reimplementation. `ccl` labels, `assign_lines` bands and `group` decides what
/// is a mark and what is a body — the same three calls the pipeline makes — so a character this
/// bench cannot decompose is one the pipeline could not have carried a mark for either.
///
/// The character is rendered **between two lowercase `l`s** rather than on its own, and that is not
/// cosmetic. `group` attaches a mark to a body only within a text line, and `line_bands` finds a
/// line by looking for rows that carry ink. A lone `é` has a blank row between its accent and its
/// body, so it bands as *two* lines and the accent never reaches the letter it belongs to — which
/// is a fact about rendering one character in isolation, not about the character. Neighbours fill
/// those rows the way the rest of a subtitle line would.
///
/// The neighbour is not chosen by hand. It is whichever unaccented ASCII character reaches highest
/// above the baseline in this font, because the gap that has to be covered sits *above* cap height
/// — the accent on a capital clears the letter it belongs to. Taking the tallest available
/// neighbour makes a failure here conclusive: no line of text this charset can spell would have
/// filled the row.
///
/// Returns `None` unless the middle glyph comes back with at least two parts. A double quote comes
/// back as two glyphs and is skipped, which is the known limitation `group` documents rather than
/// something this measurement should paper over.
fn decompose(
    font: &Font,
    character: char,
    neighbour: char,
    size: f32,
    ink: u8,
) -> Option<Decomposed> {
    let (width, height, coverage) =
        render_line(font, &format!("{neighbour} {character} {neighbour}"), size)?;
    let bits: Vec<bool> = coverage.iter().map(|c| *c >= ink).collect();
    let plane = BinaryMask::from_bits(width, height, bits).ok()?;
    let components = ccl::label(&plane, ComponentFilter::permissive()).ok()?;
    let lines = group::assign_lines(&plane, &components).ok()?;
    let glyphs = group::group(&components, &lines, GroupingRules::default()).ok()?;

    // Three glyphs, and the middle one is the subject. Anything else means the two neighbours did
    // not survive as their own glyphs, so the grouping this measurement depends on did not happen.
    let [_, glyph, _] = glyphs.as_slice() else {
        return None;
    };
    if glyph.parts.len() < 2 {
        return None;
    }

    // The body is the tallest part; everything else is the mark. Held as an index rather than a
    // box, because a diaeresis is two parts of identical bounds and comparing by value would drop
    // the wrong one.
    let body_at = glyph
        .parts
        .iter()
        .enumerate()
        .max_by_key(|(_, part)| part.bounds.height)
        .map(|(index, _)| index)?;
    let body = glyph.parts[body_at].bounds;
    let mark = glyph
        .parts
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != body_at)
        .map(|(_, part)| part.bounds)
        .reduce(Rect::union)?;

    Some(Decomposed { mask: plane, body, mark })
}

/// The normalised second-moment cross term of the ink inside `area`, in percent.
///
/// Positive where the ink falls left to right in image coordinates — a grave — and negative where
/// it rises — an acute. A mark with a vertical axis of symmetry, a circumflex or a diaeresis, sits
/// near zero and therefore *between* the two, which is what makes one number separate three marks.
///
/// Fewer than three ink pixels is reported as zero rather than as a slope: two pixels always lie on
/// a line, so the number would be a fact about the count and not about the letterform.
#[allow(clippy::cast_possible_truncation)]
fn slope(mask: &BinaryMask, area: Rect) -> i32 {
    let mut count = 0f64;
    let (mut sum_x, mut sum_y) = (0f64, 0f64);
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if mask.get(x, y) {
                count += 1.0;
                sum_x += f64::from(x);
                sum_y += f64::from(y);
            }
        }
    }
    if count < 3.0 {
        return 0;
    }

    let (mean_x, mean_y) = (sum_x / count, sum_y / count);
    let (mut cxx, mut cyy, mut cxy) = (0f64, 0f64, 0f64);
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if mask.get(x, y) {
                let (dx, dy) = (f64::from(x) - mean_x, f64::from(y) - mean_y);
                cxx += dx * dx;
                cyy += dy * dy;
                cxy += dx * dy;
            }
        }
    }

    // A mark one pixel tall has no vertical extent to lean within, so the ratio is undefined rather
    // than zero. Reporting zero is the honest answer: it is what a symmetric mark reports, and an
    // undecidable case should not be handed a direction.
    let spread = (cxx * cyy).sqrt();
    if spread < 1.0 {
        return 0;
    }
    (cxy / spread * 100.0).round() as i32
}

/// All three candidates for one rendering of one character.
#[allow(clippy::cast_possible_truncation)]
fn features(
    font: &Font,
    character: char,
    neighbour: char,
    size: f32,
    ink: u8,
) -> Option<MarkFeatures> {
    let split = decompose(font, character, neighbour, size, ink)?;
    let (body, mark) = (split.body, split.mark);
    if body.width == 0 || body.height == 0 {
        return None;
    }

    let (body_w, body_h) = (i64::from(body.width), i64::from(body.height));
    let centre_offset = (2 * i64::from(mark.x) + i64::from(mark.width))
        - (2 * i64::from(body.x) + i64::from(body.width));
    let gap = i64::from(body.y) - (i64::from(mark.y) + i64::from(mark.height));
    let placement = [
        (i64::from(mark.width) * 100 / body_w) as i32,
        (i64::from(mark.height) * 100 / body_h) as i32,
        (centre_offset * 100 / (2 * body_w)) as i32,
        (gap * 100 / body_h) as i32,
    ];

    // The mark's box is vectorised against the whole glyph's mask, so any body ink falling inside
    // that box would be counted. For a mark sitting clear above its body none does, and a mark that
    // overlapped its body would not have been labelled a separate component in the first place.
    let shape = vectorize(&split.mask, mark, AspectPolicy::Letterbox).ok()?;

    Some(MarkFeatures { placement, shape, slope: slope(&split.mask, mark) })
}

/// What one character does across the survey range, under all three candidates.
struct Spread {
    /// Median pairwise distance between renderings, per candidate. The noise floor.
    noise: [u32; 3],
    /// How often the sign of the slope agrees with its modal sign, in percent.
    sign_agreement: u32,
    /// Renderings that decomposed at all, of [`SURVEY_SIZES`] x [`INK_LEVELS`].
    renderings: usize,
}

/// The median of a list, taking the upper of the two middles on an even count.
fn median(values: &mut [u32]) -> u32 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

/// Measure one character's spread across the sizes and ink thresholds real material varies over.
///
/// This is the noise floor every separation claim below is checked against. Measured pairwise
/// rather than against the 96px reference, because the question is whether two *arrivals* of one
/// character agree, and neither of them is privileged at runtime.
#[allow(clippy::cast_possible_truncation)]
fn spread(font: &Font, character: char, neighbour: char) -> Option<Spread> {
    let mut renderings = Vec::new();
    for size in SURVEY_SIZES {
        for ink in INK_LEVELS {
            if let Some(measured) = features(font, character, neighbour, size, ink) {
                renderings.push(measured);
            }
        }
    }
    if renderings.len() < 2 {
        return None;
    }

    let mut noise = [0u32; 3];
    for (slot, candidate) in Candidate::ALL.iter().enumerate() {
        let mut distances = Vec::new();
        for (index, a) in renderings.iter().enumerate() {
            for b in &renderings[index + 1..] {
                distances.push(a.distance(b, *candidate));
            }
        }
        noise[slot] = median(&mut distances);
    }

    let mut signs: BTreeMap<i32, usize> = BTreeMap::new();
    for measured in &renderings {
        *signs.entry(measured.slope.signum()).or_default() += 1;
    }
    let modal = signs.values().copied().max().unwrap_or(0);
    let sign_agreement = (modal * 100 / renderings.len()) as u32;

    Some(Spread { noise, sign_agreement, renderings: renderings.len() })
}

/// Everything known about one character's mark.
struct Marked {
    character: char,
    reference: MarkFeatures,
    spread: Spread,
}

impl Marked {
    /// Does a candidate tell this character from `other` by more than either one's own wobble?
    ///
    /// The comparison that matters. A gap of 40 cells means nothing if one of the two characters
    /// moves 60 cells between one rendering and the next: the matcher would be reading
    /// rasterisation noise and calling it a letter.
    fn separated_from(&self, other: &Self, candidate: Candidate, slot: usize) -> (u32, u32, bool) {
        let gap = self.reference.distance(&other.reference, candidate);
        let noise = self.spread.noise[slot].max(other.spread.noise[slot]);
        (gap, noise, gap > noise)
    }
}

/// The sixteen pairs #48 names, so the answer is reported for them by name whether or not they
/// happen to fall inside the shipped margin at the shipped grid size.
///
/// #48's table was produced on the 32x32 grid #46 measured and did not ship — `Ò`/`Ó` is quoted at
/// 22 cells, which is the 2.1% of a 1024-bit vector that issue reports. At the 16x16 that ships,
/// the same 2.1% is 5 cells. Naming the pairs pins the question to the letters rather than to a
/// cell count that halves when the grid does.
const ACCENT_PAIRS: [(char, char); 16] = [
    ('\u{c0}', '\u{c1}'),
    ('\u{c8}', '\u{c9}'),
    ('\u{d9}', '\u{da}'),
    ('\u{d2}', '\u{d3}'),
    ('\u{c0}', '\u{c2}'),
    ('\u{c8}', '\u{ca}'),
    ('\u{c9}', '\u{ca}'),
    ('\u{d2}', '\u{d4}'),
    ('\u{d9}', '\u{db}'),
    ('\u{f9}', '\u{fa}'),
    ('\u{e0}', '\u{e1}'),
    ('\u{da}', '\u{db}'),
    ('\u{c1}', '\u{c2}'),
    ('\u{d3}', '\u{d4}'),
    ('\u{e8}', '\u{e9}'),
    ('\u{f2}', '\u{f3}'),
];

/// Which candidates separated a pair, as a printable list.
fn verdict(tags: &[&str]) -> String {
    if tags.is_empty() {
        "none".to_owned()
    } else {
        tags.join(" ")
    }
}

/// Measure every character in the reference charset that carries a mark.
fn measure(font: &Font, neighbour: char) -> Vec<Marked> {
    let mut marked = Vec::new();
    for character in crate::charset() {
        let (Some(reference), Some(spread)) = (
            features(font, character, neighbour, crate::RENDER_PX, REFERENCE_INK),
            spread(font, character, neighbour),
        ) else {
            continue;
        };
        marked.push(Marked { character, reference, spread });
    }
    marked
}

/// Does this character arrive as more than one connected component when rendered on its own?
///
/// The test for "has a mark" that does not depend on grouping succeeding, so a character that fails
/// to group is distinguishable from one that never had a mark to group.
fn has_mark(font: &Font, character: char, ink: u8) -> bool {
    let (metrics, coverage) = font.rasterize(character, crate::RENDER_PX);
    let (Ok(width), Ok(height)) = (u32::try_from(metrics.width), u32::try_from(metrics.height))
    else {
        return false;
    };
    if width == 0 || height == 0 {
        return false;
    }
    let bits: Vec<bool> = coverage.iter().map(|c| *c >= ink).collect();
    let Ok(mask) = BinaryMask::from_bits(width, height, bits) else {
        return false;
    };
    ccl::label(&mask, ComponentFilter::permissive()).is_ok_and(|parts| parts.len() > 1)
}

/// Can a mark reach its body at all, for each character that has one?
///
/// The question that has to be answered before any of the three candidates means anything, and it
/// turned out to have an answer nobody had measured. `group` attaches a mark to a body only within
/// one text line, and `line_bands` cuts a line at any row carrying no ink anywhere across its
/// width. An accent over a *capital* sits above the tallest letterform the charset can spell, so
/// the row between the two is blank for the whole line and the accent bands as a line of its own.
/// It is then not a mark attached to a body; it is a glyph, and the capital under it is a bare `A`.
///
/// Measured at the most generous threshold in [`INK_LEVELS`], which is more generous than the
/// runtime's: the shipped `Threshold` is fill-only at half luma, so it excludes the outline a real
/// subtitle draws around its glyphs. A gap this does not bridge is not bridged at runtime either.
fn report_reach(font: &Font) -> usize {
    println!("\n--- can a mark reach its body at all? ---");
    println!(
        "  a capital H reaches {} px above the baseline at {}px in this font",
        reach(font, 'H', crate::RENDER_PX),
        crate::RENDER_PX
    );
    println!();
    println!("  context        neighbour   reaches   marks that reach their body");

    let generous = INK_LEVELS.iter().copied().min().unwrap_or(REFERENCE_INK);
    let marked: Vec<char> = crate::charset()
        .into_iter()
        .filter(|character| has_mark(font, *character, generous))
        .collect();

    let mut best = 0usize;
    for (context, neighbour) in neighbours(font) {
        let grouped: Vec<char> = marked
            .iter()
            .copied()
            .filter(|character| {
                decompose(font, *character, neighbour, crate::RENDER_PX, generous).is_some()
            })
            .collect();
        best = best.max(grouped.len());

        let listed: String = grouped.iter().collect();
        // `Debug for char` ignores a width specifier, so the column is padded as a string.
        let shown = format!("{neighbour:?}");
        println!(
            "  {context:<13}  {shown:<9}  {:>4} px   {:>2} of {}   {listed}",
            reach(font, neighbour, crate::RENDER_PX),
            grouped.len(),
            marked.len()
        );
    }

    println!(
        "\n  a mark that does not reach its body is not a mark. It bands as a line of its own,"
    );
    println!(
        "  so the letter under it is matched bare and the accent is matched as a glyph of its"
    );
    println!("  own — a different failure from the one #48 set out to measure, and one that lands");
    println!("  wherever a line of text carries nothing that overshoots cap height.");
    println!("  Everything below is measured in the best case, the wider of the two contexts.");
    best
}

/// Report the three candidates against the pairs #48 names.
#[allow(clippy::cast_possible_truncation)]
fn report_accent_pairs(marked: &[Marked]) {
    let find = |c: char| marked.iter().find(|m| m.character == c);

    println!("\n--- the sixteen accent-direction pairs #48 names ---");
    println!("  gap is the distance between the pair at 96px; noise is the larger of the two");
    println!(
        "  characters' own median spread across {} sizes x {} ink thresholds, and a candidate",
        SURVEY_SIZES.len(),
        INK_LEVELS.len()
    );
    println!("  separates a pair only when the gap is wider than that");
    println!();
    println!("  pair    A gap/noise    B gap/noise    C slopes   gap/noise    separated by");

    let mut totals = [0usize; 3];
    let mut shape_gaps: Vec<u32> = Vec::new();
    let mut shape_noise: Vec<u32> = Vec::new();
    let mut pairs = 0usize;

    for (left, right) in ACCENT_PAIRS {
        let (Some(a), Some(b)) = (find(left), find(right)) else {
            println!("  {left} / {right}  not decomposable in this font");
            continue;
        };
        pairs += 1;

        let mut tags: Vec<&str> = Vec::new();
        let mut cells = [(0u32, 0u32); 3];
        for (slot, candidate) in Candidate::ALL.iter().enumerate() {
            let (gap, noise, separated) = a.separated_from(b, *candidate, slot);
            cells[slot] = (gap, noise);
            if separated {
                totals[slot] += 1;
                tags.push(candidate.tag());
            }
        }
        shape_gaps.push(cells[1].0);
        shape_noise.push(cells[1].1);

        println!(
            "  {left} / {right}  {:>5}/{:<6}  {:>5}/{:<6}  {:>4}/{:<4}  {:>4}/{:<6}   {}",
            cells[0].0,
            cells[0].1,
            cells[1].0,
            cells[1].1,
            a.reference.slope,
            b.reference.slope,
            cells[2].0,
            cells[2].1,
            verdict(&tags),
        );
    }

    println!();
    for (slot, candidate) in Candidate::ALL.iter().enumerate() {
        println!(
            "  {:<12} separates {} of {pairs} accent pairs",
            candidate.label(),
            totals[slot]
        );
    }

    if shape_gaps.is_empty() {
        return;
    }
    let mean_gap = shape_gaps.iter().sum::<u32>() / (shape_gaps.len() as u32);
    let median_noise = median(&mut shape_noise);
    let bits = FEATURE_BITS as u64;
    println!("\n  candidate B, in cells of a {FEATURE_BITS}-bit vector:");
    println!(
        "    mean gap between the members of a pair: {mean_gap} ({}% of the vector)",
        u64::from(mean_gap) * 100 / bits
    );
    println!(
        "    median spread of one mark across renderings: {median_noise} ({}% of the vector)",
        u64::from(median_noise) * 100 / bits
    );
}

/// Report whether candidate C holds its sign across the survey range.
fn report_slope_stability(marked: &[Marked]) {
    println!("\n--- candidate C: does the slope hold still across renderings? ---");
    println!(
        "  sign agreement is only meaningful for a mark that leans. A circumflex is symmetric,"
    );
    println!("  so its slope is zero and which side of zero it lands on is coin-flip noise — that");
    println!(
        "  is the spread column reading near zero, not a feature that cannot make its mind up."
    );
    println!();
    println!("  character   slope at 96px   spread   sign agreement   renderings");

    let find = |c: char| marked.iter().find(|m| m.character == c);
    let mut listed: Vec<char> = Vec::new();
    for (left, right) in ACCENT_PAIRS {
        for character in [left, right] {
            if listed.contains(&character) {
                continue;
            }
            listed.push(character);
            let Some(m) = find(character) else { continue };
            println!(
                "  {character:>9}   {:>13}   {:>6}   {:>14}%   {}",
                m.reference.slope, m.spread.noise[2], m.spread.sign_agreement, m.spread.renderings
            );
        }
    }

    let leaning: Vec<&Marked> = marked
        .iter()
        .filter(|m| m.reference.slope.unsigned_abs() >= LEANING_SLOPE)
        .collect();
    let agreeing = leaning
        .iter()
        .filter(|m| m.spread.sign_agreement >= SIGN_AGREEMENT_PERCENT)
        .count();
    println!(
        "\n  marks that lean at all (|slope| >= {LEANING_SLOPE}): {} of {}",
        leaning.len(),
        marked.len()
    );
    println!(
        "  of those, holding one sign in at least {SIGN_AGREEMENT_PERCENT}% of renderings: {agreeing}"
    );

    let mut spreads: Vec<u32> = marked.iter().map(|m| m.spread.noise[2]).collect();
    println!(
        "  median spread of a mark's slope across renderings, all marks: {}",
        median(&mut spreads)
    );
}

/// Report the three candidates against the pairs the shipped matcher actually calls ambiguous.
///
/// The accent pairs above are the ones #48 names; these are the ones the matcher confuses today at
/// the shipped grid size. They should mostly be the same set, and where they are not, that is worth
/// seeing.
fn report_ambiguous(marked: &[Marked], ambiguous: &[(u32, char, char)]) {
    let find = |c: char| marked.iter().find(|m| m.character == c);

    println!("\n--- the pairs the shipped matcher calls ambiguous ---");
    println!("  pair    combined   separated by");

    let mut with_marks = 0usize;
    let mut totals = [0usize; 3];
    for (combined, left, right) in ambiguous {
        let (Some(a), Some(b)) = (find(*left), find(*right)) else {
            println!("  {left} / {right}  {combined:>8}   no mark on one side");
            continue;
        };
        with_marks += 1;

        let mut tags: Vec<&str> = Vec::new();
        for (slot, candidate) in Candidate::ALL.iter().enumerate() {
            if a.separated_from(b, *candidate, slot).2 {
                totals[slot] += 1;
                tags.push(candidate.tag());
            }
        }
        println!("  {left} / {right}  {combined:>8}   {}", verdict(&tags));
    }

    println!(
        "\n  ambiguous pairs with a mark on both sides: {with_marks} of {}",
        ambiguous.len()
    );
    for (slot, candidate) in Candidate::ALL.iter().enumerate() {
        println!("  {:<12} separates {} of those", candidate.label(), totals[slot]);
    }
}

/// Does a mark's slope survive a change of typeface?
///
/// The question #9 forces on any feature that gets stored in a reference set: an embedded set is by
/// definition built from a typeface the disc was not authored in. #43 would change that by fitting
/// the set to the title, but until it lands the shipped answer is a rendered font, and a feature
/// that flips sign between two typefaces would be worse than none.
fn report_portability(reference: &Font, reference_name: &str, others: &[(String, Font)]) {
    println!("\n--- does a mark's slope survive a change of typeface? ---");
    if others.is_empty() {
        println!("  no comparison typefaces given; pass more fonts to check this");
        return;
    }

    for (name, font) in others {
        let mut flipped: Vec<String> = Vec::new();
        let mut compared = 0u32;
        let mut worst = 0u32;
        let mut listed: Vec<char> = Vec::new();
        for (left, right) in ACCENT_PAIRS {
            for character in [left, right] {
                if listed.contains(&character) {
                    continue;
                }
                listed.push(character);
                let (Some(here), Some(there)) = (
                    features(
                        reference,
                        character,
                        neighbours(reference)[0].1,
                        crate::RENDER_PX,
                        REFERENCE_INK,
                    ),
                    features(
                        font,
                        character,
                        neighbours(font)[0].1,
                        crate::RENDER_PX,
                        REFERENCE_INK,
                    ),
                ) else {
                    continue;
                };
                compared += 1;
                worst = worst.max(here.slope.abs_diff(there.slope));
                // A sign change only means something for a mark that leans in the first place. A
                // circumflex reading 0 in one typeface and 1 in another has not reversed direction;
                // it has stayed symmetric, and counting that as a flip would condemn the feature
                // for behaving exactly as designed.
                if here.slope.unsigned_abs() >= LEANING_SLOPE
                    && there.slope.unsigned_abs() >= LEANING_SLOPE
                    && here.slope.signum() != there.slope.signum()
                {
                    flipped.push(format!("{character} {}->{}", here.slope, there.slope));
                }
            }
        }
        println!(
            "  {reference_name} vs {name}: of {compared} marks, {} reverse direction; largest slope move {worst}{}{}",
            flipped.len(),
            if flipped.is_empty() { "" } else { "   " },
            flipped.join("  ")
        );
    }
}

/// Run the mark half of the separability bench.
pub fn report(
    font: &Font,
    reference_name: &str,
    others: &[(String, Font)],
    ambiguous: &[(u32, char, char)],
) {
    let neighbour = neighbours(font)[0].1;

    println!("\n--- the mark, carried separately ---");
    if report_reach(font) == 0 {
        println!("\n  no mark in this font reaches its body, so there is nothing to measure");
        return;
    }

    let marked = measure(font, neighbour);
    println!(
        "\n  {} of {} characters decompose into a body and a mark",
        marked.len(),
        crate::charset().len()
    );
    if marked.is_empty() {
        println!("  nothing to measure; the grouping rules found no marks in this font");
        return;
    }

    report_accent_pairs(&marked);
    report_slope_stability(&marked);
    report_ambiguous(&marked, ambiguous);
    report_portability(font, reference_name, others);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a mask from an ASCII picture, `#` for ink.
    fn mask_of(rows: &[&str]) -> BinaryMask {
        let height = u32::try_from(rows.len()).unwrap();
        let width = u32::try_from(rows[0].len()).unwrap();
        let bits: Vec<bool> = rows
            .iter()
            .flat_map(|row| row.chars().map(|c| c == '#'))
            .collect();
        BinaryMask::from_bits(width, height, bits).unwrap()
    }

    fn whole(mask: &BinaryMask) -> Rect {
        Rect::new(0, 0, mask.width(), mask.height())
    }

    #[test]
    fn a_grave_leans_the_opposite_way_to_an_acute() {
        // In image coordinates y grows downwards, so a grave — drawn from upper left to lower
        // right — has ink whose x and y rise together. Getting this backwards would not fail any
        // measurement in this bench; it would silently swap which letter is which.
        let grave = mask_of(&["##..", ".##.", "..##"]);
        let acute = mask_of(&["..##", ".##.", "##.."]);
        assert!(slope(&grave, whole(&grave)) > 0, "a grave falls left to right");
        assert!(slope(&acute, whole(&acute)) < 0, "an acute rises left to right");
    }

    #[test]
    fn a_symmetric_mark_lands_between_the_two_it_has_to_separate() {
        // The whole reason one signed number separates three marks: a circumflex is symmetric, so
        // its cross term cancels and it sits between an acute and a grave rather than beside one.
        let circumflex = mask_of(&["..##..", ".####.", "##..##"]);
        let leaning = slope(&mask_of(&["##..", ".##.", "..##"]), Rect::new(0, 0, 4, 3));
        let symmetric = slope(&circumflex, whole(&circumflex));
        assert!(
            symmetric.unsigned_abs() < LEANING_SLOPE,
            "a circumflex measured {symmetric}, which is a direction rather than the absence of one"
        );
        assert!(symmetric.unsigned_abs() * 2 < leaning.unsigned_abs());
    }

    #[test]
    fn a_slope_is_the_same_whatever_the_mark_is_scaled_to() {
        // The feature has to report direction and not size, or it reports rendering resolution
        // again — the axis `docs/glyph-stability.md` measured as costing 11 cells on its own.
        let small = mask_of(&["##..", ".##.", "..##"]);
        let large = mask_of(&[
            "####....", "####....", "..####..", "..####..", "....####", "....####",
        ]);
        let (a, b) = (slope(&small, whole(&small)), slope(&large, whole(&large)));
        assert!(
            a.abs_diff(b) < LEANING_SLOPE,
            "the same stroke at two sizes measured {a} and {b}"
        );
    }

    #[test]
    fn too_few_pixels_reports_no_direction_rather_than_a_guess() {
        // Two pixels always lie on a line, so any slope read off them is a fact about the count.
        // Reporting zero is the honest answer; inventing a direction is the thing this project
        // exists not to do.
        let two = mask_of(&["#.", ".#"]);
        assert_eq!(slope(&two, whole(&two)), 0);
        let empty = mask_of(&["..", ".."]);
        assert_eq!(slope(&empty, whole(&empty)), 0);
    }

    #[test]
    fn a_one_row_mark_has_no_vertical_extent_to_lean_within() {
        // A macron rasterised to a single row has zero vertical variance, so the normalising
        // divisor vanishes. That is undefined, not vertical, and it must not divide by zero.
        let flat = mask_of(&["####"]);
        assert_eq!(slope(&flat, whole(&flat)), 0);
    }

    #[test]
    fn the_median_of_an_even_count_takes_the_upper_middle() {
        assert_eq!(median(&mut [4, 1, 3, 2]), 3);
        assert_eq!(median(&mut [5, 1, 3]), 3);
        assert_eq!(median(&mut []), 0);
    }
}
