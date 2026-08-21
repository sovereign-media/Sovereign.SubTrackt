//! PGS run-length decoding.
//!
//! Not implemented — see #2.
//!
//! The encoding, for whoever picks this up: a non-zero byte is a literal pixel. A zero byte
//! introduces a run, whose following byte selects between four forms by its top two bits — a short
//! run of colour zero, a long run of colour zero, a short run of an explicit colour, and a long run
//! of an explicit colour. `00 00` ends the current line, and lines are padded to the object width.

use subtrackt_core::{Error, Result};

/// Decode one object's RLE data into a row-major index plane of `width * height` bytes.
///
/// # Errors
/// Returns [`Error::Unsupported`] until #2 lands. Once implemented it must return
/// [`Error::MalformedPacket`] for a run that overflows its line and for data that ends before the
/// declared height is filled — decoding a short object into a partially-blank bitmap would produce
/// a subtitle that reads as legitimately empty, which is exactly the silent failure this design
/// exists to avoid.
pub fn decode(_data: &[u8], _width: u32, _height: u32) -> Result<Vec<u8>> {
    Err(Error::unsupported("PGS run-length decoding", 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding_reports_the_tracking_issue() {
        let err = decode(&[0x00, 0x00], 4, 1).unwrap_err();
        assert!(matches!(err, Error::Unsupported { issue: 2, .. }), "got {err:?}");
    }
}
