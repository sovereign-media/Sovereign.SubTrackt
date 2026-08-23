//! `WebVTT` output.
//!
//! This is the format that makes the whole exercise worth it: a `WebVTT` rendition is something
//! every client can already render, whereas a burned-in bitmap is not.

use std::io::Write;

use subtrackt_core::{Error, Result, TextTrack, Timestamp, TrackWriter};

/// Writes a track as `WebVTT`.
#[derive(Debug, Clone, Copy, Default)]
pub struct VttWriter;

/// Format a timestamp as `HH:MM:SS.mmm`.
#[must_use]
pub fn format_timestamp(timestamp: Timestamp) -> String {
    let (h, m, s, ms) = timestamp.hmsm();
    format!("{h:02}:{m:02}:{s:02}.{ms:03}")
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
        let io = |e: std::io::Error| Error::io("<vtt output>", e);

        writeln!(out, "WEBVTT").map_err(io)?;
        if let Some(language) = &track.language {
            writeln!(out, "Language: {language}").map_err(io)?;
        }
        writeln!(out).map_err(io)?;

        for cue in track.cues.iter().filter(|c| !c.is_empty()) {
            write!(
                out,
                "{} --> {}",
                format_timestamp(cue.span.start),
                format_timestamp(cue.span.end)
            )
            .map_err(io)?;
            // Forced subtitles carry meaning a player can act on, so keep the distinction rather
            // than flattening every cue into the same track.
            if cue.forced {
                write!(out, " line:-1 align:center").map_err(io)?;
            }
            writeln!(out).map_err(io)?;

            // Escaped first and tagged second, so the tag survives and anything the text itself
            // held that looks like markup does not become any.
            for (index, line) in cue.lines.iter().enumerate() {
                let escaped = escape(line);
                writeln!(out, "{}", super::tagged(&escaped, cue.line_is_italic(index)))
                    .map_err(io)?;
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
    fn writes_a_header_and_a_cue() {
        let track = TextTrack::new(vec![cue(1_000, 3_500, &["Hello there."])], None);
        let out = VttWriter.to_string(&track).unwrap();
        assert_eq!(out, "WEBVTT\n\n00:00:01.000 --> 00:00:03.500\nHello there.\n\n");
    }

    #[test]
    fn a_declared_language_lands_in_the_header() {
        let track = TextTrack::new(vec![cue(0, 1_000, &["Bonjour"])], Some("fr".into()));
        assert!(
            VttWriter
                .to_string(&track)
                .unwrap()
                .contains("Language: fr\n")
        );
    }

    #[test]
    fn markup_characters_in_dialogue_are_escaped() {
        let track = TextTrack::new(vec![cue(0, 1_000, &[">> Fish & chips <ahem>"])], None);
        let out = VttWriter.to_string(&track).unwrap();
        assert!(out.contains("&gt;&gt; Fish &amp; chips &lt;ahem&gt;"), "{out}");
    }

    #[test]
    fn forced_cues_are_marked_rather_than_flattened_in_with_the_rest() {
        let mut forced = cue(0, 1_000, &["[speaking Klingon]"]);
        forced.forced = true;
        let out = VttWriter
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
        assert_eq!(VttWriter.to_string(&TextTrack::default()).unwrap(), "WEBVTT\n\n");
    }

    #[test]
    fn a_leaning_line_is_tagged_after_it_is_escaped() {
        // Order is the whole test. Escaping second would turn the tag into `&lt;i&gt;`; escaping
        // first leaves the tag intact and anything the text itself held that looks like markup
        // stays text.
        let mut c = cue(0, 1_000, &["a < b"]);
        c.italic = vec![true];
        let out = VttWriter.to_string(&TextTrack::new(vec![c], None)).unwrap();
        assert!(out.contains("<i>a &lt; b</i>"), "{out}");
    }

    #[test]
    fn a_cue_with_no_flags_carries_no_markup() {
        let track = TextTrack::new(vec![cue(0, 1_000, &["Plain text."])], None);
        assert!(!VttWriter.to_string(&track).unwrap().contains("<i>"));
    }
}
