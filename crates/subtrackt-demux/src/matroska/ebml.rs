//! EBML primitives.
//!
//! Matroska is a tree of EBML elements, each an ID, a size, and a payload. Both the ID and the
//! size are variable-length integers whose width is encoded in the leading zero count of the first
//! byte, which is the only genuinely fiddly part of the format.
//!
//! Everything here streams. A film is several gigabytes and the subtitle track is a rounding error
//! within it, so the reader seeks past what it does not need rather than buffering the file.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use subtrackt_core::{Error, Result};

/// The size field of an element whose length is not known in advance.
///
/// Live-muxed files use this for the Segment and sometimes for Clusters, so it has to be handled
/// rather than rejected: such an element runs until something else that makes sense begins.
pub const UNKNOWN_SIZE: u64 = u64::MAX;

/// An element header: what it is, how big it is, and where its body starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementHeader {
    /// Element ID, marker bit included, as the specification writes them.
    pub id: u32,
    /// Payload length in bytes, or [`UNKNOWN_SIZE`].
    pub size: u64,
    /// Absolute offset of the first payload byte.
    pub body_start: u64,
}

impl ElementHeader {
    /// Absolute offset one past the end of the payload, or `None` if the size is unknown.
    #[must_use]
    pub fn body_end(&self) -> Option<u64> {
        (self.size != UNKNOWN_SIZE).then(|| self.body_start + self.size)
    }
}

/// A seekable EBML reader that keeps the file path for error messages.
pub struct EbmlReader<R> {
    inner: R,
    path: PathBuf,
    /// Logical offset, tracked so a redundant seek can be skipped entirely.
    pos: u64,
}

impl<R: Read + Seek> EbmlReader<R> {
    /// Wrap a reader.
    pub fn new(inner: R, path: impl Into<PathBuf>) -> Self {
        Self { inner, path: path.into(), pos: 0 }
    }

    /// The file being read.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn io(&self, source: std::io::Error) -> Error {
        Error::io(&self.path, source)
    }

    /// Current absolute offset.
    ///
    /// # Errors
    /// Propagates seek failures.
    pub const fn position(&self) -> u64 {
        self.pos
    }

    /// Seek to an absolute offset.
    ///
    /// # Errors
    /// Propagates seek failures.
    pub fn seek_to(&mut self, offset: u64) -> Result<()> {
        if offset == self.pos {
            return Ok(());
        }

        // Every forward jump reads through the buffer instead of seeking, however far it goes.
        //
        // This is the whole performance story. `BufReader::seek` discards its buffer, and finding
        // a subtitle track means stepping past millions of video and audio elements spread across
        // the entire file — so seeking per element turned a 5.5 GB film into tens of gigabytes of
        // refills and over ten minutes of wall clock. Since the walk only ever moves forward, and
        // every byte has to be passed over anyway, reading through turns the whole thing into one
        // sequential pass at disk speed.
        if offset > self.pos {
            let mut remaining = offset - self.pos;
            let mut scratch = [0u8; 4096];
            while remaining > 0 {
                let want = usize::try_from(remaining.min(scratch.len() as u64)).unwrap_or(0);
                self.inner
                    .read_exact(&mut scratch[..want])
                    .map_err(|e| self.io(e))?;
                remaining -= want as u64;
            }
            self.pos = offset;
            return Ok(());
        }

        self.inner
            .seek(SeekFrom::Start(offset))
            .map_err(|e| self.io(e))?;
        self.pos = offset;
        Ok(())
    }

    /// Total length of the file.
    ///
    /// Named `byte_len` rather than `len` because it is fallible and takes `&mut self`, so the
    /// usual `len`/`is_empty` pairing does not apply.
    ///
    /// # Errors
    /// Propagates seek failures.
    pub fn byte_len(&mut self) -> Result<u64> {
        let here = self.pos;
        let end = self.inner.seek(SeekFrom::End(0)).map_err(|e| self.io(e))?;
        self.pos = end;
        self.seek_to(here)?;
        Ok(end)
    }

    /// Read exactly `count` bytes.
    ///
    /// # Errors
    /// Returns [`Error::Io`] at end of file or on a read failure.
    pub fn read_exact(&mut self, count: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; count];
        self.inner.read_exact(&mut buf).map_err(|e| self.io(e))?;
        self.pos += count as u64;
        Ok(buf)
    }

    /// Read one byte, returning `None` cleanly at end of file.
    ///
    /// # Errors
    /// Propagates read failures other than end of file.
    pub fn read_byte(&mut self) -> Result<Option<u8>> {
        let mut byte = [0u8; 1];
        match self.inner.read_exact(&mut byte) {
            Ok(()) => {
                self.pos += 1;
                Ok(Some(byte[0]))
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(self.io(e)),
        }
    }

    /// Read a variable-length integer.
    ///
    /// `keep_marker` distinguishes the two uses: element IDs are quoted with their marker bit
    /// intact (`0x1A45DFA3`), while sizes have it stripped to give the actual length.
    ///
    /// # Errors
    /// Returns [`Error::Demux`] if the first byte is zero, which encodes a width of more than
    /// eight bytes and cannot occur in a valid file.
    pub fn read_vint(&mut self, keep_marker: bool) -> Result<Option<(u64, u8)>> {
        let Some(first) = self.read_byte()? else {
            return Ok(None);
        };
        if first == 0 {
            return Err(Error::Demux(format!(
                "{}: variable-length integer wider than 8 bytes",
                self.path.display()
            )));
        }

        let width = u8::try_from(first.leading_zeros()).unwrap_or(0) + 1;
        let mut value = if keep_marker {
            u64::from(first)
        } else {
            // Clear the marker bit, which sits at position 8 - width.
            u64::from(first & !(1u8 << (8 - width)))
        };

        for _ in 1..width {
            let Some(byte) = self.read_byte()? else {
                return Ok(None);
            };
            value = (value << 8) | u64::from(byte);
        }
        Ok(Some((value, width)))
    }

    /// Read an element header, or `None` at end of file.
    ///
    /// # Errors
    /// Propagates read failures and malformed variable-length integers.
    pub fn read_header(&mut self) -> Result<Option<ElementHeader>> {
        let Some((id, _)) = self.read_vint(true)? else {
            return Ok(None);
        };
        let Some((size, width)) = self.read_vint(false)? else {
            return Ok(None);
        };

        // All value bits set means the length is not known in advance.
        let unknown = size == (1u64 << (7 * u32::from(width))) - 1;
        let body_start = self.position();

        Ok(Some(ElementHeader {
            id: u32::try_from(id).unwrap_or(u32::MAX),
            size: if unknown { UNKNOWN_SIZE } else { size },
            body_start,
        }))
    }

    /// Skip past an element's payload.
    ///
    /// # Errors
    /// Propagates seek failures.
    pub fn skip(&mut self, header: &ElementHeader) -> Result<()> {
        match header.body_end() {
            Some(end) => self.seek_to(end),
            // An unknown-size element cannot be skipped; the caller has to descend into it.
            None => Ok(()),
        }
    }

    /// Read an element's payload as an unsigned integer.
    ///
    /// # Errors
    /// Returns [`Error::Demux`] if the payload is longer than eight bytes.
    pub fn read_uint(&mut self, header: &ElementHeader) -> Result<u64> {
        if header.size > 8 {
            return Err(Error::Demux(format!(
                "{}: integer element 0x{:X} is {} bytes",
                self.path.display(),
                header.id,
                header.size
            )));
        }
        let bytes = self.read_exact(usize::try_from(header.size).unwrap_or(0))?;
        Ok(bytes.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b)))
    }

    /// Read an element's payload as a UTF-8 string, lossily.
    ///
    /// # Errors
    /// Propagates read failures.
    pub fn read_string(&mut self, header: &ElementHeader) -> Result<String> {
        let bytes = self.read_exact(usize::try_from(header.size).unwrap_or(0))?;
        // Trailing NULs are legal padding in EBML strings.
        let trimmed = bytes.split(|b| *b == 0).next().unwrap_or_default();
        Ok(String::from_utf8_lossy(trimmed).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn reader(bytes: &[u8]) -> EbmlReader<Cursor<Vec<u8>>> {
        EbmlReader::new(Cursor::new(bytes.to_vec()), "test.mkv")
    }

    /// Encode a value as a variable-length integer of the given width.
    pub fn vint(value: u64, width: u8) -> Vec<u8> {
        let mut out = value.to_be_bytes()[8 - width as usize..].to_vec();
        out[0] |= 1 << (8 - width);
        out
    }

    #[test]
    fn a_one_byte_size_reads_back_without_its_marker() {
        // 0x81 is width 1, value 1.
        let (value, width) = reader(&[0x81]).read_vint(false).unwrap().unwrap();
        assert_eq!((value, width), (1, 1));
    }

    #[test]
    fn an_element_id_keeps_its_marker_bit() {
        // The EBML header ID is quoted as 0x1A45DFA3, marker included.
        let (value, width) = reader(&[0x1A, 0x45, 0xDF, 0xA3])
            .read_vint(true)
            .unwrap()
            .unwrap();
        assert_eq!((value, width), (0x1A45_DFA3, 4));
    }

    #[test]
    fn wider_sizes_round_trip() {
        for width in 1..=8u8 {
            let value = 300u64.min((1 << (7 * u32::from(width))) - 2);
            let encoded = vint(value, width);
            let (decoded, got_width) = reader(&encoded).read_vint(false).unwrap().unwrap();
            assert_eq!(decoded, value, "width {width}");
            assert_eq!(got_width, width);
        }
    }

    #[test]
    fn a_zero_first_byte_is_rejected_rather_than_looping() {
        // Encodes a width above eight, which no valid file contains.
        assert!(reader(&[0x00, 0x01]).read_vint(false).is_err());
    }

    #[test]
    fn end_of_file_reads_as_none_not_as_an_error() {
        assert!(reader(&[]).read_vint(false).unwrap().is_none());
        assert!(reader(&[]).read_header().unwrap().is_none());
        assert!(reader(&[]).read_byte().unwrap().is_none());
    }

    #[test]
    fn a_header_reports_where_its_body_starts_and_ends() {
        // ID 0x1A45DFA3, size 4, then four payload bytes.
        let mut bytes = vec![0x1A, 0x45, 0xDF, 0xA3, 0x84];
        bytes.extend_from_slice(&[1, 2, 3, 4]);

        let header = reader(&bytes).read_header().unwrap().unwrap();
        assert_eq!(header.id, 0x1A45_DFA3);
        assert_eq!(header.size, 4);
        assert_eq!(header.body_start, 5);
        assert_eq!(header.body_end(), Some(9));
    }

    #[test]
    fn an_all_ones_size_means_unknown_length() {
        // 0xFF is width 1 with every value bit set: the live-muxing sentinel.
        let header = reader(&[0xA3, 0xFF]).read_header().unwrap().unwrap();
        assert_eq!(header.size, UNKNOWN_SIZE);
        assert_eq!(header.body_end(), None, "an unknown-size element cannot be skipped");
    }

    #[test]
    fn integers_and_strings_read_back_from_their_payloads() {
        let mut bytes = vec![0x83, 0x82, 0x01, 0x00]; // id 0x83, size 2 (0x82), value 0x0100
        let mut r = reader(&bytes);
        let header = r.read_header().unwrap().unwrap();
        assert_eq!(r.read_uint(&header).unwrap(), 256);

        bytes = vec![0x86, 0x84, b'e', b'n', b'g', 0x00];
        let mut r = reader(&bytes);
        let header = r.read_header().unwrap().unwrap();
        assert_eq!(
            r.read_string(&header).unwrap(),
            "eng",
            "trailing NUL padding is stripped"
        );
    }

    #[test]
    fn an_oversized_integer_element_is_rejected() {
        let bytes = vec![0x83, 0x89, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut r = reader(&bytes);
        let header = r.read_header().unwrap().unwrap();
        assert!(r.read_uint(&header).is_err(), "a 9-byte integer is malformed");
    }

    #[test]
    fn skipping_lands_exactly_past_the_payload() {
        let mut bytes = vec![0xA3, 0x84, 1, 2, 3, 4];
        bytes.extend_from_slice(&[0xE7, 0x81, 9]);

        let mut r = reader(&bytes);
        let first = r.read_header().unwrap().unwrap();
        r.skip(&first).unwrap();

        let second = r.read_header().unwrap().unwrap();
        assert_eq!(second.id, 0xE7, "the reader landed on the next element");
    }
}
