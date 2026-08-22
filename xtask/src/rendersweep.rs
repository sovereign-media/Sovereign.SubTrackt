//! Pricing reference renderings end to end, on read text rather than on distances.
//!
//! [#99](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/99), second half. `xtask
//! reference-render` measures the gap between the reference side and the material side in cells,
//! and cells are not characters — the same lesson `docs/glyph-stability.md` records about
//! `xtask separability`:
//!
//! > "Ambiguous" and "wrong" are not the same set.
//!
//! It was true again here. The distance bench ranked a four-size material set best on both of its
//! columns, and the first thing that set did end to end was read every `t` as an `f`. So this
//! exists: the same candidate list, scored by generating a real `.subtref` and running the ceiling
//! fixture through it.
//!
//! #45 is the standing reason this has to be a sweep rather than an argument. Changing what a
//! reference vector *is* re-prices every threshold that was fitted against the old one, and the
//! last time that happened it cost up to 12.8 points of CER with nothing erroring.

use anyhow::{Context as _, bail};
use subtrackt::score::score_text;
use subtrackt_glyph::ReferenceSet;
use subtrackt_glyph::font::{Crop, Face, Rendering, generate_under};
use subtrackt_glyph::reference::Style;

/// The threshold the reference side has always used.
const REFERENCE_INK: u8 = 128;

/// Coverage above which a disc's fill counts as ink; the value `make-fixture` authors at.
///
/// Kept as a candidate axis because it was the obvious suspect and had to be ruled out, not because
/// it turned out to matter. It did not.
const MATERIAL_INK: u8 = 160;

const fn raster(px: f32, ink: u8) -> Rendering {
    Rendering { px, ink, crop: Crop::Raster }
}

const fn ink_box(px: f32, ink: u8) -> Rendering {
    Rendering { px, ink, crop: Crop::Ink }
}

/// Exactly what `gen-reference` wrote before #99.
const fn pre_99() -> Rendering {
    raster(96.0, REFERENCE_INK)
}

/// The candidates, arranged so the controls sit next to the thing they control for.
///
/// Rows 2 to 5 are the ones that matter, and they are a controlled experiment rather than a list of
/// guesses. A second entry that keeps the **same box** — at a different threshold, a different size,
/// or both — changes the disc's figures by *zero*. A second entry that changes the box, at any
/// threshold and any size, halves its character error. The box is the whole effect.
const CANDIDATES: [(&str, &[Rendering]); 11] = [
    ("96px raster box (pre-#99)", &[pre_99()]),
    ("96px ink box, alone", &[ink_box(96.0, REFERENCE_INK)]),
    ("both boxes (shipped)", &[pre_99(), ink_box(96.0, REFERENCE_INK)]),
    // Controls: a second entry that keeps the same box.
    ("+ raster, other threshold", &[pre_99(), raster(96.0, 140)]),
    ("+ raster, other size", &[pre_99(), raster(48.0, REFERENCE_INK)]),
    // The same change, reached by moving size and threshold as well as the box.
    ("+ ink box at 21px", &[pre_99(), ink_box(21.0, MATERIAL_INK)]),
    ("+ ink box at 50px", &[pre_99(), ink_box(50.0, MATERIAL_INK)]),
    ("+ ink box, material ink", &[pre_99(), ink_box(96.0, MATERIAL_INK)]),
    // A third box, further inset. Measured because "two helped, so three will help more" is exactly
    // the kind of reasoning this project keeps disproving.
    (
        "three boxes",
        &[pre_99(), ink_box(96.0, REFERENCE_INK), ink_box(96.0, 210)],
    ),
    // Material renderings without the historical entry. These read the full stop and lose the `t`.
    (
        "ink box 21+29+38+50px",
        &[
            ink_box(21.0, MATERIAL_INK),
            ink_box(29.0, MATERIAL_INK),
            ink_box(38.0, MATERIAL_INK),
            ink_box(50.0, MATERIAL_INK),
        ],
    ),
    (
        "ink box 21+50px",
        &[ink_box(21.0, MATERIAL_INK), ink_box(50.0, MATERIAL_INK)],
    ),
];

/// One candidate's showing on the ceiling fixture.
struct Scored {
    label: &'static str,
    entries: usize,
    bytes: usize,
    cer: f64,
    wer: f64,
    matched: u64,
    unmatched: u64,
    ambiguous: u64,
}

/// Generate a set per candidate and score the ceiling fixture through each.
///
/// # Errors
/// Fails if no usable font can be found, or if any generation or extraction step fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let font = crate::accuracy::find_font(args.first()).context(
        "no font found; pass one explicitly, e.g. xtask render-sweep C:/Windows/Fonts/arial.ttf",
    )?;
    let dir = std::env::temp_dir().join("subtrackt-render-sweep");
    std::fs::create_dir_all(&dir)?;

    println!("font: {}", font.display());
    crate::fixture::make(&[font.display().to_string(), dir.display().to_string()])?;
    let truth = std::fs::read_to_string(dir.join("synthetic.txt"))?;
    let truth = truth.trim();
    let sup = dir.join("synthetic.sup");
    let bytes = std::fs::read(&font).with_context(|| format!("reading {}", font.display()))?;

    let mut scored = Vec::new();
    for (label, renderings) in CANDIDATES {
        let faces = [Face { bytes: &bytes, style: Style::Regular }];
        let generated = generate_under("sweep", &faces, false, renderings)
            .with_context(|| format!("generating a set for {label}"))?;
        let encoded = generated.set.encode();
        let set = ReferenceSet::decode(&encoded).map_err(|e| anyhow::anyhow!("{e}"))?;

        // Post-correction off: it rewrites ambiguous characters, and this comparison is about what
        // the matcher itself reads. Grey off because that is what the pipeline ships with.
        let (text, outcome) = crate::accuracy::extract(&sup, set, false, false)?;
        let score = score_text(truth, text.trim());
        scored.push(Scored {
            label,
            entries: generated.set.len(),
            bytes: encoded.len(),
            cer: score.character_error_rate() * 100.0,
            wer: score.word_error_rate() * 100.0,
            matched: outcome.report.matched,
            unmatched: outcome.report.unmatched,
            ambiguous: outcome.report.ambiguous,
        });
        report_lines(label, truth, text.trim());
    }

    println!("\n--- reference renderings, on the ceiling fixture ---");
    println!(
        "  {:<28} {:>7} {:>8} {:>7} {:>7} {:>8} {:>9} {:>10}",
        "rendering", "entries", "bytes", "CER", "WER", "matched", "unmatched", "ambiguous"
    );
    for row in &scored {
        println!(
            "  {:<28} {:>7} {:>8} {:>6.1}% {:>6.1}% {:>8} {:>9} {:>10}",
            row.label,
            row.entries,
            row.bytes,
            row.cer,
            row.wer,
            row.matched,
            row.unmatched,
            row.ambiguous
        );
    }

    let best = scored
        .iter()
        .min_by(|a, b| a.cer.total_cmp(&b.cer))
        .context("nothing was scored")?;
    println!("\n  best on CER: {} at {:.1}%", best.label, best.cer);
    let baseline = scored.first().context("nothing was scored")?;
    println!(
        "  against the pre-#99 rendering's {:.1}%: {:+.1} points, unmatched {} -> {}",
        baseline.cer,
        best.cer - baseline.cer,
        baseline.unmatched,
        best.unmatched
    );
    if scored.is_empty() {
        bail!("no candidate produced a set");
    }
    Ok(())
}

/// Print the lines a candidate got wrong.
///
/// The aggregate cannot show a *systematic* substitution, and a systematic substitution is the
/// failure this project exists to avoid. Every candidate's misreads are printed for the same reason
/// `srt-score --compare` prints every cue that got worse.
fn report_lines(label: &str, truth: &str, text: &str) {
    let wrong: Vec<(&str, &str)> = truth
        .lines()
        .zip(text.lines())
        .filter(|(want, got)| want != got)
        .collect();
    println!(
        "\n--- {label}: {} of {} lines wrong ---",
        wrong.len(),
        truth.lines().count()
    );
    for (want, got) in wrong {
        println!("  ! {got}");
        println!("      want: {want}");
    }
}
