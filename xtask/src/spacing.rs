//! How much room does the word-spacing decisiveness margin actually have?
//!
//! [#49](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/49). #40 replaced a fixed
//! multiple of the median gap with a split found in the line's own gap distribution, and gated that
//! split behind two tests: the cut must reach half the line's median glyph width, and twice the
//! median gap of the cluster below it. Both numbers were read off eight fixture lines, and the
//! tighter of the two margins is **0.61 against 0.50**.
//!
//! Eleven points on eight lines is not much evidence for the one part of the rule whose failure is
//! loud. A missed space reads as `foxjumps`, which is wrong but legible; a space invented inside a
//! word reads as garbage and costs two word errors. So the question is where the two populations
//! actually sit, and how much empty space lies between them.
//!
//! The fixture cannot answer it, because **every line in it has word gaps**. The case the test
//! exists for — a line holding one word, where every splitting criterion still finds a split — is
//! not in it at all. So this renders a corpus that has both, across typefaces and down to the 21px
//! the library survey measured, and runs it through the *real* pipeline: `.sup` in, segmentation
//! and grouping as shipped, glyph boxes out. The gaps measured here are the gaps the runtime sees,
//! not font advance metrics standing in for them.
//!
//! What it cannot do is answer #49 as filed. Studio-authored subtitles are the population in
//! question, and generated text is a model of them rather than a sample. This bounds the margin
//! from one direction — if it fails here it would certainly fail on real media — and #49 stays open
//! for the corpus run.

use std::path::PathBuf;

use anyhow::Context as _;
use fontdue::{Font, FontSettings};
use subtrackt::{Config, LayoutRules, Pipeline, UnmatchedPolicy};
use subtrackt_text::layout::split_threshold;

/// Sizes to render at, spanning what `docs/library-survey.md` measured on real material.
const SIZES: [f32; 4] = [21.0, 28.0, 42.0, 56.0];

/// Lines holding more than one word. The rule must find their breaks.
const MULTI_WORD: &[&str] = &[
    "The quick brown fox jumps",
    "over the lazy dog.",
    "- Is it 1 or l?",
    "- Neither: it is I.",
    "Café, naïve, jalapeño.",
    "0123456789 O o I l 1",
    "Follow the yellow line",
    "to Iowa in 2015.",
    "I don't know what you mean.",
    "Get out of the way!",
    "She said it was fine.",
    "A B C D E F G",
];

/// Lines holding exactly one word, which the accuracy fixture has none of.
///
/// This is the population #40's decisiveness test exists to protect, and the reason it is worth
/// rendering at all: real subtitles are full of them and a generated fixture is not.
const SINGLE_WORD: &[&str] = &[
    "Yes.",
    "No.",
    "Wait!",
    "Hello?",
    "Stop.",
    "Marseille",
    "Jonathan",
    "Impossible.",
    "Wonderful",
    "mmmmmm",
    "IIIIII",
    "Whatever",
    "1999",
    "jalapeño",
    "Understood.",
];

/// One line's measurements, as the runtime saw it.
struct Line {
    text: String,
    /// Whether the source text actually holds a word break. Ground truth, since we rendered it.
    has_break: bool,
    /// The cut the widest-jump criterion chose, before either decisiveness test.
    cut: u32,
    /// Median glyph width on the line.
    width: u32,
    /// Median gap of the cluster below the cut.
    cluster: u32,
    /// Whether the shipped rule, both tests included, would place spaces here.
    fires: bool,
}

impl Line {
    /// The cut as a fraction of median glyph width, in percent.
    fn width_ratio(&self) -> u32 {
        (self.cut * 100).checked_div(self.width).unwrap_or(0)
    }

    /// The cut as a fraction of the low cluster's median gap, in percent.
    fn cluster_ratio(&self) -> u32 {
        let cluster = self.cluster.max(1);
        self.cut * 100 / cluster
    }
}

/// The widest jump in a line's sorted gaps, and the cluster below it.
///
/// Deliberately a copy of the criterion in `subtrackt_text::layout` rather than a call into it: the
/// point here is to see the raw numbers *before* either decisiveness test is applied, and the
/// shipped function only reports the answer after them.
fn widest_jump(sorted: &[u32]) -> Option<(u32, u32)> {
    let mut best = None;
    let mut widest = 0;
    for index in 0..sorted.len().saturating_sub(1) {
        let jump = sorted[index + 1] - sorted[index];
        if jump > widest {
            widest = jump;
            best = Some(index);
        }
    }
    let cut = best?;
    let low = &sorted[..=cut];
    Some((sorted[cut + 1], low[low.len() / 2]))
}

/// Render one corpus of lines and measure every one of them through the real pipeline.
fn measure(font: &Font, name: &str, px: f32, texts: &[&str]) -> anyhow::Result<Vec<Line>> {
    let dir = std::env::temp_dir().join("subtrackt-spacing-margin");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}-{px}.sup"));

    // One line per cue, so a cue index is a line index and the mapping back to source text cannot
    // drift.
    let cues: Vec<(Vec<String>, f32)> = texts
        .iter()
        .map(|text| (vec![(*text).to_owned()], px))
        .collect();
    std::fs::write(&path, crate::fixture::build_sup(font, &cues, (1920, 1080))?)?;

    let config = Config { unmatched: UnmatchedPolicy::Placeholder, ..Config::default() };
    let survey = Pipeline::new(config)
        .survey(&path, None)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let rules = LayoutRules::default();
    let mut out = Vec::new();
    for (index, text) in texts.iter().enumerate() {
        let mut boxes: Vec<_> = survey
            .glyphs
            .iter()
            .filter(|g| g.cue == index && g.line == 0)
            .map(|g| g.bounds)
            .collect();
        if boxes.len() < 3 {
            continue;
        }
        boxes.sort_by_key(|b| b.x);

        let gaps: Vec<u32> = boxes
            .windows(2)
            .map(|pair| pair[1].x.saturating_sub(pair[0].right()))
            .collect();
        let mut widths: Vec<u32> = boxes.iter().map(|b| b.width).collect();
        widths.sort_unstable();
        let width = widths[widths.len() / 2];

        let mut sorted = gaps.clone();
        sorted.sort_unstable();
        let Some((cut, cluster)) = widest_jump(&sorted) else {
            continue;
        };

        out.push(Line {
            text: (*text).to_owned(),
            has_break: text.contains(' '),
            cut,
            width,
            cluster,
            fires: split_threshold(&gaps, width, rules).is_some(),
        });
    }
    Ok(out)
}

/// Print one population's spread of a ratio, so the trough between the two is visible.
fn spread(label: &str, values: &mut [u32]) {
    if values.is_empty() {
        println!("  {label:<24} (none)");
        return;
    }
    values.sort_unstable();
    let at = |p: usize| values[(values.len() - 1) * p / 100];
    println!(
        "  {label:<24} n={:<4} min={:<4} p5={:<4} p50={:<4} p95={:<4} max={}",
        values.len(),
        values[0],
        at(5),
        at(50),
        at(95),
        values[values.len() - 1]
    );
}

/// Run the margin check.
///
/// # Errors
/// Fails if no usable font can be found, or if a fixture cannot be built or read back.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let mut fonts: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if fonts.is_empty() {
        fonts = [
            "arial.ttf",
            "verdana.ttf",
            "tahoma.ttf",
            "trebuc.ttf",
            "segoeui.ttf",
        ]
        .iter()
        .map(|name| PathBuf::from(format!("C:/Windows/Fonts/{name}")))
        .filter(|p| p.exists())
        .collect();
    }
    if fonts.is_empty() {
        fonts = vec![crate::accuracy::find_font(None).context("no font found")?];
    }

    let mut lines = Vec::new();
    for path in &fonts {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        let name = path
            .file_stem()
            .map_or_else(|| "font".to_owned(), |s| s.to_string_lossy().into_owned());
        for px in SIZES {
            lines.extend(measure(&font, &name, px, MULTI_WORD)?);
            lines.extend(measure(&font, &name, px, SINGLE_WORD)?);
        }
    }

    let rules = LayoutRules::default();
    println!(
        "{} typefaces x {} sizes ({}-{}px), {} lines measured through the real pipeline",
        fonts.len(),
        SIZES.len(),
        SIZES[0],
        SIZES[SIZES.len() - 1],
        lines.len()
    );
    println!(
        "shipped thresholds: cut >= {}% of glyph width AND >= {}% of the low cluster",
        rules.split_min_width_percent, rules.split_min_cluster_percent
    );

    report(&lines, rules);
    Ok(())
}

/// Print both distributions, both failure classes, and how much room the thresholds have.
fn report(lines: &[Line], rules: LayoutRules) {
    println!("\n--- where the two populations sit, in percent ---");
    println!("  cut as a fraction of median glyph width");
    spread(
        "    lines with a break",
        &mut lines
            .iter()
            .filter(|l| l.has_break)
            .map(Line::width_ratio)
            .collect::<Vec<_>>(),
    );
    spread(
        "    lines without one",
        &mut lines
            .iter()
            .filter(|l| !l.has_break)
            .map(Line::width_ratio)
            .collect::<Vec<_>>(),
    );
    println!("  cut as a fraction of the low cluster's median gap");
    spread(
        "    lines with a break",
        &mut lines
            .iter()
            .filter(|l| l.has_break)
            .map(Line::cluster_ratio)
            .collect::<Vec<_>>(),
    );
    spread(
        "    lines without one",
        &mut lines
            .iter()
            .filter(|l| !l.has_break)
            .map(Line::cluster_ratio)
            .collect::<Vec<_>>(),
    );

    // The two failure modes, named rather than aggregated. Only the second is the loud one.
    let missed: Vec<&Line> = lines.iter().filter(|l| l.has_break && !l.fires).collect();
    let invented: Vec<&Line> = lines.iter().filter(|l| !l.has_break && l.fires).collect();
    println!("\n--- what the shipped rule does with them ---");
    println!(
        "  lines with a break, spaces found : {} of {}",
        lines.iter().filter(|l| l.has_break && l.fires).count(),
        lines.iter().filter(|l| l.has_break).count()
    );
    println!(
        "  lines without one, left alone    : {} of {}",
        lines.iter().filter(|l| !l.has_break && !l.fires).count(),
        lines.iter().filter(|l| !l.has_break).count()
    );
    for (label, group) in [("MISSED", &missed), ("INVENTED", &invented)] {
        for line in group.iter().take(10) {
            println!(
                "  {label:<8} {:<28} cut {} = {}% of width, {}% of cluster",
                line.text,
                line.cut,
                line.width_ratio(),
                line.cluster_ratio()
            );
        }
    }

    // Sensitivity: how far either threshold could move before the answer changes.
    println!("\n--- how much room the thresholds have ---");
    let mut with: Vec<u32> = lines
        .iter()
        .filter(|l| l.has_break)
        .map(Line::width_ratio)
        .collect();
    let mut without: Vec<u32> = lines
        .iter()
        .filter(|l| !l.has_break)
        .map(Line::width_ratio)
        .collect();
    with.sort_unstable();
    without.sort_unstable();
    match (with.first(), without.last()) {
        (Some(lowest_real), Some(highest_noise)) if lowest_real > highest_noise => {
            println!(
                "  width test: the populations do not overlap. Lines without a break reach {highest_noise}%,             lines with one start at {lowest_real}%, and the threshold sits at {}%.",
                rules.split_min_width_percent
            );
        }
        (Some(lowest_real), Some(highest_noise)) => {
            println!(
                "  width test: the populations OVERLAP. Lines without a break reach {highest_noise}%             and lines with one start at {lowest_real}%, so no threshold separates them.",
            );
        }
        _ => println!("  width test: not enough lines to say"),
    }
}
