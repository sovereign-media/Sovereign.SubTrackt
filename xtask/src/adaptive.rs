//! Reading a track twice, the second time against templates cut from its own ink.
//!
//! [#233](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/233). `glyph-stability.md`
//! closes its representation experiments with a bound rather than a fix:
//!
//! > The dominant term is sensitivity to *shape*: one pixel of extra weight on a stroke, which no
//! > representation choice can argue away because the shape really is different. So there is no
//! > fixed vector per character that survives the variation, and no way to normalise the variation
//! > away before matching.
//!
//! Both halves are right, and together they say the answer is not a better normalisation. A
//! template cut from *this disc* does not have the ±1px weight term at all, because it was cut at
//! this disc's weight. That is Tesseract's adaptive classifier, and it is a two-pass structure.
//!
//! ## Not #10, and the objection that killed #10 does not apply
//!
//! #10 grouped a stream's **unlabelled** shapes by proximity and died on a measurement: `l`, `I` and
//! `|` sit at distance zero, so no radius groups a stream's variation without merging characters
//! that were never distinguishable. That is an objection to a **radius over unlabelled shapes**. A
//! template promoted from a first-pass read carries a label and has no radius; it is compared to an
//! observation exactly the way a rendered entry is. `xtask shape-votes` made the same distinction.
//!
//! ## The seeding rule, which is the whole safety argument
//!
//! Self-training amplifies a **systematic** first-pass error, and this project has a measured
//! example. From `glyph-hit-list.md`, on A Fish Called Wanda:
//!
//! > No threshold does better than ignoring the measurement. [...] the matcher is not hesitating
//! > between them; it is confident.
//!
//! A confident, wrong and *consistent* read is the worst possible seed: promote Wanda's `l` and the
//! second pass gets more confident about the same 428 errors while every statistic improves.
//!
//! So the rule is **structural rather than a threshold**: a shape may become a template only if the
//! character it was read as belongs to **no confusion set**. `w`, `k`, `g` and `R` have no near
//! neighbour at any weight; `l`, `I`, `1`, `|`, `o`/`O`/`0`, `c`/`C` and `s`/`S` are excluded by the
//! same table post-correction already carries. A template set that never learns the ambiguous pairs
//! can still fix the weight axis for the rest of the alphabet -- and cannot make the pairs worse,
//! because it never speaks about them.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt::{Config, Pipeline, UnmatchedPolicy};
use subtrackt_glyph::matcher::{HammingMatcher, MatchThresholds};
use subtrackt_glyph::reference::{ReferenceEntry, ReferenceSet, Style};

use crate::disc;

/// Characters no template may ever be labelled with.
///
/// The confusion sets `subtrackt-text` corrects within, spelled here rather than imported: this is
/// a bench asking what that table implies for a different stage, and `geometry.rs` records the same
/// reason for duplicating a threshold rather than widening it.
const CONFUSABLE: &str = "0Oo1Il|";

/// How many times a shape must have been read the same way before it may become a template.
///
/// One occurrence that was itself a misread becomes a template for that misread, which is the
/// failure this exists for -- the same argument `VocabularyRules::min_occurrences` records, in a
/// stage where a wrong entry is far more expensive because it competes for *every* glyph rather
/// than one word.
const MIN_OCCURRENCES: u64 = 3;

/// Read a track twice and price the difference.
///
/// # Errors
/// Fails if the media, the reference set or the release subtitle cannot be read, or if either pass
/// fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: adaptive <media> <reference.subtref> <release.srt> [--min-occurrences N]")?
        .into();
    let set: PathBuf = args.get(1).context("missing the reference set")?.into();
    let release = args.get(2).context("missing the release subtitle")?;
    let floor = match args.iter().position(|a| a == "--min-occurrences") {
        Some(at) => args
            .get(at + 1)
            .context("--min-occurrences needs a value")?
            .parse()
            .context("--min-occurrences takes a number")?,
        None => MIN_OCCURRENCES,
    };

    let reference = crate::util::load_reference(&set)?;
    let want = disc::read(release)?;
    let config = Config { unmatched: UnmatchedPolicy::Placeholder, ..Config::default() };

    // Pass one, and it is a *survey* rather than an extraction: what the harvest needs is every
    // glyph's inputs, and the pipeline only carries out its answers. `fit.rs` reads a track the
    // same way for the same reason.
    let survey = Pipeline::new(config.clone())
        .survey(&media, None)
        .with_context(|| format!("surveying {}", media.display()))?;
    let matcher = HammingMatcher::new(reference.clone(), MatchThresholds::default())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let learned = harvest(&survey, &matcher, floor);
    let mut entries = reference.entries().to_vec();
    let rendered = entries.len();
    entries.extend(learned.iter().cloned());
    let adapted = ReferenceSet::new(format!("{}+adapted", reference.name()), entries);

    println!("\n--- adaptive templates (#233) ---");
    println!("  {}", media.display());
    println!(
        "  {} glyphs surveyed, {} distinct shapes; {rendered} rendered entries, {} learned",
        survey.glyphs.len(),
        survey.distinct_shapes(),
        learned.len()
    );
    let mut by_char: BTreeMap<char, usize> = BTreeMap::new();
    for entry in &learned {
        *by_char.entry(entry.character).or_default() += 1;
    }
    println!(
        "  over {} distinct characters, {} at the floor of {floor}",
        by_char.len(),
        by_char.values().filter(|n| **n == 1).count()
    );

    println!(
        "\n  {:>10} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "set", "CER", "unread", "worse", "better", "cues"
    );
    let mut baseline: Option<Vec<disc::Cue>> = None;
    for (name, set) in [("rendered", &reference), ("adapted", &adapted)] {
        let outcome = Pipeline::new(config.clone())
            .with_reference(set.clone())
            .run(&media)
            .with_context(|| format!("extracting against the {name} set"))?;
        let text = outcome.render(&config)?;
        let got = disc::parse(&text);
        let scored = disc::scored(&got, &want);
        let (better, worse) = baseline.as_ref().map_or((0, 0), |before: &Vec<disc::Cue>| {
            let changes = disc::changes(before, &got, &want);
            (
                changes.iter().filter(|c| c.now < c.was).count(),
                changes.iter().filter(|c| c.now > c.was).count(),
            )
        });
        if baseline.is_none() {
            baseline = Some(got.clone());
        }
        println!(
            "  {name:>10} {:>7.1}% {:>8} {:>8} {:>8} {:>8}",
            scored.all.cer(),
            outcome.report.unmatched,
            worse,
            better,
            got.len()
        );
    }
    Ok(())
}

/// Every shape the first pass read confidently, as a labelled template.
///
/// One entry per distinct shape rather than a consensus per character, and that is deliberate: a
/// disc draws one letter several ways -- #10's own finding -- and averaging them back into a single
/// vector is the normalisation this whole idea exists to stop doing.
fn harvest(
    survey: &subtrackt::GlyphSurvey,
    matcher: &HammingMatcher,
    floor: u64,
) -> Vec<ReferenceEntry> {
    // Keyed on the exact shape, so occurrences of one rendering accumulate and a shape seen once is
    // distinguishable from one seen a hundred times.
    // Hashed rather than ordered: a feature vector is `Hash` and not `Ord`, which is the same
    // reason `cache.rs` keys its session cache the way it does.
    let mut seen: HashMap<
        subtrackt_core::FeatureVector,
        (
            u64,
            Option<char>,
            subtrackt_core::LineMetrics,
            subtrackt_core::InkAspect,
        ),
    > = HashMap::new();
    for glyph in &survey.glyphs {
        let answer = matcher.scan_with(&glyph.features, glyph.metrics, glyph.mark, glyph.aspect);
        // Confident and callable. An ambiguous read is by definition a shape the set could not
        // place, and promoting one would write the matcher's own hesitation into the set.
        let label = answer
            .character
            .filter(|_| answer.is_unambiguous(matcher.ambiguity_margin()))
            .filter(|ch| !CONFUSABLE.contains(*ch));
        // The measurements of the *first* occurrence, kept alongside. See `harvest`'s return for
        // why a learned entry must carry them rather than report unknown.
        let slot = seen
            .entry(glyph.features)
            .or_insert((0, label, glyph.metrics, glyph.aspect));
        slot.0 += 1;
        // A shape answered two different ways across the track is not a template of either. It
        // cannot happen through the matcher alone -- the answer is a function of the inputs -- but
        // the metrics and the mark differ per occurrence, so the same vector can be called twice.
        if slot.1 != label {
            slot.1 = None;
        }
    }

    seen.into_iter()
        .filter(|(_, (count, ..))| *count >= floor)
        .filter_map(|(features, (_, label, metrics, aspect))| {
            label.map(|character| ReferenceEntry {
                character,
                style: Style::Regular,
                features,
                // The occurrence's own measurements, and **not** unknown -- which is what the
                // first attempt did, and it is the defect that made the whole idea look dead.
                // `Weights::distance` omits a term when *either* side lacks it, so an entry
                // reporting unknown pays no metric and no width penalty at all: it competes on bare
                // Hamming distance while every rendered entry it is up against pays both. 1,205
                // such entries against 478 rendered ones took The Karate Kid from 1.7% to 5.6%.
                //
                // A shape is identical across its occurrences by construction, so its metrics are
                // near-identical too, and the first is a fair representative rather than an
                // average that belongs to no occurrence.
                metrics,
                mark: subtrackt_core::MarkSlope::NONE,
                aspect,
            })
        })
        .collect()
}
