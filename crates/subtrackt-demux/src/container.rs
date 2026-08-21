//! Reading subtitle streams straight out of MKV, MP4 and MPEG-TS.
//!
//! Not implemented — see #4, which also carries the dependency decision this stage turns on
//! (`ffmpeg-next` versus native parsers versus shelling out). The type below exists so that the
//! decision can be made without touching [`crate::open`] or anything downstream of it.

use std::path::{Path, PathBuf};

use subtrackt_core::{Error, Result};

use crate::{Packet, StreamInfo, SubtitleSource};

/// Placeholder container reader.
#[derive(Debug)]
pub struct ContainerReader {
    path: PathBuf,
    streams: Vec<StreamInfo>,
}

impl ContainerReader {
    /// Open a container.
    ///
    /// # Errors
    /// Always returns [`Error::Unsupported`] until #4 lands.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(Error::io(path, std::io::ErrorKind::NotFound.into()));
        }
        Err(Error::unsupported(
            format!("demuxing subtitle streams from {}", path.display()),
            4,
        ))
    }

    /// The file this reader was opened on.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SubtitleSource for ContainerReader {
    fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    fn select(&mut self, _index: u32) -> Result<()> {
        Err(Error::unsupported("container demuxing", 4))
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        Err(Error::unsupported("container demuxing", 4))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_err(path: &Path) -> Error {
        match ContainerReader::open(path) {
            Err(err) => err,
            Ok(_) => panic!("{} should not have opened", path.display()),
        }
    }

    #[test]
    fn opening_a_missing_container_reports_io_not_unsupported() {
        let err = open_err(Path::new("no_such_file.mkv"));
        assert!(matches!(err, Error::Io { .. }), "got {err:?}");
    }

    #[test]
    fn opening_a_real_container_reports_the_tracking_issue() {
        let path = std::env::temp_dir().join("subtrackt_placeholder.mkv");
        std::fs::write(&path, b"not really a matroska file").unwrap();
        let err = open_err(&path);
        assert!(matches!(err, Error::Unsupported { issue: 4, .. }), "got {err:?}");
        std::fs::remove_file(&path).ok();
    }
}
