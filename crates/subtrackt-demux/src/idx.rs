//! Reader for VOBSUB `.idx` / `.sub` sidecar pairs.
//!
//! The `.idx` half is plain text: a palette line, a plane size, and one `timestamp: ... , filepos:
//! ...` line per subpicture. The `.sub` half is an MPEG program stream whose private-stream-1
//! packets carry the subpicture data. Parsing the text half is done here because it is what tells
//! the decoder where each subpicture starts; unpacking the PES payloads is tracked in #3.

use std::path::{Path, PathBuf};

use subtrackt_core::{Error, Result};

use crate::{BitmapCodec, Packet, StreamInfo, SubtitleSource};

/// One line of the `.idx` index: where a subpicture starts in the `.sub`, and when it appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    /// Presentation timestamp in 90 kHz ticks.
    pub pts: u64,
    /// Byte offset of the subpicture within the `.sub` file.
    pub filepos: u64,
}

/// The parsed contents of a `.idx` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    /// Subtitle plane width.
    pub plane_width: u32,
    /// Subtitle plane height.
    pub plane_height: u32,
    /// Declared language tag, when the file has an `id:` line.
    pub language: Option<String>,
    /// Subpicture entries in presentation order.
    pub entries: Vec<IndexEntry>,
}

/// Parse the text half of a VOBSUB pair.
///
/// Unknown directives are ignored rather than rejected: `.idx` files carry a good deal of player
/// configuration that has no bearing on extraction.
#[must_use]
pub fn parse_index(text: &str) -> Index {
    let mut index = Index::default();

    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();

        match key.trim() {
            // `palette` is deliberately not read here. The whole `.idx` text goes into
            // `StreamInfo::codec_private`, and `subtrackt_decode::vobsub::parse_palette` reads the
            // line from there -- which is the same path a Matroska VOBSUB track takes, where there
            // is no sidecar at all. A second copy parsed here was never consulted, and two parsers
            // for one format can only ever disagree.
            "size" => {
                if let Some((w, h)) = value.split_once('x') {
                    index.plane_width = w.trim().parse().unwrap_or(0);
                    index.plane_height = h.trim().parse().unwrap_or(0);
                }
            }
            "id" => {
                let tag = value.split(',').next().unwrap_or_default().trim();
                if !tag.is_empty() {
                    index.language = Some(tag.to_owned());
                }
            }
            "timestamp" => {
                if let Some(entry) = parse_timestamp_line(value) {
                    index.entries.push(entry);
                }
            }
            _ => {}
        }
    }

    index
}

/// Parse the body of a `timestamp: HH:MM:SS:mmm, filepos: 000000000` line.
fn parse_timestamp_line(value: &str) -> Option<IndexEntry> {
    let (stamp, rest) = value.split_once(',')?;
    let filepos = rest.split_once(':').map(|(_, p)| p.trim())?;

    let parts: Vec<&str> = stamp.trim().split(':').collect();
    if parts.len() != 4 {
        return None;
    }
    let h: u64 = parts[0].parse().ok()?;
    let m: u64 = parts[1].parse().ok()?;
    let s: u64 = parts[2].parse().ok()?;
    let ms: u64 = parts[3].parse().ok()?;

    let millis = ((h * 60 + m) * 60 + s) * 1_000 + ms;
    Some(IndexEntry {
        pts: millis * subtrackt_core::PTS_HZ / 1_000,
        filepos: u64::from_str_radix(filepos, 16)
            .or_else(|_| filepos.parse())
            .ok()?,
    })
}

/// Reads subpictures from a `.idx` / `.sub` pair.
pub struct IdxReader {
    /// Where each subpicture starts in the `.sub`, and when it appears.
    ///
    /// Nothing reads it yet: unpacking the private-stream-1 PES payloads is #3, and it is the
    /// half of VOBSUB support that has not landed. Kept because it is exactly what that work
    /// consumes and re-parsing the `.idx` to get it back would be the odd choice.
    #[allow(dead_code)]
    index: Index,
    sub_path: PathBuf,
    streams: [StreamInfo; 1],
}

impl IdxReader {
    /// Open a VOBSUB pair, given either half of it.
    ///
    /// # Errors
    /// Returns [`Error::Io`] if either file is missing and [`Error::Demux`] if the `.idx` carries
    /// no usable index entries.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let (idx_path, sub_path) = crate::vobsub_pair(path.as_ref());

        let text = std::fs::read_to_string(&idx_path).map_err(|e| Error::io(&idx_path, e))?;
        let index = parse_index(&text);

        if index.entries.is_empty() {
            return Err(Error::Demux(format!("{} lists no subpictures", idx_path.display())));
        }
        if !sub_path.exists() {
            return Err(Error::Demux(format!(
                "{} has no matching {}",
                idx_path.display(),
                sub_path.display()
            )));
        }

        let streams = [StreamInfo {
            index: 0,
            codec: BitmapCodec::VobSub,
            language: index.language.clone(),
            title: None,
            plane_width: index.plane_width,
            plane_height: index.plane_height,
            // The whole .idx is the codec configuration: the decoder reads its palette line.
            codec_private: text.into_bytes(),
        }];

        Ok(Self { index, sub_path, streams })
    }
}

impl SubtitleSource for IdxReader {
    fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    fn select(&mut self, index: u32) -> Result<()> {
        if index == 0 {
            Ok(())
        } else {
            Err(Error::Demux(format!("a VOBSUB pair has one stream; asked for {index}")))
        }
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        // Unpacking private-stream-1 PES payloads out of the .sub program stream is the other half
        // of VOBSUB support and lands with the decoder.
        let _ = &self.sub_path;
        Err(Error::unsupported("reading subpictures from a .sub program stream", 3))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# VobSub index file
size: 720x480
palette: 000000, 7e7e7e, ffffff, 1a1a1a
id: en, index: 0
timestamp: 00:00:12:345, filepos: 000000000
timestamp: 00:01:02:500, filepos: 000001800
";

    #[test]
    fn parses_the_directives_that_matter_and_ignores_the_rest() {
        let index = parse_index(SAMPLE);
        assert_eq!((index.plane_width, index.plane_height), (720, 480));
        // The palette line is deliberately not parsed here -- see `parse_index`. What proves it
        // survives is that the whole text reaches `codec_private`, which
        // `a_reader_hands_the_whole_idx_text_to_the_decoder` asserts.
        assert_eq!(index.language.as_deref(), Some("en"));
        assert_eq!(index.entries.len(), 2);
    }

    #[test]
    fn timestamps_convert_to_ninety_kilohertz_ticks() {
        let index = parse_index(SAMPLE);
        assert_eq!(index.entries[0].pts, 12_345 * subtrackt_core::PTS_HZ / 1_000);
        assert_eq!(index.entries[0].filepos, 0);
        assert_eq!(index.entries[1].filepos, 0x1800);
    }

    #[test]
    fn a_malformed_timestamp_line_is_skipped_not_fatal() {
        let index = parse_index(
            "size: 720x480\ntimestamp: nonsense\ntimestamp: 00:00:01:000, filepos: 000000010\n",
        );
        assert_eq!(index.entries.len(), 1);
    }

    #[test]
    fn a_reader_hands_the_whole_idx_text_to_the_decoder() {
        // The route the palette actually travels, and the reason `parse_index` does not read the
        // `palette:` line itself. A Matroska VOBSUB track has no sidecar at all -- its palette
        // arrives in `CodecPrivate` -- so the decoder reads that text either way, and a copy
        // parsed here would be a second parser for one format that nothing consults.
        let dir = std::env::temp_dir();
        let idx = dir.join("subtrackt_palette.idx");
        std::fs::write(&idx, SAMPLE).unwrap();
        std::fs::write(dir.join("subtrackt_palette.sub"), b"").unwrap();

        let reader = IdxReader::open(&idx).unwrap();
        let carried = String::from_utf8(reader.streams()[0].codec_private.clone()).unwrap();
        assert!(carried.contains("palette:"), "{carried}");
        assert_eq!(carried, SAMPLE, "the whole file, not a re-serialised part of it");

        std::fs::remove_file(&idx).ok();
        std::fs::remove_file(dir.join("subtrackt_palette.sub")).ok();
    }

    #[test]
    fn an_index_with_no_entries_is_rejected_at_open() {
        let path = std::env::temp_dir().join("subtrackt_empty.idx");
        std::fs::write(&path, "size: 720x480\n").unwrap();
        assert!(matches!(IdxReader::open(&path), Err(Error::Demux(_))));
        std::fs::remove_file(&path).ok();
    }
}
