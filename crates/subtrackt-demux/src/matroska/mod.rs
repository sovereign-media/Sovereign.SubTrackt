//! Matroska demuxing.
//!
//! Scoped by measurement rather than ambition. A survey of 1,328 titles carrying bitmap subtitles
//! found 1,326 of them in Matroska, one `.m2ts` and one `.iso` — so a native parser covers 99.8%
//! of the library, and the `ffmpeg-next` dependency #4 weighed against it buys 0.2%. That is what
//! keeps the library crates dependency-free and cross-compilation to `linux/arm64` uneventful.
//!
//! Hand-rolled rather than delegated to `symphonia-format-mkv`, which was evaluated against real
//! media and rejected on measurement: it does not decompress `ContentCompression` at all, and 83%
//! of this library's PGS tracks are zlib-compressed, so the code that caused most of the bugs here
//! would still have to be written. Its `next_packet()` also returns every packet of every track,
//! which ran at 77 MB/s against 177 MB/s for this reader doing the whole pipeline. The full
//! comparison and the conditions for revisiting it are in `docs/architecture.md`.
//!
//! A film is several gigabytes; the subtitle track is a rounding error inside it. There are two
//! ways to find it, and the reader takes the cheaper one the file allows:
//!
//! * **Through the index.** The `Cues` element usually says where every subtitle block is, to the
//!   byte. Reading it and then one window per block reads a fraction of a percent of the file.
//!   `index` has how, and #258 what it measured.
//! * **By walking.** Every cluster, front to back, stepping past every block that is not the track
//!   asked for. This reads the whole file, and it is what a file with no usable index gets. It is
//!   also where the index path goes if an entry stops describing the file, from that cluster on,
//!   so a damaged index costs time and never a cue.

pub mod ebml;
mod index;
mod random;

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};

use subtrackt_core::{Error, PTS_HZ, Result};

use crate::{Access, BitmapCodec, Packet, StreamInfo, SubtitleSource};
use ebml::{EbmlReader, ElementHeader, UNKNOWN_SIZE, Walk};
use index::{ClusterHead, CuePoint, Located};
use random::RandomReader;

// Element IDs, as the specification quotes them.
const SEGMENT: u32 = 0x1853_8067;
const INFO: u32 = 0x1549_A966;
const TIMESTAMP_SCALE: u32 = 0x002A_D7B1;
const TRACKS: u32 = 0x1654_AE6B;
const TRACK_ENTRY: u32 = 0x00AE;
const TRACK_NUMBER: u32 = 0x00D7;
const TRACK_TYPE: u32 = 0x0083;
const CODEC_ID: u32 = 0x0086;
const LANGUAGE: u32 = 0x0022_B59C;
const NAME: u32 = 0x536E;
const CODEC_PRIVATE: u32 = 0x63A2;
const VIDEO: u32 = 0x00E0;
const PIXEL_WIDTH: u32 = 0x00B0;
const PIXEL_HEIGHT: u32 = 0x00BA;
const CLUSTER: u32 = 0x1F43_B675;
const CLUSTER_TIMESTAMP: u32 = 0x00E7;
const SIMPLE_BLOCK: u32 = 0x00A3;
const BLOCK_GROUP: u32 = 0x00A0;
const BLOCK: u32 = 0x00A1;
const CONTENT_ENCODINGS: u32 = 0x6D80;
const CONTENT_ENCODING: u32 = 0x6240;
const CONTENT_COMPRESSION: u32 = 0x5034;
const CONTENT_COMP_ALGO: u32 = 0x4254;
const CONTENT_COMP_SETTINGS: u32 = 0x4255;

/// Track type for subtitles.
const TRACK_TYPE_SUBTITLE: u64 = 0x11;
/// Track type for video, which is where the subtitle plane dimensions come from.
const TRACK_TYPE_VIDEO: u64 = 0x01;

/// Default timestamp scale: one millisecond in nanoseconds.
const DEFAULT_TIMESTAMP_SCALE: u64 = 1_000_000;

/// How a track's block payloads are compressed.
///
/// Not a corner case: a scan of 60 titles from the library found 83% of PGS tracks compressed,
/// and 68% of files carrying at least one. A reader that ignored this would fail on most of the
/// library, which is why `miniz_oxide` is the one dependency the demux crate takes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Compression {
    /// Payloads are stored as-is.
    None,
    /// zlib, which is `ContentCompAlgo` 0 and also the default when the element is absent.
    Zlib,
    /// Header stripping: `ContentCompSettings` holds bytes removed from the front of every block.
    HeaderStrip(Vec<u8>),
}

/// Map a Matroska codec identifier to a bitmap subtitle codec.
fn bitmap_codec(codec_id: &str) -> Option<BitmapCodec> {
    match codec_id {
        "S_HDMV/PGS" => Some(BitmapCodec::Pgs),
        "S_VOBSUB" => Some(BitmapCodec::VobSub),
        _ => None,
    }
}

/// A subtitle track as declared in the file, paired with its Matroska track number.
#[derive(Debug, Clone)]
struct Track {
    /// The number blocks reference, which is not the same as the index we expose.
    number: u64,
    info: StreamInfo,
    compression: Compression,
}

/// The largest `SeekHead` read into memory. mkvmerge's is under a hundred bytes.
const MAX_SEEK_HEAD: u64 = 64 * 1024;

/// What the file's index holds for its subtitle tracks, read once for all of them.
enum Index {
    /// Every entry for a bitmap subtitle track, in file order.
    Points(Vec<CuePoint>),
    /// There is nothing to follow, and why.
    Absent(&'static str),
}

/// How the selected track is being read.
enum Mode {
    /// Front to back from `cursor`.
    Walk,
    /// Entry by entry from the index.
    Indexed {
        /// The selected track's entries, in file order.
        points: Vec<CuePoint>,
        /// The next entry to follow.
        next: usize,
        /// The cluster last read. A display set and its erase often share one.
        cluster: Option<ClusterHead>,
    },
}

/// Reads bitmap subtitle packets out of a Matroska file.
pub struct MatroskaReader<R> {
    reader: EbmlReader<R>,
    tracks: Vec<Track>,
    streams: Vec<StreamInfo>,
    /// Nanoseconds per timestamp unit.
    timestamp_scale: u64,
    /// Absolute offset of the segment's body, which every position in the index counts from.
    segment_start: u64,
    /// What the `SeekHead` elements before the first cluster point at, by element ID.
    seeks: Vec<(u32, u64)>,
    /// Where the `SeekHead` elements already read begin, so a pointer to one is not followed.
    seek_heads: Vec<u64>,
    /// Where `Cues` begins, when it comes before the first cluster rather than at the tail.
    cues_at: Option<u64>,
    /// The handle the index path reads through. Separate from the walk's, which is buffered for
    /// reading straight through and would fetch a megabyte for every block.
    random: Option<RandomReader>,
    /// The file's index, once something has asked for it.
    index: Option<Index>,
    /// Whether to follow the index when it can be. Off only to compare the two paths.
    use_index: bool,
    /// How the selected track is being read.
    mode: Mode,
    /// Whether the selected track was started from the index, and why not if it was not.
    indexed: std::result::Result<(), &'static str>,
    /// Blocks the index led to on the selected track.
    indexed_blocks: u64,
    /// Where the index stopped describing the file, if it did, in 90 kHz ticks.
    walked_from: Option<u64>,
    /// The walk yields nothing at or before this offset. Set when it takes over from the index,
    /// which has already yielded everything up to the block it last read.
    skip_through: Option<u64>,
    /// Absolute offset of the first cluster, so `select` can rewind.
    clusters_start: u64,
    /// Matroska number of the track currently selected, and how its payloads are compressed.
    selected: Option<(u64, Compression)>,
    /// Queue of packets decoded from the cluster currently being walked.
    pending: std::collections::VecDeque<Packet>,
    /// Where the next cluster-level read resumes.
    cursor: u64,
    /// One past the last byte of the segment.
    segment_end: u64,
    /// Timestamp of the cluster being walked.
    cluster_timestamp: u64,
    /// Where the current cluster's children end.
    cluster_end: u64,
    inside_cluster: bool,
}

impl MatroskaReader<BufReader<File>> {
    /// Open a Matroska file and read its track headers.
    ///
    /// # Errors
    /// Returns [`Error::Io`] if the file cannot be read and [`Error::Demux`] if it is not
    /// Matroska or declares no bitmap subtitle track.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|e| Error::io(path, e))?;
        // A large buffer keeps the read-through skipping in `EbmlReader::seek_to` effective
        // over a network filesystem, where this tool will usually be pointed.
        let mut reader =
            Self::from_reader(BufReader::with_capacity(1 << 20, file), path.to_path_buf())?;
        // Opened with the random-access hint where the platform allows one, which is why it is
        // opened here rather than handed in: the hint belongs to the handle, and on the walk's
        // handle it would turn off the readahead the walk depends on.
        reader.random = Some(RandomReader::open(path)?);
        Ok(reader)
    }
}

impl<R: Read + Seek> MatroskaReader<R> {
    /// Give the index path a handle to read through.
    ///
    /// [`Self::open`] does this itself. A reader built with [`Self::from_reader`] and not given
    /// one walks every track, which is how it behaved before the index path existed.
    ///
    /// # Errors
    /// Returns [`Error::Io`] if the handle cannot report its length.
    pub fn with_random_access(mut self, handle: impl Read + Seek + Send + 'static) -> Result<Self> {
        self.random = Some(RandomReader::new(Box::new(handle), self.reader.path().to_path_buf())?);
        self.index = None;
        Ok(self)
    }

    /// Whether [`SubtitleSource::select`] follows the index when the file has a usable one.
    ///
    /// On by default. Off exists to read one track both ways and compare what comes out, which is
    /// the check #258 makes and a library survey would repeat.
    pub const fn use_index(&mut self, enabled: bool) {
        self.use_index = enabled;
    }
    /// Parse track headers from an already-open reader.
    ///
    /// # Errors
    /// Returns [`Error::Demux`] if the file is not Matroska or declares no bitmap subtitle track.
    pub fn from_reader(inner: R, path: PathBuf) -> Result<Self> {
        let mut reader = EbmlReader::new(inner, path);
        let file_len = reader.byte_len()?;

        let segment = find_segment(&mut reader)?;
        let segment_end = segment.body_end().unwrap_or(file_len);

        let mut timestamp_scale = DEFAULT_TIMESTAMP_SCALE;
        let mut tracks: Vec<Track> = Vec::new();
        let mut plane = (0u32, 0u32);
        let mut clusters_start = None;
        let mut seeks = Vec::new();
        let mut seek_heads = Vec::new();
        let mut cues_at = None;

        // Walk the segment's children until the first cluster. Everything the reader needs is
        // declared before playback data begins — except `Cues`, which is usually at the tail and
        // is found through the `SeekHead` that points at it.
        reader.children_until(segment.body_start, segment_end, |reader, header| {
            match header.id {
                INFO => timestamp_scale = read_timestamp_scale(reader, header)?,
                index::SEEK_HEAD if header.size <= MAX_SEEK_HEAD => {
                    seek_heads.push(header.start);
                    let body = reader.read_exact(usize::try_from(header.size).unwrap_or(0))?;
                    // A SeekHead that does not parse costs the index, never the file: the walk
                    // does not need it.
                    seeks.extend(
                        index::parse_seek_head(&body, segment.body_start, reader.path())
                            .unwrap_or_default(),
                    );
                }
                index::CUES => cues_at = Some(header.start),
                TRACKS => {
                    let (found, video_plane) = read_tracks(reader, header)?;
                    tracks = found;
                    if video_plane != (0, 0) {
                        plane = video_plane;
                    }
                }
                CLUSTER => {
                    // Where the *header* begins, not its body: `select` rewinds to here and has to
                    // read the cluster from its first byte. Everything the reader needs is
                    // declared before playback data starts, so this is also where the walk ends.
                    clusters_start = Some(header.start);
                    return Ok(Walk::Stop);
                }
                _ => {}
            }
            Ok(Walk::Continue)
        })?;

        // The subtitle plane matches the video frame for PGS, and the track headers do not carry
        // it, so it is taken from the video track.
        for track in &mut tracks {
            track.info.plane_width = plane.0;
            track.info.plane_height = plane.1;
        }

        if tracks.is_empty() {
            return Err(Error::Demux(format!(
                "{} declares no PGS or VOBSUB subtitle track",
                reader.path().display()
            )));
        }

        let clusters_start = clusters_start.unwrap_or(segment_end);
        let streams = tracks.iter().map(|t| t.info.clone()).collect();

        Ok(Self {
            reader,
            tracks,
            streams,
            timestamp_scale,
            segment_start: segment.body_start,
            seeks,
            seek_heads,
            cues_at,
            random: None,
            index: None,
            use_index: true,
            mode: Mode::Walk,
            indexed: Err("no track has been selected"),
            indexed_blocks: 0,
            walked_from: None,
            skip_through: None,
            clusters_start,
            selected: None,
            pending: std::collections::VecDeque::new(),
            cursor: clusters_start,
            segment_end,
            cluster_timestamp: 0,
            cluster_end: 0,
            inside_cluster: false,
        })
    }

    /// Convert a Matroska timestamp to 90 kHz ticks.
    fn to_ticks(&self, timestamp: i64) -> u64 {
        let nanos = u128::from(timestamp.max(0).unsigned_abs()) * u128::from(self.timestamp_scale);
        u64::try_from(nanos * u128::from(PTS_HZ) / 1_000_000_000).unwrap_or(u64::MAX)
    }

    /// Read the file's index for every bitmap subtitle track, once.
    fn load_index(&mut self) -> Result<()> {
        if self.index.is_some() {
            return Ok(());
        }
        let Some(random) = self.random.as_mut() else {
            self.index = Some(Index::Absent("no handle to read the index through"));
            return Ok(());
        };

        let find = |seeks: &[(u32, u64)]| {
            seeks
                .iter()
                .find(|(id, _)| *id == index::CUES)
                .map(|(_, at)| *at)
        };
        let mut cues_at = self.cues_at.or_else(|| find(&self.seeks));

        // A SeekHead may point at a second one rather than at `Cues` itself. mkvmerge writes one
        // at the tail when the front one has no room. Followed one level, as Trickster does.
        if cues_at.is_none() {
            let others: Vec<u64> = self
                .seeks
                .iter()
                .filter(|(id, at)| *id == index::SEEK_HEAD && !self.seek_heads.contains(at))
                .map(|(_, at)| *at)
                .collect();
            for at in others {
                if let Some(body) = index::read_element(random, at, index::SEEK_HEAD)? {
                    let path = self.reader.path();
                    let more = index::parse_seek_head(&body, self.segment_start, path);
                    self.seeks.extend(more.unwrap_or_default());
                }
            }
            cues_at = find(&self.seeks);
        }

        let Some(at) = cues_at else {
            self.index = Some(Index::Absent("the file has no Cues element"));
            return Ok(());
        };
        let Some(body) = index::read_element(random, at, index::CUES)? else {
            self.index = Some(Index::Absent("the Cues position does not hold a Cues element"));
            return Ok(());
        };
        let numbers: Vec<u64> = self.tracks.iter().map(|t| t.number).collect();
        self.index = Some(
            match index::parse_cues(&body, self.segment_start, &numbers, self.reader.path()) {
                Ok(points) => Index::Points(points),
                Err(_) => Index::Absent("the Cues element is malformed"),
            },
        );
        Ok(())
    }

    /// The index's entries for track `number`, or why the track has to be walked.
    fn entries_for(
        &mut self,
        number: u64,
    ) -> Result<std::result::Result<Vec<CuePoint>, &'static str>> {
        if !self.use_index {
            return Ok(Err("the index was not consulted"));
        }
        self.load_index()?;
        let points = match &self.index {
            Some(Index::Points(points)) => points,
            Some(Index::Absent(why)) => return Ok(Err(why)),
            None => return Ok(Err("the index was not read")),
        };
        let points: Vec<CuePoint> = points
            .iter()
            .filter(|p| p.track == number)
            .copied()
            .collect();
        if points.is_empty() {
            return Ok(Err("the index has no entries for this track"));
        }
        // A pre-2013 file indexes clusters but not blocks. Finding each block would mean walking
        // its cluster's element headers, which Trickster does; here the walk already exists and
        // reads those files exactly as it always has.
        if points.iter().any(|p| p.relative.is_none()) {
            return Ok(Err("the index gives clusters but not block positions"));
        }
        Ok(Ok(points))
    }

    /// Stop following the index and walk from the cluster of the entry that failed.
    ///
    /// Everything the index already yielded lies at or before [`Self::skip_through`], and the walk
    /// passes over it, so the track comes out whole: the index's failure costs the rest of the
    /// file's bytes, never a cue.
    fn walk_from(&mut self, point: &CuePoint) {
        self.walked_from = Some(self.to_ticks(i64::try_from(point.time).unwrap_or(i64::MAX)));
        self.mode = Mode::Walk;
        self.cursor = point.cluster;
        self.inside_cluster = false;
        self.cluster_timestamp = 0;
    }

    /// Fill [`Self::pending`] from the next entry in the index, while the index is being followed.
    fn fill_indexed(&mut self, selected: u64) -> Result<()> {
        while self.pending.is_empty() {
            let Mode::Indexed { points, next, cluster } = &mut self.mode else {
                return Ok(());
            };
            let Some(point) = points.get(*next).copied() else {
                return Ok(());
            };
            *next += 1;
            let cached = *cluster;

            let Some(random) = self.random.as_mut() else {
                self.walk_from(&point);
                return Ok(());
            };
            let head = match cached {
                Some(head) if head.position == point.cluster => Some(head),
                _ => index::read_cluster_head(random, point.cluster)?,
            };
            let located = match &head {
                Some(head) => {
                    index::read_block(random, head, point.relative.unwrap_or(0), selected)?
                }
                None => Located::Lost,
            };
            if let Mode::Indexed { cluster, .. } = &mut self.mode {
                *cluster = head;
            }

            match (located, head) {
                (Located::Block { start, relative, flags, payload }, Some(head)) => {
                    self.indexed_blocks += 1;
                    self.skip_through = Some(start);
                    if flags & 0x06 != 0 {
                        return Err(self.laced(selected));
                    }
                    let pts = self.to_ticks(
                        i64::try_from(head.timestamp)
                            .unwrap_or(i64::MAX)
                            .saturating_add(i64::from(relative)),
                    );
                    let payload = self.decompress(&payload, pts)?;
                    self.pending.push_back(Packet { pts, payload });
                }
                (Located::Nothing { start }, _) => {
                    self.indexed_blocks += 1;
                    self.skip_through = Some(start);
                }
                _ => self.walk_from(&point),
            }
        }
        Ok(())
    }

    /// The error for a laced block, which neither path decodes.
    fn laced(&self, track: u64) -> Error {
        Error::Demux(format!(
            "{}: laced subtitle block on track {track}, which this reader does not handle",
            self.reader.path().display()
        ))
    }

    /// Fill [`Self::pending`] from the next block that belongs to the selected track.
    fn fill(&mut self) -> Result<()> {
        let Some((selected, _)) = self.selected.clone() else {
            return Err(Error::Demux("no subtitle stream selected".into()));
        };

        if matches!(self.mode, Mode::Indexed { .. }) {
            self.fill_indexed(selected)?;
            if matches!(self.mode, Mode::Indexed { .. }) {
                return Ok(());
            }
        }

        while self.pending.is_empty() {
            if self.inside_cluster && self.cursor >= self.cluster_end {
                self.inside_cluster = false;
            }
            if self.cursor >= self.segment_end {
                return Ok(());
            }

            self.reader.seek_to(self.cursor)?;
            let Some(header) = self.reader.read_header()? else {
                return Ok(());
            };
            let next = header.body_end().unwrap_or(self.segment_end);

            if self.inside_cluster {
                match header.id {
                    CLUSTER_TIMESTAMP => {
                        self.cluster_timestamp = self.reader.read_uint(&header)?;
                    }
                    // A SimpleBlock and a BlockGroup's Block have the same layout; only the
                    // nesting differs.
                    SIMPLE_BLOCK | BLOCK => self.take_block(&header, selected)?,
                    BLOCK_GROUP => {
                        // Descend: the Block inside carries the payload.
                        self.cursor = header.body_start;
                        continue;
                    }
                    _ => {}
                }
                self.cursor = next;
            } else if header.id == CLUSTER {
                self.inside_cluster = true;
                self.cluster_end = header.body_end().unwrap_or(self.segment_end);
                self.cursor = header.body_start;
            } else {
                self.cursor = next;
            }
        }
        Ok(())
    }

    /// Read one block, queueing its payload if it belongs to the selected track.
    ///
    /// The track number comes first in the block, so it is read on its own before anything else.
    /// That matters more than it looks: a film is mostly video blocks, and reading each one in
    /// full merely to discover it is not the subtitle track meant pulling the entire 5.5 GB
    /// through a fresh allocation per block. Peeking the header first turns that into a seek.
    fn take_block(&mut self, header: &ElementHeader, selected: u64) -> Result<()> {
        // Taking over from the index: what it already yielded is not yielded twice.
        if self.skip_through.is_some_and(|last| header.start <= last) {
            return Ok(());
        }
        let Some(size) = usize::try_from(header.size).ok().filter(|s| *s > 0) else {
            return Ok(());
        };

        // A track number is at most 8 bytes, then 2 bytes of timestamp and 1 of flags.
        self.reader.seek_to(header.body_start)?;
        let peek_len = size.min(11);
        let peek = self.reader.read_exact(peek_len)?;

        let Some((track, consumed)) = read_block_track(&peek) else {
            return Ok(());
        };
        if track != selected {
            return Ok(());
        }

        let rest = &peek[consumed..];
        if rest.len() < 3 {
            return Ok(());
        }
        let relative = i64::from(i16::from_be_bytes([rest[0], rest[1]]));
        let flags = rest[2];

        // Lacing packs several frames into one block. Subtitle tracks do not use it, so rather
        // than implement it speculatively this refuses loudly if it ever turns up.
        if flags & 0x06 != 0 {
            return Err(self.laced(track));
        }

        // Only now, having established this is the track we want, read the payload.
        let header_len = consumed + 3;
        let mut payload = peek[header_len..].to_vec();
        if size > peek_len {
            self.reader
                .seek_to(header.body_start + header_len as u64 + payload.len() as u64)?;
            payload.extend_from_slice(&self.reader.read_exact(size - peek_len)?);
        }

        let pts = self.to_ticks(
            i64::try_from(self.cluster_timestamp)
                .unwrap_or(i64::MAX)
                .saturating_add(relative),
        );
        let payload = self.decompress(&payload, pts)?;
        self.pending.push_back(Packet { pts, payload });
        Ok(())
    }
}

impl<R: Read + Seek> MatroskaReader<R> {
    /// Undo whatever `ContentEncodings` declared for the selected track.
    fn decompress(&self, payload: &[u8], pts: u64) -> Result<Vec<u8>> {
        let compression = self
            .selected
            .as_ref()
            .map_or(&Compression::None, |(_, c)| c);

        match compression {
            Compression::None => Ok(payload.to_vec()),
            Compression::HeaderStrip(prefix) => {
                let mut out = prefix.clone();
                out.extend_from_slice(payload);
                Ok(out)
            }
            // The zlib wrapper carries an Adler-32 checksum, and miniz_oxide verifies it. A
            // corrupted block therefore fails loudly here rather than decoding into a plausible
            // but wrong bitmap, which is the failure mode this project exists to avoid.
            Compression::Zlib => {
                miniz_oxide::inflate::decompress_to_vec_zlib(payload).map_err(|e| {
                    Error::MalformedPacket {
                        codec: "matroska",
                        pts,
                        reason: format!("zlib-compressed block failed to inflate: {:?}", e.status),
                    }
                })
            }
        }
    }
}

/// Read the track number a block belongs to, and how many bytes it took.
///
/// The same variable-length integer [`EbmlReader::read_vint`] reads, over a slice rather than over
/// the reader — a block's track number is already in the peeked bytes and seeking back to read it
/// would undo the whole point of peeking. Both spellings now share the arithmetic; only where the
/// bytes come from differs.
fn read_block_track(body: &[u8]) -> Option<(u64, usize)> {
    ebml::vint_from_slice(body, false)
}

/// Find the `Segment` element, checking the file really is Matroska on the way.
fn find_segment<R: Read + Seek>(reader: &mut EbmlReader<R>) -> Result<ElementHeader> {
    reader.seek_to(0)?;
    let Some(first) = reader.read_header()? else {
        return Err(Error::Demux(format!("{} is empty", reader.path().display())));
    };
    if first.id != 0x1A45_DFA3 {
        return Err(Error::Demux(format!(
            "{} does not begin with an EBML header",
            reader.path().display()
        )));
    }
    reader.skip(&first)?;

    loop {
        let Some(header) = reader.read_header()? else {
            return Err(Error::Demux(format!("{} contains no segment", reader.path().display())));
        };
        if header.id == SEGMENT {
            return Ok(header);
        }
        if header.size == UNKNOWN_SIZE {
            return Err(Error::Demux(format!(
                "{}: unknown-size element 0x{:X} before the segment",
                reader.path().display(),
                header.id
            )));
        }
        reader.skip(&header)?;
    }
}

/// Read the timestamp scale out of an `Info` element.
fn read_timestamp_scale<R: Read + Seek>(
    reader: &mut EbmlReader<R>,
    info: &ElementHeader,
) -> Result<u64> {
    let mut scale = DEFAULT_TIMESTAMP_SCALE;
    reader.children(info, |reader, child| {
        if child.id != TIMESTAMP_SCALE {
            return Ok(Walk::Continue);
        }
        // Zero is not a scale. Treating it as one would make every timestamp in the file zero.
        let declared = reader.read_uint(child)?;
        if declared != 0 {
            scale = declared;
        }
        Ok(Walk::Stop)
    })?;
    Ok(scale)
}

/// Read every bitmap subtitle track, and the video dimensions the subtitle plane matches.
fn read_tracks<R: Read + Seek>(
    reader: &mut EbmlReader<R>,
    tracks: &ElementHeader,
) -> Result<(Vec<Track>, (u32, u32))> {
    let mut found = Vec::new();
    let mut plane = (0u32, 0u32);
    let mut index = 0u32;

    reader.children(tracks, |reader, entry| {
        if entry.id == TRACK_ENTRY {
            if let Some(track) = read_track_entry(reader, entry, &mut index)? {
                found.push(track);
            } else if let Some(video) = video_plane(reader, entry)? {
                plane = video;
            }
        }
        Ok(Walk::Continue)
    })?;
    Ok((found, plane))
}

/// Read one `TrackEntry`, returning it only if it is a bitmap subtitle track.
fn read_track_entry<R: Read + Seek>(
    reader: &mut EbmlReader<R>,
    entry: &ElementHeader,
    index: &mut u32,
) -> Result<Option<Track>> {
    let mut number = 0u64;
    let mut track_type = 0u64;
    let mut codec_id = String::new();
    let mut language = None;
    let mut name = None;
    let mut compression = Compression::None;
    let mut codec_private = Vec::new();

    reader.children(entry, |reader, child| {
        match child.id {
            TRACK_NUMBER => number = reader.read_uint(child)?,
            TRACK_TYPE => track_type = reader.read_uint(child)?,
            CODEC_ID => codec_id = reader.read_string(child)?,
            LANGUAGE => language = Some(reader.read_string(child)?),
            NAME => name = Some(reader.read_string(child)?),
            CONTENT_ENCODINGS => compression = read_compression(reader, child)?,
            CODEC_PRIVATE => {
                codec_private = reader.read_exact(usize::try_from(child.size).unwrap_or(0))?;
            }
            _ => {}
        }
        Ok(Walk::Continue)
    })?;

    if track_type != TRACK_TYPE_SUBTITLE {
        return Ok(None);
    }
    let Some(codec) = bitmap_codec(&codec_id) else {
        return Ok(None);
    };

    let info = StreamInfo {
        index: *index,
        codec,
        // "und" is Matroska's default and carries no more information than absence.
        language: language.filter(|l| l != "und"),
        title: name,
        plane_width: 0,
        plane_height: 0,
        codec_private,
    };
    *index += 1;
    Ok(Some(Track { number, info, compression }))
}

/// Read a `ContentEncodings` subtree into a [`Compression`].
///
/// Matroska nests this three deep — `ContentEncodings > ContentEncoding > ContentCompression` —
/// and `ContentCompAlgo` is frequently omitted. Its default is 0, meaning zlib, so an absent
/// element means compressed rather than uncompressed. Reading it the other way round is exactly
/// the mistake that made a first survey of the library report zero compressed tracks.
///
/// Written as a plain recursive descent. An earlier hand-rolled stack version pushed its resume
/// offset as `cursor + size`, forgetting that `size` excludes the element header, so it landed
/// mid-element and descended forever — the file never finished opening.
fn read_compression<R: Read + Seek>(
    reader: &mut EbmlReader<R>,
    encodings: &ElementHeader,
) -> Result<Compression> {
    fn walk<R: Read + Seek>(
        reader: &mut EbmlReader<R>,
        start: u64,
        end: u64,
        found: &mut Compression,
        depth: u32,
    ) -> Result<()> {
        // ContentEncodings > ContentEncoding > ContentCompression is as deep as this goes.
        if depth > 4 {
            return Ok(());
        }
        reader.children_until(start, end, |reader, child| {
            let next = child.body_end().unwrap_or(child.body_start);
            match child.id {
                CONTENT_ENCODING => walk(reader, child.body_start, next, found, depth + 1)?,
                CONTENT_COMPRESSION => {
                    // Present but silent about its algorithm means zlib.
                    if *found == Compression::None {
                        *found = Compression::Zlib;
                    }
                    walk(reader, child.body_start, next, found, depth + 1)?;
                }
                CONTENT_COMP_ALGO => {
                    *found = match reader.read_uint(child)? {
                        0 => Compression::Zlib,
                        3 => Compression::HeaderStrip(Vec::new()),
                        other => {
                            return Err(Error::Demux(format!(
                                "{}: unsupported content compression algorithm {other}",
                                reader.path().display()
                            )));
                        }
                    };
                }
                CONTENT_COMP_SETTINGS => {
                    let bytes = reader.read_exact(usize::try_from(child.size).unwrap_or(0))?;
                    *found = Compression::HeaderStrip(bytes);
                }
                _ => {}
            }
            Ok(Walk::Continue)
        })
    }

    let mut found = Compression::None;
    let end = encodings.body_end().unwrap_or(encodings.body_start);
    walk(reader, encodings.body_start, end, &mut found, 0)?;
    Ok(found)
}

/// Pixel dimensions of a `TrackEntry`, if it is the video track.
fn video_plane<R: Read + Seek>(
    reader: &mut EbmlReader<R>,
    entry: &ElementHeader,
) -> Result<Option<(u32, u32)>> {
    let mut is_video = false;
    let mut plane = None;

    reader.children(entry, |reader, child| {
        if child.id == TRACK_TYPE {
            is_video = reader.read_uint(child)? == TRACK_TYPE_VIDEO;
        } else if child.id == VIDEO {
            plane = read_video_dimensions(reader, child)?;
        }
        Ok(Walk::Continue)
    })?;
    Ok(if is_video { plane } else { None })
}

/// Read `PixelWidth` and `PixelHeight` out of a `Video` element.
fn read_video_dimensions<R: Read + Seek>(
    reader: &mut EbmlReader<R>,
    video: &ElementHeader,
) -> Result<Option<(u32, u32)>> {
    let (mut width, mut height) = (0u32, 0u32);

    reader.children(video, |reader, child| {
        match child.id {
            PIXEL_WIDTH => width = u32::try_from(reader.read_uint(child)?).unwrap_or(0),
            PIXEL_HEIGHT => height = u32::try_from(reader.read_uint(child)?).unwrap_or(0),
            _ => {}
        }
        Ok(Walk::Continue)
    })?;
    Ok((width != 0 && height != 0).then_some((width, height)))
}

impl<R: Read + Seek> SubtitleSource for MatroskaReader<R> {
    fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    fn select(&mut self, index: u32) -> Result<()> {
        let track = self
            .tracks
            .iter()
            .find(|t| t.info.index == index)
            .ok_or_else(|| Error::Demux(format!("no subtitle stream with index {index}")))?;

        let number = track.number;
        self.selected = Some((number, track.compression.clone()));
        self.pending.clear();
        self.cursor = self.clusters_start;
        self.inside_cluster = false;
        self.cluster_timestamp = 0;
        self.indexed_blocks = 0;
        self.walked_from = None;
        self.skip_through = None;

        let entries = self.entries_for(number)?;
        self.indexed = entries.as_ref().map(|_| ()).map_err(|why| *why);
        self.mode = match entries {
            Ok(points) => Mode::Indexed { points, next: 0, cluster: None },
            Err(_) => Mode::Walk,
        };
        Ok(())
    }

    fn access(&self) -> Access {
        match (self.indexed, &self.random) {
            (Ok(()), Some(random)) => Access::Indexed {
                blocks: self.indexed_blocks,
                reads: random.reads(),
                bytes: random.bytes(),
                walked_from: self.walked_from,
            },
            (Err(why), _) => Access::Sequential { why },
            (Ok(()), None) => Access::Sequential { why: "no handle to read the index through" },
        }
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        if self.selected.is_none() {
            let first = self.streams.first().map(|s| s.index);
            match first {
                Some(index) => self.select(index)?,
                None => return Ok(None),
            }
        }
        self.fill()?;
        Ok(self.pending.pop_front())
    }
}

#[cfg(test)]
mod tests;
