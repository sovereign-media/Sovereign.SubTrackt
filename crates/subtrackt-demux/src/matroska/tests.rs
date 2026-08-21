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
