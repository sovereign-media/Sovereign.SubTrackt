//! Dumping raw glyph shapes from a file, without trying to read them.
//!
//! This is the instrument #8 needs. The question it exists to answer is not "what does this
//! subtitle say" — nothing can answer that until there is a reference set — but "how many visually
//! distinct glyph sets does a real library contain, and would a handful of them cover most of it".
//! That is answerable now, because a [`FeatureVector`] is comparable across titles whether or not
//! anything can name the character it stands for.
//!
//! It is also what #9 will generate reference data against, and what #14 will measure stability
//! with, so it is a shipped capability rather than a throwaway script.

use std::path::Path;

use subtrackt_core::progress::{Phase, Progress, Silent};
use subtrackt_core::{
    Error, FeatureVector, InkAspect, LineMetrics, MarkSlope, Rect, Result, TimeSpan,
};
use subtrackt_demux::StreamInfo;

use crate::pipeline::ImageSegmenter;
use subtrackt_glyph::binarize::{Binarizer, BinaryMask};

/// One glyph as it appeared, with no attempt to identify it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphRecord {
    /// Which cue it came from, counting from zero.
    pub cue: usize,
    /// Which text line within that cue.
    pub line: usize,
    /// Where it sat, in subtitle-plane coordinates.
    pub bounds: Rect,
    /// The normalised shape.
    pub features: FeatureVector,
    /// Where it stood in its line, which the shape vector cannot express.
    ///
    /// Carried so a survey can be *scored* against a reference set and not only counted. Matching
    /// reads this — see `MatchThresholds::distance` — so a survey that dropped it would rank
    /// candidate sets on shape alone and rank them differently from the extraction it is meant to
    /// predict.
    pub metrics: LineMetrics,
    /// Which way its diacritic leans, carried for the same reason.
    pub mark: MarkSlope,
    /// How wide its ink stands against its line, carried for the same reason again.
    ///
    /// #109: the shape vector nearly expresses this and misses by the width of a grid cell, which
    /// is the difference between an `l` and an `I`. A survey that dropped it would rank candidate
    /// sets without the term the extraction it predicts actually applies.
    pub aspect: InkAspect,
    /// The glyph's un-normalised ink, cropped to [`Self::bounds`].
    ///
    /// `None` unless [`Config::glyph_masks`](crate::Config::glyph_masks) asked for it, and the
    /// distinction between `None` and an empty mask is the usual one in this project: absent means
    /// nobody asked, never that the glyph had no ink.
    ///
    /// This is the only thing here that [`Self::features`] cannot be recovered from, and it is the
    /// wrong way round from how it looks. The feature vector is a *lossy* projection built to make
    /// two renderings of one character converge — letterboxed onto a 16x16 grid, thresholded per
    /// cell — so it answers "which character" well and "what is this ink like" not at all. At that
    /// resolution a stem is one to three cells, which is why stroke weight and contrast do not
    /// survive it.
    pub mask: Option<BinaryMask>,
}

/// Everything one pass over a file turned up.
#[derive(Debug, Clone)]
pub struct GlyphSurvey {
    /// The stream that was read.
    pub stream: StreamInfo,
    /// Cues examined.
    pub cues: usize,
    /// Time span covered, which says how representative the sample is.
    pub span: Option<TimeSpan>,
    /// Every glyph, in reading order.
    pub glyphs: Vec<GlyphRecord>,
}

impl GlyphSurvey {
    /// Distinct glyph shapes.
    ///
    /// Subtitle text repeats heavily, so this is far smaller than [`Self::glyphs`] and is the
    /// number that actually characterises a typeface.
    ///
    /// Counted over the **same key the matcher separates glyphs by** — vector, line metrics, mark
    /// and ink width. Until #148 it counted the vector alone, which made this and `Fit::distinct`
    /// two different quantities under near-identical names: an `o` and an `O` normalise to one
    /// vector and are two shapes to everything downstream, so the old figure was systematically
    /// low and disagreed with the `distinct shapes` a run reports.
    #[must_use]
    pub fn distinct_shapes(&self) -> usize {
        let mut seen: std::collections::HashSet<subtrackt_glyph::cache::Key> =
            std::collections::HashSet::with_capacity(self.glyphs.len());
        for glyph in &self.glyphs {
            seen.insert(subtrackt_glyph::cache::cache_key(
                &glyph.features,
                glyph.metrics,
                glyph.mark,
                glyph.aspect,
            ));
        }
        seen.len()
    }
}

impl crate::Pipeline {
    /// Segment a file into glyph shapes without matching or assembling them.
    ///
    /// `max_cues` stops the pass early. That matters more than it looks: cues are spread evenly
    /// through a film, so reading the first few hundred touches only the corresponding fraction of
    /// a multi-gigabyte file. A typeface does not change halfway through, so a prefix is a fair
    /// sample for this purpose and a full pass is wasted work.
    ///
    /// # Errors
    /// Propagates demux, decode and segmentation failures.
    pub fn survey(&self, path: impl AsRef<Path>, max_cues: Option<usize>) -> Result<GlyphSurvey> {
        self.survey_watched(path, max_cues, &Silent)
    }

    /// Survey a file, reporting where the pass has got to.
    ///
    /// Identical to [`Self::survey`] but for the observer, and indeterminate even when `max_cues`
    /// is set. The cap is an upper bound rather than a total: the pass also ends when the source
    /// runs dry, and a title with fewer cues than the cap would leave a bar stopped part-way with
    /// the work finished — which says the opposite of what happened.
    ///
    /// # Errors
    /// As [`Self::survey`].
    pub fn survey_watched(
        &self,
        path: impl AsRef<Path>,
        max_cues: Option<usize>,
        progress: &dyn Progress,
    ) -> Result<GlyphSurvey> {
        use subtrackt_core::Segmenter;

        let path = path.as_ref();
        let mut source = subtrackt_demux::open(path)?;

        let stream = self.choose_stream(source.as_ref())?;
        source.select(stream.index)?;

        let mut decoder = subtrackt_decode::decoder_for(stream.codec.ffmpeg_name())?;
        // VOBSUB carries its palette out of band; without this a subpicture has colour indices
        // and no colours.
        decoder.configure(&stream.codec_private)?;
        let segmenter = ImageSegmenter::new(
            Binarizer::new(self.config().binarize),
            self.config().grey_coverage,
            // Never deskewed, and that is the point of a survey. #122 lets the *reference set*
            // decide whether a leaning line is sampled along its slant, and a survey exists to be
            // scored against several candidate sets at once — `subtrackt fit` ranks twenty-one of
            // them over one pass. Segmenting differently per candidate would rank sets on
            // segmentations rather than on fit.
            false,
        );
        let keep_masks = self.config().glyph_masks;

        let mut glyphs = Vec::new();
        let mut cues = 0usize;
        let mut span: Option<TimeSpan> = None;

        let mut images = Vec::new();
        progress.begin(Phase::Survey, None);
        'outer: while let Some(packet) = source.next_packet()? {
            images.clear();
            images.extend(decoder.push(packet.pts, &packet.payload)?);

            for image in &images {
                // One segmentation either way. The mask is built during it and normally dropped;
                // `keep_masks` only decides whether each glyph's share of it is copied out.
                let (in_image, image_mask) = if keep_masks {
                    let (glyphs, whole) = segmenter.segment_with_mask(image)?;
                    (glyphs, Some(whole))
                } else {
                    (segmenter.segment(image)?, None)
                };

                for glyph in in_image {
                    // `crop` refuses a zero-extent rectangle, and that refusal is propagated rather
                    // than softened to `None`: a glyph with no extent means the component filter
                    // let an empty box through, and a survey that quietly recorded it as "no mask
                    // was asked for" would hide a real segmentation fault behind a flag.
                    let cropped = match &image_mask {
                        Some(whole) => Some(whole.crop(glyph.bounds)?),
                        None => None,
                    };
                    glyphs.push(GlyphRecord {
                        cue: cues,
                        line: glyph.line,
                        bounds: glyph.bounds,
                        features: glyph.features,
                        metrics: glyph.metrics,
                        mark: glyph.mark,
                        aspect: glyph.aspect,
                        mask: cropped,
                    });
                }
                span = Some(match span {
                    Some(previous) => TimeSpan::new(previous.start, image.span.end),
                    None => image.span,
                });
                cues += 1;
                progress.advance(u64::try_from(cues).unwrap_or(u64::MAX));

                if max_cues.is_some_and(|limit| cues >= limit) {
                    break 'outer;
                }
            }
        }
        progress.end();

        Ok(GlyphSurvey { stream, cues, span, glyphs })
    }
}

/// Parse a hex-encoded feature vector as written by `subtrackt glyphs`.
///
/// # Errors
/// Returns [`Error::Config`] if the string is not 64 hex characters.
pub fn parse_vector_hex(hex: &str) -> Result<FeatureVector> {
    let expected = subtrackt_core::FEATURE_WORDS * 16;
    if hex.len() != expected || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Config(format!(
            "a feature vector is {expected} hex characters, got {}",
            hex.len()
        )));
    }
    let mut words = [0u64; subtrackt_core::FEATURE_WORDS];
    for (index, word) in words.iter_mut().enumerate() {
        *word = u64::from_str_radix(&hex[index * 16..(index + 1) * 16], 16)
            .map_err(|e| Error::Config(format!("bad feature vector: {e}")))?;
    }
    Ok(FeatureVector::from_words(words))
}

/// Hex-encode a feature vector, the inverse of [`parse_vector_hex`].
#[must_use]
pub fn vector_hex(features: &FeatureVector) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(64);
    for word in features.words() {
        // Cannot fail: writing to a String is infallible.
        let _ = write!(out, "{word:016x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vector_round_trips_through_hex() {
        let mut v = FeatureVector::EMPTY;
        for bit in [0, 7, 63, 64, 200, 255] {
            v.set(bit);
        }
        let hex = vector_hex(&v);
        assert_eq!(hex.len(), subtrackt_core::FEATURE_WORDS * 16);
        assert_eq!(parse_vector_hex(&hex).unwrap(), v);
    }

    #[test]
    fn the_empty_vector_encodes_as_all_zeroes() {
        assert_eq!(
            vector_hex(&FeatureVector::EMPTY),
            "0".repeat(subtrackt_core::FEATURE_WORDS * 16)
        );
    }

    #[test]
    fn malformed_hex_is_rejected_rather_than_silently_truncated() {
        assert!(parse_vector_hex("").is_err());
        assert!(parse_vector_hex("abc").is_err());
        assert!(parse_vector_hex(&"z".repeat(subtrackt_core::FEATURE_WORDS * 16)).is_err());
    }

    #[test]
    fn distinct_shapes_deduplicates_repeated_glyphs() {
        let mut a = FeatureVector::EMPTY;
        a.set(3);
        let record = |features| GlyphRecord {
            cue: 0,
            line: 0,
            bounds: Rect::new(0, 0, 4, 8),
            features,
            metrics: LineMetrics::UNKNOWN,
            mark: MarkSlope::NONE,
            aspect: InkAspect::UNKNOWN,
            mask: None,
        };
        let survey = GlyphSurvey {
            stream: StreamInfo {
                index: 0,
                codec: subtrackt_demux::BitmapCodec::Pgs,
                language: None,
                title: None,
                plane_width: 1920,
                plane_height: 1080,
                codec_private: Vec::new(),
            },
            cues: 1,
            span: None,
            glyphs: vec![record(a), record(a), record(FeatureVector::EMPTY)],
        };
        assert_eq!(survey.glyphs.len(), 3);
        assert_eq!(
            survey.distinct_shapes(),
            2,
            "text repeats; shapes are what characterise it"
        );
    }
}
