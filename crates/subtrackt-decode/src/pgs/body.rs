//! Interpreting PGS segment bodies.
//!
//! Framing lives in [`super::segment`]; this module turns a framed body into a typed structure.
//! Every read is bounds-checked against the declared length, so a body that lies about its own
//! contents produces [`Error::MalformedPacket`] naming the presentation timestamp rather than a
//! panic or a half-filled struct.

use subtrackt_core::{Error, PaletteEntry, Rect, Result};

/// A bounds-checked cursor over a segment body.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    pts: u64,
    what: &'static str,
}

impl<'a> Reader<'a> {
    const fn new(data: &'a [u8], pts: u64, what: &'static str) -> Self {
        Self { data, pos: 0, pts, what }
    }

    fn short(&self, needed: usize) -> Error {
        Error::MalformedPacket {
            codec: "pgs",
            pts: self.pts,
            reason: format!(
                "{} needs {needed} more bytes at offset {} but the segment holds {}",
                self.what,
                self.pos,
                self.data.len()
            ),
        }
    }

    fn u8(&mut self) -> Result<u8> {
        let v = *self.data.get(self.pos).ok_or_else(|| self.short(1))?;
        self.pos += 1;
        Ok(v)
    }

    fn u16(&mut self) -> Result<u16> {
        let bytes = self
            .data
            .get(self.pos..self.pos + 2)
            .ok_or_else(|| self.short(2))?;
        self.pos += 2;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn u24(&mut self) -> Result<u32> {
        let bytes = self
            .data
            .get(self.pos..self.pos + 3)
            .ok_or_else(|| self.short(3))?;
        self.pos += 3;
        Ok(u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]))
    }

    fn rest(self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

    const fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }
}

/// One object placed by a composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositionObject {
    /// Which object definition to draw.
    pub object_id: u16,
    /// Which window it is drawn into.
    pub window_id: u8,
    /// Whether the stream marked this as forced (foreign-dialogue) subtitles.
    pub forced: bool,
    /// Horizontal placement within the subtitle plane.
    pub x: u32,
    /// Vertical placement within the subtitle plane.
    pub y: u32,
    /// Sub-rectangle of the object to draw, when the composition crops it.
    pub crop: Option<Rect>,
}

/// A Presentation Composition Segment: what is on screen, where, and under which palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composition {
    /// Subtitle plane width.
    pub plane_width: u32,
    /// Subtitle plane height.
    pub plane_height: u32,
    /// Increments once per composition.
    pub number: u16,
    /// Whether this composition only swaps the palette, leaving the objects alone.
    pub palette_update: bool,
    /// Which palette to draw with.
    pub palette_id: u8,
    /// Objects to draw. Empty means "clear the screen", which is what ends the previous cue.
    pub objects: Vec<CompositionObject>,
}

impl Composition {
    /// Whether this composition clears the screen rather than drawing anything.
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.objects.is_empty()
    }
}

/// Parse a Presentation Composition Segment body.
///
/// # Errors
/// Returns [`Error::MalformedPacket`] if the body is shorter than the fields it declares.
pub fn composition(body: &[u8], pts: u64) -> Result<Composition> {
    let mut r = Reader::new(body, pts, "composition segment");

    let plane_width = u32::from(r.u16()?);
    let plane_height = u32::from(r.u16()?);
    let _frame_rate = r.u8()?;
    let number = r.u16()?;
    let _state = r.u8()?;
    let palette_update = r.u8()? & 0x80 != 0;
    let palette_id = r.u8()?;
    let count = r.u8()?;

    let mut objects = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let object_id = r.u16()?;
        let window_id = r.u8()?;
        let flags = r.u8()?;
        let x = u32::from(r.u16()?);
        let y = u32::from(r.u16()?);

        // Bit 7 marks forced subtitles, bit 6 marks a cropped object.
        let forced = flags & 0x80 != 0;
        let crop = if flags & 0x40 == 0 {
            None
        } else {
            let cx = u32::from(r.u16()?);
            let cy = u32::from(r.u16()?);
            let cw = u32::from(r.u16()?);
            let ch = u32::from(r.u16()?);
            Some(Rect::new(cx, cy, cw, ch))
        };

        objects.push(CompositionObject { object_id, window_id, forced, x, y, crop });
    }

    Ok(Composition {
        plane_width,
        plane_height,
        number,
        palette_update,
        palette_id,
        objects,
    })
}

/// Parse a Window Definition Segment body into `(window id, rectangle)` pairs.
///
/// # Errors
/// Returns [`Error::MalformedPacket`] if the body is shorter than the window count declares.
pub fn windows(body: &[u8], pts: u64) -> Result<Vec<(u8, Rect)>> {
    let mut r = Reader::new(body, pts, "window segment");
    let count = r.u8()?;

    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let id = r.u8()?;
        let x = u32::from(r.u16()?);
        let y = u32::from(r.u16()?);
        let width = u32::from(r.u16()?);
        let height = u32::from(r.u16()?);
        out.push((id, Rect::new(x, y, width, height)));
    }
    Ok(out)
}

/// A Palette Definition Segment: an incremental update to one palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteUpdate {
    /// Which palette is being updated.
    pub id: u8,
    /// Version, which increments per update.
    pub version: u8,
    /// The entries this segment defines. Slots not listed keep their previous value.
    pub entries: Vec<(u8, PaletteEntry)>,
}

/// Parse a Palette Definition Segment body.
///
/// Note the on-wire channel order is Y, Cr, Cb, alpha — chroma-red before chroma-blue, which is
/// the reverse of how the fields are usually written down.
///
/// # Errors
/// Returns [`Error::MalformedPacket`] if the body ends part-way through an entry.
pub fn palette(body: &[u8], pts: u64) -> Result<PaletteUpdate> {
    let mut r = Reader::new(body, pts, "palette segment");
    let id = r.u8()?;
    let version = r.u8()?;

    let mut entries = Vec::with_capacity(r.remaining() / 5);
    while r.remaining() > 0 {
        let index = r.u8()?;
        let y = r.u8()?;
        let cr = r.u8()?;
        let cb = r.u8()?;
        let alpha = r.u8()?;
        entries.push((index, PaletteEntry { y, cb, cr, alpha }));
    }
    Ok(PaletteUpdate { id, version, entries })
}

/// One fragment of an Object Definition Segment.
///
/// A single object routinely spans several segments: the first carries the dimensions and the
/// total data length, and the rest are raw continuation bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectFragment<'a> {
    /// Which object this belongs to.
    pub id: u16,
    /// Version, which increments when the object is redefined.
    pub version: u8,
    /// Whether this is the first fragment of the sequence.
    pub first: bool,
    /// Whether this is the last fragment of the sequence.
    pub last: bool,
    /// Object dimensions, present only on the first fragment.
    pub dimensions: Option<(u32, u32)>,
    /// Total run-length data length across every fragment, present only on the first.
    pub total_len: Option<usize>,
    /// This fragment's run-length bytes.
    pub data: &'a [u8],
}

/// Parse an Object Definition Segment body.
///
/// # Errors
/// Returns [`Error::MalformedPacket`] if the body is too short for its header, or if the first
/// fragment declares a total length smaller than the width and height fields it also carries.
pub fn object(body: &[u8], pts: u64) -> Result<ObjectFragment<'_>> {
    let mut r = Reader::new(body, pts, "object segment");
    let id = r.u16()?;
    let version = r.u8()?;
    let sequence = r.u8()?;

    let first = sequence & 0x80 != 0;
    let last = sequence & 0x40 != 0;

    if !first {
        return Ok(ObjectFragment {
            id,
            version,
            first,
            last,
            dimensions: None,
            total_len: None,
            data: r.rest(),
        });
    }

    // The declared length covers the width and height fields as well as the run-length data.
    let declared = r.u24()? as usize;
    let width = u32::from(r.u16()?);
    let height = u32::from(r.u16()?);

    let total_len = declared
        .checked_sub(4)
        .ok_or_else(|| Error::MalformedPacket {
            codec: "pgs",
            pts,
            reason: format!(
                "object {id} declares {declared} bytes, too few for its own dimensions"
            ),
        })?;

    Ok(ObjectFragment {
        id,
        version,
        first,
        last,
        dimensions: Some((width, height)),
        total_len: Some(total_len),
        data: r.rest(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composition_body(objects: &[(u16, u8, u32, u32)], forced: bool) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&1920u16.to_be_bytes());
        b.extend_from_slice(&1080u16.to_be_bytes());
        b.push(0x10); // frame rate
        b.extend_from_slice(&7u16.to_be_bytes()); // composition number
        b.push(0x80); // epoch start
        b.push(0x00); // no palette update
        b.push(0x00); // palette id
        b.push(u8::try_from(objects.len()).unwrap());
        for (id, window, x, y) in objects {
            b.extend_from_slice(&id.to_be_bytes());
            b.push(*window);
            b.push(if forced { 0x80 } else { 0x00 });
            b.extend_from_slice(&u16::try_from(*x).unwrap().to_be_bytes());
            b.extend_from_slice(&u16::try_from(*y).unwrap().to_be_bytes());
        }
        b
    }

    #[test]
    fn a_composition_reports_its_plane_and_objects() {
        let c = composition(&composition_body(&[(1, 0, 100, 900)], false), 0).unwrap();
        assert_eq!((c.plane_width, c.plane_height), (1920, 1080));
        assert_eq!(c.number, 7);
        assert!(!c.is_clear());
        assert_eq!(c.objects.len(), 1);
        assert_eq!(c.objects[0].object_id, 1);
        assert_eq!((c.objects[0].x, c.objects[0].y), (100, 900));
        assert!(c.objects[0].crop.is_none());
        assert!(!c.objects[0].forced);
    }

    #[test]
    fn a_composition_with_no_objects_is_a_clear() {
        let c = composition(&composition_body(&[], false), 0).unwrap();
        assert!(c.is_clear(), "an empty composition is what ends the previous cue");
    }

    #[test]
    fn the_forced_flag_is_read_from_the_object_flags() {
        let c = composition(&composition_body(&[(1, 0, 0, 0)], true), 0).unwrap();
        assert!(c.objects[0].forced);
    }

    #[test]
    fn a_cropped_object_carries_its_crop_rectangle() {
        let mut body = composition_body(&[(3, 0, 10, 20)], false);
        // Set the cropped flag and append the crop rectangle. The flags byte sits at offset 14:
        // 11 bytes of composition header, then the object id (2) and window id (1).
        body[14] = 0x40;
        body.extend_from_slice(&2u16.to_be_bytes());
        body.extend_from_slice(&4u16.to_be_bytes());
        body.extend_from_slice(&8u16.to_be_bytes());
        body.extend_from_slice(&16u16.to_be_bytes());

        let c = composition(&body, 0).unwrap();
        assert_eq!(c.objects[0].crop, Some(Rect::new(2, 4, 8, 16)));
    }

    #[test]
    fn a_composition_truncated_mid_object_names_the_timestamp() {
        let mut body = composition_body(&[(1, 0, 0, 0)], false);
        body.truncate(body.len() - 3);
        match composition(&body, 4_242).unwrap_err() {
            Error::MalformedPacket { codec, pts, reason } => {
                assert_eq!(codec, "pgs");
                assert_eq!(pts, 4_242);
                assert!(reason.contains("composition segment"), "{reason}");
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_palette_entry_reads_chroma_in_wire_order() {
        // id, version, then index, Y, Cr, Cb, alpha.
        let body = [0u8, 1, 5, 235, 90, 60, 255];
        let p = palette(&body, 0).unwrap();
        assert_eq!(p.id, 0);
        assert_eq!(p.version, 1);
        assert_eq!(p.entries.len(), 1);
        let (index, entry) = p.entries[0];
        assert_eq!(index, 5);
        assert_eq!(entry, PaletteEntry { y: 235, cb: 60, cr: 90, alpha: 255 });
    }

    #[test]
    fn a_palette_ending_mid_entry_is_rejected() {
        assert!(palette(&[0, 1, 5, 235, 90], 0).is_err());
    }

    #[test]
    fn windows_are_parsed_as_rectangles() {
        let body = [1u8, 0, 0, 10, 0, 20, 0, 100, 0, 50];
        assert_eq!(windows(&body, 0).unwrap(), vec![(0, Rect::new(10, 20, 100, 50))]);
    }

    #[test]
    fn a_first_object_fragment_carries_dimensions_and_total_length() {
        let mut body = Vec::new();
        body.extend_from_slice(&1u16.to_be_bytes());
        body.push(0); // version
        body.push(0xC0); // first and last
        body.extend_from_slice(&[0x00, 0x00, 0x08]); // declared length: 4 header + 4 data
        body.extend_from_slice(&8u16.to_be_bytes());
        body.extend_from_slice(&2u16.to_be_bytes());
        body.extend_from_slice(&[1, 2, 3, 4]);

        let o = object(&body, 0).unwrap();
        assert!(o.first && o.last);
        assert_eq!(o.dimensions, Some((8, 2)));
        assert_eq!(o.total_len, Some(4));
        assert_eq!(o.data, &[1, 2, 3, 4]);
    }

    #[test]
    fn a_continuation_fragment_is_all_data() {
        let mut body = Vec::new();
        body.extend_from_slice(&1u16.to_be_bytes());
        body.push(0);
        body.push(0x40); // last only, so a continuation
        body.extend_from_slice(&[9, 9, 9]);

        let o = object(&body, 0).unwrap();
        assert!(!o.first && o.last);
        assert_eq!(o.dimensions, None);
        assert_eq!(o.data, &[9, 9, 9]);
    }

    #[test]
    fn an_object_declaring_less_than_its_own_header_is_rejected() {
        let mut body = Vec::new();
        body.extend_from_slice(&1u16.to_be_bytes());
        body.push(0);
        body.push(0x80);
        body.extend_from_slice(&[0x00, 0x00, 0x02]); // 2 bytes cannot hold width and height
        body.extend_from_slice(&8u16.to_be_bytes());
        body.extend_from_slice(&2u16.to_be_bytes());

        assert!(object(&body, 0).is_err());
    }
}
