//! Reading a file in small pieces, far apart.
//!
//! The opposite of what [`super::ebml::EbmlReader`] is built for, and it is a separate reader
//! because the two want opposite things from the same file. The walk moves forward over every
//! byte and is fastest when a large buffer reads straight through; the index path wants a few
//! kilobytes every few seconds of film with megabytes of video between, and every byte a buffer
//! fetches past the block is a byte it did not need. On a network mount the requests are the cost,
//! so this counts them, and the report prints what a run transferred rather than what it was
//! designed to.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use subtrackt_core::{Error, Result};

/// Anything the index path can read from. A trait object so the handle can differ in type from
/// the walk's buffered one.
pub trait ReadSeek: Read + Seek + Send {}

impl<T: Read + Seek + Send> ReadSeek for T {}

/// The read issued when the size of what is wanted is not yet known.
///
/// Sized to hold a whole subtitle block in the common case — PGS display sets on a Blu-ray remux
/// run 5 to 45 KB — so that finding a block and reading it is one request. A block that runs past
/// it costs one more read for the remainder. Trickster chose the same figure for the same blocks.
pub const WINDOW: usize = 64 * 1024;

/// A seek-and-read reader with one window into the file.
pub struct RandomReader {
    inner: Box<dyn ReadSeek>,
    path: PathBuf,
    len: u64,
    window: Vec<u8>,
    window_start: u64,
    reads: u64,
    bytes: u64,
}

impl RandomReader {
    /// Wrap a handle.
    ///
    /// # Errors
    /// Returns [`Error::Io`] if the handle cannot report its length.
    pub fn new(mut inner: Box<dyn ReadSeek>, path: PathBuf) -> Result<Self> {
        let len = inner
            .seek(SeekFrom::End(0))
            .map_err(|e| Error::io(&path, e))?;
        Ok(Self {
            inner,
            path,
            len,
            window: Vec::new(),
            window_start: 0,
            reads: 0,
            bytes: 0,
        })
    }

    /// Open `path` telling the OS the reads will be scattered.
    ///
    /// # Errors
    /// Returns [`Error::Io`] if the file cannot be opened.
    pub fn open(path: &Path) -> Result<Self> {
        let file = open_for_random_access(path).map_err(|e| Error::io(path, e))?;
        Self::new(Box::new(file), path.to_path_buf())
    }

    /// Length of the file.
    #[must_use]
    pub const fn file_len(&self) -> u64 {
        self.len
    }

    /// Reads issued so far.
    #[must_use]
    pub const fn reads(&self) -> u64 {
        self.reads
    }

    /// Bytes transferred so far.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The bytes from `offset` to the end of the window, holding at least `need` of them.
    ///
    /// When the window already holds `need` bytes at `offset` it is reused as it stands, which is
    /// what makes a display set and an erase in one cluster cost one read. Otherwise it is
    /// refilled with `fill` bytes from `offset` — a caller that will go on to read what follows
    /// asks for more than it needs, and one that will not asks for exactly that. Shorter than
    /// `need` only at end of file.
    ///
    /// # Errors
    /// Returns [`Error::Io`] on a failed seek or read.
    pub fn window_at(&mut self, offset: u64, need: usize, fill: usize) -> Result<&[u8]> {
        let needed_end = offset.saturating_add(need as u64).min(self.len);
        let window_end = self.window_start + self.window.len() as u64;
        let covered = offset >= self.window_start && needed_end <= window_end;
        if !covered {
            self.fill(offset, fill.max(need))?;
        }
        let from = usize::try_from(offset - self.window_start).unwrap_or(usize::MAX);
        Ok(self.window.get(from..).unwrap_or_default())
    }

    /// Exactly `count` bytes at `offset`, reusing whatever the window already holds.
    ///
    /// # Errors
    /// Returns [`Error::Io`] on a failed read, and at end of file: a block the file is too short
    /// to hold is truncated, and a truncated block is rejected rather than padded.
    pub fn read_at(&mut self, offset: u64, count: usize) -> Result<Vec<u8>> {
        let end = offset.saturating_add(count as u64);
        if end > self.len {
            return Err(Error::io(
                &self.path,
                std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!("{count} bytes at {offset} run past the end of the file"),
                ),
            ));
        }

        let window_end = self.window_start + self.window.len() as u64;
        let mut out = Vec::with_capacity(count);
        if offset >= self.window_start && offset < window_end {
            let from = usize::try_from(offset - self.window_start).unwrap_or(usize::MAX);
            let to = usize::try_from(end.min(window_end) - self.window_start).unwrap_or(usize::MAX);
            out.extend_from_slice(self.window.get(from..to).unwrap_or_default());
        }
        if out.len() < count {
            let rest = offset + out.len() as u64;
            let mut tail = vec![0u8; count - out.len()];
            self.inner
                .seek(SeekFrom::Start(rest))
                .map_err(|e| Error::io(&self.path, e))?;
            self.inner
                .read_exact(&mut tail)
                .map_err(|e| Error::io(&self.path, e))?;
            self.reads += 1;
            self.bytes += tail.len() as u64;
            out.extend_from_slice(&tail);
        }
        Ok(out)
    }

    /// Discard the window and read up to `count` bytes from `offset` into it.
    fn fill(&mut self, offset: u64, count: usize) -> Result<()> {
        self.window.clear();
        self.window_start = offset;
        let available = usize::try_from(self.len.saturating_sub(offset)).unwrap_or(usize::MAX);
        let want = count.min(available);
        if want == 0 {
            return Ok(());
        }
        self.window.resize(want, 0);
        self.inner
            .seek(SeekFrom::Start(offset))
            .map_err(|e| Error::io(&self.path, e))?;
        // A network filesystem may return a short read without being at end of file.
        let mut filled = 0;
        while filled < want {
            match self.inner.read(&mut self.window[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(Error::io(&self.path, e)),
            }
        }
        self.window.truncate(filled);
        self.reads += 1;
        self.bytes += filled as u64;
        Ok(())
    }
}

/// Open a file, telling the OS the access pattern is random.
///
/// Trickster measured what this is for: with readahead left on, 3.4 MB of wanted bytes transferred
/// 96.7 MB, because the kernel fetched its sequential window around every seek. On Windows the
/// hint is a flag `std` can pass. Elsewhere it is `posix_fadvise`, which needs `unsafe` or a crate,
/// and the workspace forbids the one and the library crates take neither — so it waits on a
/// measurement of what readahead costs there (#260). On Windows over SMB it made no measurable
/// difference; `docs/indexed-demux.md` has the figures.
#[cfg(windows)]
fn open_for_random_access(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    /// `FILE_FLAG_RANDOM_ACCESS`, spelled out rather than taken from a Win32 crate.
    const FILE_FLAG_RANDOM_ACCESS: u32 = 0x1000_0000;
    File::options()
        .read(true)
        .custom_flags(FILE_FLAG_RANDOM_ACCESS)
        .open(path)
}

#[cfg(not(windows))]
fn open_for_random_access(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn reader(bytes: Vec<u8>) -> RandomReader {
        RandomReader::new(Box::new(Cursor::new(bytes)), PathBuf::from("test.bin")).unwrap()
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0u8..=255).cycle().take(len).collect()
    }

    #[test]
    fn a_window_read_is_one_request_for_the_span_and_nothing_around_it() {
        let bytes = pattern(1 << 20);
        let mut r = reader(bytes.clone());
        let window = r.window_at(300_000, 64, WINDOW).unwrap();
        assert_eq!(window, &bytes[300_000..300_000 + WINDOW]);
        assert_eq!((r.reads(), r.bytes()), (1, WINDOW as u64));
    }

    #[test]
    fn a_span_already_in_the_window_costs_no_read() {
        // A display set and its erase can sit close together; the second must not be fetched again.
        let bytes = pattern(1 << 20);
        let mut r = reader(bytes.clone());
        r.window_at(1_000, 64, WINDOW).unwrap();
        assert_eq!(&r.window_at(60_000, 64, WINDOW).unwrap()[..64], &bytes[60_000..60_064]);
        assert_eq!(r.read_at(6_000, 2_000).unwrap(), &bytes[6_000..8_000]);
        assert_eq!(r.reads(), 1);
    }

    #[test]
    fn a_window_that_holds_too_little_at_the_offset_is_refilled_from_it() {
        let bytes = pattern(1 << 20);
        let mut r = reader(bytes.clone());
        r.window_at(0, 64, WINDOW).unwrap();
        let tail = WINDOW - 10;
        assert_eq!(r.window_at(tail as u64, 64, 64).unwrap(), &bytes[tail..tail + 64]);
        assert_eq!((r.reads(), r.bytes()), (2, WINDOW as u64 + 64));
    }

    #[test]
    fn a_payload_that_overruns_the_window_costs_exactly_one_more_read_for_the_remainder() {
        let bytes = pattern(1 << 20);
        let mut r = reader(bytes.clone());
        r.window_at(100, 64, WINDOW).unwrap();
        let payload = r.read_at(200, WINDOW + 5_000).unwrap();
        assert_eq!(payload, &bytes[200..200 + WINDOW + 5_000]);
        assert_eq!(r.reads(), 2);
        // Only what the window lacked crossed the second time.
        assert_eq!(r.bytes(), (WINDOW + 5_000 + 100) as u64);
    }

    #[test]
    fn a_window_past_the_end_is_short_rather_than_an_error() {
        let bytes = pattern(10_000);
        let mut r = reader(bytes.clone());
        assert_eq!(r.window_at(9_000, 64, WINDOW).unwrap(), &bytes[9_000..]);
        assert!(r.window_at(20_000, 10, 10).unwrap().is_empty());
    }

    #[test]
    fn a_read_the_file_is_too_short_to_hold_is_rejected_rather_than_padded() {
        let mut r = reader(pattern(10_000));
        assert!(matches!(r.read_at(9_000, 2_000), Err(Error::Io { .. })));
    }
}
