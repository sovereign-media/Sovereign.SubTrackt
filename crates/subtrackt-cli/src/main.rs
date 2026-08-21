//! Command-line front end.
//!
//! Deliberately thin. Everything of substance lives in the `subtrackt` library crate, because
//! whether this ships as a CLI at all is still open (#16) — a `cdylib` for P/Invoke would replace
//! this file and nothing else.

mod args;

use std::io::Write;

use anyhow::Context as _;
use clap::Parser as _;
use subtrackt::Pipeline;

use crate::args::{Cli, Command, ExtractArgs, GlyphsArgs};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::List { input } => list(&input),
        Command::Extract(args) => extract(&args),
        Command::Glyphs(args) => glyphs(&args),
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

    let outcome = Pipeline::new(config)
        .run(&args.input)
        .with_context(|| format!("extracting subtitles from {}", args.input.display()))?;

    let rendered = outcome.render(&config)?;
    write_output(args, &rendered)?;

    if args.report {
        eprintln!("{}", outcome.report);
    }
    Ok(())
}

/// Dump glyph shapes as tab-separated rows.
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
