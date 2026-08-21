//! Subtitle file writers.

pub mod srt;
pub mod vtt;

use subtrackt_core::{SubtitleFormat, TrackWriter};

pub use srt::SrtWriter;
pub use vtt::VttWriter;

/// The writer for a format.
#[must_use]
pub fn writer_for(format: SubtitleFormat) -> Box<dyn TrackWriter> {
    match format {
        SubtitleFormat::Srt => Box::new(SrtWriter),
        SubtitleFormat::Vtt => Box::new(VttWriter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{Confidence, Cue, TextTrack, TimeSpan, Timestamp};

    fn track() -> TextTrack {
        TextTrack::new(
            vec![Cue {
                span: TimeSpan::new(Timestamp::from_millis(0), Timestamp::from_millis(1_000)),
                lines: vec!["Hello".into()],
                confidence: Confidence { matched: 5, unmatched: 0, ambiguous: 0 },
                forced: false,
            }],
            None,
        )
    }

    #[test]
    fn each_format_gets_its_own_writer() {
        let srt = writer_for(SubtitleFormat::Srt).to_string(&track()).unwrap();
        let vtt = writer_for(SubtitleFormat::Vtt).to_string(&track()).unwrap();
        assert!(srt.starts_with('1'));
        assert!(vtt.starts_with("WEBVTT"));
    }
}
