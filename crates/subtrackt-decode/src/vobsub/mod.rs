//! DVD subpictures.
//!
//! A subpicture is a pixel blob followed by a chain of control sequences. Nothing about it is a
//! header: the display area, the palette selection, the alpha map and the timing all arrive as
//! commands, each sequence carrying its own delay relative to the subpicture's presentation
//! timestamp. That is why timing here comes out of the command chain rather than out of the packet.
//!
//! The palette is not in the packet at all. A `.idx` sidecar carries it as text, and inside
//! Matroska — which is where essentially all of this library's VOBSUB lives — the same text sits in
//! the track's `CodecPrivate`. Without it a subpicture has four colour *indices* and no colours, so
//! alpha thresholding has nothing to work with.

pub mod control;
pub mod rle;

use subtrackt_core::{
    BitmapDecoder, Error, IndexedBitmap, Palette, PaletteEntry, Rect, Result, SubtitleImage,
    TimeSpan, Timestamp,
};

pub use control::Command;

/// Delay units are 1/90000 of a second scaled by 1024.
const DELAY_TO_TICKS: u64 = 1024;

/// How long a subpicture is held when its control chain never says to stop.
///
/// Real streams do usually carry a stop command; when one does not, the alternative to a fallback
/// is dropping the cue entirely. [`BitmapDecoder::unterminated_cues`] counts how often this was
/// needed, so the approximation stays auditable.
pub const UNTERMINATED_CUE_TICKS: u64 = 5 * subtrackt_core::PTS_HZ;

/// Parse the 16-colour palette out of the text a `.idx` or a Matroska `CodecPrivate` carries.
///
/// The entries are `RRGGBB` hex. They are converted to the YCbCr the rest of the pipeline speaks.
#[must_use]
pub fn parse_palette(text: &str) -> Option<Palette> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("palette:"))?;
    let entries: Vec<PaletteEntry> = line
        .trim_start_matches("palette:")
        .split(',')
        .filter_map(|c| u32::from_str_radix(c.trim(), 16).ok())
        .map(|rgb| {
            // Truncation is the point: each channel is the low byte of its shift.
            let channel = |shift: u32| u8::try_from((rgb >> shift) & 0xFF).unwrap_or(0);
            let (r, g, b) = (channel(16), channel(8), channel(0));
            PaletteEntry::from_rgb(r, g, b, 0)
        })
        .collect();

    (!entries.is_empty()).then(|| Palette::new(entries))
}

/// What one control chain asked for.
#[derive(Debug, Default, Clone)]
struct Display {
    /// Ticks from the subpicture's timestamp to when it appears.
    start: u64,
    /// Ticks to when it is cleared, if the chain says.
    stop: Option<u64>,
    /// Which four of the sixteen palette entries this subpicture uses, for indices 0..4.
    palette: [u8; 4],
    /// Alpha for each of those four, 0–15.
    alpha: [u8; 4],
    /// Where on the plane it sits.
    area: Rect,
    /// Byte offsets of the top and bottom fields within the packet.
    fields: (usize, usize),
    /// Whether the chain asked for forced display.
    forced: bool,
}

/// Decoder state for one VOBSUB stream.
pub struct VobSubDecoder {
    /// The 16-colour palette from the sidecar or `CodecPrivate`.
    palette: Option<Palette>,
    unterminated_cues: u64,
    subpictures: u64,
}

impl VobSubDecoder {
    /// A decoder with no palette yet.
    #[must_use]
    pub const fn new() -> Self {
        Self { palette: None, unterminated_cues: 0, subpictures: 0 }
    }

    /// Supply the 16-colour palette.
    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = Some(palette);
    }

    /// Whether a palette has been supplied.
    #[must_use]
    pub const fn has_palette(&self) -> bool {
        self.palette.is_some()
    }

    /// Subpictures decoded so far.
    #[must_use]
    pub const fn subpictures(&self) -> u64 {
        self.subpictures
    }

    /// Build the four-entry palette a subpicture actually draws with.
    fn subpicture_palette(&self, display: &Display) -> Palette {
        let source = self
            .palette
            .clone()
            .unwrap_or_else(|| Palette::transparent(16));
        let mut out = Palette::transparent(4);

        for index in 0..4u8 {
            let mut entry = source.get(display.palette[index as usize]);
            // Alpha is four bits; 15 is opaque. Scaling by 17 maps 0..15 onto 0..255 exactly.
            entry.alpha = display.alpha[index as usize].min(15) * 17;
            out.set(index, entry);
        }
        out
    }
}

impl Default for VobSubDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a big-endian `u16` at `at`.
fn be16(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?]))
}

/// Walk the control chain, folding every command into one [`Display`].
fn read_control(data: &[u8], first: usize, pts: u64) -> Result<Display> {
    let malformed = |reason: String| Error::MalformedPacket { codec: "vobsub", pts, reason };

    let mut display = Display { alpha: [15; 4], ..Display::default() };
    let mut at = first;
    let mut seen = 0;

    loop {
        let delay = u64::from(
            be16(data, at)
                .ok_or_else(|| malformed(format!("control sequence at {at} is short")))?,
        );
        let next = usize::from(
            be16(data, at + 2)
                .ok_or_else(|| malformed(format!("control sequence at {at} has no link")))?,
        );
        let ticks = delay * DELAY_TO_TICKS;

        let mut cursor = at + 4;
        while let Some(&byte) = data.get(cursor) {
            let Some(command) = Command::from_byte(byte) else {
                return Err(malformed(format!("unknown control command 0x{byte:02x} at {cursor}")));
            };
            cursor += 1;
            if command == Command::End {
                break;
            }

            match command {
                Command::ForcedStartDisplay => {
                    display.forced = true;
                    display.start = ticks;
                }
                Command::StartDisplay => display.start = ticks,
                Command::StopDisplay => display.stop = Some(ticks),
                Command::SetPalette => {
                    let v = be16(data, cursor).ok_or_else(|| malformed("short palette".into()))?;
                    // Four nibbles, highest first, for colours 3 down to 0.
                    for (index, shift) in [12u32, 8, 4, 0].into_iter().enumerate() {
                        display.palette[3 - index] = u8::try_from((v >> shift) & 0x0F).unwrap_or(0);
                    }
                    cursor += 2;
                }
                Command::SetAlpha => {
                    let v = be16(data, cursor).ok_or_else(|| malformed("short alpha".into()))?;
                    for (index, shift) in [12u32, 8, 4, 0].into_iter().enumerate() {
                        display.alpha[3 - index] = u8::try_from((v >> shift) & 0x0F).unwrap_or(0);
                    }
                    cursor += 2;
                }
                Command::SetDisplayArea => {
                    let b = data
                        .get(cursor..cursor + 6)
                        .ok_or_else(|| malformed("short display area".into()))?;
                    // Six bytes, four packed twelve-bit values: x1 x2 y1 y2, inclusive.
                    let x1 = (u32::from(b[0]) << 4) | u32::from(b[1] >> 4);
                    let x2 = ((u32::from(b[1]) & 0x0F) << 8) | u32::from(b[2]);
                    let y1 = (u32::from(b[3]) << 4) | u32::from(b[4] >> 4);
                    let y2 = ((u32::from(b[4]) & 0x0F) << 8) | u32::from(b[5]);
                    display.area =
                        Rect::new(x1, y1, x2.saturating_sub(x1) + 1, y2.saturating_sub(y1) + 1);
                    cursor += 6;
                }
                Command::SetPixelOffsets => {
                    let top =
                        be16(data, cursor).ok_or_else(|| malformed("short offsets".into()))?;
                    let bottom =
                        be16(data, cursor + 2).ok_or_else(|| malformed("short offsets".into()))?;
                    display.fields = (usize::from(top), usize::from(bottom));
                    cursor += 4;
                }
                Command::ChangeColorContrast => {
                    // Karaoke-style wipes. The operand block is self-describing, so it can be
                    // skipped without understanding it — but it cannot be skipped by a fixed
                    // width, which is why this command has no `operand_len`.
                    let len = usize::from(
                        be16(data, cursor).ok_or_else(|| malformed("short wipe block".into()))?,
                    );
                    cursor += len.max(2);
                }
                Command::End => break,
            }
        }

        seen += 1;
        if next == at || next == 0 || seen > 32 {
            break;
        }
        at = next;
    }
    Ok(display)
}

impl BitmapDecoder for VobSubDecoder {
    fn codec(&self) -> &'static str {
        "vobsub"
    }

    fn unterminated_cues(&self) -> u64 {
        self.unterminated_cues
    }

    fn configure(&mut self, codec_private: &[u8]) -> Result<()> {
        if codec_private.is_empty() {
            return Ok(());
        }
        let text = String::from_utf8_lossy(codec_private);
        if let Some(palette) = parse_palette(&text) {
            self.palette = Some(palette);
        }
        Ok(())
    }

    fn push(&mut self, pts: u64, payload: &[u8]) -> Result<Vec<SubtitleImage>> {
        let malformed = |reason: String| Error::MalformedPacket { codec: "vobsub", pts, reason };

        // The subpicture header is its total size and the offset of the first control sequence.
        let declared = usize::from(be16(payload, 0).ok_or_else(|| {
            malformed(format!("subpicture is {} bytes, too short for a header", payload.len()))
        })?);
        let control_at = usize::from(
            be16(payload, 2).ok_or_else(|| malformed("no control sequence offset".into()))?,
        );

        if control_at >= payload.len() || declared > payload.len() {
            return Err(malformed(format!(
                "declares {declared} bytes with control at {control_at}, but holds {}",
                payload.len()
            )));
        }

        let display = read_control(payload, control_at, pts)?;
        if display.area.width == 0 || display.area.height == 0 {
            // A subpicture with no display area draws nothing. Real streams use this to clear.
            return Ok(Vec::new());
        }

        let pixels = rle::decode(
            payload,
            display.fields.0,
            display.fields.1,
            display.area.width,
            display.area.height,
        )
        .map_err(|e| malformed(e.to_string()))?;

        let bitmap = IndexedBitmap::new(display.area.width, display.area.height, pixels)?;
        let start = pts + display.start;
        let end = display.stop.map_or_else(
            || {
                self.unterminated_cues += 1;
                start + UNTERMINATED_CUE_TICKS
            },
            |stop| pts + stop,
        );

        self.subpictures += 1;
        Ok(vec![SubtitleImage {
            span: TimeSpan::new(Timestamp::from_ticks(start), Timestamp::from_ticks(end)),
            position: display.area,
            bitmap,
            palette: self.subpicture_palette(&display),
            forced: display.forced,
        }])
    }
}

#[cfg(test)]
mod tests;
