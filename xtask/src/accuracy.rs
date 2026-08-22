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
use subtrackt::{Config, LayoutRules, Pipeline, SpacingRule, UnmatchedPolicy};
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

pub fn find_font(explicit: Option<&String>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }
    CANDIDATES.iter().map(PathBuf::from).find(|p| p.exists())
}

/// Extract a `.sup` with a reference set and return the text, one cue line per line.
///
/// # Errors
/// Propagates any pipeline failure.
pub(crate) fn extract(
    sup: &Path,
    reference: ReferenceSet,
    grey_coverage: bool,
    post_correct: bool,
) -> anyhow::Result<(String, subtrackt::Outcome)> {
    extract_spaced(sup, reference, grey_coverage, post_correct, SpacingRule::default())
}

/// As [`extract`], with the word-spacing rule chosen rather than defaulted.
///
/// Split out for #40: scoring one spacing rule against another needs the rest of the pipeline held
/// still, and a rule selected anywhere but here would be comparing two different extractions.
pub(crate) fn extract_spaced(
    sup: &Path,
    reference: ReferenceSet,
    grey_coverage: bool,
    post_correct: bool,
    spacing: SpacingRule,
) -> anyhow::Result<(String, subtrackt::Outcome)> {
    // Placeholder rather than the default gate: the point is to score what was read, and a policy
    // that refuses the track would score nothing at all.
    let config = Config {
        unmatched: UnmatchedPolicy::Placeholder,
        grey_coverage,
        post_correct,
        layout: LayoutRules { spacing, ..LayoutRules::default() },
        ..Config::default()
    };

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
    Ok((text, outcome))
}

/// Extract with the track-vocabulary arm of post-correction switched on.
///
/// #60. A separate helper rather than another flag on `extract`, because the vocabulary is an arm
/// of the context corrector and every other setting has to be held identical for the third column
/// to mean anything.
pub(crate) fn extract_with_vocabulary(
    sup: &Path,
    reference: ReferenceSet,
    grey: bool,
) -> anyhow::Result<(String, subtrackt::Outcome)> {
    let config = Config {
        unmatched: UnmatchedPolicy::Placeholder,
        grey_coverage: grey,
        post_correct: true,
        track_vocabulary: true,
        ..Config::default()
    };
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
    Ok((text, outcome))
}

/// Lines this stage improved and lines it made worse, against the same ground truth.
///
/// The second is the number that decides a default. An aggregate gain says nothing on its own about
/// whether the corrector ever invented something, which is the failure the whole stage is built to
/// avoid.
fn lines_moved(truth: &str, before: &str, after: &str) -> (usize, usize) {
    let (mut better, mut worse) = (0usize, 0usize);
    for ((want, before), after) in truth.lines().zip(before.lines()).zip(after.lines()) {
        let errors_before = score_text(want, before).character_errors;
        let errors_after = score_text(want, after).character_errors;
        match errors_after.cmp(&errors_before) {
            std::cmp::Ordering::Less => better += 1,
            std::cmp::Ordering::Greater => worse += 1,
            std::cmp::Ordering::Equal => {}
        }
    }
    (better, worse)
}

/// Score post-correction against the same fixture read without it.
///
/// This is the measurement [#12] asks for, and the *worse* column is the one that decides it. An
/// aggregate improvement says nothing on its own: a corrector that fixes three characters and
/// invents one has still turned a detectable failure into a plausible wrong answer once, which is
/// the whole thing this project objects to in general OCR. So lines are scored individually and
/// the ones the corrector made worse are counted separately from what it gained.
///
/// Three rows since [#60]: off, the context arm alone, and the context arm with the track's own
/// vocabulary behind it. The third is strictly a superset of the second — the vocabulary is only
/// consulted where context declines — so the rows are cumulative rather than alternatives.
///
/// [#12]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/12
/// [#60]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/60
fn post_correction(
    sup: &Path,
    reference: &ReferenceSet,
    grey: bool,
    truth: &str,
) -> anyhow::Result<()> {
    let (off, _) = extract(sup, reference.clone(), grey, false)?;
    let (on, outcome) = extract(sup, reference.clone(), grey, true)?;
    let (vocab, vocab_outcome) = extract_with_vocabulary(sup, reference.clone(), grey)?;

    let (better, worse) = lines_moved(truth, &off, &on);
    let (vocab_better, vocab_worse) = lines_moved(truth, &off, &vocab);

    let score_off = score_text(truth, off.trim());
    let score_on = score_text(truth, on.trim());
    let score_vocab = score_text(truth, vocab.trim());

    println!("\n--- post-correction (#12, #60) ---");
    println!(
        "  candidates  : {} glyphs the matcher would not call outright",
        outcome.report.ambiguous
    );
    println!("  {:<22} {:>7} {:>7} {:>8} {:>7}", "", "CER", "WER", "better", "worse");
    let row = |label: &str, score: &Score, better: usize, worse: usize| {
        println!(
            "  {label:<22} {:>6.1}% {:>6.1}% {better:>8} {worse:>7}",
            score.character_error_rate() * 100.0,
            score.word_error_rate() * 100.0
        );
    };
    row("off", &score_off, 0, 0);
    row("context", &score_on, better, worse);
    row("context + vocabulary", &score_vocab, vocab_better, vocab_worse);

    println!(
        "\n  substitutions: {} by context, {} by vocabulary",
        outcome.report.corrections, vocab_outcome.report.vocabulary_corrections
    );
    // How often the evidence existed at all. A rule that never fires because nothing supports it
    // is a different result from one that fires and gains nothing, and #60 asks for both.
    println!(
        "  vocabulary   : {} distinct clear tokens learned from the track",
        vocab_outcome.report.vocabulary_tokens
    );
    for correction in &vocab_outcome.corrections {
        println!("  {correction}");
    }

    if score_on.character_error_rate() > score_off.character_error_rate() {
        bail!(
            "post-correction made the ceiling case worse: CER {:.1}% -> {:.1}%",
            score_off.character_error_rate() * 100.0,
            score_on.character_error_rate() * 100.0
        );
    }
    Ok(())
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
            "  spacing   : {} words run together (#40 gap threshold)",
            reference_words - hypothesis_words
        );
    }
    let placeholders = hypothesis.matches('\u{fffd}').count();
    if placeholders > 0 {
        println!("  unread    : {placeholders} glyphs matched nothing (reference coverage, #43)");
    }
}

/// Generate a fixture, extract it with a matching reference set, and score the result.
///
/// # Errors
/// Fails if no usable font can be found, or if any stage of generation or extraction fails.
/// Where a line's spaces sit, as offsets into the same line with its spaces removed.
///
/// Comparing space *counts* would let one invented space cancel one missed space and report a line
/// as correct. Comparing positions cannot: it says which spaces are in the wrong place, which is
/// the number that decides whether a splitting rule is safe to ship.
fn space_positions(line: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut seen = 0;
    for character in line.chars() {
        if character == ' ' {
            positions.push(seen);
        } else {
            seen += 1;
        }
    }
    positions
}

/// Strip every space, so two readings can be compared on their characters alone.
fn despaced(line: &str) -> String {
    line.chars().filter(|c| *c != ' ').collect()
}

/// One spacing rule's showing.
struct Spacing {
    label: &'static str,
    rule: SpacingRule,
    score: Score,
    /// Spaces in the right place, missed, and put where none belonged.
    found: usize,
    missed: usize,
    invented: usize,
    /// Lines whose characters matched well enough for the three counts above to mean anything.
    comparable: usize,
    text: String,
}

/// Score the word-spacing rules against each other (#40).
///
/// # Errors
/// Fails if any extraction fails, or if the shipped rule invents spaces the old one did not — an
/// aggregate CER gain would otherwise hide a rule that cuts words in half.
fn word_spacing(
    sup: &Path,
    reference: &ReferenceSet,
    grey: bool,
    truth: &str,
) -> anyhow::Result<()> {
    let mut measured = Vec::new();
    for (label, rule) in [
        ("median multiple", SpacingRule::MedianMultiple),
        ("widest split", SpacingRule::WidestSplit),
        ("otsu split", SpacingRule::OtsuSplit),
    ] {
        // Post-correction off: it rewrites characters, and this comparison is about where the
        // spaces went.
        let (text, _) = extract_spaced(sup, reference.clone(), grey, false, rule)?;
        let score = score_text(truth, text.trim());

        let (mut found, mut missed, mut invented, mut comparable) = (0, 0, 0, 0);
        for (want, got) in truth.lines().zip(text.trim().lines()) {
            // Positions mean the same thing on both sides as long as the same number of glyphs
            // stands between them; whether each was *read* correctly is a different stage's
            // problem, and requiring correct characters here would score spacing on the one line
            // in eight the matcher happens to get right. A line that lost or gained a glyph is
            // counted nowhere rather than counted wrongly.
            if despaced(want).chars().count() != despaced(got).chars().count() {
                continue;
            }
            comparable += 1;
            let (wanted, placed) = (space_positions(want), space_positions(got));
            found += placed.iter().filter(|at| wanted.contains(at)).count();
            missed += wanted.iter().filter(|at| !placed.contains(at)).count();
            invented += placed.iter().filter(|at| !wanted.contains(at)).count();
        }
        measured.push(Spacing { label, rule, score, found, missed, invented, comparable, text });
    }

    // The headroom: what the same read would score if spacing were free. Anything the spacing rule
    // cannot reach lives below this line.
    let ceiling = measured
        .first()
        .map(|m| score_text(&despaced(truth), &despaced(m.text.trim())));

    println!(
        "
--- word spacing (#40) ---"
    );
    if let Some(ceiling) = ceiling {
        println!(
            "  spacing ignored entirely: CER {:>5.1}%  <- the headroom a spacing rule plays for",
            ceiling.character_error_rate() * 100.0
        );
    }
    println!("  rule               CER     WER   spaces: right  missed  invented   (lines scored)");
    for m in &measured {
        println!(
            "  {:<16} {:>5.1}%  {:>5.1}%           {:>5}   {:>5}     {:>5}   ({} of {})",
            m.label,
            m.score.character_error_rate() * 100.0,
            m.score.word_error_rate() * 100.0,
            m.found,
            m.missed,
            m.invented,
            m.comparable,
            truth.lines().count()
        );
    }

    let shipped = LayoutRules::default().spacing;
    for m in &measured {
        if m.rule != shipped {
            continue;
        }
        println!(
            "
  the shipped rule, line by line:"
        );
        for (want, got) in truth.lines().zip(m.text.trim().lines()) {
            let mark = if want == got { ' ' } else { '!' };
            println!("  {mark} {got}");
            if mark == '!' {
                println!("      want: {want}");
            }
        }
    }

    // An invented space cuts a word in two and is the failure this rule could introduce that #11
    // could not, so it is gated separately from the aggregate.
    let baseline = measured
        .iter()
        .find(|m| m.rule == SpacingRule::MedianMultiple)
        .context("the median rule was not scored")?;
    let chosen = measured
        .iter()
        .find(|m| m.rule == shipped)
        .context("the shipped rule was not scored")?;
    if chosen.invented > baseline.invented {
        bail!(
            "the shipped spacing rule invents {} spaces against the median rule's {}",
            chosen.invented,
            baseline.invented
        );
    }
    Ok(())
}

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

    let truth = std::fs::read_to_string(dir.join("synthetic.txt"))?;
    let sup = dir.join("synthetic.sup");

    // Both feature representations, over one fixture, each against a reference set built through
    // its own normalisation. Scoring them together is the only way to attribute a difference to
    // the representation rather than to the fixture or the font.
    let mut scored = Vec::new();
    for (label, grey) in [("binary mask", false), ("grey coverage", true)] {
        let path = dir.join(if grey {
            "reference-grey.subtref"
        } else {
            "reference.subtref"
        });
        let mut gen_args = vec![
            font.display().to_string(),
            path.display().to_string(),
            "--name".to_owned(),
            "accuracy-fixture".to_owned(),
        ];
        if grey {
            gen_args.push("--grey-coverage".to_owned());
        }
        crate::gen_reference(&gen_args)?;

        let reference =
            ReferenceSet::decode(&std::fs::read(&path)?).map_err(|e| anyhow::anyhow!("{e}"))?;
        // Post-correction off here on purpose: this comparison is about the feature
        // representation, and a stage that rewrites the output afterwards would confound it.
        let (text, outcome) = extract(&sup, reference.clone(), grey, false)?;
        let score = score_text(truth.trim(), text.trim());

        println!("\n--- {label}: accuracy on generated ground truth ---");
        report(&score, &outcome.report, truth.trim(), text.trim());
        // Once, for the representation the pipeline ships with, for the same reason the gate below
        // applies to that one: it is the read a user would actually get.
        if grey == Config::default().grey_coverage {
            crate::unread::table(&outcome.unread);
        }
        scored.push((label, grey, score, text, reference));
    }

    for (label, _, _, text, _) in &scored {
        println!("\n--- {label}: extracted ---");
        for (expected, got) in truth.trim().lines().zip(text.trim().lines()) {
            let mark = if expected == got { ' ' } else { '!' };
            println!("  {mark} {got}");
            if mark == '!' {
                println!("      want: {expected}");
            }
        }
    }

    println!("\n--- binary against grey ---");
    for (label, _, score, _, _) in &scored {
        println!(
            "  {label:<14} CER {:>5.1}%   WER {:>5.1}%",
            score.character_error_rate() * 100.0,
            score.word_error_rate() * 100.0
        );
    }

    // The gate applies to whichever representation the pipeline actually ships with, since that is
    // the number a user would get. A ceiling case reading worse than half its characters correctly
    // means something is broken rather than merely imperfect.
    let shipped = Config::default().grey_coverage;
    let (_, _, score, _, reference) = scored
        .iter()
        .find(|(_, grey, _, _, _)| *grey == shipped)
        .context("the shipped representation was not scored")?;

    // Post-correction is measured against the representation the pipeline ships with, for the same
    // reason the gate is: it is the number a user would actually get.
    post_correction(&sup, reference, shipped, truth.trim())?;
    word_spacing(&sup, reference, shipped, truth.trim())?;

    if score.character_error_rate() > 0.5 {
        bail!(
            "character error rate {:.1}% on a same-font fixture; this is the ceiling case and              should be far better",
            score.character_error_rate() * 100.0
        );
    }
    Ok(())
}
