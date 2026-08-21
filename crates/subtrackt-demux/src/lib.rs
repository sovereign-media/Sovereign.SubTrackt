//! Getting bitmap subtitle packets out of whatever they arrived in.
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

/// Everything known about a subtitle stream before any of it is decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    /// Index of the stream within its container; `0` for a sidecar file.
    pub index: u32,
    /// The codec carried.
    pub codec: BitmapCodec,
    /// BCP 47 or ISO 639 language tag, when declared.
    pub language: Option<String>,
    /// Track title, when declared.
    pub title: Option<String>,
    /// Width of the subtitle plane, needed to interpret packet coordinates.
    pub plane_width: u32,
    /// Height of the subtitle plane.
    pub plane_height: u32,
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
        "mkv" | "mp4" | "m4v" | "ts" | "m2ts" | "mts" => {
            Ok(Box::new(container::ContainerReader::open(path)?))
        }
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
