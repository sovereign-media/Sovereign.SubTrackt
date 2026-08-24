//! Reading subtitle streams straight out of MP4.
//!
//! Not implemented, and this is now the whole of what is not. MKV left this module when the native
//! Matroska reader landed under #4, and MPEG-TS left it under #86 — what remains is MP4 alone, for
//! which the surveyed library holds **zero** titles carrying a bitmap subtitle track. The two
//! containers the survey did find outside Matroska were one `.m2ts`, which [`crate::mpegts`] now
//! reads, and one `.iso`, which is a filesystem rather than a container and is not dispatched here
//! at all.
//!
//! So the gap this names is no longer a share of anything measured. It is a format that might turn
//! up, and the type below exists so [`crate::open`] can dispatch it to a named failure rather than
//! an unrecognised-extension one — an unsupported container that says which issue tracks it is a
//! fact a caller can act on, and a generic error is not.

use std::path::Path;

use subtrackt_core::{Error, Result};

use crate::{Packet, StreamInfo, SubtitleSource};

/// Placeholder container reader.
///
/// Carries no state, and cannot: [`Self::open`] returns `Err` unconditionally, so an instance has
/// never existed. What the type is for is the dispatch arm in [`crate::open`] — an
/// unsupported container that names the issue tracking it is a fact a caller can act on, and a
/// generic unrecognised-extension error is not. The [`SubtitleSource`] impl below is what makes
/// that arm type-check; its methods are unreachable by construction.
#[derive(Debug)]
pub struct ContainerReader;

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
}

impl SubtitleSource for ContainerReader {
    fn streams(&self) -> &[StreamInfo] {
        &[]
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
