//! Turning a palette-indexed subtitle image into a foreground mask.
//!
//! The mask itself and the threshold policy are implemented; classifying palette indices into
//! fill, outline and anti-aliased edge is #5.
//!
//! #5 is closed and that sentence is still true. What #234 changed is that the palettes are now
//! *counted*: `docs/palette.md` has what nine bench tracks draw, and the headline is that the two
//! codecs are not the same kind of thing. A VOBSUB track draws **three** colours and no partially
//! transparent ink at all; a PGS track draws a hundred or more and authors its anti-aliasing as a
//! luma ramp at full opacity. Any future classification has to hold both.
//!
//! The reason to threshold on palette *alpha* rather than on pixel luma: both PGS and VOBSUB
//! author the glyph fill, its outline and its anti-aliased edge as separate palette entries. That
//! makes foreground-versus-background a question about the palette — answerable once per image,
//! for at most 256 entries — instead of a question about every pixel.

use subtrackt_core::{Error, IndexedBitmap, Palette, Rect, Result, SubtitleImage};

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
    /// A starting point, not a measured answer — and #234 measured what it implies without
    /// changing it. On VOBSUB the cut is ideal: three colours, a 131-point empty band at 16..147,
    /// and 128 inside it. On PGS it is not in any gap — the widest empty band in opaque ink is
    /// 33..87 on four discs and four points wide on three others, so this cut falls inside a drawn
    /// ramp rather than between two clusters. `docs/palette.md`.
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
///
/// A bit, not a byte. It was a `Vec<bool>` under this same sentence until #149 — eight times the
/// footprint of what the doc claimed, on a plane-sized buffer, reallocated at that rate once per
/// glyph by [`Self::crop`]. Storing it as words also turns [`Self::foreground_count`] into a
/// popcount and lets the projections work sixty-four columns at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryMask {
    width: u32,
    height: u32,
    /// Row-major bits, `stride` words per row so a row never straddles a word boundary.
    ///
    /// Padding each row is what makes a row a slice: the projections and the labelling pass both
    /// want to walk one row at a time, and a mask whose rows started mid-word would make every one
    /// of those a shift-and-mask. The waste is under a word per row.
    words: Vec<u64>,
    stride: usize,
}

impl BinaryMask {
    /// An all-background mask.
    #[must_use]
    pub fn blank(width: u32, height: u32) -> Self {
        let stride = (width as usize).div_ceil(64);
        Self { width, height, words: vec![0; stride * height as usize], stride }
    }

    /// Build a mask from a row-major slice of flags.
    ///
    /// # Errors
    /// Returns [`Error::Config`] if the flag count does not match the dimensions.
    pub fn from_bits(width: u32, height: u32, bits: &[bool]) -> Result<Self> {
        let expected = width as usize * height as usize;
        if bits.len() != expected {
            return Err(Error::Config(format!(
                "mask is {width}x{height} ({expected} px) but got {} flags",
                bits.len()
            )));
        }
        let mut mask = Self::blank(width, height);
        for (index, set) in bits.iter().enumerate() {
            if *set {
                let (x, y) = (index % width as usize, index / width as usize);
                mask.words[y * mask.stride + x / 64] |= 1 << (x % 64);
            }
        }
        Ok(mask)
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
        let x = x as usize;
        self.words[y as usize * self.stride + x / 64] >> (x % 64) & 1 == 1
    }

    /// Set `(x, y)`. Out-of-bounds writes are ignored.
    pub fn set(&mut self, x: u32, y: u32, value: bool) {
        if x >= self.width || y >= self.height {
            return;
        }
        let x = x as usize;
        let word = &mut self.words[y as usize * self.stride + x / 64];
        let bit = 1u64 << (x % 64);
        if value {
            *word |= bit;
        } else {
            *word &= !bit;
        }
    }

    /// The words of one row, for a caller walking a row at a time.
    ///
    /// Bit `i % 64` of word `i / 64` is column `i`. Columns at or past [`Self::width`] are always
    /// clear, so a caller may popcount a whole row without masking the tail.
    #[must_use]
    pub fn row(&self, y: u32) -> &[u64] {
        if y >= self.height {
            return &[];
        }
        let start = y as usize * self.stride;
        &self.words[start..start + self.stride]
    }

    /// Number of foreground pixels.
    #[must_use]
    pub fn foreground_count(&self) -> usize {
        self.words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    /// The ink inside `area`, as a mask of its own.
    ///
    /// Reads through [`Self::get`], so a rectangle running off the edge of the source comes back
    /// padded with background rather than refused or clipped. That is the right answer here and not
    /// a convenience: the caller is a glyph's bounding box, which is derived from this mask's own
    /// components and therefore always inside it. A rectangle that is not says the boxes and the
    /// mask have come from different images, and a smaller-than-asked-for mask would hide that by
    /// producing a plausible glyph.
    ///
    /// A zero-width or zero-height area is refused, because a glyph with no extent is a component
    /// filter that let an empty box through rather than something to return an empty mask for.
    ///
    /// # Errors
    /// Returns [`Error::Config`] if `area` has zero width or height.
    pub fn crop(&self, area: Rect) -> Result<Self> {
        if area.width == 0 || area.height == 0 {
            return Err(Error::Config(format!(
                "cannot crop a {}x{} region out of a mask",
                area.width, area.height
            )));
        }
        let mut bits = Vec::with_capacity(area.width as usize * area.height as usize);
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                bits.push(self.get(x, y));
            }
        }
        Self::from_bits(area.width, area.height, &bits)
    }

    /// Foreground pixels per row, the projection line splitting works from.
    #[must_use]
    pub fn row_projection(&self) -> Vec<u32> {
        (0..self.height)
            .map(|y| self.row(y).iter().map(|word| word.count_ones()).sum())
            .collect()
    }

    /// Foreground pixels per column.
    ///
    /// Unused on the matching path today: `split::cut_columns` computes the same profile by hand,
    /// scoped to a component's box rather than to the whole mask. #144 row 37 is to give this the
    /// rectangle and delete that copy, which is why it is kept rather than swept.
    #[must_use]
    pub fn column_projection(&self) -> Vec<u32> {
        self.column_projection_in(Rect::new(0, 0, self.width, self.height))
    }

    /// Foreground pixels per column of `area`, in that area's own coordinates.
    ///
    /// Scoped, because the caller that needs a column profile is `split::cut_columns` looking at
    /// one component's box — it had computed exactly this by hand while the whole-mask version sat
    /// here unused. Walks rows rather than columns: a column-major scan over a row-major buffer
    /// touches a different cache line per pixel.
    #[must_use]
    pub fn column_projection_in(&self, area: Rect) -> Vec<u32> {
        let mut counts = vec![0u32; area.width as usize];
        for y in area.y..area.bottom().min(self.height) {
            for (offset, count) in counts.iter_mut().enumerate() {
                let x = area.x + u32::try_from(offset).unwrap_or(u32::MAX);
                if self.get(x, y) {
                    *count += 1;
                }
            }
        }
        counts
    }
}

/// How much ink is at each pixel, from none to full, rather than a yes-or-no decision.
///
/// #14 measured that a one-pixel shift in a glyph's edge costs as much as the distance to a
/// different character, and the follow-up experiments showed the cause is the binary decision
/// itself rather than where the threshold sits. This is the alternative: keep the anti-aliasing
/// ramp as a magnitude, so the same letterform rendered two ways differs *gradually* instead of by
/// whole flipped pixels.
///
/// Connected components still need a binary mask — a component is a yes-or-no thing — so this runs
/// alongside [`BinaryMask`] rather than replacing it, and feeds the feature vector only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageMask {
    width: u32,
    height: u32,
    values: Vec<u8>,
}

impl CoverageMask {
    /// Build from a row-major slice of coverage values.
    ///
    /// # Errors
    /// Returns [`Error::Config`] if the value count does not match the dimensions.
    pub fn from_values(width: u32, height: u32, values: Vec<u8>) -> Result<Self> {
        let expected = width as usize * height as usize;
        if values.len() != expected {
            return Err(Error::Config(format!(
                "coverage mask is {width}x{height} ({expected} px) but got {} values",
                values.len()
            )));
        }
        Ok(Self { width, height, values })
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

    /// Coverage at `(x, y)`, zero outside the mask so edge scans need no special case.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.values[y as usize * self.width as usize + x as usize]
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

        // Over rows of the index plane rather than over coordinates. Both planes are the same size
        // by construction, so the per-pixel bounds check either side of the old loop was checking
        // something the shapes already guaranteed.
        let width = bitmap.width() as usize;
        for (y, row) in bitmap.pixels().chunks_exact(width).enumerate() {
            for (x, index) in row.iter().enumerate() {
                if foreground[*index as usize] {
                    mask.set(
                        u32::try_from(x).unwrap_or(u32::MAX),
                        u32::try_from(y).unwrap_or(u32::MAX),
                        true,
                    );
                }
            }
        }
        mask
    }

    /// Reduce an image to per-pixel ink coverage.
    ///
    /// Coverage is opacity times brightness: a fully opaque bright pixel is full ink, a
    /// transparent one is none, and an anti-aliased edge between fill and outline lands in
    /// between. Parameter-free on purpose — it is what "how much of this pixel is glyph" means
    /// physically, and any tunable here would just reintroduce the threshold placement the
    /// experiments in `docs/glyph-stability.md` showed does not matter.
    #[must_use]
    pub fn coverage(&self, image: &SubtitleImage) -> CoverageMask {
        let bitmap = &image.bitmap;

        // Resolve the palette once, as the binary path does.
        let ink: Vec<u8> = (0..=u8::MAX)
            .map(|index| {
                let entry = image.palette.get(index);
                let opacity = u32::from(entry.alpha);
                let brightness = if self.threshold.include_outline {
                    255
                } else {
                    u32::from(entry.y)
                };
                u8::try_from(opacity * brightness / 255).unwrap_or(255)
            })
            .collect();

        // The index plane and the coverage plane are the same shape, so this is a table lookup per
        // pixel over a slice rather than a bounds-checked read per coordinate.
        let values = bitmap
            .pixels()
            .iter()
            .map(|index| ink[*index as usize])
            .collect();

        CoverageMask { width: bitmap.width(), height: bitmap.height(), values }
    }

    /// Render the mask back to an indexed bitmap, for debug output and reference authoring.
    ///
    /// Foreground is index 1, background index 0.
    pub fn mask_as_bitmap(&self, image: &SubtitleImage) -> Result<IndexedBitmap> {
        let mask = self.mask(image);
        let mut pixels = Vec::with_capacity(mask.width() as usize * mask.height() as usize);
        for y in 0..mask.height() {
            for x in 0..mask.width() {
                pixels.push(u8::from(mask.get(x, y)));
            }
        }
        IndexedBitmap::new(mask.width(), mask.height(), pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{PaletteEntry, TimeSpan, Timestamp};

    /// A mask with a 2x2 block of ink at (1, 1) inside a 4x4 field.
    fn blocked() -> BinaryMask {
        let mut mask = BinaryMask::blank(4, 4);
        for (x, y) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            mask.set(x, y, true);
        }
        mask
    }

    #[test]
    fn cropping_takes_the_ink_at_the_rectangle_and_nothing_around_it() {
        let cropped = blocked().crop(Rect::new(1, 1, 2, 2)).unwrap();
        assert_eq!((cropped.width(), cropped.height()), (2, 2));
        assert_eq!(cropped.foreground_count(), 4, "the whole block and no margin");

        // Offset by one: a quarter of the block, in the corner nearest where it was.
        let corner = blocked().crop(Rect::new(2, 2, 2, 2)).unwrap();
        assert_eq!(corner.foreground_count(), 1);
        assert!(corner.get(0, 0));
    }

    #[test]
    fn a_crop_running_past_the_edge_is_padded_with_background_not_shrunk() {
        // The size asked for is the size returned, because a caller measuring a glyph needs the
        // box it asked about. Silently returning a smaller mask would put the ink at the wrong
        // coordinates and every axis measured from it would be wrong by an unknowable amount.
        let cropped = blocked().crop(Rect::new(3, 3, 4, 4)).unwrap();
        assert_eq!((cropped.width(), cropped.height()), (4, 4));
        assert_eq!(cropped.foreground_count(), 0, "past the ink, so background");
    }

    #[test]
    fn a_crop_with_no_extent_is_rejected_rather_than_returning_an_empty_mask() {
        // A glyph with no extent is a component filter that let an empty box through, and this
        // project reports that rather than handing back something that looks like a blank glyph.
        assert!(blocked().crop(Rect::new(0, 0, 0, 4)).is_err());
        assert!(blocked().crop(Rect::new(0, 0, 4, 0)).is_err());
    }

    #[test]
    fn cropping_the_whole_mask_returns_the_mask() {
        let mask = blocked();
        assert_eq!(mask.crop(Rect::new(0, 0, 4, 4)).unwrap(), mask);
    }

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
        assert!(BinaryMask::from_bits(2, 2, &[true; 3]).is_err());
    }

    #[test]
    fn the_debug_bitmap_mirrors_the_mask() {
        let bitmap = Binarizer::default().mask_as_bitmap(&image()).unwrap();
        assert_eq!(bitmap.pixels(), &[0, 1, 0, 0, 1, 0]);
    }
}
#[cfg(test)]
mod coverage_tests {
    use super::*;
    use subtrackt_core::{PaletteEntry, Rect, TimeSpan, Timestamp};

    /// A one-pixel image whose single palette entry has the given alpha and luma.
    fn pixel(alpha: u8, luma: u8) -> SubtitleImage {
        let mut palette = Palette::transparent(2);
        palette.set(1, PaletteEntry { y: luma, cb: 128, cr: 128, alpha });
        SubtitleImage {
            span: TimeSpan::new(Timestamp::from_ticks(0), Timestamp::from_ticks(1)),
            position: Rect::new(0, 0, 1, 1),
            bitmap: IndexedBitmap::new(1, 1, vec![1]).unwrap(),
            palette,
            forced: false,
        }
    }

    #[test]
    fn full_ink_is_opaque_and_bright_and_no_ink_is_transparent() {
        let binarizer = Binarizer::default();
        assert_eq!(binarizer.coverage(&pixel(255, 255)).get(0, 0), 255);
        assert_eq!(
            binarizer.coverage(&pixel(0, 255)).get(0, 0),
            0,
            "transparent is never ink"
        );
        assert_eq!(
            binarizer.coverage(&pixel(255, 0)).get(0, 0),
            0,
            "black fill is not fill"
        );
    }

    #[test]
    fn a_half_lit_pixel_lands_between_rather_than_at_an_end() {
        // The whole point: an anti-aliased edge must be readable as an edge.
        let value = Binarizer::default().coverage(&pixel(255, 128)).get(0, 0);
        assert!((100..=160).contains(&value), "got {value}");
    }

    #[test]
    fn opacity_and_brightness_both_scale_the_ink() {
        let binarizer = Binarizer::default();
        let dim = binarizer.coverage(&pixel(128, 255)).get(0, 0);
        let dark = binarizer.coverage(&pixel(255, 128)).get(0, 0);
        assert!(
            dim.abs_diff(dark) <= 2,
            "half alpha and half luma must cost the same: {dim} vs {dark}"
        );
    }

    #[test]
    fn including_the_outline_reads_opacity_alone() {
        // With the outline in the mask, a dark opaque pixel is fully ink rather than nearly none.
        let binarizer = Binarizer::new(Threshold { include_outline: true, ..Threshold::default() });
        assert_eq!(binarizer.coverage(&pixel(255, 16)).get(0, 0), 255);
        assert_eq!(binarizer.coverage(&pixel(0, 16)).get(0, 0), 0);
    }

    #[test]
    fn reading_outside_the_mask_is_zero_rather_than_a_panic() {
        let mask = Binarizer::default().coverage(&pixel(255, 255));
        assert_eq!(mask.get(9, 0), 0);
        assert_eq!(mask.get(0, 9), 0);
        assert_eq!(mask.width(), 1);
        assert_eq!(mask.height(), 1);
    }

    #[test]
    fn a_value_count_that_does_not_match_the_dimensions_is_rejected() {
        assert!(CoverageMask::from_values(2, 2, vec![0; 3]).is_err());
        assert!(CoverageMask::from_values(2, 2, vec![0; 4]).is_ok());
    }
}
