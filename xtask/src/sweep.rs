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
    std::fs::create_dir_all(dir)?;
    let mut args = vec![font.display().to_string(), dir.display().to_string()];
    if repeats > 1 {
        args.push("--repeat".to_owned());
        args.push(repeats.to_string());
    }
    crate::fixture::make(&args)?;

    let reference_path = dir.join("reference.subtref");
    crate::gen_reference(&[
        reference_font.display().to_string(),
        reference_path.display().to_string(),
        "--name".to_owned(),
        "sweep".to_owned(),
    ])?;
    let reference = ReferenceSet::decode(&std::fs::read(&reference_path)?)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let truth = std::fs::read_to_string(dir.join("synthetic.txt"))?;
    Ok((dir.join("synthetic.sup"), truth, reference))
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
                thresholds.distance(&a.features, a.metrics, b),
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
