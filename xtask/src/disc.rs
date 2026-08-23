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
pub(crate) const TOLERANCE_MS: i64 = 2_000;

/// One subtitle cue: when it starts, what it says, and whether the release marked it italic.
#[derive(Clone)]
pub(crate) struct Cue {
    pub(crate) start_ms: i64,
    pub(crate) text: String,
    pub(crate) italic: bool,
}

/// Parse an SRT into cues, keeping the italic marking and discarding every other tag.
///
/// Deliberately lenient. This reads files this project did not write, and a release subtitle that
/// carries a stray blank line or a positioning tag is still usable evidence — whereas refusing it
/// would mean refusing the only real ground truth available. Nothing downstream depends on the file
/// being well-formed, which is why this differs from the decoder's posture of rejecting anything
/// malformed.
pub(crate) fn parse(text: &str) -> Vec<Cue> {
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
pub(crate) fn flatten(text: &str) -> String {
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
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    distance(&a, &b)
}

/// Levenshtein distance in words, which is what a word error rate counts.
///
/// The same recurrence over whitespace-separated tokens rather than characters. A word is wrong if
/// any character in it is, so this is the harsher of the two rates by construction — and the gap
/// between them is the statistic: 1% of characters spread one to a word is a far worse read than
/// the same 1% concentrated in a handful of words, and CER alone cannot tell those apart.
///
/// Tokens come from the flattened text, so the wrapping difference the CER column already discounts
/// cannot reappear here as a word only one of the two releases split across a line break.
fn edit_words(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let a: Vec<&str> = a.split_whitespace().collect();
    let b: Vec<&str> = b.split_whitespace().collect();
    distance(&a, &b)
}

/// Rolling-row Levenshtein over any two sequences of comparable items.
///
/// Two rows rather than a full matrix: a cue is tens of characters, but a caller comparing whole
/// tracks would otherwise allocate the product of two transcripts.
///
/// Shared by the character and the word rate rather than written twice. They are one recurrence
/// over different tokens, and two copies of it could drift into disagreeing about the figures every
/// table here turns on.
fn distance<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];

    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// Every cue's text in time order, as one string.
///
/// The unit of comparison that does not assume the two releases agree about where a cue ends.
pub(crate) fn transcript(cues: &[Cue]) -> String {
    let mut out = String::new();
    for cue in cues {
        let text = flatten(&cue.text);
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&text);
    }
    out
}

/// One extraction scored against a release as a single transcript, ignoring cue boundaries.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Whole {
    pub(crate) chars: usize,
    pub(crate) char_errors: usize,
    pub(crate) words: usize,
    pub(crate) word_errors: usize,
}

impl Whole {
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn cer(self) -> f64 {
        if self.chars == 0 {
            0.0
        } else {
            100.0 * self.char_errors as f64 / self.chars as f64
        }
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn wer(self) -> f64 {
        if self.words == 0 {
            0.0
        } else {
            100.0 * self.word_errors as f64 / self.words as f64
        }
    }
}

/// Score two whole transcripts against each other.
///
/// Cue-level scoring assumes the two releases broke the dialogue into cues at the same places, and
/// they routinely do not: one release carries "Yeah, I know" and "Miss, you have to stay calm" as
/// two cues where another carries them as one. Pairing by time then compares a cue against half of
/// its counterpart, and words that were read *correctly* are counted as errors — an artifact of the
/// instrument that grows with how differently the two releases were authored.
///
/// Aligning the concatenated transcripts removes that, and removes any residual timing difference
/// with it, because time is not consulted at all. What it cannot do is split the score by style: the
/// italic column needs to know which release cue a word came from, which is what pairing provides.
/// So both are reported, and the gap between them is the size of the cue-boundary effect.
pub(crate) fn whole(got: &[Cue], want: &[Cue]) -> Whole {
    let (got, want) = (transcript(got), transcript(want));
    Whole {
        chars: want.chars().count(),
        char_errors: edit(&got, &want),
        words: want.split_whitespace().count(),
        word_errors: edit_words(&got, &want),
    }
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

/// How a release subtitle was shifted onto the disc's timeline before scoring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Alignment {
    /// Multiplier applied to the extraction's timestamps, for a release cut at another rate.
    pub(crate) scale: f64,
    /// Constant shift applied after scaling, in milliseconds.
    pub(crate) offset_ms: i64,
    /// How many cues paired under it, which is what the search maximises.
    pub(crate) paired: usize,
}

/// Frame-rate ratios between one release and another.
///
/// A release telecined or PAL-sped from another does not sit at a constant offset from it: the two
/// drift apart linearly, so a scorer that only searches a shift lines the opening cues up and loses
/// the closing ones. These are the standard conversions and their inverses; a ratio not in this list
/// simply fails to beat the others and the run reports a poor pairing rather than a wrong one.
const RATIOS: [f64; 7] = [
    1.0,
    25.0 / 24.0,
    24.0 / 25.0,
    25.0 / 23.976,
    23.976 / 25.0,
    24.0 / 23.976,
    23.976 / 24.0,
];

/// How far apart two releases may sit and still be the same film.
///
/// Wide, because a sidecar subtitle is routinely authored against a cut carrying a different amount
/// of distributor logo before the first frame of picture.
const SEARCH_MS: i64 = 180_000;

/// Find the shift and rate that put the most cues of `got` beside a partner in `want`.
///
/// The measurement this whole module exists for compares two *different releases* of one film, and
/// the assumption that they share a timeline is false often enough to dominate a corpus. An offset
/// wider than [`TOLERANCE_MS`] does not degrade a score, it destroys it: cues stop pairing, and a
/// track read almost perfectly reports a CER above 100% because nothing lined up. That failure looks
/// exactly like a misread track in a summary table, which is why this is worth searching for rather
/// than assuming away.
///
/// Maximising paired cues rather than minimising error is deliberate. Choosing the alignment that
/// scored best would let the search buy a lower CER by discarding cues it could not read, which is
/// the one thing a scorer must not be able to do; pairing counts cannot be improved that way.
pub(crate) fn estimate_alignment(got: &[Cue], want: &[Cue]) -> Alignment {
    let mut best = Alignment { scale: 1.0, offset_ms: 0, paired: pair(got, want).0.len() };

    for scale in RATIOS {
        let scaled = rescale(got, scale, 0);
        let Some(offset) = modal_offset(&scaled, want) else {
            continue;
        };
        // The mode is a histogram bucket, so probe its neighbours for the exact shift.
        for probe in [
            offset - 250,
            offset - 100,
            offset,
            offset + 100,
            offset + 250,
        ] {
            let paired = pair(&rescale(got, scale, probe), want).0.len();
            if paired > best.paired {
                best = Alignment { scale, offset_ms: probe, paired };
            }
        }
    }
    best
}

/// `got` with every timestamp scaled and then shifted.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub(crate) fn rescale(got: &[Cue], scale: f64, offset_ms: i64) -> Vec<Cue> {
    got.iter()
        .map(|c| Cue {
            start_ms: (c.start_ms as f64 * scale) as i64 + offset_ms,
            text: c.text.clone(),
            italic: c.italic,
        })
        .collect()
}

/// The most common gap between an extracted cue and a nearby release cue.
///
/// A histogram rather than a mean: most cues of a mismatched pair contribute a meaningless gap, and
/// an average over those lands nowhere. If the two releases really are the same film, one bucket
/// carries far more of the mass than any other, and that bucket is the shift.
fn modal_offset(got: &[Cue], want: &[Cue]) -> Option<i64> {
    /// Bucket width. Fine enough to shift inside the pairing tolerance, coarse enough that the
    /// ordinary jitter between two authoring passes lands in one bucket rather than spread over ten.
    const BIN_MS: i64 = 200;
    /// Release cues considered either side of each extracted cue.
    const NEAR: usize = 24;

    let starts: Vec<i64> = want.iter().map(|c| c.start_ms).collect();
    let mut histogram: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();

    for cue in got {
        let at = starts.partition_point(|s| *s < cue.start_ms);
        let lo = at.saturating_sub(NEAR);
        let hi = (at + NEAR).min(starts.len());
        for start in &starts[lo..hi] {
            let delta = start - cue.start_ms;
            if delta.abs() <= SEARCH_MS {
                *histogram.entry(delta.div_euclid(BIN_MS)).or_default() += 1;
            }
        }
    }

    let (bucket, count) = histogram.into_iter().max_by_key(|(_, n)| *n)?;
    // A mode carried by a handful of cues is noise, not an alignment.
    (count >= 8).then_some(bucket * BIN_MS + BIN_MS / 2)
}

/// Errors and characters under one grouping.
#[derive(Default, Clone, Copy)]
pub(crate) struct Tally {
    pub(crate) cues: usize,
    flat_errors: usize,
    pub(crate) flat_chars: usize,
    raw_errors: usize,
    raw_chars: usize,
    word_errors: usize,
    pub(crate) words: usize,
}

impl Tally {
    fn add(&mut self, got: &str, want: &str) {
        let (got_flat, want_flat) = (flatten(got), flatten(want));
        self.cues += 1;
        self.flat_errors += edit(&got_flat, &want_flat);
        self.flat_chars += want_flat.chars().count();
        self.raw_errors += edit(got, want);
        self.raw_chars += want.chars().count();
        self.word_errors += edit_words(&got_flat, &want_flat);
        self.words += want_flat.split_whitespace().count();
    }

    fn merge(&mut self, other: Self) {
        self.cues += other.cues;
        self.flat_errors += other.flat_errors;
        self.flat_chars += other.flat_chars;
        self.raw_errors += other.raw_errors;
        self.raw_chars += other.raw_chars;
        self.word_errors += other.word_errors;
        self.words += other.words;
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn cer(self) -> f64 {
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

    /// Word error rate, over the same flattened text the quoted CER scores.
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn wer(self) -> f64 {
        if self.words == 0 {
            return 0.0;
        }
        100.0 * self.word_errors as f64 / self.words as f64
    }
}

/// One extraction scored against a release, split the way the printed table splits it.
pub(crate) struct Scored {
    pub(crate) upright: Tally,
    pub(crate) italic: Tally,
    pub(crate) all: Tally,
    /// Extracted cues with no release cue inside the tolerance.
    pub(crate) unpaired: usize,
}

/// Score without printing, so a sweep can put a dozen of these in one table.
pub(crate) fn scored(got: &[Cue], want: &[Cue]) -> Scored {
    let (pairs, unpaired) = pair(got, want);
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
    Scored { upright, italic, all, unpaired }
}

/// How many times the release's `from` was read as `to`.
///
/// The census prints this for every pair at once; a sweep wants one pair as a number, because the
/// whole question a weight answers is what happens to that one.
pub(crate) fn confusions(got: &[Cue], want: &[Cue], from: char, to: char) -> usize {
    let (pairs, _) = pair(got, want);
    pairs
        .iter()
        .flat_map(|p| align(&flatten(&p.want.text), &flatten(&p.got.text)))
        .filter(|op| *op == Op::Substitute(from, to))
        .count()
}

/// One cue read differently by two extractions of the same track.
pub(crate) struct Change<'a> {
    /// When the release cue starts, which is how a reader finds it.
    pub(crate) at: i64,
    /// Errors before, and after.
    pub(crate) was: usize,
    pub(crate) now: usize,
    pub(crate) before: &'a str,
    pub(crate) after: &'a str,
    pub(crate) want: &'a str,
}

/// Every cue two extractions of one track disagree about, scored against the release.
///
/// Shared by `--compare` and by the sweeps rather than written twice, because the number that
/// decides whether a change ships is the *worse* column — `docs/post-correction.md`'s rule — and two
/// implementations of it could disagree about the one figure everything turns on.
pub(crate) fn changes<'a>(before: &'a [Cue], after: &'a [Cue], want: &'a [Cue]) -> Vec<Change<'a>> {
    let (before_pairs, _) = pair(before, want);
    let (after_pairs, _) = pair(after, want);

    let mut out = Vec::new();
    for a in &before_pairs {
        let Some(b) = after_pairs
            .iter()
            .find(|b| b.want.start_ms == a.want.start_ms)
        else {
            continue;
        };
        let reference = flatten(&a.want.text);
        out.push(Change {
            at: a.want.start_ms,
            was: edit(&flatten(&a.got.text), &reference),
            now: edit(&flatten(&b.got.text), &reference),
            before: &a.got.text,
            after: &b.got.text,
            want: &a.want.text,
        });
    }
    out
}

pub(crate) fn read(path: &str) -> anyhow::Result<Vec<Cue>> {
    let bytes = std::fs::read(Path::new(path)).with_context(|| format!("reading {path}"))?;
    // Release subtitles are not always valid UTF-8 and not always what their BOM claims. A lossy
    // read costs a replacement character in a cue that was already going to score badly; refusing
    // the file costs the whole measurement.
    let text = String::from_utf8_lossy(&bytes);
    Ok(parse(text.trim_start_matches('\u{feff}')))
}

/// Score one extraction against a release subtitle.
#[allow(clippy::cast_precision_loss)]
fn score(got_path: &str, want_path: &str, align: bool) -> anyhow::Result<()> {
    let (got, want, found) = load(got_path, want_path, align)?;
    let (pairs, _) = pair(&got, &want);

    let Scored { upright, italic, all, unpaired } = scored(&got, &want);
    let mut census = Census::new();
    for p in &pairs {
        census.add(&p.got.text, &p.want.text, p.want.italic);
    }

    println!(
        "  cues: {} extracted, {} in the release, {unpaired} with no partner",
        got.len(),
        want.len()
    );
    if let Some(a) = found {
        println!(
            "  aligned: rate x{:.4}, offset {:+.2}s, {} cues paired",
            a.scale,
            a.offset_ms as f64 / 1000.0,
            a.paired
        );
    }
    println!(
        "  {:<9} {:>5}  {:>7}  {:>8}  {:>10}  {:>7}  {:>8}",
        "", "cues", "chars", "CER", "CER (raw)", "words", "WER"
    );
    for (label, tally) in [("upright", upright), ("italic", italic), ("all", all)] {
        println!(
            "  {label:<9} {:>5}  {:>7}  {:>7.1}%  {:>9.1}%  {:>7}  {:>7.1}%",
            tally.cues,
            tally.flat_chars,
            tally.cer(),
            tally.raw_cer(),
            tally.words,
            tally.wer()
        );
    }
    let w = whole(&got, &want);
    println!(
        "  {:<9} {:>5}  {:>7}  {:>7.1}%  {:>10}  {:>7}  {:>7.1}%",
        "track",
        "-",
        w.chars,
        w.cer(),
        "-",
        w.words,
        w.wer()
    );
    println!("  (track: one transcript, cue boundaries and timing ignored)");

    census.print();
    prior(&pairs);
    Ok(())
}

/// Score the extraction against a character-bigram model of English, beside the release's own.
///
/// #101. The release subtitle is what makes this worth printing here rather than only in
/// `xtask fit-select`: it is an independent transcript of the same dialogue, so it is the score a
/// *correct* read of this track produces. Without it the extraction's number is a figure in an
/// unfamiliar unit; with it, the gap is the whole statistic.
///
/// Silence rather than a number when there is too little Latin-script text to score, which is the
/// constraint #101 asks to be built in from the start.
fn prior(pairs: &[Pair<'_>]) {
    let table = crate::bigram::Table::from_corpus(crate::bigram::CORPUS);
    let joined = |extracted: bool| {
        pairs
            .iter()
            .map(|p| flatten(&if extracted { p.got } else { p.want }.text))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let extraction = joined(true);
    let got = table.score(&extraction);
    let charged = table.score_charged(&extraction);
    let want = table.score(&joined(false));

    println!(
        "
--- language prior (#101) ---"
    );
    let show =
        |value: Option<f64>| value.map_or_else(|| "no score".to_owned(), |v| format!("{v:.3}"));
    println!("  extraction     : {}", show(got));
    println!(
        "  charged        : {}   <- unread characters charged the uniform floor",
        show(charged)
    );
    println!(
        "  release        : {}   <- what a correct read of this track scores",
        show(want)
    );
    println!(
        "  a uniform alphabet would score {:.3}",
        crate::bigram::Table::uniform_floor()
    );
    if let (Some(got), Some(want)) = (got, want) {
        println!("  the extraction is {:+.3} against the release", got - want);
    }
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

    let (mut better, mut worse, mut same) = (0usize, 0usize, 0usize);
    let (mut before_tally, mut after_tally) = (Tally::default(), Tally::default());

    for change in changes(&before, &after, &want) {
        before_tally.add(change.before, change.want);
        after_tally.add(change.after, change.want);

        match change.now.cmp(&change.was) {
            Ordering::Less => better += 1,
            Ordering::Equal => same += 1,
            Ordering::Greater => {
                worse += 1;
                println!(
                    "\n  WORSE at {} ms ({} -> {} errors)",
                    change.at, change.was, change.now
                );
                println!("    before {}", flatten(change.before));
                println!("    after  {}", flatten(change.after));
                println!("    want   {}", flatten(change.want));
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
    // Opt-in rather than default: every figure already published by this project was measured
    // without it, and a flag that silently re-times a release would move those numbers without any
    // change to the pipeline they describe.
    let align = args.iter().any(|a| a == "--align");
    if args.iter().any(|a| a == "--json") {
        return score_json(got, want, align);
    }
    score(got, want, align)
}

/// Read both sides, and put them on one timeline if asked.
fn load(
    got_path: &str,
    want_path: &str,
    align: bool,
) -> anyhow::Result<(Vec<Cue>, Vec<Cue>, Option<Alignment>)> {
    let got = read(got_path)?;
    let want = read(want_path)?;
    if !align {
        return Ok((got, want, None));
    }
    let found = estimate_alignment(&got, &want);
    Ok((rescale(&got, found.scale, found.offset_ms), want, Some(found)))
}

/// The same measurement as [`score`], as one JSON object on stdout.
///
/// A library sweep runs this once per title and aggregates a few hundred of them, and scraping a
/// column layout that exists to be read by a person is how that becomes silently wrong the first
/// time a table gains a column. The character tables carry raw counts rather than rates: rates do
/// not sum, and an aggregate over titles has to add the numerators and denominators itself.
///
/// Written by hand rather than derived. `xtask` has no serialisation dependency and this is a flat
/// object of numbers and single characters — see the dependency rule in `CLAUDE.md`.
fn score_json(got_path: &str, want_path: &str, align: bool) -> anyhow::Result<()> {
    let (got, want, found) = load(got_path, want_path, align)?;
    let (pairs, _) = pair(&got, &want);
    let Scored { upright, italic, all, unpaired } = scored(&got, &want);

    let mut census = Census::new();
    for p in &pairs {
        census.add(&p.got.text, &p.want.text, p.want.italic);
    }

    println!("{{");
    println!(
        "  \"cues_extracted\": {}, \"cues_release\": {}, \"cues_unpaired\": {unpaired},",
        got.len(),
        want.len()
    );
    if let Some(a) = found {
        println!(
            "  \"alignment\": {{ \"scale\": {:.6}, \"offset_ms\": {}, \"paired\": {} }},",
            a.scale, a.offset_ms, a.paired
        );
    }
    for (label, t) in [("upright", upright), ("italic", italic), ("all", all)] {
        println!(
            "  \"{label}\": {{ \"cues\": {}, \"chars\": {}, \"char_errors\": {}, \
             \"words\": {}, \"word_errors\": {}, \"cer\": {:.4}, \"wer\": {:.4} }},",
            t.cues,
            t.flat_chars,
            t.flat_errors,
            t.words,
            t.word_errors,
            t.cer(),
            t.wer()
        );
    }

    let w = whole(&got, &want);
    println!(
        "  \"track\": {{ \"chars\": {}, \"char_errors\": {}, \"words\": {}, \
         \"word_errors\": {}, \"cer\": {:.4}, \"wer\": {:.4} }},",
        w.chars,
        w.char_errors,
        w.words,
        w.word_errors,
        w.cer(),
        w.wer()
    );

    // Per-character rows: occurrences and errors, so an aggregate can re-derive the rate.
    println!("  \"characters\": [");
    let rows = census.rates(0);
    for (i, (c, seen, bad)) in rows.iter().enumerate() {
        let comma = if i + 1 == rows.len() { "" } else { "," };
        println!(
            "    {{ \"char\": {}, \"seen\": {seen}, \"wrong\": {bad} }}{comma}",
            json_char(*c)
        );
    }
    println!("  ],");

    // The confusion pairs themselves, which is what says *how* a character failed.
    println!("  \"confusions\": [");
    let subs = census.substitutions.ranked();
    for (i, ((from, to), n)) in subs.iter().enumerate() {
        let comma = if i + 1 == subs.len() { "" } else { "," };
        println!(
            "    {{ \"from\": {}, \"to\": {}, \"upright\": {}, \"italic\": {} }}{comma}",
            json_char(*from),
            json_char(*to),
            n[0],
            n[1]
        );
    }
    println!("  ]\n}}");
    Ok(())
}

/// One character as a JSON string, escaped.
fn json_char(c: char) -> String {
    match c {
        '"' => "\"\\\"\"".to_owned(),
        '\\' => "\"\\\\\"".to_owned(),
        REPLACEMENT => "\"unread\"".to_owned(),
        c if (c as u32) < 0x20 => format!("\"\\u{:04x}\"", c as u32),
        c => format!("\"{c}\""),
    }
}

/// What the pipeline writes where a glyph matched nothing.
///
/// Named so the census can report it as `unread` rather than as a lozenge nobody can search for.
/// It is the same character `UnmatchedPolicy::Placeholder` emits.
const REPLACEMENT: char = '\u{FFFD}';

/// One edit operation in an alignment, named from the release's point of view.
///
/// The direction decides what each bucket means, so it is worth stating. `want` is the release
/// subtitle and `got` is what this pipeline read, so:
///
/// - a **substitution** is a character the release has and the extraction read as something else;
/// - an **insertion** is a character the extraction produced that the release does not have — a
///   shattered glyph read as two characters, or a space put where no space belongs;
/// - a **deletion** is a character the release has that the extraction never produced — two
///   characters fused into one component, or a word space that was never split.
///
/// #98 predicted these two directional buckets the other way round, filing a missed word space
/// under insertions. It is a deletion: a space the extraction failed to emit is a release character
/// with no counterpart. The buckets are labelled by what they are rather than by the prediction,
/// and the prediction is scored against the labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// The release had the first character; the extraction read the second.
    Substitute(char, char),
    /// The extraction produced a character the release does not have.
    Insert(char),
    /// The release has a character the extraction never produced.
    Delete(char),
}

/// One column of an alignment: what the release had there, and what the extraction read.
///
/// Exactly one side is `None` for an insertion or a deletion, and both are `Some` for a match or a
/// substitution — so a column names an error *and* names a glyph that was read correctly. The
/// census only ever wanted the first, but #109 needs the second: to ask what an `l` looks like on
/// this disc, something has to say which read characters *were* the release's `l`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Step {
    /// The release character, or `None` where the extraction read one the release does not have.
    pub(crate) want: Option<char>,
    /// Its index in `want`, carried so a caller can look at what stood beside it — a word space in
    /// particular, which the extraction may never have produced.
    pub(crate) want_at: Option<usize>,
    /// The character read, or `None` where the release has one the extraction never produced.
    pub(crate) got: Option<char>,
    /// Its index in `got`, which is what lets a caller join a column back to the glyph behind it.
    pub(crate) got_at: Option<usize>,
}

/// Align two strings, returning one [`Step`] per column of the alignment.
///
/// A second pass rather than a widening of [`edit`]. #98 named both options and this is the safer
/// one: the score keeps its rolling row, so nothing about the census can move a CER figure by
/// changing how a distance is computed. The cost is one full matrix per cue, and a cue is tens of
/// characters.
///
/// `a_census_accounts_for_exactly_the_errors_the_score_counted` is what makes the two provably
/// agree — the operation count [`align`] derives from this equals the distance [`edit`] reports on
/// the same input, which is the only guarantee that a census row corresponds to a scored error.
pub(crate) fn trace(want: &str, got: &str) -> Vec<Step> {
    let want: Vec<char> = want.chars().collect();
    let got: Vec<char> = got.chars().collect();

    // The full matrix, because a traceback needs every row: `cost[i][j]` is the distance between
    // the first `i` characters of the release and the first `j` of the extraction.
    let mut cost = vec![vec![0usize; got.len() + 1]; want.len() + 1];
    for (i, row) in cost.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in cost[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=want.len() {
        for j in 1..=got.len() {
            let substitution = cost[i - 1][j - 1] + usize::from(want[i - 1] != got[j - 1]);
            let deletion = cost[i - 1][j] + 1;
            let insertion = cost[i][j - 1] + 1;
            cost[i][j] = substitution.min(deletion).min(insertion);
        }
    }

    // Walk back from the corner, breaking ties towards the diagonal. A run of characters read
    // wrongly is then counted as substitutions rather than as an insertion and a deletion that
    // happen to cost the same, which is the reading a confusion table exists to give.
    let (mut i, mut j) = (want.len(), got.len());
    let mut steps = Vec::new();
    while i > 0 || j > 0 {
        if i > 0 && j > 0 {
            let step = usize::from(want[i - 1] != got[j - 1]);
            if cost[i][j] == cost[i - 1][j - 1] + step {
                steps.push(Step {
                    want: Some(want[i - 1]),
                    want_at: Some(i - 1),
                    got: Some(got[j - 1]),
                    got_at: Some(j - 1),
                });
                i -= 1;
                j -= 1;
                continue;
            }
        }
        if i > 0 && cost[i][j] == cost[i - 1][j] + 1 {
            steps.push(Step {
                want: Some(want[i - 1]),
                want_at: Some(i - 1),
                got: None,
                got_at: None,
            });
            i -= 1;
            continue;
        }
        steps.push(Step {
            want: None,
            want_at: None,
            got: Some(got[j - 1]),
            got_at: Some(j - 1),
        });
        j -= 1;
    }
    steps.reverse();
    steps
}

/// The edit operations that turn `want` into `got`, which is [`trace`] with the matches dropped.
fn align(want: &str, got: &str) -> Vec<Op> {
    trace(want, got)
        .into_iter()
        .filter_map(|step| match (step.want, step.got) {
            (Some(want), Some(got)) if want != got => Some(Op::Substitute(want, got)),
            (Some(want), None) => Some(Op::Delete(want)),
            (None, Some(got)) => Some(Op::Insert(got)),
            (Some(_), Some(_)) | (None, None) => None,
        })
        .collect()
}

/// Counts for one bucket, split into the two populations the existing output splits by.
///
/// A `Vec` of pairs rather than a map. The alphabet a subtitle uses is small, and every table here
/// is printed in rank order, so a hash map would need an ordering pass before printing anyway.
struct Counter<K> {
    /// Column 0 is upright, column 1 italic.
    counts: Vec<(K, [usize; 2])>,
}

impl<K: PartialEq + Clone> Counter<K> {
    const fn new() -> Self {
        Self { counts: Vec::new() }
    }

    fn add(&mut self, key: &K, italic: bool) {
        let column = usize::from(italic);
        if let Some((_, counts)) = self.counts.iter_mut().find(|(k, _)| k == key) {
            counts[column] += 1;
            return;
        }
        let mut counts = [0usize; 2];
        counts[column] = 1;
        self.counts.push((key.clone(), counts));
    }

    /// The entries, most frequent first.
    fn ranked(&self) -> Vec<(K, [usize; 2])> {
        let mut ranked = self.counts.clone();
        ranked.sort_by_key(|(_, c)| std::cmp::Reverse(c[0] + c[1]));
        ranked
    }

    /// Column totals.
    fn totals(&self) -> [usize; 2] {
        self.counts
            .iter()
            .fold([0, 0], |acc, (_, c)| [acc[0] + c[0], acc[1] + c[1]])
    }
}

/// Every confusion the alignment saw, split upright from italic.
struct Census {
    substitutions: Counter<(char, char)>,
    insertions: Counter<char>,
    deletions: Counter<char>,
    /// How often each character appears in the release text that was scored.
    ///
    /// The denominator the other three buckets never had. A raw count says the full stop was
    /// misread 400 times; only this says whether that is 400 of 500 or 400 of 40,000. Ranking
    /// characters by count alone puts the frequent ones on top whatever their accuracy, which is
    /// the reading a per-character *rate* exists to replace.
    occurrences: Counter<char>,
}

/// Rows printed per table before the remainder is summarised.
///
/// The remainder is *stated* rather than dropped silently: a table that stops at twenty rows with
/// no tail line reads as a complete census, which is the kind of quiet truncation #98 exists to
/// replace.
const ROWS: usize = 20;

impl Census {
    const fn new() -> Self {
        Self {
            substitutions: Counter::new(),
            insertions: Counter::new(),
            deletions: Counter::new(),
            occurrences: Counter::new(),
        }
    }

    /// Record one cue's worth of operations.
    ///
    /// Scored on the flattened text, for the same reason the quoted CER is: the two releases wrap
    /// their lines differently, and a line break in a different place would otherwise dominate
    /// every bucket with operations on a newline.
    fn add(&mut self, got: &str, want: &str, italic: bool) {
        let want = flatten(want);
        for c in want.chars() {
            self.occurrences.add(&c, italic);
        }
        for op in align(&want, &flatten(got)) {
            match op {
                Op::Substitute(want, got) => self.substitutions.add(&(want, got), italic),
                Op::Insert(c) => self.insertions.add(&c, italic),
                Op::Delete(c) => self.deletions.add(&c, italic),
            }
        }
    }

    /// Per-character error rate: how often each release character was not read as itself.
    ///
    /// Substitutions and deletions of a character, over its occurrences. Insertions are deliberately
    /// not charged here — an inserted character is one the release never had, so there is no
    /// occurrence count to divide it by and it belongs to whichever release character it was
    /// inserted beside rather than to itself.
    ///
    /// Returns `(character, occurrences, errors)`, worst rate first, so the tail of rare-but-
    /// hopeless characters is visible rather than buried under the frequent ones.
    #[allow(clippy::cast_precision_loss)]
    fn rates(&self, floor: usize) -> Vec<(char, usize, usize)> {
        let wrong = |c: char| {
            let subs: usize = self
                .substitutions
                .counts
                .iter()
                .filter(|((from, _), _)| *from == c)
                .map(|(_, n)| n[0] + n[1])
                .sum();
            let dels = self
                .deletions
                .counts
                .iter()
                .find(|(k, _)| *k == c)
                .map_or(0, |(_, n)| n[0] + n[1]);
            subs + dels
        };

        let mut rows: Vec<(char, usize, usize)> = self
            .occurrences
            .counts
            .iter()
            .map(|(c, n)| (*c, n[0] + n[1], wrong(*c)))
            .filter(|(_, seen, _)| *seen >= floor)
            .collect();
        // Rate first, then the commoner character, so a tie between two hopeless rare glyphs is
        // still ordered by how much of the text each one costs.
        rows.sort_by(|a, b| {
            let rate = |(_, seen, bad): &(char, usize, usize)| *bad as f64 / *seen as f64;
            rate(b)
                .partial_cmp(&rate(a))
                .unwrap_or(Ordering::Equal)
                .then(b.1.cmp(&a.1))
        });
        rows
    }

    fn print(&self) {
        println!("\n--- confusion census (#98) ---");
        println!("  {:<38} {:>8} {:>7} {:>7}", "", "upright", "italic", "all");
        for (label, totals) in [
            ("substitutions", self.substitutions.totals()),
            ("insertions (read, not in the release)", self.insertions.totals()),
            ("deletions (in the release, not read)", self.deletions.totals()),
        ] {
            println!(
                "  {label:<38} {:>8} {:>7} {:>7}",
                totals[0],
                totals[1],
                totals[0] + totals[1]
            );
        }

        table("substitutions: release -> read", &self.substitutions, |(want, got)| {
            format!("{} -> {}", show(want), show(got))
        });
        table(
            "insertions: characters read that the release does not have",
            &self.insertions,
            show,
        );
        table("deletions: release characters never read", &self.deletions, show);
        self.print_rates();
    }

    /// The per-character error rate, worst first.
    ///
    /// A floor on occurrences, because a character the release used twice and the matcher missed
    /// once is a 50% rate that means nothing, and a table sorted by rate puts every such character
    /// above the ones that actually cost the read.
    #[allow(clippy::cast_precision_loss)]
    fn print_rates(&self) {
        const FLOOR: usize = 30;

        let rows = self.rates(FLOOR);
        println!("\n  per-character error rate (>= {FLOOR} occurrences in the release)");
        if rows.is_empty() {
            println!("    nothing");
            return;
        }
        println!(
            "    {:<10} {:>10} {:>8} {:>8}",
            "character", "in release", "wrong", "rate"
        );
        for (c, seen, bad) in rows.iter().take(ROWS) {
            println!(
                "    {:<10} {seen:>10} {bad:>8} {:>7.2}%",
                show(*c),
                100.0 * *bad as f64 / *seen as f64
            );
        }
        if rows.len() > ROWS {
            println!("    ... {} more characters", rows.len() - ROWS);
        }
    }
}

/// Print one ranked table, naming what it left out.
fn table<K: PartialEq + Clone>(title: &str, counter: &Counter<K>, label: impl Fn(K) -> String) {
    let ranked = counter.ranked();
    println!("\n  {title}");
    if ranked.is_empty() {
        println!("    nothing");
        return;
    }
    println!("    {:<24} {:>8} {:>7} {:>7}", "", "upright", "italic", "all");
    for (key, counts) in ranked.iter().take(ROWS) {
        println!(
            "    {:<24} {:>8} {:>7} {:>7}",
            label(key.clone()),
            counts[0],
            counts[1],
            counts[0] + counts[1]
        );
    }
    if ranked.len() > ROWS {
        let tail: usize = ranked.iter().skip(ROWS).map(|(_, c)| c[0] + c[1]).sum();
        println!(
            "    {:<24} {:>24}",
            format!("... {} more kinds", ranked.len() - ROWS),
            tail
        );
    }
}

/// A character as it should appear in a table cell.
///
/// A space is among the most common things in these tables and an unquoted one would be invisible,
/// which would hide the spacing rule — the thing #49 is asking about — behind a blank cell.
fn show(c: char) -> String {
    match c {
        ' ' => "space".to_owned(),
        REPLACEMENT => "unread".to_owned(),
        c if c.is_control() => format!("U+{:04X}", c as u32),
        c => c.to_string(),
    }
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

    #[test]
    fn a_correctly_read_character_is_a_column_of_the_alignment_rather_than_nothing() {
        // What #109 needs and the census never did. An error names itself; a character that was
        // read *correctly* is only visible as a column with both sides filled in, and without it
        // nothing can say which glyph on the disc the release calls an `l`.
        let steps = trace("all", "aII");
        assert_eq!(steps.len(), 3);
        assert_eq!(
            steps[0],
            Step {
                want: Some('a'),
                want_at: Some(0),
                got: Some('a'),
                got_at: Some(0)
            },
            "a match carries both sides and both positions"
        );
        assert_eq!(steps[1].want, Some('l'));
        assert_eq!(steps[1].got, Some('I'));
    }

    #[test]
    fn a_deletion_names_the_release_position_and_no_read_one() {
        let steps = trace("a b", "ab");
        let space = steps
            .iter()
            .find(|s| s.want == Some(' '))
            .expect("the space is a column");
        assert_eq!(
            space.got, None,
            "a space the extraction never produced was read as nothing"
        );
        assert_eq!(space.got_at, None);
        assert_eq!(space.want_at, Some(1));
    }

    #[test]
    fn a_census_accounts_for_exactly_the_errors_the_score_counted() {
        // The property the whole design of #98 turns on. The score keeps its rolling row and the
        // census runs a second, traceback-capable pass, so the only thing that makes a census row
        // correspond to a scored error is that the two agree on every input. A traceback that
        // emitted one operation too many would inflate every table without moving a single CER
        // figure, and nothing else here would notice.
        let cases = [
            ("", ""),
            ("abc", "abc"),
            ("abc", ""),
            ("", "abc"),
            ("Michelle", "Mlchelle"),
            ("Is it 1 or l?", "IsiI4orI?"),
            ("over the lazy dog.", "over Ihe I?zy dog?"),
            ("a quick brown fox", "aquick brownfox"),
            ("hello", "he\u{fffd}lo"),
            ("\"quoted\"", "''quoted''"),
        ];
        for (want, got) in cases {
            assert_eq!(
                align(want, got).len(),
                edit(got, want),
                "alignment of {want:?} -> {got:?} disagrees with the score"
            );
        }
    }

    #[test]
    fn a_substitution_is_recorded_in_the_direction_the_release_reads() {
        // `(want, got)`, not `(got, want)`. A table printed the wrong way round would name the
        // character the pipeline produced as the one it should have produced, and every conclusion
        // drawn from it would be backwards.
        assert_eq!(align("lazy", "Iazy"), vec![Op::Substitute('l', 'I')]);
    }

    #[test]
    fn a_word_space_the_extraction_never_produced_is_a_deletion() {
        // #98 predicted this bucket as an insertion. It is not: a space in the release with no
        // counterpart in the read is a release character the extraction never produced, which is
        // what a deletion is. Pinned because the prediction is scored against these labels.
        assert_eq!(align("two words", "twowords"), vec![Op::Delete(' ')]);
    }

    #[test]
    fn a_glyph_read_as_two_characters_is_an_insertion() {
        // The documented `"` case from `group.rs`: one component read as two single quotes. The
        // extraction has a character the release does not, which is the other direction.
        assert_eq!(align("\"a", "''a"), vec![Op::Insert('\''), Op::Substitute('"', '\'')]);
    }

    #[test]
    fn a_run_read_wrongly_is_counted_as_substitutions_rather_than_as_a_shift() {
        // Tie-breaking towards the diagonal, pinned. Substituting two characters and
        // inserting-then-deleting two both cost 2, so an aligner with the opposite preference
        // would report the same CER and a confusion table full of unrelated insertions.
        assert_eq!(align("to", "Io"), vec![Op::Substitute('t', 'I')]);
        assert_eq!(
            align("ab", "xy"),
            vec![Op::Substitute('a', 'x'), Op::Substitute('b', 'y')]
        );
    }

    /// A run of cues at irregular but deterministic gaps, starting at `from`.
    ///
    /// Deliberately not evenly spaced. A track of cues exactly one second apart pairs with itself
    /// under almost any shift — every cue finds *some* partner — so a uniform fixture would report
    /// an alignment as working when it had done nothing. Dialogue does not arrive on a metronome.
    fn track(from: i64, count: i64, text: &str) -> Vec<Cue> {
        let mut at = from;
        let mut state: i64 = 12_345;
        (0..count)
            .map(|i| {
                let cue = Cue { start_ms: at, text: format!("{text} {i}"), italic: false };
                // A short-period gap pattern would repeat, and a repeating pattern lines up with
                // itself at a shift that is a multiple of the period.
                state = (state.wrapping_mul(1_103_515_245) + 12_345) & 0x7fff_ffff;
                at += 1_200 + state % 4_000;
                cue
            })
            .collect()
    }

    #[test]
    fn a_release_offset_past_the_tolerance_is_found_rather_than_scored_as_a_misread() {
        // The failure this exists for. A sidecar authored against another cut sits whole seconds
        // from the disc, every cue misses its partner, and a track read perfectly reports a CER
        // above 100% — which is indistinguishable from a broken matcher in a summary table.
        let want = track(0, 60, "line");
        let got = track(30_000, 60, "line");

        let before = pair(&got, &want).0.len();
        assert!(before < 60, "the offset really does cost pairings");

        let found = estimate_alignment(&got, &want);
        assert!(
            (found.offset_ms + 30_000).abs() <= 250,
            "the shift is found to within a bucket"
        );
        assert_eq!(found.paired, 60, "and every cue pairs once applied");
    }

    #[test]
    fn a_release_cut_at_another_frame_rate_is_followed_rather_than_drifting_out_of_reach() {
        // A constant shift lines up the opening and loses the closing cues, because the two
        // releases drift apart linearly rather than sitting at a fixed distance.
        let want = track(0, 200, "line");
        let got = rescale(&want, 24.0 / 25.0, 0);

        let before = pair(&got, &want).0.len();
        assert!(before < 200, "the tail drifts past the tolerance");
        let found = estimate_alignment(&got, &want);
        assert!((found.scale - 25.0 / 24.0).abs() < 1e-6, "the rate is recovered");
        assert_eq!(found.paired, 200, "and the whole track pairs");
        assert!(found.paired > before, "which is strictly better than leaving it alone");
    }

    #[test]
    fn two_unrelated_tracks_are_left_where_they_are() {
        // The search must not invent an alignment for a sidecar belonging to another film. With no
        // repeated gap there is no mode, and the identity is what survives.
        let want = track(0, 40, "line");
        let got: Vec<Cue> = (0..40)
            .map(|i| Cue { start_ms: i * 7_919, text: "other".to_owned(), italic: false })
            .collect();

        let found = estimate_alignment(&got, &want);
        assert!(
            found.paired >= pair(&got, &want).0.len(),
            "never worse than not aligning"
        );
    }

    #[test]
    fn alignment_is_chosen_by_cues_paired_rather_than_by_the_score_it_produces() {
        // Choosing the best-scoring shift would let the search lower a CER by sliding the track
        // until the cues it read badly stopped pairing. Pairing counts cannot be gamed that way,
        // which is why they are the objective.
        let want = track(0, 50, "line");
        let got = track(5_000, 50, "line");

        let found = estimate_alignment(&got, &want);
        let shifted = rescale(&got, found.scale, found.offset_ms);
        assert_eq!(pair(&shifted, &want).0.len(), found.paired);
        assert_eq!(
            found.paired, 50,
            "the alignment keeps every cue rather than dropping the hard ones"
        );
    }

    #[test]
    fn releases_that_break_cues_differently_score_clean_as_one_transcript() {
        // The artifact this exists to remove. Both sides carry exactly the same words; they differ
        // only in where one cue ends and the next begins. Cue-level scoring charges that as errors
        // because it compares a cue against half its counterpart.
        let want = vec![
            Cue { start_ms: 0, text: "Yeah, I know".to_owned(), italic: false },
            Cue {
                start_ms: 2_000,
                text: "you have to stay calm".to_owned(),
                italic: false,
            },
        ];
        let got = vec![Cue {
            start_ms: 0,
            text: "Yeah, I know you have to stay calm".to_owned(),
            italic: false,
        }];

        let w = whole(&got, &want);
        assert_eq!(w.char_errors, 0, "the same words are the same words");
        assert_eq!(w.word_errors, 0);

        let cue_level = scored(&got, &want);
        assert!(
            cue_level.all.cer() > 0.0,
            "cue-level scoring charges the boundary as error"
        );
    }

    #[test]
    fn a_transcript_still_charges_words_that_were_genuinely_misread() {
        // The obvious way to get the test above to pass is to stop counting anything.
        let want = vec![Cue { start_ms: 0, text: "I know".to_owned(), italic: false }];
        let got = vec![Cue { start_ms: 0, text: "l know".to_owned(), italic: false }];

        let w = whole(&got, &want);
        assert_eq!(w.char_errors, 1, "the I/l substitution survives");
        assert_eq!(w.word_errors, 1);
    }

    #[test]
    fn a_transcript_ignores_timing_entirely() {
        // No pairing, so no tolerance and no alignment to get wrong.
        let want = vec![Cue { start_ms: 0, text: "hello there".to_owned(), italic: false }];
        let got = vec![Cue {
            start_ms: 9_999_999,
            text: "hello there".to_owned(),
            italic: false,
        }];

        assert_eq!(whole(&got, &want).char_errors, 0);
    }

    #[test]
    fn a_word_is_wrong_however_many_of_its_characters_are() {
        // The whole reason both rates are reported. Three characters wrong inside one word is three
        // character errors and one word error, so the two rates answer different questions and
        // neither substitutes for the other.
        assert_eq!(edit("catastrophe", "cotostrophe"), 2);
        assert_eq!(edit_words("catastrophe", "cotostrophe"), 1);
    }

    #[test]
    fn one_error_per_word_costs_far_more_words_than_characters() {
        // The distribution CER cannot see: the same handful of wrong characters spread one to a
        // word wrecks every word, and a summary quoting only CER would call this a good read.
        let want = "the cat sat on the mat";
        let got = "tho cot sot on tho mot";
        assert_eq!(edit(got, want), 5);
        assert_eq!(edit_words(got, want), 5, "five of six words carry an error");
    }

    #[test]
    fn a_word_moved_across_a_line_break_costs_no_words() {
        // The same discount the quoted CER takes, for the same reason: the two releases wrap
        // differently, and a break in another place is layout rather than a misread word.
        let a = "Just talk to me, okay?\nI can't believe you left.";
        let b = "Just talk to me,\nokay? I can't believe you left.";
        assert_eq!(edit_words(&flatten(a), &flatten(b)), 0);
    }

    #[test]
    fn a_per_character_rate_divides_errors_by_that_characters_occurrences() {
        // A raw count ranks by how common a character is; a rate ranks by how badly it reads. Here
        // `l` appears four times and is misread twice, and no count alone would say that.
        let mut census = Census::new();
        census.add("Iazy", "lazy", false);
        census.add("Iazy", "lazy", false);
        census.add("lazy", "lazy", false);
        census.add("lazy", "lazy", false);

        let rates = census.rates(0);
        let l = rates
            .iter()
            .find(|(c, ..)| *c == 'l')
            .expect("`l` was scored");
        assert_eq!((l.1, l.2), (4, 2), "seen four times, wrong twice");
    }

    #[test]
    fn an_inserted_character_is_not_charged_to_the_character_it_reads_as() {
        // An insertion is a character the release never had, so it has no occurrence count to
        // divide by. Charging it to itself would invent a denominator and report a rate over a
        // character the release never used.
        let mut census = Census::new();
        census.add("laazy", "lazy", false);

        assert_eq!(census.insertions.totals(), [1, 0], "the extra `a` is an insertion");
        let a = census.rates(0).into_iter().find(|(c, ..)| *c == 'a');
        assert_eq!(a, Some(('a', 1, 0)), "`a` occurs once and was read correctly");
    }

    #[test]
    fn a_character_seen_too_few_times_to_rank_is_held_back_by_the_floor() {
        // A character used twice and missed once is a 50% rate carrying no information, and a table
        // sorted by rate would put it above every character that actually costs the read.
        let mut census = Census::new();
        census.add("q", "q", false);

        assert!(census.rates(0).iter().any(|(c, ..)| *c == 'q'));
        assert!(census.rates(30).iter().all(|(c, ..)| *c != 'q'));
    }

    #[test]
    fn the_census_splits_the_two_populations_the_score_splits() {
        // Upright and italic read fifteen points apart on real material, so a table that pooled
        // them would attribute the italic act's confusions to the whole track.
        let mut census = Census::new();
        census.add("Iazy", "lazy", false);
        census.add("Iazy", "lazy", true);
        census.add("Iazy", "lazy", true);
        let ranked = census.substitutions.ranked();
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].0, ('l', 'I'));
        assert_eq!(ranked[0].1, [1, 2], "one upright, two italic");
    }

    #[test]
    fn a_space_is_named_rather_than_printed_as_itself() {
        // An unquoted space in a table cell is invisible, and spaces are the bucket #49 is asking
        // about.
        assert_eq!(show(' '), "space");
        assert_eq!(show(REPLACEMENT), "unread");
        assert_eq!(show('l'), "l");
    }
}
