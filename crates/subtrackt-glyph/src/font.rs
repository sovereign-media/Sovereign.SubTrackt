//! Rendering a font into a reference set.
//!
//! Behind the off-by-default `font` feature, because it is the one part of this crate that needs a
//! third-party dependency. `CLAUDE.md` says library crates take none; it also says the rule is
//! "justify it, not never", and turning an outline into coverage is as squarely someone else's
//! problem domain as the zlib exception that came before it. Off by default means a consumer who
//! does not want a rasteriser still gets a crate with nothing but `subtrackt-core` under it.
//!
//! It lives here rather than in a crate of its own because everything it needs is already in this
//! one — [`crate::binarize`], [`crate::ccl`], [`crate::feature`], [`crate::mark`],
//! [`crate::reference`] — and a separate crate would only add an edge to the stage fan that
//! `docs/architecture.md` keeps flat.
//!
//! The property that matters is the one [`vector_for`] states: a reference vector goes through the
//! *same* normalisation the runtime applies to a decoded subtitle bitmap. Both the CLI and xtask
//! call this code rather than each keeping a copy, so the two cannot drift into producing sets that
//! disagree.

use fontdue::{Font, FontSettings};
use subtrackt_core::{Error, FeatureVector, InkAspect, LineMetrics, MarkSlope, Rect, Result};

use crate::binarize::{BinaryMask, CoverageMask};
use crate::ccl::{self, ComponentFilter};
use crate::feature::{AspectPolicy, vectorize, vectorize_coverage};
use crate::group::GroupedGlyph;
use crate::reference::{ReferenceEntry, ReferenceSet, Style};

/// Pixel size glyphs are rasterised at.
///
/// Larger than any real subtitle glyph on purpose. Normalisation is scale-invariant, so the only
/// thing size buys here is a cleaner rasterisation to normalise *from*.
pub const RENDER_PX: f32 = 96.0;

/// Coverage above which a rasterised pixel counts as ink.
///
/// Matches the binarizer's default of half, so a reference glyph is thresholded the same way a
/// decoded one is.
const INK: u8 = 128;

/// What a reference set covers.
///
/// ASCII printable, plus the Latin-1 letters that carry the accents #6 works to preserve. There is
/// no point including a character the segmenter cannot deliver as one glyph.
#[must_use]
pub fn charset() -> Vec<char> {
    let mut chars: Vec<char> = (0x21u8..0x7F).map(char::from).collect();
    chars.extend("\u{c0}\u{c1}\u{c2}\u{c4}\u{c7}\u{c8}\u{c9}\u{ca}\u{cb}".chars());
    chars.extend("\u{cc}\u{cd}\u{ce}\u{cf}\u{d1}\u{d2}\u{d3}\u{d4}\u{d6}".chars());
    chars.extend("\u{d9}\u{da}\u{db}\u{dc}\u{df}\u{e0}\u{e1}\u{e2}\u{e4}".chars());
    chars.extend("\u{e7}\u{e8}\u{e9}\u{ea}\u{eb}\u{ec}\u{ed}\u{ee}\u{ef}".chars());
    chars.extend("\u{f1}\u{f2}\u{f3}\u{f4}\u{f6}\u{f9}\u{fa}\u{fb}\u{fc}".chars());
    // The eighth note. Not decoration: it opens and closes every sung line in an SDH track, and
    // #118 measured it failing **100% of the time** for the only reason a character can fail that
    // completely — no reference set could contain it, because this list is what a set is generated
    // from. A face that does not draw it contributes nothing and is skipped, as for any character
    // whose outline is empty.
    chars.push('\u{266a}');
    chars
}

/// Rasterise one character and normalise it through the pipeline's own transform.
///
/// `grey` must match the pipeline's `grey_coverage` setting. A reference built through a different
/// normalisation than the runtime uses would be compared against a subtly different transform, and
/// every distance it produced would be meaningless.
///
/// **One** vector, at [`RENDER_PX`] under the ink threshold, on the rasteriser's box. A generated set carries
/// [`RENDERINGS`] instead — see [`vectors_under`] — because #99 measured that one box cannot cover
/// what the material does. This stays because the instruments that compare *typefaces* against each
/// other need one canonical vector per character rather than a set, and because holding it at the
/// rendering it has always had keeps every figure they have already recorded comparable.
#[must_use]
pub fn vector_for(font: &Font, ch: char, grey: bool) -> Option<FeatureVector> {
    render(font, ch, RENDERINGS[0], grey)
}

/// One set of conditions a reference glyph can be rasterised under.
///
/// Three fields, and between them they are the whole of what #99 found separating the reference
/// side from the material: the size it is drawn at, the threshold that decides what is ink, and —
/// the one that turned out to carry the entire effect — which box the result is letterboxed from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rendering {
    /// Pixel size to rasterise at.
    pub px: f32,
    /// Coverage above which a pixel counts as ink.
    pub ink: u8,
    /// Which box the normalisation letterboxes.
    pub crop: Crop,
}

/// Which box a rendering letterboxes.
///
/// The runtime letterboxes a *connected component's* box — the ink that survived thresholding —
/// while fontdue returns a bitmap that includes every pixel with any coverage at all, down to 1. On
/// most glyphs those differ by a row or a column, and letterboxing is precisely the operation that
/// turns one row into a whole grid cell.
///
/// [`Crop::Raster`] exists so a bench can still generate what the tool produced before #99. Nothing
/// should choose it deliberately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crop {
    /// The rasteriser's box, which is what the reference side used before #99.
    Raster,
    /// The bounding box of what the threshold kept, which is what the runtime uses.
    Ink,
}

/// The renderings a generated reference set carries for every character.
///
/// Two entries per character, from **one** rasterisation at **one** threshold, differing only in
/// which box the normalisation letterboxes. #99 swept this against a real Blu-ray and the numbers
/// are in `docs/reference-rendering.md`: character error **5.5% to 2.8%**, coverage **96.2% to
/// 99.5%**, and the full stop — 48.8% of that disc's errors and 87% of its unread glyphs before
/// this — from 660 wrong to 6.
///
/// The mechanism is worth stating, because every plausible-sounding alternative was measured and
/// bought nothing. Rendering size does not matter: a second entry at 21px, 48px or 90px changes the
/// disc's figures by zero as long as it carries the same box. The ink threshold does not matter
/// either: 60, 128, 160 and 210 all land within six characters of each other. **The box is the
/// whole effect**, and it is the ±1px edge shift `docs/glyph-stability.md` already priced at 30
/// cells — "as much as character identity itself":
///
/// - [`Crop::Raster`] is fontdue's box, which includes every pixel with any coverage at all, down
///   to 1. Its margin is a fixed *fraction* of the glyph, so it is scale-invariant.
/// - [`Crop::Ink`] is the box of what survived the threshold, which is what the runtime
///   letterboxes because that is what a connected component's bounds are. Its margin is a fixed
///   *pixel* inset, so its cost grows as glyphs shrink.
///
/// On a 68px `M` a one-pixel inset is 1.5% of the box and nothing changes. On a 13px full stop it
/// is 15%, and the two vectors are 60 cells apart against a 51-cell ceiling — which is exactly why
/// `.` matched nothing and why small glyphs were the ones failing.
///
/// Neither box is *the* right one, because the material's own box lands somewhere between them and
/// where depends on the glyph's size in the stream. So both are carried, which is carrying the axis
/// rather than guessing a point on it. A third box, further inset, was measured and gains nothing.
///
/// This costs set size — about 1.7x, not 2x, because where the two boxes normalise to the same
/// vector the duplicate is dropped, and on a large glyph they usually do — and nothing else. The format already carries several entries per
/// character — that is how `--italic` puts two cuts in one set — and
/// [`crate::matcher::HammingMatcher::scan_with`] already treats a second entry for the winning
/// character as something that can improve the winner and can never become its own runner-up. So
/// there is no version bump: a v3 reader loads one of these sets unchanged.
pub const RENDERINGS: [Rendering; 2] = [
    Rendering { px: RENDER_PX, ink: INK, crop: Crop::Raster },
    Rendering { px: RENDER_PX, ink: INK, crop: Crop::Ink },
];

/// Every vector a generated set carries for one character.
///
/// Empty when the character has no outline at any of [`RENDERINGS`], which is the same answer
/// [`vector_for`] gives with `None` and means the same thing: the font cannot draw it.
#[must_use]
pub fn vectors_for(font: &Font, ch: char, grey: bool) -> Vec<FeatureVector> {
    vectors_under(font, ch, grey, &RENDERINGS)
}

/// Every vector a generated set carries for one character, under the renderings given.
///
/// Exists for `xtask render-sweep`, which is what chose [`RENDERINGS`]. #45 is the reason it is a
/// parameter at all: a change to what a reference vector *is* silently re-prices every threshold
/// measured against the old one, so the list has to be swept end to end rather than reasoned about,
/// and a sweep that cannot vary the thing it is sweeping is not a sweep.
#[must_use]
pub fn vectors_under(
    font: &Font,
    ch: char,
    grey: bool,
    renderings: &[Rendering],
) -> Vec<FeatureVector> {
    let mut out: Vec<FeatureVector> = Vec::with_capacity(renderings.len());
    for &rendering in renderings {
        if let Some(vector) = render(font, ch, rendering, grey) {
            // Two sizes can normalise to the same vector — which is the point of normalisation —
            // and a duplicate entry is scan cost for no separation.
            if !out.contains(&vector) {
                out.push(vector);
            }
        }
    }
    out
}

/// Rasterise and normalise one character under one rendering.
///
/// The bounds handed to [`vectorize`] are the **ink's** bounding box, not the rasteriser's. That is
/// not a tuning choice: the runtime letterboxes a connected component's box, and fontdue returns a
/// bitmap that includes every pixel with any coverage at all — down to 1 — so the two boxes differ
/// by a row or a column on most glyphs. Letterboxing is precisely the operation that turns one row
/// into a whole grid cell, so the difference is not small. #99 measured it on its own, with size and
/// threshold held still, at 6 fewer unread samples and 14 fewer wrong ones out of 973.
fn render(font: &Font, ch: char, rendering: Rendering, grey: bool) -> Option<FeatureVector> {
    let (metrics, coverage) = font.rasterize(ch, rendering.px);
    if metrics.width == 0 || metrics.height == 0 {
        return None;
    }

    let width = u32::try_from(metrics.width).ok()?;
    let height = u32::try_from(metrics.height).ok()?;
    let bits: Vec<bool> = coverage.iter().map(|c| *c >= rendering.ink).collect();
    let mask = BinaryMask::from_bits(width, height, &bits).ok()?;
    if mask.foreground_count() == 0 {
        return None;
    }
    let bounds = match rendering.crop {
        Crop::Ink => ink_bounds(&mask)?,
        Crop::Raster => Rect::new(0, 0, width, height),
    };

    if grey {
        // fontdue hands back per-pixel coverage already, which is exactly what the runtime derives
        // from a subtitle palette. No thresholding step on this path — but the *box* still comes
        // from the thresholded mask, because that is where the runtime's box comes from.
        let plane = CoverageMask::from_values(width, height, coverage).ok()?;
        return vectorize_coverage(&plane, bounds, AspectPolicy::Letterbox).ok();
    }
    vectorize(&mask, bounds, AspectPolicy::Letterbox).ok()
}

/// The bounding box of everything the mask calls foreground.
fn ink_bounds(mask: &BinaryMask) -> Option<Rect> {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0u32, 0u32);
    let mut any = false;
    for y in 0..mask.height() {
        for x in 0..mask.width() {
            if mask.get(x, y) {
                any = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    any.then(|| Rect::new(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
}

/// The size a character's aspect ratio is read at.
///
/// Not [`RENDER_PX`], and not the size that reads the ratio most *accurately* either. Both of those
/// were tried and measured, and the reason this is 56 is the whole of #113.
///
/// #109 established the problem: `I` is 7% wider than `l` in Arial's outlines, which at 96 pixels is
/// six tenths of one pixel, so the rasteriser rounds both stems to nine and the difference is gone.
/// #110 answered it by reading the ratio at **512px**, where it converges on the outline's true
/// value — and that was the wrong target. A component is *thresholded ink at subtitle size*, and
/// thresholding at that size drops the partial columns at a glyph's edges: a real disc draws its
/// x-height letters 3 to 5 points narrower than the outline says, `o` by 5.0 and `s` by 3.6, while
/// its capitals lose only 1 to 3 because they are taller. Reading the reference at 512 and the
/// material at 40 compares two different measurements of the same quantity, which is #99's mistake
/// in another form.
///
/// Rasterising at 56 reproduces the material's own quantisation instead of correcting for it, and
/// across three discs it halves the disagreement — a mean gap of 1.59 points against 2.66 — while
/// *widening* the gap it exists to read: `l` and `I` are 2.5 points apart here and 0.8 apart in the
/// outlines. `docs/error-census.md` has the sweep and the per-character table.
///
/// It costs one extra rasterisation per character at generation time and nothing at all at runtime.
/// The shape vectors stay at [`RENDER_PX`]: a smaller render does not help *them*, because the grid
/// they letterbox onto is sixteen cells wide either way.
const ASPECT_PX: f32 = 56.0;

/// The aspect ratio of a character's ink.
///
/// Measured off the thresholded **ink** box, because that is the box a component is: a value taken
/// from the rasteriser's box, which includes every pixel with any coverage at all, would be a second
/// instance of the #99 mismatch, and that one carried a whole error class on its own.
///
/// Nothing about the line enters it, which is the property that decided the shape of this feature.
/// See [`InkAspect`].
fn aspect_for(font: &Font, ch: char) -> InkAspect {
    match ink_box(font, ch, ASPECT_PX) {
        Some(box_) => InkAspect::measure(box_.width, box_.height),
        None => InkAspect::UNKNOWN,
    }
}

/// The thresholded ink box of one character at one size.
fn ink_box(font: &Font, ch: char, px: f32) -> Option<Rect> {
    let (metrics, coverage) = font.rasterize(ch, px);
    let width = u32::try_from(metrics.width).ok()?;
    let height = u32::try_from(metrics.height).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let bits: Vec<bool> = coverage.iter().map(|c| *c >= INK).collect();
    ink_bounds(&BinaryMask::from_bits(width, height, &bits).ok()?)
}

/// Where a character stands in a line of text, from the font's own metrics.
///
/// This has to mean the same thing as `subtrackt_glyph::metrics`, which derives its anchors from a
/// rendered line's ink. So the unit here is the *ink* height of a capital H rather than a figure
/// from a font table: the runtime's cap height is the row the tall glyphs actually reach, and a
/// table value would include margins the pixels never show.
fn metrics_for(font: &Font, ch: char, cap_height: i32) -> LineMetrics {
    if cap_height <= 0 {
        return LineMetrics::UNKNOWN;
    }
    let metrics = font.metrics(ch, RENDER_PX);
    if metrics.height == 0 {
        return LineMetrics::UNKNOWN;
    }

    let height = i32::try_from(metrics.height).unwrap_or(0) * 100 / cap_height;
    // fontdue reports ymin as the offset of the bitmap's bottom from the baseline: negative for a
    // descender, positive for a mark floating clear of the baseline. The runtime measures downwards
    // from the baseline, so the sign flips.
    let descent = -metrics.ymin * 100 / cap_height;

    LineMetrics::new(u32::try_from(height).unwrap_or(0), descent)
}

/// Which way a character's diacritic leans, from an isolated render.
///
/// Runs the *shipped* `mark::slope` over the character's own connected components rather than
/// reimplementing the rule, so a reference entry and a decoded glyph are measured by the same code.
/// `group` is skipped because there is nothing to skip: a character rendered on its own is already
/// one glyph, and its components are exactly the parts `group` would hand over. (It could not be
/// run here anyway — a lone `é` has a blank row between its accent and its body, so `line_bands`
/// would band it as two lines and never attach the mark. See `docs/glyph-stability.md`.)
#[must_use]
pub fn mark_for(font: &Font, ch: char) -> MarkSlope {
    let (metrics, coverage) = font.rasterize(ch, RENDER_PX);
    let (Ok(width), Ok(height)) = (u32::try_from(metrics.width), u32::try_from(metrics.height))
    else {
        return MarkSlope::NONE;
    };
    if width == 0 || height == 0 {
        return MarkSlope::NONE;
    }
    let bits: Vec<bool> = coverage.iter().map(|c| *c >= INK).collect();
    let Ok(mask) = BinaryMask::from_bits(width, height, &bits) else {
        return MarkSlope::NONE;
    };
    let Ok(parts) = ccl::label(&mask, ComponentFilter::permissive()) else {
        return MarkSlope::NONE;
    };
    crate::mark::slope(&mask, &GroupedGlyph { parts, line: 0 })
}
/// One face of a typeface, as bytes plus the style it stands for.
///
/// Bytes rather than a path on purpose: reading files, reporting which one failed and deciding what
/// counts as a font directory are all the caller's business, and keeping them out of here is what
/// lets the CLI and xtask share this code while wording their errors differently.
#[derive(Debug, Clone, Copy)]
pub struct Face<'a> {
    /// The font file's contents.
    pub bytes: &'a [u8],
    /// Which cut of the typeface this is.
    pub style: Style,
}

/// A generated set, plus what could not be generated.
///
/// The second field is the point. A character with no outline is a fact about the font, and a
/// caller that cannot see which characters were dropped has no way to tell a font missing its
/// accents from one that rendered everything.
#[derive(Debug, Clone)]
pub struct Generated {
    /// The reference set, ready to encode.
    pub set: ReferenceSet,
    /// Characters the regular face could not render, in charset order.
    pub missing: Vec<char>,
    /// Faces whose capital `H` did not rasterise, so their entries carry no line metrics.
    pub without_cap_height: Vec<Style>,
}

/// Render every face into one reference set.
///
/// `grey` must match the pipeline's `grey_coverage` setting, for the reason [`vector_for`] gives.
///
/// # Errors
/// [`Error::Config`] if a face is not a usable font, or if nothing rendered at all.
pub fn generate(name: impl Into<String>, faces: &[Face<'_>], grey: bool) -> Result<Generated> {
    generate_under(name, faces, grey, &RENDERINGS)
}

/// As [`generate`], with the renderings chosen rather than defaulted.
///
/// # Errors
/// As [`generate`].
pub fn generate_under(
    name: impl Into<String>,
    faces: &[Face<'_>],
    grey: bool,
    renderings: &[Rendering],
) -> Result<Generated> {
    let mut entries = Vec::new();
    let mut missing = Vec::new();
    let mut without_cap_height = Vec::new();

    for face in faces {
        let font = Font::from_bytes(face.bytes, FontSettings::default())
            .map_err(|e| Error::Config(format!("not a usable font: {e}")))?;

        // The unit every metric is a fraction of, taken per face: an italic and an upright cut of
        // one typeface do not share a cap height exactly, and scaling one against the other's would
        // make every metric slightly wrong in a way nothing would report.
        let cap_height = i32::try_from(font.metrics('H', RENDER_PX).height).unwrap_or(0);
        if cap_height <= 0 {
            without_cap_height.push(face.style);
        }

        for ch in charset() {
            let vectors = vectors_under(&font, ch, grey, renderings);
            if vectors.is_empty() {
                if face.style == Style::Regular {
                    missing.push(ch);
                }
                continue;
            }
            // Metrics and mark come from the canonical render rather than from each entry's own
            // size, because both are *ratios* — a fraction of the face's cap height, and a
            // normalised second moment — and measuring them at 21px would quantise them for no
            // gain. Every entry for a character therefore carries the same pair, which is what
            // makes the extra entries purely a shape question.
            let metrics = metrics_for(&font, ch, cap_height);
            let mark = mark_for(&font, ch);
            let aspect = aspect_for(&font, ch);
            for features in vectors {
                entries.push(ReferenceEntry {
                    character: ch,
                    style: face.style,
                    features,
                    metrics,
                    mark,
                    aspect,
                });
            }
        }
    }

    if entries.is_empty() {
        return Err(Error::Config("font produced no glyphs at all".to_owned()));
    }

    Ok(Generated {
        set: ReferenceSet::new(name, entries),
        missing,
        without_cap_height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fonts to try, so the tests run unattended on a developer machine and on both CI runners.
    ///
    /// The same list the accuracy harness uses. Ubuntu runners ship `DejaVu` Sans and Windows
    /// runners ship Arial, so one of these always resolves; a machine with none of them fails
    /// loudly rather than skipping quietly, because a test that silently never runs anywhere is
    /// worse than no test at all.
    const CANDIDATES: [&str; 5] = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/Library/Fonts/Arial.ttf",
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
    ];

    fn a_font() -> Vec<u8> {
        for path in CANDIDATES {
            if let Ok(bytes) = std::fs::read(path) {
                return bytes;
            }
        }
        panic!("no font found; install DejaVu Sans or run where one of {CANDIDATES:?} exists");
    }

    #[test]
    fn a_reference_vector_is_the_runtime_normalisation_of_the_same_rasterisation() {
        // The property the whole module exists to hold. Rebuilt here from fontdue's coverage
        // through `vectorize` directly, rather than through `vector_for`, so this compares two
        // independent routes to the vector instead of comparing the code to itself. A reference
        // built through any other transform would produce distances that mean nothing, and nothing
        // downstream would report it.
        let bytes = a_font();
        let font = Font::from_bytes(bytes.as_slice(), FontSettings::default()).unwrap();

        for ch in ['A', 'g', '7', '\u{e9}'] {
            let (metrics, coverage) = font.rasterize(ch, RENDER_PX);
            let width = u32::try_from(metrics.width).unwrap();
            let height = u32::try_from(metrics.height).unwrap();
            let bits: Vec<bool> = coverage.iter().map(|c| *c >= INK).collect();
            let mask = BinaryMask::from_bits(width, height, &bits).unwrap();
            let expected =
                vectorize(&mask, Rect::new(0, 0, width, height), AspectPolicy::Letterbox).unwrap();

            assert_eq!(
                vector_for(&font, ch, false),
                Some(expected),
                "{ch:?} went through a different transform than the runtime's"
            );
        }
    }

    #[test]
    fn every_face_contributes_its_own_entry_for_a_character() {
        // #66's reason for the style byte: one set holds several cuts at once, and a track that
        // changes style mid-film is read by whichever is closer to the ink. Two entries for `A`
        // carrying different styles is what makes that possible.
        let bytes = a_font();
        let faces = [
            Face { bytes: &bytes, style: Style::Regular },
            Face { bytes: &bytes, style: Style::Italic },
        ];
        let generated = generate("two-faces", &faces, false).unwrap();

        // Both cuts present, in face order. How *many* entries each face contributes is
        // `RENDERINGS`' business and varies by character — see
        // `a_second_entry_appears_only_where_the_box_actually_changes_the_vector` — so this asks
        // which styles appear rather than how many entries there are.
        let styles: Vec<Style> = generated
            .set
            .entries()
            .iter()
            .filter(|e| e.character == 'A')
            .map(|e| e.style)
            .collect();
        assert!(
            styles.contains(&Style::Regular) && styles.contains(&Style::Italic),
            "{styles:?}"
        );
        assert!(
            styles.iter().position(|s| *s == Style::Italic)
                > styles.iter().position(|s| *s == Style::Regular),
            "faces contribute in the order they were given: {styles:?}"
        );
    }

    #[test]
    fn a_character_the_font_cannot_render_is_reported_rather_than_dropped_silently() {
        // A font missing its accents and a font that rendered everything must not look the same to
        // a caller. Space has no outline in any font, so it is the one character guaranteed to
        // exercise this.
        let bytes = a_font();
        let generated =
            generate("one", &[Face { bytes: &bytes, style: Style::Regular }], false).unwrap();

        assert!(!generated.set.is_empty(), "a usable font should produce entries");
        for ch in &generated.missing {
            assert!(
                charset().contains(ch),
                "{ch:?} was reported missing but is not in the charset"
            );
        }
    }

    #[test]
    fn bytes_that_are_not_a_font_are_rejected_rather_than_producing_an_empty_set() {
        // The failure that matters. An empty reference set is indistinguishable from one that
        // simply matches nothing, so it has to be an error at the point the font is read.
        let junk = vec![0u8; 512];
        let error = generate("junk", &[Face { bytes: &junk, style: Style::Regular }], false);
        assert!(matches!(error, Err(Error::Config(_))), "got {error:?}");
    }

    #[test]
    fn no_faces_at_all_is_an_error_rather_than_an_empty_set() {
        let error = generate("nothing", &[], false);
        assert!(matches!(error, Err(Error::Config(_))), "got {error:?}");
    }
    #[test]
    fn the_crop_box_costs_a_small_glyph_far_more_than_a_large_one() {
        // The mechanism behind #99, pinned, because the whole of `RENDERINGS` rests on it and
        // nothing else in the tree would notice if it stopped being true.
        //
        // Both boxes come from one rasterisation at one threshold. The rasteriser's box includes
        // every pixel with any coverage at all; the ink box is what survived the threshold, which
        // is what the runtime letterboxes because that is what a connected component's bounds are.
        // The difference between them is roughly a *pixel*, so its share of the box grows as the
        // glyph shrinks -- 1.5% of a 68px capital, 15% of a 13px full stop.
        let bytes = a_font();
        let font = Font::from_bytes(bytes.as_slice(), FontSettings::default()).unwrap();
        let raster = |ch| render(&font, ch, RENDERINGS[0], false).unwrap();
        let ink = |ch| render(&font, ch, RENDERINGS[1], false).unwrap();

        let large = raster('M').distance(&ink('M'));
        let small = raster('.').distance(&ink('.'));
        assert!(
            small > large,
            "a full stop should be more sensitive to the box than an M: {small} against {large}"
        );
        // And the small one clears the ambiguity margin several times over, which is why one box
        // could not cover both and why two entries are carried rather than one being chosen.
        assert!(
            small > crate::matcher::MatchThresholds::default().ambiguity_margin() * 2,
            "the two boxes put a full stop only {small} cells apart"
        );
    }

    #[test]
    fn a_second_entry_appears_only_where_the_box_actually_changes_the_vector() {
        // The extra entries are not a flat doubling, and it is the same mechanism that decides
        // which characters get one: where the two boxes normalise to the same vector, the second is
        // dropped, because a duplicate entry is scan cost for no separation. A large glyph collapses
        // to one entry and a full stop keeps both, which is the finding stated as set contents.
        let bytes = a_font();
        let generated =
            generate("two", &[Face { bytes: &bytes, style: Style::Regular }], false).unwrap();
        let count = |ch: char| {
            generated
                .set
                .entries()
                .iter()
                .filter(|e| e.character == ch)
                .count()
        };
        assert_eq!(count('.'), RENDERINGS.len(), "a full stop needs both boxes");
        assert!(
            count('M') <= count('.'),
            "an M cannot need more entries than a full stop"
        );
        for ch in charset() {
            assert!(count(ch) <= RENDERINGS.len(), "{ch:?} has more entries than renderings");
        }
    }

    #[test]
    fn two_renderings_that_normalise_alike_contribute_one_entry_rather_than_two() {
        // The dedupe itself, asked directly rather than through whichever character happens to
        // exercise it in whichever font the machine has. A duplicate entry is scan cost for no
        // separation, and nothing downstream would report it.
        let bytes = a_font();
        let font = Font::from_bytes(bytes.as_slice(), FontSettings::default()).unwrap();
        let twice = [RENDERINGS[0], RENDERINGS[0]];
        assert_eq!(vectors_under(&font, 'M', false, &twice).len(), 1);
        assert_eq!(vectors_under(&font, 'M', false, &RENDERINGS[..1]).len(), 1);
    }

    #[test]
    fn every_entry_for_one_character_carries_the_same_metrics_and_mark() {
        // The extra entries are a *shape* question and nothing else. Measuring metrics per
        // rendering would quantise a ratio for no gain and would make the two entries disagree
        // about where the character stands in its line, which the matcher would then price.
        let bytes = a_font();
        let generated =
            generate("two", &[Face { bytes: &bytes, style: Style::Regular }], false).unwrap();
        for ch in ['.', 'e'] {
            let entries: Vec<_> = generated
                .set
                .entries()
                .iter()
                .filter(|e| e.character == ch)
                .collect();
            assert!(entries.len() > 1, "{ch:?} should have several entries");
            for entry in &entries[1..] {
                assert_eq!(entry.metrics, entries[0].metrics, "{ch:?}");
                assert_eq!(entry.mark, entries[0].mark, "{ch:?}");
            }
        }
    }
}
