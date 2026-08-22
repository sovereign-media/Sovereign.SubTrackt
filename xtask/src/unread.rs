//! Naming the glyphs the matcher would not call, rather than counting them.
//!
//! #98. The extraction report has always ended in a bare `N unmatched`, and every proposal about
//! what to do with those N has been ranked on inference because nothing said what they *are*. The
//! sentence "11 of the 13 unmatched in the ceiling fixture are punctuation" appeared in a document
//! with no instrument behind it; this is that instrument.
//!
//! The columns describe each unread component's geometry, and the honest caveat is that **none of
//! them classifies it**. The first version of this tool read a component wider than it was tall as
//! a fusion and everything else as punctuation, and that was wrong: a fused `rt` at 1080p is 42x44,
//! *square*, and 73% of its line's cap height, because `r` and `t` are both narrow. Width against
//! the cap height is no better — the same disc's fusions run from 73% to 200% of it.
//!
//! What settles a component is reading the text where it sat, and the size groups are what make
//! that cheap: forty-one components of exactly 42x44, each 82 cells from the nearest entry in the
//! material's own typeface, are forty-one instances of one recurring *pair*. `docs/error-census.md`
//! has the record and the correction.

use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt::{Config, Pipeline, UnmatchedPolicy, UnreadGlyph};
use subtrackt_glyph::ReferenceSet;

/// Distance past which an unread component is not a character this typeface draws.
///
/// Only a reading aid for the table. `docs/error-census.md` records why no geometric column decides
/// this: on a real disc every unread component but the full stops turned out to be a fusion, and
/// they span 73% to 200% of their line's cap height — `rt` is *narrower* than the cap height
/// because both letters are narrow, and `ry` is square. Width does not separate them and neither
/// does aspect ratio. What does is reading the text where they sat.
const MIN_TELLING_DISTANCE: u32 = 55;

/// Sizes listed before the remainder is summarised.
///
/// The remainder is *stated*. A table that stops at twenty rows with no tail line reads as a
/// complete census, which is the kind of quiet truncation this whole issue exists to replace.
const ROWS: usize = 20;

/// Print one row per distinct unread component size, most frequent first.
pub(crate) fn table(unread: &[UnreadGlyph]) {
    println!("\n--- unread glyphs (#98) ---");
    if unread.is_empty() {
        println!("  every glyph matched something");
        return;
    }

    // Grouped by size rather than listed one per glyph: the same unread character recurs across a
    // track, and 958 rows saying the same thing would bury the handful that do not.
    let mut kinds: Vec<Kind> = Vec::new();
    for glyph in unread {
        let key = (glyph.bounds.width, glyph.bounds.height, glyph.metrics.known);
        match kinds.iter_mut().find(|k| k.key() == key) {
            Some(kind) => kind.add(glyph),
            None => kinds.push(Kind::new(glyph)),
        }
    }
    kinds.sort_by_key(|k| std::cmp::Reverse(k.count));

    let without_metrics = unread.iter().filter(|g| !g.metrics.known).count();
    println!(
        "  {} unread glyphs in {} distinct sizes; {without_metrics} sat on a line with no metrics",
        unread.len(),
        kinds.len()
    );
    println!(
        "  a size that recurs is worth reading as one recurring *pair*: no single character from"
    );
    println!(
        "  the material's own typeface should sit {MIN_TELLING_DISTANCE}+ cells from its own entry."
    );
    println!(
        "  {:>6} {:>9} {:>7} {:>9} {:>9} {:>9}  first seen",
        "count", "w x h", "aspect", "w / cap", "nearest", "metrics"
    );
    for kind in kinds.iter().take(ROWS) {
        println!(
            "  {:>6} {:>9} {:>7.2} {:>9} {:>9} {:>9}  cue {} line {} at ({}, {})",
            kind.count,
            format!("{}x{}", kind.width, kind.height),
            aspect(kind.width, kind.height),
            match (kind.width_over_cap, kind.widest_over_cap) {
                (Some(low), Some(high)) if low != high => format!("{low}-{high}%"),
                (Some(low), _) => format!("{low}%"),
                _ => "-".to_owned(),
            },
            kind.nearest,
            if kind.metrics_known {
                "measured"
            } else {
                "unknown"
            },
            kind.first.cue,
            kind.first.line,
            kind.first.bounds.x,
            kind.first.bounds.y
        );
    }
    if kinds.len() > ROWS {
        let tail: usize = kinds.iter().skip(ROWS).map(|k| k.count).sum();
        println!("  {tail:>6} in {} sizes not listed", kinds.len() - ROWS);
    }
}

/// One distinct unread component size.
struct Kind {
    width: u32,
    height: u32,
    metrics_known: bool,
    count: usize,
    /// The closest any instance of this size came to a reference entry.
    ///
    /// A glyph rejected just past the 51-cell ceiling is a threshold question; one rejected at
    /// three times the ceiling is not in the set at all, and the count alone cannot tell them
    /// apart.
    nearest: u32,
    /// Width as a percentage of the line's cap height, or `None` if the line was unmeasurable.
    ///
    /// The column that decides #97's candidate C, and aspect ratio is not. A fused `rt` at 1080p is
    /// 42x44 — square — because `r` and `t` are both narrow, so a wider-than-tall test misses the
    /// most common fusion there is. Width against the line's own cap height is the scale-free
    /// measure the split rule would actually use: no single character in this charset is much wider
    /// than its cap height, so a component past about 110% is two characters.
    width_over_cap: Option<u32>,
    /// The widest any instance of this size reached, which is the one that decides it.
    widest_over_cap: Option<u32>,
    first: UnreadGlyph,
}

impl Kind {
    fn new(glyph: &UnreadGlyph) -> Self {
        Self {
            width: glyph.bounds.width,
            height: glyph.bounds.height,
            metrics_known: glyph.metrics.known,
            count: 1,
            nearest: glyph.distance,
            width_over_cap: width_over_cap(glyph),
            widest_over_cap: width_over_cap(glyph),
            first: *glyph,
        }
    }

    const fn key(&self) -> (u32, u32, bool) {
        (self.width, self.height, self.metrics_known)
    }

    fn add(&mut self, glyph: &UnreadGlyph) {
        self.count += 1;
        self.nearest = self.nearest.min(glyph.distance);
        // One size can sit on lines of different cap height -- an italic act is set smaller -- so
        // the ratio is a range rather than a value, and the widest is what says whether the
        // component is two characters.
        self.widest_over_cap = self.widest_over_cap.max(width_over_cap(glyph));
        self.width_over_cap = match (self.width_over_cap, width_over_cap(glyph)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }
}

/// Width as a percentage of the line's cap height.
///
/// `metrics.height_percent` is the glyph's height as a percentage of that cap height, so the cap
/// height in pixels is recoverable from the two — which is the only way to get a scale-free width
/// out of what an unread glyph carries.
fn width_over_cap(glyph: &UnreadGlyph) -> Option<u32> {
    if !glyph.metrics.known || glyph.metrics.height_percent == 0 {
        return None;
    }
    let cap = glyph.bounds.height * 100 / glyph.metrics.height_percent;
    (cap > 0).then(|| glyph.bounds.width * 100 / cap)
}

/// Width over height, the ratio that separates a fusion from shattered punctuation.
#[allow(clippy::cast_precision_loss)]
fn aspect(width: u32, height: u32) -> f64 {
    if height == 0 {
        return 0.0;
    }
    f64::from(width) / f64::from(height)
}

/// Run a real file through the pipeline and name what it could not read.
///
/// The ceiling fixture answers this too — `xtask accuracy` prints the same table — but a fixture is
/// rendered by this repository at one size in one font, and #97's candidate C is a claim about a
/// disc. Thirteen unread glyphs in a synthetic paragraph cannot settle what 958 on a Blu-ray are.
///
/// # Errors
/// Fails if the file or the reference set cannot be read, or if extraction fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: unread <media> <reference.subtref>")?
        .into();
    let set: PathBuf = args.get(1).context("missing the reference set")?.into();

    let reference =
        ReferenceSet::decode(&std::fs::read(&set)?).map_err(|e| anyhow::anyhow!("{e}"))?;
    // Placeholder rather than the default gate, for the same reason `xtask accuracy` uses it: a
    // policy that refuses the track would leave nothing to census.
    let config = Config { unmatched: UnmatchedPolicy::Placeholder, ..Config::default() };
    let outcome = Pipeline::new(config)
        .with_reference(reference)
        .run(&media)
        .with_context(|| format!("extracting {}", media.display()))?;

    println!(
        "{}: {} cues, {} glyphs, {} matched, {} unread",
        media.display(),
        outcome.report.cues,
        outcome.report.glyphs,
        outcome.report.matched,
        outcome.report.unmatched
    );
    table(&outcome.unread);
    Ok(())
}
