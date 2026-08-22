//! Which typeface should the binary embed a reference set for?
//!
//! [#9](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/9) has an answer waiting on a
//! measurement it could not make: `xtask accuracy` builds its fixture and its reference set from
//! the *same* font, which is the ceiling case and says nothing about the case a user actually gets.
//! An embedded set is by definition built from a typeface that is not the one the disc was authored
//! in, and the only question that matters is how much that costs.
//!
//! So this renders one fixture from a *material* font — Arial, or whatever #8's fit identified for
//! the library in question — and scores several candidate reference sets against it. The material
//! font scores itself as the ceiling, and every candidate is read against that rather than against
//! zero.
//!
//! The licensing question rides on the same number. Fitting identified Arial, which is not
//! permission to derive from it, so the open metric-compatible substitutes have to be measured
//! before anything is embedded rather than assumed to be interchangeable.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use subtrackt::score::{Score, score_text};
use subtrackt_glyph::ReferenceSet;

/// One candidate reference set, scored against the material fixture.
struct Fit {
    /// What the set was generated from.
    name: String,
    /// How the extraction scored against ground truth.
    score: Score,
    /// Glyphs identified within threshold.
    matched: u64,
    /// Glyphs with no reference within threshold.
    unmatched: u64,
    /// Mean distance of the glyphs that did match.
    mean_distance: f32,
    /// The extracted text, so the error classes can be shown rather than only counted.
    text: String,
}

impl Fit {
    /// Fraction of glyphs the matcher identified, which is what the accuracy gate reads.
    fn coverage(&self) -> f64 {
        let total = self.matched + self.unmatched;
        if total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.matched as f64 / total as f64
        }
    }
}

/// Build a reference set from `font` and score it against the fixture at `sup`.
fn fit(font: &Path, sup: &Path, dir: &Path, truth: &str) -> anyhow::Result<Fit> {
    let name = font
        .file_stem()
        .map_or_else(|| "unnamed".to_owned(), |s| s.to_string_lossy().into_owned());
    let path = dir.join(format!("fit-{name}.subtref"));

    crate::gen_reference(&[
        font.display().to_string(),
        path.display().to_string(),
        "--name".to_owned(),
        name.clone(),
    ])?;

    let reference =
        ReferenceSet::decode(&std::fs::read(&path)?).map_err(|e| anyhow::anyhow!("{e}"))?;
    let (text, outcome) = crate::accuracy::extract(sup, reference, false, false)?;

    Ok(Fit {
        score: score_text(truth, text.trim()),
        matched: outcome.report.matched,
        unmatched: outcome.report.unmatched,
        mean_distance: outcome.report.mean_match_distance(),
        text,
        name,
    })
}

/// Render a fixture from one font and score reference sets built from several others.
///
/// # Errors
/// Fails if a font is unreadable, or if any stage of generation or extraction fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let material = PathBuf::from(
        args.first()
            .context("usage: reference-fit <material.ttf> <candidate.ttf>...")?,
    );
    if !material.exists() {
        bail!("{} does not exist", material.display());
    }
    let candidates: Vec<PathBuf> = args[1..]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    if candidates.is_empty() {
        bail!("no usable candidate fonts; pass at least one alongside the material font");
    }

    let dir = std::env::temp_dir().join("subtrackt-reference-fit");
    std::fs::create_dir_all(&dir)?;

    println!("material: {}", material.display());
    crate::fixture::make(&[material.display().to_string(), dir.display().to_string()])?;
    let truth = std::fs::read_to_string(dir.join("synthetic.txt"))?;
    let truth = truth.trim();
    let sup = dir.join("synthetic.sup");

    // The material font scores itself first. Every candidate is read against that ceiling rather
    // than against zero, because the pipeline's own remaining errors — spacing, punctuation — are
    // in all of these numbers and are nothing to do with the typeface.
    let ceiling = fit(&material, &sup, &dir, truth)?;
    let mut fits = vec![ceiling];
    for candidate in &candidates {
        if candidate == &material {
            continue;
        }
        fits.push(fit(candidate, &sup, &dir, truth)?);
    }

    println!("\n--- reference sets against {}-rendered material ---", fits[0].name);
    println!(
        "  {:<24} {:>7} {:>7} {:>9} {:>9} {:>10}",
        "set", "CER", "WER", "coverage", "distance", "vs ceiling"
    );
    for f in &fits {
        let delta = f.score.character_error_rate() - fits[0].score.character_error_rate();
        println!(
            "  {:<24} {:>6.1}% {:>6.1}% {:>8.1}% {:>9.1} {:>+9.1}",
            f.name,
            f.score.character_error_rate() * 100.0,
            f.score.word_error_rate() * 100.0,
            f.coverage() * 100.0,
            f.mean_distance,
            delta * 100.0
        );
    }
    println!(
        "  distance ceiling: {} cells ({}% of {} )",
        subtrackt_glyph::matcher::MatchThresholds::default().max_distance(),
        subtrackt_glyph::matcher::MatchThresholds::default().max_distance_percent,
        subtrackt_core::FEATURE_BITS
    );

    for f in &fits {
        println!("\n--- {} ---", f.name);
        for (want, got) in truth.lines().zip(f.text.trim().lines()) {
            if want == got {
                continue;
            }
            println!("  ! {got}");
            println!("      want: {want}");
        }
    }

    Ok(())
}
