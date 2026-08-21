//! Blu-ray Presentation Graphic Stream.
//!
//! Segment framing and dispatch are implemented here so that malformed input is rejected at the
//! edge rather than deep inside a decoder. What each segment body *means* — composition, window,
//! palette, object — is #2.

pub mod rle;
pub mod segment;

use subtrackt_core::{BitmapDecoder, Error, Palette, Result, SubtitleImage};

pub use segment::{SegmentKind, SegmentRef};

/// Decoder state for one PGS stream.
///
/// PGS is a display-set protocol: segments arrive in groups terminated by an END segment, and the
/// palette and object state defined by earlier groups persists into later ones. That is why this
/// holds state rather than being a free function.
pub struct PgsDecoder {
    palette: Palette,
    /// Set by the last composition segment; a display set with no composition object is a clear,
    /// and is what closes the previous image's [`subtrackt_core::TimeSpan`].
    pending: Option<PendingComposition>,
    segments_seen: u64,
    ended_mid_display_set: bool,
}

/// A composition awaiting its END segment.
///
/// The PTS is captured now and consumed by #2, which needs it to open the display set's
/// [`subtrackt_core::TimeSpan`].
#[derive(Debug, Clone)]
struct PendingComposition {
    #[allow(dead_code)]
    pts: u64,
}

impl PgsDecoder {
    /// A decoder with an empty 256-entry palette, which is the correct initial state: PGS palette
    /// definitions are incremental updates, not replacements.
    #[must_use]
    pub fn new() -> Self {
        Self {
            palette: Palette::transparent(256),
            pending: None,
            segments_seen: 0,
            ended_mid_display_set: false,
        }
    }

    /// Number of segments fed to this decoder, for diagnostics.
    #[must_use]
    pub const fn segments_seen(&self) -> u64 {
        self.segments_seen
    }

    /// The palette as currently accumulated.
    #[must_use]
    pub const fn palette(&self) -> &Palette {
        &self.palette
    }

    /// Whether the stream ended part-way through a display set.
    #[must_use]
    pub const fn ended_mid_display_set(&self) -> bool {
        self.ended_mid_display_set
    }
}

impl Default for PgsDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl BitmapDecoder for PgsDecoder {
    fn codec(&self) -> &'static str {
        "pgs"
    }

    fn push(&mut self, pts: u64, payload: &[u8]) -> Result<Vec<SubtitleImage>> {
        // A Matroska PGS packet, and a .sup segment as re-emitted by the sup reader, may hold more
        // than one segment back to back.
        for segment in segment::iter(pts, payload) {
            let segment = segment?;
            self.segments_seen += 1;

            match segment.kind {
                SegmentKind::PresentationComposition => {
                    self.pending = Some(PendingComposition { pts });
                }
                SegmentKind::PaletteDefinition
                | SegmentKind::ObjectDefinition
                | SegmentKind::WindowDefinition => {}
                SegmentKind::End => {
                    if self.pending.take().is_some() {
                        return Err(Error::unsupported("PGS display set composition", 2));
                    }
                }
            }
        }

        Ok(Vec::new())
    }

    fn finish(&mut self) -> Result<Vec<SubtitleImage>> {
        // A stream that ends inside a display set drops its trailing composition. That is the
        // right call — half a display set has no timing to close it — but it is worth being able
        // to tell that it happened, hence the flag rather than a silent discard.
        self.ended_mid_display_set = self.pending.take().is_some();
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One segment in Matroska layout: type byte, big-endian length, body.
    fn seg(kind: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![kind];
        out.extend_from_slice(&u16::try_from(body.len()).unwrap().to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn segments_are_counted_as_they_are_framed() {
        let mut decoder = PgsDecoder::new();
        let mut packet = seg(0x14, &[0, 0]);
        packet.extend_from_slice(&seg(0x17, &[1]));
        decoder.push(9_000, &packet).unwrap();
        assert_eq!(decoder.segments_seen(), 2);
    }

    #[test]
    fn a_completed_display_set_reports_the_tracking_issue_rather_than_an_empty_image() {
        let mut decoder = PgsDecoder::new();
        let mut packet = seg(0x16, &[0; 11]);
        packet.extend_from_slice(&seg(0x80, &[]));
        let err = decoder.push(9_000, &packet).unwrap_err();
        assert!(matches!(err, Error::Unsupported { issue: 2, .. }), "got {err:?}");
    }

    #[test]
    fn the_initial_palette_is_fully_transparent() {
        let decoder = PgsDecoder::new();
        assert_eq!(decoder.palette().len(), 256);
        assert_eq!(decoder.palette().get(7).alpha, 0);
    }

    #[test]
    fn an_unterminated_display_set_is_dropped_at_finish_without_erroring() {
        let mut decoder = PgsDecoder::new();
        decoder.push(9_000, &seg(0x16, &[0; 11])).unwrap();
        assert!(decoder.finish().unwrap().is_empty());
        assert!(
            decoder.ended_mid_display_set(),
            "the truncation must be visible to the caller"
        );
    }
}
