//! `SubRip` output.

use std::io::Write;

use subtrackt_core::{Error, Result, TextTrack, Timestamp, TrackWriter};

/// Writes a track as `SubRip`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SrtWriter;

/// Format a timestamp as `HH:MM:SS,mmm`.
#[must_use]
pub fn format_timestamp(timestamp: Timestamp) -> String {
    let (h, m, s, ms) = timestamp.hmsm();
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

impl TrackWriter for SrtWriter {
    fn write(&self, track: &TextTrack, out: &mut dyn Write) -> Result<()> {
        let io = |e: std::io::Error| Error::io("<srt output>", e);

        // Empty cues are skipped rather than written as blank blocks, and the numbering counts
        // what was written — a gap in SRT indices makes some players stop reading.
        let mut index = 0;
        for cue in track.cues.iter().filter(|c| !c.is_empty()) {
            index += 1;
            writeln!(out, "{index}").map_err(io)?;
            writeln!(
                out,
                "{} --> {}",
                format_timestamp(cue.span.start),
                format_timestamp(cue.span.end)
            )
            .map_err(io)?;
            for (index, line) in cue.lines.iter().enumerate() {
                writeln!(out, "{}", super::tagged(line, cue.line_is_italic(index))).map_err(io)?;
            }
            writeln!(out).map_err(io)?;
        }
        Ok(())
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
        let out = SrtWriter.to_string(&track).unwrap();
        assert_eq!(out, "1\n00:00:01,000 --> 00:00:03,500\nHello there.\n\n");
    }

    #[test]
    fn multi_line_cues_keep_their_line_breaks() {
        let track = TextTrack::new(vec![cue(0, 1_000, &["- Yes.", "- No."])], None);
        let out = SrtWriter.to_string(&track).unwrap();
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
        let out = SrtWriter.to_string(&track).unwrap();
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
        assert_eq!(SrtWriter.to_string(&TextTrack::default()).unwrap(), "");
    }

    #[test]
    fn a_leaning_line_is_written_as_an_italic_tag_and_an_upright_one_is_untouched() {
        let mut c = cue(0, 1_000, &["He is late.", "So am I."]);
        c.italic = vec![false, true];
        let out = SrtWriter.to_string(&TextTrack::new(vec![c], None)).unwrap();
        assert!(out.contains("\nHe is late.\n"), "{out}");
        assert!(out.contains("\n<i>So am I.</i>\n"), "{out}");
    }

    #[test]
    fn a_cue_with_no_flags_is_written_exactly_as_it_was_before_the_tag_existed() {
        // The compatibility claim, asserted rather than assumed: a track nothing measured must come
        // out byte for byte as it did.
        let track = TextTrack::new(vec![cue(0, 1_000, &["Plain text."])], None);
        assert!(!SrtWriter.to_string(&track).unwrap().contains("<i>"));
    }
}
