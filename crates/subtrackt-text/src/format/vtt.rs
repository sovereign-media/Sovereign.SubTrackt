//! `WebVTT` output.
//!
//! This is the format that makes the whole exercise worth it: a `WebVTT` rendition is something
//! every client can already render, whereas a burned-in bitmap is not.

use std::io::Write;

use subtrackt_core::{Provenance, Result, TextTrack, Timestamp, TrackWriter};

/// Writes a track as `WebVTT`.
///
/// Unlike [`SrtWriter`](super::SrtWriter) this format has a comment syntax, so a provenance note
/// costs nothing in validity. It is still `None` here rather than always-on, because the writer
/// does not know what the caller wants recorded and an empty note would be worse than none.
#[derive(Debug, Clone, Default)]
pub struct VttWriter {
    /// A note to write after the header, or `None` to write none.
    pub provenance: Option<Provenance>,
}

/// How `WebVTT` spells the four things that differ between formats.
const SPELLING: super::Spelling = super::Spelling {
    header: |track, out| {
        writeln!(out, "WEBVTT")?;
        if let Some(language) = &track.language {
            writeln!(out, "Language: {language}")?;
        }
        writeln!(out)
    },
    // `NOTE` is part of the format, so this side needs no apology and no flag. The block runs from
    // the keyword to the next blank line, which is why the note's own lines can carry no `-->` --
    // `Provenance::new` takes that out.
    note: |note, out| {
        writeln!(out, "NOTE")?;
        for line in &note.lines {
            writeln!(out, "{line}")?;
        }
        writeln!(out)
    },
    numbered: false,
    decimal: '.',
    // Forced subtitles carry meaning a player can act on, so keep the distinction rather than
    // flattening every cue into the same track.
    settings: |cue, out| {
        if cue.forced {
            write!(out, " line:-1 align:center")?;
        }
        Ok(())
    },
    escape: |text| std::borrow::Cow::Owned(escape(text)),
};

/// Format a timestamp as `HH:MM:SS.mmm`.
#[must_use]
pub fn format_timestamp(timestamp: Timestamp) -> String {
    super::timestamp(timestamp, '.')
}

/// Escape the three characters `WebVTT` treats as cue-text markup.
///
/// Subtitle text does contain `<` and `&` — a dialogue line ending in `>>` for a speaker change is
/// common in captions — and emitting them raw produces a cue that renders as a broken tag.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

impl TrackWriter for VttWriter {
    fn write(&self, track: &TextTrack, out: &mut dyn Write) -> Result<()> {
        super::write_track(track, &SPELLING, self.provenance.as_ref(), out, "<vtt output>")
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
    fn writes_a_header_and_a_cue() {
        let track = TextTrack::new(vec![cue(1_000, 3_500, &["Hello there."])], None);
        let out = VttWriter::default().to_string(&track).unwrap();
        assert_eq!(out, "WEBVTT\n\n00:00:01.000 --> 00:00:03.500\nHello there.\n\n");
    }

    #[test]
    fn a_declared_language_lands_in_the_header() {
        let track = TextTrack::new(vec![cue(0, 1_000, &["Bonjour"])], Some("fr".into()));
        assert!(
            VttWriter::default()
                .to_string(&track)
                .unwrap()
                .contains("Language: fr\n")
        );
    }

    #[test]
    fn markup_characters_in_dialogue_are_escaped() {
        let track = TextTrack::new(vec![cue(0, 1_000, &[">> Fish & chips <ahem>"])], None);
        let out = VttWriter::default().to_string(&track).unwrap();
        assert!(out.contains("&gt;&gt; Fish &amp; chips &lt;ahem&gt;"), "{out}");
    }

    #[test]
    fn forced_cues_are_marked_rather_than_flattened_in_with_the_rest() {
        let mut forced = cue(0, 1_000, &["[speaking Klingon]"]);
        forced.forced = true;
        let out = VttWriter::default()
            .to_string(&TextTrack::new(vec![forced], None))
            .unwrap();
        assert!(out.contains("--> 00:00:01.000 line:-1 align:center"), "{out}");
    }

    #[test]
    fn timestamps_use_a_dot_separator_unlike_srt() {
        assert_eq!(format_timestamp(Timestamp::from_millis(3_723_004)), "01:02:03.004");
    }

    #[test]
    fn an_empty_track_still_writes_a_valid_header() {
        assert_eq!(
            VttWriter::default()
                .to_string(&TextTrack::default())
                .unwrap(),
            "WEBVTT\n\n"
        );
    }

    #[test]
    fn a_leaning_line_is_tagged_after_it_is_escaped() {
        // Order is the whole test. Escaping second would turn the tag into `&lt;i&gt;`; escaping
        // first leaves the tag intact and anything the text itself held that looks like markup
        // stays text.
        let mut c = cue(0, 1_000, &["a < b"]);
        c.italic = vec![true];
        let out = VttWriter::default()
            .to_string(&TextTrack::new(vec![c], None))
            .unwrap();
        assert!(out.contains("<i>a &lt; b</i>"), "{out}");
    }

    #[test]
    fn a_cue_with_no_flags_carries_no_markup() {
        let track = TextTrack::new(vec![cue(0, 1_000, &["Plain text."])], None);
        assert!(
            !VttWriter::default()
                .to_string(&track)
                .unwrap()
                .contains("<i>")
        );
    }
}
