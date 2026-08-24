//! `SubRip` output.

use std::io::Write;

use subtrackt_core::{Provenance, Result, TextTrack, Timestamp, TrackWriter};

/// Writes a track as `SubRip`.
///
/// The provenance note is `None` by default, and that default is the conservative one on purpose:
/// **`SubRip` has no comment syntax.** There is nowhere in the format for a line that is not a cue,
/// so a note here is text before the first index — which our own parser skips, which most parsers
/// skip, and which a strict one is entitled to reject. #129 shipped it behind a flag for exactly
/// that reason, off unless asked for, while `WebVTT` gets a note by default because `NOTE` is part
/// of that format.
#[derive(Debug, Clone, Default)]
pub struct SrtWriter {
    /// A note to write before the first cue, or `None` to write none.
    pub provenance: Option<Provenance>,
}

/// How `SubRip` spells the four things that differ between formats.
const SPELLING: super::Spelling = super::Spelling {
    // No header at all: the file starts at cue one.
    header: |_, _| Ok(()),
    // No comment syntax either, so a note is leading text. The blank line after it matters more
    // than the note does: a parser scanning for its next cue resynchronises on it, and without one
    // a lenient parser folds the note into cue one's dialogue and a viewer reads our version
    // string as a line of the film.
    note: |note, out| {
        for line in &note.lines {
            writeln!(out, "{line}")?;
        }
        writeln!(out)
    },
    numbered: true,
    decimal: ',',
    // Nothing on the timing line, and no markup in the text.
    settings: |_, _| Ok(()),
    escape: |text| std::borrow::Cow::Borrowed(text),
};

/// Format a timestamp as `HH:MM:SS,mmm`.
#[must_use]
pub fn format_timestamp(timestamp: Timestamp) -> String {
    super::timestamp(timestamp, ',')
}

impl TrackWriter for SrtWriter {
    fn write(&self, track: &TextTrack, out: &mut dyn Write) -> Result<()> {
        super::write_track(track, &SPELLING, self.provenance.as_ref(), out, "<srt output>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{Confidence, Cue, TimeSpan};

    fn cue(start_ms: u64, end_ms: u64, lines: &[&str]) -> Cue {
        Cue {
            span: TimeSpan::new(Timestamp::from_millis(start_ms), Timestamp::from_millis(end_ms)),
            lines: lines.iter().map(|l| (*l).to_owned()).collect(),
            italic: Vec::new(),
            confidence: Confidence::default(),
            forced: false,
        }
    }

    #[test]
    fn writes_the_canonical_block_layout() {
        let track = TextTrack::new(vec![cue(1_000, 3_500, &["Hello there."])], None);
        let out = SrtWriter::default().to_string(&track).unwrap();
        assert_eq!(out, "1\n00:00:01,000 --> 00:00:03,500\nHello there.\n\n");
    }

    #[test]
    fn multi_line_cues_keep_their_line_breaks() {
        let track = TextTrack::new(vec![cue(0, 1_000, &["- Yes.", "- No."])], None);
        let out = SrtWriter::default().to_string(&track).unwrap();
        assert!(out.contains("- Yes.\n- No.\n\n"));
    }

    #[test]
    fn indices_are_contiguous_even_when_empty_cues_are_dropped() {
        let track = TextTrack::new(
            vec![
                cue(0, 1_000, &["one"]),
                cue(1_000, 2_000, &["   "]),
                cue(2_000, 3_000, &["two"]),
            ],
            None,
        );
        let out = SrtWriter::default().to_string(&track).unwrap();
        assert!(out.starts_with("1\n"), "{out}");
        assert!(
            out.contains("\n2\n"),
            "second written cue must be index 2, not 3:\n{out}"
        );
        assert!(!out.contains("\n3\n"));
    }

    #[test]
    fn hours_are_formatted_with_a_comma_decimal_separator() {
        assert_eq!(format_timestamp(Timestamp::from_millis(3_723_004)), "01:02:03,004");
    }

    #[test]
    fn an_empty_track_writes_nothing_at_all() {
        assert_eq!(
            SrtWriter::default()
                .to_string(&TextTrack::default())
                .unwrap(),
            ""
        );
    }

    #[test]
    fn a_leaning_line_is_written_as_an_italic_tag_and_an_upright_one_is_untouched() {
        let mut c = cue(0, 1_000, &["He is late.", "So am I."]);
        c.italic = vec![false, true];
        let out = SrtWriter::default()
            .to_string(&TextTrack::new(vec![c], None))
            .unwrap();
        assert!(out.contains("\nHe is late.\n"), "{out}");
        assert!(out.contains("\n<i>So am I.</i>\n"), "{out}");
    }

    #[test]
    fn a_cue_with_no_flags_is_written_exactly_as_it_was_before_the_tag_existed() {
        // The compatibility claim, asserted rather than assumed: a track nothing measured must come
        // out byte for byte as it did.
        let track = TextTrack::new(vec![cue(0, 1_000, &["Plain text."])], None);
        assert!(
            !SrtWriter::default()
                .to_string(&track)
                .unwrap()
                .contains("<i>")
        );
    }
}
