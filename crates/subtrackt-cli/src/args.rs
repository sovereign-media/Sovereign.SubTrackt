//! Command-line surface.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use subtrackt::{Config, UnmatchedPolicy};
use subtrackt_core::SubtitleFormat;

/// Extract plain text from bitmap image-based subtitle streams.
#[derive(Debug, Parser)]
#[command(name = "subtrackt", version, about, long_about = None)]
pub struct Cli {
    /// What to do.
    #[command(subcommand)]
    pub command: Command,

    /// Increase log verbosity. Repeatable.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// List the bitmap subtitle streams in a file.
    List {
        /// Input file: a container, a `.sup` dump, or a VOBSUB `.idx`/`.sub` pair.
        input: PathBuf,
    },

    /// Extract a subtitle stream to text.
    Extract(ExtractArgs),

    /// Dump raw glyph shapes, without trying to read them.
    ///
    /// One tab-separated row per glyph: cue, line, x, y, width, height, and the 256-bit feature
    /// vector as hex. Feature vectors are comparable across files, so this is what the typeface
    /// survey and the reference-set generator both work from.
    Glyphs(GlyphsArgs),
}

/// Arguments for `subtrackt extract`.
#[derive(Debug, Args)]
pub struct ExtractArgs {
    /// Input file.
    pub input: PathBuf,

    /// Where to write. Defaults to stdout.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Which subtitle stream to read. Defaults to the first bitmap stream.
    #[arg(short, long)]
    pub stream: Option<u32>,

    /// Output format.
    #[arg(short, long, value_enum, default_value_t = Format::Srt)]
    pub format: Format,

    /// What to do about glyphs the matcher cannot identify.
    #[arg(long, value_enum, default_value_t = Unmatched::FailTrack)]
    pub on_unmatched: Unmatched,

    /// With `--on-unmatched threshold`, the fraction of glyphs that must match.
    #[arg(long, default_value_t = 0.98, value_parser = parse_ratio)]
    pub min_matched: f32,

    /// Include the glyph outline in the foreground mask as well as the fill.
    #[arg(long)]
    pub include_outline: bool,

    /// Run post-correction on ambiguous reads. Off by default; see #12.
    #[arg(long)]
    pub post_correct: bool,

    /// Print the extraction summary to stderr.
    #[arg(long)]
    pub report: bool,
}

/// Reject a ratio outside `0.0..=1.0` at parse time rather than silently clamping later.
fn parse_ratio(raw: &str) -> Result<f32, String> {
    let value: f32 = raw
        .parse()
        .map_err(|_| format!("`{raw}` is not a number"))?;
    if (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(format!("`{raw}` is outside 0.0..=1.0"))
    }
}

/// Arguments for `subtrackt glyphs`.
#[derive(Debug, Args)]
pub struct GlyphsArgs {
    /// Input file.
    pub input: PathBuf,

    /// Which subtitle stream to read. Defaults to the first bitmap stream.
    #[arg(short, long)]
    pub stream: Option<u32>,

    /// Stop after this many cues.
    ///
    /// Cues are spread evenly through a film, so a few hundred touches only that fraction of a
    /// multi-gigabyte file. A typeface does not change halfway through.
    #[arg(short, long)]
    pub limit: Option<usize>,

    /// Include the glyph outline in the foreground mask as well as the fill.
    #[arg(long)]
    pub include_outline: bool,

    /// Print a one-line summary to stderr instead of per-glyph rows.
    #[arg(long)]
    pub summary: bool,
}

impl GlyphsArgs {
    /// The pipeline configuration these arguments describe.
    #[must_use]
    pub fn to_config(&self) -> Config {
        let mut config = Config { stream: self.stream, ..Config::default() };
        config.binarize.include_outline = self.include_outline;
        config
    }
}

/// Output format, as a CLI value.
///
/// These doc comments are what clap prints in `--help`, so they stay as plain prose — backticks
/// around the format names would satisfy `doc_markdown` and then leak into the help output.
#[allow(clippy::doc_markdown)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// SubRip.
    Srt,
    /// WebVTT.
    Vtt,
}

impl From<Format> for SubtitleFormat {
    fn from(format: Format) -> Self {
        match format {
            Format::Srt => Self::Srt,
            Format::Vtt => Self::Vtt,
        }
    }
}

/// Unmatched-glyph policy, as a CLI value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Unmatched {
    /// Omit any cue containing an unread glyph.
    Drop,
    /// Emit the cue with a placeholder character.
    Placeholder,
    /// Fail the whole track so the caller can fall back to burn-in.
    FailTrack,
    /// Fail only if the matched fraction falls below `--min-matched`.
    Threshold,
}

impl ExtractArgs {
    /// Build the pipeline configuration these arguments describe.
    #[must_use]
    pub fn to_config(&self) -> Config {
        let mut config = Config {
            stream: self.stream,
            format: self.format.into(),
            post_correct: self.post_correct,
            unmatched: match self.on_unmatched {
                Unmatched::Drop => UnmatchedPolicy::Drop,
                Unmatched::Placeholder => UnmatchedPolicy::Placeholder,
                Unmatched::FailTrack => UnmatchedPolicy::FailTrack,
                Unmatched::Threshold => UnmatchedPolicy::Threshold { min_ratio: self.min_matched },
            },
            ..Config::default()
        };
        config.binarize.include_outline = self.include_outline;
        config
    }

    /// The output path, defaulting to the input path with the format's extension.
    #[must_use]
    pub fn output_path(&self) -> Option<PathBuf> {
        self.output.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn extract(args: &[&str]) -> ExtractArgs {
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Command::Extract(args) => args,
            other => panic!("expected extract, got {other:?}"),
        }
    }

    #[test]
    fn the_cli_definition_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn defaults_match_the_conservative_library_defaults() {
        let args = extract(&["subtrackt", "extract", "movie.sup"]);
        let config = args.to_config();
        assert_eq!(config.unmatched, UnmatchedPolicy::FailTrack);
        assert_eq!(config.format, SubtitleFormat::Srt);
        assert!(!config.post_correct);
        assert!(!config.binarize.include_outline);
    }

    #[test]
    fn the_threshold_policy_picks_up_the_ratio_flag() {
        let args = extract(&[
            "subtrackt",
            "extract",
            "movie.sup",
            "--on-unmatched",
            "threshold",
            "--min-matched",
            "0.9",
        ]);
        assert_eq!(
            args.to_config().unmatched,
            UnmatchedPolicy::Threshold { min_ratio: 0.9 }
        );
    }

    #[test]
    fn a_ratio_outside_the_unit_interval_is_rejected_at_parse_time() {
        assert!(
            Cli::try_parse_from(["subtrackt", "extract", "movie.sup", "--min-matched", "1.5"])
                .is_err()
        );
        assert!(parse_ratio("0.5").is_ok());
        assert!(parse_ratio("not a number").is_err());
    }

    #[test]
    fn format_and_output_flags_are_carried_through() {
        let args = extract(&[
            "subtrackt",
            "extract",
            "movie.sup",
            "-f",
            "vtt",
            "-o",
            "out.vtt",
            "-s",
            "2",
        ]);
        let config = args.to_config();
        assert_eq!(config.format, SubtitleFormat::Vtt);
        assert_eq!(config.stream, Some(2));
        assert_eq!(args.output_path(), Some(PathBuf::from("out.vtt")));
    }

    #[test]
    fn glyphs_takes_a_cue_limit() {
        let cli =
            Cli::try_parse_from(["subtrackt", "glyphs", "movie.mkv", "--limit", "150"]).unwrap();
        match cli.command {
            Command::Glyphs(args) => {
                assert_eq!(args.limit, Some(150));
                assert_eq!(args.to_config().stream, None);
            }
            other => panic!("expected glyphs, got {other:?}"),
        }
    }

    #[test]
    fn list_takes_just_an_input() {
        let cli = Cli::try_parse_from(["subtrackt", "list", "movie.mkv"]).unwrap();
        assert!(matches!(cli.command, Command::List { .. }));
    }
}
