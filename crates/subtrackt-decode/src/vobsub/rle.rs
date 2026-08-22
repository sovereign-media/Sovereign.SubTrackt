//! VOBSUB nibble run-length decoding.
//!
//! Two bits per pixel, four colours, runs encoded in nibbles. A run is read one nibble at a time
//! until the accumulated value is large enough to be unambiguous:
//!
//! | Nibbles read | Value range | Count |
//! | ---: | :--- | :--- |
//! | 1 | `0x4`–`0xF` | 1–3 |
//! | 2 | `0x10`–`0x3F` | 4–15 |
//! | 3 | `0x40`–`0xFF` | 16–63 |
//! | 4 | `0x100`–`0x3FFF` | 64–4095 |
//!
//! In every case the low two bits are the colour and the rest is the count. A run whose count comes
//! out as zero means "to the end of the line", which is how trailing background is written without
//! knowing the width in advance.
//!
//! The two halves of a subpicture are the even and odd scanlines, stored separately and interleaved
//! back together here. Each field pads to a byte boundary at the end of every line.

use std::fmt;

/// What can be wrong with a nibble run-length stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RleError {
    /// The data ended part-way through a field.
    UnexpectedEnd {
        /// `0` for the top field (even lines), `1` for the bottom.
        field: u8,
        /// The line being filled.
        line: u32,
    },
    /// A run ran past the right edge of its line.
    LineOverflow {
        /// The line the run started on.
        line: u32,
        /// Declared subpicture width.
        width: u32,
    },
}

impl fmt::Display for RleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd { field, line } => {
                write!(f, "data ended inside field {field} on line {line}")
            }
            Self::LineOverflow { line, width } => {
                write!(f, "run on line {line} overflows the {width}px line")
            }
        }
    }
}

impl std::error::Error for RleError {}

/// A cursor over nibbles.
struct Nibbles<'a> {
    data: &'a [u8],
    /// Position in nibbles, so the low bit selects which half of a byte.
    at: usize,
}

impl<'a> Nibbles<'a> {
    const fn new(data: &'a [u8], byte_offset: usize) -> Self {
        Self { data, at: byte_offset * 2 }
    }

    fn next_nibble(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.at / 2)?;
        let value = if self.at.is_multiple_of(2) {
            byte >> 4
        } else {
            byte & 0x0F
        };
        self.at += 1;
        Some(value)
    }

    /// Advance to the next whole byte, which every line is padded to.
    const fn align(&mut self) {
        if self.at % 2 == 1 {
            self.at += 1;
        }
    }
}

/// Read one run as `(count, colour)`. A count of zero means "to the end of the line".
fn read_run(nibbles: &mut Nibbles<'_>) -> Option<(u32, u8)> {
    let mut value = u32::from(nibbles.next_nibble()?);
    // Keep pulling nibbles until the value is unambiguous, at most four in total.
    if value < 0x4 {
        value = (value << 4) | u32::from(nibbles.next_nibble()?);
        if value < 0x10 {
            value = (value << 4) | u32::from(nibbles.next_nibble()?);
            if value < 0x40 {
                value = (value << 4) | u32::from(nibbles.next_nibble()?);
            }
        }
    }
    Some((value >> 2, u8::try_from(value & 0x3).unwrap_or(0)))
}

/// Decode one field into alternate lines of `out`, starting at `first_line`.
fn decode_field(
    out: &mut [u8],
    width: u32,
    height: u32,
    data: &[u8],
    offset: usize,
    first_line: u32,
) -> Result<(), RleError> {
    let mut nibbles = Nibbles::new(data, offset);
    let stride = width as usize;
    let mut line = first_line;

    while line < height {
        let mut column = 0u32;
        while column < width {
            let Some((count, colour)) = read_run(&mut nibbles) else {
                return Err(RleError::UnexpectedEnd { field: u8::from(first_line == 1), line });
            };
            // Zero means the rest of the line, which is how trailing background is written.
            let count = if count == 0 { width - column } else { count };
            if column + count > width {
                return Err(RleError::LineOverflow { line, width });
            }

            let start = line as usize * stride + column as usize;
            out[start..start + count as usize].fill(colour);
            column += count;
        }
        nibbles.align();
        line += 2;
    }
    Ok(())
}

/// Decode the two interlaced fields of a subpicture into one progressive index plane.
///
/// `top_offset` and `bottom_offset` are byte offsets into `data`, as the `0x06` control command
/// gives them.
///
/// # Errors
/// Returns [`RleError`] if either field ends early or a run overflows its line.
pub fn decode(
    data: &[u8],
    top_offset: usize,
    bottom_offset: usize,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, RleError> {
    let mut out = vec![0u8; width as usize * height as usize];
    if width == 0 || height == 0 {
        return Ok(out);
    }

    decode_field(&mut out, width, height, data, top_offset, 0)?;
    decode_field(&mut out, width, height, data, bottom_offset, 1)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pack nibbles into bytes, padding the tail.
    fn pack(nibbles: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for pair in nibbles.chunks(2) {
            out.push((pair[0] << 4) | pair.get(1).copied().unwrap_or(0));
        }
        out
    }

    /// Encode one run the way the format does, in the narrowest form that fits.
    ///
    /// Hand-picking nibbles for these tests got the widths wrong twice; deriving them from the
    /// same rule the decoder reads makes the fixtures say what they mean.
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

    /// A field of `lines` lines, each one run of `count` pixels of `colour`.
    fn field(count: u32, colour: u8, lines: usize) -> Vec<u8> {
        let mut nibbles = Vec::new();
        for _ in 0..lines {
            nibbles.extend(run(count, colour));
            // Each line pads to a byte boundary.
            if nibbles.len() % 2 == 1 {
                nibbles.push(0);
            }
        }
        pack(&nibbles)
    }

    #[test]
    fn a_run_encodes_in_the_narrowest_form_that_fits() {
        assert_eq!(run(3, 1).len(), 1, "count 3 fits in one nibble");
        assert_eq!(run(4, 1).len(), 2);
        assert_eq!(run(16, 0).len(), 3);
        assert_eq!(run(64, 0).len(), 4);
    }

    #[test]
    fn every_run_width_decodes_back_to_what_it_encoded() {
        for (count, colour) in [
            (1u32, 0u8),
            (3, 1),
            (4, 2),
            (15, 3),
            (16, 1),
            (63, 2),
            (64, 0),
        ] {
            let data = pack(&run(count, colour));
            assert_eq!(
                read_run(&mut Nibbles::new(&data, 0)),
                Some((count, colour)),
                "count {count} colour {colour}"
            );
        }
    }

    #[test]
    fn a_zero_count_run_fills_to_the_end_of_the_line() {
        let data = pack(&[0x0, 0x0, 0x0, 0x0]);
        assert_eq!(decode(&data, 0, 0, 4, 2).unwrap(), vec![0; 8]);
    }

    #[test]
    fn the_two_fields_interleave_into_alternate_lines() {
        let mut data = field(4, 1, 2);
        let bottom_at = data.len();
        data.extend_from_slice(&field(4, 2, 2));

        let out = decode(&data, 0, bottom_at, 4, 4).unwrap();
        assert_eq!(&out[0..4], &[1, 1, 1, 1], "line 0 comes from the top field");
        assert_eq!(&out[4..8], &[2, 2, 2, 2], "line 1 comes from the bottom field");
        assert_eq!(&out[8..12], &[1, 1, 1, 1], "line 2 from the top field again");
        assert_eq!(&out[12..16], &[2, 2, 2, 2]);
    }

    #[test]
    fn each_line_is_padded_to_a_byte_boundary() {
        // A three-nibble run leaves the cursor mid-byte. Without the pad, every later line decodes
        // from the wrong place.
        let data = pack(&[0x0, 0x4, 0x0, 0xF]);
        let mut nibbles = Nibbles::new(&data, 0);
        read_run(&mut nibbles).unwrap();
        assert_eq!(nibbles.at, 3);
        nibbles.align();
        assert_eq!(nibbles.at, 4);
    }

    #[test]
    fn a_field_that_ends_early_is_rejected_rather_than_padded() {
        // Half a subpicture reads as legitimately blank, which is the silent failure to avoid.
        let err = decode(&pack(&run(3, 1)), 0, 0, 40, 4).unwrap_err();
        assert!(matches!(err, RleError::UnexpectedEnd { .. }), "got {err:?}");
    }

    #[test]
    fn a_run_past_the_right_edge_is_rejected() {
        let err = decode(&pack(&run(4, 0)), 0, 0, 2, 2).unwrap_err();
        assert_eq!(err, RleError::LineOverflow { line: 0, width: 2 });
        assert!(err.to_string().contains("overflows"));
    }

    #[test]
    fn a_zero_sized_subpicture_decodes_to_nothing() {
        assert!(decode(&[], 0, 0, 0, 0).unwrap().is_empty());
    }

    #[test]
    fn no_input_can_make_the_decoder_panic() {
        for a in 0..=u8::MAX {
            for b in 0..=u8::MAX {
                let _ = decode(&[a, b], 0, 1, 4, 2);
                let _ = decode(&[a, b, a, b], 0, 2, 8, 4);
                let _ = decode(&[a], 0, 0, 2, 2);
            }
        }
    }
}
