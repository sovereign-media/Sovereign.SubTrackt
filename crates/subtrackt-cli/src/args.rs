//! Command-line surface.

use std::path::PathBuf;
use std::sync::OnceLock;

use clap::{Args, Parser, Subcommand, ValueEnum};
use subtrackt::{Config, UnmatchedPolicy};
use subtrackt_core::SubtitleFormat;

/// What `--version` prints: the binary, and the reference data compiled into it.
///
/// Two different things decide what an extraction says, and they are fixed in different places. A
/// bad read is either the code or the data it matched against, and a version string naming only the
/// first leaves the second untraceable — which matters more here than usual, because the embedded
/// set is empty and a user seeing every glyph come back unmatched should be able to find out why
/// from the tool rather than from the source.
fn version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let set = subtrackt_glyph::reference::embedded();
        format!(
            "{} (reference set: {}, {} glyphs)",
            env!("CARGO_PKG_VERSION"),
            set.name(),
            set.len()
        )
    })
}

/// Extract plain text from bitmap image-based subtitle streams.
#[derive(Debug, Parser)]
#[command(name = "subtrackt", version = version(), about, long_about = None)]
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
///
/// The bool count is what a command line is: clap derives one field per flag, and grouping them
/// into a state enum to satisfy the lint would only move the flatness somewhere it reads worse.
#[allow(clippy::struct_excessive_bools)]
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

    /// Resolve ambiguous reads from the characters around them. See docs/post-correction.md.
    #[arg(long, overrides_with = "no_post_correct")]
    pub post_correct: bool,

    /// Leave ambiguous reads exactly as the matcher returned them.
    #[arg(long, overrides_with = "post_correct")]
    pub no_post_correct: bool,

    /// Print the extraction summary to stderr.
    #[arg(long)]
    pub report: bool,

    /// Reference glyph set to match against, as written by `xtask gen-reference`.
    ///
    /// Required in practice: nothing is embedded, and docs/reference-set.md records the
    /// measurement saying a shipped set would read worse than no set at all. Without this every
    /// glyph comes back unmatched, which is the honest answer rather than a broken one.
    #[arg(long)]
    pub reference: Option<PathBuf>,
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
    /// Whether post-correction runs, resolving the flag pair against the library default.
    ///
    /// Written so that flipping [`Config::post_correct`] flips the CLI with it. Both flags exist
    /// even though only one of them currently changes anything, because the default is a
    /// measurement result (see `docs/post-correction.md`) rather than a fixed property, and a
    /// caller that has pinned the behaviour it wants should not have to notice when it moves.
    /// `overrides_with` makes the last flag on the line the one that counts.
    #[must_use]
    pub fn post_correct(&self) -> bool {
        if self.post_correct {
            true
        } else if self.no_post_correct {
            false
        } else {
            Config::default().post_correct
        }
    }

    /// Build the pipeline configuration these arguments describe.
    #[must_use]
    pub fn to_config(&self) -> Config {
        let mut config = Config {
            stream: self.stream,
            format: self.format.into(),
            post_correct: self.post_correct(),
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
    fn the_version_names_the_reference_data_as_well_as_the_binary() {
        // A user whose every glyph comes back unmatched is looking at empty reference data, not at
        // a broken decoder, and the tool should be able to tell them that itself.
        let version = version();
        assert!(version.starts_with(env!("CARGO_PKG_VERSION")), "{version}");
        assert!(version.contains("reference set:"), "{version}");
        assert!(
            version.contains("empty"),
            "{version}: nothing is embedded, and it should say so"
        );
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
    fn both_post_correction_flags_work_and_the_last_one_wins() {
        let extract_with = |flags: &[&str]| {
            let mut argv = vec!["subtrackt", "extract", "movie.sup"];
            argv.extend_from_slice(flags);
            extract(&argv).to_config().post_correct
        };

        assert!(extract_with(&["--post-correct"]));
        assert!(!extract_with(&["--no-post-correct"]));
        assert_eq!(
            extract_with(&[]),
            Config::default().post_correct,
            "neither flag means whatever the measurement made the default"
        );

        // Both are accepted together rather than rejected, so a wrapper script can append one to a
        // command line that already carries the other. The rightmost is the answer.
        assert!(!extract_with(&["--post-correct", "--no-post-correct"]));
        assert!(extract_with(&["--no-post-correct", "--post-correct"]));
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
