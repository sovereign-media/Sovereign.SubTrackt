//! Does any setting of #40's two decisiveness constants make an ink gap beat a box gap?
//!
//! [#225](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/225). #219 measured that a
//! word gap is taken between bounding **boxes**, and that this understates the space in front of a
//! `j` by 29 points and a `T` by 46. #222 built the correction and it removed 69% of the defect on
//! the tracks that have it while making **600 cues worse** across the nine-track English bench.
//!
//! The diagnosis was not that the measurement is wrong. It is that the measurement and the rule
//! reading it are one system: `split_min_width_percent` at 50 and `split_min_cluster_percent` at
//! 200 were fitted against a distribution taken between boxes, and that distribution compressed the
//! gap around a full stop **by accident**. Take the compression out and the constants no longer sit
//! where the two populations separate.
//!
//! So this asks the question directly. Every setting on the grid, the whole pipeline at each, scored
//! against a release subtitle — the shape `xtask width-sweep` established, for the same reason it
//! established it: a set-internal statistic cannot see this, because the thing that moved is the
//! shape of a *line's* gap distribution rather than any distance in the reference set.
//!
//! ## The two columns that decide it
//!
//! **`worse` against the shipped setting**, which is the column `CLAUDE.md` says to read, and
//! **`glued`**, which is the only place a gain is visible at all. An English track has six glued
//! instances in 66,216 glyphs; Gone Girl's Swedish track has 80 and its Norwegian 62. A sweep that
//! reported only CER would be reading the cost of a change with no column for its benefit.

use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt::{Config, Pipeline, UnmatchedPolicy};

use crate::disc;

/// Cut-to-glyph-width floors to try, in percent.
///
/// The shipped 50 first, so the top row of every table is the setting in the tree. Upward from
/// there because the ink gap is *wider* than the box gap it replaces — a floor that was right for
/// boxes is by construction too low for ink, and #40 chose 50 against real word gaps running 0.61
/// to 1.60 of median glyph width. Past 120 the floor exceeds the narrowest real word gap on the
/// fixture and the rule would refuse a line that has one.
const WIDTHS: [u32; 6] = [50, 65, 80, 95, 110, 125];

/// Cut-to-low-cluster floors to try, in percent.
///
/// The shipped 200 first, for the same reason. #40 measured the fixture's tightest real break at
/// 220% of its cluster median, so the interesting region is what happens as the floor approaches
/// and passes it.
const CLUSTERS: [u32; 5] = [200, 250, 300, 400, 500];

/// One setting's outcome.
struct Row {
    width: u32,
    cluster: u32,
    shared: u32,
    cer: f64,
    worse: usize,
    better: usize,
    glued: usize,
}

/// How many times `word` is fused to the word in front of it.
///
/// The gain column, and it needs no sidecar — which is the whole reason it is here. The Swedish and
/// Norwegian tracks that show this defect have no scoreable sidecar in the library, so a CER for
/// them would be a number about the wrong thing. Two or more letters immediately before the word,
/// with no space between, is what `attjag` and `ifyou` are.
fn glued(text: &str, word: &str) -> usize {
    if word.is_empty() {
        return 0;
    }
    let mut count = 0;
    let haystack: Vec<char> = text.chars().collect();
    let needle: Vec<char> = word.chars().collect();
    for at in 0..haystack.len().saturating_sub(needle.len()) {
        if haystack[at..at + needle.len()] != needle[..] {
            continue;
        }
        // The word has to end here, or `jag` matches inside `jagade`.
        if haystack
            .get(at + needle.len())
            .is_some_and(|c| c.is_alphabetic())
        {
            continue;
        }
        // And at least two letters have to run into it.
        let letters = haystack[..at]
            .iter()
            .rev()
            .take_while(|c| c.is_alphabetic())
            .count();
        if letters >= 2 {
            count += 1;
        }
    }
    count
}

/// Parse a comma-separated list of settings, or fall back to the default grid.
fn grid(args: &[String], flag: &str, fallback: &[u32]) -> anyhow::Result<Vec<u32>> {
    match args.iter().position(|a| a == flag) {
        Some(at) => args
            .get(at + 1)
            .with_context(|| format!("{flag} needs a comma-separated list"))?
            .split(',')
            .map(|value| value.trim().parse().context("a setting is a percentage"))
            .collect(),
        None => Ok(fallback.to_vec()),
    }
}

/// Extract once at one setting.
fn extract(
    media: &PathBuf,
    reference: &subtrackt_glyph::ReferenceSet,
    band_gaps: bool,
    width: u32,
    cluster: u32,
    shared: u32,
) -> anyhow::Result<(Vec<disc::Cue>, String)> {
    let mut config = Config { unmatched: UnmatchedPolicy::Placeholder, ..Config::default() };
    config.layout.band_gaps = band_gaps;
    config.layout.split_min_width_percent = width;
    config.layout.split_min_cluster_percent = cluster;
    config.layout.band_gap_min_shared = shared;

    let outcome = Pipeline::new(config.clone())
        .with_reference(reference.clone())
        .run(media)
        .with_context(|| format!("extracting at {width}/{cluster}/{shared}"))?;
    let text = outcome.render(&config)?;
    let cues = disc::parse(&text);
    Ok((cues, text))
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context(
            "usage: gap-sweep <media> <reference.subtref> [release.srt] \
             [--glued WORD] [--widths a,b] [--clusters a,b] [--shared a,b]",
        )?
        .into();
    let set: PathBuf = args.get(1).context("missing the reference set")?.into();
    let release = args.get(2).filter(|a| !a.starts_with("--"));
    let word = args
        .iter()
        .position(|a| a == "--glued")
        .and_then(|at| args.get(at + 1))
        .cloned()
        .unwrap_or_default();

    let widths = grid(args, "--widths", &WIDTHS)?;
    let clusters = grid(args, "--clusters", &CLUSTERS)?;
    let shares = grid(args, "--shared", &[1, 2])?;

    let reference = crate::util::load_reference(&set)?;
    let want = match release {
        Some(path) => disc::read(path)?,
        None => Vec::new(),
    };

    // The baseline is the box gap at the shipped constants, because that is what a change is
    // measured against — not the best row of the sweep, and not the same constants with bands on.
    let (baseline, baseline_text) = extract(&media, &reference, false, 50, 200, 1)?;
    let baseline_scored = (!want.is_empty()).then(|| disc::scored(&baseline, &want).all.cer());
    println!("  {} against {}", media.display(), set.display());
    println!(
        "  baseline: boxes at 50/200, CER {}, glued {}",
        baseline_scored.map_or_else(|| "--".to_owned(), |cer| format!("{cer:.1}%")),
        glued(&baseline_text, &word)
    );
    println!(
        "\n  {:>6} {:>8} {:>7} {:>8} {:>8} {:>8}",
        "shared", "width", "cluster", "CER", "worse", "glued"
    );

    let mut rows = Vec::new();
    for shared in &shares {
        for width in &widths {
            for cluster in &clusters {
                let (cues, text) = extract(&media, &reference, true, *width, *cluster, *shared)?;
                let changes = disc::changes(&baseline, &cues, &want);
                let row = Row {
                    width: *width,
                    cluster: *cluster,
                    shared: *shared,
                    cer: if want.is_empty() {
                        0.0
                    } else {
                        disc::scored(&cues, &want).all.cer()
                    },
                    worse: changes.iter().filter(|c| c.now > c.was).count(),
                    better: changes.iter().filter(|c| c.now < c.was).count(),
                    glued: glued(&text, &word),
                };
                println!(
                    "  {:>6} {:>8} {:>7} {:>7.1}% {:>8} {:>8}",
                    row.shared, row.width, row.cluster, row.cer, row.worse, row.glued
                );
                rows.push(row);
            }
        }
    }

    // The question is whether *any* row beats the baseline, so the best row by the column that
    // decides is worth naming rather than leaving a reader to scan a grid for it.
    if let Some(best) = rows.iter().min_by_key(|row| row.worse) {
        println!(
            "\n  fewest worse cues: {} at {}/{} shared {} -- {} better, glued {} against the \
             baseline's {}",
            best.worse,
            best.width,
            best.cluster,
            best.shared,
            best.better,
            best.glued,
            glued(&baseline_text, &word)
        );
    }
    for note in [
        "",
        "  `worse` counts cues the setting made worse **than the shipped box measurement**, which is",
        "  the column CLAUDE.md says to read. A row is only interesting at zero: anything above it is",
        "  a change that costs cues on a track this bench can score, and `glued` is the only column",
        "  that can show what it buys.",
    ] {
        println!("{note}");
    }
    Ok(())
}
