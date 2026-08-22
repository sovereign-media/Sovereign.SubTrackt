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
}

/// Everything measured for one material font.
struct Trial {
    material: String,
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

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "unnamed".to_owned(), |s| s.to_string_lossy().into_owned())
}

/// Build one reference set per candidate font, once, and keep them.
///
/// Generating a set is the expensive half of fitting and it does not depend on the material, so a
/// fitter would cache exactly this. Doing the same here keeps the cost figure at the end honest
/// about what fitting would actually cost rather than about what this harness happens to repeat.
fn reference_sets(fonts: &[PathBuf], dir: &Path) -> anyhow::Result<Vec<(String, ReferenceSet)>> {
    let mut out = Vec::new();
    for font in fonts {
        let name = stem(font);
        let path = dir.join(format!("select-{name}.subtref"));
        crate::gen_reference(&[
            font.display().to_string(),
            path.display().to_string(),
            "--name".to_owned(),
            name.clone(),
        ])?;
        let set =
            ReferenceSet::decode(&std::fs::read(&path)?).map_err(|e| anyhow::anyhow!("{e}"))?;
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
    let name = stem(material);
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
        });
    }

    Ok(Trial { material: name, scores })
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

/// Run the leave-one-out fit selection experiment.
///
/// # Errors
/// Fails if fewer than two usable fonts are given, or if any stage of generation or extraction
/// fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let flag_value = args.iter().position(|a| a == "--repeat").map(|at| at + 1);
    let fonts: Vec<PathBuf> = args
        .iter()
        .enumerate()
        .filter(|(index, a)| !a.starts_with("--") && Some(*index) != flag_value)
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

    let dir = std::env::temp_dir().join("subtrackt-fit-select");
    std::fs::create_dir_all(&dir)?;

    println!("fonts: {}", fonts.iter().map(|f| stem(f)).collect::<Vec<_>>().join(" "));
    println!("fixture: {repeats} rendering(s) of each cue");

    let generating = Instant::now();
    let sets = reference_sets(&fonts, &dir)?;
    let generating = generating.elapsed();

    let scanning = Instant::now();
    let mut trials = Vec::new();
    for material in &fonts {
        trials.push(trial(material, &sets, &dir, repeats).with_context(|| stem(material))?);
    }
    let scanning = scanning.elapsed();

    report_present(&trials);
    report_withheld(&trials);
    report_floor(&trials);
    report_margin(&trials);

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
        }
    }

    fn trial_of(material: &str, scores: Vec<Scored>) -> Trial {
        Trial { material: material.to_owned(), scores }
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
    fn charging_the_unmatched_glyphs_can_only_raise_a_score() {
        // The statistic exists because a mean over matched glyphs alone rewards a set that gave up.
        // Whatever else it does, it must never flatter a set relative to the mean it corrects.
        let s = scored("segoeui", 18.2, 16.9);
        assert!(charged_mean(&s) >= matched_mean(&s));
    }

    #[test]
    fn an_empty_candidate_list_has_no_argmin_rather_than_a_default_one() {
        let t = trial_of("arial", Vec::new());
        assert!(t.best(false, matched_mean).is_none());
        assert!(t.truth().is_none());
    }
}
