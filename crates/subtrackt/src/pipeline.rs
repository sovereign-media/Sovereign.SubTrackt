//! Wiring the stages together.
//!
//! The control flow here is complete: open, select a stream, decode packets, segment, match,
//! assemble, gate, write. The stages it calls into are at varying stages of completion, so a run
//! against a real file currently stops at the first unimplemented one and says which issue tracks
//! it. That is the point of building it this way round — the shape of the pipeline is settled and
//! testable before the expensive parts exist, and each stage can be dropped in without touching
//! the others.

use std::path::Path;

use subtrackt_core::progress::{Phase, Progress, Silent};
use subtrackt_core::{
    Error, GlyphMatcher, LineMetrics, Rect, Result, Segmenter, SubtitleImage, TextTrack,
};
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
    /// Every glyph the matcher would not name, in cue order.
    ///
    /// Counted in [`Report::unmatched`] as well; this says *which*. #98 is the reason it is carried
    /// rather than merely tallied, and the argument is the one `corrections` already makes: the
    /// project's whole thesis is that an unmatched glyph is a **fact** rather than a confidence
    /// score, and a fact a caller cannot inspect is doing only half its job. A user whose reference
    /// set is missing a character learns which one from here and from nowhere else.
    ///
    /// Always collected, with no flag to switch it off, because an unread glyph is by construction
    /// rare — a few percent of a track — and a feature film's worth is tens of kilobytes against the
    /// tens of thousands of glyphs the run already holds resident.
    pub unread: Vec<UnreadGlyph>,
    /// The stream that was read.
    pub stream: StreamInfo,
}

/// One glyph that matched nothing, and everything known about it that is not its shape.
///
/// Deliberately not the [`FeatureVector`](subtrackt_core::FeatureVector). The vector is a lossy
/// 16x16 projection and says nothing a reader can act on; what a reader needs is where the glyph
/// sat, how big it was, and whether its line could be measured at all — because a glyph on an
/// unmeasurable line was matched on shape alone and failed for a different reason than one that had
/// metrics and still found nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnreadGlyph {
    /// Which cue it came from, counting from zero.
    pub cue: usize,
    /// Which text line within that cue.
    pub line: usize,
    /// Where it sat, in subtitle-plane coordinates.
    pub bounds: Rect,
    /// Where it stood in its line, or [`LineMetrics::UNKNOWN`] if the line was unmeasurable.
    pub metrics: LineMetrics,
    /// How far the nearest reference entry was — the one that was still too far to accept.
    ///
    /// A glyph rejected at 52 cells against a 51-cell ceiling is a different problem from one
    /// rejected at 140, and the count alone cannot tell them apart.
    pub distance: u32,
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
    /// Overrides the embedded set. Nothing is embedded — `docs/reference-set.md` measured a
    /// shipped set as worse than none — so supplying one from a file is the only way to match
    /// anything at all.
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
    /// track, this returns [`Error::TrackRejected`] rather than a partial track — the caller is
    /// expected to fall back to burn-in rather than to ship half a subtitle. Every counter behind
    /// that decision is in [`Report`], so the caller can log why it happened.
    pub fn run(&self, path: impl AsRef<Path>) -> Result<Outcome> {
        self.run_watched(path, &Silent)
    }

    /// Extract a track, reporting where the run has got to.
    ///
    /// Identical to [`Self::run`] but for the observer. A film is minutes of work in two shapes --
    /// packets arriving with no total in sight, then every decoded image segmented, clustered and
    /// matched -- and a front end that says nothing across either is indistinguishable from a hung
    /// one. What gets drawn, and whether anything does, belongs entirely to `progress`.
    ///
    /// # Errors
    /// As [`Self::run`].
    pub fn run_watched(&self, path: impl AsRef<Path>, progress: &dyn Progress) -> Result<Outcome> {
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

        // Indeterminate: the source is streamed until it is exhausted, so how many packets there
        // are is not known until there are none left.
        progress.begin(Phase::Decode, None);
        while let Some(packet) = source.next_packet()? {
            report.packets += 1;
            progress.advance(report.packets);
            images.extend(decoder.push(packet.pts, &packet.payload)?);
        }
        images.extend(decoder.finish()?);
        progress.end();
        report.images = images.len().try_into().unwrap_or(u64::MAX);

        let mut corrections = Vec::new();
        let mut unread = Vec::new();
        let mut stages = Stages {
            segmenter: &segmenter,
            matcher: &mut matcher,
            assembler: &assembler,
        };
        let track = self.build_track(
            &images,
            &mut stages,
            &mut report,
            &mut corrections,
            &mut unread,
            progress,
        )?;

        report.cues = track.cues.len().try_into().unwrap_or(u64::MAX);
        report.cache_hits = stages.matcher.cache_hits();

        if report.is_rejected_by(self.config.unmatched) {
            // Every number behind the decision, because the caller is being told to fall back to
            // burn-in and that is expensive enough to deserve a reason.
            return Err(Error::TrackRejected {
                policy: self.config.unmatched.name(),
                matched: report.matched,
                unmatched: report.unmatched,
                required: self.config.unmatched.required_ratio(),
            });
        }

        Ok(Outcome { track, report, corrections, unread, stream })
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
            let corrector = ContextCorrector::new(self.config.matching.ambiguity_margin());
            // The vocabulary is an arm of this corrector, not a stage of its own — two correctors
            // in sequence could let one's output become the other's evidence, which is the cascade
            // `docs/post-correction.md` guarantees cannot happen.
            if self.config.track_vocabulary {
                Box::new(corrector.with_vocabulary(self.config.vocabulary))
            } else {
                Box::new(corrector)
            }
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
        stages: &mut Stages<'_>,
        report: &mut Report,
        corrections: &mut Vec<CorrectionLog>,
        unread: &mut Vec<UnreadGlyph>,
        progress: &dyn Progress,
    ) -> Result<TextTrack> {
        // Segment everything before matching anything. The matcher groups a stream's own shapes
        // and matches the groups, which it cannot do while answers are already being handed out —
        // see `subtrackt_glyph::cluster` for why identifying glyphs one at a time cannot work.
        let mut per_image: Vec<Vec<subtrackt_core::Glyph>> = Vec::with_capacity(images.len());
        // Three reported phases rather than one, because this is three passes over the same images
        // with a whole-stream barrier between them. One bar filling up three times under a single
        // label would say less than three that each name what is running.
        let total: u64 = images.len().try_into().unwrap_or(u64::MAX);
        let mut done = 0u64;
        progress.begin(Phase::Segment, Some(total));
        for image in images {
            per_image.push(stages.segmenter.segment(image)?);
            done += 1;
            progress.advance(done);
        }
        progress.end();

        let all_glyphs: Vec<subtrackt_core::Glyph> = per_image
            .iter()
            .flat_map(|glyphs| glyphs.iter().cloned())
            .collect();
        // One call with no way to see inside it, so a spinner rather than a bar. It still earns
        // its line: on a feature film the grouping pass is seconds of otherwise silent work.
        progress.begin(Phase::Cluster, None);
        stages.matcher.prepare(&all_glyphs)?;
        progress.end();
        report.distinct_shapes = stages.matcher.distinct_shapes();
        report.clusters = stages.matcher.clusters();
        report.glyphs_without_metrics = all_glyphs
            .iter()
            .filter(|g| !g.metrics.known)
            .count()
            .try_into()
            .unwrap_or(u64::MAX);

        // Assemble every cue before correcting any of them. #60's vocabulary arm needs the whole
        // track's clear tokens, and a decision needing the whole track cannot be made while
        // answers are already being handed out — the same argument that makes `matcher.prepare` a
        // separate pass. Every image is already resident, so this costs nothing but the ordering.
        let mut read_cues = Vec::with_capacity(images.len());
        done = 0;
        progress.begin(Phase::Read, Some(total));
        for (cue, (image, glyphs)) in images.iter().zip(per_image).enumerate() {
            let mut identified = Vec::with_capacity(glyphs.len());
            for glyph in &glyphs {
                identified.push(stages.matcher.match_glyph(glyph)?);
            }

            // Named here rather than counted, because this is the one place a glyph and the answer
            // it did not get are both in hand. See `Outcome::unread`.
            unread.extend(
                glyphs
                    .iter()
                    .zip(&identified)
                    .filter(|(_, m)| m.character.is_none())
                    .map(|(glyph, m)| UnreadGlyph {
                        cue,
                        line: glyph.line,
                        bounds: glyph.bounds,
                        metrics: glyph.metrics,
                        distance: m.distance,
                    }),
            );

            // How well the matched glyphs fitted, not just how many did. See `Report::distance_sum`
            // for why the second number cannot stand in for the first.
            report.distance_sum += identified
                .iter()
                .filter(|m| m.character.is_some())
                .map(|m| u64::from(m.distance))
                .sum::<u64>();

            let read = stages
                .assembler
                .assemble_annotated(image, &glyphs, &identified)?;
            report.record(read.cue.confidence);
            read_cues.push(read);
            done += 1;
            progress.advance(done);
        }
        progress.end();

        let mut corrector = self.corrector();
        corrector.observe(&read_cues);

        let mut cues = Vec::with_capacity(read_cues.len());
        for (index, read) in read_cues.into_iter().enumerate() {
            let mut cue = read.cue;

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
        report.vocabulary_corrections = corrections
            .iter()
            .filter(|c| {
                matches!(c.rule, subtrackt_text::correct::CorrectionRule::Vocabulary { .. })
            })
            .count()
            .try_into()
            .unwrap_or(u64::MAX);
        report.vocabulary_tokens = corrector.vocabulary_size().try_into().unwrap_or(u64::MAX);
        report.corrector = corrector.name();
        Ok(TextTrack::new(cues, None))
    }
}

/// The stage instances one run is assembled from.
///
/// Grouped rather than passed one by one because they travel together and always will: they are
/// the fan of stage crates the architecture document describes, and a run either has all of them
/// or is not a run.
struct Stages<'a> {
    segmenter: &'a ImageSegmenter,
    matcher: &'a mut HammingMatcher,
    assembler: &'a SpatialAssembler,
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

    /// Segment an image, and hand back the foreground mask it was segmented from.
    ///
    /// The mask is built on every call already and dropped at the end of it; this is the same work
    /// with the intermediate kept. It exists because a glyph's *un-normalised* ink is not
    /// recoverable from what [`Segmenter::segment`] returns — [`FeatureVector`] is letterboxed onto
    /// a 16x16 grid and thresholded per cell, which is a lossy projection built to make two
    /// renderings of one character converge.
    ///
    /// #63 is the caller: telling a good reference-set fit from a bad one needs the ink's *style*
    /// — stroke weight, contrast, the shape of a terminal — and at 16x16 a stem is one to three
    /// cells, so those are quantised away before anything can measure them. `xtask font-id`
    /// measured the cost of asking through the grid instead at 46 to 54 points of font-retrieval
    /// accuracy.
    ///
    /// Kept off the [`Segmenter`] trait: the trait is the stage contract every extraction runs
    /// through, and this is an instrument. Nothing on the matching path calls it.
    pub(crate) fn segment_with_mask(
        &self,
        image: &SubtitleImage,
    ) -> Result<(Vec<subtrackt_core::Glyph>, BinaryMask)> {
        let mask = self.mask(image);
        let glyphs = self.segment_from(image, &mask)?;
        Ok((glyphs, mask))
    }

    /// The body of both entry points, over a mask the caller owns.
    fn segment_from(
        &self,
        image: &SubtitleImage,
        mask: &BinaryMask,
    ) -> Result<Vec<subtrackt_core::Glyph>> {
        use subtrackt_glyph::ccl::{self, ComponentFilter};
        use subtrackt_glyph::feature::{self, AspectPolicy};
        use subtrackt_glyph::group::{self, GroupingRules};
        use subtrackt_glyph::mark;
        use subtrackt_glyph::metrics::{self, MetricRules};

        // Components and lines are yes-or-no questions and need the binary mask. Only the feature
        // vector reads the coverage plane, and only when asked to.
        let coverage = self.grey_coverage.then(|| self.binarizer.coverage(image));
        let components = ccl::label(mask, ComponentFilter::default())?;
        // One banding, used for both the assignment and the metrics. A band of nothing but accents
        // is not a text line — see `group::text_lines` — and a caller that banded twice could
        // measure line anchors against a different set than it grouped by.
        let bands = group::text_lines(mask, &components, GroupingRules::default());
        let lines = group::assign_to(&bands, &components)?;
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
                        || feature::vectorize(mask, bounds, AspectPolicy::default()),
                        |c| feature::vectorize_coverage(c, bounds, AspectPolicy::default()),
                    )?,
                    metrics: line_metrics,
                    // Read off the binary mask before the boxes are merged: once base and mark
                    // share a bounding box, letterboxing scales the direction away.
                    mark: mark::slope(mask, glyph),
                })
            })
            .collect()
    }
}

impl Segmenter for ImageSegmenter {
    fn segment(&self, image: &SubtitleImage) -> Result<Vec<subtrackt_core::Glyph>> {
        let mask = self.mask(image);
        self.segment_from(image, &mask)
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
    fn segmenting_with_the_mask_kept_returns_the_same_glyphs_as_segmenting_without() {
        // The instrument must not change what it observes. `segment_with_mask` exists so #63 can
        // read a glyph's un-normalised ink, and if it segmented even slightly differently from the
        // shipped path then every measurement taken through it would be about a pipeline nobody
        // runs.
        let segmenter = ImageSegmenter::new(Binarizer::default(), false);
        let plain = segmenter.segment(&image()).unwrap();
        let (with_mask, mask) = segmenter.segment_with_mask(&image()).unwrap();
        assert_eq!(plain, with_mask);
        assert_eq!((mask.width(), mask.height()), (8, 8), "the whole image, not a glyph");
    }

    #[test]
    fn the_kept_mask_carries_ink_the_feature_vector_cannot_express() {
        // The 4x4 block is 16 pixels of ink in a 4x4 box: solid. The feature vector letterboxes
        // that onto 16x16 and says which cells are inked, so it can say the shape is square but
        // not that the stroke is four pixels wide -- which is the whole reason this exists.
        let segmenter = ImageSegmenter::new(Binarizer::default(), false);
        let (glyphs, mask) = segmenter.segment_with_mask(&image()).unwrap();
        assert_eq!(glyphs.len(), 1);

        let cropped = mask.crop(glyphs[0].bounds).unwrap();
        assert_eq!(
            (cropped.width(), cropped.height()),
            (glyphs[0].bounds.width, glyphs[0].bounds.height)
        );
        assert_eq!(
            cropped.foreground_count(),
            (cropped.width() * cropped.height()) as usize,
            "a solid block crops to solid ink, at its own resolution"
        );
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
