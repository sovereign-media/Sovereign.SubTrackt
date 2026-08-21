//! Turning a palette-indexed subtitle image into a foreground mask.
//!
//! The mask itself and the threshold policy are implemented; classifying palette indices into
//! fill, outline and anti-aliased edge is #5.
//!
//! The reason to threshold on palette *alpha* rather than on pixel luma: both PGS and VOBSUB
//! author the glyph fill, its outline and its anti-aliased edge as separate palette entries. That
//! makes foreground-versus-background a question about the palette — answerable once per image,
//! for at most 256 entries — instead of a question about every pixel.

use subtrackt_core::{Error, IndexedBitmap, Palette, Result, SubtitleImage};

/// Which parts of a glyph count as foreground.
///
/// This is the decision #5 has to settle with measurement. Including the outline thickens every
/// glyph and shifts every feature vector; excluding it can sever thin strokes at low resolutions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Threshold {
    /// Minimum palette alpha for a pixel to be foreground at all.
    pub min_alpha: u8,
    /// Minimum palette luma for a foreground pixel to count as fill rather than outline.
    ///
    /// Subtitle text is conventionally light on a dark outline, so fill is the high-luma half.
    pub min_luma: u8,
    /// Whether the outline is included in the mask alongside the fill.
    pub include_outline: bool,
}

impl Default for Threshold {
    /// Fill only, at half alpha and half luma.
    ///
    /// A starting point, not a measured answer.
    fn default() -> Self {
        Self { min_alpha: 128, min_luma: 128, include_outline: false }
    }
}

impl Threshold {
    /// Whether a palette entry is foreground under this threshold.
    #[must_use]
    pub fn accepts(self, palette: &Palette, index: u8) -> bool {
        let entry = palette.get(index);
        if entry.alpha < self.min_alpha {
            return false;
        }
        self.include_outline || entry.y >= self.min_luma
    }
}

/// A one-bit-per-pixel foreground mask, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryMask {
    width: u32,
    height: u32,
    bits: Vec<bool>,
}

impl BinaryMask {
    /// An all-background mask.
    #[must_use]
    pub fn blank(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            bits: vec![false; width as usize * height as usize],
        }
    }

    /// Build a mask from a row-major slice of flags.
    ///
    /// # Errors
    /// Returns [`Error::Config`] if the flag count does not match the dimensions.
    pub fn from_bits(width: u32, height: u32, bits: Vec<bool>) -> Result<Self> {
        let expected = width as usize * height as usize;
        if bits.len() != expected {
            return Err(Error::Config(format!(
                "mask is {width}x{height} ({expected} px) but got {} flags",
                bits.len()
            )));
        }
        Ok(Self { width, height, bits })
    }

    /// Mask width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Mask height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Whether `(x, y)` is foreground. Out-of-bounds reads are background, which lets neighbour
    /// scans run without edge special-casing.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.bits[y as usize * self.width as usize + x as usize]
    }

    /// Set `(x, y)`. Out-of-bounds writes are ignored.
    pub fn set(&mut self, x: u32, y: u32, value: bool) {
        if x < self.width && y < self.height {
            let index = y as usize * self.width as usize + x as usize;
            self.bits[index] = value;
        }
    }

    /// Number of foreground pixels.
    #[must_use]
    pub fn foreground_count(&self) -> usize {
        self.bits.iter().filter(|b| **b).count()
    }

    /// Foreground pixels per row, the projection line splitting works from.
    #[must_use]
    pub fn row_projection(&self) -> Vec<u32> {
        (0..self.height)
            .map(|y| {
                (0..self.width)
                    .filter(|&x| self.get(x, y))
                    .count()
                    .try_into()
                    .unwrap_or(u32::MAX)
            })
            .collect()
    }

    /// Foreground pixels per column, the projection word spacing works from.
    #[must_use]
    pub fn column_projection(&self) -> Vec<u32> {
        (0..self.width)
            .map(|x| {
                (0..self.height)
                    .filter(|&y| self.get(x, y))
                    .count()
                    .try_into()
                    .unwrap_or(u32::MAX)
            })
            .collect()
    }
}

/// Applies a [`Threshold`] to a subtitle image.
#[derive(Debug, Clone, Copy, Default)]
pub struct Binarizer {
    threshold: Threshold,
}

impl Binarizer {
    /// A binarizer using the given threshold.
    #[must_use]
    pub const fn new(threshold: Threshold) -> Self {
        Self { threshold }
    }

    /// The threshold in force.
    #[must_use]
    pub const fn threshold(&self) -> Threshold {
        self.threshold
    }

    /// Reduce an image to a foreground mask.
    #[must_use]
    pub fn mask(&self, image: &SubtitleImage) -> BinaryMask {
        let bitmap = &image.bitmap;
        let mut mask = BinaryMask::blank(bitmap.width(), bitmap.height());

        // Resolve the palette once — at most 256 entries against potentially millions of pixels.
        let foreground: Vec<bool> = (0..=u8::MAX)
            .map(|i| self.threshold.accepts(&image.palette, i))
            .collect();

        for y in 0..bitmap.height() {
            for x in 0..bitmap.width() {
                if let Some(index) = bitmap.get(x, y) {
                    mask.set(x, y, foreground[index as usize]);
                }
            }
        }
        mask
    }

    /// Render the mask back to an indexed bitmap, for debug output and reference authoring.
    ///
    /// Foreground is index 1, background index 0.
    pub fn mask_as_bitmap(&self, image: &SubtitleImage) -> Result<IndexedBitmap> {
        let mask = self.mask(image);
        let pixels = (0..mask.height())
            .flat_map(|y| (0..mask.width()).map(move |x| (x, y)))
            .map(|(x, y)| u8::from(mask.get(x, y)))
            .collect();
        IndexedBitmap::new(mask.width(), mask.height(), pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{PaletteEntry, Rect, TimeSpan, Timestamp};

    /// A 3x2 image: index 0 transparent, 1 opaque bright fill, 2 opaque dark outline.
    fn image() -> SubtitleImage {
        let mut palette = Palette::transparent(3);
        palette.set(1, PaletteEntry { y: 235, cb: 128, cr: 128, alpha: 255 });
        palette.set(2, PaletteEntry { y: 16, cb: 128, cr: 128, alpha: 255 });

        SubtitleImage {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            position: Rect::new(0, 0, 3, 2),
            bitmap: IndexedBitmap::new(3, 2, vec![0, 1, 2, 0, 1, 0]).unwrap(),
            palette,
            forced: false,
        }
    }

    #[test]
    fn fill_only_thresholding_keeps_the_bright_entry_and_drops_the_outline() {
        let mask = Binarizer::default().mask(&image());
        assert!(mask.get(1, 0), "bright fill must be foreground");
        assert!(!mask.get(2, 0), "dark outline must not be");
        assert!(!mask.get(0, 0), "transparent must not be");
        assert_eq!(mask.foreground_count(), 2);
    }

    #[test]
    fn including_the_outline_picks_up_the_dark_entry_too() {
        let threshold = Threshold { include_outline: true, ..Threshold::default() };
        let mask = Binarizer::new(threshold).mask(&image());
        assert!(mask.get(2, 0));
        assert_eq!(mask.foreground_count(), 3);
    }

    #[test]
    fn a_transparent_palette_yields_an_empty_mask_rather_than_a_full_one() {
        let mut img = image();
        img.palette = Palette::transparent(3);
        assert_eq!(Binarizer::default().mask(&img).foreground_count(), 0);
    }

    #[test]
    fn projections_count_foreground_along_each_axis() {
        let mask = Binarizer::default().mask(&image());
        assert_eq!(mask.row_projection(), vec![1, 1]);
        assert_eq!(mask.column_projection(), vec![0, 2, 0]);
    }

    #[test]
    fn out_of_bounds_reads_are_background_so_neighbour_scans_need_no_edge_cases() {
        let mask = BinaryMask::blank(2, 2);
        assert!(!mask.get(99, 99));
        assert!(BinaryMask::from_bits(2, 2, vec![true; 3]).is_err());
    }

    #[test]
    fn the_debug_bitmap_mirrors_the_mask() {
        let bitmap = Binarizer::default().mask_as_bitmap(&image()).unwrap();
        assert_eq!(bitmap.pixels(), &[0, 1, 0, 0, 1, 0]);
    }
}
