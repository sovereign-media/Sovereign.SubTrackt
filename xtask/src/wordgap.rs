//! Which letters lose the word space in front of them, and why.
//!
//! [#219](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/219). Gone Girl's Swedish
//! track fuses `jag` to the word before it 80 times, its Norwegian track fuses `jeg` 62 times, and
//! its **English** track — same disc, same typeface, same layout — fuses `you` six times. The bug is
//! not Scandinavian. It has simply never been measurable on a bench that is nine English tracks.
//!
//! Six instances cannot rank a change. This can, because it has ground truth by construction:
//! render a line, know where its spaces were, and ask which ones came back.
//!
//! ## What it renders
//!
//! One cue per ordered pair of letters, `nonP Lnon anan onon` — four words and three spaces, of
//! which the **first** is the pair under test and the other two are controls in neutral company.
//! Four words rather than two because `split_threshold` reads the line's own gap distribution, and
//! a line with a single break is a different population from a subtitle line, which is the
//! distinction `xtask spacing-margin` exists to make.
//!
//! Then the whole thing goes through the shipped pipeline against a reference set generated from
//! the same font, and the emitted text is compared to what was rendered. A pair whose letters did
//! not survive the round trip at all is discarded rather than counted: a fused or defused glyph
//! changes the character count, and charging that to the spacing rule would measure the matcher.
//!
//! ## What it also measures, and why
//!
//! #219 predicted the ranking would follow "how high the leftmost ink starts". The mechanism this
//! found is a *pair* property rather than a letter one, so both sides are measured:
//! [`facing_heights`] reports the height at which `P`'s **rightmost** ink sits and the height at
//! which `L`'s **leftmost** ink sits, as percentages of cap height above the baseline.
//!
//! The gap the assembler measures is between bounding **boxes**. When the two facing edges are at
//! different heights, the boxes can overlap in x while the ink never comes close — so the measured
//! gap understates the real separation, and the more the two heights differ, the more it understates
//! it. That is a falsifiable statement about pairs, and the table at the end is what falsifies it.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context as _;
use fontdue::{Font, FontSettings};
use subtrackt::{Config, Pipeline, UnmatchedPolicy};
use subtrackt_glyph::font::{Face, RENDER_PX, generate};
use subtrackt_glyph::reference::Style;

/// Letters tested on both sides of the break.
///
/// Lowercase only. A capital after a space is the common case in prose and is *easier* — a capital's
/// left edge is nearly always its full height — so including it would dilute the population the
/// defect lives in. Every glued instance the disc produced is lowercase: `attjag`, `ifyou`.
const ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz";

/// Where the tested space sits, counted in characters of the space-stripped line.
///
/// `nonP` is four characters, so the break under test is after the fourth. Held as a constant
/// because [`line_for`]'s shape and this index have to agree and nothing else would notice if they
/// stopped.
const TESTED_BREAK: usize = 4;

/// The line rendered for each pair, with `{P}` and `{L}` filled in.
///
/// The two control breaks are `n`-to-`a` and `n`-to-`o`: round-shouldered letters whose facing ink
/// runs the full x-height, which is the easiest case there is. They are here to catch a line whose
/// spacing collapsed for some reason other than the pair — a rendering accident, a threshold that
/// found no decisive cut at all — because a run that silently counted those as pair failures would
/// blame the letters for the harness.
///
/// **The `H` and the `p` are load-bearing.** A line of nothing but x-height letters gives
/// `metrics::measure_all` no cap line and no baseline to work from, so every glyph on it is matched
/// on shape alone — and the first casualty is `o` against `O`, which is the pair #37's line-metric
/// term exists for. The first version of this harness rendered `non{P} {L}non anan onon` and read
/// it back as `?O?? ??O? ???? O?O?`, discarding 651 of 676 pairs as unreadable while the spacing it
/// was built to measure worked perfectly. A real subtitle line has an ascender on it; a synthetic
/// one has to be given one deliberately.
fn line_for(previous: char, next: char) -> String {
    format!("Hon{previous} {next}nop anan onon")
}

/// What one pair did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The tested space came back, and so did both controls.
    Kept,
    /// The tested space was lost while both controls survived. The defect.
    Lost,
    /// A control was lost too, so the line says nothing about the pair.
    LineFailed,
    /// The characters did not survive the round trip, so the spaces cannot be located.
    Unreadable,
}

/// Height above the baseline of a glyph's leftmost and rightmost ink, in percent of cap height.
///
/// The pair of numbers the mechanism is stated in. Measured off the rasteriser at [`RENDER_PX`] and
/// thresholded at the binarizer's half, so it is the same ink the segmenter would see rather than
/// an outline's control points.
///
/// Returns `None` for a character the font cannot draw, and for one whose ink is empty.
fn facing_heights(font: &Font, ch: char) -> Option<(i32, i32)> {
    let (metrics, coverage) = font.rasterize(ch, RENDER_PX);
    let (width, height) = (metrics.width, metrics.height);
    if width == 0 || height == 0 {
        return None;
    }
    let cap = i32::try_from(font.metrics('H', RENDER_PX).height)
        .unwrap_or(1)
        .max(1);

    // The mid-row of a column's ink, as a height above the baseline. fontdue reports `ymin` as the
    // offset of the bitmap's bottom edge from the baseline, negative where a glyph descends.
    let column_height = |x: usize| -> Option<i32> {
        let rows: Vec<usize> = (0..height)
            .filter(|y| coverage[y * width + x] >= 128)
            .collect();
        let first = *rows.first()?;
        let last = *rows.last()?;
        let mid = first.midpoint(last);
        let above_bottom = i32::try_from(height - mid).unwrap_or(0);
        Some((above_bottom + metrics.ymin) * 100 / cap)
    };

    let left = (0..width).find_map(column_height)?;
    let right = (0..width).rev().find_map(column_height)?;
    Some((left, right))
}

/// The space positions of a line, counted in characters of the space-stripped line.
fn break_positions(text: &str) -> Vec<usize> {
    let mut at = 0;
    let mut out = Vec::new();
    for ch in text.chars() {
        if ch == ' ' {
            out.push(at);
        } else {
            at += 1;
        }
    }
    out
}

/// Everything but the spaces.
fn squeezed(text: &str) -> String {
    text.chars().filter(|ch| *ch != ' ').collect()
}

/// Render every pair, read them back, and say what happened to each break.
fn measure(font: &Font, bytes: &[u8], px: f32) -> anyhow::Result<BTreeMap<(char, char), Verdict>> {
    let pairs: Vec<(char, char)> = ALPHABET
        .chars()
        .flat_map(|previous| ALPHABET.chars().map(move |next| (previous, next)))
        .collect();
    let cues: Vec<(Vec<String>, f32)> = pairs
        .iter()
        .map(|(previous, next)| (vec![line_for(*previous, *next)], px))
        .collect();

    let dir = std::env::temp_dir().join("subtrackt-word-gap");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("pairs-{px}.sup"));
    std::fs::write(&path, crate::fixture::build_sup(font, &cues, (1920, 1080))?)?;

    let faces = [Face { bytes, style: Style::Regular }];
    let reference = generate("word-gap", &faces, false)?.set;
    let config = Config { unmatched: UnmatchedPolicy::Placeholder, ..Config::default() };
    let outcome = Pipeline::new(config)
        .with_reference(reference)
        .run(&path)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut out = BTreeMap::new();
    for (index, pair) in pairs.iter().enumerate() {
        let Some(cue) = outcome.track.cues.get(index) else {
            out.insert(*pair, Verdict::Unreadable);
            continue;
        };
        let want = line_for(pair.0, pair.1);
        let got = cue.text().replace('\n', " ");
        if squeezed(&got) != squeezed(&want) {
            out.insert(*pair, Verdict::Unreadable);
            continue;
        }
        let found = break_positions(&got);
        let wanted = break_positions(&want);
        let controls_kept = wanted
            .iter()
            .filter(|at| **at != TESTED_BREAK)
            .all(|at| found.contains(at));
        let verdict = match (found.contains(&TESTED_BREAK), controls_kept) {
            (true, true) => Verdict::Kept,
            (false, true) => Verdict::Lost,
            _ => Verdict::LineFailed,
        };
        out.insert(*pair, verdict);
    }
    Ok(out)
}

/// Rank the right-hand letters by how often the space in front of them is lost.
fn report_letters(verdicts: &BTreeMap<(char, char), Verdict>, font: &Font) {
    let mut rows: Vec<(usize, usize, char, i32)> = ALPHABET
        .chars()
        .map(|next| {
            let judged = verdicts
                .iter()
                .filter(|((_, l), v)| *l == next && matches!(v, Verdict::Kept | Verdict::Lost))
                .count();
            let lost = verdicts
                .iter()
                .filter(|((_, l), v)| *l == next && **v == Verdict::Lost)
                .count();
            let left = facing_heights(font, next).map_or(0, |(left, _)| left);
            (lost, judged, next, left)
        })
        .collect();
    rows.sort_by(|a, b| {
        (b.0 * 100 / a.1.max(1))
            .cmp(&(a.0 * 100 / b.1.max(1)))
            .then(a.2.cmp(&b.2))
    });

    println!("\nThe letter after the space\n");
    println!("{:<6} {:>7} {:>8} {:>14}", "letter", "lost", "of", "left ink at");
    for (lost, judged, next, left) in rows {
        if lost == 0 {
            continue;
        }
        println!("{next:<6} {lost:>7} {judged:>8} {left:>13}%");
    }
    println!(
        "\n  `left ink at` is the height of the letter's leftmost ink above the baseline, as a\n  \
         percentage of cap height. Letters that never lose a space are omitted."
    );
}

/// Rank the left-hand letters the same way.
fn report_previous(verdicts: &BTreeMap<(char, char), Verdict>, font: &Font) {
    let mut rows: Vec<(usize, usize, char, i32)> = ALPHABET
        .chars()
        .map(|previous| {
            let judged = verdicts
                .iter()
                .filter(|((p, _), v)| *p == previous && matches!(v, Verdict::Kept | Verdict::Lost))
                .count();
            let lost = verdicts
                .iter()
                .filter(|((p, _), v)| *p == previous && **v == Verdict::Lost)
                .count();
            let right = facing_heights(font, previous).map_or(0, |(_, right)| right);
            (lost, judged, previous, right)
        })
        .collect();
    rows.sort_by(|a, b| {
        (b.0 * 100 / a.1.max(1))
            .cmp(&(a.0 * 100 / b.1.max(1)))
            .then(a.2.cmp(&b.2))
    });

    println!("\nThe letter before the space\n");
    println!("{:<6} {:>7} {:>8} {:>14}", "letter", "lost", "of", "right ink at");
    for (lost, judged, previous, right) in rows {
        if lost == 0 {
            continue;
        }
        println!("{previous:<6} {lost:>7} {judged:>8} {right:>13}%");
    }
}

/// Does the vertical offset between the two facing edges predict the loss?
///
/// The mechanism, put where it can be refuted. A measured gap is between boxes; two edges at the
/// same height make the box gap the ink gap, and two at different heights let the boxes overlap in
/// x while the ink never comes close.
fn report_mechanism(verdicts: &BTreeMap<(char, char), Verdict>, font: &Font) {
    let heights: BTreeMap<char, (i32, i32)> = ALPHABET
        .chars()
        .filter_map(|ch| facing_heights(font, ch).map(|h| (ch, h)))
        .collect();

    // Bucketed rather than correlated, because a correlation coefficient over a bimodal population
    // says less than the two buckets it is computed from.
    let mut buckets: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for ((previous, next), verdict) in verdicts {
        if !matches!(verdict, Verdict::Kept | Verdict::Lost) {
            continue;
        }
        let (Some((_, right)), Some((left, _))) = (heights.get(previous), heights.get(next)) else {
            continue;
        };
        let offset = right.abs_diff(*left);
        let bucket = (offset / 20) * 20;
        let entry = buckets.entry(bucket).or_insert((0, 0));
        entry.1 += 1;
        if *verdict == Verdict::Lost {
            entry.0 += 1;
        }
    }

    println!("\nLoss against the offset between the two facing edges\n");
    println!("{:<14} {:>7} {:>8} {:>8}", "offset", "lost", "of", "rate");
    for (bucket, (lost, judged)) in buckets {
        println!(
            "{:<14} {lost:>7} {judged:>8} {:>7}%",
            format!("{bucket}-{}%", bucket + 19),
            lost * 100 / judged.max(1)
        );
    }
}

/// One measured gap between two glyphs standing next to each other on a line.
struct Gap {
    previous: char,
    next: char,
    /// The gap as a percentage of the line's median glyph width, which is the unit
    /// `split_threshold`'s first decisiveness test is stated in.
    ratio: u32,
    /// The same distance measured between **ink** rather than between boxes, in the same unit.
    ///
    /// `None` where the two glyphs share no row — a `p` beside a `'` never face each other at all,
    /// and a horizontal distance between them is not a thing that exists.
    ink_ratio: Option<u32>,
    /// Whether the line's own split threshold puts this gap in the word-break class.
    is_break: bool,
}

/// The narrowest horizontal separation between two glyphs' ink, over the rows they share.
///
/// This is the number a reader's eye uses and the number the assembler does not have. The shipped
/// measurement is `next.x - previous.right()`, a distance between **bounding boxes**, and the two
/// agree only when both glyphs' facing edges are vertical and at the same height. A `j` whose box
/// is widened leftwards by its descender hook is nowhere near the letter before it at any height
/// the letter before it occupies — but its box is.
///
/// Rows are the subtitle plane's, so the two masks are indexed through their own bounds.
fn ink_gap(previous: &subtrackt::GlyphRecord, next: &subtrackt::GlyphRecord) -> Option<u32> {
    let (left, right) = (previous.mask.as_ref()?, next.mask.as_ref()?);
    let top = previous.bounds.y.max(next.bounds.y);
    let bottom = previous.bounds.bottom().min(next.bounds.bottom());
    let mut narrowest: Option<u32> = None;
    for y in top..bottom {
        let last = (0..left.width())
            .rev()
            .find(|x| left.get(*x, y - previous.bounds.y));
        let first = (0..right.width()).find(|x| right.get(*x, y - next.bounds.y));
        let (Some(last), Some(first)) = (last, first) else {
            continue;
        };
        let span = (next.bounds.x + first).saturating_sub(previous.bounds.x + last + 1);
        narrowest = Some(narrowest.map_or(span, |best: u32| best.min(span)));
    }
    narrowest
}

/// Measure every adjacent pair on a real track, with no ground truth and no sidecar.
///
/// The fixture below can say what the pipeline does to text somebody generated. This says what it
/// does to a disc, and it is the half that matters, because the defect is a *rendering* property:
/// how wide a studio sets its word space, against how wide the assembler measures one.
///
/// Nothing here needs to know which words were on the screen. Every line supplies its own median
/// glyph width and its own split threshold, so each gap is scored against the line it came from.
fn measure_media(
    media: &Path,
    reference: subtrackt_glyph::ReferenceSet,
) -> anyhow::Result<Vec<Gap>> {
    use subtrackt_glyph::matcher::{HammingMatcher, MatchThresholds};
    use subtrackt_text::layout::split_threshold;

    // Masks on, which is what makes the ink measurement possible at all. They cost a copy of
    // every glyph's ink and nothing on the matching path reads them, so this is the one command in
    // the tree that has a reason to ask.
    let config = Config {
        unmatched: UnmatchedPolicy::Placeholder,
        glyph_masks: true,
        ..Config::default()
    };
    let survey = Pipeline::new(config)
        .with_reference(reference.clone())
        .survey(media, None)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let matcher = HammingMatcher::new(reference, MatchThresholds::default())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut lines: BTreeMap<(usize, usize), Vec<&subtrackt::GlyphRecord>> = BTreeMap::new();
    for glyph in &survey.glyphs {
        lines
            .entry((glyph.cue, glyph.line))
            .or_default()
            .push(glyph);
    }

    let rules = subtrackt::LayoutRules::default();
    let mut out = Vec::new();
    for glyphs in lines.values_mut() {
        if glyphs.len() < 3 {
            continue;
        }
        glyphs.sort_by_key(|g| g.bounds.x);
        let gaps: Vec<u32> = glyphs
            .windows(2)
            .map(|pair| pair[1].bounds.x.saturating_sub(pair[0].bounds.right()))
            .collect();
        let mut widths: Vec<u32> = glyphs.iter().map(|g| g.bounds.width).collect();
        widths.sort_unstable();
        let width = widths[widths.len() / 2].max(1);
        // A line with no decisive cut is one long word, and its gaps say nothing about spacing.
        let Some(threshold) = split_threshold(&gaps, width, rules) else {
            continue;
        };

        let read: Vec<Option<char>> = glyphs
            .iter()
            .map(|g| {
                matcher
                    .scan_with(&g.features, g.metrics, g.mark, g.aspect)
                    .character
            })
            .collect();
        for (index, gap) in gaps.iter().enumerate() {
            let (Some(previous), Some(next)) = (read[index], read[index + 1]) else {
                continue;
            };
            out.push(Gap {
                previous,
                next,
                ratio: gap * 100 / width,
                ink_ratio: ink_gap(glyphs[index], glyphs[index + 1]).map(|ink| ink * 100 / width),
                is_break: *gap >= threshold,
            });
        }
    }
    Ok(out)
}

/// How wide a word space measures in front of each letter, on real material.
///
/// The number to read is the **median of the break class**. Every one of those gaps is the same
/// physical thing — the space a studio set between two words — so a letter whose column reads 55
/// where another reads 95 is not spaced differently on screen. It is *measured* differently, and
/// the ones at the bottom of this table are the ones whose true spaces fall under the threshold and
/// vanish.
fn report_media(gaps: &[Gap], font: &Font) {
    let letters: std::collections::BTreeSet<char> = gaps
        .iter()
        .map(|g| g.next)
        .filter(|c| c.is_alphabetic())
        .collect();

    let median_of = |values: &mut Vec<u32>| -> Option<u32> {
        values.sort_unstable();
        values.get(values.len() / 2).copied()
    };

    let mut rows: Vec<(u32, u32, u32, usize, char, i32)> = letters
        .into_iter()
        .filter_map(|next| {
            let mut breaks: Vec<u32> = gaps
                .iter()
                .filter(|g| g.next == next && g.is_break)
                .map(|g| g.ratio)
                .collect();
            let mut within: Vec<u32> = gaps
                .iter()
                .filter(|g| g.next == next && !g.is_break)
                .map(|g| g.ratio)
                .collect();
            let mut ink: Vec<u32> = gaps
                .iter()
                .filter(|g| g.next == next && g.is_break)
                .filter_map(|g| g.ink_ratio)
                .collect();
            if breaks.len() < 30 {
                return None;
            }
            let count = breaks.len();
            let broken = median_of(&mut breaks)?;
            let inside = median_of(&mut within).unwrap_or(0);
            let inked = median_of(&mut ink).unwrap_or(0);
            let left = facing_heights(font, next).map_or(0, |(left, _)| left);
            Some((broken, inked, inside, count, next, left))
        })
        .collect();
    rows.sort_by_key(|row| (row.0, row.4));

    println!(
        "
How wide a word space measures in front of each letter
"
    );
    println!(
        "{:<8} {:>8} {:>8} {:>8} {:>8} {:>14}",
        "letter", "box", "ink", "within", "breaks", "left ink at"
    );
    for (broken, inked, inside, count, next, left) in &rows {
        println!("{next:<8} {broken:>7}% {inked:>7}% {inside:>7}% {count:>8} {left:>13}%");
    }
    for note in [
        "",
        "  Every median is a percentage of the line's median glyph width. `box` is the gap the",
        "  assembler measures -- between bounding boxes -- and `ink` is the narrowest distance",
        "  between the two glyphs' own ink over the rows they share. Where the two disagree,",
        "  the letter's box is wider than the letter looks and the space in front of it is",
        "  measured short. `within` is the same box gap between two letters of one word, so",
        "  `box` against `within` is the separation the split threshold has to find.",
        "  Letters with fewer than 30 word breaks in front of them are omitted.",
    ] {
        println!("{note}");
    }
}

/// The pairs whose box gap understates their ink gap by the most.
///
/// The per-letter table above averages over whatever letter happened to precede each one, which is
/// what makes it comparable between tracks. This says which *pairs* the effect actually lives in,
/// and it is where the letter before the space finally matters: a `j` after a round `o` and a `j`
/// after a `T` are not the same measurement, because the two boxes overhang towards each other.
fn report_pairs(gaps: &[Gap], font: &Font, breaks: bool) {
    let mut per_pair: BTreeMap<(char, char), Vec<(u32, u32)>> = BTreeMap::new();
    for gap in gaps.iter().filter(|g| g.is_break == breaks) {
        if let Some(ink) = gap.ink_ratio {
            per_pair
                .entry((gap.previous, gap.next))
                .or_default()
                .push((gap.ratio, ink));
        }
    }

    let mut rows: Vec<(i64, u32, u32, usize, char, char)> = per_pair
        .into_iter()
        .filter(|(_, seen)| seen.len() >= 20)
        .map(|((previous, next), mut seen)| {
            seen.sort_unstable();
            let (box_gap, _) = seen[seen.len() / 2];
            let mut inks: Vec<u32> = seen.iter().map(|(_, ink)| *ink).collect();
            inks.sort_unstable();
            let ink = inks[inks.len() / 2];
            (
                i64::from(ink) - i64::from(box_gap),
                box_gap,
                ink,
                seen.len(),
                previous,
                next,
            )
        })
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.4.cmp(&b.4)).then(a.5.cmp(&b.5)));

    println!(
        "\nThe {} pairs where the box understates the space by the most\n",
        if breaks { "word-break" } else { "within-word" }
    );
    println!(
        "{:<8} {:>8} {:>8} {:>8} {:>12} {:>12}",
        "pair", "box", "ink", "breaks", "right ink at", "left ink at"
    );
    for (_, box_gap, ink, count, previous, next) in rows.iter().take(15) {
        let right = facing_heights(font, *previous).map_or(0, |(_, right)| right);
        let left = facing_heights(font, *next).map_or(0, |(left, _)| left);
        println!(
            "{:<8} {box_gap:>7}% {ink:>7}% {count:>8} {right:>11}% {left:>11}%",
            format!("{previous} {next}")
        );
    }
    for note in [
        "",
        "  Pairs seen fewer than 20 times across the track are omitted. `right ink at` is the",
        "  height of the *preceding* letter's rightmost ink and `left ink at` the following",
        "  letter's leftmost, both as percentages of cap height above the baseline.",
    ] {
        println!("{note}");
    }
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let path = args
        .first()
        .map_or("C:/Windows/Fonts/arial.ttf", String::as_str);
    let px: f32 = match args.iter().position(|a| a == "--px") {
        Some(at) => args.get(at + 1).context("--px needs a value")?.parse()?,
        None => 33.0,
    };
    let bytes = std::fs::read(Path::new(path)).with_context(|| format!("reading {path}"))?;
    let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
        .map_err(|e| anyhow::anyhow!("{path} is not a usable font: {e}"))?;

    // The disc first, because it is the instrument and the fixture is the control.
    if let Some(at) = args.iter().position(|a| a == "--media") {
        let media = args.get(at + 1).context("--media needs a path")?;
        let set = args
            .iter()
            .position(|a| a == "--reference")
            .and_then(|at| args.get(at + 1))
            .context("--media needs --reference")?;
        let reference = crate::util::load_reference(Path::new(set))?;
        let gaps = measure_media(Path::new(media), reference)?;
        let breaks = gaps.iter().filter(|g| g.is_break).count();
        println!("{} adjacent pairs on {media}, {breaks} of them word breaks", gaps.len());
        report_media(&gaps, &font);
        report_pairs(&gaps, &font, true);
        report_pairs(&gaps, &font, false);
    }

    let verdicts = measure(&font, &bytes, px)?;
    let counted = |want: Verdict| verdicts.values().filter(|v| **v == want).count();
    println!(
        "{} pairs at {px}px: {} kept, {} lost, {} line failures, {} unreadable",
        verdicts.len(),
        counted(Verdict::Kept),
        counted(Verdict::Lost),
        counted(Verdict::LineFailed),
        counted(Verdict::Unreadable),
    );

    report_letters(&verdicts, &font);
    report_previous(&verdicts, &font);
    report_mechanism(&verdicts, &font);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{FeatureVector, InkAspect, LineMetrics, MarkSlope, Rect};
    use subtrackt_glyph::binarize::BinaryMask;

    #[test]
    fn the_tested_break_index_matches_where_the_line_actually_puts_it() {
        // The one place this harness could lie without erroring. `TESTED_BREAK` names a position in
        // the space-stripped line, and if `line_for` ever gains or loses a character before the
        // break, every verdict would silently be about a control instead.
        let line = line_for('r', 'j');
        assert_eq!(break_positions(&line).first(), Some(&TESTED_BREAK));
        assert_eq!(break_positions(&line).len(), 3, "two controls and the pair under test");
    }

    #[test]
    fn a_lost_space_moves_a_break_position_rather_than_changing_the_letters() {
        // How a verdict is decided. The comparison is only meaningful while the characters match,
        // which is why a round trip that changed one is discarded rather than counted.
        let want = "Honr jnop anan onon";
        let got = "Honrjnop anan onon";
        assert_eq!(squeezed(want), squeezed(got));
        assert!(!break_positions(got).contains(&TESTED_BREAK));
        assert!(break_positions(got).contains(&8), "the controls are untouched");
    }

    #[test]
    fn the_leftmost_ink_of_a_j_sits_below_the_baseline() {
        // The whole mechanism in one character. `j`'s box is widened leftwards by a descender hook,
        // so the box reaches further left than anything a preceding letter could touch -- and the
        // gap the assembler measures is between boxes. Every letter whose box gap and ink gap
        // disagree on a real disc has this property; every letter where they agree does not.
        let bytes = a_font();
        let font = Font::from_bytes(bytes.as_slice(), FontSettings::default()).unwrap();
        let (left, _) = facing_heights(&font, 'j').expect("a font that draws j");
        assert!(
            left < 0,
            "j's leftmost ink measured at {left}% of cap height, expected below 0"
        );

        let (stem, _) = facing_heights(&font, 'n').expect("a font that draws n");
        assert!(
            stem > 0,
            "n's leftmost ink measured at {stem}%, expected above the baseline"
        );
    }

    /// A glyph whose mask is `rows`, each string a row with `#` for ink.
    fn glyph(x: u32, y: u32, rows: &[&str]) -> subtrackt::GlyphRecord {
        let width = u32::try_from(rows[0].len()).unwrap();
        let height = u32::try_from(rows.len()).unwrap();
        let bits: Vec<bool> = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        subtrackt::GlyphRecord {
            cue: 0,
            line: 0,
            bounds: Rect::new(x, y, width, height),
            features: FeatureVector::EMPTY,
            metrics: LineMetrics::UNKNOWN,
            mark: MarkSlope::NONE,
            aspect: InkAspect::UNKNOWN,
            mask: Some(BinaryMask::from_bits(width, height, &bits).unwrap()),
        }
    }

    #[test]
    fn the_ink_gap_is_wider_than_the_box_gap_when_a_glyph_overhangs() {
        // The measurement stated as an example. The right-hand glyph's box begins two columns
        // before its stem, because its bottom row reaches left -- so the boxes are one column apart
        // while the ink the two glyphs actually present to each other is four.
        let left = glyph(0, 0, &["##", "##", "##", "  "]);
        let right = glyph(3, 0, &["  ##", "  ##", "  ##", "####"]);
        assert_eq!(
            right.bounds.x - left.bounds.right(),
            1,
            "the gap the assembler measures"
        );
        assert_eq!(ink_gap(&left, &right), Some(3), "the gap between the ink itself");
    }

    #[test]
    fn the_ink_gap_equals_the_box_gap_for_two_upright_stems() {
        // The control, and the reason the two columns of the disc table are worth printing side by
        // side: for most letters they agree exactly, so a disagreement is a fact about the letter
        // rather than about the measurement.
        let left = glyph(0, 0, &["##", "##", "##"]);
        let right = glyph(5, 0, &["##", "##", "##"]);
        assert_eq!(right.bounds.x - left.bounds.right(), 3);
        assert_eq!(ink_gap(&left, &right), Some(3));
    }

    #[test]
    fn two_glyphs_that_share_no_row_have_no_horizontal_distance_at_all() {
        // An apostrophe beside a comma never face each other, and a number for how far apart they
        // are would be invented rather than measured.
        let high = glyph(0, 0, &["##", "##"]);
        let low = glyph(5, 8, &["##", "##"]);
        assert_eq!(ink_gap(&high, &low), None);
    }

    /// Fonts to try, matching the list `subtrackt_glyph::font`'s own tests use.
    fn a_font() -> Vec<u8> {
        for path in [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/Library/Fonts/Arial.ttf",
            "C:/Windows/Fonts/arial.ttf",
            "C:/Windows/Fonts/segoeui.ttf",
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                return bytes;
            }
        }
        panic!("no font found; install DejaVu Sans");
    }
}
