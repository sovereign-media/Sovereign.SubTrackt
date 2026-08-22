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
use subtrackt_core::{Error, FeatureVector, LineMetrics, MarkSlope, Rect, Result};

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
    chars
}

/// Rasterise one character and normalise it exactly as the runtime would.
///
/// `grey` must match the pipeline's `grey_coverage` setting. A reference built through a different
/// normalisation than the runtime uses would be compared against a subtly different transform, and
/// every distance it produced would be meaningless.
#[must_use]
pub fn vector_for(font: &Font, ch: char, grey: bool) -> Option<FeatureVector> {
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
    let Ok(mask) = BinaryMask::from_bits(width, height, bits) else {
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
            match vector_for(&font, ch, grey) {
                Some(features) => entries.push(ReferenceEntry {
                    character: ch,
                    style: face.style,
                    features,
                    metrics: metrics_for(&font, ch, cap_height),
                    mark: mark_for(&font, ch),
                }),
                None if face.style == Style::Regular => missing.push(ch),
                None => {}
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
            let mask = BinaryMask::from_bits(width, height, bits).unwrap();
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

        let styles: Vec<Style> = generated
            .set
            .entries()
            .iter()
            .filter(|e| e.character == 'A')
            .map(|e| e.style)
            .collect();
        assert_eq!(styles, vec![Style::Regular, Style::Italic]);
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
}
