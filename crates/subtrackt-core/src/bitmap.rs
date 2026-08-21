//! Palettes and the indexed bitmaps that bitmap subtitle codecs decode into.
//!
//! PGS and VOBSUB both encode a palette-indexed image, so the decoders stop at
//! [`IndexedBitmap`] and leave colour resolution to the binarizer. Keeping the index around
//! matters: alpha thresholding on the *palette* is cheaper and less lossy than thresholding on a
//! materialised RGBA buffer.

/// A straight (non-premultiplied) 8-bit-per-channel colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgba8 {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel; `0` is fully transparent.
    pub a: u8,
}

impl Rgba8 {
    /// A fully transparent colour.
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };

    /// Perceptual luminance (ITU-R BT.601), ignoring alpha.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn luma(self) -> u8 {
        let y = 0.299 * f32::from(self.r) + 0.587 * f32::from(self.g) + 0.114 * f32::from(self.b);
        // The coefficients sum to 1, so the clamp is belt-and-braces rather than load-bearing.
        y.round().clamp(0.0, 255.0) as u8
    }
}

/// One palette slot as authored by the codec, in the codec's native YCbCr + alpha form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PaletteEntry {
    /// Luma.
    pub y: u8,
    /// Blue-difference chroma.
    pub cb: u8,
    /// Red-difference chroma.
    pub cr: u8,
    /// Opacity; `0` is fully transparent.
    pub alpha: u8,
}

impl PaletteEntry {
    /// Convert to RGBA using the BT.601 limited-range matrix that both PGS and VOBSUB assume.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn to_rgba(self) -> Rgba8 {
        let y = 1.164_383 * (f32::from(self.y) - 16.0);
        let cb = f32::from(self.cb) - 128.0;
        let cr = f32::from(self.cr) - 128.0;
        let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
        Rgba8 {
            r: clamp(y + 1.596_027 * cr),
            g: clamp(y - 0.391_762 * cb - 0.812_968 * cr),
            b: clamp(y + 2.017_232 * cb),
            a: self.alpha,
        }
    }
}

impl PaletteEntry {
    /// Build an entry from RGB, for codecs whose palettes are authored that way.
    ///
    /// VOBSUB is the reason this exists: its palette arrives as `RRGGBB` hex in a `.idx` sidecar or
    /// a Matroska `CodecPrivate`, while everything downstream expects the YCbCr that PGS uses.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn from_rgb(r: u8, g: u8, b: u8, alpha: u8) -> Self {
        let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
        let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
        Self {
            y: clamp(0.256_788 * r + 0.504_129 * g + 0.097_906 * b + 16.0),
            cb: clamp(-0.148_223 * r - 0.290_993 * g + 0.439_216 * b + 128.0),
            cr: clamp(0.439_216 * r - 0.367_788 * g - 0.071_427 * b + 128.0),
            alpha,
        }
    }
}

/// A codec palette. PGS palettes hold up to 256 entries; VOBSUB uses 16 with a 4-entry
/// per-subpicture selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    entries: Vec<PaletteEntry>,
}

impl Palette {
    /// Build a palette from its entries.
    #[must_use]
    pub fn new(entries: Vec<PaletteEntry>) -> Self {
        Self { entries }
    }

    /// A fully transparent palette of `len` entries, the correct starting state for PGS where
    /// palette updates are incremental.
    #[must_use]
    pub fn transparent(len: usize) -> Self {
        Self { entries: vec![PaletteEntry::default(); len] }
    }

    /// Look up an entry, returning a transparent entry for out-of-range indices rather than
    /// panicking — malformed streams do reference undefined palette slots.
    #[must_use]
    pub fn get(&self, index: u8) -> PaletteEntry {
        self.entries
            .get(index as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Overwrite a single slot, growing the palette if needed.
    pub fn set(&mut self, index: u8, entry: PaletteEntry) {
        let index = index as usize;
        if index >= self.entries.len() {
            self.entries.resize(index + 1, PaletteEntry::default());
        }
        self.entries[index] = entry;
    }

    /// Number of slots currently defined.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the palette defines no slots.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// An axis-aligned rectangle in subtitle-plane coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    /// Distance from the left edge of the plane.
    pub x: u32,
    /// Distance from the top edge of the plane.
    pub y: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Rect {
    /// Construct a rectangle.
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// Pixel area.
    #[must_use]
    pub const fn area(self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// One past the rightmost column.
    #[must_use]
    pub const fn right(self) -> u32 {
        self.x + self.width
    }

    /// One past the bottom row.
    #[must_use]
    pub const fn bottom(self) -> u32 {
        self.y + self.height
    }

    /// The smallest rectangle containing both inputs.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: self.right().max(other.right()) - x,
            height: self.bottom().max(other.bottom()) - y,
        }
    }

    /// Whether the two rectangles overlap on the horizontal axis, the test that decides whether
    /// a diacritic belongs to the glyph below it.
    #[must_use]
    pub const fn overlaps_horizontally(self, other: Self) -> bool {
        self.x < other.right() && other.x < self.right()
    }
}

/// A palette-indexed image: one byte per pixel, row-major, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedBitmap {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl IndexedBitmap {
    /// Build a bitmap from raw indices.
    ///
    /// # Errors
    /// Returns [`crate::Error::Config`] if `pixels.len()` is not `width * height`.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> crate::Result<Self> {
        let expected = width as usize * height as usize;
        if pixels.len() != expected {
            return Err(crate::Error::Config(format!(
                "bitmap is {width}x{height} ({expected} px) but got {} bytes",
                pixels.len()
            )));
        }
        Ok(Self { width, height, pixels })
    }

    /// An all-zero bitmap of the given size.
    #[must_use]
    pub fn blank(width: u32, height: u32) -> Self {
        Self { width, height, pixels: vec![0; width as usize * height as usize] }
    }

    /// Bitmap width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Bitmap height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The raw index plane, row-major.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Mutable access to the raw index plane, for decoders filling a buffer in place.
    #[must_use]
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// The palette index at `(x, y)`, or `None` if out of bounds.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.pixels
            .get(y as usize * self.width as usize + x as usize)
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_a_pixel_count_that_does_not_match_the_dimensions() {
        assert!(IndexedBitmap::new(4, 4, vec![0; 15]).is_err());
        assert!(IndexedBitmap::new(4, 4, vec![0; 16]).is_ok());
    }

    #[test]
    fn palette_set_grows_and_unknown_indices_read_back_transparent() {
        let mut palette = Palette::transparent(2);
        let entry = PaletteEntry { y: 235, cb: 128, cr: 128, alpha: 255 };
        palette.set(9, entry);
        assert_eq!(palette.len(), 10);
        assert_eq!(palette.get(9), entry);
        assert_eq!(palette.get(200).alpha, 0);
    }

    #[test]
    fn rgb_round_trips_back_through_the_ycbcr_conversion() {
        // The two matrices are inverses, so a colour survives the trip within rounding.
        for (r, g, b) in [
            (255u8, 0u8, 0u8),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 255),
            (0, 0, 0),
        ] {
            let back = PaletteEntry::from_rgb(r, g, b, 255).to_rgba();
            let close = |a: u8, b: u8| i32::from(a).abs_diff(i32::from(b)) <= 2;
            assert!(
                close(back.r, r) && close(back.g, g) && close(back.b, b),
                "({r},{g},{b}) came back as {back:?}"
            );
        }
    }

    #[test]
    fn opaque_white_palette_entry_converts_to_white() {
        let white = PaletteEntry { y: 235, cb: 128, cr: 128, alpha: 255 }.to_rgba();
        assert_eq!((white.r, white.g, white.b, white.a), (255, 255, 255, 255));
    }

    #[test]
    fn horizontal_overlap_is_what_groups_a_diacritic_with_its_glyph() {
        let stem = Rect::new(10, 20, 8, 12);
        let dot = Rect::new(11, 14, 3, 3);
        let neighbour = Rect::new(20, 20, 8, 12);
        assert!(stem.overlaps_horizontally(dot));
        assert!(!stem.overlaps_horizontally(neighbour));
        assert_eq!(stem.union(dot), Rect::new(10, 14, 8, 18));
    }
}
