//! Subtitle file writers.

pub mod srt;
pub mod vtt;

use std::io::Write;

use subtrackt_core::{
    Cue, Error, Provenance, Result, SubtitleFormat, TextTrack, Timestamp, TrackWriter,
};

pub use srt::SrtWriter;
pub use vtt::VttWriter;

/// The writer for a format, with no provenance note.
#[must_use]
pub fn writer_for(format: SubtitleFormat) -> Box<dyn TrackWriter> {
    writer_with_provenance(format, None)
}

/// The writer for a format, carrying a note it should record.
///
/// Split from [`writer_for`] rather than replacing it because most callers — the fitter, the
/// surveys, every test that checks cue layout — want the bytes the format defines and nothing
/// else. A note is something a caller asks for.
#[must_use]
pub fn writer_with_provenance(
    format: SubtitleFormat,
    provenance: Option<Provenance>,
) -> Box<dyn TrackWriter> {
    match format {
        SubtitleFormat::Srt => Box::new(SrtWriter { provenance }),
        SubtitleFormat::Vtt => Box::new(VttWriter { provenance }),
    }
}

/// What separates one subtitle format from the other.
///
/// The two writers used to spell out about eighty percent of each other: the same error closure,
/// the same provenance block guarded on emptiness, the same `filter(|c| !c.is_empty())` cue loop,
/// the same enumerated per-line `tagged` write, the same trailing blank. [`TrackWriter`] was
/// already the right trait and `writer_with_provenance` already the right dispatch — the
/// duplication was one level below, where two implementations of one procedure differed in four
/// details and repeated everything else.
///
/// These are those four details. Anything a third format would need that is not here is a sign
/// this is the wrong shape, not a reason to copy [`write_track`] again.
struct Spelling {
    /// Written once before any cue. `WebVTT` has a magic line and an optional language; `SubRip`
    /// has nothing at all.
    header: fn(&TextTrack, &mut dyn Write) -> std::io::Result<()>,
    /// How a comment is spelled, or `None` where the format has no syntax for one.
    ///
    /// The asymmetry #129 settled. `WebVTT` has `NOTE`; `SubRip` has nowhere to put a line that is
    /// not a cue, so a note there is leading text that a strict parser may reject — which is why
    /// it needs the blank line after it and why it is off unless asked for.
    note: fn(&Provenance, &mut dyn Write) -> std::io::Result<()>,
    /// Whether cues are numbered, counting **what was written** rather than what was iterated: a
    /// gap in `SubRip` indices makes some players stop reading.
    numbered: bool,
    /// The character between seconds and milliseconds. The whole of the timing difference.
    decimal: char,
    /// Anything the format appends to the timing line, such as `WebVTT`'s cue settings.
    settings: fn(&Cue, &mut dyn Write) -> std::io::Result<()>,
    /// How cue text is escaped. `WebVTT` treats `<`, `>` and `&` as markup; `SubRip` does not.
    escape: fn(&str) -> std::borrow::Cow<'_, str>,
}

/// Write a track, in whichever format `spelling` describes.
///
/// The order of the two transformations on a line is load-bearing and is why `escape` and
/// [`tagged`] are separate steps here rather than one: the text is escaped **first** and tagged
/// **second**, so the tag survives and anything the text itself held that looked like markup does
/// not become any.
fn write_track(
    track: &TextTrack,
    spelling: &Spelling,
    provenance: Option<&Provenance>,
    out: &mut dyn Write,
    what: &'static str,
) -> Result<()> {
    let io = |e: std::io::Error| Error::io(what, e);

    (spelling.header)(track, out).map_err(io)?;
    if let Some(note) = provenance.filter(|n| !n.is_empty()) {
        (spelling.note)(note, out).map_err(io)?;
    }

    let mut index = 0;
    for cue in track.cues.iter().filter(|c| !c.is_empty()) {
        index += 1;
        if spelling.numbered {
            writeln!(out, "{index}").map_err(io)?;
        }
        write!(
            out,
            "{} --> {}",
            timestamp(cue.span.start, spelling.decimal),
            timestamp(cue.span.end, spelling.decimal)
        )
        .map_err(io)?;
        (spelling.settings)(cue, out).map_err(io)?;
        writeln!(out).map_err(io)?;

        for (line, text) in cue.lines.iter().enumerate() {
            let escaped = (spelling.escape)(text);
            writeln!(out, "{}", tagged(&escaped, cue.line_is_italic(line))).map_err(io)?;
        }
        writeln!(out).map_err(io)?;
    }
    Ok(())
}

/// A timestamp as `HH:MM:SS<decimal>mmm`.
///
/// One function taking the separator, rather than one per format. `SubRip` writes a comma and
/// `WebVTT` a full stop, and that was the entire difference between two copies of this.
#[must_use]
pub fn timestamp(at: Timestamp, decimal: char) -> String {
    let (h, m, s, ms) = at.hmsm();
    format!("{h:02}:{m:02}:{s:02}{decimal}{ms:03}")
}

/// One rendered line, tagged if the cue says it leans.
///
/// Both formats spell an italic line the same way, so the decision about *whether* a line is italic
/// lives on the [`Cue`] and only the spelling lives here. A line that was not
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
    use subtrackt_core::{Confidence, Cue, Provenance, TextTrack, TimeSpan, Timestamp};

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
    fn a_webvtt_note_sits_after_the_header_and_before_the_first_cue() {
        // Where the format says a NOTE block may go. Before the WEBVTT line it would invalidate
        // the file; inside the cue list it would attach to a cue.
        let note = Provenance::new(["made by something", "from some set"]);
        let out = writer_with_provenance(SubtitleFormat::Vtt, Some(note))
            .to_string(&track())
            .unwrap();
        assert!(
            out.starts_with(
                "WEBVTT

NOTE
made by something
from some set

"
            ),
            "{out}"
        );
        assert!(out.contains("00:00:00.000 --> 00:00:01.000"), "{out}");
    }

    #[test]
    fn a_subrip_note_is_followed_by_a_blank_line_before_the_first_index() {
        // SubRip has no comment syntax, so the note is leading text and the blank line is the only
        // thing that stops a lenient parser folding it into cue one's dialogue.
        let note = Provenance::new(["made by something"]);
        let out = writer_with_provenance(SubtitleFormat::Srt, Some(note))
            .to_string(&track())
            .unwrap();
        assert_eq!(out.lines().next(), Some("made by something"), "{out}");
        assert_eq!(out.lines().nth(1), Some(""), "a parser resynchronises on this");
        assert_eq!(out.lines().nth(2), Some("1"), "{out}");
    }

    #[test]
    fn no_note_leaves_both_formats_byte_for_byte_as_they_were() {
        // The property that lets this ship without moving any figure the project has published.
        for format in [SubtitleFormat::Srt, SubtitleFormat::Vtt] {
            let plain = writer_for(format).to_string(&track()).unwrap();
            let none = writer_with_provenance(format, None)
                .to_string(&track())
                .unwrap();
            assert_eq!(plain, none);
        }
    }

    #[test]
    fn an_empty_note_writes_nothing_rather_than_an_empty_comment() {
        let out = writer_with_provenance(SubtitleFormat::Vtt, Some(Provenance::default()))
            .to_string(&track())
            .unwrap();
        assert!(!out.contains("NOTE"), "{out}");
        assert_eq!(out, writer_for(SubtitleFormat::Vtt).to_string(&track()).unwrap());
    }

    #[test]
    fn each_format_gets_its_own_writer() {
        let srt = writer_for(SubtitleFormat::Srt).to_string(&track()).unwrap();
        let vtt = writer_for(SubtitleFormat::Vtt).to_string(&track()).unwrap();
        assert!(srt.starts_with('1'));
        assert!(vtt.starts_with("WEBVTT"));
    }
}
