//! DVD subpictures.
//!
//! Not implemented — see #3. The control-sequence opcodes are enumerated in [`control`] because
//! they are the part of the format worth writing down before writing any code: a subpicture's
//! timing, palette selection, alpha map and display area all arrive as commands in a chained
//! sequence rather than as header fields.

pub mod control;
pub mod rle;

use subtrackt_core::{BitmapDecoder, Error, Palette, Result, SubtitleImage};

/// Decoder state for one VOBSUB stream.
pub struct VobSubDecoder {
    /// The 16-colour palette from the `.idx` sidecar. VOBSUB subpictures select four of these per
    /// subpicture, so without it nothing can be coloured — or alpha-thresholded.
    palette: Option<Palette>,
}

impl VobSubDecoder {
    /// A decoder with no palette yet.
    #[must_use]
    pub const fn new() -> Self {
        Self { palette: None }
    }

    /// Supply the palette read from the `.idx` sidecar.
    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = Some(palette);
    }

    /// Whether a palette has been supplied.
    #[must_use]
    pub const fn has_palette(&self) -> bool {
        self.palette.is_some()
    }
}

impl Default for VobSubDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl BitmapDecoder for VobSubDecoder {
    fn codec(&self) -> &'static str {
        "vobsub"
    }

    fn push(&mut self, _pts: u64, _payload: &[u8]) -> Result<Vec<SubtitleImage>> {
        Err(Error::unsupported("VOBSUB subpicture decoding", 3))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding_reports_the_tracking_issue() {
        let mut decoder = VobSubDecoder::new();
        let err = decoder.push(0, &[0; 16]).unwrap_err();
        assert!(matches!(err, Error::Unsupported { issue: 3, .. }), "got {err:?}");
    }

    #[test]
    fn the_sidecar_palette_starts_absent_and_can_be_supplied() {
        let mut decoder = VobSubDecoder::new();
        assert!(!decoder.has_palette());
        decoder.set_palette(Palette::transparent(16));
        assert!(decoder.has_palette());
    }
}
