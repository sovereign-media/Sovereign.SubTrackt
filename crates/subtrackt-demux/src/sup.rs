//! Reader for raw PGS `.sup` dumps.
//!
//! A `.sup` file is the simplest possible PGS container: a flat sequence of segments, each with a
//! ten-byte header carrying the magic `PG`, a 90 kHz PTS, a DTS, the segment type and its length.
//! No index, no seeking, no container parsing — which is exactly why this is the input the rest of
//! the pipeline is developed against while the container demuxer (#4) is still open.
//!
//! Each emitted [`Packet`] payload is one segment in the same layout PGS packets take inside
//! Matroska — type byte, big-endian length, data — so the decoder in `subtrackt-decode` sees the
//! same bytes whichever reader produced them.

use std::path::{Path, PathBuf};

use subtrackt_core::{Error, Result};

use crate::{BitmapCodec, Packet, StreamInfo, SubtitleSource};

/// Bytes of `PG` magic plus PTS, DTS, type and length.
const HEADER_LEN: usize = 13;

/// Segment type of a Presentation Composition Segment, which carries the plane dimensions.
const SEGMENT_PCS: u8 = 0x16;

/// Reads segments out of a `.sup` file.
///
/// The whole file is read up front. `.sup` dumps of a feature run to tens of megabytes, which is
/// fine for a one-shot extraction; if this ever needs to stream, the parser below already works a
/// segment at a time and only the buffer needs replacing.
pub struct SupReader {
    path: PathBuf,
    data: Vec<u8>,
    cursor: usize,
    trailing_bytes: usize,
    streams: [StreamInfo; 1],
}

impl SupReader {
    /// Open and validate a `.sup` file.
    ///
    /// # Errors
    /// Returns [`Error::Io`] if the file cannot be read and [`Error::Demux`] if it does not start
    /// with the PGS magic.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let data = std::fs::read(path).map_err(|e| Error::io(path, e))?;

        if data.len() < HEADER_LEN || &data[..2] != b"PG" {
            return Err(Error::Demux(format!(
                "{} does not begin with the PGS 'PG' magic",
                path.display()
            )));
        }

        let (plane_width, plane_height) = plane_size(&data).ok_or_else(|| {
            Error::Demux(format!("{} contains no composition segment", path.display()))
        })?;

        Ok(Self {
            path: path.to_path_buf(),
            data,
            cursor: 0,
            trailing_bytes: 0,
            streams: [StreamInfo {
                index: 0,
                codec: BitmapCodec::Pgs,
                // A bare .sup carries no metadata: whatever named the file is all there is, and
                // guessing a language from the filename is the caller's business, not ours.
                language: None,
                title: None,
                plane_width,
                plane_height,
            }],
        })
    }

    /// The file this reader was opened on.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Bytes left unread because the file ended inside a segment.
    ///
    /// Non-zero means the dump was truncated and the final cue may be missing.
    #[must_use]
    pub const fn trailing_bytes(&self) -> usize {
        self.trailing_bytes
    }
}

/// Scan for the first PCS and read the plane dimensions out of its first four bytes.
fn plane_size(data: &[u8]) -> Option<(u32, u32)> {
    let mut cursor = 0;
    while let Some((segment, next)) = read_segment(data, cursor) {
        if segment.kind == SEGMENT_PCS && segment.body.len() >= 4 {
            let width = u32::from(u16::from_be_bytes([segment.body[0], segment.body[1]]));
            let height = u32::from(u16::from_be_bytes([segment.body[2], segment.body[3]]));
            return Some((width, height));
        }
        cursor = next;
    }
    None
}

struct RawSegment<'a> {
    pts: u64,
    kind: u8,
    body: &'a [u8],
}

/// Read the segment starting at `offset`, returning it and the offset of the next one.
///
/// Returns `None` at end of file, on a truncated header, on a truncated body, or on lost sync —
/// all of which mean "stop reading" rather than "this file is corrupt", because a `.sup` that ends
/// mid-segment is common and the segments before it are still good.
fn read_segment(data: &[u8], offset: usize) -> Option<(RawSegment<'_>, usize)> {
    let header = data.get(offset..offset + HEADER_LEN)?;
    if &header[..2] != b"PG" {
        return None;
    }

    let pts = u64::from(u32::from_be_bytes([header[2], header[3], header[4], header[5]]));
    let kind = header[10];
    let length = usize::from(u16::from_be_bytes([header[11], header[12]]));

    let start = offset + HEADER_LEN;
    let body = data.get(start..start + length)?;
    Some((RawSegment { pts, kind, body }, start + length))
}

impl SubtitleSource for SupReader {
    fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    fn select(&mut self, index: u32) -> Result<()> {
        if index == 0 {
            self.cursor = 0;
            Ok(())
        } else {
            Err(Error::Demux(format!("a .sup file has one stream; asked for {index}")))
        }
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        let Some((segment, next)) = read_segment(&self.data, self.cursor) else {
            // A .sup that ends mid-segment is common enough that it is not an error, but the
            // trailing byte count is worth keeping so a caller can tell "clean end of file" from
            // "the last cue was lost".
            self.trailing_bytes = self.data.len().saturating_sub(self.cursor);
            self.cursor = self.data.len();
            return Ok(None);
        };

        // Re-emit the segment in the Matroska PGS layout so both readers agree on packet shape.
        let mut payload = Vec::with_capacity(3 + segment.body.len());
        payload.push(segment.kind);
        payload.extend_from_slice(
            &u16::try_from(segment.body.len())
                .unwrap_or(u16::MAX)
                .to_be_bytes(),
        );
        payload.extend_from_slice(segment.body);

        self.cursor = next;
        Ok(Some(Packet { pts: segment.pts, payload }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `.sup` segment with the given type, PTS and body.
    fn segment(kind: u8, pts: u32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::from(*b"PG");
        out.extend_from_slice(&pts.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes()); // DTS
        out.push(kind);
        out.extend_from_slice(&u16::try_from(body.len()).unwrap().to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    /// A PCS body just long enough to carry 1920x1080.
    fn pcs_body() -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&1920u16.to_be_bytes());
        body.extend_from_slice(&1080u16.to_be_bytes());
        body.extend_from_slice(&[0; 6]);
        body
    }

    fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn reads_every_segment_and_reports_plane_size_from_the_composition() {
        let mut file = segment(SEGMENT_PCS, 90_000, &pcs_body());
        file.extend_from_slice(&segment(0x15, 90_000, &[1, 2, 3]));
        file.extend_from_slice(&segment(0x80, 90_000, &[]));
        let path = write_temp("subtrackt_sup_reads.sup", &file);

        let mut reader = SupReader::open(&path).unwrap();
        assert_eq!(reader.streams()[0].plane_width, 1920);
        assert_eq!(reader.streams()[0].plane_height, 1080);
        assert_eq!(reader.streams()[0].codec, BitmapCodec::Pgs);

        let mut kinds = Vec::new();
        while let Some(packet) = reader.next_packet().unwrap() {
            assert_eq!(packet.pts, 90_000);
            kinds.push(packet.payload[0]);
        }
        assert_eq!(kinds, vec![SEGMENT_PCS, 0x15, 0x80]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn payload_carries_the_segment_header_matroska_style() {
        let file = {
            let mut f = segment(SEGMENT_PCS, 0, &pcs_body());
            f.extend_from_slice(&segment(0x15, 0, &[0xAA, 0xBB]));
            f
        };
        let path = write_temp("subtrackt_sup_payload.sup", &file);

        let mut reader = SupReader::open(&path).unwrap();
        reader.next_packet().unwrap(); // the PCS
        let ods = reader.next_packet().unwrap().unwrap();
        assert_eq!(ods.payload, vec![0x15, 0x00, 0x02, 0xAA, 0xBB]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_truncated_trailing_segment_ends_the_stream_without_losing_earlier_ones() {
        let mut file = segment(SEGMENT_PCS, 0, &pcs_body());
        file.extend_from_slice(&segment(0x15, 0, &[1, 2, 3, 4]));
        file.truncate(file.len() - 2); // chop the last ODS body in half
        let path = write_temp("subtrackt_sup_truncated.sup", &file);

        let mut reader = SupReader::open(&path).unwrap();
        assert!(reader.next_packet().unwrap().is_some());
        assert!(reader.next_packet().unwrap().is_none());
        assert!(reader.trailing_bytes() > 0, "truncation must be visible to the caller");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_file_without_the_magic_is_rejected() {
        let path = write_temp("subtrackt_sup_bad_magic.sup", b"not a sup file at all");
        assert!(matches!(SupReader::open(&path), Err(Error::Demux(_))));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn select_rejects_a_stream_index_that_does_not_exist() {
        let path = write_temp("subtrackt_sup_select.sup", &segment(SEGMENT_PCS, 0, &pcs_body()));
        let mut reader = SupReader::open(&path).unwrap();
        assert!(reader.select(0).is_ok());
        assert!(reader.select(3).is_err());
        std::fs::remove_file(&path).ok();
    }
}
