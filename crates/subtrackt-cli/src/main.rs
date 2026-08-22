//! Command-line front end.
//!
//! Deliberately thin. Everything of substance lives in the `subtrackt` library crate. That was
//! originally because whether this shipped as a CLI at all was open; #16 settled it on the CLI, and
//! the thinness is worth keeping regardless — `docs/distribution.md` has the numbers.

mod args;

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use clap::Parser as _;
use subtrackt::Pipeline;
use subtrackt_glyph::ReferenceSet;
use subtrackt_glyph::font::{Face, generate};
use subtrackt_glyph::reference::Style;

use crate::args::{Cli, Command, ExtractArgs, FitArgs, GenReferenceArgs, GlyphsArgs};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::List { input } => list(&input),
        Command::Extract(args) => extract(&args),
        Command::Fit(args) => fit(&args),
        Command::Glyphs(args) => glyphs(&args),
        Command::GenReference(args) => gen_reference(&args),
    }
}

/// Logs go to stderr so that piping extracted text to a file stays clean.
fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(format!("subtrackt={level}")));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

fn list(input: &std::path::Path) -> anyhow::Result<()> {
    let streams = Pipeline::list(input)
        .with_context(|| format!("listing subtitle streams in {}", input.display()))?;

    if streams.is_empty() {
        println!("no bitmap subtitle streams found");
        return Ok(());
    }

    for stream in streams {
        println!(
            "{:>3}  {:<20} {:<5} {}x{}{}",
            stream.index,
            stream.codec.ffmpeg_name(),
            stream.language.as_deref().unwrap_or("--"),
            stream.plane_width,
            stream.plane_height,
            stream.title.map(|t| format!("  {t}")).unwrap_or_default(),
        );
    }
    Ok(())
}

fn extract(args: &ExtractArgs) -> anyhow::Result<()> {
    let config = args.to_config();

    let mut pipeline = Pipeline::new(config);
    if let Some(path) = &args.reference {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading reference set {}", path.display()))?;
        let set = subtrackt::core::Result::from(ReferenceSet::decode(&bytes))
            .with_context(|| format!("parsing reference set {}", path.display()))?;
        eprintln!("reference set: {} ({} glyphs)", set.name(), set.len());
        pipeline = pipeline.with_reference(set);
    }

    let outcome = pipeline
        .run(&args.input)
        .with_context(|| format!("extracting subtitles from {}", args.input.display()))?;

    let rendered = outcome.render(&config)?;
    write_output(args, &rendered)?;

    // Every correction, individually, whenever post-correction ran. A stage allowed to rewrite
    // text has to leave a trace of what it rewrote, and a count alone is not one: `3 corrections`
    // cannot be checked by anybody, and `'I' -> 'l' in "jalapeño"` can.
    for correction in &outcome.corrections {
        tracing::info!("post-correction: {correction}");
    }

    if args.report {
        eprintln!("{}", outcome.report);
        for correction in &outcome.corrections {
            eprintln!("  {correction}");
        }
    }
    Ok(())
}

/// Dump glyph shapes as tab-separated rows.
/// Every `.subtref` in a directory, loaded and named after its file.
///
/// A candidate that will not decode is skipped with a count rather than failing the run. A
/// directory of reference sets is something a user accumulates over time, and one stale file in it
/// should not stop the other fifty being considered — the count is reported so a silently smaller
/// candidate list is still visible.
fn load_candidates(dir: &Path) -> anyhow::Result<(Vec<ReferenceSet>, Vec<String>)> {
    let mut sets = Vec::new();
    let mut skipped = Vec::new();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("subtref"))
        })
        .collect();
    // Sorted so a run over the same directory is reproducible before the scores even arrive.
    paths.sort();

    for path in paths {
        let name = path
            .file_stem()
            .map_or_else(|| "unnamed".to_owned(), |s| s.to_string_lossy().into_owned());
        match std::fs::read(&path)
            .ok()
            .and_then(|bytes| ReferenceSet::decode(&bytes).ok())
        {
            Some(set) => sets.push(set),
            None => skipped.push(name),
        }
    }
    Ok((sets, skipped))
}

fn fit(args: &FitArgs) -> anyhow::Result<()> {
    let (candidates, skipped) = load_candidates(&args.references)?;
    if candidates.is_empty() {
        bail!(
            "no usable reference set in {} — generate one with `subtrackt gen-reference <font> <dir>`",
            args.references.display()
        );
    }

    let survey = Pipeline::new(args.to_config())
        .survey(&args.input, Some(args.limit))
        .with_context(|| format!("surveying glyphs in {}", args.input.display()))?;
    if survey.glyphs.is_empty() {
        bail!(
            "{} segmented into no glyphs, so there is nothing to fit against",
            args.input.display()
        );
    }

    let thresholds = args.to_config().matching;
    let (ranked, unusable) = subtrackt::rank(&survey, candidates, thresholds)?;
    let Some(winner) = ranked.first() else {
        bail!("no candidate could be scored against this title");
    };

    eprintln!(
        "{} cues, {} glyphs, {} distinct shapes",
        survey.cues,
        survey.glyphs.len(),
        survey.distinct_shapes()
    );
    if !skipped.is_empty() {
        eprintln!(
            "  {} file(s) in {} are not reference sets and were skipped: {}",
            skipped.len(),
            args.references.display(),
            skipped.join(" ")
        );
    }
    if unusable > 0 {
        eprintln!(
            "  {unusable} set(s) were built for a different grid size and cannot be compared"
        );
    }

    eprintln!("\n  {:<24} {:>8} {:>10}", "reference set", "score", "read");
    for fit in ranked.iter().take(args.show.max(1)) {
        eprintln!("  {:<24} {:>8.1} {:>9.1}%", fit.name, fit.score, fit.coverage() * 100.0);
    }

    // The point of the whole command, and the sentence that keeps it honest. #63 measured four
    // statistics and none of them separates a good read from a bad one, so the score below ranks
    // candidates against each other and says nothing about whether the winner is any good.
    eprintln!(
        "\n  score is mean distance per glyph, charging unread glyphs the {}-cell ceiling.",
        thresholds.max_distance()
    );
    eprintln!("  Lower fits better. Nothing here checks whether the winner is good enough --");
    eprintln!("  no measured statistic can. Read a few cues before trusting a track to it.");

    match &args.output {
        Some(path) => {
            let source = args.references.join(format!("{}.subtref", winner.name));
            std::fs::copy(&source, path)
                .with_context(|| format!("copying {} to {}", source.display(), path.display()))?;
            eprintln!("\n  wrote {} to {}", winner.name, path.display());
            eprintln!(
                "  subtrackt extract {} --reference {}",
                args.input.display(),
                path.display()
            );
        }
        None => eprintln!(
            "\n  subtrackt extract {} --reference {}",
            args.input.display(),
            args.references
                .join(format!("{}.subtref", winner.name))
                .display()
        ),
    }
    Ok(())
}

fn glyphs(args: &GlyphsArgs) -> anyhow::Result<()> {
    let survey = Pipeline::new(args.to_config())
        .survey(&args.input, args.limit)
        .with_context(|| format!("surveying glyphs in {}", args.input.display()))?;

    eprintln!(
        "{}	{}	lang={}	{}x{}	cues={}	glyphs={}	shapes={}",
        args.input.display(),
        survey.stream.codec.ffmpeg_name(),
        survey.stream.language.as_deref().unwrap_or("-"),
        survey.stream.plane_width,
        survey.stream.plane_height,
        survey.cues,
        survey.glyphs.len(),
        survey.distinct_shapes(),
    );
    if args.summary {
        return Ok(());
    }

    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for glyph in &survey.glyphs {
        writeln!(
            out,
            "{}	{}	{}	{}	{}	{}	{}",
            glyph.cue,
            glyph.line,
            glyph.bounds.x,
            glyph.bounds.y,
            glyph.bounds.width,
            glyph.bounds.height,
            subtrackt::survey::vector_hex(&glyph.features),
        )
        .context("writing glyph rows")?;
    }
    out.flush().context("flushing glyph rows")
}

fn write_output(args: &ExtractArgs, rendered: &str) -> anyhow::Result<()> {
    if let Some(path) = args.output_path() {
        return std::fs::write(&path, rendered)
            .with_context(|| format!("writing {}", path.display()));
    }

    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(rendered.as_bytes())
        .context("writing to stdout")?;
    stdout.flush().context("flushing stdout")
}

/// Font file extensions worth trying in a directory.
///
/// Collections (`.ttc`) are left out: they hold several faces behind one file and picking the first
/// silently would put a face in the set under a name that does not describe it.
const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "TTF", "OTF"];

/// Render a font, or a directory of fonts, into reference sets.
fn gen_reference(args: &GenReferenceArgs) -> anyhow::Result<()> {
    if args.font.is_dir() {
        if args.name.is_some() || args.italic.is_some() || args.bold.is_some() {
            bail!(
                "--name, --italic and --bold describe one typeface, so they cannot be combined                  with a directory of fonts"
            );
        }
        return gen_reference_dir(&args.font, &args.output);
    }

    let mut faces = vec![(args.font.clone(), Style::Regular)];
    for (path, style) in [
        (args.italic.as_ref(), Style::Italic),
        (args.bold.as_ref(), Style::Bold),
    ] {
        if let Some(path) = path {
            faces.push((path.clone(), style));
        }
    }

    let name = match &args.name {
        Some(name) => name.clone(),
        None => stem(&args.font),
    };
    let written = write_set(&name, &faces, &args.output)?;
    eprintln!("{} glyphs -> {}", written, args.output.display());
    Ok(())
}

/// One `.subtref` per font in `dir`, written into `out`.
fn gen_reference_dir(dir: &Path, out: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;

    let mut fonts: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| FONT_EXTENSIONS.contains(&e))
        })
        .collect();
    // Sorted so the same directory yields the same set of files in the same order every run, which
    // matters when the output is going to be ranked and compared.
    fonts.sort();

    if fonts.is_empty() {
        bail!("no .ttf or .otf files in {}", dir.display());
    }

    let mut made = 0usize;
    let mut skipped = Vec::new();
    for font in &fonts {
        let name = stem(font);
        let target = out.join(format!("{name}.subtref"));
        match write_set(&name, &[(font.clone(), Style::Regular)], &target) {
            Ok(glyphs) => {
                made += 1;
                eprintln!("  {name}: {glyphs} glyphs");
            }
            // One unreadable font in a directory of forty is not a reason to produce nothing. It is
            // named rather than swallowed, and if every font fails the command still fails.
            Err(e) => skipped.push(format!("{name}: {e:#}")),
        }
    }

    for skip in &skipped {
        eprintln!("  skipped {skip}");
    }
    if made == 0 {
        bail!("none of the {} fonts in {} could be read", fonts.len(), dir.display());
    }
    eprintln!("{made} reference sets -> {}", out.display());
    Ok(())
}

/// Generate one set from `faces` and write it to `out`, returning how many glyphs it holds.
fn write_set(name: &str, faces: &[(PathBuf, Style)], out: &Path) -> anyhow::Result<usize> {
    let mut loaded = Vec::new();
    for (path, style) in faces {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        loaded.push((*style, bytes));
    }
    let faces: Vec<Face<'_>> = loaded
        .iter()
        .map(|(style, bytes)| Face { bytes, style: *style })
        .collect();

    // `false` rather than a flag: `extract` has no grey-coverage switch, so a set built that way
    // could never be matched by this binary. The knob stays in xtask, where both sides can move
    // together.
    let generated = generate(name, &faces, false)?;
    for style in &generated.without_cap_height {
        eprintln!(
            "  {name}: the {style:?} face rasterises no capital H; entries carry no line metrics"
        );
    }
    if !generated.missing.is_empty() {
        // Space and a few marks legitimately have no outline; anything else is worth knowing about
        // before the set is used to read a film.
        eprintln!(
            "  {name}: no outline for {} characters: {:?}",
            generated.missing.len(),
            generated.missing
        );
    }

    std::fs::write(out, generated.set.encode())
        .with_context(|| format!("writing {}", out.display()))?;
    Ok(generated.set.len())
}

/// A path's file stem, or `unnamed` if it has none.
fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "unnamed".to_owned(), |s| s.to_string_lossy().into_owned())
}
