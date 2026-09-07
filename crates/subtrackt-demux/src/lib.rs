//! Getting subtitle packets out of whatever they arrived in.
//!
//! Every stream this can *name* is in [`Codec`]; the ones it can currently *read* are the two in
//! [`BitmapCodec`]. #253 widened the first without widening the second on purpose — a text track
//! that a caller cannot see is one they have to open the file a second time to find, and naming it
//! is a much smaller change than reading it.
//!
//! Two input shapes are supported by design:
//!
//! * **Sidecar files** — a raw `.sup` PGS dump, or a VOBSUB `.idx`/`.sub` pair. These need no
//!   container parsing at all, which is why they are the first target: the whole pipeline can be
//!   exercised end to end without taking on a demuxer dependency.
//! * **Containers** — MKV, MP4 and MPEG-TS. Which demuxer backs this is an open decision
//!   (`ffmpeg-next` versus native parsers) and is deliberately behind [`SubtitleSource`] so it can
//!   be settled without disturbing anything downstream.

pub mod container;
pub mod idx;
pub mod matroska;
pub mod mpegts;
pub mod sup;

use std::path::{Path, PathBuf};

use subtrackt_core::{Error, Result};

/// Which bitmap subtitle codec a stream carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitmapCodec {
    /// Blu-ray Presentation Graphic Stream (`hdmv_pgs_subtitle`).
    Pgs,
    /// DVD subpictures (`dvd_subtitle`).
    VobSub,
}

impl BitmapCodec {
    /// The `FFmpeg` codec name, which is how Sovereign already identifies these streams.
    #[must_use]
    pub const fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Pgs => "hdmv_pgs_subtitle",
            Self::VobSub => "dvd_subtitle",
        }
    }
}

/// Which text subtitle codec a stream carries.
///
/// Named but not read. #253 widened [`StreamInfo`] so a text track could be *listed*; turning one
/// into cues is #251 for `SubRip` and #254 for the two `SubStation` dialects. A caller that reaches
/// one gets [`Error::Unsupported`] naming the issue, which is the house rule for a stage that does
/// not exist yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextCodec {
    /// `SubRip`, Matroska's `S_TEXT/UTF8`. Timed lines and nothing else.
    SubRip,
    /// Advanced `SubStation` Alpha, `S_TEXT/ASS`.
    Ass,
    /// `SubStation` Alpha, `S_TEXT/SSA`. Differs from [`Self::Ass`] in field count.
    Ssa,
    /// `WebVTT`, `S_TEXT/WEBVTT`.
    WebVtt,
}

impl TextCodec {
    /// The `FFmpeg` codec name, matching [`BitmapCodec::ffmpeg_name`].
    #[must_use]
    pub const fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::SubRip => "subrip",
            Self::Ass => "ass",
            Self::Ssa => "ssa",
            Self::WebVtt => "webvtt",
        }
    }
}

/// Which subtitle codec a stream carries, of either kind.
///
/// The split is the one that decides which pipeline reads the stream, and it is the only
/// distinction worth putting in the type: a bitmap track goes through decode, segment, match and
/// assemble, and a text track goes through none of them. Everything downstream that merely wants a
/// name calls [`Self::ffmpeg_name`] and never asks which variant it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    /// A track stored as pictures, which this tool reads.
    Bitmap(BitmapCodec),
    /// A track that is already text.
    Text(TextCodec),
}

impl Codec {
    /// The `FFmpeg` codec name, which is how Sovereign already identifies these streams.
    #[must_use]
    pub const fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Bitmap(codec) => codec.ffmpeg_name(),
            Self::Text(codec) => codec.ffmpeg_name(),
        }
    }

    /// The bitmap codec, or `None` for a text track.
    #[must_use]
    pub const fn bitmap(self) -> Option<BitmapCodec> {
        match self {
            Self::Bitmap(codec) => Some(codec),
            Self::Text(_) => None,
        }
    }

    /// Whether the track is already text.
    #[must_use]
    pub const fn is_text(self) -> bool {
        matches!(self, Self::Text(_))
    }
}

/// Everything known about a subtitle stream before any of it is decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    /// Index of the stream within its container; `0` for a sidecar file.
    pub index: u32,
    /// The codec carried, of either kind.
    pub codec: Codec,
    /// BCP 47 or ISO 639 language tag, when declared.
    pub language: Option<String>,
    /// Track title, when declared.
    pub title: Option<String>,
    /// Width of the subtitle plane, needed to interpret packet coordinates.
    pub plane_width: u32,
    /// Height of the subtitle plane.
    pub plane_height: u32,
    /// Codec configuration the container carried alongside the track.
    ///
    /// VOBSUB needs this: inside Matroska its 16-colour palette lives here as a text blob, in the
    /// same `palette:` format a `.idx` sidecar uses. There is no sidecar to read, so without this
    /// a Matroska VOBSUB track cannot be coloured at all — and colour is what alpha thresholding
    /// depends on.
    pub codec_private: Vec<u8>,
}

/// One codec packet with its presentation timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    /// Presentation timestamp in 90 kHz ticks.
    pub pts: u64,
    /// Raw codec bytes, exactly as they appeared in the container.
    pub payload: Vec<u8>,
}

/// An opened source of subtitle packets.
///
/// Implementations are iterators in spirit but not in signature: [`Self::next_packet`] returns a
/// `Result<Option<_>>` so that a mid-file parse failure is distinguishable from end of stream.
pub trait SubtitleSource {
    /// Streams available in this source.
    fn streams(&self) -> &[StreamInfo];

    /// Select which stream subsequent [`Self::next_packet`] calls read from.
    ///
    /// # Errors
    /// Returns [`Error::Demux`] if `index` names no stream in this source.
    fn select(&mut self, index: u32) -> Result<()>;

    /// Read the next packet from the selected stream, or `None` at end of stream.
    fn next_packet(&mut self) -> Result<Option<Packet>>;
}

/// Open a file, dispatching on its extension.
///
/// # Errors
/// Returns [`Error::Unsupported`] for a container format whose demuxer is not implemented, and
/// [`Error::Io`] if the file cannot be read.
pub fn open(path: impl AsRef<Path>) -> Result<Box<dyn SubtitleSource>> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "sup" => Ok(Box::new(sup::SupReader::open(path)?)),
        "idx" | "sub" => Ok(Box::new(idx::IdxReader::open(path)?)),
        "mkv" | "mka" | "webm" => Ok(Box::new(matroska::MatroskaReader::open(path)?)),
        "ts" | "m2ts" | "mts" => Ok(Box::new(mpegts::MpegTsReader::open(path)?)),
        "mp4" | "m4v" => Ok(Box::new(container::ContainerReader::open(path)?)),
        "" => Err(Error::Demux(format!(
            "{} has no extension to dispatch on",
            path.display()
        ))),
        other => Err(Error::Demux(format!("unrecognised input extension .{other}"))),
    }
}

/// Locate the `.sub` payload file that pairs with a VOBSUB `.idx`, or vice versa.
#[must_use]
pub fn vobsub_pair(path: &Path) -> (PathBuf, PathBuf) {
    (path.with_extension("idx"), path.with_extension("sub"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Box<dyn SubtitleSource>` is not `Debug`, so unwrap the error side by hand.
    fn open_err(path: &str) -> Error {
        match open(Path::new(path)) {
            Err(err) => err,
            Ok(_) => panic!("{path} should not have opened"),
        }
    }

    #[test]
    fn unknown_extensions_are_rejected_before_any_io() {
        let err = open_err("nonexistent.avi");
        assert!(matches!(err, Error::Demux(_)), "got {err:?}");
    }

    #[test]
    fn an_extensionless_path_says_so_rather_than_guessing() {
        let err = open_err("subtitles");
        assert!(matches!(err, Error::Demux(_)), "got {err:?}");
    }

    #[test]
    fn vobsub_pairing_works_from_either_half() {
        let (idx, sub) = vobsub_pair(Path::new("/media/movie.sub"));
        assert_eq!(idx, Path::new("/media/movie.idx"));
        assert_eq!(sub, Path::new("/media/movie.sub"));
    }
}
