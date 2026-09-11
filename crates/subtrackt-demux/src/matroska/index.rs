//! Finding a track's blocks through the file's own index.
//!
//! A Matroska file written by mkvmerge indexes its subtitle tracks. Its default is `--cues iframes`
//! for video *and* subtitle tracks, every subtitle block is a keyframe, so every subtitle block
//! gets a `CuePoint` — and since 2013 each entry carries `CueRelativePosition`, the block's offset
//! inside its cluster. `FFmpeg`'s muxer indexes every subtitle packet the same way. The cluster's
//! position and the block's offset inside it place every block of every track to the byte, and
//! one read of the `Cues` element, a couple of megabytes at the tail of the file, finds them all.
//!
//! Trickster built the same thing first (Sovereign.Trickster#32) and this follows it, with one
//! difference on purpose. Trickster finds most blocks with a single read by *assuming* the next
//! cluster's header is as long as the last one, and then takes the block's time from the index.
//! Here the cluster header is always read, so a block's time is its cluster's timestamp plus its
//! own offset — the same arithmetic the walk does, from the same bytes. Two ways of reaching a
//! packet then agree by construction rather than because the muxer wrote a consistent index, and
//! that costs one small read per cluster that holds a wanted block.
//!
//! Nothing here decides whether to trust the index; see `MatroskaReader::select` for that.

use std::io::Cursor;
use std::path::Path;

use subtrackt_core::Result;

use super::ebml::{self, EbmlReader, Walk};
use super::random::{RandomReader, WINDOW};
use super::{BLOCK, BLOCK_GROUP, CLUSTER, CLUSTER_TIMESTAMP, SIMPLE_BLOCK};

// Element IDs only this module needs.
pub const SEEK_HEAD: u32 = 0x114D_9B74;
const SEEK: u32 = 0x4DBB;
const SEEK_ID: u32 = 0x53AB;
const SEEK_POSITION: u32 = 0x53AC;
pub const CUES: u32 = 0x1C53_BB6B;
const CUE_POINT: u32 = 0xBB;
const CUE_TIME: u32 = 0xB3;
const CUE_TRACK_POSITIONS: u32 = 0xB7;
const CUE_TRACK: u32 = 0xF7;
const CUE_CLUSTER_POSITION: u32 = 0xF1;
const CUE_RELATIVE_POSITION: u32 = 0xF0;

/// Enough of a cluster's start to hold its header, a CRC-32 child, and the timestamp after it.
/// `FFmpeg` writes a CRC-32 on every cluster, and the timestamp comes next.
const CLUSTER_HEAD_BYTES: usize = 64;

/// The longest element header: a 4-byte ID and an 8-byte size.
const MAX_ELEMENT_HEADER: usize = 12;

/// Enough bytes at a block's position to parse everything in front of its payload: a group's
/// header, the `Block` header inside it, and the track number, timestamp and flags.
const BLOCK_HEAD_BYTES: usize = 48;

/// The largest element this reads whole into memory.
///
/// A `Cues` element for a feature film with every video keyframe and 42 subtitle tracks indexed is
/// 2.3 MB. A size field claiming gigabytes is a damaged file, and allocating what it claims would
/// make the damage a crash.
const MAX_INDEX_BYTES: u64 = 256 * 1024 * 1024;

/// One `CueTrackPositions` entry: where one block of one track is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CuePoint {
    /// Absolute offset of the cluster's element header. First, so points sort into file order.
    pub cluster: u64,
    /// Offset of the block's element from the first byte of the cluster's body. Absent in files
    /// from before 2013.
    pub relative: Option<u64>,
    /// The track the block belongs to.
    pub track: u64,
    /// `CueTime`, in the file's timestamp units. Used only to name an entry in a report; a block's
    /// time is always read from the block.
    pub time: u64,
}

/// `SeekHead` entries: element ID to absolute position.
///
/// # Errors
/// Propagates malformed variable-length integers.
pub fn parse_seek_head(body: &[u8], segment_start: u64, path: &Path) -> Result<Vec<(u32, u64)>> {
    let mut reader = EbmlReader::new(Cursor::new(body), path);
    let mut found = Vec::new();
    reader.children_until(0, body.len() as u64, |reader, seek| {
        if seek.id != SEEK {
            return Ok(Walk::Continue);
        }
        let (mut id, mut position) = (None, None);
        reader.children(seek, |reader, field| {
            match field.id {
                SEEK_ID => id = u32::try_from(reader.read_uint(field)?).ok(),
                SEEK_POSITION => position = Some(reader.read_uint(field)?),
                _ => {}
            }
            Ok(Walk::Continue)
        })?;
        if let (Some(id), Some(position)) =
            (id, position.and_then(|p| segment_start.checked_add(p)))
        {
            found.push((id, position));
        }
        Ok(Walk::Continue)
    })?;
    Ok(found)
}

/// Every entry in a `Cues` body for one of `tracks`, in file order, duplicates dropped.
///
/// # Errors
/// Propagates malformed variable-length integers.
pub fn parse_cues(
    body: &[u8],
    segment_start: u64,
    tracks: &[u64],
    path: &Path,
) -> Result<Vec<CuePoint>> {
    let mut reader = EbmlReader::new(Cursor::new(body), path);
    let mut points = Vec::new();
    reader.children_until(0, body.len() as u64, |reader, point| {
        if point.id != CUE_POINT {
            return Ok(Walk::Continue);
        }
        let mut time = None;
        let mut positions = Vec::new();
        reader.children(point, |reader, field| {
            match field.id {
                CUE_TIME => time = Some(reader.read_uint(field)?),
                CUE_TRACK_POSITIONS => {
                    let (mut track, mut cluster, mut relative) = (None, None, None);
                    reader.children(field, |reader, entry| {
                        match entry.id {
                            CUE_TRACK => track = Some(reader.read_uint(entry)?),
                            CUE_CLUSTER_POSITION => cluster = Some(reader.read_uint(entry)?),
                            CUE_RELATIVE_POSITION => relative = Some(reader.read_uint(entry)?),
                            _ => {}
                        }
                        Ok(Walk::Continue)
                    })?;
                    if let (Some(track), Some(cluster)) = (track, cluster) {
                        positions.push((track, cluster, relative));
                    }
                }
                _ => {}
            }
            Ok(Walk::Continue)
        })?;
        let Some(time) = time else {
            return Ok(Walk::Continue);
        };
        for (track, cluster, relative) in positions {
            if !tracks.contains(&track) {
                continue;
            }
            if let Some(cluster) = segment_start.checked_add(cluster) {
                points.push(CuePoint { cluster, relative, track, time });
            }
        }
        Ok(Walk::Continue)
    })?;
    points.sort_unstable();
    points.dedup_by_key(|p| (p.cluster, p.relative, p.track));
    Ok(points)
}

/// Read the element at `position` whole, if it is an `id` of a size worth reading.
///
/// `None` when the bytes there are something else: the caller is following a pointer the file
/// wrote about itself, and a pointer that does not lead where it says is a file to walk, not one
/// to refuse.
///
/// # Errors
/// Returns [`subtrackt_core::Error::Io`] on a failed read.
pub fn read_element(reader: &mut RandomReader, position: u64, id: u32) -> Result<Option<Vec<u8>>> {
    let head = reader.window_at(position, MAX_ELEMENT_HEADER, WINDOW)?;
    let Some((found, id_width)) = ebml::vint_from_slice(head, true) else {
        return Ok(None);
    };
    let Some((size, size_width)) = head
        .get(id_width..)
        .and_then(|b| ebml::vint_from_slice(b, false))
    else {
        return Ok(None);
    };
    if u32::try_from(found).ok() != Some(id) || is_unknown(size, size_width) {
        return Ok(None);
    }
    let body_start = position + (id_width + size_width) as u64;
    if size > MAX_INDEX_BYTES || body_start.saturating_add(size) > reader.file_len() {
        return Ok(None);
    }
    let size = usize::try_from(size).unwrap_or(usize::MAX);
    reader.read_at(body_start, size).map(Some)
}

/// What a cluster's first bytes say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterHead {
    /// Absolute offset of the cluster's element header.
    pub position: u64,
    /// Absolute offset of the cluster's first child, which `CueRelativePosition` counts from.
    pub body_start: u64,
    /// `ClusterTimestamp`.
    pub timestamp: u64,
}

/// Read the head of the cluster at `position`.
///
/// `None` if what is there is not a cluster whose timestamp comes before its first block. The
/// specification puts the timestamp first; `FFmpeg` puts a CRC-32 in front of it, and anything else
/// that is not a block is stepped over the same way.
///
/// # Errors
/// Returns [`subtrackt_core::Error::Io`] on a failed read.
pub fn read_cluster_head(reader: &mut RandomReader, position: u64) -> Result<Option<ClusterHead>> {
    let bytes = reader.window_at(position, CLUSTER_HEAD_BYTES, CLUSTER_HEAD_BYTES)?;
    let Some((id, id_width)) = ebml::vint_from_slice(bytes, true) else {
        return Ok(None);
    };
    let Some((_, size_width)) = bytes
        .get(id_width..)
        .and_then(|b| ebml::vint_from_slice(b, false))
    else {
        return Ok(None);
    };
    if id != u64::from(CLUSTER) {
        return Ok(None);
    }

    let mut at = id_width + size_width;
    let body_start = position + at as u64;
    while let Some((child, width, size, size_width)) = element_at(bytes, at) {
        let data = at + width + size_width;
        let Ok(size) = usize::try_from(size) else {
            return Ok(None);
        };
        match u32::try_from(child).unwrap_or(0) {
            CLUSTER_TIMESTAMP => {
                let Some(value) = bytes.get(data..data + size).filter(|v| v.len() <= 8) else {
                    return Ok(None);
                };
                let timestamp = value.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
                return Ok(Some(ClusterHead { position, body_start, timestamp }));
            }
            SIMPLE_BLOCK | BLOCK_GROUP => return Ok(None),
            _ => at = data.saturating_add(size),
        }
    }
    Ok(None)
}

/// Where a cue point led.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Located {
    /// A block on the track. `start` is the absolute offset of the `SimpleBlock` or `Block`
    /// element, which is what the walk compares against when it has to take over.
    Block {
        /// Absolute offset of the block element.
        start: u64,
        /// The block's timestamp relative to its cluster.
        relative: i16,
        /// The block's flags byte.
        flags: u8,
        /// The codec bytes, with content encodings still applied.
        payload: Vec<u8>,
    },
    /// A block the walk also passes over without yielding anything: a zero-length element, or
    /// one too short to hold its own header. Mirrored rather than refused, because two ways of
    /// reading one track must yield the same packets.
    Nothing {
        /// Absolute offset of the block element.
        start: u64,
    },
    /// The bytes there are not a block on this track. The index does not describe the file.
    Lost,
}

/// Read the block `relative` bytes into the cluster `head` describes, for `track`.
///
/// # Errors
/// Returns [`subtrackt_core::Error::Io`] on a failed read, including a block the file is too short
/// to hold.
pub fn read_block(
    reader: &mut RandomReader,
    head: &ClusterHead,
    relative: u64,
    track: u64,
) -> Result<Located> {
    let Some(position) = head.body_start.checked_add(relative) else {
        return Ok(Located::Lost);
    };
    let file_len = reader.file_len();
    let window = reader.window_at(position, BLOCK_HEAD_BYTES, WINDOW)?;

    let Some((id, id_width, size, size_width)) = element_at(window, 0) else {
        return Ok(Located::Lost);
    };
    // Where the block element begins, and its declared size.
    let (start_in_window, block_size) = match u32::try_from(id).unwrap_or(0) {
        SIMPLE_BLOCK => (0, size),
        BLOCK_GROUP => {
            // The walk descends into the group and takes its `Block`; so does this. The group
            // may carry other children, and the `Block` is found among them rather than assumed
            // to be first.
            let group_end =
                (id_width + size_width).saturating_add(usize::try_from(size).unwrap_or(usize::MAX));
            let mut at = id_width + size_width;
            let mut found = None;
            while at < group_end {
                let Some((child, width, child_size, child_width)) = element_at(window, at) else {
                    break;
                };
                if u32::try_from(child).ok() == Some(BLOCK) {
                    found = Some((at, child_size));
                    break;
                }
                let Ok(child_size) = usize::try_from(child_size) else {
                    break;
                };
                at = at
                    .saturating_add(width + child_width)
                    .saturating_add(child_size);
            }
            match found {
                Some(block) => block,
                None => return Ok(Located::Lost),
            }
        }
        _ => return Ok(Located::Lost),
    };

    let start = position + start_in_window as u64;
    let Some((_, width, _, block_size_width)) = element_at(window, start_in_window) else {
        return Ok(Located::Lost);
    };
    let body_start = start + (width + block_size_width) as u64;
    if body_start.saturating_add(block_size) > file_len {
        return Ok(Located::Lost);
    }
    if block_size == 0 {
        return Ok(Located::Nothing { start });
    }

    // The same peek the walk takes: a track number of at most 8 bytes, 2 of timestamp, 1 of flags.
    let peek_len = usize::try_from(block_size).unwrap_or(usize::MAX).min(11);
    let peek = reader.read_at(body_start, peek_len)?;
    let Some((block_track, consumed)) = ebml::vint_from_slice(&peek, false) else {
        return Ok(Located::Lost);
    };
    if block_track != track {
        return Ok(Located::Lost);
    }
    let Some(fields) = peek.get(consumed..consumed + 3) else {
        return Ok(Located::Nothing { start });
    };
    let relative_ts = i16::from_be_bytes([fields[0], fields[1]]);
    let flags = fields[2];

    let header_len = consumed + 3;
    let payload_len = usize::try_from(block_size).unwrap_or(usize::MAX) - header_len;
    let payload = reader.read_at(body_start + header_len as u64, payload_len)?;
    Ok(Located::Block { start, relative: relative_ts, flags, payload })
}

/// The element header at `at` in `bytes`: ID, ID width, size, size width.
fn element_at(bytes: &[u8], at: usize) -> Option<(u64, usize, u64, usize)> {
    let (id, id_width) = ebml::vint_from_slice(bytes.get(at..)?, true)?;
    let (size, size_width) = ebml::vint_from_slice(bytes.get(at + id_width..)?, false)?;
    if is_unknown(size, size_width) {
        return None;
    }
    Some((id, id_width, size, size_width))
}

/// Whether a size field is EBML's all-ones spelling of "unknown".
fn is_unknown(size: u64, width: usize) -> bool {
    u32::try_from(width).is_ok_and(|w| w <= 8 && size == (1u64 << (7 * w)) - 1)
}
