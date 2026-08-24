//! The trait for each box in the pipeline diagram.
//!
//! Stages are traits rather than concrete functions so that the facade crate can swap
//! implementations — a stub for an unimplemented codec, a fake for tests, a real decoder in
//! production — without any stage crate depending on another.

use crate::bitmap::IndexedBitmap;
use crate::error::Result;
use crate::glyph::{Glyph, GlyphMatch};
use crate::subtitle::{Cue, SubtitleImage, TextTrack};

/// Turns raw codec packets into decoded bitmap subtitles.
///
/// Implementations are stateful: PGS in particular carries palette and object state across
/// packets, so a decoder instance belongs to exactly one stream.
pub trait BitmapDecoder {
    /// A short codec name for diagnostics, e.g. `pgs`.
    fn codec(&self) -> &'static str;

    /// Feed one packet, returning any subtitle images it completed.
    ///
    /// A packet may complete zero images (a palette update), one, or several.
    fn push(&mut self, pts: u64, payload: &[u8]) -> Result<Vec<SubtitleImage>>;

    /// Flush any image left open when the stream ended.
    fn finish(&mut self) -> Result<Vec<SubtitleImage>> {
        Ok(Vec::new())
    }

    /// Supply the codec configuration the container carried, if any.
    ///
    /// Called once before the first packet. PGS ignores it; VOBSUB needs it, because its palette
    /// arrives out of band rather than in the packets.
    ///
    /// # Errors
    /// Returns an error if the configuration is present but unusable.
    fn configure(&mut self, _codec_private: &[u8]) -> Result<()> {
        Ok(())
    }

    /// How many cues this decoder had to invent an end time for.
    ///
    /// `CLAUDE.md` allows an approximation where it is the lesser evil, on the condition that it is
    /// *counted so it can be audited* — and the trailing cue of a stream that never clears is the
    /// example the rule is written around. Both decoders counted it from the day they made the
    /// approximation. Neither count could be read: they were inherent methods, and
    /// `subtrackt_decode::decoder_for` hands back a `Box<dyn BitmapDecoder>`, so the pipeline held
    /// the number and had no way to ask for it. A run that guessed the end of forty cues reported
    /// exactly what a run that guessed none did.
    ///
    /// Defaulted to zero on the same argument as [`GlyphMatcher::cache_hits`]: a decoder that never
    /// approximates has nothing to declare, and should not have to say so.
    fn unterminated_cues(&self) -> u64 {
        0
    }
}

/// Binarizes a subtitle image and cuts it into glyphs.
pub trait Segmenter {
    /// Produce glyphs in reading order: top line first, left to right within a line.
    fn segment(&self, image: &SubtitleImage) -> Result<Vec<Glyph>>;

    /// Binarize without segmenting, for debug output and reference-data authoring.
    fn binarize(&self, image: &SubtitleImage) -> Result<IndexedBitmap>;
}

/// Identifies a glyph against the reference set.
pub trait GlyphMatcher {
    /// Show the matcher every glyph in the stream before any of them is matched.
    ///
    /// The default is to do nothing, which is what a purely streaming matcher wants. It exists
    /// because the shipped matcher is not one: #14 measured that a fixed reference set cannot
    /// identify glyphs one at a time, and the answer is to group a stream's own shapes first and
    /// match the group. That needs the whole stream in hand, and a stage cannot ask for it after
    /// answers have already been given out.
    ///
    /// # Errors
    /// Returns an error if the preparation itself fails. A matcher that merely finds nothing to
    /// match returns `Ok`.
    fn prepare(&mut self, _glyphs: &[Glyph]) -> Result<()> {
        Ok(())
    }

    /// Match one glyph.
    ///
    /// Returning `Ok` with a `None` character is the normal way to report "no reference within
    /// threshold"; the error variant is reserved for a matcher that could not run at all.
    fn match_glyph(&mut self, glyph: &Glyph) -> Result<GlyphMatch>;

    /// How many glyphs this matcher answered from its session cache, for reporting.
    fn cache_hits(&self) -> u64 {
        0
    }
}

/// Reassembles matched glyphs into lines of text, inserting spaces and applying post-correction.
pub trait TextAssembler {
    /// Build a cue from one image's glyphs and their matches.
    ///
    /// The two slices are index-aligned.
    fn assemble(
        &self,
        image: &SubtitleImage,
        glyphs: &[Glyph],
        matches: &[GlyphMatch],
    ) -> Result<Cue>;
}

/// Serialises a track to a subtitle file format.
pub trait TrackWriter {
    /// Write the track to `out`.
    ///
    /// # Errors
    /// Returns [`crate::Error::Io`] if the sink rejects a write.
    fn write(&self, track: &TextTrack, out: &mut dyn std::io::Write) -> Result<()>;

    /// Render the track to a `String`, for callers that do not want a sink.
    fn to_string(&self, track: &TextTrack) -> Result<String> {
        let mut buf = Vec::new();
        self.write(track, &mut buf)?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }
}
