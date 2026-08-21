//! End-to-end cover for the two stages that are complete: reading a `.sup` file and decoding the
//! PGS inside it.
//!
//! The unit tests in `subtrackt-decode` feed the decoder packets directly. This one goes through
//! the demuxer as the pipeline does, so a change to the packet shape either reader emits shows up
//! as a failure here rather than as a mystery further downstream.

use std::path::PathBuf;

use subtrackt_decode::pgs::rle;

/// One `.sup` segment: `PG` magic, PTS, DTS, type, length, body.
fn sup_segment(kind: u8, pts: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::from(*b"PG");
    out.extend_from_slice(&pts.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.push(kind);
    out.extend_from_slice(&u16::try_from(body.len()).unwrap().to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// A composition placing `objects` as `(id, x, y)`, or a clear when empty.
fn composition(objects: &[(u16, u16, u16)]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&1920u16.to_be_bytes());
    b.extend_from_slice(&1080u16.to_be_bytes());
    b.push(0x10);
    b.extend_from_slice(&0u16.to_be_bytes());
    b.push(0x80);
    b.push(0x00);
    b.push(0x00);
    b.push(u8::try_from(objects.len()).unwrap());
    for (id, x, y) in objects {
        b.extend_from_slice(&id.to_be_bytes());
        b.push(0);
        b.push(0);
        b.extend_from_slice(&x.to_be_bytes());
        b.extend_from_slice(&y.to_be_bytes());
    }
    b
}

/// A palette with one opaque white entry and one transparent black one.
fn palette() -> Vec<u8> {
    let mut b = vec![0, 0];
    b.extend_from_slice(&[1, 235, 128, 128, 255]);
    b.extend_from_slice(&[0, 16, 128, 128, 0]);
    b
}

/// A complete object definition carrying `pixels`.
fn object(id: u16, width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let data = rle::encode(pixels, width, height);
    let mut b = Vec::new();
    b.extend_from_slice(&id.to_be_bytes());
    b.push(0);
    b.push(0xC0);
    b.extend_from_slice(&u32::try_from(data.len() + 4).unwrap().to_be_bytes()[1..]);
    b.extend_from_slice(&u16::try_from(width).unwrap().to_be_bytes());
    b.extend_from_slice(&u16::try_from(height).unwrap().to_be_bytes());
    b.extend_from_slice(&data);
    b
}

/// A glyph-ish bitmap: two vertical strokes and two horizontal bars.
fn glyph(width: u32, height: u32) -> Vec<u8> {
    (0..height)
        .flat_map(|r| (0..width).map(move |c| (r, c)))
        .map(|(r, c)| u8::from(matches!(c, 2 | 3 | 8 | 9) || r == 2 || r == height - 3))
        .collect()
}

/// Write a `.sup` holding `cues` as `(start, end)` tick pairs, and return its path.
fn write_sup(name: &str, cues: &[(u32, u32)], width: u32, height: u32) -> PathBuf {
    let pixels = glyph(width, height);
    let mut file = Vec::new();

    for (start, end) in cues {
        file.extend_from_slice(&sup_segment(0x16, *start, &composition(&[(1, 400, 900)])));
        file.extend_from_slice(&sup_segment(0x14, *start, &palette()));
        file.extend_from_slice(&sup_segment(0x15, *start, &object(1, width, height, &pixels)));
        file.extend_from_slice(&sup_segment(0x80, *start, &[]));

        file.extend_from_slice(&sup_segment(0x16, *end, &composition(&[])));
        file.extend_from_slice(&sup_segment(0x80, *end, &[]));
    }

    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, &file).unwrap();
    path
}

/// Read every packet from a `.sup` through the decoder, as the pipeline does.
fn decode_all(path: &PathBuf) -> Vec<subtrackt_core::SubtitleImage> {
    let mut source = subtrackt_demux::open(path).unwrap();
    let stream = source.streams()[0].clone();
    let mut decoder = subtrackt_decode::decoder_for(stream.codec.ffmpeg_name()).unwrap();

    let mut images = Vec::new();
    while let Some(packet) = source.next_packet().unwrap() {
        images.extend(decoder.push(packet.pts, &packet.payload).unwrap());
    }
    images.extend(decoder.finish().unwrap());
    images
}

#[test]
fn a_sup_file_decodes_to_timed_and_placed_bitmaps() {
    let path = write_sup(
        "subtrackt_e2e_basic.sup",
        &[(90_000, 180_000), (270_000, 360_000)],
        12,
        16,
    );
    let images = decode_all(&path);

    assert_eq!(images.len(), 2, "two display sets, two images");

    assert_eq!(images[0].span.start.ticks(), 90_000);
    assert_eq!(images[0].span.end.ticks(), 180_000);
    assert_eq!(images[1].span.start.ticks(), 270_000);
    assert_eq!(images[1].span.end.ticks(), 360_000);

    for image in &images {
        assert_eq!(image.position, subtrackt_core::Rect::new(400, 900, 12, 16));
        assert_eq!(image.bitmap.width(), 12);
        assert_eq!(image.bitmap.height(), 16);
        assert_eq!(image.bitmap.pixels(), glyph(12, 16).as_slice());
        assert!(!image.forced);
    }

    std::fs::remove_file(&path).ok();
}

#[test]
fn the_decoded_palette_makes_the_glyph_opaque_and_the_background_transparent() {
    let path = write_sup("subtrackt_e2e_palette.sup", &[(0, 90_000)], 12, 16);
    let images = decode_all(&path);

    let palette = &images[0].palette;
    assert_eq!(palette.get(1).alpha, 255, "the glyph must survive alpha thresholding");
    assert_eq!(palette.get(0).alpha, 0, "the background must not");
    assert_eq!(palette.get(1).to_rgba().luma(), 255, "and the glyph must read as white");

    std::fs::remove_file(&path).ok();
}

#[test]
fn binarizing_a_decoded_image_recovers_the_glyph_shape() {
    // The first stage downstream of decode. It is the cheapest possible check that the decoded
    // bitmap and palette agree with each other, ahead of segmentation landing in #5.
    let path = write_sup("subtrackt_e2e_binarize.sup", &[(0, 90_000)], 12, 16);
    let images = decode_all(&path);

    let mask = subtrackt_glyph::Binarizer::default().mask(&images[0]);
    assert_eq!(mask.width(), 12);
    // Sum rather than count: the fixture is 0/1 valued, and counting the obvious way trips
    // clippy::naive_bytecount, whose suggested fix is a crate this workspace will not take.
    let expected: usize = glyph(12, 16).into_iter().map(usize::from).sum();
    assert_eq!(mask.foreground_count(), expected);
    assert!(mask.get(2, 5), "a stroke pixel is foreground");
    assert!(!mask.get(0, 5), "a background pixel is not");

    std::fs::remove_file(&path).ok();
}

#[test]
fn a_wide_object_round_trips_through_the_long_form_run_encoding() {
    // 1920px lines exercise the 14-bit run length, which a 12px fixture never reaches.
    let path = write_sup("subtrackt_e2e_wide.sup", &[(0, 90_000)], 1920, 4);
    let images = decode_all(&path);

    assert_eq!(images[0].bitmap.width(), 1920);
    assert_eq!(images[0].bitmap.pixels(), glyph(1920, 4).as_slice());

    std::fs::remove_file(&path).ok();
}
