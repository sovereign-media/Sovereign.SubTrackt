//! Command-line surface.

use std::path::PathBuf;
use std::sync::OnceLock;

use clap::{Args, Parser, Subcommand, ValueEnum};
use subtrackt::config::DEFAULT_MIN_MATCHED;
use subtrackt::config::Defusing;
use subtrackt::{Config, LineScale, ProvenancePolicy, UnmatchedPolicy, VocabularyRules};
use subtrackt_core::SubtitleFormat;

use crate::style::When;

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

    /// Colour stderr by severity. Auto follows the terminal and honours `NO_COLOR`.
    #[arg(long, value_enum, value_name = "WHEN", default_value_t = When::Auto, global = true)]
    pub color: When,

    /// Draw a spinner and a progress bar on stderr.
    #[arg(long, value_enum, value_name = "WHEN", default_value_t = When::Auto, global = true)]
    pub progress: When,

    /// Neither colour nor progress: the output a redirected run already produces.
    #[arg(long, global = true)]
    pub plain: bool,
}

impl Cli {
    /// When to colour, with `--plain` folded in.
    ///
    /// `--plain` is one flag for the case detection gets wrong in both directions at once -- a CI
    /// runner with a pty allocated, a container with `TERM` set -- and it wins over the specific
    /// flags rather than losing to them, because there is no reading of `--plain --color always`
    /// where the user wanted colour.
    #[must_use]
    pub const fn color(&self) -> When {
        if self.plain { When::Never } else { self.color }
    }

    /// When to draw progress, with `--plain` folded in the same way.
    #[must_use]
    pub const fn progress(&self) -> When {
        if self.plain {
            When::Never
        } else {
            self.progress
        }
    }
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

    /// Choose which reference set reads a title best, from a directory of them.
    ///
    /// Scans the title once and scores every `.subtref` in the directory against it, then reports
    /// them best first. Nothing checks whether the winner is good enough — no measured statistic
    /// separates a good read from a bad one — so this proposes a set and leaves the decision with
    /// you. See issue 63.
    Fit(FitArgs),

    /// Dump raw glyph shapes, without trying to read them.
    ///
    /// One tab-separated row per glyph: cue, line, x, y, width, height, and the 256-bit feature
    /// vector as hex. Feature vectors are comparable across files, so this is what the typeface
    /// survey and the reference-set generator both work from.
    Glyphs(GlyphsArgs),

    /// Render a font into a reference glyph set.
    ///
    /// Nothing is embedded, so this is how a set comes to exist. Point it at one font for one
    /// `.subtref`, or at a directory of fonts for one set per font — which is what `fit` wants,
    /// since it scores a directory of candidates and proposes a winner.
    ///
    /// The vectors go through the same normalisation `extract` applies to a decoded subtitle
    /// bitmap. That is the whole reason this is a subcommand rather than a script: a reference
    /// built through any other transform produces distances that mean nothing.
    GenReference(GenReferenceArgs),
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
    #[arg(long, value_enum, default_value_t = Unmatched::Threshold)]
    pub on_unmatched: Unmatched,

    /// With `--on-unmatched threshold`, the fraction of glyphs that must match.
    #[arg(long, default_value_t = DEFAULT_MIN_MATCHED, value_parser = parse_ratio)]
    pub min_matched: f32,

    /// Include the glyph outline in the foreground mask as well as the fill.
    #[arg(long)]
    pub include_outline: bool,

    /// Retry a component the matcher cannot read as two characters that touched.
    #[arg(long, overrides_with = "no_defuse")]
    pub defuse: bool,

    /// Leave an unreadable component unread rather than trying to cut it.
    #[arg(long, overrides_with = "defuse")]
    pub no_defuse: bool,

    /// Read the track even if its declared language is in a script the reference set cannot spell.
    ///
    /// For a mistagged stream, or a set deliberately fitted to something the tag does not describe.
    /// The guard it turns off refuses only on a fact -- the set holds not one character of the
    /// declared script -- so overriding it asserts the container is wrong.
    #[arg(long)]
    pub ignore_declared_script: bool,

    /// Measure a line whose glyphs are all one height against the scale the track is drawn at.
    #[arg(long, overrides_with = "no_borrow_track_scale")]
    pub borrow_track_scale: bool,

    /// Leave such a line unmeasured, and match its glyphs on shape alone.
    #[arg(long, overrides_with = "borrow_track_scale")]
    pub no_borrow_track_scale: bool,

    /// Resolve ambiguous reads from the characters around them. See docs/post-correction.md.
    #[arg(long, overrides_with = "no_post_correct")]
    pub post_correct: bool,

    /// Leave ambiguous reads exactly as the matcher returned them.
    #[arg(long, overrides_with = "post_correct")]
    pub no_post_correct: bool,

    /// Also resolve a word-edge glyph from words the same track read clearly. Needs
    /// --post-correct.
    #[arg(long, overrides_with = "no_track_vocabulary")]
    pub track_vocabulary: bool,

    /// Do not consult the track's own vocabulary.
    #[arg(long, overrides_with = "track_vocabulary")]
    pub no_track_vocabulary: bool,

    /// Also read a one-character word of `l` as `I`. Needs --post-correct, and assumes English.
    #[arg(long, overrides_with = "no_lone_words")]
    pub lone_words: bool,

    /// Leave a one-character word exactly as the matcher read it.
    #[arg(long, overrides_with = "lone_words")]
    pub no_lone_words: bool,

    /// Assert the track is English, for the rules that need to know. The container is asked first.
    #[arg(long)]
    pub assume_english: bool,

    /// How many times a word must be read clearly before it counts as evidence.
    #[arg(long, default_value_t = VocabularyRules::default().min_occurrences)]
    pub vocab_min_occurrences: usize,

    /// Shortest word the vocabulary arm may correct, in characters.
    #[arg(long, default_value_t = VocabularyRules::default().min_len)]
    pub vocab_min_len: usize,

    /// Let a candidate match the start of a clear word rather than all of it, so `look` is
    /// supported by a clear `looking`.
    #[arg(long, overrides_with = "no_vocab_prefix")]
    pub vocab_prefix: bool,

    /// Require a candidate to match a clear word in full.
    #[arg(long, overrides_with = "vocab_prefix")]
    pub no_vocab_prefix: bool,

    /// Record what produced this file, and what it read, as a comment near the top.
    ///
    /// Forces one into `SubRip` too, which has no comment syntax -- see --no-provenance.
    #[arg(long, overrides_with = "no_provenance")]
    pub provenance: bool,

    /// Write no such comment, in any format.
    #[arg(long, overrides_with = "provenance")]
    pub no_provenance: bool,

    /// Print the extraction summary to stderr.
    #[arg(long)]
    pub report: bool,

    /// Reference glyph set to match against, as written by `subtrackt gen-reference`.
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

/// Arguments for `subtrackt gen-reference`.
#[derive(Debug, Args)]
pub struct GenReferenceArgs {
    /// Font file, or a directory of them.
    pub font: PathBuf,

    /// Where to write: a `.subtref` file for one font, a directory for a directory of fonts.
    pub output: PathBuf,

    /// Name recorded inside the set. Defaults to the font file's stem.
    ///
    /// This is what `fit` prints when it ranks candidates, so it is worth setting to something you
    /// will recognise in that table.
    #[arg(long)]
    pub name: Option<String>,

    /// Italic cut of the same typeface, contributing its own vector for every character.
    ///
    /// #66 is the measurement behind this: a track that changes style mid-film is read by whichever
    /// face is closer to the ink, so both can be present in one set at once.
    #[arg(long)]
    pub italic: Option<PathBuf>,

    /// Bold cut of the same typeface.
    #[arg(long)]
    pub bold: Option<PathBuf>,
}

/// Arguments for `subtrackt fit`.
#[derive(Debug, Args)]
pub struct FitArgs {
    /// Input file.
    pub input: PathBuf,

    /// Directory of candidate reference sets. Every `.subtref` in it is scored.
    #[arg(short, long)]
    pub references: PathBuf,

    /// Write the winning set here, so it can be passed straight to `extract --reference`.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Which subtitle stream to read. Defaults to the first bitmap stream.
    #[arg(short, long)]
    pub stream: Option<u32>,

    /// Stop after this many cues.
    ///
    /// Cues are spread evenly through a film, so a few hundred touches only that fraction of a
    /// multi-gigabyte file. A typeface does not change halfway through, so a prefix is a fair
    /// sample and a full pass is wasted work.
    #[arg(short, long, default_value_t = 400)]
    pub limit: usize,

    /// How many candidates to list. The winner is always listed.
    #[arg(long, default_value_t = 5)]
    pub show: usize,

    /// Include the glyph outline in the foreground mask as well as the fill.
    #[arg(long)]
    pub include_outline: bool,
}

impl FitArgs {
    /// The pipeline configuration these arguments describe.
    ///
    /// Has to match what `extract` will use, or the fit would rank candidates against a
    /// segmentation the extraction never performs.
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
    /// `SubRip`.
    Srt,
    /// `WebVTT`.
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

    /// Where a one-height line takes its scale from, resolved the same way and for the same reason.
    ///
    /// The default is `FromTheTrack`, so `--borrow-track-scale` changes nothing today. #184's
    /// measurement is what put it there — 75 cues better and 9 worse across the bench — and a
    /// caller who has pinned either behaviour should not have to notice if that moves.
    #[must_use]
    pub fn line_scale(&self) -> LineScale {
        if self.borrow_track_scale {
            LineScale::FromTheTrack
        } else if self.no_borrow_track_scale {
            LineScale::FromTheLineAlone
        } else {
            Config::default().line_scale
        }
    }

    /// Whether the de-fusing pass runs, resolved the same way and for the same reason.
    ///
    /// The default is `On`, so `--defuse` changes nothing today and exists for the same reason
    /// `--post-correct` does: the default is a measurement result rather than a fixed property.
    #[must_use]
    pub fn defuse(&self) -> Defusing {
        if self.defuse {
            Defusing::On
        } else if self.no_defuse {
            Defusing::Off
        } else {
            Config::default().defuse
        }
    }

    /// Whether the vocabulary arm runs, resolved the same way and for the same reason.
    #[must_use]
    pub fn track_vocabulary(&self) -> bool {
        if self.track_vocabulary {
            true
        } else if self.no_track_vocabulary {
            false
        } else {
            Config::default().track_vocabulary
        }
    }

    /// Whether the lone-word arm runs, resolved the same way.
    #[must_use]
    pub fn lone_words(&self) -> bool {
        if self.lone_words {
            true
        } else if self.no_lone_words {
            false
        } else {
            Config::default().lone_words
        }
    }

    /// Whether the vocabulary arm may match the start of a clear word, resolved the same way.
    ///
    /// This one is a pair rather than a bare flag because a bare flag *is* a decision: a plain
    /// `bool` defaults to `false`, so the CLI would have silently overridden the setting the sweep
    /// in `docs/post-correction.md` chose. Prefix matching found nine substitutions against exact
    /// matching's seven with zero cues made worse either way, and a caller reaching the library
    /// through the binary should get the measurement rather than clap's zero value.
    #[must_use]
    pub fn vocab_prefix(&self) -> bool {
        if self.vocab_prefix {
            true
        } else if self.no_vocab_prefix {
            false
        } else {
            VocabularyRules::default().prefix_match
        }
    }

    /// Whether a provenance comment is written, and in which formats.
    ///
    /// Three-valued rather than a bool, because the default is not the same for both formats and
    /// the reason is the formats themselves: `WebVTT` defines `NOTE` and `SubRip` defines no comment at
    /// all. So the default writes one where there is somewhere legal to put it, `--provenance`
    /// forces one into `SubRip` as leading text, and `--no-provenance` refuses both.
    #[must_use]
    pub fn provenance(&self) -> ProvenancePolicy {
        if self.provenance {
            ProvenancePolicy::Always
        } else if self.no_provenance {
            ProvenancePolicy::Never
        } else {
            Config::default().provenance
        }
    }

    /// Build the pipeline configuration these arguments describe.
    #[must_use]
    pub fn to_config(&self) -> Config {
        let mut config = Config {
            stream: self.stream,
            format: self.format.into(),
            defuse: self.defuse(),
            line_scale: self.line_scale(),
            post_correct: self.post_correct(),
            track_vocabulary: self.track_vocabulary(),
            lone_words: self.lone_words(),
            assume_english: self.assume_english,
            check_declared_script: !self.ignore_declared_script,
            provenance: self.provenance(),
            vocabulary: VocabularyRules {
                min_occurrences: self.vocab_min_occurrences,
                min_len: self.vocab_min_len,
                prefix_match: self.vocab_prefix(),
            },
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

    fn fit(args: &[&str]) -> FitArgs {
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Command::Fit(args) => args,
            other => panic!("expected fit, got {other:?}"),
        }
    }

    #[test]
    fn the_last_vocabulary_flag_on_the_line_wins() {
        // The same resolution the post-correction pair uses, and for the same reason: the default
        // is a measurement result rather than a fixed property, so a caller that has pinned the
        // behaviour it wants should not have to notice when it moves.
        assert!(
            extract(&["subtrackt", "extract", "a.sup", "--track-vocabulary"]).track_vocabulary()
        );
        assert!(
            !extract(&["subtrackt", "extract", "a.sup", "--no-track-vocabulary"])
                .track_vocabulary()
        );
        assert!(
            extract(&[
                "subtrackt",
                "extract",
                "a.sup",
                "--no-track-vocabulary",
                "--track-vocabulary",
            ])
            .track_vocabulary()
        );
        assert_eq!(
            extract(&["subtrackt", "extract", "a.sup"]).track_vocabulary(),
            Config::default().track_vocabulary,
            "with neither flag it follows the library"
        );
    }

    #[test]
    fn prefix_matching_follows_the_library_rather_than_clap_s_zero_value() {
        // This was a bare `--vocab-prefix` bool, so the CLI shipped prefix matching *off* while
        // the sweep in `docs/post-correction.md` had chosen it on. A plain bool cannot express
        // "unset": its absence and an explicit `false` are the same value, and the pair is what
        // separates them.
        assert_eq!(
            extract(&["subtrackt", "extract", "a.sup"]).vocab_prefix(),
            VocabularyRules::default().prefix_match,
            "with neither flag it follows the library"
        );
        assert!(extract(&["subtrackt", "extract", "a.sup", "--vocab-prefix"]).vocab_prefix());
        assert!(!extract(&["subtrackt", "extract", "a.sup", "--no-vocab-prefix"]).vocab_prefix());
        assert!(
            extract(&[
                "subtrackt",
                "extract",
                "a.sup",
                "--no-vocab-prefix",
                "--vocab-prefix"
            ])
            .vocab_prefix(),
            "the last flag on the line wins, as it does for every other pair"
        );
    }

    #[test]
    fn the_configuration_carries_the_resolved_vocabulary_rules() {
        // The resolver is only worth having if `to_config` calls it, which is the half that was
        // wrong: the three knobs sat beside each other and only two of them reached the library.
        let config = extract(&["subtrackt", "extract", "a.sup"]).to_config();
        assert_eq!(config.vocabulary, VocabularyRules::default());
    }

    #[test]
    fn fit_needs_an_input_and_a_directory_of_candidates() {
        let args = fit(&["subtrackt", "fit", "movie.mkv", "--references", "sets"]);
        assert_eq!(args.input, PathBuf::from("movie.mkv"));
        assert_eq!(args.references, PathBuf::from("sets"));
        assert!(args.output.is_none(), "writing the winner is opt-in");

        assert!(
            Cli::try_parse_from(["subtrackt", "fit", "movie.mkv"]).is_err(),
            "with nothing to choose between there is no fit to report"
        );
    }

    #[test]
    fn fit_samples_a_prefix_by_default_rather_than_reading_the_whole_film() {
        // A typeface does not change halfway through, and cues are spread evenly, so a few hundred
        // touches only that fraction of a multi-gigabyte file. On a real Blu-ray this is the
        // difference between three seconds and seventy.
        let args = fit(&["subtrackt", "fit", "movie.mkv", "-r", "sets"]);
        assert_eq!(args.limit, 400);
        assert_eq!(fit(&["subtrackt", "fit", "m.mkv", "-r", "s", "-l", "50"]).limit, 50);
    }

    #[test]
    fn fit_segments_the_way_extract_will() {
        // The fit ranks candidates against a segmentation; if it were not the one `extract`
        // performs, it would be ranking them for a different question than the one being asked.
        let args = fit(&["subtrackt", "fit", "m.mkv", "-r", "s", "--include-outline"]);
        assert!(args.to_config().binarize.include_outline);
        assert!(
            !fit(&["subtrackt", "fit", "m.mkv", "-r", "s"])
                .to_config()
                .binarize
                .include_outline
        );
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
    fn defaults_match_the_library_defaults_rather_than_restating_them() {
        // The gate default is a measurement result (docs/architecture.md), so the CLI has to track
        // it rather than carry its own copy that can drift.
        let args = extract(&["subtrackt", "extract", "movie.sup"]);
        let config = args.to_config();
        assert_eq!(config.unmatched, Config::default().unmatched);
        assert_eq!(
            config.unmatched,
            UnmatchedPolicy::Threshold { min_ratio: DEFAULT_MIN_MATCHED }
        );
        assert_eq!(config.format, SubtitleFormat::Srt);
        assert_eq!(config.post_correct, Config::default().post_correct);
        assert_eq!(config.lone_words, Config::default().lone_words);
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
    fn plain_means_both_the_others_never_whatever_they_were_set_to() {
        // One flag rather than two, because the case it exists for -- detection getting it wrong
        // -- gets both wrong at once.
        let cli = Cli::try_parse_from(["subtrackt", "list", "m.mkv", "--plain"]).unwrap();
        assert_eq!(cli.color(), When::Never);
        assert_eq!(cli.progress(), When::Never);

        let cli = Cli::try_parse_from([
            "subtrackt",
            "list",
            "m.mkv",
            "--plain",
            "--color",
            "always",
            "--progress",
            "always",
        ])
        .unwrap();
        assert_eq!(
            cli.color(),
            When::Never,
            "there is no reading of that where colour was wanted"
        );
        assert_eq!(cli.progress(), When::Never);
    }

    #[test]
    fn the_display_flags_are_global_and_default_to_auto() {
        // Global so they can be typed after the subcommand, which is where a hand reaches for them
        // when a run turns out to be noisier than expected.
        let cli =
            Cli::try_parse_from(["subtrackt", "extract", "m.sup", "--color", "never"]).unwrap();
        assert_eq!(cli.color(), When::Never);
        assert_eq!(cli.progress(), When::Auto);
        assert_eq!(
            Cli::try_parse_from(["subtrackt", "list", "m.mkv"])
                .unwrap()
                .color(),
            When::Auto
        );
    }

    #[test]
    fn list_takes_just_an_input() {
        let cli = Cli::try_parse_from(["subtrackt", "list", "movie.mkv"]).unwrap();
        assert!(matches!(cli.command, Command::List { .. }));
    }
}
