//! Wiring the stages together.
//!
//! The control flow here is complete: open, select a stream, decode packets, segment, match,
//! assemble, gate, write. The stages it calls into are at varying stages of completion, so a run
//! against a real file currently stops at the first unimplemented one and says which issue tracks
//! it. That is the point of building it this way round — the shape of the pipeline is settled and
//! testable before the expensive parts exist, and each stage can be dropped in without touching
//! the others.

use std::path::Path;

use subtrackt_core::{Error, GlyphMatcher, Result, Segmenter, SubtitleImage, TextTrack};
use subtrackt_demux::{StreamInfo, SubtitleSource};
use subtrackt_glyph::binarize::{Binarizer, BinaryMask};
use subtrackt_glyph::matcher::HammingMatcher;
use subtrackt_glyph::reference;
use subtrackt_text::layout::SpatialAssembler;
use subtrackt_text::writer_for;

use crate::config::{Config, UnmatchedPolicy};
use crate::report::Report;

/// The result of a run.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The extracted track.
    pub track: TextTrack,
    /// Counters and the gate decision.
    pub report: Report,
    /// The stream that was read.
    pub stream: StreamInfo,
}

impl Outcome {
    /// Serialise the track in the configured format.
    ///
    /// # Errors
    /// Propagates writer failures.
    pub fn render(&self, config: &Config) -> Result<String> {
        writer_for(config.format).to_string(&self.track)
    }
}

/// Runs the extraction pipeline.
pub struct Pipeline {
    config: Config,
}

impl Pipeline {
    /// Build a pipeline.
    #[must_use]
    pub const fn new(config: Config) -> Self {
        Self { config }
    }

    /// The configuration in force.
    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
    }

    /// List the bitmap subtitle streams in a file without decoding anything.
    ///
    /// # Errors
    /// Propagates demux failures.
    pub fn list(path: impl AsRef<Path>) -> Result<Vec<StreamInfo>> {
        Ok(subtrackt_demux::open(path)?.streams().to_vec())
    }

    /// Extract a track from a file.
    ///
    /// # Errors
    /// Propagates any stage failure. Notably, if the configured [`UnmatchedPolicy`] rejects the
    /// track, this returns [`Error::UnmatchedGlyph`] rather than a partial track — the caller is
    /// expected to fall back to burn-in rather than to ship half a subtitle. Every counter behind
    /// that decision is in [`Report`], so the caller can log why it happened.
    pub fn run(&self, path: impl AsRef<Path>) -> Result<Outcome> {
        let path = path.as_ref();
        let mut source = subtrackt_demux::open(path)?;

        let stream = self.choose_stream(source.as_ref())?;
        source.select(stream.index)?;

        let mut decoder = subtrackt_decode::decoder_for(stream.codec.ffmpeg_name())?;
        let mut matcher = HammingMatcher::new(reference::embedded(), self.config.matching)?;
        let segmenter = ImageSegmenter::new(Binarizer::new(self.config.binarize));
        let assembler = SpatialAssembler::new(self.config.layout_rules());

        let mut report = Report {
            reference_set: matcher.references().name().to_owned(),
            ..Report::default()
        };
        let mut images = Vec::new();

        while let Some(packet) = source.next_packet()? {
            report.packets += 1;
            images.extend(decoder.push(packet.pts, &packet.payload)?);
        }
        images.extend(decoder.finish()?);
        report.images = images.len().try_into().unwrap_or(u64::MAX);

        let track = self.build_track(&images, &segmenter, &mut matcher, &assembler, &mut report)?;

        report.cues = track.cues.len().try_into().unwrap_or(u64::MAX);
        report.cache_hits = matcher.cache_hits();

        if report.is_rejected_by(self.config.unmatched) {
            return Err(Error::UnmatchedGlyph { best_distance: u32::MAX });
        }

        Ok(Outcome { track, report, stream })
    }

    /// Pick the configured stream, or the first one.
    pub(crate) fn choose_stream(&self, source: &dyn SubtitleSource) -> Result<StreamInfo> {
        let streams = source.streams();
        match self.config.stream {
            Some(index) => streams
                .iter()
                .find(|s| s.index == index)
                .cloned()
                .ok_or_else(|| Error::Demux(format!("no subtitle stream with index {index}"))),
            None => streams
                .first()
                .cloned()
                .ok_or_else(|| Error::Demux("no bitmap subtitle stream found".into())),
        }
    }

    /// Segment, match and assemble every image into cues, applying the unmatched-glyph policy.
    fn build_track(
        &self,
        images: &[SubtitleImage],
        segmenter: &ImageSegmenter,
        matcher: &mut HammingMatcher,
        assembler: &SpatialAssembler,
        report: &mut Report,
    ) -> Result<TextTrack> {
        use subtrackt_core::TextAssembler;

        let mut cues = Vec::with_capacity(images.len());

        for image in images {
            let glyphs = segmenter.segment(image)?;

            let mut identified = Vec::with_capacity(glyphs.len());
            for glyph in &glyphs {
                identified.push(matcher.match_glyph(glyph)?);
            }

            let cue = assembler.assemble(image, &glyphs, &identified)?;
            report.record(cue.confidence);

            // Per-cue policy. The track-level gate runs afterwards over the accumulated tally,
            // because "one unread glyph in a feature" and "40% of the track unread" deserve
            // different answers and only the second is visible at track level.
            if !cue.confidence.is_complete() && self.config.unmatched == UnmatchedPolicy::Drop {
                report.cues_dropped += 1;
                continue;
            }
            cues.push(cue);
        }

        Ok(TextTrack::new(cues, None))
    }
}

/// Binarize, label, group and vectorize — the [`Segmenter`] side of `subtrackt-glyph`.
pub(crate) struct ImageSegmenter {
    binarizer: Binarizer,
}

impl ImageSegmenter {
    pub(crate) const fn new(binarizer: Binarizer) -> Self {
        Self { binarizer }
    }

    /// The foreground mask for an image.
    fn mask(&self, image: &SubtitleImage) -> BinaryMask {
        self.binarizer.mask(image)
    }
}

impl Segmenter for ImageSegmenter {
    fn segment(&self, image: &SubtitleImage) -> Result<Vec<subtrackt_core::Glyph>> {
        use subtrackt_glyph::ccl::{self, ComponentFilter};
        use subtrackt_glyph::feature::{self, AspectPolicy};
        use subtrackt_glyph::group::{self, GroupingRules};

        let mask = self.mask(image);
        let components = ccl::label(&mask, ComponentFilter::default())?;
        let lines = group::assign_lines(&mask, &components)?;
        let grouped = group::group(&components, &lines, GroupingRules::default())?;

        grouped
            .iter()
            .map(|glyph| {
                let bounds = glyph
                    .parts
                    .iter()
                    .map(|p| p.bounds)
                    .reduce(subtrackt_core::Rect::union)
                    .unwrap_or_default();
                Ok(subtrackt_core::Glyph {
                    bounds,
                    line: glyph.line,
                    features: feature::vectorize(&mask, bounds, AspectPolicy::default())?,
                })
            })
            .collect()
    }

    fn binarize(&self, image: &SubtitleImage) -> Result<subtrackt_core::IndexedBitmap> {
        self.binarizer.mask_as_bitmap(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{IndexedBitmap, Palette, PaletteEntry, Rect, TimeSpan, Timestamp};

    /// An 8x8 image holding one 4x4 block: big enough to survive the component area filter, and
    /// small enough relative to the image to survive the coverage filter.
    fn image() -> SubtitleImage {
        let mut palette = Palette::transparent(2);
        palette.set(1, PaletteEntry { y: 235, cb: 128, cr: 128, alpha: 255 });

        let pixels: Vec<u8> = (0..8)
            .flat_map(|y| (0..8).map(move |x| u8::from((2..6).contains(&x) && (2..6).contains(&y))))
            .collect();

        SubtitleImage {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            position: Rect::new(0, 0, 8, 8),
            bitmap: IndexedBitmap::new(8, 8, pixels).unwrap(),
            palette,
            forced: false,
        }
    }

    #[test]
    fn the_binarizer_runs_end_to_end_inside_the_segmenter() {
        let segmenter = ImageSegmenter::new(Binarizer::default());
        let bitmap = segmenter.binarize(&image()).unwrap();
        assert_eq!(bitmap.width(), 8);
        assert_eq!(bitmap.get(3, 3), Some(1), "inside the block is foreground");
        assert_eq!(bitmap.get(0, 0), Some(0), "outside it is not");
    }

    #[test]
    fn segmentation_carries_a_component_through_to_a_feature_vector() {
        let segmenter = ImageSegmenter::new(Binarizer::default());
        // Segmentation is complete now, so this no longer fails at all. What the test still
        // pins is that a glyph-sized component makes it all the way to a feature vector.
        let glyphs = segmenter.segment(&image()).unwrap();
        assert_eq!(glyphs.len(), 1, "the 4x4 block is one glyph");
        assert!(
            glyphs[0].features.popcount() > 0,
            "and it vectorizes to something non-empty"
        );
    }

    #[test]
    fn a_missing_input_file_fails_at_demux_not_deeper_in() {
        let err = Pipeline::new(Config::default())
            .run("no_such_file.sup")
            .unwrap_err();
        assert!(matches!(err, Error::Io { .. }), "got {err:?}");
    }

    #[test]
    fn an_unknown_extension_is_refused_before_anything_is_read() {
        let err = Pipeline::list("subtitles.avi").unwrap_err();
        assert!(matches!(err, Error::Demux(_)), "got {err:?}");
    }
}
