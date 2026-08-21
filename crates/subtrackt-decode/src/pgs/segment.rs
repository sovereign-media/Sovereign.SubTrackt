//! PGS segment framing.
//!
//! Every PGS segment is a type byte, a big-endian length, and a body. Framing is separated from
//! interpretation so that a truncated or mistyped segment is caught once, here, with the PTS
//! attached — the decoder proper never sees a body shorter than its declared length.

use subtrackt_core::{Error, Result};

/// Header bytes preceding every segment body.
pub const HEADER_LEN: usize = 3;

/// The five segment types PGS defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SegmentKind {
    /// Palette Definition Segment: an incremental palette update.
    PaletteDefinition,
    /// Object Definition Segment: RLE bitmap data, possibly one fragment of several.
    ObjectDefinition,
    /// Presentation Composition Segment: what is displayed, where, and whether it is forced.
    PresentationComposition,
    /// Window Definition Segment: the regions objects may be drawn into.
    WindowDefinition,
    /// End of display set.
    End,
}

impl SegmentKind {
    /// Map the type byte, or `None` for a value PGS does not define.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x14 => Some(Self::PaletteDefinition),
            0x15 => Some(Self::ObjectDefinition),
            0x16 => Some(Self::PresentationComposition),
            0x17 => Some(Self::WindowDefinition),
            0x80 => Some(Self::End),
            _ => None,
        }
    }

    /// The type byte.
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::PaletteDefinition => 0x14,
            Self::ObjectDefinition => 0x15,
            Self::PresentationComposition => 0x16,
            Self::WindowDefinition => 0x17,
            Self::End => 0x80,
        }
    }
}

/// A framed segment borrowed from the packet buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentRef<'a> {
    /// Presentation timestamp of the packet this segment came from.
    pub pts: u64,
    /// What kind of segment it is.
    pub kind: SegmentKind,
    /// The segment body, exactly `length` bytes.
    pub body: &'a [u8],
}

/// Frame every segment in a packet.
///
/// The iterator yields `Err` at most once: the first framing failure ends iteration, because after
/// a bad length there is no way to know where the next segment starts.
#[must_use]
pub fn iter(pts: u64, payload: &[u8]) -> SegmentIter<'_> {
    SegmentIter { pts, payload, cursor: 0, failed: false }
}

/// Iterator returned by [`iter`].
pub struct SegmentIter<'a> {
    pts: u64,
    payload: &'a [u8],
    cursor: usize,
    failed: bool,
}

impl<'a> Iterator for SegmentIter<'a> {
    type Item = Result<SegmentRef<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.cursor >= self.payload.len() {
            return None;
        }

        let malformed = |reason: String, pts: u64| {
            Some(Err(Error::MalformedPacket { codec: "pgs", pts, reason }))
        };

        let Some(header) = self.payload.get(self.cursor..self.cursor + HEADER_LEN) else {
            self.failed = true;
            return malformed(
                format!(
                    "{} trailing bytes, too few for a segment header",
                    self.payload.len() - self.cursor
                ),
                self.pts,
            );
        };

        let Some(kind) = SegmentKind::from_byte(header[0]) else {
            self.failed = true;
            return malformed(format!("unknown segment type 0x{:02x}", header[0]), self.pts);
        };

        let length = usize::from(u16::from_be_bytes([header[1], header[2]]));
        let start = self.cursor + HEADER_LEN;

        let Some(body) = self.payload.get(start..start + length) else {
            self.failed = true;
            return malformed(
                format!(
                    "segment 0x{:02x} declares {length} bytes but only {} remain",
                    header[0],
                    self.payload.len() - start
                ),
                self.pts,
            );
        };

        self.cursor = start + length;
        Some(Ok(SegmentRef { pts: self.pts, kind, body }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(kind: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![kind];
        out.extend_from_slice(&u16::try_from(body.len()).unwrap().to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn every_defined_type_byte_round_trips() {
        for kind in [
            SegmentKind::PaletteDefinition,
            SegmentKind::ObjectDefinition,
            SegmentKind::PresentationComposition,
            SegmentKind::WindowDefinition,
            SegmentKind::End,
        ] {
            assert_eq!(SegmentKind::from_byte(kind.to_byte()), Some(kind));
        }
        assert_eq!(SegmentKind::from_byte(0x42), None);
    }

    #[test]
    fn several_segments_in_one_packet_are_framed_in_order() {
        let mut packet = seg(0x16, &[1, 2, 3]);
        packet.extend_from_slice(&seg(0x15, &[4]));
        packet.extend_from_slice(&seg(0x80, &[]));

        let framed: Vec<_> = iter(9_000, &packet).map(Result::unwrap).collect();
        assert_eq!(framed.len(), 3);
        assert_eq!(framed[0].kind, SegmentKind::PresentationComposition);
        assert_eq!(framed[0].body, &[1, 2, 3]);
        assert_eq!(framed[1].body, &[4]);
        assert!(framed[2].body.is_empty());
    }

    #[test]
    fn a_body_shorter_than_its_declared_length_is_malformed_and_stops_iteration() {
        let mut packet = seg(0x15, &[1, 2, 3, 4]);
        packet.truncate(packet.len() - 2);

        let mut it = iter(1_234, &packet);
        let err = it.next().unwrap().unwrap_err();
        match err {
            Error::MalformedPacket { codec, pts, ref reason } => {
                assert_eq!(codec, "pgs");
                assert_eq!(pts, 1_234);
                assert!(reason.contains("declares 4 bytes"), "{reason}");
            }
            other => panic!("got {other:?}"),
        }
        assert!(it.next().is_none(), "iteration must stop after a framing failure");
    }

    #[test]
    fn an_unknown_segment_type_is_malformed_rather_than_skipped() {
        let err = iter(0, &seg(0x42, &[1])).next().unwrap().unwrap_err();
        assert!(matches!(err, Error::MalformedPacket { .. }));
    }

    #[test]
    fn an_empty_packet_frames_nothing_without_erroring() {
        assert_eq!(iter(0, &[]).count(), 0);
    }
}
