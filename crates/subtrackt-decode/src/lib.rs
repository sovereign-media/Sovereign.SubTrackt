//! Turning codec packets into palette-indexed bitmaps.
//!
//! Both decoders implement [`subtrackt_core::BitmapDecoder`] and are stateful: PGS carries palette
//! and object state across packets, and VOBSUB carries display state across control sequences. A
//! decoder instance therefore belongs to exactly one stream.

pub mod pgs;
pub mod vobsub;

use subtrackt_core::{BitmapDecoder, Error, Result};

/// Build a decoder for a codec name as reported by the demuxer.
///
/// # Errors
/// Returns [`Error::Unsupported`] for a codec with no decoder.
pub fn decoder_for(codec: &str) -> Result<Box<dyn BitmapDecoder>> {
    match codec {
        "hdmv_pgs_subtitle" | "pgs" => Ok(Box::new(pgs::PgsDecoder::new())),
        "dvd_subtitle" | "vobsub" => Ok(Box::new(vobsub::VobSubDecoder::new())),
        other => Err(Error::unsupported(format!("decoding {other}"), 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_bitmap_codecs_resolve_to_a_decoder() {
        assert_eq!(decoder_for("hdmv_pgs_subtitle").unwrap().codec(), "pgs");
        assert_eq!(decoder_for("dvd_subtitle").unwrap().codec(), "vobsub");
    }

    #[test]
    fn a_text_codec_is_rejected_rather_than_silently_decoded() {
        assert!(decoder_for("subrip").is_err());
    }
}
