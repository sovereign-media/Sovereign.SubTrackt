//! PGS run-length coding.
//!
//! The encoding, in full, because it is small and the spec is awkward to find. A non-zero byte is
//! one pixel of that palette index. A zero byte introduces a run, and the byte after it selects
//! between five forms:
//!
//! | Second byte | Meaning |
//! | :--- | :--- |
//! | `00000000` | end of line |
//! | `00LLLLLL` | `L` pixels of colour 0 |
//! | `01LLLLLL LLLLLLLL` | 14-bit run of colour 0 |
//! | `10LLLLLL CCCCCCCC` | `L` pixels of colour `C` |
//! | `11LLLLLL LLLLLLLL CCCCCCCC` | 14-bit run of colour `C` |
//!
//! Lines are padded to the object width, and the object to its declared height.
//!
//! Errors here are deliberately loud. Decoding a truncated object into a partially blank bitmap
//! would produce a subtitle that reads as legitimately empty, and an empty subtitle is
//! indistinguishable from one that genuinely had no text — exactly the silent failure this project
//! exists to avoid.

use std::fmt;

/// Largest run a single RLE sequence can express, being the 14-bit length field.
const MAX_RUN: usize = 0x3FFF;

/// What can be wrong with a run-length stream.
///
/// Kept separate from [`subtrackt_core::Error`] so this module stays a pure codec: the caller is
/// the one that knows the presentation timestamp, and it attaches it when converting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RleError {
    /// A run ran past the right edge of its line.
    LineOverflow {
        /// Zero-based line the run started on.
        line: u32,
        /// Declared object width.
        width: u32,
        /// How many pixels past the edge the run would have written.
        overflow_by: u32,
    },
    /// The data ended in the middle of a run header.
    UnexpectedEnd,
    /// The stream decoded to a different number of lines than the object declared.
    LineCount {
        /// Lines actually decoded.
        decoded: u32,
        /// Lines the object header declared.
        expected: u32,
    },
}

impl fmt::Display for RleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LineOverflow { line, width, overflow_by } => {
                write!(f, "run on line {line} overflows the {width}px line by {overflow_by}px")
            }
            Self::UnexpectedEnd => f.write_str("data ended inside a run header"),
            Self::LineCount { decoded, expected } => {
                write!(f, "decoded {decoded} lines but the object declares {expected}")
            }
        }
    }
}

impl std::error::Error for RleError {}

/// Decode one object into a row-major index plane of `width * height` bytes.
///
/// # Errors
/// Returns [`RleError`] for a run that overflows its line, data that ends inside a run header, or
/// a line count that disagrees with the object header.
pub fn decode(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, RleError> {
    let mut plane = Plane::new(width, height);
    let mut cursor = 0;

    while cursor < data.len() {
        let byte = data[cursor];
        cursor += 1;

        // A non-zero byte is a single literal pixel, which is the common case inside a glyph.
        if byte != 0 {
            plane.write(byte, 1)?;
            continue;
        }

        let flag = *data.get(cursor).ok_or(RleError::UnexpectedEnd)?;
        cursor += 1;

        if flag == 0 {
            plane.end_line();
            continue;
        }

        let mut count = usize::from(flag & 0x3F);
        if flag & 0x40 != 0 {
            let low = *data.get(cursor).ok_or(RleError::UnexpectedEnd)?;
            cursor += 1;
            count = (count << 8) | usize::from(low);
        }
        let colour = if flag & 0x80 == 0 {
            0
        } else {
            let c = *data.get(cursor).ok_or(RleError::UnexpectedEnd)?;
            cursor += 1;
            c
        };

        plane.write(colour, count)?;
    }

    plane.finish()
}

/// The index plane being filled, and where in it the next run lands.
struct Plane {
    pixels: Vec<u8>,
    stride: usize,
    rows: usize,
    width: u32,
    height: u32,
    line: usize,
    column: usize,
}

impl Plane {
    fn new(width: u32, height: u32) -> Self {
        let stride = width as usize;
        let rows = height as usize;
        Self {
            pixels: vec![0; stride * rows],
            stride,
            rows,
            width,
            height,
            line: 0,
            column: 0,
        }
    }

    /// Write `count` pixels of `colour` at the cursor, advancing it.
    fn write(&mut self, colour: u8, count: usize) -> Result<(), RleError> {
        let line = u32::try_from(self.line).unwrap_or(u32::MAX);

        // A run on a line past the declared height has nowhere to go. Either way the object
        // header and its data disagree, which is the fact the caller needs.
        if self.line >= self.rows {
            return Err(RleError::LineCount {
                decoded: line.saturating_add(1),
                expected: self.height,
            });
        }
        if self.column + count > self.stride {
            return Err(RleError::LineOverflow {
                line,
                width: self.width,
                overflow_by: u32::try_from(self.column + count - self.stride).unwrap_or(u32::MAX),
            });
        }

        let start = self.line * self.stride + self.column;
        self.pixels[start..start + count].fill(colour);
        self.column += count;
        Ok(())
    }

    fn end_line(&mut self) {
        self.line += 1;
        self.column = 0;
    }

    /// Check the line count and hand back the plane.
    fn finish(mut self) -> Result<Vec<u8>, RleError> {
        // Encoders are inconsistent about terminating the final line, so a partially written
        // trailing line counts as complete rather than rejecting a file every other tool accepts.
        if self.column > 0 {
            self.line += 1;
        }
        if self.line != self.rows {
            return Err(RleError::LineCount {
                decoded: u32::try_from(self.line).unwrap_or(u32::MAX),
                expected: self.height,
            });
        }
        Ok(self.pixels)
    }
}

/// Encode a row-major index plane back to PGS run-length data.
///
/// Present so the decoder can be round-tripped against known bitmaps, and so the fixtures in #15
/// can be built without shipping copyrighted subtitle streams.
///
/// # Panics
/// Panics if `pixels.len()` is not `width * height`. This builds fixtures; a caller that gets that
/// wrong has a bug rather than bad input.
#[must_use]
pub fn encode(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let stride = width as usize;
    assert_eq!(
        pixels.len(),
        stride * height as usize,
        "pixel count must match the dimensions"
    );

    let mut out = Vec::new();
    for row in pixels.chunks_exact(stride) {
        let mut index = 0;
        while index < row.len() {
            let colour = row[index];
            let mut run = 1;
            while index + run < row.len() && row[index + run] == colour {
                run += 1;
            }
            index += run;

            // Runs wider than the 14-bit length field go out as several runs.
            let mut left = run;
            while left > 0 {
                let chunk = left.min(MAX_RUN);
                emit_run(&mut out, colour, chunk);
                left -= chunk;
            }
        }
        out.extend_from_slice(&[0x00, 0x00]);
    }
    out
}

/// Emit the shortest encoding of one run.
fn emit_run(out: &mut Vec<u8>, colour: u8, count: usize) {
    debug_assert!(count > 0 && count <= MAX_RUN);
    let high = u8::try_from(count >> 8).unwrap_or(0);
    let low = u8::try_from(count & 0xFF).unwrap_or(0);

    if colour == 0 {
        if count <= 0x3F {
            out.extend_from_slice(&[0x00, low]);
        } else {
            out.extend_from_slice(&[0x00, 0x40 | high, low]);
        }
        return;
    }
    // One or two literal pixels cost no more than a run header would.
    if count <= 2 {
        for _ in 0..count {
            out.push(colour);
        }
    } else if count <= 0x3F {
        out.extend_from_slice(&[0x00, 0x80 | low, colour]);
    } else {
        out.extend_from_slice(&[0x00, 0xC0 | high, low, colour]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_pixel_run_decodes_to_those_pixels() {
        assert_eq!(decode(&[1, 2, 3, 0x00, 0x00], 3, 1).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn a_short_transparent_run_fills_with_zero() {
        assert_eq!(decode(&[0x00, 0x04, 0x00, 0x00], 4, 1).unwrap(), vec![0, 0, 0, 0]);
    }

    #[test]
    fn a_short_coloured_run_fills_with_that_colour() {
        assert_eq!(decode(&[0x00, 0x84, 0x07, 0x00, 0x00], 4, 1).unwrap(), vec![7, 7, 7, 7]);
    }

    #[test]
    fn long_form_runs_use_the_fourteen_bit_length() {
        // 0x41 0x00 -> 0x100 = 256 pixels of colour 0.
        let transparent = decode(&[0x00, 0x41, 0x00, 0x00, 0x00], 256, 1).unwrap();
        assert_eq!(transparent.len(), 256);
        assert!(transparent.iter().all(|p| *p == 0));

        // 0xC1 0x00 0x09 -> 256 pixels of colour 9.
        let coloured = decode(&[0x00, 0xC1, 0x00, 0x09, 0x00, 0x00], 256, 1).unwrap();
        assert!(coloured.iter().all(|p| *p == 9));
    }

    #[test]
    fn each_end_of_line_marker_starts_the_next_row() {
        let pixels = decode(&[1, 1, 0x00, 0x00, 2, 2, 0x00, 0x00], 2, 2).unwrap();
        assert_eq!(pixels, vec![1, 1, 2, 2]);
    }

    #[test]
    fn a_short_line_is_padded_to_the_object_width() {
        assert_eq!(decode(&[5, 0x00, 0x00], 4, 1).unwrap(), vec![5, 0, 0, 0]);
    }

    #[test]
    fn a_run_past_the_right_edge_is_rejected_rather_than_clipped() {
        let err = decode(&[0x00, 0x88, 0x01, 0x00, 0x00], 4, 1).unwrap_err();
        assert_eq!(err, RleError::LineOverflow { line: 0, width: 4, overflow_by: 4 });
        assert!(err.to_string().contains("overflows"));
    }

    #[test]
    fn data_ending_inside_a_run_header_is_rejected() {
        assert_eq!(decode(&[0x00], 4, 1).unwrap_err(), RleError::UnexpectedEnd);
        assert_eq!(decode(&[0x00, 0x41], 4, 1).unwrap_err(), RleError::UnexpectedEnd);
        assert_eq!(decode(&[0x00, 0x81], 4, 1).unwrap_err(), RleError::UnexpectedEnd);
    }

    #[test]
    fn an_object_that_ends_early_is_rejected_not_padded_with_blank_lines() {
        // Two lines declared, one supplied. Padding would look like a legitimately blank line.
        let err = decode(&[1, 1, 0x00, 0x00], 2, 2).unwrap_err();
        assert_eq!(err, RleError::LineCount { decoded: 1, expected: 2 });
    }

    #[test]
    fn more_lines_than_declared_is_rejected() {
        let err = decode(&[1, 0x00, 0x00, 1, 0x00, 0x00, 1, 0x00, 0x00], 1, 2).unwrap_err();
        assert!(matches!(err, RleError::LineCount { expected: 2, .. }), "got {err:?}");
    }

    #[test]
    fn encode_then_decode_returns_the_original_bitmap() {
        let cases: &[(Vec<u8>, u32, u32)] = &[
            (vec![0, 1, 2, 3], 4, 1),
            (vec![0; 64], 8, 8),
            (vec![9; 64], 8, 8),
            ((0..64u32).map(|v| u8::try_from(v % 7).unwrap()).collect(), 8, 8),
        ];
        for (pixels, width, height) in cases {
            let encoded = encode(pixels, *width, *height);
            let decoded = decode(&encoded, *width, *height).unwrap();
            assert_eq!(&decoded, pixels, "round trip failed for {width}x{height}");
        }
    }

    #[test]
    fn round_trip_survives_runs_longer_than_the_length_field() {
        // 20_000 px exceeds the 14-bit maximum, so the encoder has to split the run.
        let width = 20_000;
        let pixels = vec![3u8; width as usize];
        let encoded = encode(&pixels, width, 1);
        assert_eq!(decode(&encoded, width, 1).unwrap(), pixels);
    }

    #[test]
    fn no_input_can_make_the_decoder_panic() {
        // Stands in for a fuzz target until #15 sets one up: walk every two-byte stream and a
        // spread of longer ones against dimensions small enough that most are invalid, and
        // require a clean Result from all of them.
        for a in 0..=u8::MAX {
            for b in 0..=u8::MAX {
                let _ = decode(&[a, b], 2, 2);
                let _ = decode(&[a, b, a], 3, 1);
                let _ = decode(&[0x00, a, b], 4, 2);
                let _ = decode(&[a, 0x00, b, 0x00, 0x00], 4, 1);
            }
        }
    }
}
