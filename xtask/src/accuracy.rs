//! End-to-end accuracy scoring against generated ground truth.
//!
//! This is the instrument [#15](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/15)
//! asks for, and it measures the thing nothing else in this project measures. Every earlier number
//! — reference coverage, glyph distance distributions, the typeface fit — is about whether shapes
//! *look alike*. This is the only one that says whether the tool reads the *right characters*, and
//! those two can diverge without warning.
//!
//! Fixture, reference set and scoring all come from the same font on purpose. That is the ceiling
//! case: if the typeface is known exactly and the rendering is ours, whatever error remains belongs
//! to the pipeline rather than to typeface mismatch. Real material can only be worse, so this is a
//! useful upper bound to hold stages against.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use subtrackt::score::{Score, score_text};
use subtrackt::{Config, Pipeline, UnmatchedPolicy};
use subtrackt_glyph::ReferenceSet;

/// Fonts to try when none is named, so the harness runs unattended on both developer machines
/// and Linux CI.
const CANDIDATES: [&str; 6] = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/Library/Fonts/Arial.ttf",
    "C:/Windows/Fonts/arial.ttf",
    "C:/Windows/Fonts/segoeui.ttf",
];

fn find_font(explicit: Option<&String>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }
    CANDIDATES.iter().map(PathBuf::from).find(|p| p.exists())
}

/// Extract a `.sup` with a reference set and return the text, one cue line per line.
fn extract(sup: &Path, reference: ReferenceSet) -> anyhow::Result<(String, subtrackt::Report)> {
    // Placeholder rather than the default gate: the point is to score what was read, and a policy
    // that refuses the track would score nothing at all.
    let config = Config { unmatched: UnmatchedPolicy::Placeholder, ..Config::default() };

    let outcome = Pipeline::new(config)
        .with_reference(reference)
        .run(sup)
        .with_context(|| format!("extracting {}", sup.display()))?;

    let text = outcome
        .track
        .cues
        .iter()
        .map(subtrackt::core::Cue::text)
        .collect::<Vec<_>>()
        .join("\n");
    Ok((text, outcome.report))
}

/// Print a score and the error classes behind it.
fn report(score: &Score, report: &subtrackt::Report, reference: &str, hypothesis: &str) {
    println!(
        "  reference : {} characters, {} words",
        score.reference_characters, score.reference_words
    );
    println!(
        "  CER       : {}/{} = {:.1}%",
        score.character_errors,
        score.reference_characters,
        score.character_error_rate() * 100.0
    );
    println!(
        "  WER       : {}/{} = {:.1}%",
        score.word_errors,
        score.reference_words,
        score.word_error_rate() * 100.0
    );
    println!(
        "  glyphs    : {} matched / {} unmatched / {} ambiguous",
        report.matched, report.unmatched, report.ambiguous
    );

    // Word count tells spacing errors apart from character errors, which the rates alone do not:
    // one missing space costs two word errors and only one character error.
    let reference_words = reference.split_whitespace().count();
    let hypothesis_words = hypothesis.split_whitespace().count();
    if hypothesis_words < reference_words {
        println!(
            "  spacing   : {} words run together (#11 gap threshold)",
            reference_words - hypothesis_words
        );
    }
    let placeholders = hypothesis.matches('\u{fffd}').count();
    if placeholders > 0 {
        println!("  unread    : {placeholders} glyphs matched nothing (#9 reference coverage)");
    }
}

/// Generate a fixture, extract it with a matching reference set, and score the result.
///
/// # Errors
/// Fails if no usable font can be found, or if any stage of generation or extraction fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let font = find_font(args.first()).context(
        "no font found; pass one explicitly, e.g. xtask accuracy C:/Windows/Fonts/arial.ttf",
    )?;

    let dir = std::env::temp_dir().join("subtrackt-accuracy");
    std::fs::create_dir_all(&dir)?;

    println!("font: {}", font.display());
    crate::fixture::make(&[font.display().to_string(), dir.display().to_string()])?;
    crate::gen_reference(&[
        font.display().to_string(),
        dir.join("reference.subtref").display().to_string(),
        "--name".to_owned(),
        "accuracy-fixture".to_owned(),
    ])?;

    let reference = ReferenceSet::decode(&std::fs::read(dir.join("reference.subtref"))?)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let truth = std::fs::read_to_string(dir.join("synthetic.txt"))?;
    let (text, run_report) = extract(&dir.join("synthetic.sup"), reference)?;

    let score = score_text(truth.trim(), text.trim());
    println!("\n--- accuracy on generated ground truth ---");
    report(&score, &run_report, truth.trim(), text.trim());

    println!("\n--- extracted ---");
    for (expected, got) in truth.trim().lines().zip(text.trim().lines()) {
        let mark = if expected == got { ' ' } else { '!' };
        println!("  {mark} {got}");
        if mark == '!' {
            println!("      want: {expected}");
        }
    }

    // A ceiling case that reads worse than half its characters correctly means something is broken
    // rather than merely imperfect, and the harness should say so rather than print a number.
    if score.character_error_rate() > 0.5 {
        bail!(
            "character error rate {:.1}% on a same-font fixture; this is the ceiling case and \
             should be far better",
            score.character_error_rate() * 100.0
        );
    }
    Ok(())
}
