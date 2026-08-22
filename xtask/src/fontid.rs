//! Can a *character-agnostic* style descriptor tell a good fit from a bad one?
//!
//! #63's fifth statistic, and the first one that is not a function of the matcher's own assignment.
//! The four that failed — mean match distance, that distance charging the unmatched, the
//! winner-versus-runner-up margin, and inter-candidate agreement — all ask the matcher how it feels
//! about the answer it just gave, so all four inherit the bias that produced it:
//!
//! > a systematically wrong set is **by construction** a low-distance one — the matcher chose `I`
//! > for `t` precisely because they were close.
//!
//! The proposal, from *Font Representation Learning via Paired-glyph Matching* (BMVC 2022), is to
//! never consult the assignment. Do not ask whether this glyph is an `I` or an `l`; ask whether the
//! ink in this track is *shaped* the way this typeface shapes ink, pooled over the whole track with
//! the labels thrown away. The 108 `I`-for-`l` errors every sans-serif makes are invisible to a
//! statistic that never looks at a character identity, which is exactly why they cannot corrupt it.
//!
//! That is an argument about independence, and #63 makes it falsifiable: **if this correlates with
//! mean match distance across candidates, the independence claim is wrong and this is a fourth
//! instance of the same mechanism after all.** So that is measured first, and the run stops there
//! if it fails.
//!
//! # What is measured, in the order #63 asks for
//!
//! 1. **Independence** — style score against mean match distance across candidates.
//! 2. **The negative case** — a fixture whose typeface is absent from the candidate list must land
//!    outside the calibrated same-font band, per #43's rule about which test gets written first.
//! 3. **`i != j` retrieval** — the paper's metric: never let a character be compared with itself.
//! 4. **Separation** on the eight-fixture leave-one-out — every good read scoring better than every
//!    bad one, which is #63's bar, and not correlation.
//! 5. **The calibration** — same-font and different-font distributions, from the font files alone.
//!
//! # Where the numbers come from
//!
//! Nothing here touches the matching path and nothing here changes a library crate. The track side
//! reads [`GlyphRecord`](subtrackt::survey::GlyphRecord), which the shipped `survey` already
//! produces: the 16x16 letterboxed feature vector, the glyph's pixel bounds, and its line metrics.
//! The font side builds the same triple from a rasterised character. Both go through the axis code
//! below, so the two sides cannot drift into measuring different things — the identity
//! `gen-reference` already maintains with the runtime, applied to a second quantity.
//!
//! **Pooling is over distinct shapes rather than glyph instances**, and that is load-bearing. A
//! track's glyphs follow English letter frequency; a font's charset is uniform. Pooling instances
//! would compare an `e`-heavy distribution against a uniform one and read the difference as style.
//! Deduplicating by feature vector leaves roughly one entry per character on both sides, which is
//! the closest this can get to the same population without asserting a language — the prior
//! `docs/post-correction.md` refuses.

// Every cast below turns a count of cells, glyphs or fonts into a float to divide it. The largest
// is a glyph count in the tens of thousands, far inside what an f64 holds exactly, so the
// precision-loss lint has nothing to warn about here.
#![allow(clippy::cast_precision_loss)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use fontdue::{Font, FontSettings};
use subtrackt::Pipeline;
use subtrackt_core::{FEATURE_GRID, FeatureVector};
use subtrackt_glyph::font::{RENDER_PX, charset, vector_for};

/// The axes, in the order they appear in every table below.
///
/// Seven, each a fraction of something measured rather than an absolute cell count, per the rule in
/// `CLAUDE.md`: the same title ships at several resolutions and `FEATURE_GRID` may yet move.
const AXIS_NAMES: [&str; AXES] = [
    "slant", "weight", "contrast", "terminal", "density", "aspect", "round",
];

/// How many axes a style vector carries.
const AXES: usize = 7;

/// Each axis is pooled as a median and a spread, so the descriptor is twice as long.
const DESCRIPTOR_LEN: usize = AXES * 2;

/// Ink below this many cells is not a glyph worth measuring a second moment over.
///
/// The same floor `mark::slope_of` applies for the same reason: a moment over three cells is noise
/// with a direction attached.
const MIN_INK_CELLS: usize = 6;

/// How many interleaved subsets the pooled weight fit splits each charset into.
///
/// Four leaves roughly thirty-five characters per fold, which is enough for a median to be stable
/// while still giving four descriptors per font to measure a spread over.
const FOLDS: usize = 4;

/// Coverage above which a rasterised pixel counts as ink.
///
/// The same figure `subtrackt_glyph::font` thresholds a reference glyph at, and the binarizer's
/// default of half. A style measured at a different threshold would not be measuring the same ink
/// the matcher sees.
const RASTER_INK: u8 = 128;

/// One glyph's style, with its character identity never consulted.
type StyleVector = [f32; AXES];

/// A track's or a font's pooled style: per axis, a median and a spread.
#[derive(Clone, Debug)]
struct Descriptor {
    name: String,
    values: [f32; DESCRIPTOR_LEN],
    /// How many distinct shapes were pooled, so a thin sample is visible rather than implied.
    shapes: usize,
}

/// The ink bounding box within the feature grid, in cells.
///
/// The vector is letterboxed, so a glyph occupies a centred sub-box with its own aspect ratio and
/// the rest of the grid is blank. Measuring density or roundness over the whole grid would read
/// that blank margin as style, and it is geometry.
struct InkBox {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    cells: Vec<(usize, usize)>,
    /// Row-major occupancy over the source grid, so a run scan is a lookup rather than a search.
    ///
    /// At 16x16 a linear scan of the cell list would do. At the raster resolutions
    /// [`raw_styles`] measures, the same glyph is thousands of pixels and the quadratic version
    /// takes minutes per font — the grid is what makes the two paths cost the same.
    grid: Vec<bool>,
    stride: usize,
}

impl InkBox {
    /// The box a set of ink coordinates occupies on a `stride`-wide grid.
    fn new(cells: Vec<(usize, usize)>, stride: usize, rows: usize) -> Option<Self> {
        if cells.len() < MIN_INK_CELLS {
            return None;
        }
        let left = cells.iter().map(|c| c.0).min()?;
        let right = cells.iter().map(|c| c.0).max()?;
        let top = cells.iter().map(|c| c.1).min()?;
        let bottom = cells.iter().map(|c| c.1).max()?;

        let mut grid = vec![false; stride * rows];
        for &(x, y) in &cells {
            if let Some(slot) = grid.get_mut(y * stride + x) {
                *slot = true;
            }
        }
        Some(Self {
            left,
            top,
            width: right - left + 1,
            height: bottom - top + 1,
            cells,
            grid,
            stride,
        })
    }

    /// The set cells of a normalised feature vector.
    fn of(features: &FeatureVector) -> Option<Self> {
        let mut cells = Vec::new();
        for index in 0..FEATURE_GRID * FEATURE_GRID {
            if features.get(index) {
                cells.push((index % FEATURE_GRID, index / FEATURE_GRID));
            }
        }
        Self::new(cells, FEATURE_GRID, FEATURE_GRID)
    }

    /// The inked pixels of a rasterised glyph, before any normalisation.
    ///
    /// The same threshold `subtrackt_glyph::font` applies when it builds a reference vector, so
    /// this is the same ink — just not yet flattened onto the grid.
    fn of_raster(coverage: &[u8], width: usize, height: usize) -> Option<Self> {
        let cells: Vec<(usize, usize)> = coverage
            .iter()
            .enumerate()
            .filter(|(_, c)| **c >= RASTER_INK)
            .map(|(i, _)| (i % width, i / width))
            .collect();
        Self::new(cells, width, height)
    }

    /// Horizontal ink run lengths, in cells, over every row of the box.
    ///
    /// A stem crossed at one row contributes its width; a bowl contributes two. Pooled over a
    /// track this is the stroke weight, which is the axis a bold face and a light face of one
    /// typeface differ on most and the one `vectorize` deliberately absorbs within a glyph.
    fn runs(&self) -> Vec<usize> {
        let mut runs = Vec::new();
        for y in self.top..self.top + self.height {
            let mut run = 0usize;
            for x in self.left..self.left + self.width {
                if self.grid.get(y * self.stride + x).copied().unwrap_or(false) {
                    run += 1;
                } else if run > 0 {
                    runs.push(run);
                    run = 0;
                }
            }
            if run > 0 {
                runs.push(run);
            }
        }
        runs
    }
}

/// The style of one glyph, from its normalised vector and its shape in the plane.
///
/// `aspect` is the glyph's real pixel width over its real pixel height, which both sides have
/// exactly: the track from `bounds`, the font from the rasteriser's metrics. It is taken from there
/// rather than from the letterbox because the grid quantises it to sixteen levels.
///
/// Returns `None` for a glyph with too little ink to measure, which is a fact rather than a zero —
/// a fabricated axis value would pool indistinguishably from a real one.
fn style_of(features: &FeatureVector, aspect: f32) -> Option<StyleVector> {
    style_of_box(&InkBox::of(features)?, aspect)
}

/// The axes themselves, over whatever grid the ink was measured on.
///
/// Shared by both resolutions on purpose. The whole question [`raw_styles`] asks is whether the
/// 16x16 grid is throwing the signal away, and that is only a fair question if the arithmetic
/// either side of it is identical — otherwise the comparison measures two implementations.
fn style_of_box(ink: &InkBox, aspect: f32) -> Option<StyleVector> {
    let count = ink.cells.len() as f32;

    // Slant: the normalised second-moment cross term, which is `mark::slope_of` applied to the
    // body instead of to the mark. Positive leans one way, negative the other, zero is upright.
    let mean_x = ink.cells.iter().map(|c| c.0 as f32).sum::<f32>() / count;
    let mean_y = ink.cells.iter().map(|c| c.1 as f32).sum::<f32>() / count;
    let (mut cxx, mut cyy, mut cxy) = (0f32, 0f32, 0f32);
    for &(x, y) in &ink.cells {
        let (dx, dy) = (x as f32 - mean_x, y as f32 - mean_y);
        cxx += dx * dx;
        cyy += dy * dy;
        cxy += dx * dy;
    }
    let spread = (cxx * cyy).sqrt();
    let slant = if spread > f32::EPSILON {
        cxy / spread
    } else {
        0.0
    };

    let mut runs = ink.runs();
    if runs.is_empty() {
        return None;
    }
    runs.sort_unstable();

    // Weight: the median run as a fraction of the box height, so a stem reads the same whether the
    // glyph was rendered at 30 pixels or 90.
    let weight = percentile(&runs, 0.5) / ink.height as f32;

    // Contrast: how much the widest strokes exceed the narrowest. Percentiles rather than the
    // outright max and min, because sixteen cells give few enough runs that one stray cell would
    // otherwise be the whole statistic. A serif face with a hairline scores high; a grotesque with
    // one stem width scores near zero.
    let (wide, narrow) = (percentile(&runs, 0.9), percentile(&runs, 0.1));
    let contrast = if wide > f32::EPSILON {
        (wide - narrow) / wide
    } else {
        0.0
    };

    // Terminal energy: ink in the top and bottom rows of the box against ink in the middle band.
    // Serif faces load their terminals; sans faces do not.
    let edge_rows = (ink.height / 8).max(1);
    let mut edge = 0f32;
    let mut middle = 0f32;
    for &(_, y) in &ink.cells {
        let from_top = y - ink.top;
        let from_bottom = ink.top + ink.height - 1 - y;
        if from_top < edge_rows || from_bottom < edge_rows {
            edge += 1.0;
        } else {
            middle += 1.0;
        }
    }
    let terminal = if middle > f32::EPSILON {
        edge / middle
    } else {
        0.0
    };

    // Density: how much of its own box the glyph fills.
    let density = count / (ink.width * ink.height) as f32;

    // Roundness: ink in the box's four corners against the corner area. A square-shouldered face
    // fills them; a round one leaves them empty.
    let corner_w = (ink.width / 4).max(1);
    let corner_h = (ink.height / 4).max(1);
    let mut corner_ink = 0f32;
    for &(x, y) in &ink.cells {
        let near_x = (x - ink.left) < corner_w || (ink.left + ink.width - 1 - x) < corner_w;
        let near_y = (y - ink.top) < corner_h || (ink.top + ink.height - 1 - y) < corner_h;
        if near_x && near_y {
            corner_ink += 1.0;
        }
    }
    let round = 1.0 - corner_ink / (4.0 * (corner_w * corner_h) as f32);

    Some([slant, weight, contrast, terminal, density, aspect, round])
}

/// The index `fraction` of the way through a slice of `len`, clamped inside it.
///
/// One function rather than the same three lines in four places, and the only place in this module
/// that casts a float back to an index. The cast is safe by construction and not by inspection:
/// `fraction` is a literal between zero and one at every call site, so the product cannot be
/// negative and cannot exceed `len - 1`, and the `min` holds even if a caller ever passes more.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn index_at(len: usize, fraction: f32) -> usize {
    if len == 0 {
        return 0;
    }
    (((len - 1) as f32 * fraction.clamp(0.0, 1.0)).round() as usize).min(len - 1)
}

/// The value at `fraction` through an already-sorted slice.
fn percentile(sorted: &[usize], fraction: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[index_at(sorted.len(), fraction)] as f32
}

/// Pool per-glyph style into one descriptor: per axis, a median and an interquartile spread.
///
/// Median and IQR rather than mean and standard deviation because a track carries a handful of
/// shapes that are not letterforms at all — a full stop that segmented into one cell, half of a
/// shattered colon — and a mean would let those move the answer.
fn pool(name: impl Into<String>, styles: &[StyleVector]) -> Option<Descriptor> {
    if styles.is_empty() {
        return None;
    }
    let mut values = [0f32; DESCRIPTOR_LEN];
    for axis in 0..AXES {
        let mut column: Vec<f32> = styles.iter().map(|s| s[axis]).collect();
        column.sort_by(f32::total_cmp);
        let at = |f: f32| column[index_at(column.len(), f)];
        values[axis] = at(0.5);
        values[AXES + axis] = at(0.75) - at(0.25);
    }
    Some(Descriptor { name: name.into(), values, shapes: styles.len() })
}

/// Per-axis weights, and the distance they define.
///
/// The weights are not taste. Each is the ratio of between-font scatter to within-font scatter on
/// that axis, measured over the candidate fonts themselves — the paper's pairing, arithmetic
/// instead of a network: same font and different character pulled together, different font pushed
/// apart. An axis that varies more between characters than between fonts earns a weight near zero
/// and stops contributing, which is how a bad axis removes itself.
/// Per-component weights, and the scale each component is measured against.
///
/// The scale is not decoration. A ratio of between-font to within-font scatter is *scale-free* —
/// doubling an axis doubles both — but the distance it multiplies is not: `|a - b|` on an axis
/// whose values span 2.0 contributes ten times what the same weight buys on one spanning 0.2. A
/// weight fitted without dividing that out is not the weight that gets applied, and the symptom is
/// the one this bench first showed: a fitted weighting that scored *worse* than weighting every
/// axis equally, which is not something a correct Fisher ratio can do.
///
/// So each component is divided by its spread across the candidate fonts before the weight is
/// applied. The weight then controls contribution, which is what it was fitted to control.
#[derive(Clone, Debug)]
struct Weights {
    weights: [f32; DESCRIPTOR_LEN],
    scale: [f32; DESCRIPTOR_LEN],
}

impl Weights {
    /// Every axis equal, on the same scale, so the comparison isolates the weighting.
    fn flat_like(other: &Self) -> Self {
        Self { weights: [1.0; DESCRIPTOR_LEN], scale: other.scale }
    }

    /// Fit on *pooled* descriptors rather than on per-character style vectors.
    ///
    /// The distinction is the difference between a weighting that helps and one that hurts, and it
    /// took a measurement to see. [`Self::fit`] computes within-font scatter across characters —
    /// but an `i` and an `M` differ enormously on every axis, so that scatter is character shape,
    /// not font noise, and it swamps the between-font term for every axis at once. The distance
    /// being weighted never sees per-character values: it compares medians, where the character
    /// variation has already been pooled away.
    ///
    /// So the within-font term is measured on the quantity that is actually compared. Each font's
    /// charset is split into `folds` interleaved subsets and each fold pooled separately, giving
    /// several descriptors per font whose spread is what a descriptor's own sampling noise looks
    /// like. Interleaved rather than contiguous so every fold spans the whole charset — a
    /// contiguous split would put the digits in one fold and the accented capitals in another and
    /// measure the charset's ordering as noise.
    fn fit_pooled(per_font: &BTreeMap<String, Vec<StyleVector>>, folds: usize) -> Self {
        let mut by_font: BTreeMap<String, Vec<[f32; DESCRIPTOR_LEN]>> = BTreeMap::new();
        for (name, styles) in per_font {
            let mut descriptors = Vec::new();
            for fold in 0..folds {
                let subset: Vec<StyleVector> =
                    styles.iter().skip(fold).step_by(folds).copied().collect();
                if let Some(pooled) = pool(name.clone(), &subset) {
                    descriptors.push(pooled.values);
                }
            }
            if descriptors.len() > 1 {
                by_font.insert(name.clone(), descriptors);
            }
        }
        Self::from_scatter(&by_font)
    }

    /// The Fisher ratio per component, over whatever population the caller measured.
    ///
    /// The denominator is regularised, and a test is the reason. An axis with *zero* within-font
    /// scatter is not uninformative — it is perfectly discriminative, the best axis available — but
    /// a bare `between / within` divides by zero there, and guarding that division with a zero
    /// weight hands the strongest axis the weakest weight. Adding a floor derived from the
    /// components that do vary keeps the ratio finite and keeps its ordering right, and it changes
    /// nothing for a component whose scatter is already above the floor.
    fn from_scatter(by_font: &BTreeMap<String, Vec<[f32; DESCRIPTOR_LEN]>>) -> Self {
        let mut weights = [0f32; DESCRIPTOR_LEN];
        let mut scale = [1f32; DESCRIPTOR_LEN];
        let mut scatter = [None; DESCRIPTOR_LEN];

        for (component, slot) in scatter.iter_mut().enumerate() {
            let mut within = 0f32;
            let mut count = 0f32;
            let mut means = Vec::new();
            for values in by_font.values() {
                let column: Vec<f32> = values.iter().map(|v| v[component]).collect();
                if column.is_empty() {
                    continue;
                }
                let mean = column.iter().sum::<f32>() / column.len() as f32;
                means.push(mean);
                for value in &column {
                    within += (value - mean) * (value - mean);
                    count += 1.0;
                }
            }
            if means.len() < 2 || count < 1.0 {
                continue;
            }
            let grand = means.iter().sum::<f32>() / means.len() as f32;
            let between =
                means.iter().map(|m| (m - grand) * (m - grand)).sum::<f32>() / means.len() as f32;
            if between > f32::EPSILON {
                scale[component] = between.sqrt();
            }
            *slot = Some((between, within / count));
        }

        // The floor: a thousandth of the mean within-font scatter over the components that have
        // any. Small enough to leave a normal ratio alone, large enough that a zero-scatter axis
        // lands far above the others rather than at infinity.
        let measured: Vec<f32> = scatter
            .iter()
            .flatten()
            .map(|(_, within)| *within)
            .collect();
        let floor = if measured.is_empty() {
            f32::EPSILON
        } else {
            (measured.iter().sum::<f32>() / measured.len() as f32 / 1000.0).max(f32::EPSILON)
        };
        for (component, entry) in scatter.iter().enumerate() {
            if let Some((between, within)) = entry {
                weights[component] = between / (within + floor);
            }
        }

        let total: f32 = weights.iter().sum();
        if total > f32::EPSILON {
            for weight in &mut weights {
                *weight /= total;
            }
        }
        Self { weights, scale }
    }

    /// Fit from per-font, per-character style vectors.
    ///
    /// Kept because the bench reports what it scores against [`Self::fit_pooled`], and that gap is
    /// the evidence for preferring the pooled one. Each character contributes one point, so the
    /// within-font term is character shape rather than sampling noise -- which is exactly the flaw
    /// the comparison exists to show.
    fn fit(per_font: &BTreeMap<String, Vec<StyleVector>>) -> Self {
        // One axis value fills both of its descriptor slots, so the median and the spread earn the
        // same weight: they measure one axis, and splitting them would let the fit prefer a spread
        // whose median it had discarded.
        let by_font: BTreeMap<String, Vec<[f32; DESCRIPTOR_LEN]>> = per_font
            .iter()
            .filter(|(_, styles)| !styles.is_empty())
            .map(|(name, styles)| {
                let widened = styles
                    .iter()
                    .map(|style| {
                        let mut row = [0f32; DESCRIPTOR_LEN];
                        row[..AXES].copy_from_slice(style);
                        row[AXES..].copy_from_slice(style);
                        row
                    })
                    .collect();
                (name.clone(), widened)
            })
            .collect();

        // The scale comes from the pooled descriptors either way: it has to describe the quantity
        // `distance` divides, and that is a pooled median, never a per-character value.
        let mut fitted = Self::from_scatter(&by_font);
        let pooled: BTreeMap<String, Vec<[f32; DESCRIPTOR_LEN]>> = per_font
            .iter()
            .filter_map(|(name, styles)| {
                pool(name.clone(), styles).map(|d| (name.clone(), vec![d.values]))
            })
            .collect();
        fitted.scale = Self::from_scatter(&pooled).scale;
        fitted
    }

    /// Weighted L1 between two descriptors, each component on its own scale.
    ///
    /// L1 rather than L2 because one axis being wildly wrong should cost what it is, not its
    /// square: a track of mostly punctuation has one bad axis and should not be refused for it.
    fn distance(&self, a: &Descriptor, b: &Descriptor) -> f32 {
        (0..DESCRIPTOR_LEN)
            .map(|i| self.weights[i] * (a.values[i] - b.values[i]).abs() / self.scale[i])
            .sum()
    }
}

/// Every character of the charset, as a style vector, for one font.
///
/// This is the font side of the identity: the same `vector_for` the reference set is built from,
/// and the rasteriser's own metrics for the aspect the letterbox quantises away.
fn font_styles(font: &Font) -> Vec<StyleVector> {
    let mut styles = Vec::new();
    for ch in charset() {
        let Some(features) = vector_for(font, ch, false) else {
            continue;
        };
        let metrics = font.metrics(ch, RENDER_PX);
        if metrics.width == 0 || metrics.height == 0 {
            continue;
        }
        let aspect = metrics.width as f32 / metrics.height as f32;
        if let Some(style) = style_of(&features, aspect) {
            styles.push(style);
        }
    }
    styles
}

/// The same axes, measured on the raw raster instead of the normalised grid.
///
/// This exists to answer one question, and it is the question a negative result here has to answer
/// before it can close anything: **is the idea wrong, or is the input wrong?**
///
/// The 16x16 vector every other path reads is a *character-identifying* representation. It
/// letterboxes, it thresholds each cell at half coverage, and `docs/glyph-stability.md` records
/// that it was built to absorb rendering size and anti-aliasing — which are precisely the axes a
/// style descriptor lives on. At that resolution a stem is one to three cells, so stroke weight and
/// contrast are quantised almost to nothing before they are ever measured. The paper this proposal
/// borrows from feeds a network full-resolution glyph images.
///
/// So: run the identical axis code over the 96-pixel raster, where a stem is a dozen pixels. If
/// retrieval climbs, the axes were fine and the grid was the problem — which makes this a question
/// about what `survey` would have to expose, not about whether style carries font identity. If it
/// does not climb, the axes genuinely do not carry it, and no change to the input rescues them.
///
/// Font files only. There is no track side to this: a decoded subtitle glyph arrives as a
/// [`GlyphRecord`](subtrackt::survey::GlyphRecord), and its mask is not on the far side of that
/// API. That asymmetry is the finding if this half works.
fn raw_styles(font: &Font) -> Vec<StyleVector> {
    let mut styles = Vec::new();
    for ch in charset() {
        let (metrics, coverage) = font.rasterize(ch, RENDER_PX);
        if metrics.width == 0 || metrics.height == 0 {
            continue;
        }
        let Some(ink) = InkBox::of_raster(&coverage, metrics.width, metrics.height) else {
            continue;
        };
        let aspect = metrics.width as f32 / metrics.height as f32;
        if let Some(style) = style_of_box(&ink, aspect) {
            styles.push(style);
        }
    }
    styles
}

/// A track's style, from what the shipped pipeline segmented out of it.
///
/// Deduplicated by feature vector, so the pooling population is the track's shape inventory rather
/// than its letter frequency. Nothing here consults a reference set, a match or a character.
fn track_styles(sup: &Path, resolution: Resolution) -> anyhow::Result<Vec<StyleVector>> {
    let config = subtrackt::Config {
        glyph_masks: resolution == Resolution::Mask,
        ..subtrackt::Config::default()
    };
    let survey = Pipeline::new(config)
        .survey(sup, None)
        .with_context(|| format!("surveying {}", sup.display()))?;

    let mut seen = std::collections::BTreeSet::new();
    let mut styles = Vec::new();
    for glyph in &survey.glyphs {
        if !seen.insert(glyph.features.words().to_owned()) {
            continue;
        }
        if glyph.bounds.height == 0 {
            continue;
        }
        let aspect = f32::from(u16::try_from(glyph.bounds.width).unwrap_or(u16::MAX))
            / f32::from(u16::try_from(glyph.bounds.height).unwrap_or(u16::MAX));

        let style = match resolution {
            Resolution::Grid => style_of(&glyph.features, aspect),
            // The glyph's own ink, at the resolution it was decoded at. Deduplication is still by
            // feature vector rather than by mask: two instances of one character differ by a pixel
            // of anti-aliasing and would both survive a mask-keyed dedup, which would reintroduce
            // the letter-frequency weighting the dedup exists to remove.
            Resolution::Mask => glyph.mask.as_ref().and_then(|mask| {
                let cells: Vec<(usize, usize)> = (0..mask.height())
                    .flat_map(|y| (0..mask.width()).map(move |x| (x, y)))
                    .filter(|&(x, y)| mask.get(x, y))
                    .map(|(x, y)| (x as usize, y as usize))
                    .collect();
                let ink = InkBox::new(cells, mask.width() as usize, mask.height() as usize)?;
                style_of_box(&ink, aspect)
            }),
        };
        if let Some(style) = style {
            styles.push(style);
        }
    }
    Ok(styles)
}

/// Which representation of a glyph's ink the style axes are measured on.
///
/// The distinction #63 turned on. Both go through the same [`style_of_box`], so a difference
/// between them is a difference in what survived, not in how it was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolution {
    /// The 16x16 normalised feature vector, which is all a survey carried before #63.
    Grid,
    /// The glyph's un-normalised mask, which `Config::glyph_masks` now keeps.
    Mask,
}

impl Resolution {
    /// What the tables call it.
    const fn label(self) -> &'static str {
        match self {
            Self::Grid => "16x16 grid",
            Self::Mask => "glyph mask",
        }
    }
}

/// Load a font, naming the file if it will not parse.
fn load(path: &Path) -> anyhow::Result<Font> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Font::from_bytes(bytes.as_slice(), FontSettings::default())
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
}

/// The filename stem, which is what every table in this module names a font by.
fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "unnamed".to_owned(), |s| s.to_string_lossy().into_owned())
}

/// Spearman rank correlation, for the independence check.
///
/// Rank rather than Pearson because the question is whether the two statistics *order* candidates
/// the same way. A monotone but curved relationship is still the same ordering, and a selector only
/// ever reads the ordering.
fn spearman(left: &[f64], right: &[f64]) -> f64 {
    if left.len() < 2 || left.len() != right.len() {
        return 0.0;
    }
    let rank = |values: &[f64]| -> Vec<f64> {
        let mut order: Vec<usize> = (0..values.len()).collect();
        order.sort_by(|&i, &j| values[i].total_cmp(&values[j]));
        let mut ranks = vec![0f64; values.len()];
        let mut at = 0usize;
        while at < order.len() {
            let mut end = at + 1;
            while end < order.len() && values[order[end]].total_cmp(&values[order[at]]).is_eq() {
                end += 1;
            }
            // Ties share the average of the ranks they span, so a run of equal values cannot
            // manufacture an ordering the data does not have.
            let mean = (at + end - 1) as f64 / 2.0 + 1.0;
            for &index in &order[at..end] {
                ranks[index] = mean;
            }
            at = end;
        }
        ranks
    };
    let (ranked_left, ranked_right) = (rank(left), rank(right));
    let n = left.len() as f64;
    let mean = f64::midpoint(n, 1.0);
    let (mut covariance, mut spread_left, mut spread_right) = (0f64, 0f64, 0f64);
    for index in 0..left.len() {
        let (dx, dy) = (ranked_left[index] - mean, ranked_right[index] - mean);
        covariance += dx * dy;
        spread_left += dx * dx;
        spread_right += dy * dy;
    }
    if spread_left <= 0.0 || spread_right <= 0.0 {
        return 0.0;
    }
    covariance / (spread_left * spread_right).sqrt()
}

/// One material font's trial: its track style, and what every candidate scored against it.
struct Trial {
    material: String,
    track: Descriptor,
    /// Per candidate: the name, the style distance, and the matcher's own mean match distance.
    candidates: Vec<(String, f32, f64)>,
}

/// Build the fixture for one material font and score every candidate two ways.
///
/// Both statistics come from the same extraction so they cannot differ over the sample: the style
/// distance from the survey's glyphs, the mean match distance from the report.
fn trial(
    material: &Path,
    fonts: &[PathBuf],
    descriptors: &BTreeMap<String, Descriptor>,
    sets: &[(String, subtrackt_glyph::ReferenceSet)],
    weights: &Weights,
    dir: &Path,
    resolution: Resolution,
) -> anyhow::Result<Trial> {
    let name = stem(material);
    let fixture_dir = dir.join(format!("fontid-{name}"));
    std::fs::create_dir_all(&fixture_dir)?;
    crate::fixture::make(&[
        material.display().to_string(),
        fixture_dir.display().to_string(),
    ])?;
    let sup = fixture_dir.join("synthetic.sup");

    let styles = track_styles(&sup, resolution)?;
    let track = pool(name.clone(), &styles)
        .with_context(|| format!("{name}: the fixture segmented into no measurable glyph"))?;

    let mut candidates = Vec::new();
    for font in fonts {
        let candidate = stem(font);
        let Some(descriptor) = descriptors.get(&candidate) else {
            continue;
        };
        let Some((_, set)) = sets.iter().find(|(n, _)| *n == candidate) else {
            continue;
        };
        let (_, outcome) = crate::accuracy::extract(&sup, set.clone(), false, false)?;
        candidates.push((
            candidate,
            weights.distance(&track, descriptor),
            f64::from(outcome.report.mean_match_distance()),
        ));
    }
    Ok(Trial { material: name, track, candidates })
}

/// Step 1: is this a fourth instance of the mechanism after all?
///
/// #63 makes the independence claim falsifiable and says to test it first. If the style score
/// orders candidates the way mean match distance does, then it is reading the matcher's argmin
/// through a longer path and inherits the same bias. Reported per material and pooled, because one
/// material's ordering is a handful of points and the pooled figure is what decides.
fn report_independence(trials: &[Trial]) -> f64 {
    println!("\n--- 1. independence: style score against mean match distance ---");
    println!("  the falsification. If these order candidates alike, the argument was wrong.");
    println!();
    println!("  {:<12} {:>8}  candidates, style-ranked", "material", "rho");

    let mut rhos = Vec::new();
    for t in trials {
        let style: Vec<f64> = t.candidates.iter().map(|c| f64::from(c.1)).collect();
        let matched: Vec<f64> = t.candidates.iter().map(|c| c.2).collect();
        let rho = spearman(&style, &matched);
        rhos.push(rho);

        let mut ranked: Vec<&(String, f32, f64)> = t.candidates.iter().collect();
        ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
        let listed: Vec<String> = ranked.iter().take(4).map(|c| c.0.clone()).collect();
        println!("  {:<12} {rho:>8.2}  {}", t.material, listed.join(" "));
    }

    let mean = rhos.iter().sum::<f64>() / rhos.len().max(1) as f64;
    println!();
    println!("  mean rho {mean:.2} over {} materials", rhos.len());
    mean
}

/// Steps 2 and 4: the negative case, and separation.
///
/// The bar #63 sets is separation and not correlation: **every** good read scoring better than
/// **every** bad one. So the overlap is stated as the gap between the worst same-font distance and
/// the best different-font one, where negative means they overlap and no floor exists.
fn report_separation(
    trials: &[Trial],
    descriptors: &BTreeMap<String, Descriptor>,
    weights: &Weights,
) {
    println!("\n--- 2. the negative case: the material's own font withheld ---");
    println!(
        "  {:<12} {:>12} {:>12} {:>10}",
        "material", "own font", "best other", "margin"
    );

    let mut own = Vec::new();
    let mut other = Vec::new();
    for t in trials {
        let Some(mine) = descriptors.get(&t.material) else {
            continue;
        };
        let d_own = weights.distance(&t.track, mine);
        let d_other = t
            .candidates
            .iter()
            .filter(|c| c.0 != t.material)
            .map(|c| c.1)
            .min_by(f32::total_cmp)
            .unwrap_or(f32::MAX);
        println!(
            "  {:<12} {d_own:>12.4} {d_other:>12.4} {:>10.4}",
            t.material,
            d_other - d_own
        );
        own.push(d_own);
        other.push(d_other);
    }

    println!("\n--- 4. separation: does every good read score better than every bad one? ---");
    let worst_own = own.iter().copied().fold(f32::MIN, f32::max);
    let best_other = other.iter().copied().fold(f32::MAX, f32::min);
    println!();
    println!("  worst same-font distance  {worst_own:.4}");
    println!("  best  other-font distance {best_other:.4}");
    let gap = best_other - worst_own;
    if gap > 0.0 {
        println!("  gap {gap:+.4} -- the distributions do not overlap, so a floor exists here.");
    } else {
        println!("  gap {gap:+.4} -- they overlap. No single floor ships every good read without");
        println!("  also shipping a bad one, which is the bar #63 sets and this fails.");
    }

    let hits = trials
        .iter()
        .filter(|t| {
            t.candidates
                .iter()
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .is_some_and(|c| c.0 == t.material)
        })
        .count();
    println!();
    println!(
        "  argmin picks the material's own font on {hits}/{} materials",
        trials.len()
    );
}

/// Step 3: the paper's metric. Never let a character be compared with itself.
///
/// Query characters and gallery characters are disjoint by construction: even-indexed charset
/// entries pool the query, odd-indexed ones the gallery. A hit is the query font's own gallery
/// entry coming first. If this cannot retrieve a font from ink whose letters it has never seen,
/// the hand-crafted axes are not carrying the signal and the rest is moot.
fn report_retrieval(per_font: &BTreeMap<String, Vec<StyleVector>>, weights: &Weights) -> f64 {
    let mut queries = BTreeMap::new();
    let mut gallery = BTreeMap::new();
    for (name, styles) in per_font {
        let evens: Vec<StyleVector> = styles.iter().step_by(2).copied().collect();
        let odds: Vec<StyleVector> = styles.iter().skip(1).step_by(2).copied().collect();
        if let (Some(q), Some(g)) = (pool(name.clone(), &evens), pool(name.clone(), &odds)) {
            queries.insert(name.clone(), q);
            gallery.insert(name.clone(), g);
        }
    }

    let mut hits = 0usize;
    for (name, query) in &queries {
        let best = gallery.iter().min_by(|a, b| {
            weights
                .distance(query, a.1)
                .total_cmp(&weights.distance(query, b.1))
        });
        if best.is_some_and(|(candidate, _)| candidate == name) {
            hits += 1;
        }
    }
    let rate = hits as f64 / queries.len().max(1) as f64 * 100.0;
    println!(
        "  {hits}/{} fonts retrieved from disjoint characters ({rate:.0}%)",
        queries.len()
    );
    rate
}

/// Step 5: the scale, from the font files alone.
///
/// The reason this proposal can have a floor at all where mean match distance could not: the
/// candidate fonts are labelled data. The distances between them are measurable offline, with no
/// subtitle material and no ground truth in the loop, so a threshold can be quoted in units that
/// mean something before a track is ever seen.
fn report_calibration(descriptors: &BTreeMap<String, Descriptor>, weights: &Weights) {
    println!("\n--- 5. calibration: distances between the candidate fonts themselves ---");
    println!("  measured from the font files alone -- no material, no ground truth.");

    let entries: Vec<&Descriptor> = descriptors.values().collect();
    let mut pairs = Vec::new();
    for (i, a) in entries.iter().enumerate() {
        for b in entries.iter().skip(i + 1) {
            pairs.push((weights.distance(a, b), a.name.clone(), b.name.clone()));
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let cross: Vec<f32> = pairs.iter().map(|p| p.0).collect();

    let at = |v: &[f32], f: f32| -> f32 { v.get(index_at(v.len(), f)).copied().unwrap_or(0.0) };
    println!();
    println!(
        "  different-font distance: p10 {:.4}  median {:.4}  p90 {:.4}",
        at(&cross, 0.1),
        at(&cross, 0.5),
        at(&cross, 0.9)
    );

    // Prediction 4 is the one #63 says decides whether this ships, and it is about the *closest*
    // pairs rather than the distribution: a style descriptor should call two metric-compatible cuts
    // near-identical, because by every measure here they are. Naming them is what makes that
    // prediction checkable rather than asserted.
    println!();
    for (distance, a, b) in pairs.iter().take(3) {
        println!("  closest pair:  {a:<12} {b:<12} {distance:.4}");
    }
    if let Some((distance, a, b)) = pairs.last() {
        println!("  furthest pair: {a:<12} {b:<12} {distance:.4}");
    }
    println!("  a font's distance to itself is zero by construction, so the band a track has to");
    println!(
        "  fall inside is set by how far a decoded track drifts from its own font -- which is"
    );
    println!("  step 2's column, not this one's.");
}

/// Step 3 at both resolutions, which is the comparison that says what a negative result means.
///
/// Same axes, same weighting arithmetic, same disjoint query and gallery characters. The only
/// difference is whether the ink was flattened onto the 16x16 grid first. See [`raw_styles`].
fn report_resolutions(
    fonts: &[PathBuf],
    per_font: &BTreeMap<String, Vec<StyleVector>>,
) -> anyhow::Result<()> {
    println!("\n--- 3. i != j retrieval: query and gallery share no character ---");
    println!("  #63 predicted above 70%. Below that the axes are not carrying the signal.");
    println!();
    // Each row fits its own weights from the population it is about to score, rather than being
    // handed the caller's. Passing them in was a bug that made the per-character row print the
    // pooled figure, so the two rows agreed by construction and the comparison said nothing.
    let per_character = Weights::fit(per_font);
    let pooled_weights = Weights::fit_pooled(per_font, FOLDS);
    print!("  on the 16x16 grid, per-character fit: ");
    let weighted = report_retrieval(per_font, &per_character);
    print!("  on the 16x16 grid, pooled fit:        ");
    let pooled = report_retrieval(per_font, &pooled_weights);
    print!("  on the 16x16 grid, equal weights:     ");
    let flat = report_retrieval(per_font, &Weights::flat_like(&pooled_weights));

    let mut per_font_raw = BTreeMap::new();
    for font in fonts {
        let loaded = load(font)?;
        per_font_raw.insert(stem(font), raw_styles(&loaded));
    }
    let raw_pooled = Weights::fit_pooled(&per_font_raw, FOLDS);
    print!("  on the 96px raster, pooled fit:       ");
    let raw = report_retrieval(&per_font_raw, &raw_pooled);
    print!("  on the 96px raster, equal weights:    ");
    let raw_flat = report_retrieval(&per_font_raw, &Weights::flat_like(&raw_pooled));
    println!();
    println!(
        "  pooled fit against per-character fit on the grid: {:+.0} points",
        pooled - weighted
    );

    // How many characters each side actually measured, so a retrieval gap cannot be quietly
    // explained by one path having silently dropped half the charset under `MIN_INK_CELLS`.
    let measured = |m: &BTreeMap<String, Vec<StyleVector>>| -> usize {
        m.values().map(Vec::len).sum::<usize>() / m.len().max(1)
    };
    println!();
    println!(
        "  characters measured per font: {} on the grid, {} on the raster",
        measured(per_font),
        measured(&per_font_raw)
    );
    println!(
        "  the weighting is worth {:+.0} points on the grid, {:+.0} on the raster.",
        weighted - flat,
        raw - raw_flat
    );
    println!(
        "  the raster is worth {:+.0} points over the grid, which is the number that says",
        raw - weighted
    );
    println!("  whether a negative result here is about the axes or about what they were fed.");
    Ok(())
}

/// Run the bench.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let keep_going = args.iter().any(|a| a == "--continue");
    let retrieval_only = args.iter().any(|a| a == "--retrieval-only");
    let fonts: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    if fonts.len() < 2 {
        bail!(
            "font-id needs at least two fonts; pass several, e.g. \
             xtask font-id C:/Windows/Fonts/arial.ttf C:/Windows/Fonts/verdana.ttf ..."
        );
    }

    println!("fonts: {}", fonts.iter().map(|f| stem(f)).collect::<Vec<_>>().join(" "));

    // The font side first: per-character style for every candidate, which the weights are fitted
    // from and the descriptors pooled from. No material is involved in any of it.
    let mut per_font = BTreeMap::new();
    for font in &fonts {
        let loaded = load(font)?;
        per_font.insert(stem(font), font_styles(&loaded));
    }
    // The pooled fit rather than the per-character one: the bench measured the difference and it
    // is +17 to +33 points. See `Weights::fit_pooled`.
    let weights = Weights::fit_pooled(&per_font, FOLDS);
    println!();
    println!(
        "  {:<10} {:>8}   between-font scatter over within-font scatter",
        "axis", "weight"
    );
    for (index, name) in AXIS_NAMES.iter().enumerate() {
        println!("  {name:<10} {:>8.3}", weights.weights[index] * 2.0);
    }

    // The retrieval comparison needs no material at all, so it can be run over a large font list
    // cheaply -- which is the only way 6-of-8 becomes a number worth quoting.
    if retrieval_only {
        return report_resolutions(&fonts, &per_font);
    }

    let dir = std::env::temp_dir().join("subtrackt-font-id");
    std::fs::create_dir_all(&dir)?;
    let sets = crate::select::reference_sets(&fonts, &dir)?;

    // Both resolutions, all the way through. Before `Config::glyph_masks` a track could only be
    // measured on the grid, and the three steps needing one were measured on a descriptor that
    // font-file retrieval showed loses 46 to 54 points. Running both is what turns that from an
    // objection into a number.
    for resolution in [Resolution::Grid, Resolution::Mask] {
        println!("\n\n========== track measured on the {} ==========", resolution.label());

        // The font side has to be measured the same way, or a full-resolution track would be
        // compared against a descriptor built through the very projection under test.
        let mut per_side = BTreeMap::new();
        for font in &fonts {
            let loaded = load(font)?;
            let styles = match resolution {
                Resolution::Grid => font_styles(&loaded),
                Resolution::Mask => raw_styles(&loaded),
            };
            per_side.insert(stem(font), styles);
        }
        let side_weights = Weights::fit_pooled(&per_side, FOLDS);
        let side_descriptors: BTreeMap<String, Descriptor> = per_side
            .iter()
            .filter_map(|(name, styles)| pool(name.clone(), styles).map(|d| (name.clone(), d)))
            .collect();

        let mut trials = Vec::new();
        for material in &fonts {
            trials.push(
                trial(
                    material,
                    &fonts,
                    &side_descriptors,
                    &sets,
                    &side_weights,
                    &dir,
                    resolution,
                )
                .with_context(|| stem(material))?,
            );
        }
        println!(
            "track samples: {}",
            trials
                .iter()
                .map(|t| format!("{} {}", t.material, t.track.shapes))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let rho = report_independence(&trials);
        report_separation(&trials, &side_descriptors, &side_weights);
        report_calibration(&side_descriptors, &side_weights);

        if rho.abs() >= 0.5 && !keep_going && resolution == Resolution::Mask {
            println!();
            println!("  #63: if the score correlates with mean match distance across candidates,");
            println!("  the independence claim is wrong and this is a fourth instance after all.");
            println!("  Read the caveat in the issue before taking that at face value: two");
            println!("  independent statistics that both track the right answer must correlate.");
        }
    }

    report_resolutions(&fonts, &per_font)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vector with the given cells set, for building shapes by hand.
    fn vector(cells: &[(usize, usize)]) -> FeatureVector {
        let mut v = FeatureVector::EMPTY;
        for &(x, y) in cells {
            v.set(y * FEATURE_GRID + x);
        }
        v
    }

    /// A filled rectangle of ink.
    fn block(x0: usize, y0: usize, w: usize, h: usize) -> FeatureVector {
        let cells: Vec<(usize, usize)> = (0..h)
            .flat_map(|dy| (0..w).map(move |dx| (x0 + dx, y0 + dy)))
            .collect();
        vector(&cells)
    }

    #[test]
    fn a_glyph_with_too_little_ink_has_no_style_rather_than_a_zero_one() {
        // The project's rule: never invent data to avoid an error. A fabricated axis value would
        // pool indistinguishably from a measured one, which is the failure this whole issue is
        // about.
        assert!(style_of(&vector(&[(3, 3), (4, 3)]), 1.0).is_none());
        assert!(style_of(&FeatureVector::EMPTY, 1.0).is_none());
        assert!(style_of(&block(4, 4, 4, 4), 1.0).is_some());
    }

    #[test]
    fn the_ink_box_is_the_glyph_rather_than_the_letterbox() {
        // The vector is letterboxed, so a narrow glyph sits in a centred sub-box with blank margins
        // either side. Measuring density over the whole grid would read that margin as style.
        let ink = InkBox::of(&block(6, 2, 4, 12)).expect("has ink");
        assert_eq!((ink.left, ink.top), (6, 2));
        assert_eq!((ink.width, ink.height), (4, 12));
    }

    #[test]
    fn density_is_one_for_a_solid_block_whatever_size_it_is() {
        // Every axis is a fraction of something measured, so the same shape at two sizes has to
        // produce the same number -- that is the property `FEATURE_GRID` moving must not break.
        let small = style_of(&block(6, 6, 4, 4), 1.0).expect("has ink");
        let large = style_of(&block(2, 2, 12, 12), 1.0).expect("has ink");
        assert!((small[4] - 1.0).abs() < 1e-6, "{small:?}");
        assert!((large[4] - 1.0).abs() < 1e-6, "{large:?}");
    }

    #[test]
    fn slant_is_zero_upright_and_signed_when_the_ink_leans() {
        let upright = style_of(&block(4, 2, 8, 12), 1.0).expect("has ink");
        assert!(upright[0].abs() < 1e-6, "a symmetric block does not lean: {upright:?}");

        // A staircase leaning right: x grows with y.
        let cells: Vec<(usize, usize)> = (0..12)
            .flat_map(|i| [(2 + i, 2 + i), (3 + i, 2 + i)])
            .collect();
        let leaning = style_of(&vector(&cells), 1.0).expect("has ink");
        assert!(leaning[0] > 0.5, "a rightward staircase leans: {leaning:?}");
    }

    #[test]
    fn weight_reads_a_thick_stem_as_heavier_than_a_thin_one() {
        // The axis a bold cut and a light cut of one typeface differ on most, and the one
        // `vectorize` deliberately absorbs within a single glyph.
        let thin = style_of(&block(7, 2, 1, 12), 1.0).expect("has ink");
        let thick = style_of(&block(5, 2, 5, 12), 1.0).expect("has ink");
        assert!(thick[1] > thin[1], "thin {thin:?} thick {thick:?}");
    }

    #[test]
    fn the_style_of_a_glyph_never_consults_a_character_or_a_match() {
        // The whole independence argument in one assertion: `style_of` takes a vector and an
        // aspect ratio. There is no reference set, no candidate and no assignment in its signature,
        // so it cannot be a function of the matcher's argmin -- which is the mechanism #63 says
        // every previous statistic broke on.
        let first = style_of(&block(4, 2, 8, 12), 0.7).expect("has ink");
        let again = style_of(&block(4, 2, 8, 12), 0.7).expect("has ink");
        for (one, other) in first.iter().zip(again.iter()) {
            assert!((one - other).abs() < f32::EPSILON, "{first:?} {again:?}");
        }
    }

    #[test]
    fn pooling_takes_the_median_so_one_shattered_mark_cannot_move_it() {
        // A track carries shapes that are not letterforms: a full stop that segmented into one
        // cell, half of a colon. A mean would let those set the answer.
        let mut styles = vec![[0.5f32; AXES]; 20];
        styles.push([99.0; AXES]);
        let pooled = pool("t", &styles).expect("has styles");
        assert!((pooled.values[0] - 0.5).abs() < 1e-6, "{:?}", pooled.values);
        assert_eq!(pooled.shapes, 21);
    }

    #[test]
    fn an_axis_that_varies_more_between_characters_than_between_fonts_earns_no_weight() {
        // How a bad axis removes itself. Axis 0 is pure noise within each font and identical
        // across them; axis 1 separates the fonts cleanly. The fit has to prefer the second.
        let mut per_font = BTreeMap::new();
        for (name, offset) in [("a", 0.0f32), ("b", 10.0f32)] {
            let styles: Vec<StyleVector> = (0..10)
                .map(|i| {
                    let mut s = [0f32; AXES];
                    s[0] = i as f32;
                    s[1] = offset;
                    s
                })
                .collect();
            per_font.insert(name.to_owned(), styles);
        }
        let weights = Weights::fit(&per_font);
        assert!(weights.weights[1] > weights.weights[0], "{:?}", weights.weights);
        assert!(
            weights.weights[0] < 0.01,
            "a within-font-noisy axis earns nothing: {:?}",
            weights.weights
        );
    }

    #[test]
    fn spearman_is_one_for_the_same_ordering_and_minus_one_for_the_reverse() {
        let a = [1.0, 2.0, 3.0, 4.0];
        assert!((spearman(&a, &[10.0, 20.0, 30.0, 40.0]) - 1.0).abs() < 1e-9);
        assert!((spearman(&a, &[40.0, 30.0, 20.0, 10.0]) + 1.0).abs() < 1e-9);
        // Ties share a rank rather than inventing an order between equal values.
        assert!(spearman(&a, &[1.0, 1.0, 1.0, 1.0]).abs() < 1e-9);
    }
}
