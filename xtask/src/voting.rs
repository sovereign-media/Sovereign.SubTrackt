//! Does one shape ever get two answers?
//!
//! Candidate E of [#97](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/97), held
//! there on the grounds that "the bench is a count, so if it comes back small the count is the
//! whole write-up and there is nothing to build". This is that count.
//!
//! The session cache is keyed by [shape + metrics + mark](subtrackt_glyph::cache), so one shape
//! vector appearing on lines with different metrics is scanned separately and can land on different
//! characters. The proposal is to aggregate those decisions across identical vectors and let the
//! majority answer.
//!
//! It is explicitly **not** #10. The objection that killed clustering — `I`, `l` and `|` sit at
//! distance *zero*, so no radius groups a stream's variation without merging characters that were
//! never distinguishable — cannot apply, because the radius here is exactly zero and no two
//! distinct reference entries are ever merged. It aggregates decisions about *the same observed
//! vector*, not shapes that resemble each other.
//!
//! Which is also why the count is the whole question. If a shape never gets two answers there is
//! nothing to aggregate; if it does, the interesting number is how often the majority would differ
//! from what the glyph actually got.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt::{Config, Pipeline};
use subtrackt_core::{FeatureVector, LineMetrics, MarkSlope};
use subtrackt_glyph::ReferenceSet;
use subtrackt_glyph::matcher::{HammingMatcher, MatchThresholds};

/// One distinct (shape, metrics, mark) key and what it was answered with.
struct Keyed {
    metrics: LineMetrics,
    mark: MarkSlope,
    /// How many glyphs in the stream carried this exact key.
    glyphs: usize,
    answer: Option<char>,
}

/// Count how many distinct shape vectors receive more than one answer.
///
/// # Errors
/// Fails if the file or the reference set cannot be read, or if the survey fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: shape-votes <media> <reference.subtref>")?
        .into();
    let set: PathBuf = args.get(1).context("missing the reference set")?.into();

    let reference =
        ReferenceSet::decode(&std::fs::read(&set)?).map_err(|e| anyhow::anyhow!("{e}"))?;
    // The survey rather than an extraction, because what is wanted is every glyph's *inputs* —
    // shape, metrics and mark — and the pipeline only carries out its answers.
    let survey = Pipeline::new(Config::default())
        .survey(&media, None)
        .with_context(|| format!("surveying {}", media.display()))?;

    let matcher = HammingMatcher::new(reference, MatchThresholds::default())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Grouped by shape, then by the whole cache key inside it. A `BTreeMap` keyed on the raw words
    // so the grouping is exact rather than by a hash that could collide.
    let mut by_shape: BTreeMap<[u64; 4], Vec<Keyed>> = BTreeMap::new();
    for glyph in &survey.glyphs {
        let entry = by_shape.entry(*glyph.features.words()).or_default();
        match entry
            .iter_mut()
            .find(|k| k.metrics == glyph.metrics && k.mark == glyph.mark)
        {
            Some(keyed) => keyed.glyphs += 1,
            None => entry.push(Keyed {
                metrics: glyph.metrics,
                mark: glyph.mark,
                glyphs: 1,
                answer: None,
            }),
        }
    }

    // One scan per distinct cache key, which is exactly what the runtime does.
    let mut scans = 0usize;
    for (words, keys) in &mut by_shape {
        let features = FeatureVector::from_words(*words);
        for keyed in keys.iter_mut() {
            keyed.answer = matcher
                .scan_with(&features, keyed.metrics, keyed.mark)
                .character;
            scans += 1;
        }
    }

    report(&by_shape, scans, survey.glyphs.len());
    Ok(())
}

/// Rows listed before the remainder is summarised.
const ROWS: usize = 20;

fn report(by_shape: &BTreeMap<[u64; 4], Vec<Keyed>>, scans: usize, glyphs: usize) {
    let split: Vec<(&[u64; 4], &Vec<Keyed>)> = by_shape
        .iter()
        .filter(|(_, keys)| {
            let mut answers: Vec<Option<char>> = keys.iter().map(|k| k.answer).collect();
            answers.sort_unstable();
            answers.dedup();
            answers.len() > 1
        })
        .collect();

    println!(
        "  {glyphs} glyphs, {} distinct shapes, {scans} distinct cache keys",
        by_shape.len()
    );
    println!(
        "\n--- shapes that received more than one answer (#97 candidate E) ---\n  {} of {} \
         distinct shapes",
        split.len(),
        by_shape.len()
    );
    if split.is_empty() {
        println!("  none. There is nothing for a vote to aggregate, and E closes on this count.");
        return;
    }

    // The number that decides it is not how many shapes split, but how many *glyphs* sit on the
    // losing side of one -- that is the most a majority vote could ever recover.
    let mut recoverable = 0usize;
    println!(
        "\n  {:>8} {:>10}  the answers this shape received",
        "glyphs", "majority"
    );
    for (index, (_, keys)) in split.iter().enumerate() {
        let total: usize = keys.iter().map(|k| k.glyphs).sum();
        let mut tally: BTreeMap<Option<char>, usize> = BTreeMap::new();
        for keyed in *keys {
            *tally.entry(keyed.answer).or_insert(0) += keyed.glyphs;
        }
        let winner = tally.iter().max_by_key(|(_, count)| **count);
        let majority = winner.map_or(0, |(_, count)| *count);
        recoverable += total - majority;
        if index < ROWS {
            let listed: Vec<String> = tally
                .iter()
                .map(|(answer, count)| {
                    format!("{} x{count}", answer.map_or_else(|| "unread".to_owned(), String::from))
                })
                .collect();
            println!(
                "  {total:>8} {:>10}  {}",
                winner
                    .and_then(|(answer, _)| *answer)
                    .map_or_else(|| "unread".to_owned(), String::from),
                listed.join("   ")
            );
        }
    }
    if split.len() > ROWS {
        println!("  ... {} more shapes not listed", split.len() - ROWS);
    }

    println!(
        "\n  {recoverable} glyphs of {glyphs} sit on the losing side of a split, which is the most\n  \
         a majority vote could move -- and only if the majority is right every time."
    );
}
