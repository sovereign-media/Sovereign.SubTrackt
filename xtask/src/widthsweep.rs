//! What the width term is worth, priced against a real disc.
//!
//! [#110](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/110). Every other weight in
//! the matcher was chosen by a sweep and this one has to be too, but it cannot be swept the way
//! #37's and #48's were: `xtask metric-sweep` measures separation *inside a reference set*, and
//! `docs/glyph-stability.md` records the lesson that came of trusting that — "ambiguous" and
//! "wrong" are not the same set. The pair this term exists for sits at distance zero in the set, so
//! a set-internal statistic would say it was fixed the moment the weight was non-zero.
//!
//! So the sweep runs the whole pipeline at each setting and scores the result against a release
//! subtitle. That is affordable because of `xtask dump-sup`: a feature film's subtitle track is
//! sixteen megabytes and reads in a tenth of a second, so a twelve-point sweep is seconds rather
//! than the twenty minutes twelve passes over a 5.5 GB rip would cost.
//!
//! The two numbers that decide it are printed side by side, because the whole question is whether a
//! window exists where both are good at once: how much of `l` → `I` the setting recovers, and what
//! it costs everything else. A term strong enough to separate a nine-thousandths-of-cap-height
//! difference is also strong enough to reject a `w` the disc draws four percent wider than the font
//! does, and only the disc can say where between those the setting sits.

use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt::{Config, Pipeline, UnmatchedPolicy};

use crate::disc;

/// The settings swept, in tenths of a percent of the feature vector per full cap height.
///
/// Spread over two orders of magnitude because the arithmetic says the interesting region is not
/// where the other weights live. #37's metric term is priced at 196 for differences counted in
/// whole percentage points; this one is priced for differences counted in tenths, and the `l`/`I`
/// gap is **eight** of them — so anything under about 340 cannot move a single cell and the term is
/// off however non-zero it looks. The measured window is 340 to 540; `--weights` narrows onto it.
const WEIGHTS: [u32; 12] = [
    0, 196, 440, 700, 1000, 1500, 2000, 3000, 4000, 6000, 9000, 14000,
];

/// Run the pipeline once per setting and score each result.
///
/// # Errors
/// Fails if the media, the reference set or the release subtitle cannot be read, or if an
/// extraction fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: width-sweep <media> <reference.subtref> <release.srt> [--post-correct]")?
        .into();
    let set: PathBuf = args.get(1).context("missing the reference set")?.into();
    let release = args.get(2).context("missing the release subtitle")?;
    let post_correct = args.iter().any(|a| a == "--post-correct");
    // The default grid spans two orders of magnitude to find the window at all; refining inside it
    // is what `--weights` is for, and a sweep that could not be narrowed would report the window's
    // existence and never its edges.
    let weights: Vec<u32> = match args.iter().position(|a| a == "--weights") {
        Some(at) => args
            .get(at + 1)
            .context("--weights needs a comma-separated list")?
            .split(',')
            .map(|w| {
                w.trim()
                    .parse()
                    .context("a weight is a number of tenths of a percent")
            })
            .collect::<anyhow::Result<Vec<u32>>>()?,
        None => WEIGHTS.to_vec(),
    };

    let reference = crate::util::load_reference(&set)?;
    let want = disc::read(release)?;

    println!(
        "  {} against {}, post-correction {}",
        media.display(),
        set.display(),
        if post_correct { "on" } else { "off" }
    );
    println!(
        "  {:>8} {:>8} {:>8} {:>8} {:>9} {:>8} {:>8}",
        "weight", "CER", "upright", "italic", "l -> I", "unread", "worse"
    );

    let mut baseline: Option<Vec<disc::Cue>> = None;
    for weight in weights {
        let mut config = Config {
            unmatched: UnmatchedPolicy::Placeholder,
            post_correct,
            ..Config::default()
        };
        // Both weights, together. Grouping decides which glyphs get one answer, so a sweep that
        // moved the matcher's weight and left the clusterer's would be measuring a pipeline that
        // merged the pair before the term it is sweeping was ever asked.
        config.matching.width_weight_permille = weight;
        config.clustering.width_weight_permille = weight;

        let outcome = Pipeline::new(config)
            .with_reference(reference.clone())
            .run(&media)
            .with_context(|| format!("extracting at weight {weight}"))?;

        let text = outcome.render(&config)?;
        let got = disc::parse(&text);
        let scored = disc::scored(&got, &want);
        // Against the setting that is off rather than against the previous row: what a reader
        // wants is what the term costs relative to not having it, and `docs/post-correction.md`
        // makes that column the one a change is judged on.
        let worse = baseline.as_ref().map_or(0, |before: &Vec<disc::Cue>| {
            disc::changes(before, &got, &want)
                .iter()
                .filter(|c| c.now > c.was)
                .count()
        });
        if baseline.is_none() {
            baseline = Some(got.clone());
        }

        println!(
            "  {weight:>8} {:>7.1}% {:>7.1}% {:>7.1}% {:>9} {:>8} {:>8}",
            scored.all.cer(),
            scored.upright.cer(),
            scored.italic.cer(),
            disc::confusions(&got, &want, 'l', 'I'),
            outcome.report.unmatched,
            worse
        );
    }
    Ok(())
}
