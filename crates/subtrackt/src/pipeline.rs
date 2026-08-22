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
use subtrackt_text::correct::{ContextCorrector, CorrectionLog, NoopCorrector, PostCorrector};
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
    /// Every substitution post-correction made, in cue order.
    ///
    /// Empty unless [`Config::post_correct`] was set. It is carried out of the run rather than
    /// merely counted because a correction that leaves no trace is the failure mode the whole
    /// stage is built to avoid: a caller has to be able to see what was rewritten, not just how
    /// often.
    pub corrections: Vec<CorrectionLog>,
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
    /// Overrides the embedded set. #9 keeps the embedded one empty until a set is worth shipping,
    /// so supplying one from a file is currently the only way to match anything at all.
    reference: Option<subtrackt_glyph::ReferenceSet>,
}

impl Pipeline {
    /// Build a pipeline.
    #[must_use]
    pub const fn new(config: Config) -> Self {
        Self { config, reference: None }
    }

    /// Use `reference` instead of the embedded set.
    #[must_use]
    pub fn with_reference(mut self, reference: subtrackt_glyph::ReferenceSet) -> Self {
        self.reference = Some(reference);
        self
    }

    /// The reference set this pipeline will match against.
    #[must_use]
    pub fn reference(&self) -> subtrackt_glyph::ReferenceSet {
        self.reference.clone().unwrap_or_else(reference::embedded)
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
        // VOBSUB carries its palette out of band; without this a subpicture has colour indices
        // and no colours.
        decoder.configure(&stream.codec_private)?;
        let mut matcher = HammingMatcher::new(self.reference(), self.config.matching)?
            .with_cluster_rules(self.config.clustering);
        let segmenter =
            ImageSegmenter::new(Binarizer::new(self.config.binarize), self.config.grey_coverage);
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

        let mut corrections = Vec::new();
        let track = self.build_track(
            &images,
            &segmenter,
            &mut matcher,
            &assembler,
            &mut report,
            &mut corrections,
        )?;

        report.cues = track.cues.len().try_into().unwrap_or(u64::MAX);
        report.cache_hits = matcher.cache_hits();

        if report.is_rejected_by(self.config.unmatched) {
            return Err(Error::UnmatchedGlyph { best_distance: u32::MAX });
        }

        Ok(Outcome { track, report, corrections, stream })
    }

    /// The corrector this configuration asks for.
    ///
    /// Off is a corrector too, not an absent one. Keeping the switched-off case on the same code
    /// path means the reporting, the logging and the cue loop are identical either way, so nothing
    /// can behave differently for a reason other than the corrections themselves.
    fn corrector(&self) -> Box<dyn PostCorrector> {
        if self.config.post_correct {
            // One margin, from the matching thresholds, shared with the confidence tally the
            // assembler produced. A corrector working to a different one would rewrite glyphs the
            // report had already called clean.
            Box::new(ContextCorrector::new(self.config.matching.ambiguity_margin()))
        } else {
            Box::new(NoopCorrector)
        }
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
        corrections: &mut Vec<CorrectionLog>,
    ) -> Result<TextTrack> {
        // Segment everything before matching anything. The matcher groups a stream's own shapes
        // and matches the groups, which it cannot do while answers are already being handed out —
        // see `subtrackt_glyph::cluster` for why identifying glyphs one at a time cannot work.
        let mut per_image: Vec<Vec<subtrackt_core::Glyph>> = Vec::with_capacity(images.len());
        for image in images {
            per_image.push(segmenter.segment(image)?);
        }

        let all_glyphs: Vec<subtrackt_core::Glyph> = per_image
            .iter()
            .flat_map(|glyphs| glyphs.iter().cloned())
            .collect();
        matcher.prepare(&all_glyphs)?;
        report.distinct_shapes = matcher.distinct_shapes();
        report.clusters = matcher.clusters();
        report.glyphs_without_metrics = all_glyphs
            .iter()
            .filter(|g| !g.metrics.known)
            .count()
            .try_into()
            .unwrap_or(u64::MAX);

        let corrector = self.corrector();
        let mut cues = Vec::with_capacity(images.len());

        for (index, (image, glyphs)) in images.iter().zip(per_image).enumerate() {
            let mut identified = Vec::with_capacity(glyphs.len());
            for glyph in &glyphs {
                identified.push(matcher.match_glyph(glyph)?);
            }

            let read = assembler.assemble_annotated(image, &glyphs, &identified)?;
            let mut cue = read.cue;
            report.record(cue.confidence);

            // Post-correction, before the policy runs. It can only exchange one ambiguous
            // character for another, so it moves no glyph between the matched and unmatched
            // tallies and the gate below decides on exactly the same numbers either way. The
            // cheap pre-check keeps the corrector away from cues that were read cleanly.
            if subtrackt_text::correct::has_correctable_glyphs(cue.confidence) {
                corrector.correct(&mut cue, &read.origins, index, corrections);
            }

            // Per-cue policy. The track-level gate runs afterwards over the accumulated tally,
            // because "one unread glyph in a feature" and "40% of the track unread" deserve
            // different answers and only the second is visible at track level.
            if !cue.confidence.is_complete() && self.config.unmatched == UnmatchedPolicy::Drop {
                report.cues_dropped += 1;
                continue;
            }
            cues.push(cue);
        }

        report.corrections = corrections.len().try_into().unwrap_or(u64::MAX);
        report.corrector = corrector.name();
        Ok(TextTrack::new(cues, None))
    }
}

/// Binarize, label, group and vectorize — the [`Segmenter`] side of `subtrackt-glyph`.
pub(crate) struct ImageSegmenter {
    binarizer: Binarizer,
    /// Whether the feature vector reads ink coverage rather than the binary mask.
    grey_coverage: bool,
}

impl ImageSegmenter {
    pub(crate) const fn new(binarizer: Binarizer, grey_coverage: bool) -> Self {
        Self { binarizer, grey_coverage }
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
        use subtrackt_glyph::metrics::{self, MetricRules};

        let mask = self.mask(image);
        // Components and lines are yes-or-no questions and need the binary mask. Only the feature
        // vector reads the coverage plane, and only when asked to.
        let coverage = self.grey_coverage.then(|| self.binarizer.coverage(image));
        let components = ccl::label(&mask, ComponentFilter::default())?;
        let bands = group::line_bands(&mask);
        let lines = group::assign_lines(&mask, &components)?;
        let grouped = group::group(&components, &lines, GroupingRules::default())?;

        // Where each glyph stands in its line, which the feature vector cannot express and which is
        // the only thing separating `o` from `O`. Measured per line, from that line's own ink.
        let measured = metrics::measure_all(&bands, &grouped, MetricRules::default());

        grouped
            .iter()
            .zip(measured)
            .map(|(glyph, line_metrics)| {
                let bounds = glyph
                    .parts
                    .iter()
                    .map(|p| p.bounds)
                    .reduce(subtrackt_core::Rect::union)
                    .unwrap_or_default();
                Ok(subtrackt_core::Glyph {
                    bounds,
                    line: glyph.line,
                    features: coverage.as_ref().map_or_else(
                        || feature::vectorize(&mask, bounds, AspectPolicy::default()),
                        |c| feature::vectorize_coverage(c, bounds, AspectPolicy::default()),
                    )?,
                    metrics: line_metrics,
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
        let segmenter = ImageSegmenter::new(Binarizer::default(), false);
        let bitmap = segmenter.binarize(&image()).unwrap();
        assert_eq!(bitmap.width(), 8);
        assert_eq!(bitmap.get(3, 3), Some(1), "inside the block is foreground");
        assert_eq!(bitmap.get(0, 0), Some(0), "outside it is not");
    }

    #[test]
    fn segmentation_carries_a_component_through_to_a_feature_vector() {
        let segmenter = ImageSegmenter::new(Binarizer::default(), false);
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
