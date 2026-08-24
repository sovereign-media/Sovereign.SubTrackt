//! Can a fitter tell "the right reference set is in this list" from "it is not"?
//!
//! The falsification #43 asks for, and it asks for it *first*: a fixture rendered in a font absent
//! from the candidate list must be **refused** rather than fitted to the nearest thing. A fitter
//! without that property is a machine for producing confident wrong answers, which is the failure
//! this project exists not to have.
//!
//! `docs/reference-set.md` records the encouraging half: mean match distance ranked six candidates
//! and put the winner first, and a real Blu-ray later put the argmin first out of ten. Both of
//! those had the right answer *in the list*. The argmin is trivially something when one candidate
//! is correct; the question here is what it reports when none is.
//!
//! So: leave-one-out. Every font in turn plays the material, its fixture is scored against every
//! candidate, and the run is reported twice — once with the material's own font present, once with
//! it withheld. A floor exists only if those two distributions do not overlap.
//!
//! Two statistics are measured rather than one, because the obvious one has a hole in it.
//!
//! - **Mean match distance**, which #43 proposes and [`Report`](subtrackt::Report) already carries.
//!   It averages over the glyphs that *matched*, so a set that recognises a tenth of the track at
//!   close range scores better than one that recognises all of it at medium range. A selector
//!   reading it alone would prefer the set that gave up.
//! - **Mean distance charging the unmatched**, which fixes that by charging every unread glyph the
//!   match ceiling. That is a *lower bound* on what it actually cost — the glyph was rejected for
//!   exceeding the ceiling, so its true distance is higher — which keeps the statistic honest in
//!   the direction that matters.
//!
//! Neither is assumed. Both are reported, and the floor sweep at the end says which one, if either,
//! can carry the decision.

// Every cast below turns a count of glyphs, fixtures or cells into a float to divide it. The
// largest is a glyph count in the tens of thousands, which is far inside what either float
// represents exactly, so the precision-loss lint has nothing to warn about in this module.
#![allow(clippy::cast_precision_loss)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context as _, bail};
use subtrackt::score::score_text;
use subtrackt_core::FEATURE_BITS;
use subtrackt_glyph::ReferenceSet;
use subtrackt_glyph::matcher::MatchThresholds;

/// Floors to sweep, in tenths of a percent of the feature vector.
///
/// A fraction of [`FEATURE_BITS`] rather than a cell count, for the reason every threshold in this
/// project is one: the same decision has to mean the same thing if `FEATURE_GRID` moves. #45 is the
/// record of what happens when one of them does not.
const FLOORS_PERMILLE: [u32; 12] = [20, 30, 40, 50, 60, 70, 80, 90, 100, 120, 150, 200];

/// What one candidate reference set scored against one material fixture.
#[derive(Clone)]
struct Scored {
    candidate: String,
    /// Mean distance over the glyphs that matched. What `--report` prints as `fit`.
    matched_mean: f32,
    /// Mean distance with every unmatched glyph charged the match ceiling.
    charged_mean: f32,
    coverage: f64,
    cer: f64,
    /// Mean bigram log-probability of the text this candidate read, under the declared language.
    ///
    /// #101. `None` where there was too little Latin-script text to score, which is the silence
    /// [`crate::bigram::Table::score`] promises rather than a fabricated figure.
    ///
    /// The one statistic here that never consults the matcher's assignment. It reads the
    /// distribution of *output characters*, three stages downstream of the ink, so neither of the
    /// two mechanisms that killed #63's five can reach it.
    bigram: Option<f64>,
    /// The same, with every unread character charged the uniform floor.
    ///
    /// #63's own repair for mean match distance, applied to the same flaw here: an unread character
    /// is not a Latin letter, so it breaks the bigram chain and the surviving bigrams are only the
    /// ones the set was confident about. A set that reads three-quarters of a track otherwise
    /// scores on its best three-quarters.
    charged_bigram: Option<f64>,
    /// One entry per line of output, so agreement can be asked per line rather than only per
    /// track. Lines rather than cues because the ground truth the fixture writes carries no cue
    /// boundary, and because "lines made worse" is the unit `docs/post-correction.md` already
    /// judges a stage by.
    lines: Vec<String>,
}

/// Everything measured for one material font.
struct Trial {
    material: String,
    /// What the *ground truth* scores under the language prior.
    ///
    /// #101's anchor, and the row that turned out to matter most. Without it the extraction's
    /// figure is a number in an unfamiliar unit; with it, the question becomes whether a read is
    /// near or far from what a correct read of this same text scores — and on a fixture that
    /// deliberately carries French, Spanish and Italian lines for the accent tests, "what a correct
    /// read scores" is nowhere near what English scores.
    truth_bigram: Option<f64>,
    /// The ground truth the fixture was rendered from, one entry per line.
    truth_lines: Vec<String>,
    /// Every candidate, including the material's own font.
    scores: Vec<Scored>,
}

impl Trial {
    /// The best candidate under one statistic, optionally excluding the material's own font.
    fn best(&self, withhold_material: bool, pick: Pick) -> Option<&Scored> {
        self.scores
            .iter()
            .filter(|s| !withhold_material || s.candidate != self.material)
            .min_by(|a, b| pick(a).total_cmp(&pick(b)))
    }

    /// The candidate with the lowest character error rate, which is the answer a floor is judged
    /// against rather than the answer it produces.
    fn truth(&self) -> Option<&Scored> {
        self.scores.iter().min_by(|a, b| a.cer.total_cmp(&b.cer))
    }
}

fn matched_mean(s: &Scored) -> f32 {
    s.matched_mean
}

fn charged_mean(s: &Scored) -> f32 {
    s.charged_mean
}

/// How one statistic reads a scored candidate.
type Pick = fn(&Scored) -> f32;

/// The two statistics under test, named so a column and a verdict cannot drift apart.
const STATISTICS: [(&str, Pick); 2] = [
    ("mean match distance", matched_mean),
    ("charging unmatched", charged_mean),
];

/// Build one reference set per candidate font, once, and keep them.
///
/// Generating a set is the expensive half of fitting and it does not depend on the material, so a
/// fitter would cache exactly this. Doing the same here keeps the cost figure at the end honest
/// about what fitting would actually cost rather than about what this harness happens to repeat.
pub(crate) fn reference_sets(
    fonts: &[PathBuf],
    dir: &Path,
) -> anyhow::Result<Vec<(String, ReferenceSet)>> {
    let mut out = Vec::new();
    for font in fonts {
        let name = crate::util::stem(font);
        let path = dir.join(format!("select-{name}.subtref"));
        crate::gen_reference(&[
            font.display().to_string(),
            path.display().to_string(),
            "--name".to_owned(),
            name.clone(),
        ])?;
        let set = crate::util::load_reference(&path)?;
        out.push((name, set));
    }
    Ok(out)
}

/// Score every candidate against one material font's fixture.
fn trial(
    material: &Path,
    sets: &[(String, ReferenceSet)],
    dir: &Path,
    repeats: usize,
) -> anyhow::Result<Trial> {
    let name = crate::util::stem(material);
    let fixture_dir = dir.join(format!("material-{name}-x{repeats}"));
    std::fs::create_dir_all(&fixture_dir)?;
    let mut args = vec![
        material.display().to_string(),
        fixture_dir.display().to_string(),
    ];
    if repeats > 1 {
        args.push("--repeat".to_owned());
        args.push(repeats.to_string());
    }
    crate::fixture::make(&args)?;

    let truth = std::fs::read_to_string(fixture_dir.join("synthetic.txt"))?;
    let sup = fixture_dir.join("synthetic.sup");
    let ceiling = f64::from(MatchThresholds::default().max_distance());

    // Built once per trial rather than once per candidate: it depends on the language, not on the
    // read, and rebuilding it inside the loop would suggest otherwise.
    let english = crate::bigram::Table::from_corpus(crate::bigram::CORPUS);

    let mut scores = Vec::new();
    for (candidate, set) in sets {
        let (text, outcome) = crate::accuracy::extract(&sup, set.clone(), false, false)?;
        let report = &outcome.report;
        let total = report.matched + report.unmatched;

        let charged = if total == 0 {
            ceiling
        } else {
            (report.distance_sum as f64 + report.unmatched as f64 * ceiling) / total as f64
        };
        let coverage = if total == 0 {
            0.0
        } else {
            report.matched as f64 / total as f64
        };

        #[allow(clippy::cast_possible_truncation)]
        scores.push(Scored {
            candidate: candidate.clone(),
            matched_mean: report.mean_match_distance(),
            charged_mean: charged as f32,
            coverage,
            cer: score_text(truth.trim(), text.trim()).character_error_rate() * 100.0,
            bigram: english.score(text.trim()),
            charged_bigram: english.score_charged(text.trim()),
            lines: text.trim().lines().map(str::to_owned).collect(),
        });
    }

    let truth_lines: Vec<String> = truth.trim().lines().map(str::to_owned).collect();
    let truth_bigram = english.score(truth.trim());

    Ok(Trial { material: name, truth_bigram, truth_lines, scores })
}

/// Report what each statistic picks when the right answer is available.
///
/// The half that was already encouraging, re-measured across as many fixtures as there are fonts
/// rather than the one #43's table rests on.
fn report_present(trials: &[Trial]) {
    println!("\n--- with the material's own font in the candidate list ---");
    println!("  does the argmin pick it, and is it actually the best read?");
    println!();
    println!(
        "  {:<12} {:<18} {:<20} {:<20}",
        "material", "best available", "argmin, matched mean", "argmin, charged mean"
    );

    let mut penalties: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
    for t in trials {
        let truth = t.truth();
        let floor_cer = truth.map_or(0.0, |s| s.cer);
        let mut picks = Vec::new();
        for (index, (_, pick)) in STATISTICS.iter().enumerate() {
            let choice = t.best(false, *pick);
            let penalty = choice.map_or(0.0, |s| s.cer) - floor_cer;
            penalties[index].push(penalty);
            picks.push(format!(
                "{} (+{penalty:.1})",
                choice.map_or("-", |s| s.candidate.as_str())
            ));
        }
        println!(
            "  {:<12} {:<18} {:<20} {:<20}",
            t.material,
            format!("{} ({:.1}%)", truth.map_or("-", |s| s.candidate.as_str()), floor_cer),
            picks[0],
            picks[1]
        );
    }

    println!();
    println!("  the number beside each pick is what choosing it costs in CER against the best");
    println!("  candidate available — which is the question, rather than whether the argmin");
    println!("  happened to name the font the fixture was rendered from.");
    println!();
    for (index, (label, _)) in STATISTICS.iter().enumerate() {
        let costs = &penalties[index];
        let worst = costs.iter().copied().fold(0.0_f64, f64::max);
        let mean = costs.iter().sum::<f64>() / costs.len() as f64;
        let free = costs.iter().filter(|c| **c < 0.05).count();
        println!(
            "  {label:<22} costs nothing on {free} of {} fixtures, {mean:.1} points on average, {worst:.1} at worst",
            costs.len()
        );
    }
}

/// Report what each statistic reports when the right answer is *absent*.
///
/// The half that decides whether any of this is safe. Nothing here is a ranking question: the
/// argmin will always name something, and the only question is whether the number beside it is
/// distinguishable from the number it produces when the answer is real.
fn report_withheld(trials: &[Trial]) {
    println!("\n--- with the material's own font withheld ---");
    println!("  the argmin still names something. The question is what its score looks like.");
    println!();
    println!(
        "  {:<12} {:>22} {:>22}   {:>6}",
        "material", "matched mean: in / out", "charged mean: in / out", "read"
    );

    for t in trials {
        let mut cells = Vec::new();
        for (_, pick) in STATISTICS {
            let inside = t.best(false, pick).map_or(0.0, pick);
            let outside = t.best(true, pick).map_or(0.0, pick);
            cells.push(format!("{inside:>9.1} /{outside:>9.1}"));
        }
        // Coverage of the withheld pick, because the hazard the charged statistic exists for is a
        // set that scores well by reading almost nothing. If a wrong answer is being accepted at
        // 40% coverage, the number to refuse it on may not be a distance at all.
        let read = t
            .best(true, matched_mean)
            .map_or(0.0, |s| s.coverage * 100.0);
        println!("  {:<12} {:>22} {:>22}   {read:>5.1}%", t.material, cells[0], cells[1]);
    }
}

/// Does the *margin* between the winner and the runner-up say what the winner's score cannot?
///
/// The absolute floor asks "is this set close to the material". This asks "is this set decisively
/// closer than the alternatives" — which is the shape the matcher already uses for one call it
/// cannot make on distance alone. `MatchThresholds::ambiguity_margin` exists because a winner that
/// barely beats its runner-up has not really won, and a fitter facing a candidate list with no
/// right answer in it is in exactly that position: everything is equally wrong, so nothing stands
/// out.
///
/// The reason it might fail is visible in the fonts themselves. Verdana and Tahoma are siblings, so
/// withholding one leaves the other standing out by a wide margin while still being the wrong
/// answer. Whether that happens often enough to sink the idea is what this measures.
fn report_margin(trials: &[Trial]) {
    #[allow(clippy::cast_possible_truncation)]
    let bits = FEATURE_BITS as f32;

    for (label, pick) in STATISTICS {
        println!(
            "
--- the margin between winner and runner-up, on {label} ---"
        );
        println!(
            "  {:<12} {:>26} {:>26}",
            "material", "present: win / next / gap", "withheld: win / next / gap"
        );

        let mut present_gaps = Vec::new();
        let mut withheld_gaps = Vec::new();
        for t in trials {
            let mut cells = Vec::new();
            for withhold in [false, true] {
                let mut ranked: Vec<f32> = t
                    .scores
                    .iter()
                    .filter(|s| !withhold || s.candidate != t.material)
                    .map(pick)
                    .collect();
                ranked.sort_by(f32::total_cmp);
                let (win, next) = (
                    ranked.first().copied().unwrap_or(0.0),
                    ranked.get(1).copied().unwrap_or(0.0),
                );
                let gap = next - win;
                if withhold {
                    withheld_gaps.push(gap);
                } else {
                    present_gaps.push(gap);
                }
                cells.push(format!("{win:>7.1} /{next:>7.1} /{gap:>7.1}"));
            }
            println!("  {:<12} {:>26} {:>26}", t.material, cells[0], cells[1]);
        }

        let smallest_present = present_gaps.iter().copied().fold(f32::MAX, f32::min);
        let largest_withheld = withheld_gaps.iter().copied().fold(f32::MIN, f32::max);
        println!(
            "
  smallest gap when the answer is present : {smallest_present:.1} cells ({:.1}%)",
            100.0 * smallest_present / bits
        );
        println!(
            "  largest gap when the answer is withheld : {largest_withheld:.1} cells ({:.1}%)",
            100.0 * largest_withheld / bits
        );
        if smallest_present > largest_withheld {
            println!("  **separated** — a margin between them decides it on these fixtures");
        } else {
            println!(
                "  **they overlap by {:.1} cells** — a margin cannot decide it either",
                largest_withheld - smallest_present
            );
        }
    }
}

/// What would a floor actually buy, and what would it cost?
///
/// The leave-one-out frame above asks whether the true font was present. That turns out to be the
/// wrong question, and the Tahoma row is why: withhold Tahoma and the argmin picks Verdana, which
/// reads Tahoma-rendered material at 1.7% character error — *better than Tahoma's own set reads it*.
/// Refusing that extraction because the "right" font was missing would be throwing away a clean
/// read to satisfy a definition.
///
/// So the floor's job is not to detect an absent typeface. It is to refuse a **bad read**, and the
/// only question that matters is whether the distance statistic predicts one. This prices every
/// floor by what it ships and what it turns away: a floor that refuses a title reading at 1.3% is
/// not being safe, it is being useless.
///
/// Note that CER is not comparable *between* materials here. Arial renders `l` and `I` identically
/// and Verdana does not, so Arial-on-Arial reads at 11% where Verdana-on-Verdana reads at 1.3% —
/// a difference in the material, not in the fit. Each row is therefore shown against the best any
/// candidate achieved on that same material.
fn report_floor(trials: &[Trial]) {
    #[allow(clippy::cast_possible_truncation)]
    let bits = FEATURE_BITS as u32;

    for (label, pick) in STATISTICS {
        println!(
            "
--- what a floor on {label} would buy ---"
        );
        println!("  choosing by argmin over the full candidate list, which is the realistic case");
        println!();
        println!(
            "  {:<12} {:>9} {:>9} {:>16}",
            "material", "argmin", "its CER", "best available"
        );
        let mut rows = Vec::new();
        for t in trials {
            let Some(choice) = t.best(false, pick) else {
                continue;
            };
            let floor_cer = t.truth().map_or(0.0, |s| s.cer);
            rows.push((pick(choice), choice.cer, floor_cer, t.material.clone()));
            println!(
                "  {:<12} {:>9.1} {:>8.1}% {:>15.1}%",
                t.material,
                pick(choice),
                choice.cer,
                floor_cer
            );
        }

        println!();
        println!("  floor              shipped   worst shipped   refused   best refused");
        for permille in FLOORS_PERMILLE {
            let floor = (bits * permille / 1000) as f32;
            let shipped: Vec<&(f32, f64, f64, String)> =
                rows.iter().filter(|r| r.0 <= floor).collect();
            let refused: Vec<&(f32, f64, f64, String)> =
                rows.iter().filter(|r| r.0 > floor).collect();

            let worst_shipped = shipped.iter().map(|r| r.1).fold(f64::MIN, f64::max);
            let best_refused = refused.iter().map(|r| r.1).fold(f64::MAX, f64::min);
            let show = |value: f64, empty: bool| {
                if empty {
                    "      -".to_owned()
                } else {
                    format!("{value:>5.1}%")
                }
            };
            println!(
                "  {permille:>3} permille = {floor:>5.1}  {:>7}   {:>13}   {:>7}   {:>12}",
                shipped.len(),
                show(worst_shipped, shipped.is_empty()),
                refused.len(),
                show(best_refused, refused.is_empty())
            );
        }
        println!();
        println!("  read the last column as the cost: it is the *cleanest* extraction that floor");
        println!("  throws away. A floor is only worth having where that number is bad.");
    }
}

/// One material's agreement between winner and runner-up, beside what the winner actually read.
struct AgreementRow {
    agreement: f64,
    winner_cer: f64,
}

/// Character error rate below which a read counts as good, for the separation question.
///
/// A line has to be drawn somewhere to ask whether agreement separates good reads from bad ones,
/// and this one is generous to the idea: every material whose best candidate reads under it is
/// unambiguously a success, and every material above it is unambiguously not.
const GOOD_READ_PERCENT: f64 = 5.0;

/// How many candidates vote in the per-line committee.
///
/// Three rather than two: two candidates that disagree say only that one of them is wrong, while
/// three that agree is the first count at which "they corroborate each other" means anything. It is
/// also the point where the cost is still one extra scan of glyphs already segmented.
const COMMITTEE: usize = 3;

/// Does the winner agreeing with its runner-up say anything about whether the winner is right?
///
/// The idea left standing when the floor and the margin both failed: stop asking how good the
/// winner's score is and start asking whether anything corroborates it. Two reference sets that
/// produce the same text are, the argument goes, unlikely to be wrong in the same way.
///
/// The arithmetic is discouraging before the run. If the winner reads at 2% and the runner-up at
/// 15%, they disagree on about 13% — so disagreement largely measures the *runner-up's* error, and
/// high agreement means the two typefaces are similar rather than that either is right. Verdana and
/// Tahoma are siblings by the same designer. That is the reason to measure it rather than to assume
/// either way.
fn report_agreement(trials: &[Trial]) {
    println!("\n--- does the winner agree with its runner-up? ---");
    println!("  agreement is one minus the character error rate of the winner against the");
    println!("  runner-up, which is a comparison between two extractions rather than a threshold");
    println!("  on either one's score");
    println!();
    println!(
        "  {:<12} {:>10} {:>12} {:>12} {:>12}",
        "material", "winner", "runner-up", "agreement", "winner CER"
    );

    let mut rows: Vec<AgreementRow> = Vec::new();
    for t in trials {
        let mut ranked: Vec<&Scored> = t.scores.iter().collect();
        ranked.sort_by(|a, b| a.matched_mean.total_cmp(&b.matched_mean));
        let (Some(winner), Some(runner_up)) = (ranked.first(), ranked.get(1)) else {
            continue;
        };

        let joined = |s: &Scored| s.lines.join("\n");
        let agreement =
            100.0 - score_text(&joined(runner_up), &joined(winner)).character_error_rate() * 100.0;
        rows.push(AgreementRow { agreement, winner_cer: winner.cer });
        println!(
            "  {:<12} {:>10} {:>12} {:>11.1}% {:>11.1}%",
            t.material, winner.candidate, runner_up.candidate, agreement, winner.cer
        );
    }

    // The question, as a separation rather than a correlation: a floor on agreement is only worth
    // having if every good read agrees more than every bad one does.
    let (good, bad): (Vec<&AgreementRow>, Vec<&AgreementRow>) =
        rows.iter().partition(|r| r.winner_cer < GOOD_READ_PERCENT);
    let worst_good = good.iter().map(|r| r.agreement).fold(f64::MAX, f64::min);
    let best_bad = bad.iter().map(|r| r.agreement).fold(f64::MIN, f64::max);

    println!(
        "\n  reads under 5% CER agree at least : {worst_good:.1}%  ({} of them)",
        good.len()
    );
    println!(
        "  reads over 5% CER agree at most   : {best_bad:.1}%  ({} of them)",
        bad.len()
    );
    if worst_good > best_bad {
        println!(
            "  **separated** — a floor on agreement would accept the first and refuse the second"
        );
    } else {
        println!(
            "  **they overlap by {:.1} points** — agreement does not separate a good read from a bad one",
            best_bad - worst_good
        );
    }
}

/// Does a language prior separate a good read from a bad one?
///
/// #101, and the sixth statistic put to #63's bar. The five before it all asked the matcher about
/// its own answer, or measured decoded ink; this reads the *text*, three stages downstream, and
/// never touches the assignment. `docs/fit-confidence.md` leaves a standing filter for exactly this
/// case and this is the answer to it.
///
/// The bar is #63's, unchanged, and it is separation rather than correlation:
///
/// > Retrieval asks for a ranking. A floor needs a margin.
///
/// So the table below reports, for every candidate on every material — not only the winner — the
/// bigram score beside the character error rate, and then asks whether **every** good read scores
/// better than **every** bad one. A statistic that merely correlates convicts nothing: two genuinely
/// independent statistics that both track the right answer must correlate. What decides it is
/// whether one can be thresholded without throwing away a clean extraction.
///
/// The independence test is the other half, and it is the one #63 specifies: does this go wrong in
/// the *same places* as mean match distance? Calibri fitting closest at 14.7 cells while reading at
/// 11.5% CER, against a further-out Trebuchet reading at 2.6%, is the case that decides it —
/// agreement where match distance is already right is evidence of nothing.
fn report_bigram(trials: &[Trial]) {
    println!("\n--- a language prior on the read text (#101) ---");
    println!(
        "  mean bigram log-probability per character pair, under a table built from a\n  \
         public-domain English text at bench time. Higher is more English-like; a uniform\n  \
         alphabet would score {:.2}.",
        crate::bigram::Table::uniform_floor()
    );
    println!();
    println!(
        "  {:<12} {:<12} {:>9} {:>9} {:>9} {:>9} {:>10}",
        "material", "candidate", "CER", "read", "bigram", "charged", "distance"
    );

    let mut good: Vec<Ranked> = Vec::new();
    let mut bad: Vec<Ranked> = Vec::new();
    let mut rows: Vec<Read> = Vec::new();
    for t in trials {
        // The anchor first: what a *perfect* read of this material scores. Anything below it is
        // the read's fault; the distance from it is the only reading of a candidate's figure that
        // does not require calibrating an unfamiliar unit by eye.
        println!(
            "  {:<12} {:<12} {:>8.1}% {:>9} {:>9} {:>9} {:>10}",
            t.material,
            "GROUND TRUTH",
            0.0,
            "100.0%",
            t.truth_bigram
                .map_or_else(|| "no score".to_owned(), |v| format!("{v:.2}")),
            t.truth_bigram
                .map_or_else(|| "no score".to_owned(), |v| format!("{v:.2}")),
            "-"
        );
        let mut ranked: Vec<&Scored> = t.scores.iter().collect();
        ranked.sort_by(|a, b| a.cer.total_cmp(&b.cer));
        for s in ranked {
            let show =
                |v: Option<f64>| v.map_or_else(|| "no score".to_owned(), |v| format!("{v:.2}"));
            println!(
                "  {:<12} {:<12} {:>8.1}% {:>8.1}% {:>9} {:>9} {:>10.1}",
                t.material,
                s.candidate,
                s.cer,
                s.coverage * 100.0,
                show(s.bigram),
                show(s.charged_bigram),
                s.matched_mean
            );
            // Silence, not a number, and it stays out of the bar rather than entering it as a
            // guess. A row that vanished entirely would look like a candidate never scored, so it
            // is printed either way.
            let Some(bigram) = s.charged_bigram else {
                continue;
            };
            let label = format!("{}/{}", t.material, s.candidate);
            rows.push((s.cer, bigram, label.clone()));
            if s.cer < GOOD_READ_PERCENT {
                good.push((bigram, label));
            } else {
                bad.push((bigram, label));
            }
        }
    }

    separation(&good, &bad);
    separation_sweep(&rows);
    argmin_only(trials);
    independence(trials);
}

/// The same question restricted to the read a fitter would actually produce.
///
/// The table above puts every candidate to the bar, which is the strict form and the one #63's
/// figures are in. It is also not the situation a gate is ever in: a fitter picks the argmin by
/// mean match distance and a gate sees *that* read and no other. Fourteen rows rather than 196, and
/// the bar applied to them.
fn argmin_only(trials: &[Trial]) {
    println!(
        "
  restricted to the read a fitter would produce, on the charged score — the argmin by
  match distance:"
    );
    println!(
        "  {:<12} {:<12} {:>9} {:>10} {:>16}",
        "material", "argmin", "its CER", "bigram", "vs ground truth"
    );
    let (mut good, mut bad): (Vec<Ranked>, Vec<Ranked>) = (Vec::new(), Vec::new());
    for t in trials {
        let Some(choice) = t.best(false, matched_mean) else {
            continue;
        };
        let Some(bigram) = choice.charged_bigram else {
            continue;
        };
        println!(
            "  {:<12} {:<12} {:>8.1}% {bigram:>10.2} {:>16}",
            t.material,
            choice.candidate,
            choice.cer,
            t.truth_bigram
                .map_or_else(|| "-".to_owned(), |truth| format!("{:+.2}", bigram - truth))
        );
        let label = format!("{}/{}", t.material, choice.candidate);
        if choice.cer < GOOD_READ_PERCENT {
            good.push((bigram, label));
        } else {
            bad.push((bigram, label));
        }
    }
    separation(&good, &bad);
}

/// One scored read: how badly it read, what the prior made of it, and which pair it was.
type Read = (f64, f64, String);

/// One scored read under a single statistic, for the two-bucket form of the bar.
type Ranked = (f64, String);

/// The bar, swept across where "good" is drawn rather than at one arbitrary line.
///
/// [`GOOD_READ_PERCENT`] is generous to the idea and it is still a line someone chose. Sweeping it
/// says something the single figure cannot: *how bad* a read has to be before the statistic can
/// tell. A gate that separates 3% from 30% and not 3% from 8% is a different product from one that
/// separates nothing, and #101's own framing — a mismatched set produces "73% correct text and 27%
/// confidently wrong" — is a claim about the far end of this sweep.
fn separation_sweep(rows: &[Read]) {
    println!(
        "
  the bar, swept across where a read stops counting as good:"
    );
    println!(
        "    {:>10} {:>6} {:>6} {:>22} {:>22}",
        "line", "good", "bad", "worst good", "best bad"
    );
    for line in [5.0f64, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0] {
        let (good, bad): (Vec<&Read>, Vec<&Read>) =
            rows.iter().partition(|(cer, _, _)| *cer < line);
        let worst_good = good.iter().min_by(|a, b| a.1.total_cmp(&b.1));
        let best_bad = bad.iter().max_by(|a, b| a.1.total_cmp(&b.1));
        let (Some(worst_good), Some(best_bad)) = (worst_good, best_bad) else {
            println!("    {line:>9.0}% {:>6} {:>6}   not both kinds", good.len(), bad.len());
            continue;
        };
        let verdict = if worst_good.1 > best_bad.1 {
            format!("SEPARATED by {:.2}", worst_good.1 - best_bad.1)
        } else {
            format!("overlaps by {:.2}", best_bad.1 - worst_good.1)
        };
        println!(
            "    {line:>9.0}% {:>6} {:>6} {:>22} {:>22}   {verdict}",
            good.len(),
            bad.len(),
            format!("{:.2} ({})", worst_good.1, worst_good.2),
            format!("{:.2} ({})", best_bad.1, best_bad.2)
        );
    }
}

/// The bar: every good read above every bad one, with the crossing pair named when it is not.
fn separation(good: &[Ranked], bad: &[Ranked]) {
    let worst_good = good.iter().min_by(|a, b| a.0.total_cmp(&b.0));
    let best_bad = bad.iter().max_by(|a, b| a.0.total_cmp(&b.0));
    println!(
        "\n  reads under {GOOD_READ_PERCENT:.0}% CER: {} of them, worst scoring {}",
        good.len(),
        worst_good.map_or_else(|| "-".to_owned(), |(v, n)| format!("{v:.2} ({n})"))
    );
    println!(
        "  reads over  {GOOD_READ_PERCENT:.0}% CER: {} of them, best scoring  {}",
        bad.len(),
        best_bad.map_or_else(|| "-".to_owned(), |(v, n)| format!("{v:.2} ({n})"))
    );

    let (Some(worst_good), Some(best_bad)) = (worst_good, best_bad) else {
        println!("  not enough of both kinds to ask the question");
        return;
    };
    if worst_good.0 > best_bad.0 {
        println!(
            "  **separated** by {:.2} — a floor between them accepts every good read and refuses\n  \
             every bad one on these materials",
            worst_good.0 - best_bad.0
        );
    } else {
        // The crossing pair is the deliverable when it fails. "It overlaps" is a result; *which*
        // two rows overlap is what says whether the statistic is close or is measuring nothing.
        println!(
            "  **they overlap by {:.2}** — {} reads well and scores below {}, which reads badly",
            best_bad.0 - worst_good.0,
            worst_good.1,
            best_bad.1
        );
    }
}

/// Does the prior go wrong in the same places mean match distance does?
///
/// The discriminating test #63 specifies, and the reason correlating the two convicts nothing. For
/// every material, this asks whether the two statistics **rank the candidates the same way** — and
/// counts the pairs where they disagree, since a statistic that only ever agrees adds nothing no
/// matter how well it correlates.
fn independence(trials: &[Trial]) {
    println!(
        "\n  where the prior and mean match distance disagree about which candidate is better:"
    );
    println!(
        "  {:<12} {:<28} {:>10} {:>10}",
        "material", "pair", "by distance", "by prior"
    );

    let (mut disagreements, mut prior_right, mut distance_right) = (0usize, 0usize, 0usize);
    for t in trials {
        let scored: Vec<&Scored> = t
            .scores
            .iter()
            .filter(|s| s.charged_bigram.is_some())
            .collect();
        for (index, a) in scored.iter().enumerate() {
            for b in &scored[index + 1..] {
                let distance_prefers_a = a.matched_mean < b.matched_mean;
                let prior_prefers_a = a.charged_bigram > b.charged_bigram;
                if distance_prefers_a == prior_prefers_a {
                    continue;
                }
                disagreements += 1;
                // Which of them was right, judged against the only thing that knows: the CER.
                let truth_prefers_a = a.cer < b.cer;
                if truth_prefers_a == prior_prefers_a {
                    prior_right += 1;
                } else {
                    distance_right += 1;
                }
                if disagreements <= 24 {
                    println!(
                        "  {:<12} {:<28} {:>10} {:>10}",
                        t.material,
                        format!(
                            "{} ({:.1}%) vs {} ({:.1}%)",
                            a.candidate, a.cer, b.candidate, b.cer
                        ),
                        if distance_prefers_a {
                            &a.candidate
                        } else {
                            &b.candidate
                        },
                        if prior_prefers_a {
                            &a.candidate
                        } else {
                            &b.candidate
                        }
                    );
                }
            }
        }
    }
    if disagreements > 24 {
        println!("  ... {} further disagreements not listed", disagreements - 24);
    }
    println!(
        "\n  {disagreements} disagreements; the prior was right in {prior_right} and mean match\n  \
         distance in {distance_right}. A statistic that never disagreed would add nothing however\n  \
         well it correlated, and one that disagrees and is usually wrong is worse than nothing."
    );
}

/// Where the top candidates disagree, is the winner wrong more often?
///
/// The per-line version of the same question, and the one that could survive the track-level answer
/// failing. It is not a floor: it produces a fact about one line rather than a verdict about a
/// track, which is the shape `--on-unmatched` already has for a glyph the matcher declined to call.
///
/// Judged on the two rates that matter rather than on an aggregate. A flag that fires on most lines
/// of a *good* extraction is not a flag, however well it correlates.
fn report_committee(trials: &[Trial]) {
    println!("\n--- where the top {COMMITTEE} disagree, is the winner wrong? ---");
    println!(
        "  {:<12} {:>8} {:>10} {:>12} {:>12} {:>10}",
        "material", "lines", "flagged", "wrong|flagged", "wrong|clean", "lift"
    );

    let (mut total_flagged, mut total_lines) = (0usize, 0usize);
    for t in trials {
        let mut ranked: Vec<&Scored> = t.scores.iter().collect();
        ranked.sort_by(|a, b| a.matched_mean.total_cmp(&b.matched_mean));
        let panel: Vec<&&Scored> = ranked.iter().take(COMMITTEE).collect();
        if panel.len() < COMMITTEE {
            continue;
        }

        let lines = t
            .truth_lines
            .len()
            .min(panel.iter().map(|s| s.lines.len()).min().unwrap_or(0));
        let (mut flagged_wrong, mut flagged, mut clean_wrong, mut clean) = (0, 0, 0, 0);

        for index in 0..lines {
            let winner = &panel[0].lines[index];
            let agrees = panel
                .iter()
                .all(|s| s.lines[index].as_str() == winner.as_str());
            let wrong = winner.as_str() != t.truth_lines[index].as_str();
            match (agrees, wrong) {
                (true, true) => {
                    clean += 1;
                    clean_wrong += 1;
                }
                (true, false) => clean += 1,
                (false, true) => {
                    flagged += 1;
                    flagged_wrong += 1;
                }
                (false, false) => flagged += 1,
            }
        }

        total_flagged += flagged;
        total_lines += lines;
        let rate = |wrong: usize, of: usize| {
            if of == 0 {
                f64::NAN
            } else {
                100.0 * wrong as f64 / of as f64
            }
        };
        let (flagged_rate, clean_rate) = (rate(flagged_wrong, flagged), rate(clean_wrong, clean));
        println!(
            "  {:<12} {lines:>8} {:>9.0}% {:>11.0}% {:>11.0}% {:>10}",
            t.material,
            rate(flagged, lines),
            flagged_rate,
            clean_rate,
            if clean_rate > 0.0 {
                format!("{:.1}x", flagged_rate / clean_rate)
            } else if flagged_rate > 0.0 {
                "inf".to_owned()
            } else {
                "-".to_owned()
            }
        );
    }

    println!(
        "\n  lines flagged across every fixture: {total_flagged} of {total_lines} ({:.0}%)",
        100.0 * total_flagged as f64 / total_lines.max(1) as f64
    );
    println!("  a flag that fires on most lines of a good extraction is not a flag, whatever it");
    println!("  correlates with — so read that share before the lift column.");
}

/// Which characters of `a` survive an alignment against `b` unchanged.
///
/// One flag per character of `a`: true where the alignment pairs it with an identical character in
/// `b`, false where it was substituted or deleted. That is the per-character version of "do these
/// two extractions agree here", and it is what the line-level test above could not express — a
/// single differing character condemns a whole line, which is why that test flagged 100% of them.
///
/// Full matrix with a traceback rather than the two-row form used elsewhere in this file, because
/// the answer needed is *where* the two differ and not merely how much. Run per line, so the matrix
/// is tens of cells on a side.
fn aligned_matches(a: &str, b: &str) -> Vec<bool> {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut cost = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for (i, row) in cost.iter_mut().enumerate() {
        row[0] = u32::try_from(i).unwrap_or(u32::MAX);
    }
    for (j, cell) in cost[0].iter_mut().enumerate() {
        *cell = u32::try_from(j).unwrap_or(u32::MAX);
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let substitute = cost[i - 1][j - 1] + u32::from(a[i - 1] != b[j - 1]);
            cost[i][j] = substitute.min(cost[i - 1][j] + 1).min(cost[i][j - 1] + 1);
        }
    }

    let mut matched = vec![false; a.len()];
    let (mut i, mut j) = (a.len(), b.len());
    while i > 0 && j > 0 {
        let same = a[i - 1] == b[j - 1];
        if cost[i][j] == cost[i - 1][j - 1] + u32::from(!same) {
            matched[i - 1] = same;
            i -= 1;
            j -= 1;
        } else if cost[i][j] == cost[i - 1][j] + 1 {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    matched
}

/// Per character, does the committee agreeing predict the winner being right?
///
/// The line-level test above asked whether three extractions were byte-identical over a whole line
/// and found that they essentially never are — which measures line length, not corroboration. This
/// asks the same question one character at a time, which is the granularity a fitter would actually
/// have: it would rescan one segmentation against several reference sets and hold N answers for
/// each *glyph*, aligned by construction. Aligning the texts is the closest this harness can get to
/// that without a scan-only path through the pipeline.
///
/// Reported as two rates rather than an aggregate, for the same reason the line version was: a flag
/// that fires on most characters of a good extraction is not a flag.
fn character_tally(t: &Trial, panel: &[&&Scored]) -> (Tally4, BTreeMap<char, usize>) {
    let lines = t
        .truth_lines
        .len()
        .min(panel.iter().map(|s| s.lines.len()).min().unwrap_or(0));
    let mut counts = Tally4::default();
    let mut agreed_wrong: BTreeMap<char, usize> = BTreeMap::new();

    for index in 0..lines {
        let winner = &panel[0].lines[index];
        let correct = aligned_matches(winner, &t.truth_lines[index]);
        let mut supported = vec![true; correct.len()];
        for other in panel.iter().skip(1) {
            for (slot, agrees) in aligned_matches(winner, &other.lines[index])
                .into_iter()
                .enumerate()
            {
                supported[slot] &= agrees;
            }
        }

        let written: Vec<char> = winner.chars().collect();
        for (slot, (right, backed)) in correct.into_iter().zip(supported).enumerate() {
            if backed
                && !right
                && let Some(c) = written.get(slot)
            {
                *agreed_wrong.entry(*c).or_insert(0usize) += 1;
            }
            match (backed, right) {
                (true, true) => counts.clean += 1,
                (true, false) => {
                    counts.clean += 1;
                    counts.clean_wrong += 1;
                }
                (false, true) => counts.flagged += 1,
                (false, false) => {
                    counts.flagged += 1;
                    counts.flagged_wrong += 1;
                }
            }
        }
    }
    (counts, agreed_wrong)
}

/// The four cells of the flagged-against-wrong table.
#[derive(Default, Clone, Copy)]
struct Tally4 {
    flagged: usize,
    flagged_wrong: usize,
    clean: usize,
    clean_wrong: usize,
}

fn report_character_committee(trials: &[Trial]) {
    println!("\n--- per character: where the top {COMMITTEE} disagree, is the winner wrong? ---");
    println!(
        "  {:<12} {:>8} {:>9} {:>14} {:>13} {:>8}",
        "material", "chars", "flagged", "wrong|flagged", "wrong|clean", "lift"
    );

    let (mut all_flagged, mut all_chars, mut all_flagged_wrong, mut all_clean_wrong) =
        (0usize, 0usize, 0usize, 0usize);
    let mut agreed_errors: Vec<(String, String)> = Vec::new();

    for t in trials {
        let mut ranked: Vec<&Scored> = t.scores.iter().collect();
        ranked.sort_by(|a, b| a.matched_mean.total_cmp(&b.matched_mean));
        let panel: Vec<&&Scored> = ranked.iter().take(COMMITTEE).collect();
        if panel.len() < COMMITTEE {
            continue;
        }

        let (counts, agreed_wrong) = character_tally(t, &panel);
        let (flagged, flagged_wrong, clean, clean_wrong) =
            (counts.flagged, counts.flagged_wrong, counts.clean, counts.clean_wrong);

        // What the committee agreed on and got wrong anyway. This is the diagnostic that says
        // whether the idea failed by accident or by construction: a shared confusion is agreed on
        // *because* every candidate resolves it the same way.
        if !agreed_wrong.is_empty() {
            let mut worst: Vec<(char, usize)> =
                agreed_wrong.iter().map(|(c, n)| (*c, *n)).collect();
            worst.sort_unstable_by_key(|(c, n)| (std::cmp::Reverse(*n), *c));
            let listed: Vec<String> = worst
                .iter()
                .take(4)
                .map(|(c, n)| format!("{c:?} x{n}"))
                .collect();
            agreed_errors.push((t.material.clone(), listed.join("  ")));
        }

        let chars = flagged + clean;
        all_flagged += flagged;
        all_chars += chars;
        all_flagged_wrong += flagged_wrong;
        all_clean_wrong += clean_wrong;

        let rate = |wrong: usize, of: usize| {
            if of == 0 {
                0.0
            } else {
                100.0 * wrong as f64 / of as f64
            }
        };
        let (flagged_rate, clean_rate) = (rate(flagged_wrong, flagged), rate(clean_wrong, clean));
        println!(
            "  {:<12} {chars:>8} {:>8.0}% {:>13.0}% {:>12.1}% {:>8}",
            t.material,
            rate(flagged, chars),
            flagged_rate,
            clean_rate,
            if clean_rate > 0.0 {
                format!("{:.0}x", flagged_rate / clean_rate)
            } else if flagged_rate > 0.0 {
                "inf".to_owned()
            } else {
                "-".to_owned()
            }
        );
    }

    let clean_total = all_chars - all_flagged;
    println!(
        "\n  flagged {all_flagged} of {all_chars} characters ({:.0}%)",
        100.0 * all_flagged as f64 / all_chars.max(1) as f64
    );
    println!(
        "  of the flagged, {:.0}% are wrong; of the unflagged, {:.1}% are",
        100.0 * all_flagged_wrong as f64 / all_flagged.max(1) as f64,
        100.0 * all_clean_wrong as f64 / clean_total.max(1) as f64
    );
    println!("  the second number is the one that decides it: a character three sets agree on is");
    println!("  only safe to trust if it is almost never wrong.");

    println!(
        "
  what the committee agreed on and got wrong anyway:"
    );
    for (material, listed) in &agreed_errors {
        println!("    {material:<12} {listed}");
    }
}

/// How deep a shortlist to price, beyond the argmin.
///
/// #62 ships the fit as a proposal a user accepts rather than a decision the tool makes, so the
/// question is not only "is the argmin right" but "is the right answer somewhere a user could
/// reasonably be shown". A list of three or five is a thing a person can look at; a list of a
/// hundred and twenty-eight is not.
const SHORTLIST: [usize; 3] = [1, 3, 5];

/// Every font file in a directory, sorted so a run is reproducible.
fn pool_fonts(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ttf"))
        })
        .collect();
    out.sort();
    Ok(out)
}

/// Does the argmin survive a candidate list the size of a real font directory?
///
/// The leave-one-out harness uses eight typefaces chosen to span the design space. A user's machine
/// has a hundred and more, most of them irrelevant and some of them — symbol faces, script faces —
/// capable of producing a reference set that matches nothing while scoring well on the glyphs it
/// does match. Enumerating installed fonts is the only candidate-list option that works with no
/// setup, and this is the measurement that says whether it is safe.
///
/// Reported as the CER cost of following the ranking, which is what a user would actually pay,
/// rather than as whether the argmin named a particular file.
fn report_pool(
    materials: &[PathBuf],
    sets: &[(String, ReferenceSet)],
    dir: &Path,
    repeats: usize,
) -> anyhow::Result<()> {
    println!("\n--- a candidate pool the size of a font directory ---");
    println!("  {} candidates against {} materials", sets.len(), materials.len());
    println!();
    println!(
        "  {:<12} {:>21} {:>9} {:>21} {:>7}",
        "material", "best available", "ranked by", "its argmin", "rank"
    );
    println!(
        "  {:<12} {:>21} {:>9} {:>21} {:>7}",
        "", "(lowest CER)", "", "(and its CER)", "of best"
    );

    // [statistic][shortlist depth] -> what each depth costs in CER against the best candidate.
    let mut penalties: Vec<Vec<Vec<f64>>> =
        vec![vec![Vec::new(); SHORTLIST.len()]; STATISTICS.len()];

    for material in materials {
        let t = trial(material, sets, dir, repeats).with_context(|| crate::util::stem(material))?;
        let Some(best) = t.truth() else { continue };

        for (index, (label, pick)) in STATISTICS.iter().enumerate() {
            let mut ranked: Vec<&Scored> = t.scores.iter().collect();
            ranked.sort_by(|a, b| pick(a).total_cmp(&pick(b)));

            // Where the genuinely best-reading candidate sits in this ordering. A user shown a
            // shortlist only benefits if the answer is on it.
            let rank = ranked
                .iter()
                .position(|s| s.candidate == best.candidate)
                .map_or(0, |slot| slot + 1);

            for (slot, depth) in SHORTLIST.iter().enumerate() {
                let shortlist_best = ranked
                    .iter()
                    .take(*depth)
                    .map(|s| s.cer)
                    .fold(f64::MAX, f64::min);
                penalties[index][slot].push(shortlist_best - best.cer);
            }

            let leading = if index == 0 {
                format!("{:<12} {:>13} ({:>4.1}%)", t.material, best.candidate, best.cer)
            } else {
                format!("{:<12} {:>21}", "", "")
            };
            println!(
                "  {leading} {:>9} {:>13} ({:>4.1}%) {rank:>7}",
                short(label),
                ranked.first().map_or("-", |s| s.candidate.as_str()),
                ranked.first().map_or(0.0, |s| s.cer),
            );
        }
    }

    println!("\n  what following each ranking costs, in points of CER against the best candidate:");
    println!("  {:<22} {:<18} {:>8} {:>8}", "statistic", "shortlist", "mean", "worst");
    for (index, (name, _)) in STATISTICS.iter().enumerate() {
        for (slot, depth) in SHORTLIST.iter().enumerate() {
            let costs = &penalties[index][slot];
            if costs.is_empty() {
                continue;
            }
            let mean = costs.iter().sum::<f64>() / costs.len() as f64;
            let worst = costs.iter().copied().fold(0.0_f64, f64::max);
            let label = if *depth == 1 {
                "the argmin alone".to_owned()
            } else {
                format!("best of top {depth}")
            };
            println!("  {name:<22} {label:<18} {mean:>7.1} {worst:>7.1}");
        }
    }
    println!("\n  a shortlist is only worth showing if the answer is on it — read the rank column");
    println!("  before the cost table.");
    Ok(())
}

/// A statistic's name, shortened to fit a column.
fn short(label: &str) -> &str {
    match label {
        "mean match distance" => "matched",
        other => other.split(' ').next_back().unwrap_or(other),
    }
}

/// Run the pool-scale selection experiment.
///
/// # Errors
/// As [`run`].
fn run_pool(pool: &Path, materials: &[PathBuf], dir: &Path, repeats: usize) -> anyhow::Result<()> {
    let candidates = pool_fonts(pool)?;
    println!("pool: {} fonts from {}", candidates.len(), pool.display());
    println!(
        "materials: {}",
        materials
            .iter()
            .map(|f| crate::util::stem(f))
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!("fixture: {repeats} rendering(s) of each cue");

    let generating = Instant::now();
    let sets = reference_sets(&candidates, dir)?;
    let generating = generating.elapsed();

    let scanning = Instant::now();
    report_pool(materials, &sets, dir, repeats)?;
    let scanning = scanning.elapsed();

    println!(
        "
--- what fitting costs at this pool size ---"
    );
    println!(
        "  generating {} reference sets: {:.1}s ({:.0}ms each)",
        sets.len(),
        generating.as_secs_f64(),
        generating.as_secs_f64() * 1000.0 / sets.len().max(1) as f64
    );
    println!(
        "  scanning {} materials against all of them: {:.1}s ({:.1}s per material)",
        materials.len(),
        scanning.as_secs_f64(),
        scanning.as_secs_f64() / materials.len().max(1) as f64
    );
    println!("  the sets do not depend on the material, so a fitter caches them once and pays");
    println!("  only the scan per title.");
    Ok(())
}

/// Run the leave-one-out fit selection experiment.
///
/// # Errors
/// Fails if fewer than two usable fonts are given, or if any stage of generation or extraction
/// fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let flag_values: Vec<usize> = ["--repeat", "--pool"]
        .iter()
        .filter_map(|flag| args.iter().position(|a| a == flag))
        .map(|at| at + 1)
        .collect();
    let fonts: Vec<PathBuf> = args
        .iter()
        .enumerate()
        .filter(|(index, a)| !a.starts_with("--") && !flag_values.contains(index))
        .map(|(_, a)| a)
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    if fonts.len() < 2 {
        bail!(
            "leave-one-out needs at least two fonts; pass several, e.g. \
             xtask fit-select C:/Windows/Fonts/arial.ttf C:/Windows/Fonts/verdana.ttf ..."
        );
    }

    // The obvious objection to everything below is sample size: the fixture is a few hundred
    // glyphs where a feature film is tens of thousands, and a mean over a few hundred is noisy. So
    // the fixture length is a variable, and the run can be repeated at a longer one to find out
    // whether an overlap is a property of the statistic or of the sample.
    let repeats: usize = match args.iter().position(|a| a == "--repeat") {
        Some(at) => args
            .get(at + 1)
            .context("--repeat needs a value")?
            .parse()?,
        None => 1,
    };

    let pool = match args.iter().position(|a| a == "--pool") {
        Some(at) => Some(PathBuf::from(args.get(at + 1).context("--pool needs a directory")?)),
        None => None,
    };

    let dir = std::env::temp_dir().join("subtrackt-fit-select");
    std::fs::create_dir_all(&dir)?;

    if let Some(pool) = pool {
        return run_pool(&pool, &fonts, &dir, repeats);
    }

    println!(
        "fonts: {}",
        fonts
            .iter()
            .map(|f| crate::util::stem(f))
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!("fixture: {repeats} rendering(s) of each cue");

    let generating = Instant::now();
    let sets = reference_sets(&fonts, &dir)?;
    let generating = generating.elapsed();

    let scanning = Instant::now();
    let mut trials = Vec::new();
    for material in &fonts {
        trials.push(
            trial(material, &sets, &dir, repeats).with_context(|| crate::util::stem(material))?,
        );
    }
    let scanning = scanning.elapsed();

    report_present(&trials);
    report_withheld(&trials);
    report_floor(&trials);
    report_margin(&trials);
    report_agreement(&trials);
    report_bigram(&trials);
    report_committee(&trials);
    report_character_committee(&trials);

    // #16 prices per-track invocation carefully enough that this should not arrive unmeasured.
    let extractions = trials.len() * sets.len();
    println!("\n--- what fitting costs ---");
    println!(
        "  generating {} reference sets: {:.2}s total, {:.0}ms each",
        sets.len(),
        generating.as_secs_f64(),
        generating.as_secs_f64() * 1000.0 / sets.len() as f64
    );
    println!(
        "  {extractions} extractions ({} fixtures x {} candidates): {:.2}s total, {:.0}ms each",
        trials.len(),
        sets.len(),
        scanning.as_secs_f64(),
        scanning.as_secs_f64() * 1000.0 / extractions as f64
    );
    println!(
        "  so fitting one title against {} candidates costs about {:.2}s here, on a fixture of {} cues",
        sets.len(),
        scanning.as_secs_f64() / trials.len() as f64,
        9 * repeats
    );
    println!(
        "  ({repeats} rendering(s) of 9). That is the harness's cost, not a fitter's: this decodes"
    );
    println!("  the fixture once per candidate, where a fitter would decode once and rescan the");
    println!(
        "  glyphs it already has. The scan is the part that repeats, and on a real track it is"
    );
    println!("  a fraction of a 70-second extraction that is mostly demux and decode. Which is");
    println!("  itself an argument about where fitting belongs: inside the pipeline, after");
    println!("  segmentation, rather than as N extractions from the outside.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scored(candidate: &str, matched_mean: f32, cer: f64) -> Scored {
        Scored {
            candidate: candidate.to_owned(),
            matched_mean,
            charged_mean: matched_mean + 2.0,
            coverage: 0.9,
            cer,
            bigram: None,
            charged_bigram: None,
            lines: Vec::new(),
        }
    }

    fn trial_of(material: &str, scores: Vec<Scored>) -> Trial {
        Trial {
            material: material.to_owned(),
            truth_bigram: None,
            truth_lines: Vec::new(),
            scores,
        }
    }

    #[test]
    fn withholding_the_material_font_leaves_the_argmin_to_the_rest() {
        let t = trial_of(
            "arial",
            vec![
                scored("arial", 12.0, 5.0),
                scored("verdana", 20.0, 18.0),
                scored("tahoma", 22.0, 21.0),
            ],
        );
        assert_eq!(t.best(false, matched_mean).unwrap().candidate, "arial");
        assert_eq!(t.best(true, matched_mean).unwrap().candidate, "verdana");
    }

    #[test]
    fn the_best_read_is_not_always_the_material_own_font() {
        // The row that reframed this measurement. Verdana reads Tahoma-rendered material better
        // than Tahoma's own set does, so "was the true font present" is the wrong question to ask
        // of a floor — refusing that extraction would throw away a clean read.
        let t = trial_of(
            "tahoma",
            vec![scored("tahoma", 18.4, 2.5), scored("verdana", 17.6, 1.7)],
        );
        assert_eq!(t.truth().unwrap().candidate, "verdana");
        assert_ne!(t.truth().unwrap().candidate, t.material);
    }

    #[test]
    fn a_lower_distance_does_not_mean_a_better_read() {
        // The finding, as a test. These are the measured numbers from the eight-times fixture:
        // Calibri wins its material at 14.7 cells and reads at 11.5%, while Trebuchet wins its own
        // at 16.1 and reads at 2.6%. Any floor tight enough to refuse the first also refuses the
        // second, which is why the floor #43 asks for cannot be built on this statistic.
        let calibri = trial_of("calibri", vec![scored("calibri", 14.7, 11.5)]);
        let trebuc = trial_of("trebuc", vec![scored("trebuc", 16.1, 2.6)]);

        let (a, b) = (
            calibri.best(false, matched_mean).unwrap(),
            trebuc.best(false, matched_mean).unwrap(),
        );
        assert!(a.matched_mean < b.matched_mean, "the closer fit by distance");
        assert!(a.cer > b.cer, "reads four times worse");

        // And so no floor separates them: one that ships `b` also ships `a`.
        for floor in [15.0_f32, 16.0, 17.0, 20.0] {
            let ships_good = b.matched_mean <= floor;
            let ships_bad = a.matched_mean <= floor;
            assert!(
                !ships_good || ships_bad,
                "a floor at {floor} shipped the good read while refusing the bad one"
            );
        }
    }

    #[test]
    fn a_set_that_reads_almost_nothing_loses_once_the_unmatched_are_charged() {
        // The hazard the charged statistic exists for, and the one that only appears at pool scale.
        // A symbol face matches a tenth of the track at close range, so a mean taken over the
        // glyphs that matched flatters it above a set that read everything at medium range. On a
        // real font directory this is not hypothetical: SegoeIcons wins Georgia-rendered material
        // on mean match distance and reads it at 79.8%.
        let ceiling = f64::from(MatchThresholds::default().max_distance());
        let charged = |matched: u64, distance_sum: u64, unmatched: u64| {
            (distance_sum as f64 + unmatched as f64 * ceiling) / (matched + unmatched) as f64
        };

        // Reads a tenth of the glyphs, and those very closely.
        let symbol = charged(100, 400, 900);
        // Reads all of them, at four times the distance each.
        let honest = charged(1000, 16_000, 0);

        // On mean-over-matched the symbol face scores 4 cells against the honest set's 16, so it
        // would win. What follows is that charging the unmatched reverses it.
        assert!(
            symbol > honest,
            "and loses once the unmatched are charged: {symbol:.1} against {honest:.1}"
        );
    }

    #[test]
    fn charging_the_unmatched_glyphs_can_only_raise_a_score() {
        // The statistic exists because a mean over matched glyphs alone rewards a set that gave up.
        // Whatever else it does, it must never flatter a set relative to the mean it corrects.
        let s = scored("segoeui", 18.2, 16.9);
        assert!(charged_mean(&s) >= matched_mean(&s));
    }

    #[test]
    fn alignment_marks_the_characters_that_survived_unchanged() {
        assert_eq!(aligned_matches("abc", "abc"), vec![true, true, true]);
        assert_eq!(aligned_matches("abc", "abd"), vec![true, true, false]);
        assert_eq!(aligned_matches("abc", ""), vec![false, false, false]);
        assert!(aligned_matches("", "abc").is_empty());
    }

    #[test]
    fn an_insertion_does_not_condemn_every_character_after_it() {
        // The reason this replaced the line-level test. Comparing two extractions position by
        // position would call everything after an extra character a disagreement; aligning them
        // says only the extra character disagrees.
        let matched = aligned_matches("abcd", "abXcd");
        assert_eq!(matched, vec![true, true, true, true]);
    }

    #[test]
    fn a_shared_confusion_reads_as_agreement() {
        // The finding, as a test. Three reference sets that all read `l` as `I` agree with each
        // other on every character and are wrong together — so agreement corroborates the error
        // rather than catching it, which is why the committee's lift inverts on Arial.
        let winner = "FoIIow the yeIIow Iine";
        let others = ["FoIIow the yeIIow Iine", "FoIIow the yeIIow Iine"];
        let truth = "Follow the yellow line";

        let correct = aligned_matches(winner, truth);
        let mut supported = vec![true; correct.len()];
        for other in others {
            for (slot, agrees) in aligned_matches(winner, other).into_iter().enumerate() {
                supported[slot] &= agrees;
            }
        }

        let agreed_and_wrong = correct
            .iter()
            .zip(&supported)
            .filter(|(right, backed)| **backed && !**right)
            .count();
        assert!(
            agreed_and_wrong >= 5,
            "the committee agreed on {agreed_and_wrong} wrong characters, and should have"
        );
        assert!(
            supported.iter().all(|b| *b),
            "and flagged none of them, because all three said the same thing"
        );
    }

    #[test]
    fn an_empty_candidate_list_has_no_argmin_rather_than_a_default_one() {
        let t = trial_of("arial", Vec::new());
        assert!(t.best(false, matched_mean).is_none());
        assert!(t.truth().is_none());
    }
}
