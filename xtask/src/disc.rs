//! Scoring an extraction against a release subtitle for the same film.
//!
//! Everything else in this project measures against ground truth it rendered itself. That is the
//! ceiling case by construction — same font on both sides, no compression, no authoring — and
//! `docs/glyph-stability.md` is largely a record of the ways real material differs from it. What
//! has been missing is a number from a real disc.
//!
//! A subtitle file shipped alongside a rip is not ground truth. It comes from a different release,
//! it may itself have been read off the same bitmaps by some other tool, and it can carry its own
//! errors. What it *is* is an independent transcript of the same dialogue, produced without
//! reference to this pipeline — which is enough to rank two extractions of one track against each
//! other, and enough to tell 7% from 30%. It is not enough to certify an absolute figure, and no
//! claim here should be read as doing so.
//!
//! Two decisions the numbers turn on:
//!
//! - **Cues pair by start time, not by index.** The two releases are cut differently, so one
//!   missing cue would shift every score after it. A cue with no partner inside the tolerance is
//!   counted and reported rather than dropped: scoring only what lined up would flatter a run that
//!   produced half a track.
//! - **Whitespace is flattened before scoring.** The releases wrap their lines differently, and a
//!   line break in a different place is a layout difference between releases rather than a
//!   character the matcher got wrong. The unflattened figure is printed beside it so the size of
//!   that effect stays visible instead of being taken on trust.
//!
//! The split by style is the reason this exists in its own right rather than as a one-off script.
//! A release subtitle marks its italic cues, so the same run answers [#14][issue-14] — whether a
//! reference set needs a vector per typographic variant — on material that changes style mid-film.
//!
//! [issue-14]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/14

use std::cmp::Ordering;
use std::path::Path;

use anyhow::Context as _;

/// How far apart two cues may start and still be the same cue.
///
/// Two releases of one film share an authoring pass but not a frame rate or a cut, so a constant
/// offset of up to a second or so between them is ordinary. Wider than this and it stops being the
/// same cue; narrower and an ordinary release difference reads as a missing cue.
const TOLERANCE_MS: i64 = 2_000;

/// One subtitle cue: when it starts, what it says, and whether the release marked it italic.
struct Cue {
    start_ms: i64,
    text: String,
    italic: bool,
}

/// Parse an SRT into cues, keeping the italic marking and discarding every other tag.
///
/// Deliberately lenient. This reads files this project did not write, and a release subtitle that
/// carries a stray blank line or a positioning tag is still usable evidence — whereas refusing it
/// would mean refusing the only real ground truth available. Nothing downstream depends on the file
/// being well-formed, which is why this differs from the decoder's posture of rejecting anything
/// malformed.
fn parse(text: &str) -> Vec<Cue> {
    let mut cues: Vec<Cue> = Vec::new();
    let mut current: Option<(i64, Vec<String>)> = None;

    for line in text.lines() {
        if let Some(start) = timestamp(line) {
            if let Some((at, body)) = current.take() {
                push(&mut cues, at, &body);
            }
            current = Some((start, Vec::new()));
        } else if let Some((_, body)) = current.as_mut() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.chars().all(|c| c.is_ascii_digit()) {
                body.push(trimmed.to_owned());
            }
        }
    }
    if let Some((at, body)) = current {
        push(&mut cues, at, &body);
    }
    cues
}

/// Add one cue, dropping it if it has no text at all.
fn push(cues: &mut Vec<Cue>, start_ms: i64, body: &[String]) {
    let joined = body.join("\n");
    if joined.trim().is_empty() {
        return;
    }
    let italic = joined.contains("<i>") || joined.contains("<I>");
    cues.push(Cue { start_ms, text: strip_tags(&joined), italic });
}

/// The start time of an SRT timing line, in milliseconds.
fn timestamp(line: &str) -> Option<i64> {
    let (left, _) = line.split_once("-->")?;
    let left = left.trim();
    let (clock, fraction) = left.split_once([',', '.'])?;
    let mut parts = clock.split(':');
    let hours: i64 = parts.next()?.trim().parse().ok()?;
    let minutes: i64 = parts.next()?.parse().ok()?;
    let seconds: i64 = parts.next()?.parse().ok()?;
    let millis: i64 = fraction.get(..3)?.parse().ok()?;
    Some(((hours * 60 + minutes) * 60 + seconds) * 1000 + millis)
}

/// Remove every `<...>` tag.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0u32;
    for c in text.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Collapse every run of whitespace to one space.
fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Levenshtein distance in characters.
///
/// Two rows rather than a full matrix: a cue is tens of characters, but a caller comparing whole
/// tracks would otherwise allocate the product of two transcripts.
fn edit(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];

    for (i, ca) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != *cb);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// One extracted cue matched to its release counterpart.
struct Pair<'a> {
    got: &'a Cue,
    want: &'a Cue,
}

/// Pair extracted cues to release cues by start time.
///
/// Both sides are in time order, so this is a merge rather than a search.
fn pair<'a>(got: &'a [Cue], want: &'a [Cue]) -> (Vec<Pair<'a>>, usize) {
    let mut pairs = Vec::new();
    let mut unpaired = 0;
    let mut index = 0;

    for cue in got {
        while index < want.len() && want[index].start_ms < cue.start_ms - TOLERANCE_MS {
            index += 1;
        }
        match want.get(index) {
            Some(candidate) if (candidate.start_ms - cue.start_ms).abs() <= TOLERANCE_MS => {
                pairs.push(Pair { got: cue, want: candidate });
                index += 1;
            }
            _ => unpaired += 1,
        }
    }
    (pairs, unpaired)
}

/// Errors and characters under one grouping.
#[derive(Default, Clone, Copy)]
struct Tally {
    cues: usize,
    flat_errors: usize,
    flat_chars: usize,
    raw_errors: usize,
    raw_chars: usize,
}

impl Tally {
    fn add(&mut self, got: &str, want: &str) {
        self.cues += 1;
        self.flat_errors += edit(&flatten(got), &flatten(want));
        self.flat_chars += flatten(want).chars().count();
        self.raw_errors += edit(got, want);
        self.raw_chars += want.chars().count();
    }

    fn merge(&mut self, other: Self) {
        self.cues += other.cues;
        self.flat_errors += other.flat_errors;
        self.flat_chars += other.flat_chars;
        self.raw_errors += other.raw_errors;
        self.raw_chars += other.raw_chars;
    }

    #[allow(clippy::cast_precision_loss)]
    fn cer(self) -> f64 {
        if self.flat_chars == 0 {
            return 0.0;
        }
        100.0 * self.flat_errors as f64 / self.flat_chars as f64
    }

    #[allow(clippy::cast_precision_loss)]
    fn raw_cer(self) -> f64 {
        if self.raw_chars == 0 {
            return 0.0;
        }
        100.0 * self.raw_errors as f64 / self.raw_chars as f64
    }
}

fn read(path: &str) -> anyhow::Result<Vec<Cue>> {
    let bytes = std::fs::read(Path::new(path)).with_context(|| format!("reading {path}"))?;
    // Release subtitles are not always valid UTF-8 and not always what their BOM claims. A lossy
    // read costs a replacement character in a cue that was already going to score badly; refusing
    // the file costs the whole measurement.
    let text = String::from_utf8_lossy(&bytes);
    Ok(parse(text.trim_start_matches('\u{feff}')))
}

/// Score one extraction against a release subtitle.
fn score(got_path: &str, want_path: &str) -> anyhow::Result<()> {
    let got = read(got_path)?;
    let want = read(want_path)?;
    let (pairs, unpaired) = pair(&got, &want);

    let (mut upright, mut italic) = (Tally::default(), Tally::default());
    for p in &pairs {
        let tally = if p.want.italic {
            &mut italic
        } else {
            &mut upright
        };
        tally.add(&p.got.text, &p.want.text);
    }
    let mut all = upright;
    all.merge(italic);

    println!(
        "  cues: {} extracted, {} in the release, {unpaired} with no partner",
        got.len(),
        want.len()
    );
    println!(
        "  {:<9} {:>5}  {:>7}  {:>8}  {:>10}",
        "", "cues", "chars", "CER", "CER (raw)"
    );
    for (label, tally) in [("upright", upright), ("italic", italic), ("all", all)] {
        println!(
            "  {label:<9} {:>5}  {:>7}  {:>7.1}%  {:>9.1}%",
            tally.cues,
            tally.flat_chars,
            tally.cer(),
            tally.raw_cer()
        );
    }
    Ok(())
}

/// Compare two extractions of one track, cue by cue.
///
/// The count that matters is `worse`, not the aggregate. `docs/post-correction.md` states the rule:
/// a stage that fixes three characters and invents one has still turned a detectable failure into a
/// plausible wrong answer once, and an aggregate hides it. So every cue that got worse is printed
/// in full rather than counted.
fn compare(before_path: &str, after_path: &str, want_path: &str) -> anyhow::Result<()> {
    let before = read(before_path)?;
    let after = read(after_path)?;
    let want = read(want_path)?;

    let (before_pairs, _) = pair(&before, &want);
    let (after_pairs, _) = pair(&after, &want);

    let (mut better, mut worse, mut same) = (0usize, 0usize, 0usize);
    let (mut before_tally, mut after_tally) = (Tally::default(), Tally::default());

    for a in &before_pairs {
        let Some(b) = after_pairs
            .iter()
            .find(|b| b.want.start_ms == a.want.start_ms)
        else {
            continue;
        };
        let reference = flatten(&a.want.text);
        let was = edit(&flatten(&a.got.text), &reference);
        let now = edit(&flatten(&b.got.text), &reference);
        before_tally.add(&a.got.text, &a.want.text);
        after_tally.add(&b.got.text, &a.want.text);

        match now.cmp(&was) {
            Ordering::Less => better += 1,
            Ordering::Equal => same += 1,
            Ordering::Greater => {
                worse += 1;
                println!("\n  WORSE at {} ms ({was} -> {now} errors)", a.want.start_ms);
                println!("    before {}", flatten(&a.got.text));
                println!("    after  {}", flatten(&b.got.text));
                println!("    want   {reference}");
            }
        }
    }

    println!("\n  cues compared : {}", better + worse + same);
    println!("  cues improved : {better}");
    println!("  cues worse    : {worse}");
    println!("  cues unchanged: {same}");
    println!("  CER before    : {:>5.1}%", before_tally.cer());
    println!("  CER after     : {:>5.1}%", after_tally.cer());
    println!("  points gained : {:>5.1}", before_tally.cer() - after_tally.cer());
    Ok(())
}

/// Score an extraction against a release subtitle for the same film.
///
/// # Errors
/// Fails if a file cannot be read, or if the arguments are missing.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

    if let Some(at) = args.iter().position(|a| a == "--compare") {
        let after = args
            .get(at + 1)
            .context("--compare needs the second extraction")?;
        let before = positional
            .first()
            .context("usage: srt-score <before.srt> <release.srt> --compare <after.srt>")?;
        let want = positional
            .get(1)
            .filter(|p| **p != after)
            .or_else(|| positional.get(2))
            .context("missing the release subtitle to score against")?;
        return compare(before, after, want);
    }

    let got = positional
        .first()
        .context("usage: srt-score <extracted.srt> <release.srt>")?;
    let want = positional.get(1).context("missing the release subtitle")?;
    score(got, want)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "1\n00:00:01,000 --> 00:00:02,000\n<i>Hello there</i>\n\n\
                          2\n00:00:04,500 --> 00:00:06,000\nSecond line\nwrapped\n";

    #[test]
    fn a_cue_keeps_its_italic_marking_and_loses_every_other_tag() {
        let cues = parse(SAMPLE);
        assert_eq!(cues.len(), 2);
        assert!(cues[0].italic, "the release marked this one italic");
        assert_eq!(cues[0].text, "Hello there", "and the tag itself is not text");
        assert!(!cues[1].italic);
    }

    #[test]
    fn a_timestamp_is_read_in_milliseconds_from_the_start_of_the_track() {
        let cues = parse(SAMPLE);
        assert_eq!(cues[0].start_ms, 1_000);
        assert_eq!(cues[1].start_ms, 4_500);
    }

    #[test]
    fn a_wrapped_cue_keeps_its_break_until_the_score_flattens_it() {
        // Both halves matter: the raw column has to see the break, and the flattened column has to
        // not. A parser that dropped the newline would make the two columns identical by
        // construction and the comparison between them meaningless.
        let cues = parse(SAMPLE);
        assert_eq!(cues[1].text, "Second line\nwrapped");
        assert_eq!(flatten(&cues[1].text), "Second line wrapped");
    }

    #[test]
    fn a_cue_with_no_partner_within_the_tolerance_is_counted_rather_than_dropped() {
        // The failure this guards against is a run that produced half a track scoring well on the
        // half it produced.
        let got =
            parse("1\n00:00:01,000 --> 00:00:02,000\nA\n\n2\n00:01:00,000 --> 00:01:01,000\nB\n");
        let want = parse("1\n00:00:01,200 --> 00:00:02,000\nA\n");
        let (pairs, unpaired) = pair(&got, &want);
        assert_eq!(pairs.len(), 1);
        assert_eq!(unpaired, 1);
    }

    #[test]
    fn a_release_cut_differently_still_pairs_by_time_rather_than_by_index() {
        // The reason pairing is not by index: one cue missing from the middle of a release would
        // otherwise misalign every cue after it and report a wrong track as an unreadable one.
        let got = parse(
            "1\n00:00:01,000 --> 00:00:02,000\nA\n\n2\n00:00:05,000 --> 00:00:06,000\nB\n\n\
             3\n00:00:09,000 --> 00:00:10,000\nC\n",
        );
        let want =
            parse("1\n00:00:01,000 --> 00:00:02,000\nA\n\n2\n00:00:09,100 --> 00:00:10,000\nC\n");
        let (pairs, unpaired) = pair(&got, &want);
        assert_eq!(pairs.len(), 2);
        assert_eq!(unpaired, 1, "the cue this release does not have");
        assert_eq!(pairs[1].got.text, "C", "and C matched C rather than B matching C");
    }

    #[test]
    fn edit_distance_counts_substitutions_insertions_and_deletions_alike() {
        assert_eq!(edit("abc", "abc"), 0);
        assert_eq!(edit("abc", "abd"), 1);
        assert_eq!(edit("abc", "ab"), 1);
        assert_eq!(edit("ab", "abc"), 1);
        assert_eq!(edit("", "abc"), 3);
        assert_eq!(edit("abc", ""), 3);
    }

    #[test]
    fn a_line_break_in_a_different_place_costs_nothing_once_flattened() {
        // The whole reason the flattened column is the one quoted. Two releases wrap the same
        // sentence differently, and that is a layout difference rather than a character the matcher
        // got wrong.
        let a = "Just talk to me, okay?\nI can't believe you just left.";
        let b = "Just talk to me,\nokay? I can't believe you just left.";
        assert!(edit(a, b) > 0, "the break moved");
        assert_eq!(edit(&flatten(a), &flatten(b)), 0);
    }
}
