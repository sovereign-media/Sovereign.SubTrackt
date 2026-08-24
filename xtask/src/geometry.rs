//! What a character the release names actually looked like on the disc.
//!
//! [#109](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/109). `l` read as `I` is
//! two thirds of the errors left on a real Blu-ray, and #10 measured the pair at distance **zero**:
//! the same 256 bits, the same height, no mark. Every lever the pipeline has is therefore blind to
//! it by construction, and the issue's first question is whether any evidence exists *at all* —
//! specifically whether the space a character occupies, which its own ink cannot show, separates
//! the two.
//!
//! That question needs something no instrument here had: a **label**. `xtask unread` describes
//! glyphs the matcher would not call, and `xtask srt-score` counts characters the release disagrees
//! with, but neither can point at a glyph and say "the release says this one was an `l`". This can,
//! by aligning the read text against the release cue by cue and carrying the alignment back to the
//! glyph that produced each character — [`disc::trace`] is the same traceback the confusion census
//! runs, read for its matches instead of its errors.
//!
//! The caveat that applies to every figure here is the one `disc.rs` opens with: a release subtitle
//! is an independent transcript, not ground truth. A label is wrong wherever the release itself is
//! wrong. That is tolerable for a *distribution* over hundreds of glyphs and would not be for any
//! claim about a single one, which is why nothing below reports individual glyphs.

// Every ratio here divides one count of glyphs by another. A feature film holds tens of thousands
// of them, far inside the 2^53 an f64 counts exactly, so the precision-loss lint has nothing to
// warn about here — the same reasoning `feature.rs` records for the same allow.
#![allow(clippy::cast_precision_loss)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context as _;
use fontdue::{Font, FontSettings};
use subtrackt_glyph::ReferenceSet;
use subtrackt_glyph::matcher::{HammingMatcher, MatchThresholds};
use subtrackt_glyph::reference::Style;

use crate::disc;

/// One glyph, the character the release says it was, and the geometry of its surroundings.
struct Labelled {
    /// What the release has at this position.
    want: char,
    /// What the matcher read it as, before post-correction. `None` if it read nothing.
    got: Option<char>,
    /// Ink width in pixels, which is the only thing the letterboxed vector sees of it.
    width: u32,
    /// Ink height in pixels.
    height: u32,
    /// The cap height of the line it stood on, in pixels.
    ///
    /// Every ratio here is against this rather than against a constant, for the reason `CLAUDE.md`
    /// gives: the same title ships at several resolutions and an absolute pixel threshold is a bug
    /// waiting for a different disc.
    cap: f64,
    /// Ink pixels, so a stem's mean width can be read at finer than whole-pixel resolution.
    ink: Option<u32>,
    /// Left edge to the *next* glyph's left edge — the advance the typeface gave this character.
    ///
    /// `None` unless the next glyph on the same line is the very next character in the release
    /// text, because an advance measured across a word space is the space's width and not the
    /// character's.
    advance: Option<u32>,
    /// This glyph's right edge to the next glyph's left edge, under the same condition.
    gap: Option<u32>,
    /// Whether the release marked the cue italic.
    italic: bool,
    /// Which cue and line it stood on, so a glyph on the wrong side of a threshold can be found.
    at: (usize, usize),
}

/// Measure the geometry of every glyph the release can name, and report it for one pair.
///
/// # Errors
/// Fails if the media, the reference set or the release subtitle cannot be read, or if extraction
/// fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: glyph-geometry <media> <reference.subtref> <release.srt> [--pair lI]")?
        .into();
    let set: PathBuf = args.get(1).context("missing the reference set")?.into();
    let release = args.get(2).context("missing the release subtitle")?;
    let pair: Vec<char> = match args.iter().position(|a| a == "--pair") {
        Some(at) => args
            .get(at + 1)
            .context("--pair needs two characters")?
            .chars()
            .collect(),
        None => vec!['l', 'I'],
    };

    let reference = crate::util::load_reference(&set)?;
    // Placeholder rather than the default gate, for the same reason `xtask unread` uses it: a
    // policy that refuses the track would leave nothing to measure. Post-correction stays off —
    // the question is what the *matcher* saw, and the corrector's answer is the thing under test.
    let (outcome, survey) = disc::paired_passes(&media, reference.clone())?;

    let matcher = HammingMatcher::new(reference.clone(), MatchThresholds::default())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let want_cues = disc::read(release)?;

    let labelled = label(&outcome, &survey, &matcher, &want_cues);
    report(&labelled, &pair, survey.glyphs.len());
    // The italic act is set in its own face, and a slanted stem covers more columns than an upright
    // one of the same weight — so the same rule has to be asked against the italic outlines or the
    // answer is about the wrong typeface.
    if let Some(at) = args.iter().position(|a| a == "--font") {
        let path = args.get(at + 1).context("--font needs a path")?;
        predicted(&labelled, &pair, &face(path)?, false)?;
    }
    if let Some(at) = args.iter().position(|a| a == "--italic") {
        let path = args.get(at + 1).context("--italic needs a path")?;
        predicted(&labelled, &pair, &face(path)?, true)?;
    }
    if let Some(at) = args.iter().position(|a| a == "--font") {
        let path = args.get(at + 1).context("--font needs a path")?;
        calibrate(&labelled, path, &reference)?;
    }
    Ok(())
}

/// Load one face.
fn face(path: &str) -> anyhow::Result<Font> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {path}"))?;
    Font::from_bytes(bytes, FontSettings::default()).map_err(|e| anyhow::anyhow!("{path}: {e}"))
}

/// How wide the typeface says each character is, beside how wide the disc drew it.
///
/// The decisive question for any width term, and not answerable from the disc alone. A rule that
/// separates `l` from `I` has to get its expectation from the *reference side*, so what matters is
/// not only that the disc's two populations differ but that the difference sits where the font
/// predicts — otherwise the separation is a fact about one rendering and a threshold fitted to it
/// would be the absolute-pixel constant `CLAUDE.md` forbids.
///
/// Two font-side figures rather than one, because the reference set and the runtime do not agree on
/// which box a glyph is: `font::metrics_for` scales the rasteriser's box, which includes every
/// pixel with any coverage at all, while a component is the ink that survived thresholding. #99
/// found that difference carried a whole error class on its own.
fn calibrate(labelled: &[Labelled], path: &str, reference: &ReferenceSet) -> anyhow::Result<()> {
    let font = face(path)?;
    let upright: Vec<&Labelled> = labelled.iter().filter(|g| !g.italic).collect();
    let drawn = drawn_by_the_disc(&upright, &font);

    size_sweep(&drawn, &font);

    println!(
        "
  aspect ratio per character: what the disc drew, and what each size predicts"
    );
    print!("  {:>4} {:>7} {:>8} {:>8}", "char", "n", "disc", "set");
    for px in SHOWN {
        print!(" {:>8}", format!("{px:.0}px"));
    }
    println!();
    for (ch, sample) in &drawn {
        let carried = reference
            .entries()
            .iter()
            .find(|e| e.character == *ch && e.style == Style::Regular)
            .filter(|e| e.aspect.known)
            .map(|e| f64::from(e.aspect.permille) / 10.0);
        print!(
            "  {:>4} {:>7} {:>8.2} {:>8}",
            ch,
            sample.values.len(),
            sample.mean(),
            carried.map_or_else(|| "-".to_owned(), |w| format!("{w:.2}"))
        );
        for px in SHOWN {
            print!(
                " {:>8}",
                aspect_at(&font, *ch, px)
                    .map_or_else(|| "-".to_owned(), |a| format!("{:+.1}", a - sample.mean()))
            );
        }
        println!();
    }
    println!("  the size columns are the font's prediction minus what the disc drew");
    Ok(())
}

/// Every character the disc drew often enough to have a mean worth comparing against.
fn drawn_by_the_disc(upright: &[&Labelled], font: &Font) -> Vec<(char, Sample)> {
    let mut characters: Vec<char> = upright.iter().map(|g| g.want).collect();
    characters.sort_unstable();
    characters.dedup();
    characters
        .into_iter()
        .map(|ch| {
            let sample = Sample::new(values(upright, ch, |g| {
                (g.height > 0).then(|| f64::from(g.width) * 100.0 / f64::from(g.height))
            }));
            (ch, sample)
        })
        .filter(|(_, sample)| sample.values.len() >= MIN_SAMPLE)
        // A character the font draws no outline for is not a calibration failure, it is a space —
        // and the smallest size in the sweep has to draw every one of the rest, or the rows below
        // would be comparing different alphabets to each other.
        .filter(|(ch, _)| {
            !ch.is_whitespace() && SIZES.iter().all(|px| aspect_at(font, *ch, *px).is_some())
        })
        .collect()
}

/// The aspect ratio of one character's ink at one render size, as a percentage.
fn aspect_at(font: &Font, ch: char, px: f32) -> Option<f64> {
    let (width, height) = ink_box(font, ch, px)?;
    (height > 0).then(|| f64::from(width) * 100.0 / f64::from(height))
}

/// Sizes the per-character table prints, chosen from what the sweep above finds.
const SHOWN: [f32; 4] = [512.0, 96.0, 56.0, 44.0];

/// Sizes the reference side is asked to predict the disc from.
///
/// #110 reads the ratio at 512px, where it has converged on the outline's true value. #113 asks
/// whether that is the right thing to converge on: the runtime measures a ratio between two
/// *integer* pixel counts, and a 33-pixel `s` that loses one column to rounding is 4% narrower than
/// its outline. If the disagreement is quantisation then a font rasterised near the material's own
/// size reproduces it, and one of these rows will be much better than the others.
const SIZES: [f32; 12] = [
    512.0, 256.0, 128.0, 96.0, 72.0, 56.0, 48.0, 44.0, 40.0, 36.0, 33.0, 30.0,
];

/// The pairs an aspect ratio is asked to decide, and the only ones it is asked to decide alone.
///
/// A letterboxed vector cannot separate a letter from its own capital — that is what #37's line
/// metrics are for — and it cannot separate `l` from `I` at all. These are the pairs where the two
/// members share a shape and the term is therefore load-bearing. Punctuation is deliberately absent:
/// `!`, `'`, `,` and `"` have ratios as crowded as any case pair and shapes that separate them
/// outright, so counting them would measure a rule nothing proposes.
const CONFUSABLE: [(char, char); 9] = [
    ('l', 'I'),
    ('c', 'C'),
    ('o', 'O'),
    ('s', 'S'),
    ('u', 'U'),
    ('v', 'V'),
    ('w', 'W'),
    ('x', 'X'),
    ('y', 'Y'),
];

/// How well the font predicts the disc, size by size.
///
/// Two statistics, because they answer different halves. The **mean gap** says how close the two
/// sides are on average, weighted by how often the disc draws each character, and is what a distance
/// term pays on every glyph. The **pairs** column is what decides #113: for each member of a
/// confusable pair the disc drew, whether its measured ratio sits nearer its own font value than
/// nearer its partner's. `s` → `S` is exactly one of those going the wrong way.
fn size_sweep(drawn: &[(char, Sample)], font: &Font) {
    println!("\n  how well the font predicts the disc, by the size it is rasterised at (#113)");
    println!(
        "  {:>8} {:>10} {:>8} {:>8}   which way the wrong ones point",
        "px", "mean gap", "pairs", "glyphs"
    );

    for px in SIZES {
        let (mut total, mut glyphs) = (0.0f64, 0usize);
        for (ch, sample) in drawn {
            let Some(own) = aspect_at(font, *ch, px) else {
                continue;
            };
            total += (sample.mean() - own).abs() * sample.values.len() as f64;
            glyphs += sample.values.len();
        }

        let (mut right, mut counted, mut wrong_glyphs) = (0usize, 0usize, 0usize);
        let mut wrong: Vec<String> = Vec::new();
        for (a, b) in CONFUSABLE {
            for (member, partner) in [(a, b), (b, a)] {
                let Some((_, sample)) = drawn.iter().find(|(c, _)| *c == member) else {
                    continue;
                };
                let (Some(own), Some(other)) =
                    (aspect_at(font, member, px), aspect_at(font, partner, px))
                else {
                    continue;
                };
                counted += 1;
                if (sample.mean() - own).abs() <= (sample.mean() - other).abs() {
                    right += 1;
                } else {
                    wrong_glyphs += sample.values.len();
                    if wrong.len() < 5 {
                        wrong.push(format!("{member}->{partner}"));
                    }
                }
            }
        }
        println!(
            "  {px:>8.0} {:>10.2} {:>8} {:>8}   {}",
            total / glyphs.max(1) as f64,
            format!("{right}/{counted}"),
            wrong_glyphs,
            wrong.join(" ")
        );
    }
}

/// Glyphs a character needs before its disc-side width is worth printing beside the font's.
const MIN_SAMPLE: usize = 20;

/// The size the font's own width ratio is read at.
///
/// The whole finding, and the reason it is not [`RENDER_PX`](subtrackt_glyph::font::RENDER_PX).
/// `I` is about 7% wider than `l` in Arial's outlines, which at 96px is **six tenths of a pixel**:
/// the rasteriser rounds both to nine and the difference is gone. It reappears at 128px and is
/// stable from 256 up. A reference set generated at 96px therefore cannot carry this number, and
/// nothing is wrong with the typeface, the set or the matcher — the fidelity of one measurement is
/// what is wrong, which is exactly the kind of thing a bench has to say out loud.
const HIFI_PX: f32 = 512.0;

/// Score the pair against the threshold the *font* implies, not the one that best fits the disc.
///
/// The distinction is the whole difference between a feature and a fitted constant. A cut chosen
/// on the disc's own two populations is guaranteed to separate them and says nothing about whether
/// anything could have predicted it; a cut halfway between what the typeface draws is a number the
/// reference set could carry, and its error rate here is what a width term would actually achieve.
///
/// Two candidate features, because the italic act says they are not the same measurement. The
/// **box** is what the letterboxed vector already sees a lossy version of; the **stem** — ink
/// divided by height — is the box with the slant divided back out, and an italic `l` and an italic
/// `I` differ by their stems while their boxes are dominated by a slant they share.
fn predicted(
    labelled: &[Labelled],
    pair: &[char],
    font: &Font,
    italic: bool,
) -> anyhow::Result<()> {
    let (Some(first), Some(second)) = (pair.first().copied(), pair.get(1).copied()) else {
        return Ok(());
    };
    let cap = f64::from(ink_box(font, 'H', HIFI_PX).map_or(0, |(_, height)| height));
    anyhow::ensure!(cap > 0.0, "the font rasterises no capital H at {HIFI_PX}px");

    println!(
        "
  {} — measured against the face it was set in, at {HIFI_PX:.0}px",
        if italic { "italic" } else { "upright" }
    );
    for (name, from_font, from_disc) in FEATURES {
        let (Some(low), Some(high)) = (from_font(font, first, cap), from_font(font, second, cap))
        else {
            println!("    {name}: the font draws no outline for one of the pair");
            continue;
        };
        let cut = f64::midpoint(low, high);
        println!(
            "    {name}: the font draws {first} at {low:.2}% of cap height, {second} at {high:.2}%"
        );
        println!(
            "      the threshold halfway between them, which a reference set could carry: {cut:.2}%"
        );

        let population: Vec<&Labelled> = labelled.iter().filter(|g| g.italic == italic).collect();
        let (mut wrong, mut total) = (0usize, 0usize);
        for (character, above) in [(first, low > high), (second, high > low)] {
            let measured: Vec<(&Labelled, f64)> = population
                .iter()
                .copied()
                .filter(|g| g.want == character)
                .filter_map(|g| from_disc(g).map(|value| (g, value)))
                .collect();
            // Named, not just counted. A glyph on the wrong side of a threshold is either an
            // artefact of the instrument — a line too short for its cap height to be measured well
            // — or a real hazard, and only looking at where it sat tells them apart.
            let missed: Vec<&(&Labelled, f64)> = measured
                .iter()
                .filter(|(_, value)| (*value > cut) != above)
                .collect();
            let listed: Vec<String> = missed
                .iter()
                .take(3)
                .map(|(g, value)| {
                    format!("cue {} line {} at {value:.1}% of a {:.0}px cap", g.at.0, g.at.1, g.cap)
                })
                .collect();
            println!(
                "      {character}: {} of {} on the wrong side{}",
                missed.len(),
                measured.len(),
                if listed.is_empty() {
                    String::new()
                } else {
                    format!("  ({})", listed.join("; "))
                }
            );
            wrong += missed.len();
            total += measured.len();
        }
        if total > 0 {
            println!("      {wrong} of {total} — {:.1}%", wrong as f64 * 100.0 / total as f64);
        }
    }
    Ok(())
}

/// The two candidate width features: a name, how the font gives it, and how the disc gives it.
///
/// Both are fractions of cap height, and both are read off the *ink* rather than the rasteriser's
/// box, because a component is by definition what survived thresholding. #99 is the record of what
/// happens when the two sides letterbox different boxes.
type FontSide = fn(&Font, char, f64) -> Option<f64>;
type DiscSide = fn(&Labelled) -> Option<f64>;
const FEATURES: [(&str, FontSide, DiscSide); 3] = [
    (
        "aspect ratio",
        |font, ch, _| {
            let (width, height) = ink_box(font, ch, HIFI_PX)?;
            (height > 0).then(|| f64::from(width) * 100.0 / f64::from(height))
        },
        |g| (g.height > 0).then(|| f64::from(g.width) * 100.0 / f64::from(g.height)),
    ),
    (
        "box width",
        |font, ch, cap| ink_box(font, ch, HIFI_PX).map(|(w, _)| f64::from(w) * 100.0 / cap),
        |g| Some(f64::from(g.width) * 100.0 / g.cap),
    ),
    (
        "stem width (ink / height)",
        |font, ch, cap| {
            let (ink, _) = ink_and_box(font, ch, HIFI_PX)?;
            let (_, height) = ink_box(font, ch, HIFI_PX)?;
            (height > 0).then(|| f64::from(ink) / f64::from(height) * 100.0 / cap)
        },
        |g| {
            g.ink
                .filter(|_| g.height > 0)
                .map(|ink| f64::from(ink) / f64::from(g.height) * 100.0 / g.cap)
        },
    ),
];

/// The coverage above which a pixel counts as ink, which is `font::INK` — private there, and
/// duplicated here rather than widened, because this is a bench asking what that choice implies and
/// not a second consumer of it.
const INK: u8 = 128;

/// Ink pixels of one character at one size, and its box, thresholded as the generator does.
fn ink_and_box(font: &Font, ch: char, px: f32) -> Option<(u32, (u32, u32))> {
    let (_, coverage) = font.rasterize(ch, px);
    let ink = coverage.iter().filter(|&&value| value >= INK).count();
    let box_ = ink_box(font, ch, px)?;
    Some((u32::try_from(ink).unwrap_or(u32::MAX), box_))
}

/// The ink box of one character at one size, thresholded the way the reference generator is.
fn ink_box(font: &Font, ch: char, px: f32) -> Option<(u32, u32)> {
    let (metrics, coverage) = font.rasterize(ch, px);
    let (mut x0, mut x1, mut y0, mut y1) = (metrics.width, 0usize, metrics.height, 0usize);
    for y in 0..metrics.height {
        for x in 0..metrics.width {
            if coverage[y * metrics.width + x] >= INK {
                x0 = x0.min(x);
                x1 = x1.max(x + 1);
                y0 = y0.min(y);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x1 > x0).then(|| (u32::try_from(x1 - x0).unwrap_or(0), u32::try_from(y1 - y0).unwrap_or(0)))
}

/// Join every glyph to the release character standing in its place.
fn label(
    outcome: &subtrackt::Outcome,
    survey: &subtrackt::GlyphSurvey,
    matcher: &HammingMatcher,
    want_cues: &[disc::Cue],
) -> Vec<Labelled> {
    // Glyphs arrive in one flat list; this is where each cue's share begins.
    let mut by_cue: Vec<Vec<usize>> = vec![Vec::new(); survey.cues];
    for (index, glyph) in survey.glyphs.iter().enumerate() {
        if let Some(slot) = by_cue.get_mut(glyph.cue) {
            slot.push(index);
        }
    }

    let mut out = Vec::new();
    for (cue_index, members) in by_cue.iter().enumerate() {
        let Some(cue) = outcome.track.cues.get(cue_index) else {
            continue;
        };
        let Some(want) =
            disc::nearest(want_cues, i64::try_from(cue.span.start.as_millis()).unwrap_or(i64::MAX))
        else {
            continue;
        };

        // Reading order, the assembler's rule: lines top to bottom, glyphs left to right within
        // one. Reproduced rather than borrowed because the assembler returns a rendered string and
        // what is wanted here is the glyph behind each character of it.
        let mut order: Vec<usize> = members.clone();
        order.sort_by_key(|&index| (survey.glyphs[index].line, survey.glyphs[index].bounds.x));

        // Read without the spaces the assembler would insert. Their absence costs nothing: the
        // release keeps its own spaces, so every one of them aligns as a deletion and the letters
        // either side stay on the diagonal — and a label can then ask whether a word space stood
        // *next to* a glyph, which is exactly what decides whether its advance is measurable.
        let got: String = order
            .iter()
            .map(|&index| {
                let glyph = &survey.glyphs[index];
                matcher
                    .scan_with(&glyph.features, glyph.metrics, glyph.mark, glyph.aspect)
                    .character
                    .unwrap_or('\u{fffd}')
            })
            .collect();
        let want_text = disc::flatten(&want.text);
        let want_chars: Vec<char> = want_text.chars().collect();
        let got_chars: Vec<char> = got.chars().collect();

        // One cap height per line rather than per glyph. `height_percent` is an integer, so a
        // single glyph gives the line's cap height to about a percent; the median over a line of
        // them is far tighter, and every glyph on a line shares one cap height by definition.
        let caps = cap_heights(survey, members);

        let mut label_of: Vec<Option<usize>> = vec![None; got_chars.len()];
        for step in disc::trace(&want_text, &got) {
            if let (Some(got_at), Some(want_at)) = (step.got_at, step.want_at) {
                label_of[got_at] = Some(want_at);
            }
        }

        for (position, &index) in order.iter().enumerate() {
            let Some(want_at) = label_of[position] else {
                continue;
            };
            let glyph = &survey.glyphs[index];
            let Some(&cap) = caps.get(&glyph.line) else {
                continue;
            };

            // The next glyph is only the next *character* if the release has no space between
            // them and no line ended. Anything else measures a word space or a line break.
            let next = order.get(position + 1).map(|&index| &survey.glyphs[index]);
            let adjacent = next.filter(|next| {
                next.line == glyph.line
                    && label_of.get(position + 1) == Some(&Some(want_at + 1))
                    && want_chars
                        .get(want_at + 1)
                        .is_some_and(|c| !c.is_whitespace())
            });

            out.push(Labelled {
                want: want_chars[want_at],
                got: (got_chars[position] != '\u{fffd}').then(|| got_chars[position]),
                width: glyph.bounds.width,
                height: glyph.bounds.height,
                cap,
                ink: glyph
                    .mask
                    .as_ref()
                    .map(|m| u32::try_from(m.foreground_count()).unwrap_or(u32::MAX)),
                advance: adjacent.map(|next| next.bounds.x.saturating_sub(glyph.bounds.x)),
                gap: adjacent.map(|next| next.bounds.x.saturating_sub(glyph.bounds.right())),
                italic: want.italic,
                at: (cue_index, glyph.line),
            });
        }
    }
    out
}

/// The cap height of each line of one cue, in pixels.
///
/// `metrics.height_percent` is the glyph's height as a percentage of it, so each glyph gives one
/// estimate and the median of a line's estimates is the line's. A line whose metrics were never
/// measurable is absent rather than defaulted, which is the choice `LineMetrics::UNKNOWN` makes
/// everywhere else: a fabricated cap height would put every ratio on this line quietly wrong.
fn cap_heights(survey: &subtrackt::GlyphSurvey, members: &[usize]) -> BTreeMap<usize, f64> {
    let mut per_line: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    for &index in members {
        let glyph = &survey.glyphs[index];
        if !glyph.metrics.known || glyph.metrics.height_percent == 0 {
            continue;
        }
        let cap = f64::from(glyph.bounds.height) * 100.0 / f64::from(glyph.metrics.height_percent);
        per_line.entry(glyph.line).or_default().push(cap);
    }
    per_line
        .into_iter()
        .filter_map(|(line, mut caps)| {
            caps.sort_by(f64::total_cmp);
            caps.get(caps.len() / 2).map(|&cap| (line, cap))
        })
        .collect()
}

/// One measured quantity for one class of glyph.
struct Sample {
    values: Vec<f64>,
}

impl Sample {
    fn new(values: Vec<f64>) -> Self {
        Self { values }
    }

    fn mean(&self) -> f64 {
        if self.values.is_empty() {
            return 0.0;
        }
        self.values.iter().sum::<f64>() / self.values.len() as f64
    }

    fn sd(&self) -> f64 {
        if self.values.len() < 2 {
            return 0.0;
        }
        let mean = self.mean();
        let variance = self
            .values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (self.values.len() - 1) as f64;
        variance.sqrt()
    }

    /// The median and the quartiles either side of it.
    ///
    /// #171 is why these sit beside the mean. A mean and a standard deviation cannot tell a dead
    /// axis from a contaminated population: an `l` whose ink measures four times its own median
    /// width is not a wide `l`, it is a component fused to its neighbour, and a handful of those
    /// move the mean and explode the deviation while the bulk of the class sits untouched. The
    /// quartiles say what a *typical* glyph does, so the two readings can be told apart.
    ///
    /// Deliberately not routed through any of the four `percentile` functions #165 is about: they
    /// use three different index formulas and published figures depend on each, so picking one here
    /// would be choosing which of those to change.
    fn quartiles(&self) -> (f64, f64, f64) {
        if self.values.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        let mut sorted = self.values.clone();
        sorted.sort_by(f64::total_cmp);
        let at = |q: usize| sorted[(sorted.len() - 1) * q / 100];
        (at(25), at(50), at(75))
    }

    fn range(&self) -> (f64, f64) {
        let low = self.values.iter().copied().fold(f64::INFINITY, f64::min);
        let high = self
            .values
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        (low, high)
    }
}

/// The best a single threshold on one measure could do at telling the two classes apart.
///
/// Reported as the error rate of the *best* cut rather than as a distance between means, because a
/// mean difference says a feature exists and only this says whether it decides anything. The
/// counterfactual it is measured against is stated with it: `l` → `I` costs 330 errors today, so a
/// rule whose best cut errs on 40% of the pair would trade 330 errors for something near 300.
fn best_cut(a: &Sample, b: &Sample) -> Option<(f64, f64)> {
    if a.values.is_empty() || b.values.is_empty() {
        return None;
    }
    let mut cuts: Vec<f64> = a.values.iter().chain(&b.values).copied().collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();

    let mut best: Option<(f64, f64)> = None;
    for &cut in &cuts {
        // `a` below the cut, `b` on or above it. Both directions are tried, because which class is
        // the wider one is a fact about the typeface and not something to assume.
        for direction in [1.0, -1.0] {
            let wrong = a
                .values
                .iter()
                .filter(|&&v| v * direction >= cut * direction)
                .count()
                + b.values
                    .iter()
                    .filter(|&&v| v * direction < cut * direction)
                    .count();
            let rate = wrong as f64 / (a.values.len() + b.values.len()) as f64;
            if best.is_none_or(|(_, previous)| rate < previous) {
                best = Some((cut * direction, rate));
            }
        }
    }
    best
}

/// Print what the two classes look like, measure by measure.
fn report(labelled: &[Labelled], pair: &[char], glyphs: usize) {
    println!("\n--- glyph geometry by what the release says it was (#109) ---");
    println!(
        "  {glyphs} glyphs surveyed, {} of them labelled by a paired release cue",
        labelled.len()
    );

    let mut tally: BTreeMap<char, usize> = BTreeMap::new();
    for glyph in labelled {
        *tally.entry(glyph.want).or_insert(0) += 1;
    }
    let listed: Vec<String> = pair
        .iter()
        .map(|c| format!("{c} x{}", tally.get(c).copied().unwrap_or(0)))
        .collect();
    println!("  the pair under test: {}", listed.join("   "));

    let (Some(first), Some(second)) = (pair.first().copied(), pair.get(1).copied()) else {
        println!("  --pair needs two characters");
        return;
    };

    // The two populations kept apart. The italic act is set in another face at another size, so
    // mixing them would widen every spread here with a difference that has nothing to do with the
    // question — and #66 is the record of how differently that act behaves.
    for italic in [false, true] {
        let population: Vec<&Labelled> = labelled.iter().filter(|g| g.italic == italic).collect();
        println!(
            "
  {}: {} of {} labelled glyphs",
            if italic { "italic" } else { "upright" },
            population.len(),
            labelled.len()
        );
        measures(&population, first, second);
        read_as(&population, first, second);
    }
}

/// Print every measure for one population.
fn measures(upright: &[&Labelled], first: char, second: char) {
    for (name, measure) in MEASURES {
        let a = Sample::new(values(upright, first, measure));
        let b = Sample::new(values(upright, second, measure));
        println!("\n  {name}");
        for (character, sample) in [(first, &a), (second, &b)] {
            if sample.values.is_empty() {
                println!("    {character}: no glyph carries it");
                continue;
            }
            let (low, high) = sample.range();
            println!(
                "    {character}: n={:<5} mean {:>7.2}  sd {:>6.2}  range {:>6.2} .. {:>6.2}",
                sample.values.len(),
                sample.mean(),
                sample.sd(),
                low,
                high
            );
            // #171: the mean and the deviation cannot tell a dead axis from a contaminated
            // population, and the quartiles can. See `Sample::quartiles`.
            let (q1, median, q3) = sample.quartiles();
            println!(
                "           {:>12}  p25 {q1:>6.2}  median {median:>6.2}  p75 {q3:>6.2}",
                ""
            );
        }
        match best_cut(&a, &b) {
            Some((cut, rate)) => println!(
                "    best single threshold: {:.2} -> {:.1}% of the pair on the wrong side",
                cut.abs(),
                rate * 100.0
            ),
            None => println!("    no threshold: one class is empty"),
        }
    }
}

/// The measures, each a name and how to pull it out of one labelled glyph.
///
/// Every ratio is against the line's own cap height rather than in pixels, which `CLAUDE.md`
/// requires and which is also what makes the italic act comparable at all: it is set smaller.
const MEASURES: [(&str, DiscSide); 6] = [
    ("ink width, pixels", |g| Some(f64::from(g.width))),
    ("aspect ratio, % of the glyph's own height", |g| {
        (g.height > 0).then(|| f64::from(g.width) * 100.0 / f64::from(g.height))
    }),
    ("ink width, % of cap height", |g| {
        Some(f64::from(g.width) * 100.0 / g.cap)
    }),
    ("mean stem width (ink / height), % of cap height", |g| {
        g.ink
            .filter(|_| g.height > 0)
            .map(|ink| f64::from(ink) / f64::from(g.height) * 100.0 / g.cap)
    }),
    ("advance to the next character, % of cap height", |g| {
        g.advance.map(|advance| f64::from(advance) * 100.0 / g.cap)
    }),
    ("gap to the next character, % of cap height", |g| {
        g.gap.map(|gap| f64::from(gap) * 100.0 / g.cap)
    }),
];

fn values(glyphs: &[&Labelled], want: char, measure: DiscSide) -> Vec<f64> {
    glyphs
        .iter()
        .filter(|g| g.want == want)
        .filter_map(|g| measure(g))
        .collect()
}

/// What the matcher actually read each class as, which is the error the geometry would have to fix.
fn read_as(glyphs: &[&Labelled], first: char, second: char) {
    println!("\n  what the matcher read them as, before post-correction");
    for want in [first, second] {
        let mut tally: BTreeMap<String, usize> = BTreeMap::new();
        for glyph in glyphs.iter().filter(|g| g.want == want) {
            *tally
                .entry(glyph.got.map_or_else(|| "unread".to_owned(), String::from))
                .or_insert(0) += 1;
        }
        let mut ranked: Vec<(String, usize)> = tally.into_iter().collect();
        ranked.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        let listed: Vec<String> = ranked
            .iter()
            .take(6)
            .map(|(got, count)| format!("{got} x{count}"))
            .collect();
        println!("    {want}: {}", listed.join("   "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_populations_that_do_not_overlap_are_cut_without_error() {
        let low = Sample::new(vec![11.5, 11.9, 12.0]);
        let high = Sample::new(vec![13.9, 14.2, 14.4]);
        let (_, rate) = best_cut(&low, &high).expect("both classes have members");
        assert!(
            rate.abs() < f64::EPSILON,
            "an unoverlapped pair costs nothing to separate"
        );
    }

    #[test]
    fn a_measure_that_carries_no_information_cuts_at_half_the_pair() {
        let a = Sample::new(vec![1.0, 1.0, 1.0, 1.0]);
        let b = Sample::new(vec![1.0, 1.0, 1.0, 1.0]);
        let (_, rate) = best_cut(&a, &b).expect("both classes have members");
        assert!(
            (rate - 0.5).abs() < f64::EPSILON,
            "identical distributions cannot beat calling every glyph the same character"
        );
    }

    #[test]
    fn the_wider_class_may_be_either_side_of_the_cut() {
        // Which of a pair is the wider one is a fact about the typeface. A search that only ever
        // tried "the first class is the smaller" would report a perfect feature as a useless one
        // for half the pairs it was asked about.
        let wide = Sample::new(vec![20.0, 21.0]);
        let narrow = Sample::new(vec![10.0, 11.0]);
        let (_, rate) = best_cut(&wide, &narrow).expect("both classes have members");
        assert!(rate.abs() < f64::EPSILON);
    }

    #[test]
    fn an_empty_class_has_no_threshold_rather_than_a_perfect_one() {
        let some = Sample::new(vec![1.0, 2.0]);
        let none = Sample::new(Vec::new());
        assert!(
            best_cut(&some, &none).is_none(),
            "a pair with nothing on one side would otherwise score 0% and mean nothing"
        );
    }

    #[test]
    fn a_single_observation_has_a_mean_and_no_spread() {
        let one = Sample::new(vec![12.5]);
        assert!((one.mean() - 12.5).abs() < f64::EPSILON);
        assert!(one.sd().abs() < f64::EPSILON, "one glyph cannot disagree with itself");
    }
}
