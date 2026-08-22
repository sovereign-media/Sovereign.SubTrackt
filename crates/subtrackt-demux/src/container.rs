//! Reading subtitle streams straight out of MP4 and MPEG-TS.
//!
//! Not implemented — see #86. MKV left this module when the native Matroska reader landed under #4;
//! what remains is the 0.2% that reader does not reach, measured rather than guessed: the library
//! survey found 1,326 of 1,328 titles in Matroska, one `.m2ts` and one `.iso`.
//!
//! The type below exists so that [`crate::open`] can dispatch these extensions to a named failure
//! rather than an unrecognised-extension one. An unsupported container that says which issue tracks
//! it is a fact a caller can act on; a generic error is not.

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
    /// Always returns [`Error::Unsupported`] until #86 lands.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(Error::io(path, std::io::ErrorKind::NotFound.into()));
        }
        Err(Error::unsupported(
            format!("demuxing subtitle streams from {}", path.display()),
            86,
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
        Err(Error::unsupported("container demuxing", 86))
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        Err(Error::unsupported("container demuxing", 86))
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
        let err = open_err(Path::new("no_such_file.m2ts"));
        assert!(matches!(err, Error::Io { .. }), "got {err:?}");
    }

    #[test]
    fn opening_a_real_container_reports_the_tracking_issue() {
        let path = std::env::temp_dir().join("subtrackt_placeholder.m2ts");
        std::fs::write(&path, b"not really a transport stream").unwrap();
        let err = open_err(&path);
        assert!(matches!(err, Error::Unsupported { issue: 86, .. }), "got {err:?}");
        std::fs::remove_file(&path).ok();
    }
}
