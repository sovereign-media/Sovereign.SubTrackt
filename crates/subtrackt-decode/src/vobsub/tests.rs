//! Tests for the VOBSUB decoder, against subpictures built here.

use super::*;

/// Pack nibbles into bytes.
fn pack(nibbles: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for pair in nibbles.chunks(2) {
        out.push((pair[0] << 4) | pair.get(1).copied().unwrap_or(0));
    }
    out
}

/// Build a subpicture: header, pixel data, then one control sequence.
///
/// `area` is `(x1, y1, x2, y2)` inclusive, as the format stores it.
fn subpicture(area: (u32, u32, u32, u32), fields: (&[u8], &[u8]), stop: Option<u16>) -> Vec<u8> {
    let mut body = vec![0u8; 4];
    let top_at = body.len();
    body.extend_from_slice(fields.0);
    let bottom_at = body.len();
    body.extend_from_slice(fields.1);

    let control_at = body.len();
    let mut control = Vec::new();
    control.extend_from_slice(&0u16.to_be_bytes()); // delay
    control.extend_from_slice(&0u16.to_be_bytes()); // link, patched below

    control.push(0x01); // start display
    control.push(0x03); // palette: colours 3,2,1,0 -> entries 3,2,1,0
    control.extend_from_slice(&0x3210u16.to_be_bytes());
    control.push(0x04); // alpha: colour 0 transparent, the rest opaque
    control.extend_from_slice(&0xFFF0u16.to_be_bytes());
    control.push(0x05); // display area
    let (x1, y1, x2, y2) = area;
    control.push(u8::try_from(x1 >> 4).unwrap());
    control.push(u8::try_from(((x1 & 0x0F) << 4) | (x2 >> 8)).unwrap());
    control.push(u8::try_from(x2 & 0xFF).unwrap());
    control.push(u8::try_from(y1 >> 4).unwrap());
    control.push(u8::try_from(((y1 & 0x0F) << 4) | (y2 >> 8)).unwrap());
    control.push(u8::try_from(y2 & 0xFF).unwrap());
    control.push(0x06); // pixel data offsets
    control.extend_from_slice(&u16::try_from(top_at).unwrap().to_be_bytes());
    control.extend_from_slice(&u16::try_from(bottom_at).unwrap().to_be_bytes());
    control.push(0xFF); // end of commands

    if let Some(delay) = stop {
        // A second sequence, linked from the first, carrying the stop.
        let second_at = control_at + control.len();
        control[2..4].copy_from_slice(&u16::try_from(second_at).unwrap().to_be_bytes());

        control.extend_from_slice(&delay.to_be_bytes());
        control.extend_from_slice(&u16::try_from(second_at).unwrap().to_be_bytes());
        control.push(0x02); // stop display
        control.push(0xFF);
    } else {
        control[2..4].copy_from_slice(&u16::try_from(control_at).unwrap().to_be_bytes());
    }

    body.extend_from_slice(&control);
    let total = u16::try_from(body.len()).unwrap();
    body[0..2].copy_from_slice(&total.to_be_bytes());
    body[2..4].copy_from_slice(&u16::try_from(control_at).unwrap().to_be_bytes());
    body
}

/// Encode one run in the narrowest form that fits, as the format does.
fn run(count: u32, colour: u8) -> Vec<u8> {
    let value = (count << 2) | u32::from(colour);
    let width = if (0x4..=0xF).contains(&value) {
        1
    } else if value <= 0x3F {
        2
    } else if value <= 0xFF {
        3
    } else {
        4
    };
    (0..width)
        .rev()
        .map(|i| u8::try_from((value >> (i * 4)) & 0xF).unwrap())
        .collect()
}

/// A field of `lines` lines, each a single 4-pixel run of `colour`.
fn field(colour: u8, lines: usize) -> Vec<u8> {
    let mut nibbles = Vec::new();
    for _ in 0..lines {
        nibbles.extend(run(4, colour));
        if nibbles.len() % 2 == 1 {
            nibbles.push(0);
        }
    }
    pack(&nibbles)
}

#[test]
fn a_subpicture_decodes_to_a_placed_and_timed_image() {
    let sp = subpicture((10, 20, 13, 23), (&field(1, 2), &field(2, 2)), Some(90));
    let mut decoder = VobSubDecoder::new();

    let images = decoder.push(900_000, &sp).unwrap();
    assert_eq!(images.len(), 1);
    let image = &images[0];

    assert_eq!(image.position, Rect::new(10, 20, 4, 4));
    assert_eq!(image.bitmap.width(), 4);
    assert_eq!(image.bitmap.height(), 4);
    assert_eq!(image.span.start.ticks(), 900_000, "no start delay in this chain");
    assert_eq!(image.span.end.ticks(), 900_000 + 90 * DELAY_TO_TICKS);
    assert_eq!(decoder.subpictures(), 1);
}

#[test]
fn the_two_fields_land_on_alternate_lines() {
    let sp = subpicture((0, 0, 3, 3), (&field(1, 2), &field(2, 2)), Some(90));
    let images = VobSubDecoder::new().push(0, &sp).unwrap();
    let bitmap = &images[0].bitmap;

    assert_eq!(bitmap.get(0, 0), Some(1), "line 0 from the top field");
    assert_eq!(bitmap.get(0, 1), Some(2), "line 1 from the bottom field");
    assert_eq!(bitmap.get(0, 2), Some(1));
    assert_eq!(bitmap.get(0, 3), Some(2));
}

#[test]
fn the_palette_command_selects_from_the_sixteen_colour_table() {
    let mut decoder = VobSubDecoder::new();
    decoder
        .configure(b"palette: 000000, ff0000, 00ff00, 0000ff\n")
        .unwrap();
    assert!(decoder.has_palette());

    let sp = subpicture((0, 0, 3, 3), (&field(1, 2), &field(2, 2)), Some(90));
    let images = decoder.push(0, &sp).unwrap();
    let palette = &images[0].palette;

    // Colour 1 selects entry 1, which is pure red.
    let red = palette.get(1).to_rgba();
    assert!(red.r > 200 && red.g < 60 && red.b < 60, "got {red:?}");
    assert_eq!(palette.get(1).alpha, 255, "the alpha command made it opaque");
    assert_eq!(palette.get(0).alpha, 0, "and left colour 0 transparent");
}

#[test]
fn without_a_palette_everything_reads_transparent_rather_than_guessing() {
    // A stream with no CodecPrivate and no sidecar has colour indices and no colours. Inventing
    // some would produce a plausible bitmap that is not what the disc says.
    let sp = subpicture((0, 0, 3, 3), (&field(1, 2), &field(2, 2)), Some(90));
    let images = VobSubDecoder::new().push(0, &sp).unwrap();
    assert_eq!(images[0].palette.get(1).to_rgba().r, 0);
}

#[test]
fn a_chain_with_no_stop_command_still_yields_a_cue() {
    let sp = subpicture((0, 0, 3, 3), (&field(1, 2), &field(2, 2)), None);
    let mut decoder = VobSubDecoder::new();
    let images = decoder.push(0, &sp).unwrap();

    assert_eq!(images.len(), 1, "dropping it would lose a line of dialogue");
    assert_eq!(images[0].span.end.ticks(), UNTERMINATED_CUE_TICKS);
    assert_eq!(decoder.unterminated_cues(), 1, "the approximation must be countable");
}

#[test]
fn the_display_area_is_read_inclusively() {
    // x1..x2 and y1..y2 are inclusive bounds, so a 10..13 span is four pixels wide, not three.
    // Off by one here would crop a column off every subtitle in the library.
    let sp = subpicture((10, 20, 13, 23), (&field(1, 2), &field(2, 2)), Some(90));
    let images = VobSubDecoder::new().push(0, &sp).unwrap();
    assert_eq!(images[0].position, Rect::new(10, 20, 4, 4));
}

#[test]
fn a_truncated_subpicture_is_rejected() {
    assert!(VobSubDecoder::new().push(0, &[]).is_err());
    assert!(VobSubDecoder::new().push(0, &[0, 4]).is_err());
    // Control offset past the end of the packet.
    assert!(
        VobSubDecoder::new()
            .push(0, &[0, 8, 0xFF, 0xFF, 0, 0, 0, 0])
            .is_err()
    );
}

#[test]
fn an_unknown_control_command_is_rejected_rather_than_skipped() {
    let mut sp = subpicture((0, 0, 3, 3), (&field(1, 2), &field(2, 2)), Some(90));
    let at = sp.iter().position(|b| *b == 0x01).unwrap();
    sp[at] = 0x7E;
    let err = VobSubDecoder::new().push(1_234, &sp).unwrap_err();
    match err {
        Error::MalformedPacket { codec, pts, reason } => {
            assert_eq!(codec, "vobsub");
            assert_eq!(pts, 1_234);
            assert!(reason.contains("unknown control command"), "{reason}");
        }
        other => panic!("got {other:?}"),
    }
}

#[test]
fn a_palette_parses_out_of_idx_or_codec_private_text() {
    let text = "# comment\nsize: 720x480\npalette: 000000, ffffff, ff0000\nid: en, index: 0\n";
    let palette = parse_palette(text).unwrap();
    assert_eq!(palette.len(), 3);
    // Within rounding: the palette goes RGB to YCbCr on the way in and back on the way out.
    assert!(palette.get(1).to_rgba().r >= 250, "white stays white");
    assert!(
        palette.get(2).to_rgba().r >= 250 && palette.get(2).to_rgba().g <= 5,
        "red stays red"
    );
}

#[test]
fn text_with_no_palette_line_yields_nothing_rather_than_an_empty_palette() {
    assert!(parse_palette("size: 720x480\n").is_none());
    assert!(parse_palette("").is_none());
}

#[test]
fn configuring_with_nothing_leaves_the_decoder_alone() {
    let mut decoder = VobSubDecoder::new();
    decoder.configure(&[]).unwrap();
    assert!(!decoder.has_palette());
}

#[test]
fn the_codec_name_is_reported_for_diagnostics() {
    assert_eq!(VobSubDecoder::new().codec(), "vobsub");
}
