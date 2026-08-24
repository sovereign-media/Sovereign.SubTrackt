//! Choosing the clustering radius by measuring it.
//!
//! The radius in `subtrackt_glyph::cluster` is the one number the #10 redesign turns on, and it
//! sits between two failures. Too tight and a stream's own renderings of a character stay apart, so
//! clustering does nothing and each noisy vector is matched on its own. Too loose and different
//! characters merge, so a whole cluster takes one label and every glyph in it is wrong together.
//! Neither failure is visible in a cluster count; both are visible in CER.
//!
//! Two fixtures, because they fail in opposite directions and a radius has to survive both:
//!
//! - **Plain** — every character rendered once. There is no within-stream variation to absorb, so
//!   clustering can only cost. This bounds the damage.
//! - **Varied** — the cue set repeated at several rendering sizes, which is what a real stream
//!   looks like: one typeface and weight throughout, the same characters recurring as several
//!   slightly different rasterisations. This is where clustering can pay.
//!
//! A radius of zero reproduces the behaviour that existed before clustering — one label decision
//! per distinct shape — so it is the honest baseline rather than a synthetic one.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use subtrackt::score::score_text;
use subtrackt::{Config, Pipeline, UnmatchedPolicy};
use subtrackt_glyph::ReferenceSet;
use subtrackt_glyph::cluster::ClusterRules;
use subtrackt_glyph::matcher::MatchThresholds;

/// Radii to try, in percent of the feature vector.
const RADII: [u32; 8] = [0, 2, 4, 6, 8, 10, 12, 16];

/// Metric weights to try, in tenths of a percent of the feature vector per full cap height.
///
/// The unrounded-looking values are the hundredths-of-a-cell settings this sweep used before #45,
/// re-expressed. Each lands on exactly the same cell count at 16x16 as the setting it replaces — 0,
/// 10, 25, 50, 75, 100, 150 and 250 hundredths — so the rows still line up with the ones recorded
/// in `docs/glyph-stability.md`, and now the column means the same thing at any grid size.
const WEIGHTS: [u32; 8] = [0, 40, 98, 196, 293, 391, 586, 977];

/// Mark weights to try, in tenths of a percent of the feature vector per 100 points of slope.
///
/// Chosen to land on round cell counts at 16x16 — 0, 5, 10, 20, 30, 40, 50 and 75 per 100 points —
/// which puts the acute-against-grave gap #48 measured, 131 points, at 0 to 98 cells. That spans
/// everything from below the 7-cell ambiguity margin to well past the 51-cell match ceiling, so the
/// sweep sees the setting stop mattering at one end and start rejecting correct characters at the
/// other.
const MARK_WEIGHTS: [u32; 8] = [0, 20, 39, 78, 117, 156, 195, 293];

/// One fixture's outcome at one setting.
struct Row {
    setting: u32,
    scaled: u32,
    distinct_shapes: u64,
    clusters: u64,
    ambiguous: u64,
    cer: f64,
}

/// Build a fixture and its reference set, returning the `.sup` path and the ground truth.
fn fixture(
    font: &Path,
    reference_font: &Path,
    dir: &Path,
    repeats: usize,
) -> anyhow::Result<(PathBuf, String, ReferenceSet)> {
    fixture_at(font, reference_font, dir, repeats, None)
}

/// As [`fixture`], at a chosen rendering size.
///
/// The size is a variable rather than a constant for #48. A mark is a fixed *fraction* of a glyph,
/// so its absolute size follows the rendering size, and the whole question of whether its direction
/// survives is a question about how few pixels it lands on. `docs/library-survey.md` measured real
/// subtitle glyphs at 21 to 50 px; the fixture's own 42 is the comfortable end of that.
fn fixture_at(
    font: &Path,
    reference_font: &Path,
    dir: &Path,
    repeats: usize,
    px: Option<f32>,
) -> anyhow::Result<(PathBuf, String, ReferenceSet)> {
    std::fs::create_dir_all(dir)?;
    let mut args = vec![font.display().to_string(), dir.display().to_string()];
    if repeats > 1 {
        args.push("--repeat".to_owned());
        args.push(repeats.to_string());
    }
    if let Some(px) = px {
        args.push("--px".to_owned());
        args.push(px.to_string());
    }
    crate::fixture::make(&args)?;

    let reference_path = dir.join("reference.subtref");
    crate::gen_reference(&[
        reference_font.display().to_string(),
        reference_path.display().to_string(),
        "--name".to_owned(),
        "sweep".to_owned(),
    ])?;
    let reference = crate::util::load_reference(&reference_path)?;

    let truth = std::fs::read_to_string(dir.join("synthetic.txt"))?;
    Ok((dir.join("synthetic.sup"), truth, reference))
}

/// The accent-direction characters the fixture carries both members of.
///
/// Kept as pairs rather than as a flat list because the failure this counts is directional: a
/// matcher that read every grave as an acute would produce the right number of accented characters
/// overall, and only the split between the two would say so.
const ACCENT_PAIRS: [(char, char); 4] = [
    ('\u{e0}', '\u{e1}'),
    ('\u{e8}', '\u{e9}'),
    ('\u{f2}', '\u{f3}'),
    ('\u{f9}', '\u{fa}'),
];

/// How often each accent-direction character appears in a string.
fn accent_census(text: &str) -> Vec<(char, usize)> {
    ACCENT_PAIRS
        .iter()
        .flat_map(|(a, b)| [*a, *b])
        .map(|c| (c, text.matches(c).count()))
        .collect()
}

/// Did the accents come out, and did they come out leaning the right way?
///
/// The question the CER column cannot answer. A wrong-leaning accent is one character in a line of
/// thirty, so flipping every one of them moves CER by a point or two — well inside the spread
/// between the conditions this sweep runs. Counting the characters directly is the only way to see
/// whether the term has anything to do.
fn report_accents(
    sup: &Path,
    truth: &str,
    reference: &ReferenceSet,
    weights: &[u32],
) -> anyhow::Result<()> {
    println!(
        "
  the accented characters themselves, against {} of them in the ground truth:",
        accent_census(truth).iter().map(|(_, n)| n).sum::<usize>()
    );

    let mut header = String::new();
    for c in ACCENT_PAIRS.iter().flat_map(|(a, b)| [*a, *b]) {
        let _ = write!(header, "{c:>4}");
    }
    println!("  weight {header}");

    let mut truth_row = String::new();
    for (_, n) in accent_census(truth) {
        let _ = write!(truth_row, "{n:>4}");
    }
    println!("   truth {truth_row}");

    // Whether the term is armed at all. If the reference set carried no slopes, or carried the
    // same sign for both members of a pair, every row below would be identical for a reason that
    // has nothing to do with the material — and the conclusion drawn from them would be wrong.
    let mut slope_row = String::new();
    for c in ACCENT_PAIRS.iter().flat_map(|(a, b)| [*a, *b]) {
        match reference.entries().iter().find(|e| e.character == c) {
            Some(entry) if entry.mark.known => {
                let _ = write!(slope_row, "{:>4}", entry.mark.percent);
            }
            Some(_) => slope_row.push_str("   -"),
            None => slope_row.push_str("   ?"),
        }
    }
    println!("  slopes {slope_row}   (the reference set's own, so the term is armed)");

    for weight in weights {
        let config = Config {
            unmatched: UnmatchedPolicy::Placeholder,
            matching: MatchThresholds {
                mark_weight_permille: *weight,
                ..MatchThresholds::default()
            },
            clustering: ClusterRules { mark_weight_permille: *weight, ..ClusterRules::default() },
            ..Config::default()
        };
        let outcome = Pipeline::new(config)
            .with_reference(reference.clone())
            .run(sup)
            .context("extracting")?;
        let text = outcome
            .track
            .cues
            .iter()
            .map(subtrackt::core::Cue::text)
            .collect::<Vec<_>>()
            .join("\n");
        let mut row = String::new();
        for (_, n) in accent_census(&text) {
            let _ = write!(row, "{n:>4}");
        }
        println!("  {weight:>6} {row}");
    }
    Ok(())
}

/// Extract one fixture under one configuration and score it.
fn measure(
    sup: &Path,
    truth: &str,
    reference: &ReferenceSet,
    config: Config,
) -> anyhow::Result<Row> {
    let outcome = Pipeline::new(config)
        .with_reference(reference.clone())
        .run(sup)
        .context("extracting")?;

    let text = outcome
        .track
        .cues
        .iter()
        .map(subtrackt::core::Cue::text)
        .collect::<Vec<_>>()
        .join("\n");
    let score = score_text(truth.trim(), text.trim());

    Ok(Row {
        setting: 0,
        scaled: 0,
        distinct_shapes: outcome.report.distinct_shapes,
        clusters: outcome.report.clusters,
        ambiguous: outcome.report.ambiguous,
        cer: score.character_error_rate() * 100.0,
    })
}

/// Extract at one clustering radius.
fn measure_radius(
    sup: &Path,
    truth: &str,
    reference: &ReferenceSet,
    radius_percent: u32,
) -> anyhow::Result<Row> {
    let rules = ClusterRules { radius_percent, ..ClusterRules::default() };
    let config = Config {
        unmatched: UnmatchedPolicy::Placeholder,
        clustering: rules,
        ..Config::default()
    };
    let mut row = measure(sup, truth, reference, config)?;
    row.setting = radius_percent;
    row.scaled = rules.radius();
    Ok(row)
}

/// Extract at one line-metric weight.
fn measure_weight(
    sup: &Path,
    truth: &str,
    reference: &ReferenceSet,
    metric_weight_permille: u32,
) -> anyhow::Result<Row> {
    let thresholds = MatchThresholds { metric_weight_permille, ..MatchThresholds::default() };
    let config = Config {
        unmatched: UnmatchedPolicy::Placeholder,
        matching: thresholds,
        ..Config::default()
    };
    let mut row = measure(sup, truth, reference, config)?;
    row.setting = metric_weight_permille;
    // What an `o` against an `O` — 28 points of cap height — is worth in cells. Quoted in the
    // `o`/`O` gap rather than in the whole cap height the setting is priced against, because that
    // pair is what the term exists to separate and what #37 reported.
    row.scaled = 28 * thresholds.metric_weight() / 100;
    Ok(row)
}

/// Extract at one mark weight.
fn measure_mark_weight(
    sup: &Path,
    truth: &str,
    reference: &ReferenceSet,
    mark_weight_permille: u32,
) -> anyhow::Result<Row> {
    let thresholds = MatchThresholds { mark_weight_permille, ..MatchThresholds::default() };
    let config = Config {
        unmatched: UnmatchedPolicy::Placeholder,
        matching: thresholds,
        // Clustering keys on the mark as well, so it has to price it the same way or the two
        // stages would disagree about whether two glyphs are the same shape.
        clustering: ClusterRules { mark_weight_permille, ..ClusterRules::default() },
        ..Config::default()
    };
    let mut row = measure(sup, truth, reference, config)?;
    row.setting = mark_weight_permille;
    // What an acute against a grave is worth in cells. Quoted in that gap rather than in the 100
    // points the setting is priced against, because that pair is what the term exists to separate
    // and 131 points is what #48 measured it at.
    row.scaled = 131 * thresholds.mark_weight() / 100;
    Ok(row)
}

fn print_rows(label: &str, heading: &str, baseline_note: &str, rows: &[Row]) {
    println!(
        "
--- {label} ---"
    );
    println!("  {heading:<12}  shapes  clusters  ambiguous       CER");
    for row in rows {
        let baseline = rows.first().map_or(row.cer, |r| r.cer);
        let delta = row.cer - baseline;
        let mark = if row.setting == 0 {
            format!("  ({baseline_note})")
        } else {
            format!("  {delta:+.1}")
        };
        println!(
            "  {:>4} = {:>3}   {:>7}  {:>8}  {:>9}  {:>7.1}%{mark}",
            row.setting, row.scaled, row.distinct_shapes, row.clusters, row.ambiguous, row.cer
        );
    }
}

/// Print the closest pairs in the reference set.
///
/// This is the diagnostic that explains the sweep rather than merely reporting it. Clustering can
/// only work if a character's own renderings are closer to each other than the nearest *different*
/// character is — so the distances between the closest pairs are the ceiling on any radius, and if
/// they sit below the variation a stream contains, no radius exists that helps.
fn report_closest_pairs(reference: &ReferenceSet, radii: &[u32]) {
    let entries = reference.entries();
    let thresholds = MatchThresholds::default();

    let mut pairs: Vec<(u32, u32, char, char)> = Vec::new();
    for (index, a) in entries.iter().enumerate() {
        for b in &entries[index + 1..] {
            pairs.push((
                a.features.distance(&b.features),
                thresholds.distance(&a.features, a.metrics, a.mark, a.aspect, b),
                a.character,
                b.character,
            ));
        }
    }
    pairs.sort_unstable();

    println!(
        "
--- the closest pairs in the reference set ---"
    );
    println!("  pair    shape   +metrics");
    for (shape, with, a, b) in pairs.iter().take(12) {
        let verdict = if with == shape {
            "  <-- still tied"
        } else {
            ""
        };
        println!("  {a} / {b}  {shape:>5}   {with:>8}{verdict}");
    }

    let tied =
        |pick: fn(&(u32, u32, char, char)) -> u32| pairs.iter().filter(|p| pick(p) == 0).count();
    println!(
        "
  pairs at distance zero: {} by shape, {} once line metrics are counted",
        tied(|p| p.0),
        tied(|p| p.1)
    );

    println!(
        "
  pairs a clustering radius would merge, counting metrics:"
    );
    for radius_percent in radii.iter().filter(|r| **r > 0) {
        let radius =
            ClusterRules { radius_percent: *radius_percent, ..ClusterRules::default() }.radius();
        let merged = pairs
            .iter()
            .filter(|(_, with, _, _)| *with <= radius)
            .count();
        println!("    {radius_percent:>3}% = {radius:>3} cells: {merged} pairs");
    }
}

/// Sweep the clustering radius over both fixtures.
///
/// # Errors
/// Fails if no usable font can be found, or if any stage of generation or extraction fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let font = crate::accuracy::find_font(args.first()).context(
        "no font found; pass one explicitly, e.g. xtask cluster-sweep C:/Windows/Fonts/arial.ttf",
    )?;
    println!("font: {}", font.display());

    // A second typeface for the material, so the reference set is a near miss rather than an exact
    // one. This is the realistic condition and the one clustering is supposed to help with: when
    // the reference is exact, matching does not fail on variation and there is nothing to absorb.
    let other = crate::accuracy::find_font(args.get(1))
        .filter(|p| p != &font)
        .or_else(|| {
            [
                "C:/Windows/Fonts/verdana.ttf",
                "C:/Windows/Fonts/tahoma.ttf",
                "C:/Windows/Fonts/segoeui.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.exists() && p != &font)
        });
    match &other {
        Some(path) => println!("material typeface for the cross-font case: {}", path.display()),
        None => println!("no second typeface found; skipping the cross-font case"),
    }

    let root = std::env::temp_dir().join("subtrackt-sweep");
    let mut cases: Vec<(String, usize, PathBuf)> = vec![
        ("plain, reference typeface exact".to_owned(), 1, font.clone()),
        (
            "varied (5 renderings), reference typeface exact".to_owned(),
            5,
            font.clone(),
        ),
    ];
    if let Some(path) = other {
        cases.push(("plain, reference typeface a near miss".to_owned(), 1, path.clone()));
        cases.push((
            "varied (5 renderings), reference typeface a near miss".to_owned(),
            5,
            path,
        ));
    }

    let mut reported = false;
    for (label, repeats, material) in cases {
        let dir = root.join(format!(
            "{repeats}-{}",
            material.file_stem().unwrap_or_default().to_string_lossy()
        ));
        let (sup, truth, reference) = fixture(&material, &font, &dir, repeats)?;

        if !reported {
            report_closest_pairs(&reference, &RADII);
            reported = true;
        }

        let mut rows = Vec::new();
        for radius in RADII {
            rows.push(measure_radius(&sup, &truth, &reference, radius)?);
        }
        print_rows(&label, "radius", "baseline: no clustering", &rows);
    }

    println!(
        "\nA radius is only worth shipping if it does not hurt the plain fixture and helps the\n\
         varied one, since real streams are the varied case and a fixture is the plain one."
    );
    Ok(())
}

/// Sweep the line-metric weight over the same fixtures.
///
/// # Errors
/// As [`run`].
pub fn run_metric(args: &[String]) -> anyhow::Result<()> {
    let font = crate::accuracy::find_font(args.first()).context(
        "no font found; pass one explicitly, e.g. xtask metric-sweep C:/Windows/Fonts/arial.ttf",
    )?;
    println!("reference typeface: {}", font.display());

    let other = [
        "C:/Windows/Fonts/verdana.ttf",
        "C:/Windows/Fonts/tahoma.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.exists() && p != &font);

    let root = std::env::temp_dir().join("subtrackt-metric-sweep");
    let mut cases: Vec<(String, usize, PathBuf)> = vec![
        ("plain, reference typeface exact".to_owned(), 1, font.clone()),
        (
            "varied (5 renderings), reference typeface exact".to_owned(),
            5,
            font.clone(),
        ),
    ];
    if let Some(path) = other {
        println!("material typeface for the cross-font case: {}", path.display());
        cases.push(("plain, reference typeface a near miss".to_owned(), 1, path.clone()));
        cases.push((
            "varied (5 renderings), reference typeface a near miss".to_owned(),
            5,
            path,
        ));
    }

    for (label, repeats, material) in cases {
        let dir = root.join(format!(
            "{repeats}-{}",
            material.file_stem().unwrap_or_default().to_string_lossy()
        ));
        let (sup, truth, reference) = fixture(&material, &font, &dir, repeats)?;

        let mut rows = Vec::new();
        for weight in WEIGHTS {
            rows.push(measure_weight(&sup, &truth, &reference, weight)?);
        }
        print_rows(&label, "weight", "baseline: shape only", &rows);
    }

    let shipped = MatchThresholds::default();
    println!(
        "
The weight is in tenths of a percent of the feature vector per full cap height. The second
         column is what that makes an `o` against an `O` — 28 points of cap height — worth in
         cells, against an ambiguity margin of {} and a match ceiling of {}.",
        shipped.ambiguity_margin(),
        shipped.max_distance()
    );
    Ok(())
}

/// Sweep the mark weight over the same fixtures.
///
/// The #37 pattern applied to #48. `xtask separability` established that the mark's direction
/// separates the pairs; it cannot say what the term should be *worth*, because separability is a
/// property of the reference set and the price is a property of the matcher. Only CER can answer
/// that, and only on material where both members of a pair appear — which is what the last three
/// fixture cues are for.
///
/// # Errors
/// As [`run`].
pub fn run_mark(args: &[String]) -> anyhow::Result<()> {
    let font = crate::accuracy::find_font(args.first()).context(
        "no font found; pass one explicitly, e.g. xtask mark-sweep C:/Windows/Fonts/arial.ttf",
    )?;
    println!("reference typeface: {}", font.display());

    let other = [
        "C:/Windows/Fonts/verdana.ttf",
        "C:/Windows/Fonts/tahoma.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.exists() && p != &font);

    let root = std::env::temp_dir().join("subtrackt-mark-sweep");

    // Rendering size is a variable here, unlike every other sweep, and it is the variable that
    // matters. A mark is a fixed fraction of a glyph, so at the small end of the range
    // `docs/library-survey.md` measured — 21 px — it lands on three or four pixels, and both the
    // shape difference the term is meant to replace and the direction the term reads get harder at
    // once. The fixture's own 42 px is the comfortable end.
    let sizes: [(&str, Option<f32>); 2] = [("42px", None), ("21px", Some(21.0))];

    let mut materials: Vec<(String, PathBuf)> = vec![("exact".to_owned(), font.clone())];
    if let Some(path) = other {
        println!("material typeface for the cross-font case: {}", path.display());
        materials.push(("a near miss".to_owned(), path));
    }

    for (size_label, px) in sizes {
        for (match_label, material) in &materials {
            for repeats in [1usize, 5] {
                let dir = root.join(format!(
                    "{size_label}-{repeats}-{}",
                    material.file_stem().unwrap_or_default().to_string_lossy()
                ));
                let (sup, truth, reference) = fixture_at(material, &font, &dir, repeats, px)?;

                let variation = if repeats > 1 {
                    "varied (5 renderings)"
                } else {
                    "plain"
                };
                let label = format!("{size_label}, {variation}, reference typeface {match_label}");

                let mut rows = Vec::new();
                for weight in MARK_WEIGHTS {
                    rows.push(measure_mark_weight(&sup, &truth, &reference, weight)?);
                }
                print_rows(&label, "weight", "baseline: mark ignored", &rows);
                report_accents(&sup, &truth, &reference, &[0, 20, 39, 78, 293])?;
            }
        }
    }

    let shipped = MatchThresholds::default();
    println!(
        "
The weight is in tenths of a percent of the feature vector per 100 points of slope. The
         second column is what that makes an acute against a grave — 131 points — worth in cells,
         against an ambiguity margin of {} and a match ceiling of {}. A setting past the ceiling
         does not merely demote a wrong-leaning accent, it rejects the character outright.",
        shipped.ambiguity_margin(),
        shipped.max_distance()
    );
    Ok(())
}
