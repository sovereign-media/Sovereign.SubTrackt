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

use std::path::{Path, PathBuf};

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
    /// How many glyphs stood on the line.
    glyphs: usize,
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
    let survey = Pipeline::new(config.clone())
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
            glyphs: boxes.len(),
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

/// Measure the lines of a real subtitle track rather than a generated one.
///
/// The half of #49 that generated text cannot supply, and the half that turned out to matter. There
/// is no ground truth here — nobody has transcribed the film — but the question is distributional
/// rather than about correctness: where does the cut sit relative to a glyph, and how much would
/// move if the threshold moved. Real dialogue supplies both populations by itself, since it is full
/// of single-word lines in a way a fixture is not.
///
/// A reference set is built from a font so the text is legible enough to eyeball where the spaces
/// landed. It will be a near miss for whatever the disc was authored in — that is #43's problem and
/// not this one's, and it does not touch the geometry these numbers come from.
fn media(path: &Path, font: &Path) -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join("subtrackt-spacing-margin");
    std::fs::create_dir_all(&dir)?;
    let reference_path = dir.join("reference.subtref");
    crate::gen_reference(&[
        font.display().to_string(),
        reference_path.display().to_string(),
        "--name".to_owned(),
        "spacing-margin".to_owned(),
    ])?;
    let reference = crate::util::load_reference(&reference_path)?;

    let config = Config { unmatched: UnmatchedPolicy::Placeholder, ..Config::default() };
    let survey = Pipeline::new(config.clone())
        .with_reference(reference.clone())
        .survey(path, None)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let rules = LayoutRules::default();
    let mut keys: Vec<(usize, usize)> = survey.glyphs.iter().map(|g| (g.cue, g.line)).collect();
    keys.sort_unstable();
    keys.dedup();

    let mut measured: Vec<Line> = Vec::new();
    for (cue, line) in keys {
        let mut boxes: Vec<_> = survey
            .glyphs
            .iter()
            .filter(|g| g.cue == cue && g.line == line)
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
        measured.push(Line {
            text: format!("cue {cue} line {line}"),
            has_break: false,
            cut,
            width,
            cluster,
            fires: split_threshold(&gaps, width, rules).is_some(),
            glyphs: boxes.len(),
        });
    }

    report_media(path, &measured, survey.cues, survey.glyphs.len());

    let outcome = Pipeline::new(config.clone())
        .with_reference(reference)
        .run(path)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("  first cues as extracted, to see where the spaces landed:");
    for cue in outcome.track.cues.iter().take(10) {
        for line in &cue.lines {
            println!("    {line}");
        }
    }
    Ok(())
}

/// Print what a real track's lines say about the two thresholds.
fn report_media(path: &Path, measured: &[Line], cues: usize, glyphs: usize) {
    let rules = LayoutRules::default();
    println!();
    println!("--- {} ---", path.display());
    println!("  {cues} cues, {glyphs} glyphs, {} lines measurable", measured.len());

    let mut ratios: Vec<u32> = measured.iter().map(Line::width_ratio).collect();
    spread("  cut / glyph width", &mut ratios);
    let mut cluster_ratios: Vec<u32> = measured.iter().map(Line::cluster_ratio).collect();
    spread("  cut / low cluster", &mut cluster_ratios);

    println!("  distribution of cut / glyph width, in bands of ten:");
    for band in 0..14u32 {
        let (low, high) = (band * 10, band * 10 + 9);
        let count = ratios.iter().filter(|r| **r >= low && **r <= high).count();
        if count > 0 {
            let bar: String =
                std::iter::repeat_n('#', (count * 60 / measured.len().max(1)).max(1)).collect();
            println!("    {low:>3}-{high:<3} {count:>5}  {bar}");
        }
    }

    let fires = measured.iter().filter(|l| l.fires).count();
    println!(
        "  the rule places spaces on {fires} lines and declines {}",
        measured.len() - fires
    );

    let disagree = measured
        .iter()
        .filter(|l| {
            let by_width = l.width_ratio() >= rules.split_min_width_percent;
            let by_cluster = l.cluster_ratio() >= rules.split_min_cluster_percent;
            by_width != by_cluster
        })
        .count();
    println!("  lines where the two tests disagree: {disagree}");

    // A declined line is the right answer for a one-word cue and the wrong one for a whole
    // sentence. Length is the cheap proxy: single words rarely run to fifteen glyphs, so a long
    // declined line is a run-together failure rather than a word left intact.
    let (short, long): (Vec<_>, Vec<_>) = measured
        .iter()
        .filter(|l| !l.fires)
        .partition(|l| l.glyphs < 15);
    println!(
        "  of {} declined, {} are short enough to be one word and {} long enough to be a miss",
        short.len() + long.len(),
        short.len(),
        long.len()
    );

    println!("  how many lines change answer if the width floor moves:");
    for floor in [30, 40, 50, 60, 70, 80] {
        let would = measured
            .iter()
            .filter(|l| {
                l.width_ratio() >= floor && l.cluster_ratio() >= rules.split_min_cluster_percent
            })
            .count();
        let mark = if floor == rules.split_min_width_percent {
            "  <- shipped"
        } else {
            ""
        };
        println!("    {floor:>3}   {would:>5} lines get spaces{mark}");
    }
}

/// Run the margin check.
///
/// # Errors
/// Fails if no usable font can be found, or if a fixture cannot be built or read back.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    // The value after `--media` is a path, not a font; excluding it keeps the positional arguments
    // meaning one thing.
    let media_at = args.iter().position(|a| a == "--media").map(|at| at + 1);
    let mut fonts: Vec<PathBuf> = args
        .iter()
        .enumerate()
        .filter(|(index, a)| !a.starts_with("--") && Some(*index) != media_at)
        .map(|(_, a)| PathBuf::from(a))
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

    if let Some(at) = args.iter().position(|a| a == "--media") {
        let media_path = args.get(at + 1).context("--media needs a path")?;
        let font = fonts
            .first()
            .cloned()
            .or_else(|| crate::accuracy::find_font(None))
            .context("no font found to build a reference set from")?;
        return media(Path::new(media_path), &font);
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
