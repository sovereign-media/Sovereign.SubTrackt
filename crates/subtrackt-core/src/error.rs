//! The single error type crossing stage boundaries.
//!
//! Written by hand rather than derived. Every library crate in this workspace has zero
//! dependencies, which is what makes the single-static-binary goal in the architecture document
//! achievable and keeps the eventual `cdylib` (#16) free of transitive surprises. `thiserror` would
//! be the obvious convenience here and it is not worth the precedent.

use std::fmt;
use std::path::PathBuf;

/// Convenience alias used throughout the workspace.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Anything that can go wrong between a container on disk and a text track.
///
/// Variants are grouped by pipeline stage so a caller can decide policy per stage: a
/// [`Error::Demux`] failure means the file is unusable, whereas [`Error::UnmatchedGlyph`] is
/// routine and is governed by the unmatched-glyph policy rather than aborting the run.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The input file could not be read.
    Io {
        /// The file being read when the failure occurred.
        path: PathBuf,
        /// The underlying operating system error.
        source: std::io::Error,
    },

    /// The container could not be parsed, or holds no subtitle stream we can use.
    Demux(String),

    /// A subtitle packet was structurally invalid (truncated segment, bad RLE run, ...).
    MalformedPacket {
        /// The codec whose parser rejected the packet, e.g. `pgs`.
        codec: &'static str,
        /// Presentation timestamp of the offending packet, in 90 kHz ticks.
        pts: u64,
        /// What the parser objected to.
        reason: String,
    },

    /// A glyph was segmented cleanly but matched nothing in the reference set.
    ///
    /// This is the failure mode that distinguishes a glyph matcher from a general OCR engine: it
    /// is *detectable*, so the caller can fall back rather than emit a plausible-but-wrong read.
    /// See the accuracy-gate discussion in the architecture document.
    UnmatchedGlyph {
        /// Hamming distance to the closest reference vector, for diagnostics.
        best_distance: u32,
    },

    /// The track was read and the accuracy gate refused it.
    ///
    /// Distinct from [`Error::UnmatchedGlyph`], which is one glyph finding nothing within
    /// threshold. This is the whole track being abandoned in favour of a fallback, and it carries
    /// the numbers behind that decision because the caller is being asked to do expensive work —
    /// burn-in — and deserves to know how close the read came rather than only that it lost.
    TrackRejected {
        /// The policy that refused it.
        policy: &'static str,
        /// Glyphs identified within threshold.
        matched: u64,
        /// Glyphs with no reference within threshold.
        unmatched: u64,
        /// Fraction the policy required, in `0.0..=1.0`.
        required: f32,
    },

    /// The track declares a language whose script the reference set holds no character of.
    ///
    /// Refused before a packet is decoded, and it is the one refusal in this pipeline that rests on
    /// evidence from *outside* the read. Every other gate counts something the matcher produced,
    /// and #218 measured why that cannot work here: mean match distance for five non-Latin tracks
    /// on one disc is 26.5 to 37.3, and for the same English track read with six wrong typefaces it
    /// is 23.1 to 34.9. A wrong script and a wrong typeface are the same event to anything
    /// downstream — the set cannot spell this track — so no statistic over the read can name which.
    /// The container's declared tag can.
    ///
    /// Distinct from [`Error::TrackRejected`] on purpose: that is a fraction falling below a floor,
    /// and this is a fact about two things that were never comparable.
    ScriptMismatch {
        /// The language tag the container declared.
        declared: String,
        /// The script that language is written in.
        script: crate::Script,
        /// The reference set that cannot spell it.
        reference_set: String,
    },

    /// The requested codec, container or output format is recognised but not yet supported.
    Unsupported {
        /// Human-readable description of the unsupported thing.
        what: String,
        /// The issue number tracking the work.
        issue: u32,
    },

    /// Configuration was internally inconsistent.
    Config(String),
}

impl Error {
    /// Build an [`Error::Unsupported`] for a not-yet-implemented pipeline stage.
    #[must_use]
    pub fn unsupported(what: impl Into<String>, issue: u32) -> Self {
        Self::Unsupported { what: what.into(), issue }
    }

    /// Attach a path to an [`std::io::Error`].
    #[must_use]
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, .. } => write!(f, "failed to read {}", path.display()),
            Self::Demux(reason) => write!(f, "demux failed: {reason}"),
            Self::MalformedPacket { codec, pts, reason } => {
                write!(f, "malformed {codec} packet at pts {pts}: {reason}")
            }
            Self::UnmatchedGlyph { best_distance } => {
                write!(f, "no reference glyph within threshold (best distance {best_distance})")
            }
            Self::TrackRejected { policy, matched, unmatched, required } => {
                let total = matched + unmatched;
                #[allow(clippy::cast_precision_loss)]
                let read = if total == 0 {
                    0.0
                } else {
                    *matched as f32 / total as f32
                };
                write!(
                    f,
                    "track rejected by the {policy} gate: {matched} of {total} glyphs read \
                     ({:.1}%), floor is {:.1}%",
                    read * 100.0,
                    required * 100.0
                )
            }
            Self::ScriptMismatch { declared, script, reference_set } => write!(
                f,
                "the track declares {declared}, which is written in {script}, and the reference \
                 set {reference_set} holds no {script} character at all: it would read as \
                 whatever the set does hold rather than fail"
            ),
            Self::Unsupported { what, issue } => {
                write!(f, "{what} is not supported yet (tracking issue #{issue})")
            }
            Self::Config(reason) => write!(f, "invalid configuration: {reason}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unsupported_stage_names_its_tracking_issue_in_the_message() {
        let err = Error::unsupported("PGS run-length decoding", 2);
        assert_eq!(
            err.to_string(),
            "PGS run-length decoding is not supported yet (tracking issue #2)"
        );
    }

    #[test]
    fn an_io_error_keeps_the_path_and_chains_to_the_os_error() {
        use std::error::Error as _;
        let err = Error::io("movie.sup", std::io::ErrorKind::NotFound.into());
        assert!(err.to_string().contains("movie.sup"));
        assert!(
            err.source().is_some(),
            "the OS error must stay reachable for context chains"
        );
    }

    #[test]
    fn a_malformed_packet_reports_where_in_the_stream_it_was() {
        let err = Error::MalformedPacket {
            codec: "pgs",
            pts: 1_234,
            reason: "run overflows its line".into(),
        };
        let message = err.to_string();
        assert!(message.contains("pgs"), "{message}");
        assert!(message.contains("1234"), "{message}");
    }

    #[test]
    fn errors_without_a_cause_report_no_source() {
        use std::error::Error as _;
        assert!(Error::Config("bad".into()).source().is_none());
    }
}
