//! Blu-ray Presentation Graphic Stream.
//!
//! PGS is a display-set protocol rather than a sequence of self-contained frames. Segments arrive
//! in groups terminated by an END segment, and palettes, windows and objects defined by one group
//! persist into later ones. A group carrying composition objects puts something on screen; a group
//! carrying none clears it, and that clear is what supplies the *end* timestamp of the cue before
//! it. That is why this holds state, and why an image is only emitted once the following display
//! set has been seen.

pub mod body;
pub mod rle;
pub mod segment;

use std::collections::BTreeMap;

use subtrackt_core::{
    BitmapDecoder, Error, IndexedBitmap, Palette, Rect, Result, SubtitleImage, TimeSpan, Timestamp,
};

pub use body::{Composition, CompositionObject, ObjectFragment, PaletteUpdate};
pub use segment::{SegmentKind, SegmentRef};

/// How long a trailing cue is held when the stream ends without clearing it.
///
/// Streams essentially always end with a clear. When one does not, the final cue has no end time
/// in the data at all. Dropping it loses a line of dialogue; holding it for a fixed interval gets
/// the text out with approximate timing. The text is what this tool exists to recover, so it is
/// emitted — and [`BitmapDecoder::unterminated_cues`] counts how often the fallback was
/// needed, which is the condition `CLAUDE.md` attaches to being allowed to approximate at all.
pub const UNTERMINATED_CUE_TICKS: u64 = 5 * subtrackt_core::PTS_HZ;

/// An object being reassembled across fragments, or one already decoded.
#[derive(Debug, Clone)]
struct StoredObject {
    width: u32,
    height: u32,
    /// Total run-length bytes the first fragment promised.
    total_len: usize,
    data: Vec<u8>,
    complete: bool,
}

/// A composed image waiting for the display set that will end it.
#[derive(Debug, Clone)]
struct OpenImage {
    start: u64,
    position: Rect,
    bitmap: IndexedBitmap,
    palette: Palette,
    forced: bool,
}

/// Decoder state for one PGS stream.
pub struct PgsDecoder {
    palettes: BTreeMap<u8, Palette>,
    objects: BTreeMap<u16, StoredObject>,
    /// The composition of the display set currently being read.
    composition: Option<Composition>,
    open: Option<OpenImage>,
    segments_seen: u64,
    ended_mid_display_set: bool,
    unterminated_cues: u64,
}

impl PgsDecoder {
    /// A decoder with no state yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            palettes: BTreeMap::new(),
            objects: BTreeMap::new(),
            composition: None,
            open: None,
            segments_seen: 0,
            ended_mid_display_set: false,
            unterminated_cues: 0,
        }
    }

    /// Number of segments fed to this decoder, for diagnostics.
    #[must_use]
    pub const fn segments_seen(&self) -> u64 {
        self.segments_seen
    }

    /// Whether the stream ended part-way through a display set.
    #[must_use]
    pub const fn ended_mid_display_set(&self) -> bool {
        self.ended_mid_display_set
    }

    /// The palette a composition would draw with, or a transparent one if never defined.
    #[must_use]
    pub fn palette(&self, id: u8) -> Palette {
        self.palettes
            .get(&id)
            .cloned()
            .unwrap_or_else(|| Palette::transparent(256))
    }

    /// Apply one framed segment, returning any image it completed.
    fn apply(&mut self, segment: &SegmentRef<'_>) -> Result<Option<SubtitleImage>> {
        let pts = segment.pts;
        match segment.kind {
            SegmentKind::PresentationComposition => {
                self.composition = Some(body::composition(segment.body, pts)?);
            }
            SegmentKind::WindowDefinition => {
                // Parsed and dropped. A window definition has to be *validated* -- a malformed one
                // is a malformed stream and should be rejected here rather than downstream -- but
                // the rectangles themselves were only ever stored. Nothing composited against
                // them: `compose` places objects at their own coordinates and unions the result.
                body::windows(segment.body, pts)?;
            }
            SegmentKind::PaletteDefinition => {
                let update = body::palette(segment.body, pts)?;
                // Palette definitions are incremental: slots this segment does not mention keep
                // whatever an earlier one set, so the entry is merged rather than replaced.
                let palette = self
                    .palettes
                    .entry(update.id)
                    .or_insert_with(|| Palette::transparent(256));
                for (index, entry) in update.entries {
                    palette.set(index, entry);
                }
            }
            SegmentKind::ObjectDefinition => {
                self.store_object(&body::object(segment.body, pts)?, pts)?;
            }
            SegmentKind::End => return self.end_display_set(pts),
        }
        Ok(None)
    }

    /// Accumulate one object fragment, reassembling across segments.
    fn store_object(&mut self, fragment: &ObjectFragment<'_>, pts: u64) -> Result<()> {
        if fragment.first {
            let (width, height) = fragment.dimensions.unwrap_or((0, 0));
            let total_len = fragment.total_len.unwrap_or(0);
            self.objects.insert(
                fragment.id,
                StoredObject {
                    width,
                    height,
                    total_len,
                    data: fragment.data.to_vec(),
                    complete: false,
                },
            );
        } else {
            let object =
                self.objects
                    .get_mut(&fragment.id)
                    .ok_or_else(|| Error::MalformedPacket {
                        codec: "pgs",
                        pts,
                        reason: format!(
                            "object {} continues a sequence that never started",
                            fragment.id
                        ),
                    })?;
            object.data.extend_from_slice(fragment.data);
        }

        if fragment.last {
            let object = self
                .objects
                .get_mut(&fragment.id)
                .expect("just inserted or fetched");
            if object.data.len() != object.total_len {
                return Err(Error::MalformedPacket {
                    codec: "pgs",
                    pts,
                    reason: format!(
                        "object {} promised {} bytes of run-length data but {} arrived",
                        fragment.id,
                        object.total_len,
                        object.data.len()
                    ),
                });
            }
            object.complete = true;
        }
        Ok(())
    }

    /// Handle an END segment: close whatever was on screen, and open whatever replaces it.
    fn end_display_set(&mut self, pts: u64) -> Result<Option<SubtitleImage>> {
        let Some(composition) = self.composition.take() else {
            // An END with no composition before it defines nothing. Real streams do not do this,
            // but ignoring it is friendlier than failing a whole track over a stray segment.
            return Ok(None);
        };

        if composition.is_clear() {
            return Ok(self.open.take().map(|open| finish(open, pts)));
        }

        let image = self.compose(&composition, pts)?;

        // A palette-only update re-composes the same pixels under a new palette, which is how
        // fades and karaoke wipes are authored. Emitting a fresh cue per step would multiply one
        // line of dialogue into dozens, so an unchanged bitmap extends the open cue instead.
        if let Some(open) = self.open.as_ref()
            && open.bitmap == image.bitmap
            && open.position == image.position
        {
            return Ok(None);
        }

        let closed = self.open.take().map(|open| finish(open, pts));
        self.open = Some(image);
        Ok(closed)
    }

    /// Draw a composition's objects into a single bitmap.
    ///
    /// A display set can place several objects — commonly one window at the top of the frame and
    /// one at the bottom. They belong to the same moment of dialogue, so they are composited into
    /// one image rather than emitted separately; downstream, one image is one cue.
    fn compose(&self, composition: &Composition, pts: u64) -> Result<OpenImage> {
        let mut placements = Vec::with_capacity(composition.objects.len());

        for object in &composition.objects {
            let stored =
                self.objects
                    .get(&object.object_id)
                    .ok_or_else(|| Error::MalformedPacket {
                        codec: "pgs",
                        pts,
                        reason: format!(
                            "composition references object {} which was never defined",
                            object.object_id
                        ),
                    })?;
            if !stored.complete {
                return Err(Error::MalformedPacket {
                    codec: "pgs",
                    pts,
                    reason: format!(
                        "composition references object {} before its data finished arriving",
                        object.object_id
                    ),
                });
            }

            let pixels = rle::decode(&stored.data, stored.width, stored.height).map_err(|e| {
                Error::MalformedPacket {
                    codec: "pgs",
                    pts,
                    reason: format!("object {}: {e}", object.object_id),
                }
            })?;

            // A crop rectangle outside the object is a stream bug; clamping keeps a usable image
            // rather than discarding a cue over a few pixels.
            let crop = object
                .crop
                .map_or(Rect::new(0, 0, stored.width, stored.height), |c| {
                    Rect::new(
                        c.x.min(stored.width),
                        c.y.min(stored.height),
                        c.width
                            .min(stored.width.saturating_sub(c.x.min(stored.width))),
                        c.height
                            .min(stored.height.saturating_sub(c.y.min(stored.height))),
                    )
                });

            placements.push((
                Rect::new(object.x, object.y, crop.width, crop.height),
                crop,
                stored.width,
                pixels,
            ));
        }

        let bounds = placements
            .iter()
            .map(|(place, ..)| *place)
            .reduce(Rect::union)
            .unwrap_or_default();

        let mut canvas = IndexedBitmap::blank(bounds.width, bounds.height);
        let stride = bounds.width as usize;

        for (place, crop, source_stride, pixels) in &placements {
            let source_stride = *source_stride as usize;
            let offset_x = (place.x - bounds.x) as usize;
            let offset_y = (place.y - bounds.y) as usize;

            for row in 0..crop.height as usize {
                let src = (crop.y as usize + row) * source_stride + crop.x as usize;
                let dst = (offset_y + row) * stride + offset_x;
                let width = crop.width as usize;
                if src + width <= pixels.len() && dst + width <= stride * bounds.height as usize {
                    canvas.pixels_mut()[dst..dst + width]
                        .copy_from_slice(&pixels[src..src + width]);
                }
            }
        }

        Ok(OpenImage {
            start: pts,
            position: bounds,
            bitmap: canvas,
            palette: self.palette(composition.palette_id),
            forced: composition.objects.iter().any(|o| o.forced),
        })
    }
}

/// Close an open image at `end`.
fn finish(open: OpenImage, end: u64) -> SubtitleImage {
    SubtitleImage {
        span: TimeSpan::new(Timestamp::from_ticks(open.start), Timestamp::from_ticks(end)),
        position: open.position,
        bitmap: open.bitmap,
        palette: open.palette,
        forced: open.forced,
    }
}

impl Default for PgsDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl BitmapDecoder for PgsDecoder {
    fn codec(&self) -> &'static str {
        "pgs"
    }

    fn push(&mut self, pts: u64, payload: &[u8]) -> Result<Vec<SubtitleImage>> {
        let mut out = Vec::new();

        // A Matroska PGS packet, and a .sup segment as re-emitted by the sup reader, may hold
        // more than one segment back to back.
        for segment in segment::iter(pts, payload) {
            let segment = segment?;
            self.segments_seen += 1;
            if let Some(image) = self.apply(&segment)? {
                out.push(image);
            }
        }

        Ok(out)
    }

    fn unterminated_cues(&self) -> u64 {
        self.unterminated_cues
    }

    fn finish(&mut self) -> Result<Vec<SubtitleImage>> {
        self.ended_mid_display_set = self.composition.take().is_some();

        let Some(open) = self.open.take() else {
            return Ok(Vec::new());
        };
        self.unterminated_cues += 1;
        let end = open.start + UNTERMINATED_CUE_TICKS;
        Ok(vec![finish(open, end)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::PaletteEntry;

    /// One segment in Matroska layout: type byte, big-endian length, body.
    fn seg(kind: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![kind];
        out.extend_from_slice(&u16::try_from(body.len()).unwrap().to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    /// A composition placing `objects` as `(id, x, y)`, or clearing the screen when empty.
    fn pcs(objects: &[(u16, u32, u32)], forced: bool) -> Vec<u8> {
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
            b.push(if forced { 0x40 } else { 0x00 });
            b.extend_from_slice(&u16::try_from(*x).unwrap().to_be_bytes());
            b.extend_from_slice(&u16::try_from(*y).unwrap().to_be_bytes());
        }
        b
    }

    /// A palette defining one opaque white entry at `index`.
    fn pds(index: u8) -> Vec<u8> {
        vec![0, 0, index, 235, 128, 128, 255]
    }

    /// A complete object carrying `pixels`, optionally split into two fragments.
    fn ods(id: u16, width: u32, height: u32, pixels: &[u8], split: bool) -> Vec<Vec<u8>> {
        let data = rle::encode(pixels, width, height);
        let mut head = Vec::new();
        head.extend_from_slice(&id.to_be_bytes());
        head.push(0);

        if !split {
            head.push(0xC0);
            let declared = u32::try_from(data.len() + 4).unwrap();
            head.extend_from_slice(&declared.to_be_bytes()[1..]);
            head.extend_from_slice(&u16::try_from(width).unwrap().to_be_bytes());
            head.extend_from_slice(&u16::try_from(height).unwrap().to_be_bytes());
            head.extend_from_slice(&data);
            return vec![head];
        }

        let cut = data.len() / 2;
        head.push(0x80); // first only
        let declared = u32::try_from(data.len() + 4).unwrap();
        head.extend_from_slice(&declared.to_be_bytes()[1..]);
        head.extend_from_slice(&u16::try_from(width).unwrap().to_be_bytes());
        head.extend_from_slice(&u16::try_from(height).unwrap().to_be_bytes());
        head.extend_from_slice(&data[..cut]);

        let mut tail = Vec::new();
        tail.extend_from_slice(&id.to_be_bytes());
        tail.push(0);
        tail.push(0x40); // last only
        tail.extend_from_slice(&data[cut..]);

        vec![head, tail]
    }

    /// A display set that shows `pixels`, as one packet.
    fn show(width: u32, height: u32, pixels: &[u8], split: bool, forced: bool) -> Vec<u8> {
        let mut packet = seg(0x16, &pcs(&[(1, 10, 20)], forced));
        packet.extend_from_slice(&seg(0x14, &pds(1)));
        for fragment in ods(1, width, height, pixels, split) {
            packet.extend_from_slice(&seg(0x15, &fragment));
        }
        packet.extend_from_slice(&seg(0x80, &[]));
        packet
    }

    /// A display set that clears the screen.
    fn clear() -> Vec<u8> {
        let mut packet = seg(0x16, &pcs(&[], false));
        packet.extend_from_slice(&seg(0x80, &[]));
        packet
    }

    #[test]
    fn a_show_then_clear_pair_produces_one_timed_image() {
        let mut d = PgsDecoder::new();
        let pixels = vec![1, 0, 1, 0, 0, 1, 0, 1];

        assert!(
            d.push(90_000, &show(4, 2, &pixels, false, false))
                .unwrap()
                .is_empty()
        );
        let images = d.push(180_000, &clear()).unwrap();

        assert_eq!(images.len(), 1);
        let image = &images[0];
        assert_eq!(image.span.start.ticks(), 90_000);
        assert_eq!(image.span.end.ticks(), 180_000);
        assert_eq!(image.bitmap.pixels(), pixels.as_slice());
        assert_eq!(image.position, Rect::new(10, 20, 4, 2));
        assert!(!image.forced);
    }

    #[test]
    fn the_palette_reaches_the_image_with_chroma_the_right_way_round() {
        let mut d = PgsDecoder::new();
        d.push(0, &show(2, 1, &[1, 1], false, false)).unwrap();
        let images = d.push(90_000, &clear()).unwrap();

        let entry = images[0].palette.get(1);
        assert_eq!(entry, PaletteEntry { y: 235, cb: 128, cr: 128, alpha: 255 });
        assert_eq!(entry.to_rgba().a, 255);
    }

    #[test]
    fn an_object_split_across_fragments_is_reassembled() {
        let pixels: Vec<u8> = (0..64u32).map(|v| u8::try_from(v % 5).unwrap()).collect();

        let mut whole = PgsDecoder::new();
        whole.push(0, &show(8, 8, &pixels, false, false)).unwrap();
        let a = whole.push(90_000, &clear()).unwrap();

        let mut split = PgsDecoder::new();
        split.push(0, &show(8, 8, &pixels, true, false)).unwrap();
        let b = split.push(90_000, &clear()).unwrap();

        assert_eq!(
            a[0].bitmap, b[0].bitmap,
            "fragmenting must not change the decoded object"
        );
        assert_eq!(b[0].bitmap.pixels(), pixels.as_slice());
    }

    #[test]
    fn the_forced_flag_survives_to_the_image() {
        let mut d = PgsDecoder::new();
        d.push(0, &show(2, 1, &[1, 1], false, true)).unwrap();
        assert!(d.push(90_000, &clear()).unwrap()[0].forced);
    }

    #[test]
    fn a_new_composition_closes_the_previous_cue_without_a_clear_between() {
        let mut d = PgsDecoder::new();
        d.push(0, &show(2, 1, &[1, 1], false, false)).unwrap();
        let images = d.push(90_000, &show(2, 1, &[1, 0], false, false)).unwrap();

        assert_eq!(images.len(), 1, "the first cue must close when the second opens");
        assert_eq!(images[0].span.end.ticks(), 90_000);
    }

    #[test]
    fn a_palette_only_update_extends_the_cue_rather_than_duplicating_it() {
        // Fades are authored as repeated compositions of identical pixels under new palettes.
        // Emitting one cue per step would multiply a single line into dozens.
        let mut d = PgsDecoder::new();
        d.push(0, &show(2, 1, &[1, 1], false, false)).unwrap();

        for step in 1..5 {
            let emitted = d
                .push(step * 9_000, &show(2, 1, &[1, 1], false, false))
                .unwrap();
            assert!(emitted.is_empty(), "step {step} should not have emitted a cue");
        }

        let images = d.push(90_000, &clear()).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].span.start.ticks(), 0, "the cue keeps its original start");
        assert_eq!(images[0].span.end.ticks(), 90_000);
    }

    #[test]
    fn two_objects_composite_into_one_image_rather_than_two_cues() {
        // Top and bottom windows belong to the same moment of dialogue.
        let mut packet = seg(0x16, &pcs(&[(1, 0, 0), (2, 0, 10)], false));
        packet.extend_from_slice(&seg(0x14, &pds(1)));
        for fragment in ods(1, 4, 1, &[1, 1, 1, 1], false) {
            packet.extend_from_slice(&seg(0x15, &fragment));
        }
        for fragment in ods(2, 4, 1, &[2, 2, 2, 2], false) {
            packet.extend_from_slice(&seg(0x15, &fragment));
        }
        packet.extend_from_slice(&seg(0x80, &[]));

        let mut d = PgsDecoder::new();
        d.push(0, &packet).unwrap();
        let images = d.push(90_000, &clear()).unwrap();

        assert_eq!(images.len(), 1, "one display set is one cue");
        let image = &images[0];
        assert_eq!(image.position, Rect::new(0, 0, 4, 11));
        assert_eq!(image.bitmap.get(0, 0), Some(1));
        assert_eq!(image.bitmap.get(0, 10), Some(2));
        assert_eq!(
            image.bitmap.get(0, 5),
            Some(0),
            "the gap between windows stays transparent"
        );
    }

    #[test]
    fn a_composition_referencing_an_undefined_object_is_malformed() {
        let mut packet = seg(0x16, &pcs(&[(9, 0, 0)], false));
        packet.extend_from_slice(&seg(0x80, &[]));

        let err = PgsDecoder::new().push(1_234, &packet).unwrap_err();
        match err {
            Error::MalformedPacket { pts, reason, .. } => {
                assert_eq!(pts, 1_234);
                assert!(reason.contains("never defined"), "{reason}");
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn an_object_whose_data_does_not_match_its_declared_length_is_malformed() {
        let mut fragments = ods(1, 4, 1, &[1, 1, 1, 1], false);
        fragments[0].extend_from_slice(&[0x00, 0x00]); // extra bytes beyond the declared length

        let mut packet = seg(0x16, &pcs(&[(1, 0, 0)], false));
        packet.extend_from_slice(&seg(0x15, &fragments[0]));
        packet.extend_from_slice(&seg(0x80, &[]));

        let err = PgsDecoder::new().push(7, &packet).unwrap_err();
        assert!(matches!(err, Error::MalformedPacket { .. }), "got {err:?}");
    }

    #[test]
    fn a_continuation_without_a_first_fragment_is_malformed() {
        let mut tail = Vec::new();
        tail.extend_from_slice(&1u16.to_be_bytes());
        tail.push(0);
        tail.push(0x40);
        tail.extend_from_slice(&[1, 2, 3]);

        let err = PgsDecoder::new().push(0, &seg(0x15, &tail)).unwrap_err();
        assert!(matches!(err, Error::MalformedPacket { .. }), "got {err:?}");
    }

    #[test]
    fn a_stream_that_never_clears_still_yields_its_last_cue() {
        let mut d = PgsDecoder::new();
        d.push(90_000, &show(2, 1, &[1, 1], false, false)).unwrap();

        let images = d.finish().unwrap();
        assert_eq!(images.len(), 1, "dropping it would lose a line of dialogue");
        assert_eq!(images[0].span.end.ticks(), 90_000 + UNTERMINATED_CUE_TICKS);
        assert_eq!(d.unterminated_cues(), 1, "the approximation must be countable");
    }

    #[test]
    fn the_invented_end_time_survives_being_boxed_as_a_trait_object() {
        // Countable is not the same as *reachable*, and #147 is the difference. The count above
        // was an inherent method, and `decoder_for` hands the pipeline a `Box<dyn BitmapDecoder>`
        // — so the one caller that has to audit the approximation was the one caller that could
        // not see it. `CLAUDE.md` permits approximating here only on the condition that it is
        // counted so it can be audited, which makes this test the condition rather than a nicety.
        let mut decoder: Box<dyn BitmapDecoder> = Box::new(PgsDecoder::new());
        decoder
            .push(90_000, &show(2, 1, &[1, 1], false, false))
            .unwrap();
        assert_eq!(decoder.unterminated_cues(), 0, "nothing has been invented yet");

        let images = decoder.finish().unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(decoder.unterminated_cues(), 1);
    }

    #[test]
    fn an_unterminated_display_set_is_dropped_at_finish_without_erroring() {
        let mut d = PgsDecoder::new();
        d.push(9_000, &seg(0x16, &pcs(&[(1, 0, 0)], false)))
            .unwrap();
        assert!(d.finish().unwrap().is_empty());
        assert!(
            d.ended_mid_display_set(),
            "the truncation must be visible to the caller"
        );
    }

    #[test]
    fn segments_are_counted_as_they_are_framed() {
        let mut d = PgsDecoder::new();
        let mut packet = seg(0x14, &pds(1));
        packet.extend_from_slice(&seg(0x17, &[0]));
        d.push(9_000, &packet).unwrap();
        assert_eq!(d.segments_seen(), 2);
    }

    #[test]
    fn an_undefined_palette_reads_back_transparent() {
        assert_eq!(PgsDecoder::new().palette(0).get(1).alpha, 0);
    }
}
