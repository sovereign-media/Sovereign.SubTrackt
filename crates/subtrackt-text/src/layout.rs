//! Turning a sequence of matched glyphs back into lines of text.
//!
//! The hard part is spacing. A proportional typeface has no single space width, and the gap inside
//! a kerned pair can be wider than the gap around a real space elsewhere on the same line. So the
//! threshold is derived per line from the gaps actually observed — the median gap stands in for
//! "normal letter spacing", and anything several times wider than that is a word break.
//!
//! Deriving it per line rather than fixing it matters twice over: it survives the same title
//! shipping at 480p and 1080p, and it survives one cue being set larger than another.

use subtrackt_core::{
    Confidence, Cue, Error, Glyph, GlyphMatch, Result, SubtitleImage, TextAssembler,
};

/// Rules for reconstructing text from glyph geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutRules {
    /// A gap wider than this multiple of the line's median inter-glyph gap, in percent, is a space.
    pub space_gap_percent: u32,
    /// Character substituted for a glyph the matcher could not identify.
    pub placeholder: char,
    /// Whether a leading `-` is followed by a space even when the gap alone would not warrant one.
    ///
    /// A dash opening a line marks a second speaker. Set tight against the following word it reads
    /// as a hyphen, which changes what the line means.
    pub preserve_speaker_dash: bool,
    /// Distance margin below which a match counts as ambiguous, for the confidence tally.
    ///
    /// Should agree with the matcher's own margin; the pipeline wires them from one source.
    pub ambiguity_margin: u32,
}

impl Default for LayoutRules {
    fn default() -> Self {
        Self {
            space_gap_percent: 250,
            placeholder: '\u{fffd}',
            preserve_speaker_dash: true,
            ambiguity_margin: 8,
        }
    }
}

/// Assembles cues from glyph geometry and their matches.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpatialAssembler {
    rules: LayoutRules,
}

impl SpatialAssembler {
    /// An assembler using the given rules.
    #[must_use]
    pub const fn new(rules: LayoutRules) -> Self {
        Self { rules }
    }

    /// The rules in force.
    #[must_use]
    pub const fn rules(&self) -> LayoutRules {
        self.rules
    }

    /// Build one line of text from the glyphs assigned to it, and say what produced each
    /// character of it.
    fn render_line(&self, line: &[(Glyph, GlyphMatch)]) -> (String, Vec<Option<GlyphMatch>>) {
        let gaps: Vec<u32> = line
            .windows(2)
            .map(|pair| pair[1].0.bounds.x.saturating_sub(pair[0].0.bounds.right()))
            .collect();
        let median = median_gap(&gaps);

        let mut out = String::new();
        let mut origins = Vec::with_capacity(line.len());
        for (index, (_, matched)) in line.iter().enumerate() {
            if index > 0 {
                let gap = gaps[index - 1];
                let opens_with_dash = self.rules.preserve_speaker_dash
                    && index == 1
                    && line[0].1.character == Some('-');

                if opens_with_dash || is_space(gap, median, self.rules) {
                    out.push(' ');
                    origins.push(None);
                }
            }
            out.push(matched.character.unwrap_or(self.rules.placeholder));
            origins.push(Some(matched.clone()));
        }
        (out, origins)
    }

    /// Assemble a cue and keep the per-character provenance post-correction needs.
    ///
    /// # Errors
    /// Same as [`TextAssembler::assemble`].
    pub fn assemble_annotated(
        &self,
        image: &SubtitleImage,
        glyphs: &[Glyph],
        matches: &[GlyphMatch],
    ) -> Result<AssembledCue> {
        if glyphs.len() != matches.len() {
            return Err(Error::Config(format!(
                "assemble got {} glyphs and {} matches; they must be index-aligned",
                glyphs.len(),
                matches.len()
            )));
        }

        let confidence = matches
            .iter()
            .fold(Confidence::default(), |mut tally, matched| {
                if matched.character.is_some() {
                    tally.matched += 1;
                    if !matched.is_unambiguous(self.rules.ambiguity_margin) {
                        tally.ambiguous += 1;
                    }
                } else {
                    tally.unmatched += 1;
                }
                tally
            });

        let line_count = glyphs.iter().map(|g| g.line).max().map_or(0, |m| m + 1);
        let mut lines = Vec::with_capacity(line_count);
        let mut origins = Vec::with_capacity(line_count);

        for line_index in 0..line_count {
            let mut members: Vec<(Glyph, GlyphMatch)> = glyphs
                .iter()
                .zip(matches)
                .filter(|(g, _)| g.line == line_index)
                .map(|(g, m)| (g.clone(), m.clone()))
                .collect();
            if members.is_empty() {
                continue;
            }
            // Reading order is the assembler's responsibility; upstream ordering is not a
            // guarantee it should depend on.
            members.sort_by_key(|(g, _)| g.bounds.x);

            let (rendered, rendered_origins) = self.render_line(&members);
            if !rendered.trim().is_empty() {
                lines.push(rendered);
                origins.push(rendered_origins);
            }
        }

        let cue = Cue { span: image.span, lines, confidence, forced: image.forced };
        Ok(AssembledCue { cue, origins })
    }
}

/// A cue together with what produced each of its characters.
///
/// Post-correction has to know which characters came from a glyph the matcher could not call
/// outright, and only the assembler knows: by the time a [`Cue`] exists its characters have been
/// sorted into reading order, split across lines, and had spaces inserted between them. Handing a
/// corrector the cue and the match list separately would make it re-derive that mapping, and a
/// corrector working from a *guess* about which glyph produced which character is precisely the
/// thing this stage must not be.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledCue {
    /// The cue.
    pub cue: Cue,
    /// One entry per line of [`Cue::lines`], and within it one entry per `char` of that line: the
    /// match that produced the character, or `None` for a space the assembler inserted.
    pub origins: Vec<Vec<Option<GlyphMatch>>>,
}

/// Median of the observed gaps, or zero when there are none to measure.
fn median_gap(gaps: &[u32]) -> u32 {
    if gaps.is_empty() {
        return 0;
    }
    let mut sorted = gaps.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

impl TextAssembler for SpatialAssembler {
    fn assemble(
        &self,
        image: &SubtitleImage,
        glyphs: &[Glyph],
        matches: &[GlyphMatch],
    ) -> Result<Cue> {
        self.assemble_annotated(image, glyphs, matches)
            .map(|assembled| assembled.cue)
    }
}

/// Whether a gap is wide enough to be a word break.
///
/// Split out because it is the single number #11 turns on, and it should be testable without a
/// full image.
#[must_use]
pub fn is_space(gap: u32, median_gap: u32, rules: LayoutRules) -> bool {
    if median_gap == 0 {
        return false;
    }
    gap * 100 / median_gap >= rules.space_gap_percent
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtrackt_core::{FeatureVector, IndexedBitmap, Palette, Rect, TimeSpan, Timestamp};

    fn image() -> SubtitleImage {
        SubtitleImage {
            span: TimeSpan::new(Timestamp::ZERO, Timestamp::from_millis(1_000)),
            position: Rect::new(0, 0, 2, 2),
            bitmap: IndexedBitmap::blank(2, 2),
            palette: Palette::transparent(2),
            forced: false,
        }
    }

    fn glyph(x: u32, width: u32, line: usize) -> Glyph {
        Glyph {
            bounds: Rect::new(x, 0, width, 10),
            line,
            features: FeatureVector::EMPTY,
            // Layout works from geometry, not from where a glyph stands in its line.
            metrics: subtrackt_core::LineMetrics::UNKNOWN,
        }
    }

    fn matched(c: char) -> GlyphMatch {
        GlyphMatch { character: Some(c), distance: 1, runner_up_distance: 60 }
    }

    /// Lay out `text` on one line, with `scale` pixels per unit so the same fixture can be run at
    /// two resolutions. A space in `text` becomes a wide gap; everything else a kerned gap.
    fn lay_out(text: &str, scale: u32) -> (Vec<Glyph>, Vec<GlyphMatch>) {
        let (mut glyphs, mut matches) = (Vec::new(), Vec::new());
        let mut x = 0;
        for ch in text.chars() {
            if ch == ' ' {
                x += 5 * scale; // a word gap
                continue;
            }
            glyphs.push(glyph(x, 6 * scale, 0));
            matches.push(matched(ch));
            x += 7 * scale; // glyph width plus one unit of kerning
        }
        (glyphs, matches)
    }

    fn assemble(glyphs: &[Glyph], matches: &[GlyphMatch]) -> Cue {
        SpatialAssembler::default()
            .assemble(&image(), glyphs, matches)
            .unwrap()
    }

    #[test]
    fn a_kerned_gap_is_not_a_space_but_a_word_gap_is() {
        let rules = LayoutRules::default();
        assert!(!is_space(2, 3, rules), "a tight kerned pair must not become a space");
        assert!(is_space(9, 3, rules), "a word gap must");
    }

    #[test]
    fn a_line_with_no_measurable_gaps_inserts_no_spaces() {
        // One glyph on a line means no median to compare against; guessing here would produce
        // spurious spaces in short cues.
        assert!(!is_space(50, 0, LayoutRules::default()));
    }

    #[test]
    fn word_gaps_become_spaces_and_kerning_does_not() {
        let (glyphs, matches) = lay_out("HELLO THERE", 1);
        assert_eq!(assemble(&glyphs, &matches).text(), "HELLO THERE");
    }

    #[test]
    fn the_same_line_reads_the_same_at_two_resolutions() {
        // The property the per-line median exists for. A fixed pixel threshold tuned at 1080p
        // would run every word together at 480p.
        for scale in [1, 2, 3, 5] {
            let (glyphs, matches) = lay_out("HELLO THERE", scale);
            assert_eq!(
                assemble(&glyphs, &matches).text(),
                "HELLO THERE",
                "scale {scale} changed the spacing"
            );
        }
    }

    #[test]
    fn a_single_glyph_line_produces_that_glyph() {
        let cue = assemble(&[glyph(0, 6, 0)], &[matched('A')]);
        assert_eq!(cue.text(), "A");
    }

    #[test]
    fn output_carries_no_doubled_or_trailing_spaces() {
        let (glyphs, matches) = lay_out("A B  C", 2);
        let text = assemble(&glyphs, &matches).text();
        assert!(!text.contains("  "), "doubled space in {text:?}");
        assert_eq!(text.trim_end(), text, "trailing space in {text:?}");
        assert_eq!(text.trim_start(), text, "leading space in {text:?}");
    }

    #[test]
    fn a_speaker_dash_keeps_its_space_from_the_following_word() {
        // Set tight, a leading dash reads as a hyphen and changes what the line means.
        let mut glyphs = vec![glyph(0, 4, 0)];
        let mut matches = vec![matched('-')];
        for (index, ch) in "Yes".chars().enumerate() {
            let index = u32::try_from(index).unwrap();
            glyphs.push(glyph(6 + index * 7, 6, 0));
            matches.push(matched(ch));
        }

        assert_eq!(assemble(&glyphs, &matches).text(), "- Yes");
    }

    #[test]
    fn a_dash_mid_line_is_left_alone() {
        let mut glyphs = vec![glyph(0, 6, 0)];
        let mut matches = vec![matched('A')];
        glyphs.push(glyph(7, 4, 0));
        matches.push(matched('-'));
        glyphs.push(glyph(12, 6, 0));
        matches.push(matched('B'));

        assert_eq!(
            assemble(&glyphs, &matches).text(),
            "A-B",
            "only a leading dash is a speaker"
        );
    }

    #[test]
    fn two_lines_come_back_as_two_lines_in_order() {
        let mut glyphs = Vec::new();
        let mut matches = Vec::new();
        for (index, ch) in "AB".chars().enumerate() {
            glyphs.push(glyph(u32::try_from(index).unwrap() * 7, 6, 0));
            matches.push(matched(ch));
        }
        for (index, ch) in "CD".chars().enumerate() {
            glyphs.push(glyph(u32::try_from(index).unwrap() * 7, 6, 1));
            matches.push(matched(ch));
        }

        let cue = assemble(&glyphs, &matches);
        assert_eq!(cue.lines, vec!["AB".to_owned(), "CD".to_owned()]);
        assert_eq!(cue.text(), "AB\nCD");
    }

    #[test]
    fn glyphs_are_ordered_by_position_not_by_arrival() {
        // Reading order is this stage's job; depending on upstream ordering would be a silent
        // coupling that breaks the first time a stage reorders anything.
        let glyphs = vec![glyph(14, 6, 0), glyph(0, 6, 0), glyph(7, 6, 0)];
        let matches = vec![matched('C'), matched('A'), matched('B')];
        assert_eq!(assemble(&glyphs, &matches).text(), "ABC");
    }

    #[test]
    fn an_unmatched_glyph_becomes_the_placeholder_and_is_counted() {
        let glyphs = vec![glyph(0, 6, 0), glyph(7, 6, 0)];
        let matches = vec![matched('A'), GlyphMatch::unmatched(200)];

        let cue = assemble(&glyphs, &matches);
        assert_eq!(cue.text(), "A\u{fffd}");
        assert_eq!(cue.confidence.matched, 1);
        assert_eq!(cue.confidence.unmatched, 1);
        assert!(!cue.confidence.is_complete(), "the gate has to be able to see this");
    }

    #[test]
    fn a_close_runner_up_is_tallied_as_ambiguous_without_changing_the_text() {
        let glyphs = vec![glyph(0, 6, 0)];
        let matches = vec![GlyphMatch { character: Some('0'), distance: 8, runner_up_distance: 9 }];

        let cue = assemble(&glyphs, &matches);
        assert_eq!(cue.text(), "0", "an ambiguous read is still the matcher's answer");
        assert_eq!(cue.confidence.matched, 1);
        assert_eq!(cue.confidence.ambiguous, 1, "but post-correction needs to know");
    }

    #[test]
    fn a_cue_with_no_glyphs_has_no_lines() {
        let cue = assemble(&[], &[]);
        assert!(cue.lines.is_empty());
        assert!(cue.is_empty());
    }

    #[test]
    fn the_forced_flag_and_timing_come_from_the_image() {
        let mut img = image();
        img.forced = true;
        let cue = SpatialAssembler::default()
            .assemble(&img, &[glyph(0, 6, 0)], &[matched('A')])
            .unwrap();

        assert!(cue.forced);
        assert_eq!(cue.span, img.span);
    }

    #[test]
    fn every_character_of_an_assembled_line_says_which_glyph_produced_it() {
        // What post-correction stands on. A corrector that had to re-derive this mapping would be
        // guessing which glyph produced which character, and a guess is what this stage may not be.
        let (glyphs, matches) = lay_out("AB CD", 1);
        let assembled = SpatialAssembler::default()
            .assemble_annotated(&image(), &glyphs, &matches)
            .unwrap();

        assert_eq!(assembled.cue.lines, vec!["AB CD".to_owned()]);
        assert_eq!(assembled.origins.len(), 1);
        assert_eq!(
            assembled.origins[0].len(),
            assembled.cue.lines[0].chars().count(),
            "one entry per character, spaces included"
        );

        let characters: Vec<Option<char>> = assembled.origins[0]
            .iter()
            .map(|origin| origin.as_ref().and_then(|m| m.character))
            .collect();
        assert_eq!(
            characters,
            vec![Some('A'), Some('B'), None, Some('C'), Some('D')],
            "the inserted space came from no glyph and has to say so"
        );
    }

    #[test]
    fn provenance_follows_the_reading_order_the_assembler_imposed() {
        // Origins are aligned with the *rendered* line, not with the order the glyphs arrived in,
        // which is the whole reason the assembler is the one producing them.
        let glyphs = vec![glyph(14, 6, 0), glyph(0, 6, 0), glyph(7, 6, 0)];
        let matches = vec![
            matched('C'),
            matched('A'),
            GlyphMatch { character: Some('B'), distance: 8, runner_up_distance: 9 },
        ];

        let assembled = SpatialAssembler::default()
            .assemble_annotated(&image(), &glyphs, &matches)
            .unwrap();
        assert_eq!(assembled.cue.lines, vec!["ABC".to_owned()]);

        let ambiguous: Vec<bool> = assembled.origins[0]
            .iter()
            .map(|origin| {
                origin
                    .as_ref()
                    .is_some_and(|m| !m.is_unambiguous(LayoutRules::default().ambiguity_margin))
            })
            .collect();
        assert_eq!(
            ambiguous,
            vec![false, true, false],
            "the close call is the middle character"
        );
    }

    #[test]
    fn mismatched_glyph_and_match_slices_are_a_configuration_error_not_a_panic() {
        let err = SpatialAssembler::default()
            .assemble(&image(), &[glyph(0, 6, 0)], &[])
            .unwrap_err();
        assert!(matches!(err, Error::Config(_)), "got {err:?}");
    }
}
