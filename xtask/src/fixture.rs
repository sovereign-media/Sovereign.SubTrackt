//! Building PGS fixtures with known ground truth.
//!
//! Fixtures are *generated*, not clipped from real subtitles, and that solves two problems at once.
//! The ground truth is exact rather than hand-transcribed, and nothing copyrighted is redistributed
//! — the text is ours and the bitmap is our own rendering of it.
//!
//! The rendering deliberately imitates how a real subtitle is authored: anti-aliased glyphs, a
//! white fill, and a dark outline around it. That matters more than it sounds. #14 measured that a
//! one-pixel shift in where the binarization edge falls costs as much as character identity, so a
//! fixture rendered as clean solid text would exercise a much easier problem than the real one and
//! would flatter every stage downstream of it.

// Glyph metrics and canvas coordinates here are all small positive integers bounded by the
// rasterised glyph sizes, so the numeric-cast lints have nothing to catch.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use fontdue::{Font, FontSettings};
use subtrackt_decode::pgs::rle;

/// Palette indices, matching how subtitles are conventionally authored.
const TRANSPARENT: u8 = 0;
const FILL: u8 = 1;
const OUTLINE: u8 = 2;

/// Coverage above which a rasterised pixel is glyph fill.
const FILL_INK: u8 = 160;
/// Coverage above which a pixel is at least outline; below it the pixel is background.
const OUTLINE_INK: u8 = 24;

/// Margin around the text block, so glyphs never touch the object edge.
const MARGIN: u32 = 8;

/// A rendered line of text as a palette-indexed bitmap.
struct Rendered {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Draw one line of text, imitating a real subtitle: anti-aliased fill inside a dark outline.
fn render_line(font: &Font, text: &str, px: f32) -> anyhow::Result<Rendered> {
    // Lay the glyphs out first so the canvas can be sized to them.
    let mut placed = Vec::new();
    let mut pen = 0i32;
    let (mut top, mut bottom) = (i32::MAX, i32::MIN);

    for ch in text.chars() {
        let (metrics, coverage) = font.rasterize(ch, px);
        if metrics.width > 0 && metrics.height > 0 {
            let x = pen + metrics.xmin;
            let y = -metrics.ymin - i32::try_from(metrics.height).unwrap_or(0);
            top = top.min(y);
            bottom = bottom.max(y + i32::try_from(metrics.height).unwrap_or(0));
            placed.push((x, y, metrics.width, metrics.height, coverage));
        }
        pen += metrics
            .advance_width
            .round()
            .clamp(0.0, f32::from(i16::MAX)) as i32;
    }
    if placed.is_empty() {
        bail!("{text:?} rendered to nothing");
    }

    let width = u32::try_from(pen).unwrap_or(0) + MARGIN * 2;
    let height = u32::try_from(bottom - top).unwrap_or(0) + MARGIN * 2;
    let mut coverage_plane = vec![0u8; (width * height) as usize];

    for (x, y, gw, gh, coverage) in placed {
        for row in 0..gh {
            for col in 0..gw {
                let px_x = x + i32::try_from(col).unwrap_or(0) + i32::try_from(MARGIN).unwrap_or(0);
                let px_y =
                    y - top + i32::try_from(row).unwrap_or(0) + i32::try_from(MARGIN).unwrap_or(0);
                if px_x < 0 || px_y < 0 {
                    continue;
                }
                let (px_x, px_y) = (px_x as u32, px_y as u32);
                if px_x >= width || px_y >= height {
                    continue;
                }
                let at = (px_y * width + px_x) as usize;
                let value = coverage[row * gw + col];
                coverage_plane[at] = coverage_plane[at].max(value);
            }
        }
    }

    // Grow the ink by one pixel to stand in for the outline a real subtitle carries.
    let mut pixels = vec![TRANSPARENT; coverage_plane.len()];
    for y in 0..height {
        for x in 0..width {
            let at = (y * width + x) as usize;
            let here = coverage_plane[at];
            if here >= FILL_INK {
                pixels[at] = FILL;
                continue;
            }

            let near_ink = [
                (-1i32, 0i32),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, 1),
                (-1, 1),
                (1, -1),
            ]
            .iter()
            .any(|(dx, dy)| {
                let nx = i64::from(x) + i64::from(*dx);
                let ny = i64::from(y) + i64::from(*dy);
                if nx < 0 || ny < 0 || nx >= i64::from(width) || ny >= i64::from(height) {
                    return false;
                }
                coverage_plane[(ny as u32 * width + nx as u32) as usize] >= FILL_INK
            });

            if here >= OUTLINE_INK || near_ink {
                pixels[at] = OUTLINE;
            }
        }
    }

    Ok(Rendered { width, height, pixels })
}

/// Stack rendered lines into one bitmap, centred, with a gap between them.
fn stack(lines: &[Rendered], gap: u32) -> Rendered {
    let width = lines.iter().map(|l| l.width).max().unwrap_or(0);
    let height = lines.iter().map(|l| l.height).sum::<u32>() + gap * (lines.len() as u32 - 1);
    let mut pixels = vec![TRANSPARENT; (width * height) as usize];

    let mut y0 = 0;
    for line in lines {
        let x0 = (width - line.width) / 2;
        for y in 0..line.height {
            for x in 0..line.width {
                let value = line.pixels[(y * line.width + x) as usize];
                if value != TRANSPARENT {
                    pixels[((y0 + y) * width + x0 + x) as usize] = value;
                }
            }
        }
        y0 += line.height + gap;
    }
    Rendered { width, height, pixels }
}

/// One `.sup` segment: `PG` magic, PTS, DTS, type, length, body.
fn sup_segment(kind: u8, pts: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::from(*b"PG");
    out.extend_from_slice(&pts.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.push(kind);
    out.extend_from_slice(&u16::try_from(body.len()).unwrap_or(u16::MAX).to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// A presentation composition placing one object, or clearing the screen when `place` is `None`.
fn composition(plane: (u32, u32), at: Option<(u32, u32)>) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&u16::try_from(plane.0).unwrap_or(0).to_be_bytes());
    b.extend_from_slice(&u16::try_from(plane.1).unwrap_or(0).to_be_bytes());
    b.push(0x10);
    b.extend_from_slice(&0u16.to_be_bytes());
    b.push(0x80);
    b.push(0x00);
    b.push(0x00);
    b.push(u8::from(at.is_some()));
    if let Some((x, y)) = at {
        b.extend_from_slice(&OBJECT_ID.to_be_bytes());
        b.push(0);
        b.push(0);
        b.extend_from_slice(&u16::try_from(x).unwrap_or(0).to_be_bytes());
        b.extend_from_slice(&u16::try_from(y).unwrap_or(0).to_be_bytes());
    }
    b
}

/// A palette: opaque white fill, opaque near-black outline.
fn palette() -> Vec<u8> {
    let mut b = vec![0, 0];
    b.extend_from_slice(&[FILL, 235, 128, 128, 255]);
    b.extend_from_slice(&[OUTLINE, 16, 128, 128, 255]);
    b
}

/// Object id used throughout; the composition and the definition must agree on it.
const OBJECT_ID: u16 = 0;

/// A complete object definition.
fn object(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let data = rle::encode(pixels, width, height);
    let mut b = Vec::new();
    b.extend_from_slice(&OBJECT_ID.to_be_bytes());
    b.push(0);
    b.push(0xC0);
    b.extend_from_slice(&u32::try_from(data.len() + 4).unwrap_or(0).to_be_bytes()[1..]);
    b.extend_from_slice(&u16::try_from(width).unwrap_or(0).to_be_bytes());
    b.extend_from_slice(&u16::try_from(height).unwrap_or(0).to_be_bytes());
    b.extend_from_slice(&data);
    b
}

/// Rendering sizes a repeated fixture cycles through, as multiples of the nominal size.
///
/// A real stream is authored once, so its characters do not change typeface or weight — but the
/// same character still reaches the segmenter as several slightly different rasterisations, which
/// is why a Blu-ray yields a few hundred distinct shapes for a few dozen characters. These
/// multiples reproduce that: same font, same everything else, a few percent of size either way.
const VARIED_SCALES: [f32; 5] = [1.0, 0.94, 1.06, 0.97, 1.03];

/// Build a `.sup` from cues, each a list of text lines rendered at its own size.
pub(crate) fn build_sup(
    font: &Font,
    cues: &[(Vec<String>, f32)],
    plane: (u32, u32),
) -> anyhow::Result<Vec<u8>> {
    let mut file = Vec::new();

    for (index, (lines, px)) in cues.iter().enumerate() {
        let px = *px;
        let rendered: Vec<Rendered> = lines
            .iter()
            .map(|line| render_line(font, line, px))
            .collect::<anyhow::Result<_>>()?;
        let image = stack(&rendered, px as u32 / 4);

        let start = 90_000 * (1 + index as u32 * 3);
        let end = start + 90_000 * 2;
        let x = (plane.0.saturating_sub(image.width)) / 2;
        let y = plane.1.saturating_sub(image.height + 60);

        file.extend_from_slice(&sup_segment(0x16, start, &composition(plane, Some((x, y)))));
        file.extend_from_slice(&sup_segment(0x14, start, &palette()));
        file.extend_from_slice(&sup_segment(
            0x15,
            start,
            &object(image.width, image.height, &image.pixels),
        ));
        file.extend_from_slice(&sup_segment(0x80, start, &[]));

        file.extend_from_slice(&sup_segment(0x16, end, &composition(plane, None)));
        file.extend_from_slice(&sup_segment(0x80, end, &[]));
    }
    Ok(file)
}

/// The lines every generated fixture carries.
///
/// Written for this purpose, not taken from any film. Between them they cover both cases, digits,
/// the ambiguous pairs #12 cares about, accented letters, a two-speaker dash and a colon — the
/// punctuation case #6 is built around.
///
/// The `yellow line` cue is aimed squarely at post-correction and carries both halves of it.
/// `yellow` and `line` are what it should fix when the matcher reads an `l` as an `I`; `Iowa` is
/// what it must not touch, because the evidence for rewriting a word-initial capital in an
/// otherwise-lowercase word is identical in both, and only one of them would be a correction. A
/// measurement of a corrector over a corpus with nothing to damage measures half the question.
///
/// The last three cues are #48's. One line of accented text answers whether a diacritic is
/// *grouped*; it cannot answer whether the direction of one is read, because that needs both
/// members of a pair — an `à` against an `á` — in the same material. Between them these carry
/// `à`/`á`, `è`/`é`, `ò`/`ó` and `ù`/`ú`: four of the sixteen accent-direction pairs the matcher
/// calls ambiguous.
///
/// `plaît` and `maître` carry `î`, which is #58's: the circumflex is three times the width of the
/// stem under it, so until the overlap rule was denominated by the narrower of the two it never
/// grouped at all. `naïve` above carries the half of that issue still open — a diaeresis straddles
/// its stem, so neither dot overlaps it and only their union would.
///
/// The all-capitals line is #57's. An accent over a capital sits above every letterform this
/// charset can spell, so the row beneath it is blank across the line and it used to band on its own
/// — `À` segmenting as a bare `A` plus a floating grave. #48 deliberately kept accented capitals
/// out of the fixture while that was true, because including one would have scored a segmentation
/// gap inside a figure read as a matching result. Now that `group::text_lines` merges a band of
/// nothing but marks into the line beneath it, the line belongs here and is what shows the fix.
fn default_cues() -> Vec<Vec<String>> {
    [
        vec!["The quick brown fox jumps"],
        vec!["over the lazy dog."],
        vec!["- Is it 1 or l?", "- Neither: it is I."],
        vec!["Café, naïve, jalapeño."],
        vec!["0123456789 O o I l 1"],
        vec!["Follow the yellow line", "to Iowa in 2015."],
        vec!["Où est-il? Là, à côté.", "S'il vous plaît, maître."],
        vec!["Está más allá: adiós, Perú."],
        vec!["Però è così che sarà.", "ÉTAIT-CE ÀPRE? ÎLE."],
    ]
    .iter()
    .map(|lines| lines.iter().map(|s| (*s).to_owned()).collect())
    .collect()
}

/// Generate a fixture: a `.sup` and the ground truth beside it.
///
/// # Errors
/// Propagates font loading and file writing failures.
pub fn make(args: &[String]) -> anyhow::Result<()> {
    let font_path = args
        .first()
        .context("usage: make-fixture <font.ttf> <out-dir> [--px N]")?;
    let out_dir: PathBuf = args.get(1).context("missing output directory")?.into();

    let px: f32 = match args.iter().position(|a| a == "--px") {
        Some(at) => args.get(at + 1).context("--px needs a value")?.parse()?,
        None => 42.0,
    };

    let bytes =
        std::fs::read(Path::new(font_path)).with_context(|| format!("reading {font_path}"))?;
    let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
        .map_err(|e| anyhow::anyhow!("{font_path}: {e}"))?;

    std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    // Repeating the cue set is what makes a fixture resemble a stream rather than a page: the same
    // characters recur, each time rasterised slightly differently, which is the variation
    // clustering exists to absorb. Without it a fixture can only show clustering's cost.
    let repeats: usize = match args.iter().position(|a| a == "--repeat") {
        Some(at) => args
            .get(at + 1)
            .context("--repeat needs a value")?
            .parse()?,
        None => 1,
    };

    let base = default_cues();
    let mut cues: Vec<(Vec<String>, f32)> = Vec::new();
    for pass in 0..repeats.max(1) {
        let scale = VARIED_SCALES[pass % VARIED_SCALES.len()];
        for lines in &base {
            cues.push((lines.clone(), px * scale));
        }
    }

    let sup = build_sup(&font, &cues, (1920, 1080))?;

    let sup_path = out_dir.join("synthetic.sup");
    let truth_path = out_dir.join("synthetic.txt");
    let truth: String = cues
        .iter()
        .map(|(lines, _)| lines.join("\n"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    std::fs::write(&sup_path, &sup).with_context(|| format!("writing {}", sup_path.display()))?;
    std::fs::write(&truth_path, &truth)
        .with_context(|| format!("writing {}", truth_path.display()))?;

    eprintln!(
        "{} cues, {} bytes -> {}\nground truth -> {}",
        cues.len(),
        sup.len(),
        sup_path.display(),
        truth_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacking_centres_lines_and_leaves_a_gap() {
        let wide = Rendered { width: 6, height: 1, pixels: vec![FILL; 6] };
        let narrow = Rendered { width: 2, height: 1, pixels: vec![FILL; 2] };
        let stacked = stack(&[wide, narrow], 1);

        assert_eq!(stacked.width, 6);
        assert_eq!(stacked.height, 3, "two 1px lines plus a 1px gap");
        assert_eq!(stacked.pixels[6..12], vec![TRANSPARENT; 6], "the gap row is empty");
        assert_eq!(stacked.pixels[14], FILL, "the narrow line is centred");
        assert_eq!(stacked.pixels[12], TRANSPARENT);
    }

    #[test]
    fn the_composition_and_the_object_definition_agree_on_the_id() {
        // They did not, and the decoder rightly refused the fixture: a composition referencing an
        // object that was never defined is exactly the malformed case it guards against.
        let c = composition((1920, 1080), Some((0, 0)));
        assert_eq!(u16::from_be_bytes([c[11], c[12]]), OBJECT_ID);
        let o = object(2, 1, &[FILL, FILL]);
        assert_eq!(u16::from_be_bytes([o[0], o[1]]), OBJECT_ID);
    }

    #[test]
    fn a_composition_with_no_object_is_a_clear() {
        assert_eq!(*composition((1920, 1080), None).last().unwrap(), 0);
        assert_eq!(composition((1920, 1080), Some((10, 20))).len(), 11 + 8);
    }

    #[test]
    fn the_palette_defines_an_opaque_fill_and_outline() {
        let p = palette();
        assert_eq!(p.len(), 2 + 5 * 2);
        assert_eq!(p[2], FILL);
        assert_eq!(p[3], 235, "fill is bright");
        assert_eq!(p[7], OUTLINE);
        assert_eq!(p[8], 16, "outline is dark, so luma thresholding separates them");
    }

    #[test]
    fn ground_truth_covers_the_cases_the_pipeline_is_built_around() {
        let all = default_cues().concat().join(" ");
        assert!(all.contains(": "), "a colon, which #6 must not merge into a letter");
        assert!(all.contains("- "), "a speaker dash, which #11 must keep spaced");
        assert!(all.contains('1') && all.contains('l'), "the ambiguous pair from #12");
        assert!(all.contains('\u{e9}'), "an accented letter, which #6 must group");
        for (grave, acute) in [
            ('\u{e0}', '\u{e1}'),
            ('\u{e8}', '\u{e9}'),
            ('\u{f2}', '\u{f3}'),
            ('\u{f9}', '\u{fa}'),
        ] {
            assert!(
                all.contains(grave) && all.contains(acute),
                "both directions of {grave}/{acute}, or #48's feature has nothing to be measured on"
            );
        }
        assert!(
            all.chars().any(|c| ('\u{c0}'..='\u{dd}').contains(&c)),
            "an accented capital, which is #57's. Until a band of nothing but marks was merged \
             into the line beneath it, `À` segmented as a bare `A` plus a floating grave, and this \
             line was deliberately absent so the gap would not be scored as a matching error"
        );
        assert!(
            all.contains("yellow"),
            "an ambiguous letter with clear letters either side, which #12 can resolve"
        );
        assert!(
            all.contains("Iowa"),
            "and a proper noun whose capital #12 must leave alone, or the corrector is measured \
             only on what it can gain"
        );
    }
}
