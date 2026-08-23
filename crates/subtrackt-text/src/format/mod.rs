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

/// One rendered line, tagged if the cue says it leans.
///
/// Both formats spell an italic line the same way, so the decision about *whether* a line is italic
/// lives on the [`Cue`](subtrackt_core::Cue) and only the spelling lives here. A line that was not
/// shown to lean is written exactly as it was before #123 — no tag, no wrapper, byte for byte.
///
/// The tag goes around the whole line rather than around a run inside it. A line is the unit the
/// slant was measured over, and marking part of one would be a claim nothing here can make.
#[must_use]
pub fn tagged(text: &str, italic: bool) -> std::borrow::Cow<'_, str> {
    if italic {
        std::borrow::Cow::Owned(format!("<i>{text}</i>"))
    } else {
        std::borrow::Cow::Borrowed(text)
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
                italic: Vec::new(),
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
