//! Connected component labelling.
//!
//! Classic two-pass labelling with a union-find. The first pass walks the mask in row-major order
//! giving each foreground pixel a provisional label, merging labels whenever a pixel touches two
//! that were previously thought distinct; the second pass resolves every provisional label to its
//! root and accumulates a bounding box.
//!
//! Connectivity is 8-way, which matters at subtitle resolutions: a diagonal stroke in a `V` or an
//! `X` is a single pen movement and a 4-connected pass would split it in two.
//!
//! One component is not one character. Anti-aliasing routinely fuses adjacent glyphs, and a
//! diacritic is always its own component. Turning components into characters is #6.

use subtrackt_core::{Error, Rect, Result};

use crate::binarize::BinaryMask;

/// Sentinel meaning "no label assigned".
const UNLABELLED: u32 = 0;

/// Constraints on what counts as a glyph-sized component.
///
/// Both bounds are needed. Without the lower one, compression noise and stray anti-aliasing pixels
/// become glyphs; without the upper one, a background box or a frame border becomes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentFilter {
    /// Components with fewer foreground pixels than this are discarded.
    pub min_area: u64,
    /// Components covering more than this fraction of the image, in percent, are candidates for
    /// being a background rather than a glyph — but only if they are also solid, see below.
    pub max_coverage_percent: u32,
    /// How densely filled a component must be, in percent of its own bounding box, before covering
    /// most of the image counts against it.
    ///
    /// Coverage alone is the wrong test and caused a real bug: a PGS object is cropped tightly
    /// around its text, so a cue holding a single character is one component covering essentially
    /// the whole image, and a coverage-only filter silently dropped it. What separates a background
    /// box from a glyph is density — a box is solid, a letter is mostly holes.
    pub max_fill_percent: u32,
}

impl Default for ComponentFilter {
    fn default() -> Self {
        Self { min_area: 4, max_coverage_percent: 50, max_fill_percent: 90 }
    }
}

impl ComponentFilter {
    /// A filter that keeps everything, for callers doing their own culling.
    #[must_use]
    pub const fn permissive() -> Self {
        Self { min_area: 0, max_coverage_percent: 100, max_fill_percent: 100 }
    }

    /// Whether a component of the given bounds and pixel count survives the filter.
    #[must_use]
    pub fn accepts(self, bounds: Rect, pixels: u64, image_area: u64) -> bool {
        if pixels < self.min_area {
            return false;
        }
        if image_area == 0 || bounds.area() == 0 {
            return true;
        }

        let dominates = bounds.area() * 100 / image_area > u64::from(self.max_coverage_percent);
        let solid = pixels * 100 / bounds.area() >= u64::from(self.max_fill_percent);

        // Both, not either. A big sparse component is a glyph on a tightly cropped object; a big
        // solid one is a background.
        !(dominates && solid)
    }
}

/// One labelled connected component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Component {
    /// Tight bounding box in mask coordinates.
    pub bounds: Rect,
    /// Foreground pixels in the component.
    pub pixels: u64,
    /// Which label this component's pixels carry in a [`LabelMap`].
    ///
    /// A bounding box is not a component, and for slanted text the difference is the whole story:
    /// an italic letter's box overhangs its neighbour's, so **reading the mask inside one box picks
    /// up the next letter's ink**. Every consumer that only wants a shape has been able to ignore
    /// that; #121 wants where the ink begins and ends, and a neighbour's foot inside the box would
    /// close exactly the gap the measurement exists to open.
    ///
    /// [`NO_LABEL`] for a component nothing labelled — a union of two others synthesised while
    /// grouping. Its ink is its members' and is found through them.
    pub label: u32,
}

/// The label of a component no pass ever assigned one to.
///
/// Zero is safe as the sentinel because the union-find reserves index 0 for its own unlabelled marker,
/// so no real component can carry it.
pub const NO_LABEL: u32 = 0;

/// Which component each pixel of a mask belongs to.
///
/// Kept beside the components rather than folded into them because it is a whole plane and most
/// callers want neither it nor the cost of it — the same split [`label`] and [`label_with_map`]
/// make, and the same one `ImageSegmenter::segment` and `segment_with_mask` make one crate up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelMap {
    width: u32,
    height: u32,
    labels: Vec<u32>,
}

impl LabelMap {
    /// The label at `(x, y)`, or [`NO_LABEL`] outside the mask or on background.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> u32 {
        if x >= self.width || y >= self.height {
            return NO_LABEL;
        }
        self.labels[y as usize * self.width as usize + x as usize]
    }

    /// Call `visit` for every pixel of one component, in row-major order.
    ///
    /// Scoped to `bounds` so the walk costs the component's box rather than the plane; a
    /// component's own box always contains all of it, by construction.
    pub fn for_each(&self, label: u32, bounds: Rect, mut visit: impl FnMut(u32, u32)) {
        if label == NO_LABEL {
            return;
        }
        for y in bounds.y..bounds.bottom() {
            for x in bounds.x..bounds.right() {
                if self.at(x, y) == label {
                    visit(x, y);
                }
            }
        }
    }
}

/// Union-find over provisional labels.
///
/// Union by size with path halving, so the first pass stays effectively linear in pixel count
/// rather than degrading on the long thin runs that a horizontal stroke produces.
struct Labels {
    parent: Vec<u32>,
    size: Vec<u32>,
}

impl Labels {
    fn new() -> Self {
        // Index 0 is the UNLABELLED sentinel and is never a real label.
        Self { parent: vec![0], size: vec![0] }
    }

    fn fresh(&mut self) -> u32 {
        let id = u32::try_from(self.parent.len()).unwrap_or(u32::MAX);
        self.parent.push(id);
        self.size.push(1);
        id
    }

    fn find(&mut self, mut label: u32) -> u32 {
        while self.parent[label as usize] != label {
            let grandparent = self.parent[self.parent[label as usize] as usize];
            self.parent[label as usize] = grandparent;
            label = grandparent;
        }
        label
    }

    fn union(&mut self, a: u32, b: u32) {
        let (mut a, mut b) = (self.find(a), self.find(b));
        if a == b {
            return;
        }
        if self.size[a as usize] < self.size[b as usize] {
            std::mem::swap(&mut a, &mut b);
        }
        self.parent[b as usize] = a;
        self.size[a as usize] += self.size[b as usize];
    }

    fn count(&self) -> usize {
        self.parent.len()
    }
}

/// Label 8-connected foreground components in a mask.
///
/// Components come back ordered by top edge, then left edge. That is deterministic but it is not
/// reading order — assigning components to text lines is #6, and doing it here would guess at a
/// baseline the mask alone does not establish.
///
/// # Errors
/// Returns [`Error::Config`] if the mask is large enough to exhaust the 32-bit label space, which
/// takes a mask far larger than any subtitle plane.
pub fn label(mask: &BinaryMask, filter: ComponentFilter) -> Result<Vec<Component>> {
    run(mask, filter, false).map(|(components, _)| components)
}

/// Label components and say which of them owns each pixel.
///
/// The map is the same labelling, kept rather than dropped. It exists because a component's
/// bounding box is not the component — see [`Component::label`] — and #121 needs the ink itself.
///
/// # Errors
/// Same as [`label`].
///
/// # Panics
/// Does not. The map is only absent when it was not asked for, and it is asked for here.
pub fn label_with_map(
    mask: &BinaryMask,
    filter: ComponentFilter,
) -> Result<(Vec<Component>, LabelMap)> {
    let (components, map) = run(mask, filter, true)?;
    Ok((components, map.expect("asked for the map, so `run` produced one")))
}

/// The one labelling both entry points run.
///
/// `want_map` is not a micro-optimisation. Resolving every pixel to its component's label and then
/// clearing the ones the filter rejected is a second and third pass over the whole plane, and the
/// callers that want only boxes — `font`, `xtask body-box` — outnumber the one that wants ink.
/// `a_full_width_subtitle_line_labels_correctly_and_quickly` is the reason it is a parameter rather
/// than a comment: it caught exactly this.
fn run(
    mask: &BinaryMask,
    filter: ComponentFilter,
    want_map: bool,
) -> Result<(Vec<Component>, Option<LabelMap>)> {
    let width = mask.width();
    let height = mask.height();
    let stride = width as usize;

    if stride.saturating_mul(height as usize) >= u32::MAX as usize {
        return Err(Error::Config(format!(
            "mask is {width}x{height}, too large to label with 32-bit labels"
        )));
    }

    let mut provisional = vec![UNLABELLED; stride * height as usize];
    let mut labels = Labels::new();

    // First pass: assign provisional labels, merging on contact.
    for y in 0..height {
        for x in 0..width {
            if !mask.get(x, y) {
                continue;
            }

            // Neighbours already visited in row-major order: W, NW, N, NE.
            let mut found = UNLABELLED;
            for (nx, ny) in neighbours(x, y, width) {
                let neighbour = provisional[ny as usize * stride + nx as usize];
                if neighbour == UNLABELLED {
                    continue;
                }
                if found == UNLABELLED {
                    found = neighbour;
                } else {
                    labels.union(found, neighbour);
                }
            }

            provisional[y as usize * stride + x as usize] = if found == UNLABELLED {
                labels.fresh()
            } else {
                found
            };
        }
    }

    // Second pass: resolve to roots and accumulate bounding boxes.
    let mut boxes: Vec<Option<Accumulator>> = vec![None; labels.count()];
    for y in 0..height {
        for x in 0..width {
            let provisional_label = provisional[y as usize * stride + x as usize];
            if provisional_label == UNLABELLED {
                continue;
            }
            let root = labels.find(provisional_label);
            if want_map {
                provisional[y as usize * stride + x as usize] = root;
            }
            match &mut boxes[root as usize] {
                Some(accumulator) => accumulator.add(x, y),
                slot @ None => *slot = Some(Accumulator::new(x, y, root)),
            }
        }
    }

    let image_area = u64::from(width) * u64::from(height);
    let mut components: Vec<Component> = boxes
        .into_iter()
        .flatten()
        .map(Accumulator::finish)
        .filter(|c| filter.accepts(c.bounds, c.pixels, image_area))
        .collect();

    components.sort_unstable_by_key(|c| (c.bounds.y, c.bounds.x));
    if !want_map {
        return Ok((components, None));
    }

    // A pixel whose component the filter rejected is background as far as the map is concerned.
    // Leaving it labelled would let a caller walk ink that no `Component` in the list accounts for.
    // Indexed rather than hashed: labels are dense and small, and this runs once per pixel.
    let mut kept = vec![false; labels.count()];
    for component in &components {
        if let Some(slot) = kept.get_mut(component.label as usize) {
            *slot = true;
        }
    }
    for slot in &mut provisional {
        if !kept.get(*slot as usize).copied().unwrap_or(false) {
            *slot = NO_LABEL;
        }
    }
    Ok((components, Some(LabelMap { width, height, labels: provisional })))
}

/// The already-visited neighbours of `(x, y)` in row-major order: W, NW, N, NE.
///
/// The `width` bound on the north-east neighbour is load-bearing. Without it, a pixel in the last
/// column reads index `(y - 1) * stride + width`, which is the *first pixel of its own row* — and
/// that pixel has already been labelled, so the two edges of the image get silently unioned into
/// one component.
fn neighbours(x: u32, y: u32, width: u32) -> impl Iterator<Item = (u32, u32)> {
    let west = (x > 0).then(|| (x - 1, y));
    let north_west = (x > 0 && y > 0).then(|| (x - 1, y - 1));
    let north = (y > 0).then(|| (x, y - 1));
    let north_east = (y > 0 && x + 1 < width).then(|| (x + 1, y - 1));
    [west, north_west, north, north_east].into_iter().flatten()
}

/// Running bounding box and pixel count for one component.
#[derive(Debug, Clone, Copy)]
struct Accumulator {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    pixels: u64,
    label: u32,
}

impl Accumulator {
    const fn new(x: u32, y: u32, label: u32) -> Self {
        Self { min_x: x, min_y: y, max_x: x, max_y: y, pixels: 1, label }
    }

    fn add(&mut self, x: u32, y: u32) {
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_y = self.min_y.min(y);
        self.max_y = self.max_y.max(y);
        self.pixels += 1;
    }

    fn finish(self) -> Component {
        Component {
            bounds: Rect::new(
                self.min_x,
                self.min_y,
                self.max_x - self.min_x + 1,
                self.max_y - self.min_y + 1,
            ),
            pixels: self.pixels,
            label: self.label,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a mask from rows of `#` (foreground) and anything else (background).
    fn mask(rows: &[&str]) -> BinaryMask {
        let height = u32::try_from(rows.len()).unwrap();
        let width = u32::try_from(rows[0].len()).unwrap();
        let bits: Vec<bool> = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        BinaryMask::from_bits(width, height, &bits).unwrap()
    }

    fn label_all(rows: &[&str]) -> Vec<Component> {
        label(&mask(rows), ComponentFilter::permissive()).unwrap()
    }

    #[test]
    fn an_empty_mask_has_no_components() {
        assert!(label_all(&["....", "....", "...."]).is_empty());
    }

    #[test]
    fn separated_blobs_are_counted_and_bounded_exactly() {
        let components = label_all(&[
            "##...##.", //
            "##...##.", "........", "....###.",
        ]);

        assert_eq!(components.len(), 3);
        assert_eq!(components[0].bounds, Rect::new(0, 0, 2, 2));
        assert_eq!(components[0].pixels, 4);
        assert_eq!(components[1].bounds, Rect::new(5, 0, 2, 2));
        assert_eq!(components[2].bounds, Rect::new(4, 3, 3, 1));
        assert_eq!(components[2].pixels, 3);
    }

    #[test]
    fn diagonal_contact_joins_a_component() {
        // The stroke of a V or an X. Four-connectivity would report two components here, which
        // would split one glyph into two and hand the matcher nonsense.
        let components = label_all(&["#...", ".#..", "..#.", "...#"]);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].bounds, Rect::new(0, 0, 4, 4));
        assert_eq!(components[0].pixels, 4);
    }

    #[test]
    fn a_u_shape_merges_labels_that_started_out_separate() {
        // The two arms get different provisional labels and only meet on the last row, which is
        // exactly the case the union-find exists for.
        let components = label_all(&["#..#", "#..#", "####"]);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].pixels, 8);
        assert_eq!(components[0].bounds, Rect::new(0, 0, 4, 3));
    }

    #[test]
    fn a_comb_merges_many_arms_into_one_component() {
        // Several separate labels created on row 0, all united by row 2. A naive first-wins
        // labeller reports five components here.
        let components = label_all(&["#.#.#.#.#", ".........", "#########"]);
        assert_eq!(components.len(), 6, "five detached teeth plus the spine");

        let joined = label_all(&["#.#.#.#.#", "#.#.#.#.#", "#########"]);
        assert_eq!(joined.len(), 1);
    }

    #[test]
    fn a_hole_does_not_split_the_ring_around_it() {
        // The counter of an O or a D.
        let components = label_all(&["####", "#..#", "#..#", "####"]);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].pixels, 12);
    }

    #[test]
    fn blocks_touching_only_at_a_corner_are_one_component() {
        // Worth pinning explicitly, because it surprises: under 8-connectivity a single shared
        // corner is contact. It is the right call for glyph strokes and it does mean two nearby
        // characters can fuse.
        assert_eq!(label_all(&["##..", "##..", "..##", "..##"]).len(), 1);
    }

    #[test]
    fn touching_glyphs_are_one_component_which_is_correct_here() {
        // Anti-aliasing fuses adjacent characters at low resolution. Splitting them is a
        // different problem and deliberately out of scope; this test pins the behaviour so the
        // decision is visible rather than accidental.
        let components = label_all(&["#.#.#", "#####", "#.#.#"]);
        assert_eq!(components.len(), 1);
    }

    #[test]
    fn the_minimum_area_filter_drops_specks() {
        let rows = ["#....", ".....", "..###", "..###"];
        assert_eq!(
            label(&mask(&rows), ComponentFilter::permissive())
                .unwrap()
                .len(),
            2
        );

        let filter = ComponentFilter { min_area: 4, ..ComponentFilter::permissive() };
        let kept = label(&mask(&rows), filter).unwrap();
        assert_eq!(
            kept.len(),
            1,
            "the single stray pixel is compression noise, not a glyph"
        );
        assert_eq!(kept[0].pixels, 6);
    }

    #[test]
    fn a_solid_box_filling_the_image_is_dropped_as_a_background() {
        let rows = ["####", "####", "####", "####"];
        assert!(
            label(&mask(&rows), ComponentFilter::default())
                .unwrap()
                .is_empty(),
            "a solid component covering the whole image is a background, not a glyph"
        );
    }

    #[test]
    fn a_sparse_glyph_filling_the_image_is_kept() {
        // The bug coverage-only filtering caused. A PGS object is cropped tightly around its
        // text, so a cue holding one character is a single component spanning the whole image.
        // Judged on coverage alone it looks exactly like a background and was silently dropped.
        let rows = ["#..#", "####", "#..#", "#..#"];
        let kept = label(&mask(&rows), ComponentFilter::default()).unwrap();
        assert_eq!(kept.len(), 1, "a letter shape is mostly holes and must survive");
        assert_eq!(kept[0].bounds, Rect::new(0, 0, 4, 4));
    }

    #[test]
    fn the_left_and_right_edges_are_not_joined_through_the_row_wrap() {
        // Regression guard. The north-east neighbour of the last column is off the end of the
        // row; read without a bound it lands on the first pixel of the current row, which has
        // already been labelled, and fuses the two edges of the image into one component.
        let components = label_all(&["#..#", "#..#"]);
        assert_eq!(components.len(), 2, "opposite edges of the image are not adjacent");
        assert_eq!(components[0].bounds, Rect::new(0, 0, 1, 2));
        assert_eq!(components[1].bounds, Rect::new(3, 0, 1, 2));
    }

    #[test]
    fn components_come_back_top_to_bottom_then_left_to_right() {
        // The blank row matters: without it the lower-left block touches the upper-right one
        // diagonally, and under 8-connectivity that is deliberately a single component.
        let components = label_all(&["..##", "..##", "....", "##..", "##.."]);
        assert_eq!(components.len(), 2);
        assert_eq!(components[0].bounds, Rect::new(2, 0, 2, 2));
        assert_eq!(components[1].bounds, Rect::new(0, 3, 2, 2));
    }

    #[test]
    fn single_pixel_masks_do_not_trip_the_edge_cases() {
        assert_eq!(label_all(&["#"]).len(), 1);
        assert!(label_all(&["."]).is_empty());
    }

    #[test]
    fn a_full_width_subtitle_line_labels_correctly_and_quickly() {
        // 1920x200 is a realistic subtitle plane. The assertion that matters is the component
        // count; the timing bound is a loose regression guard against an accidental quadratic,
        // set two orders of magnitude above the expected cost so it cannot flake on a slow runner.
        let width = 1920u32;
        let height = 200u32;
        let mut bits = vec![false; (width * height) as usize];

        // 60 evenly spaced 20x40 blocks, as a stand-in for a line of text.
        let mut expected = 0;
        for block in 0..60u32 {
            let x0 = block * 32;
            for y in 80..120u32 {
                for x in x0..x0 + 20 {
                    bits[(y * width + x) as usize] = true;
                }
            }
            expected += 1;
        }

        let mask = BinaryMask::from_bits(width, height, &bits).unwrap();
        let start = std::time::Instant::now();
        let components = label(&mask, ComponentFilter::default()).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(components.len(), expected);
        assert_eq!(components[0].bounds, Rect::new(0, 80, 20, 40));
        assert!(
            elapsed.as_millis() < 100,
            "labelling took {elapsed:?}, expected well under 1ms"
        );
    }

    #[test]
    fn a_full_width_subtitle_line_yields_its_label_map_quickly_too() {
        // The map costs two more passes over the plane than the boxes do, and #121 put it on the
        // extraction path. Guarded separately from the boxes because the two entry points do
        // different amounts of work on purpose.
        let (width, height) = (1920u32, 200u32);
        let mut bits = vec![false; (width * height) as usize];
        for block in 0..60u32 {
            for y in 80..120u32 {
                for x in block * 32..block * 32 + 20 {
                    bits[(y * width + x) as usize] = true;
                }
            }
        }

        let mask = BinaryMask::from_bits(width, height, &bits).unwrap();
        let start = std::time::Instant::now();
        let (components, map) = label_with_map(&mask, ComponentFilter::default()).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(components.len(), 60);
        assert_eq!(map.at(0, 80), components[0].label);
        assert_eq!(map.at(0, 0), NO_LABEL, "background carries no label");
        assert!(
            elapsed.as_millis() < 100,
            "labelling took {elapsed:?}, expected well under 1ms"
        );
    }

    #[test]
    fn a_component_the_filter_rejected_leaves_no_label_behind() {
        // Otherwise a caller walking the map would find ink that no `Component` in the list
        // accounts for, which is the same class of silent wrongness as a fabricated measurement.
        let rows = ["#.....", "......", "..####", "..####", "..####", "..####"];
        let (kept, map) = label_with_map(&mask(&rows), ComponentFilter::default()).unwrap();
        assert_eq!(kept.len(), 1, "the single-pixel speck should have been dropped");
        assert_eq!(map.at(0, 0), NO_LABEL);
        assert_eq!(map.at(2, 2), kept[0].label);
    }

    #[test]
    fn the_map_walks_a_component_and_not_its_neighbour_inside_the_same_box() {
        // The property #121 turns on. The two shapes interleave, so each one's bounding box holds
        // some of the other's ink, and only the label separates them.
        let rows = ["...#..#.", "..#..#..", ".#..#...", "#..#...."];
        let (components, map) =
            label_with_map(&mask(&rows), ComponentFilter::permissive()).unwrap();
        assert_eq!(components.len(), 2);
        let left = components.iter().min_by_key(|c| c.bounds.x).unwrap();
        assert!(left.bounds.right() > 3, "the boxes were meant to overlap");

        let mut seen = 0u64;
        map.for_each(left.label, left.bounds, |_, _| seen += 1);
        assert_eq!(seen, left.pixels, "the walk found exactly its own ink");
    }

    #[test]
    fn the_filter_drops_specks_and_full_image_boxes() {
        let filter = ComponentFilter::default();
        let image_area = 1_000;
        assert!(
            !filter.accepts(Rect::new(0, 0, 1, 1), 1, image_area),
            "a speck is not a glyph"
        );
        assert!(
            filter.accepts(Rect::new(0, 0, 8, 12), 40, image_area),
            "a glyph-sized box is"
        );
        assert!(
            !filter.accepts(Rect::new(0, 0, 40, 25), 1_000, image_area),
            "a solid box covering the whole image is a background"
        );
        assert!(
            filter.accepts(Rect::new(0, 0, 40, 25), 300, image_area),
            "but a sparse one covering the whole image is a glyph on a tight crop"
        );
    }
}
