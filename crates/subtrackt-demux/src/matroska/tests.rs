//! Tests for the Matroska reader, against files built here rather than checked in.
//!
//! Building the container in the test is the same approach the PGS work took with `rle::encode`:
//! it keeps fixtures out of the repository, makes every field under test explicit, and means a
//! failure points at a specific element rather than at an opaque blob.

use std::io::Cursor;

use super::*;

/// Encode `value` as a variable-length integer of exactly `width` bytes, marker included.
fn vint(value: u64, width: u8) -> Vec<u8> {
    let mut out = value.to_be_bytes()[8 - width as usize..].to_vec();
    out[0] |= 1 << (8 - width);
    out
}

/// Encode a length using the narrowest width that will hold it.
///
/// All-ones means "unknown size", so a value that would encode as all ones takes the next width up.
fn size_vint(len: u64) -> Vec<u8> {
    for width in 1..=8u8 {
        let max = (1u64 << (7 * u32::from(width))) - 1;
        if len < max {
            return vint(len, width);
        }
    }
    vint(len, 8)
}

/// An element: raw ID bytes, then size, then payload.
fn elem(id: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&size_vint(payload.len() as u64));
    out.extend_from_slice(payload);
    out
}

/// An unsigned-integer element, big-endian and minimally wide.
fn uint(id: &[u8], value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let start = bytes.iter().position(|b| *b != 0).unwrap_or(7);
    elem(id, &bytes[start..])
}

fn text(id: &[u8], value: &str) -> Vec<u8> {
    elem(id, value.as_bytes())
}

/// A `SimpleBlock` for `track`, `relative` ticks from its cluster.
fn simple_block(track: u64, relative: i16, flags: u8, data: &[u8]) -> Vec<u8> {
    let mut body = vint(track, 1);
    body.extend_from_slice(&relative.to_be_bytes());
    body.push(flags);
    body.extend_from_slice(data);
    elem(&[0xA3], &body)
}

/// One block: track number, ticks relative to its cluster, and payload.
type BlockSpec = (u64, i16, Vec<u8>);

/// One cluster: its timestamp, and the blocks inside it.
type ClusterSpec = (u64, Vec<BlockSpec>);

/// How a track should be declared in the test file.
struct TrackSpec {
    number: u64,
    kind: u64,
    codec: &'static str,
    language: Option<&'static str>,
    title: Option<&'static str>,
}

impl TrackSpec {
    fn subtitle(number: u64, codec: &'static str) -> Self {
        Self {
            number,
            kind: TRACK_TYPE_SUBTITLE,
            codec,
            language: None,
            title: None,
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut body = uint(&[0xD7], self.number);
        body.extend_from_slice(&uint(&[0x83], self.kind));
        body.extend_from_slice(&text(&[0x86], self.codec));
        if let Some(language) = self.language {
            body.extend_from_slice(&text(&[0x22, 0xB5, 0x9C], language));
        }
        if let Some(title) = self.title {
            body.extend_from_slice(&text(&[0x53, 0x6E], title));
        }
        if self.kind == TRACK_TYPE_VIDEO {
            let mut video = uint(&[0xB0], 1920);
            video.extend_from_slice(&uint(&[0xBA], 1080));
            body.extend_from_slice(&elem(&[0xE0], &video));
        }
        elem(&[0xAE], &body)
    }
}

/// Build a Matroska file holding `tracks` and `clusters`.
///
/// Each cluster is a timestamp and a list of `(track, relative, payload)` blocks.
fn build(tracks: &[TrackSpec], clusters: &[ClusterSpec], timestamp_scale: u64) -> Vec<u8> {
    let mut file = elem(&[0x1A, 0x45, 0xDF, 0xA3], &elem(&[0x42, 0x82], b"matroska"));

    let info = elem(&[0x15, 0x49, 0xA9, 0x66], &uint(&[0x2A, 0xD7, 0xB1], timestamp_scale));

    let mut track_bodies = Vec::new();
    for spec in tracks {
        track_bodies.extend_from_slice(&spec.encode());
    }
    let tracks_element = elem(&[0x16, 0x54, 0xAE, 0x6B], &track_bodies);

    let mut segment_body = info;
    segment_body.extend_from_slice(&tracks_element);

    for (timestamp, blocks) in clusters {
        let mut cluster = uint(&[0xE7], *timestamp);
        for (track, relative, payload) in blocks {
            cluster.extend_from_slice(&simple_block(*track, *relative, 0x00, payload));
        }
        segment_body.extend_from_slice(&elem(&[0x1F, 0x43, 0xB6, 0x75], &cluster));
    }

    file.extend_from_slice(&elem(&[0x18, 0x53, 0x80, 0x67], &segment_body));
    file
}

fn reader(bytes: Vec<u8>) -> Result<MatroskaReader<Cursor<Vec<u8>>>> {
    MatroskaReader::from_reader(Cursor::new(bytes), PathBuf::from("test.mkv"))
}

/// The error side of `reader`. `MatroskaReader` holds a reader and is not `Debug`, so `unwrap_err`
/// is unavailable.
fn reader_err(bytes: Vec<u8>) -> Error {
    match reader(bytes) {
        Err(err) => err,
        Ok(_) => panic!("expected the file to be rejected"),
    }
}

/// A file with one video track and one PGS track, carrying `clusters`.
fn pgs_file(clusters: &[ClusterSpec]) -> Vec<u8> {
    let video = TrackSpec {
        number: 1,
        kind: TRACK_TYPE_VIDEO,
        codec: "V_MPEGH/ISO/HEVC",
        language: None,
        title: None,
    };
    let subs = TrackSpec {
        language: Some("eng"),
        title: Some("Full"),
        ..TrackSpec::subtitle(2, "S_HDMV/PGS")
    };
    build(&[video, subs], clusters, DEFAULT_TIMESTAMP_SCALE)
}

#[test]
fn a_pgs_track_is_found_with_its_metadata() {
    let r = reader(pgs_file(&[])).unwrap();
    let streams = r.streams();

    assert_eq!(streams.len(), 1, "the video track is not a subtitle stream");
    assert_eq!(streams[0].codec, BitmapCodec::Pgs);
    assert_eq!(streams[0].language.as_deref(), Some("eng"));
    assert_eq!(streams[0].title.as_deref(), Some("Full"));
}

#[test]
fn the_subtitle_plane_comes_from_the_video_track() {
    // PGS track headers do not carry the plane size, and the decoder needs it to place cues.
    let r = reader(pgs_file(&[])).unwrap();
    assert_eq!((r.streams()[0].plane_width, r.streams()[0].plane_height), (1920, 1080));
}

#[test]
fn blocks_come_back_as_packets_with_ninety_kilohertz_timestamps() {
    // Cluster at 1000ms, block at +500ms, so 1.5s = 135_000 ticks.
    let file = pgs_file(&[(1_000, vec![(2, 500, vec![0xAA, 0xBB])])]);
    let mut r = reader(file).unwrap();
    r.select(0).unwrap();

    let packet = r.next_packet().unwrap().unwrap();
    assert_eq!(packet.pts, 135_000);
    assert_eq!(packet.payload, vec![0xAA, 0xBB]);
    assert!(r.next_packet().unwrap().is_none(), "the file holds one block");
}

#[test]
fn blocks_on_other_tracks_are_skipped() {
    let file = pgs_file(&[(
        0,
        vec![
            (1, 0, vec![0xFF; 8]),
            (2, 0, vec![0x01]),
            (1, 10, vec![0xFF; 8]),
        ],
    )]);
    let mut r = reader(file).unwrap();
    r.select(0).unwrap();

    let packet = r.next_packet().unwrap().unwrap();
    assert_eq!(
        packet.payload,
        vec![0x01],
        "only the subtitle track's block comes through"
    );
    assert!(r.next_packet().unwrap().is_none());
}

#[test]
fn packets_arrive_in_order_across_several_clusters() {
    let file = pgs_file(&[
        (0, vec![(2, 0, vec![1])]),
        (1_000, vec![(2, 0, vec![2])]),
        (2_000, vec![(2, 250, vec![3])]),
    ]);
    let mut r = reader(file).unwrap();
    r.select(0).unwrap();

    let mut seen = Vec::new();
    while let Some(packet) = r.next_packet().unwrap() {
        seen.push((packet.pts, packet.payload[0]));
    }
    assert_eq!(seen, vec![(0, 1), (90_000, 2), (202_500, 3)]);
}

#[test]
fn the_timestamp_scale_is_honoured() {
    // A scale of 100us instead of the usual 1ms: the same tick count is a tenth of the time.
    let video = TrackSpec {
        number: 1,
        kind: TRACK_TYPE_VIDEO,
        codec: "V_MPEGH/ISO/HEVC",
        language: None,
        title: None,
    };
    let subs = TrackSpec::subtitle(2, "S_HDMV/PGS");
    let file = build(&[video, subs], &[(1_000, vec![(2, 0, vec![1])])], 100_000);

    let mut r = reader(file).unwrap();
    r.select(0).unwrap();
    assert_eq!(r.next_packet().unwrap().unwrap().pts, 9_000, "1000 * 100us = 100ms");
}

#[test]
fn several_subtitle_tracks_are_selectable_independently() {
    let video = TrackSpec {
        number: 1,
        kind: TRACK_TYPE_VIDEO,
        codec: "V_MPEGH/ISO/HEVC",
        language: None,
        title: None,
    };
    let first = TrackSpec { language: Some("eng"), ..TrackSpec::subtitle(2, "S_HDMV/PGS") };
    let second = TrackSpec { language: Some("fra"), ..TrackSpec::subtitle(3, "S_HDMV/PGS") };
    let file = build(
        &[video, first, second],
        &[(0, vec![(2, 0, vec![0xE0]), (3, 0, vec![0xA0])])],
        DEFAULT_TIMESTAMP_SCALE,
    );

    let mut r = reader(file).unwrap();
    assert_eq!(r.streams().len(), 2);
    assert_eq!(r.streams()[1].language.as_deref(), Some("fra"));

    r.select(1).unwrap();
    assert_eq!(r.next_packet().unwrap().unwrap().payload, vec![0xA0]);

    // Selecting rewinds, so the same reader can be reused for another track.
    r.select(0).unwrap();
    assert_eq!(r.next_packet().unwrap().unwrap().payload, vec![0xE0]);
}

#[test]
fn selecting_a_stream_that_does_not_exist_is_rejected() {
    let mut r = reader(pgs_file(&[])).unwrap();
    assert!(r.select(9).is_err());
}

#[test]
fn not_selecting_anything_reads_the_first_stream() {
    let mut r = reader(pgs_file(&[(0, vec![(2, 0, vec![7])])])).unwrap();
    assert_eq!(r.next_packet().unwrap().unwrap().payload, vec![7]);
}

#[test]
fn a_vobsub_track_is_recognised_too() {
    let file = build(&[TrackSpec::subtitle(1, "S_VOBSUB")], &[], DEFAULT_TIMESTAMP_SCALE);
    let r = reader(file).unwrap();
    assert_eq!(r.streams()[0].codec, BitmapCodec::VobSub);
}

#[test]
fn text_subtitle_tracks_are_ignored() {
    // SRT and ASS are already handled upstream; this tool is only for the bitmap codecs.
    let file = build(&[TrackSpec::subtitle(1, "S_TEXT/UTF8")], &[], DEFAULT_TIMESTAMP_SCALE);
    let err = reader_err(file);
    assert!(matches!(err, Error::Demux(_)), "got {err:?}");
}

#[test]
fn a_file_that_is_not_matroska_is_rejected() {
    let err = reader_err(b"this is not a matroska file at all".to_vec());
    assert!(matches!(err, Error::Demux(_)), "got {err:?}");
}

#[test]
fn an_undefined_language_reads_as_absent_rather_than_as_the_literal_und() {
    let file = build(
        &[TrackSpec { language: Some("und"), ..TrackSpec::subtitle(1, "S_HDMV/PGS") }],
        &[],
        DEFAULT_TIMESTAMP_SCALE,
    );
    let r = reader(file).unwrap();
    assert_eq!(
        r.streams()[0].language,
        None,
        "und carries no more information than absence"
    );
}

#[test]
fn a_laced_block_is_refused_loudly_rather_than_decoded_wrongly() {
    // Subtitle tracks do not use lacing. Rather than implement it speculatively, the reader
    // refuses — silently taking the first frame of a lace would drop cues without a trace.
    let video = TrackSpec {
        number: 1,
        kind: TRACK_TYPE_VIDEO,
        codec: "V_MPEGH/ISO/HEVC",
        language: None,
        title: None,
    };
    let subs = TrackSpec::subtitle(2, "S_HDMV/PGS");

    let mut segment_body = elem(
        &[0x15, 0x49, 0xA9, 0x66],
        &uint(&[0x2A, 0xD7, 0xB1], DEFAULT_TIMESTAMP_SCALE),
    );
    let mut bodies = video.encode();
    bodies.extend_from_slice(&subs.encode());
    segment_body.extend_from_slice(&elem(&[0x16, 0x54, 0xAE, 0x6B], &bodies));

    let mut cluster = uint(&[0xE7], 0);
    cluster.extend_from_slice(&simple_block(2, 0, 0x02, &[1, 2, 3])); // Xiph lacing bit
    segment_body.extend_from_slice(&elem(&[0x1F, 0x43, 0xB6, 0x75], &cluster));

    let mut file = elem(&[0x1A, 0x45, 0xDF, 0xA3], &elem(&[0x42, 0x82], b"matroska"));
    file.extend_from_slice(&elem(&[0x18, 0x53, 0x80, 0x67], &segment_body));

    let mut r = reader(file).unwrap();
    r.select(0).unwrap();
    let err = r.next_packet().unwrap_err();
    assert!(matches!(err, Error::Demux(_)), "got {err:?}");
}

// --- The index path -------------------------------------------------------------------------
//
// Every test here reads one file both ways and holds the index to the walk's packets. The walk is
// the reference because it is the reader every published figure in this project came from.

/// One block in an indexed fixture.
#[derive(Clone)]
enum Blk {
    /// A `SimpleBlock`: track, ticks from its cluster, payload.
    Simple(u64, i16, Vec<u8>),
    /// A `BlockGroup` carrying a `BlockDuration` after its `Block`, which is how mkvmerge writes
    /// subtitles.
    Group(u64, i16, Vec<u8>),
}

/// One index entry, before it is written.
#[derive(Clone, Debug)]
struct Entry {
    time: u64,
    track: u64,
    /// Offset of the cluster's header from the segment body.
    cluster: u64,
    relative: Option<u64>,
}

/// Where the `Cues` element goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    /// After the last cluster, found through the `SeekHead`, which is where mkvmerge puts it.
    Tail,
    /// Before the first cluster, with no `SeekHead` at all.
    Front,
    /// After the last cluster, found through a second `SeekHead` that the first points at.
    TailBehindSecondSeekHead,
    /// Nowhere: the file has no index.
    Absent,
}

/// Element IDs a `SeekHead` names, as the bytes it stores them in.
const CUES_ID: [u8; 4] = [0x1C, 0x53, 0xBB, 0x6B];
const SEEK_HEAD_ID: [u8; 4] = [0x11, 0x4D, 0x9B, 0x74];

/// An unsigned-integer element eight bytes wide, so an element's size does not depend on its
/// value and positions can be computed before the values are known.
fn uint8(id: &[u8], value: u64) -> Vec<u8> {
    elem(id, &value.to_be_bytes())
}

fn block_body(track: u64, relative: i16, data: &[u8]) -> Vec<u8> {
    let mut body = vint(track, 1);
    body.extend_from_slice(&relative.to_be_bytes());
    body.push(0x80);
    body.extend_from_slice(data);
    body
}

fn cues_element(entries: &[Entry], segment_offset: u64) -> Vec<u8> {
    let mut body = Vec::new();
    for entry in entries {
        let mut positions = uint8(&[0xF7], entry.track);
        positions.extend(uint8(&[0xF1], segment_offset + entry.cluster));
        if let Some(relative) = entry.relative {
            positions.extend(uint8(&[0xF0], relative));
        }
        let mut point = uint8(&[0xB3], entry.time);
        point.extend(elem(&[0xB7], &positions));
        body.extend(elem(&[0xBB], &point));
    }
    elem(&[0x1C, 0x53, 0xBB, 0x6B], &body)
}

/// A file with a video track and two PGS tracks, numbered 2 and 3, with the blocks of `indexed`
/// tracks entered in its `Cues`. `edit` sees the entries before they are written.
fn indexed_file(
    clusters: &[(u64, Vec<Blk>)],
    indexed: &[u64],
    place: Place,
    edit: impl FnOnce(&mut Vec<Entry>),
) -> Vec<u8> {
    let video = TrackSpec {
        number: 1,
        kind: TRACK_TYPE_VIDEO,
        codec: "V_MPEGH/ISO/HEVC",
        language: None,
        title: None,
    };
    let first = TrackSpec { language: Some("eng"), ..TrackSpec::subtitle(2, "S_HDMV/PGS") };
    let second = TrackSpec { language: Some("fra"), ..TrackSpec::subtitle(3, "S_HDMV/PGS") };

    let info = elem(
        &[0x15, 0x49, 0xA9, 0x66],
        &uint(&[0x2A, 0xD7, 0xB1], DEFAULT_TIMESTAMP_SCALE),
    );
    let mut track_bodies = video.encode();
    track_bodies.extend(first.encode());
    track_bodies.extend(second.encode());
    let tracks = elem(&[0x16, 0x54, 0xAE, 0x6B], &track_bodies);

    let mut blob = Vec::new();
    let mut entries = Vec::new();
    for (timestamp, blocks) in clusters {
        let mut body = uint(&[0xE7], *timestamp);
        for block in blocks {
            let at = body.len() as u64;
            let (track, relative) = match block {
                Blk::Simple(track, relative, data) => {
                    body.extend(elem(&[0xA3], &block_body(*track, *relative, data)));
                    (*track, *relative)
                }
                Blk::Group(track, relative, data) => {
                    let mut group = elem(&[0xA1], &block_body(*track, *relative, data));
                    group.extend(uint(&[0x9B], 2_000));
                    body.extend(elem(&[0xA0], &group));
                    (*track, *relative)
                }
            };
            if indexed.contains(&track) {
                entries.push(Entry {
                    time: timestamp.saturating_add_signed(i64::from(relative)),
                    track,
                    cluster: blob.len() as u64,
                    relative: Some(at),
                });
            }
        }
        blob.extend(elem(&[0x1F, 0x43, 0xB6, 0x75], &body));
    }
    edit(&mut entries);

    let seek_head = |target: [u8; 4], position: u64| {
        let mut seek = elem(&[0x53, 0xAB], &target);
        seek.extend(uint8(&[0x53, 0xAC], position));
        elem(&SEEK_HEAD_ID, &elem(&[0x4D, 0xBB], &seek))
    };
    let head_len = (info.len() + tracks.len()) as u64;

    let mut segment = Vec::new();
    match place {
        Place::Tail => {
            let seek_len = seek_head(CUES_ID, 0).len() as u64;
            let clusters_at = seek_len + head_len;
            segment.extend(seek_head(CUES_ID, clusters_at + blob.len() as u64));
            segment.extend(&info);
            segment.extend(&tracks);
            segment.extend(&blob);
            segment.extend(cues_element(&entries, clusters_at));
        }
        Place::TailBehindSecondSeekHead => {
            let seek_len = seek_head(CUES_ID, 0).len() as u64;
            let clusters_at = seek_len + head_len;
            let cues_at = clusters_at + blob.len() as u64;
            let cues = cues_element(&entries, clusters_at);
            segment.extend(seek_head(SEEK_HEAD_ID, cues_at + cues.len() as u64));
            segment.extend(&info);
            segment.extend(&tracks);
            segment.extend(&blob);
            segment.extend(&cues);
            segment.extend(seek_head(CUES_ID, cues_at));
        }
        Place::Front => {
            // Every value in the index is eight bytes wide, so its length is known before the
            // positions it holds are.
            let cues_len = cues_element(&entries, 0).len() as u64;
            segment.extend(&info);
            segment.extend(&tracks);
            segment.extend(cues_element(&entries, head_len + cues_len));
            segment.extend(&blob);
        }
        Place::Absent => {
            segment.extend(&info);
            segment.extend(&tracks);
            segment.extend(&blob);
        }
    }

    let mut file = elem(&[0x1A, 0x45, 0xDF, 0xA3], &elem(&[0x42, 0x82], b"matroska"));
    file.extend(elem(&[0x18, 0x53, 0x80, 0x67], &segment));
    file
}

/// A film in miniature: every cluster opens with a large video frame, and the subtitle track's
/// display sets and erases sit among them. Track 3 carries a block in every other cluster.
fn film() -> Vec<(u64, Vec<Blk>)> {
    (0..24u64)
        .map(|n| {
            let mut blocks = vec![Blk::Simple(1, 0, vec![0x11; 150_000])];
            if n % 3 == 1 {
                blocks.push(Blk::Group(2, 120, vec![0x50, u8::try_from(n).unwrap(), 0x01]));
                blocks.push(Blk::Simple(1, 200, vec![0x22; 30_000]));
                blocks.push(Blk::Group(2, 700, vec![0x50, u8::try_from(n).unwrap(), 0x00]));
            }
            if n % 2 == 0 {
                blocks.push(Blk::Simple(3, 400, vec![0x60, u8::try_from(n).unwrap()]));
            }
            (n * 1_000, blocks)
        })
        .collect()
}

/// Read stream `index` to the end.
fn drain<R: Read + Seek>(r: &mut MatroskaReader<R>, index: u32) -> Vec<Packet> {
    r.select(index).unwrap();
    let mut packets = Vec::new();
    while let Some(packet) = r.next_packet().unwrap() {
        packets.push(packet);
    }
    packets
}

/// The file read by walking, which is how every reader was built before #258.
fn walked(bytes: &[u8], index: u32) -> Vec<Packet> {
    drain(&mut reader(bytes.to_vec()).unwrap(), index)
}

/// A reader that can follow the index.
fn with_index(bytes: &[u8]) -> MatroskaReader<Cursor<Vec<u8>>> {
    reader(bytes.to_vec())
        .unwrap()
        .with_random_access(Cursor::new(bytes.to_vec()))
        .unwrap()
}

#[test]
fn the_index_yields_exactly_the_packets_the_walk_does() {
    let bytes = indexed_file(&film(), &[2, 3], Place::Tail, |_| {});
    for stream in 0..2 {
        let mut r = with_index(&bytes);
        let indexed = drain(&mut r, stream);
        assert!(!indexed.is_empty());
        assert_eq!(indexed, walked(&bytes, stream), "stream {stream}");
        assert!(
            matches!(r.access(), Access::Indexed { walked_from: None, .. }),
            "stream {stream} was read {:?}",
            r.access()
        );
    }
}

#[test]
fn the_index_reads_a_small_fraction_of_the_file() {
    // The point of it. 24 clusters of video frames around 16 subtitle blocks: the walk passes over
    // every byte, and the index reads its own element, one cluster head per cluster that holds a
    // wanted block, and one window per block — with the window, not the file, as the unit.
    let bytes = indexed_file(&film(), &[2, 3], Place::Tail, |_| {});
    let mut r = with_index(&bytes);
    let packets = drain(&mut r, 0);
    let Access::Indexed { blocks, reads, bytes: read, .. } = r.access() else {
        panic!("read {:?}", r.access());
    };
    assert_eq!(blocks, packets.len() as u64);
    // The index, then a head and a window for each of eight clusters: both blocks of a cluster
    // fall inside one window.
    assert_eq!(reads, 1 + 8 * 2, "read {reads} times");
    assert!(read < bytes.len() as u64 / 5, "read {read} of {} bytes", bytes.len());
}

#[test]
fn an_index_before_the_first_cluster_is_found_without_a_seek_head() {
    let bytes = indexed_file(&film(), &[2, 3], Place::Front, |_| {});
    let mut r = with_index(&bytes);
    assert_eq!(drain(&mut r, 1), walked(&bytes, 1));
    assert!(matches!(r.access(), Access::Indexed { .. }), "read {:?}", r.access());
}

#[test]
fn an_index_behind_a_second_seek_head_is_followed_there() {
    // mkvmerge writes a second SeekHead at the tail when the front one has no room for `Cues`.
    let bytes = indexed_file(&film(), &[2, 3], Place::TailBehindSecondSeekHead, |_| {});
    let mut r = with_index(&bytes);
    assert_eq!(drain(&mut r, 0), walked(&bytes, 0));
    assert!(matches!(r.access(), Access::Indexed { .. }), "read {:?}", r.access());
}

#[test]
fn a_file_with_no_index_is_walked_and_says_why() {
    let bytes = indexed_file(&film(), &[], Place::Absent, |_| {});
    let mut r = with_index(&bytes);
    assert_eq!(drain(&mut r, 0), walked(&bytes, 0));
    assert_eq!(r.access(), Access::Sequential { why: "the file has no Cues element" });
}

#[test]
fn a_track_the_index_does_not_cover_is_walked_while_one_it_does_is_not() {
    // A muxer can index some tracks and not others. The uncovered track is read the old way; its
    // neighbour still gets the index.
    let bytes = indexed_file(&film(), &[2], Place::Tail, |_| {});
    let mut r = with_index(&bytes);
    assert_eq!(drain(&mut r, 1), walked(&bytes, 1));
    assert_eq!(
        r.access(),
        Access::Sequential { why: "the index has no entries for this track" }
    );
    assert_eq!(drain(&mut r, 0), walked(&bytes, 0));
    assert!(matches!(r.access(), Access::Indexed { .. }), "read {:?}", r.access());
}

#[test]
fn an_index_without_block_positions_is_walked_rather_than_searched() {
    let bytes = indexed_file(&film(), &[2, 3], Place::Tail, |entries| {
        for entry in entries.iter_mut() {
            entry.relative = None;
        }
    });
    let mut r = with_index(&bytes);
    assert_eq!(drain(&mut r, 0), walked(&bytes, 0));
    assert_eq!(
        r.access(),
        Access::Sequential { why: "the index gives clusters but not block positions" }
    );
}

#[test]
fn an_entry_that_leads_elsewhere_hands_the_rest_of_the_track_to_the_walk_without_losing_a_cue() {
    // The fifth entry is pointed at its cluster's video frame, which follows the cluster's
    // four-byte timestamp. Everything before it came from the index; the walk takes over at that
    // cluster, passes over what the index already yielded, and the track comes out whole —
    // neither short a packet nor carrying one twice.
    let bytes = indexed_file(&film(), &[2, 3], Place::Tail, |entries| {
        let fifth = entries.iter_mut().filter(|e| e.track == 2).nth(4).unwrap();
        fifth.relative = Some(4);
    });
    let mut r = with_index(&bytes);
    let packets = drain(&mut r, 0);
    assert_eq!(packets, walked(&bytes, 0));
    let Access::Indexed { blocks, walked_from: Some(pts), .. } = r.access() else {
        panic!("read {:?}", r.access());
    };
    assert_eq!(blocks, 4, "four blocks came from the index before the fifth failed");
    assert_eq!(pts, packets[4].pts, "the report names where the index stopped");
}

#[test]
fn an_entry_that_leads_past_the_end_of_the_file_is_walked_rather_than_trusted() {
    let bytes = indexed_file(&film(), &[2, 3], Place::Tail, |entries| {
        entries.last_mut().unwrap().relative = Some(u64::from(u32::MAX));
    });
    let mut r = with_index(&bytes);
    assert_eq!(drain(&mut r, 1), walked(&bytes, 1));
    assert!(
        matches!(r.access(), Access::Indexed { walked_from: Some(_), .. }),
        "read {:?}",
        r.access()
    );
}

#[test]
fn an_entry_pointing_at_another_tracks_block_is_not_yielded_as_this_ones() {
    // An extra English entry at a French block's position: the block there is on the wrong track,
    // and the reader has to notice rather than yield a French cue as an English one.
    let bytes = indexed_file(&film(), &[2, 3], Place::Tail, |entries| {
        let french = entries
            .iter()
            .find(|e| e.track == 3 && e.cluster > 0)
            .cloned()
            .unwrap();
        entries.push(Entry { track: 2, ..french });
    });
    let mut r = with_index(&bytes);
    assert_eq!(drain(&mut r, 0), walked(&bytes, 0));
    assert!(
        matches!(r.access(), Access::Indexed { walked_from: Some(_), .. }),
        "read {:?}",
        r.access()
    );
}

#[test]
fn an_index_missing_an_entry_loses_that_cue_and_nothing_says_so() {
    // Pinned because it is the risk the whole path carries. The walk sees every block; the index
    // sees what the muxer chose to enter, and a block it left out is not read. Nothing in the file
    // says an entry is missing, so nothing here can refuse — the protection is measuring how often
    // real muxers do this, which is #259's survey, not a check this reader could make.
    let bytes = indexed_file(&film(), &[2, 3], Place::Tail, |entries| {
        let first = entries.iter().position(|e| e.track == 2).unwrap();
        entries.remove(first);
    });
    let mut r = with_index(&bytes);
    let indexed = drain(&mut r, 0);
    let walked = walked(&bytes, 0);
    assert_eq!(indexed.len() + 1, walked.len());
    assert_eq!(indexed[..], walked[1..]);
    assert!(matches!(r.access(), Access::Indexed { walked_from: None, .. }));
}

#[test]
fn a_duplicated_entry_yields_its_block_once() {
    let bytes = indexed_file(&film(), &[2, 3], Place::Tail, |entries| {
        let copy = entries[0].clone();
        entries.insert(1, copy);
    });
    let mut r = with_index(&bytes);
    assert_eq!(drain(&mut r, 0), walked(&bytes, 0));
}

#[test]
fn switching_the_index_off_reads_the_same_file_the_old_way() {
    let bytes = indexed_file(&film(), &[2, 3], Place::Tail, |_| {});
    let mut r = with_index(&bytes);
    r.use_index(false);
    assert_eq!(drain(&mut r, 0), walked(&bytes, 0));
    assert_eq!(r.access(), Access::Sequential { why: "the index was not consulted" });
}

#[test]
fn a_reader_given_no_second_handle_walks_as_it_always_did() {
    let bytes = indexed_file(&film(), &[2, 3], Place::Tail, |_| {});
    let mut r = reader(bytes).unwrap();
    drain(&mut r, 0);
    assert_eq!(
        r.access(),
        Access::Sequential { why: "no handle to read the index through" }
    );
}

#[test]
fn an_empty_block_is_skipped_without_erroring() {
    let file = pgs_file(&[(0, vec![(2, 0, vec![]), (2, 100, vec![9])])]);
    let mut r = reader(file).unwrap();
    r.select(0).unwrap();

    // The zero-length block carries no payload; the next one still arrives.
    let mut payloads = Vec::new();
    while let Some(packet) = r.next_packet().unwrap() {
        payloads.push(packet.payload);
    }
    assert!(payloads.contains(&vec![9]));
}
