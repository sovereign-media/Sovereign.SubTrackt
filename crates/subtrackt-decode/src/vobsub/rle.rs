//! VOBSUB nibble run-length decoding.
//!
//! Not implemented — see #3.
//!
//! The encoding, for whoever picks this up: pixels are 2 bits, runs are nibble-aligned, and the
//! run length grows in nibbles until the top bits are non-zero. The two halves of the data are the
//! even and odd scanlines, stored separately and interleaved back together on decode.

use subtrackt_core::{Error, Result};

/// Decode the two interlaced fields of a subpicture into one progressive index plane.
///
/// # Errors
/// Returns [`Error::Unsupported`] until #3 lands.
pub fn decode(
    _top_field: &[u8],
    _bottom_field: &[u8],
    _width: u32,
    _height: u32,
) -> Result<Vec<u8>> {
    Err(Error::unsupported("VOBSUB run-length decoding", 3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding_reports_the_tracking_issue() {
        let err = decode(&[], &[], 8, 2).unwrap_err();
        assert!(matches!(err, Error::Unsupported { issue: 3, .. }), "got {err:?}");
    }
}
