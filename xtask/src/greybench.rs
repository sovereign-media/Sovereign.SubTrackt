//! What grey coverage is worth on a real disc, which is the one thing it was never asked.
//!
//! [#235](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/235). `docs/glyph-stability.md`
//! measured the representation two ways — `xtask measure-stability` for variance and `xtask accuracy`
//! for a fixture's character error — and shipped it off. Neither instrument reads a disc, and
//! `scripts/bench/run.py` did not exist yet, so the third question was never put.
//!
//! Two things make it worth putting again rather than merely worth recording. The cost it was
//! refused for was a **case collapse** — `o` to `O`, `u` to `U`, named in the document rather than
//! inferred — and #37's line-metric term, which is what decides case, landed afterwards. And the
//! benefit it was measured to give is a third off sensitivity to *rendering size*, which is what
//! the two VOBSUB tracks added by #140 need most: they fit at 16.1 and 15.2 against 10.0 to 11.8
//! for every PGS track on the bench.
//!
//! **Both sides or neither.** A reference set generated through one normalisation and a disc read
//! through the other are compared under different transforms and every distance is meaningless —
//! the trap the original measurement records having to avoid. So this takes two sets and pairs each
//! with its own setting; passing the same set twice is an error rather than a control.

use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt::{Config, Pipeline, UnmatchedPolicy};

use crate::disc;

/// Run the pipeline both ways and price the difference.
///
/// # Errors
/// Fails if the media, either reference set or the release subtitle cannot be read, or if an
/// extraction fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: grey-bench <media> <binary.subtref> <grey.subtref> <release.srt>")?
        .into();
    let binary_set: PathBuf = args
        .get(1)
        .context("missing the binary reference set")?
        .into();
    let grey_set: PathBuf = args
        .get(2)
        .context("missing the grey reference set")?
        .into();
    let release = args.get(3).context("missing the release subtitle")?;

    if binary_set == grey_set {
        anyhow::bail!(
            "the two reference sets are the same file; each representation needs a set generated \
             through its own normalisation, or every distance is meaningless"
        );
    }

    let want = disc::read(release)?;
    println!(
        "  {} against {} / {}",
        media.display(),
        binary_set.display(),
        grey_set.display()
    );
    println!(
        "  {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "coverage", "CER", "upright", "italic", "unread", "better", "worse"
    );

    let mut baseline: Option<Vec<disc::Cue>> = None;
    for (grey, set) in [(false, &binary_set), (true, &grey_set)] {
        let reference = crate::util::load_reference(set)?;
        let config = Config {
            unmatched: UnmatchedPolicy::Placeholder,
            grey_coverage: grey,
            ..Config::default()
        };

        let outcome = Pipeline::new(config.clone())
            .with_reference(reference)
            .run(&media)
            .with_context(|| format!("extracting with grey coverage {grey}"))?;

        let text = outcome.render(&config)?;
        let got = disc::parse(&text);
        let scored = disc::scored(&got, &want);
        // Against the binary row rather than against the previous one, which is the same choice
        // `width-sweep` makes: what a reader wants is what the representation costs relative to the
        // one that ships.
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
            "  {:>8} {:>7.1}% {:>7.1}% {:>7.1}% {:>8} {:>8} {:>8}",
            if grey { "grey" } else { "binary" },
            scored.all.cer(),
            scored.upright.cer(),
            scored.italic.cer(),
            outcome.report.unmatched,
            better,
            worse
        );
    }
    Ok(())
}
