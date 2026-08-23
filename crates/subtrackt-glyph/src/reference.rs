//! The reference glyph set the matcher compares against.
//!
//! [#8](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/8) established that a
//! dominant glyph family runs through the library, so a fixed reference set is worth having. What
//! it did not establish is *which* set, and the survey's recommendation was explicit: stop naming
//! typefaces and fit one to the evidence instead. Until that fit is done, [`embedded`] stays empty
//! — an unlisted typeface should degrade to a clean failure rather than to confident garbage.
//!
//! The on-disk format below is what the generator writes and what the binary will eventually
//! embed.

use subtrackt_core::{
    Error, FEATURE_GRID, FEATURE_WORDS, FeatureVector, InkAspect, LineMetrics, MarkSlope, Result,
};

/// Typographic variant of a reference glyph.
///
/// Whether variants need separate reference vectors at all is #14. If one vector per character
/// survives bold, italic and outline variation, this collapses to a single entry per character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Style {
    /// Upright, normal weight.
    #[default]
    Regular,
    /// Bold.
    Bold,
    /// Italic or oblique.
    Italic,
    /// Bold italic.
    BoldItalic,
}

impl Style {
    /// Encode as a single byte for the on-disk format.
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::Regular => 0,
            Self::Bold => 1,
            Self::Italic => 2,
            Self::BoldItalic => 3,
        }
    }

    /// Decode from the on-disk format, or `None` for an unknown value.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Regular),
            1 => Some(Self::Bold),
            2 => Some(Self::Italic),
            3 => Some(Self::BoldItalic),
            _ => None,
        }
    }
}

/// One reference glyph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceEntry {
    /// The character this vector stands for.
    pub character: char,
    /// Which typographic variant it was rendered as.
    pub style: Style,
    /// The normalised vector.
    pub features: FeatureVector,
    /// How tall this character stands in a line of text, relative to that line.
    ///
    /// The shape vector cannot express this — see [`LineMetrics`] — and without it a reference set
    /// places `o` and `O` at almost the same point. Generated from font metrics rather than from
    /// the rasterisation, because a glyph rendered on its own has no line to be relative to.
    pub metrics: LineMetrics,
    /// Which way this character's diacritic leans, for the characters that have one.
    ///
    /// The other thing the shape vector cannot express — see [`MarkSlope`]. Sixteen of the 21 pairs
    /// the matcher calls ambiguous are one base letter differing only in this.
    pub mark: MarkSlope,
    /// How wide this character's ink stands against its face's cap height.
    ///
    /// The third — see [`InkAspect`], and note that the vector *nearly* expresses it. Letterboxing
    /// keeps a glyph's aspect ratio but keeps it onto sixteen cells, and the difference between an
    /// `l` and an `I` is a fifth of one cell. Generated at a larger render than the vectors are,
    /// because at 96 pixels that difference is six tenths of a pixel and rounds away.
    pub aspect: InkAspect,
}

/// A named collection of reference glyphs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceSet {
    name: String,
    grid: usize,
    entries: Vec<ReferenceEntry>,
}

impl ReferenceSet {
    /// Build a set, recording the grid size its vectors were generated at.
    #[must_use]
    pub fn new(name: impl Into<String>, entries: Vec<ReferenceEntry>) -> Self {
        Self { name: name.into(), grid: FEATURE_GRID, entries }
    }

    /// An empty set, which is what the binary ships with today.
    #[must_use]
    pub fn empty() -> Self {
        Self::new("empty", Vec::new())
    }

    /// The set's name, surfaced in `--version` so a bad extraction can be traced to its data.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The grid size the vectors were generated at.
    ///
    /// A set generated at a different grid size than the running binary cannot be compared
    /// against, and the matcher must refuse it rather than produce meaningless distances.
    #[must_use]
    pub const fn grid(&self) -> usize {
        self.grid
    }

    /// Whether this set was generated for the grid size this build uses.
    #[must_use]
    pub const fn matches_build_grid(&self) -> bool {
        self.grid == FEATURE_GRID
    }

    /// The entries.
    #[must_use]
    pub fn entries(&self) -> &[ReferenceEntry] {
        &self.entries
    }

    /// Number of reference glyphs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the set holds no glyphs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Magic at the head of a serialised reference set, last byte being the format version.
///
/// Version 2 added line metrics, version 3 the mark slope, version 4 the ink width. Older sets
/// still load, and an entry from one reports the field it predates as unknown rather than as a
/// default standing in for a measurement — which is what keeps a distance term from being applied
/// to data that never carried it.
const MAGIC: &[u8; 8] = b"SUBTREF\x04";

/// The version 3 magic, still readable. Its entries carry metrics and a mark but no width.
const MAGIC_V3: &[u8; 8] = b"SUBTREF\x03";

/// The version 2 magic, still readable. Its entries carry metrics but no mark.
const MAGIC_V2: &[u8; 8] = b"SUBTREF\x02";

/// The version 1 magic, still readable. Its entries carry neither.
const MAGIC_V1: &[u8; 8] = b"SUBTREF\x01";

/// Bytes per entry: codepoint, style, metrics-known flag, height, descent, mark-known flag, slope,
/// width-known flag, width, then the vector.
const ENTRY_LEN: usize = 4 + 1 + 1 + 2 + 2 + 1 + 2 + 1 + 2 + FEATURE_WORDS * 8;

/// Bytes per entry in version 3: as above without the width-known flag and the width.
const ENTRY_LEN_V3: usize = 4 + 1 + 1 + 2 + 2 + 1 + 2 + FEATURE_WORDS * 8;

/// Bytes per entry in version 2: as version 3 without the mark-known flag and the slope.
const ENTRY_LEN_V2: usize = 4 + 1 + 1 + 2 + 2 + FEATURE_WORDS * 8;

/// Bytes per entry in version 1: codepoint, style, two reserved, then the vector.
const ENTRY_LEN_V1: usize = 4 + 1 + 2 + FEATURE_WORDS * 8;

/// Bytes before the name.
const HEADER_LEN: usize = 16;

impl ReferenceSet {
    /// Serialise to the on-disk format.
    ///
    /// Deliberately uncompressed, which departs from the `flate2` row in §3 of #1. A full set is a
    /// few hundred entries of 39 bytes — single-digit kilobytes — so compressing it would buy
    /// nothing measurable and cost a dependency.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let name = self.name.as_bytes();
        let mut out = Vec::with_capacity(HEADER_LEN + name.len() + self.entries.len() * ENTRY_LEN);

        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&u16::try_from(self.grid).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(&u16::try_from(name.len()).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(&u32::try_from(self.entries.len()).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(name);

        for entry in &self.entries {
            out.extend_from_slice(&(entry.character as u32).to_le_bytes());
            out.push(entry.style.to_byte());
            out.push(u8::from(entry.metrics.known));
            out.extend_from_slice(
                &u16::try_from(entry.metrics.height_percent)
                    .unwrap_or(u16::MAX)
                    .to_le_bytes(),
            );
            out.extend_from_slice(
                &i16::try_from(entry.metrics.descent_percent)
                    .unwrap_or(0)
                    .to_le_bytes(),
            );
            out.push(u8::from(entry.mark.known));
            out.extend_from_slice(&i16::try_from(entry.mark.percent).unwrap_or(0).to_le_bytes());
            out.push(u8::from(entry.aspect.known));
            out.extend_from_slice(
                &u16::try_from(entry.aspect.permille)
                    .unwrap_or(u16::MAX)
                    .to_le_bytes(),
            );
            for word in entry.features.words() {
                out.extend_from_slice(&word.to_le_bytes());
            }
        }
        out
    }

    /// Parse the on-disk format.
    ///
    /// # Errors
    /// Returns [`Error::Config`] for a bad magic, a truncated file, or an entry whose style byte
    /// or codepoint is invalid. A set that half-loads would silently narrow what the matcher can
    /// recognise, which looks from the outside exactly like a typeface it has never seen — so it
    /// refuses rather than loading what it can.
    ///
    /// # Panics
    /// Does not. The slice length is checked against the entry count before any entry is read, so
    /// the fixed-width reads inside the loop cannot go out of bounds.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let bad = |what: &str| Error::Config(format!("reference set: {what}"));

        if bytes.len() < HEADER_LEN {
            return Err(bad("too short for a header"));
        }
        let entry_len = match &bytes[..8] {
            m if m == MAGIC => ENTRY_LEN,
            m if m == MAGIC_V3 => ENTRY_LEN_V3,
            m if m == MAGIC_V2 => ENTRY_LEN_V2,
            m if m == MAGIC_V1 => ENTRY_LEN_V1,
            _ => return Err(bad("bad magic")),
        };
        let grid = usize::from(u16::from_le_bytes([bytes[8], bytes[9]]));
        let name_len = usize::from(u16::from_le_bytes([bytes[10], bytes[11]]));
        let count = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;

        let name_end = HEADER_LEN + name_len;
        let body = bytes.get(name_end..).ok_or_else(|| bad("truncated name"))?;
        let name = String::from_utf8_lossy(&bytes[HEADER_LEN..name_end]).into_owned();

        if body.len() != count * entry_len {
            return Err(bad(&format!(
                "declares {count} entries ({} bytes) but holds {}",
                count * entry_len,
                body.len()
            )));
        }

        let mut entries = Vec::with_capacity(count);
        for chunk in body.chunks_exact(entry_len) {
            let code = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let character =
                char::from_u32(code).ok_or_else(|| bad(&format!("codepoint U+{code:04X}")))?;
            let style = Style::from_byte(chunk[4])
                .ok_or_else(|| bad(&format!("style byte {}", chunk[4])))?;

            // A version 1 entry has no metrics, and a version 1 or 2 entry no mark. Each reports
            // unknown rather than a stand-in, so a term is never applied to data that predates it.
            let metrics = if entry_len != ENTRY_LEN_V1 && chunk[5] != 0 {
                LineMetrics::new(
                    u32::from(u16::from_le_bytes([chunk[6], chunk[7]])),
                    i32::from(i16::from_le_bytes([chunk[8], chunk[9]])),
                )
            } else {
                LineMetrics::UNKNOWN
            };
            let mark = if matches!(entry_len, ENTRY_LEN | ENTRY_LEN_V3) && chunk[10] != 0 {
                MarkSlope::new(i32::from(i16::from_le_bytes([chunk[11], chunk[12]])))
            } else {
                MarkSlope::NONE
            };
            let aspect = if entry_len == ENTRY_LEN && chunk[13] != 0 {
                InkAspect::new(u32::from(u16::from_le_bytes([chunk[14], chunk[15]])))
            } else {
                InkAspect::UNKNOWN
            };

            let words_at = match entry_len {
                ENTRY_LEN => 16,
                ENTRY_LEN_V3 => 13,
                ENTRY_LEN_V2 => 10,
                _ => 7,
            };
            // `as_chunks` rather than a slice and a `try_into().expect(...)`: the eight-byte window
            // is a property the type system can carry, so the panic path that asserted it at
            // runtime is gone rather than merely unreachable. A parser is the last place to leave
            // one — see the "Failing" rule in `CLAUDE.md`.
            let (chunks, _) = chunk[words_at..].as_chunks::<8>();
            let mut words = [0u64; FEATURE_WORDS];
            for (word, bytes) in words.iter_mut().zip(chunks) {
                *word = u64::from_le_bytes(*bytes);
            }
            entries.push(ReferenceEntry {
                character,
                style,
                features: FeatureVector::from_words(words),
                metrics,
                mark,
                aspect,
            });
        }

        Ok(Self { name, grid, entries })
    }
}

/// The reference set compiled into this binary.
///
/// **Empty, and now empty on purpose rather than pending.** `xtask reference-fit` scored six
/// candidate sets against Arial-rendered material and `docs/reference-set.md` records what it
/// found: Liberation Sans — metric-compatible with Arial, openly licensed, the obvious thing to
/// embed — reads it at 26.8% character error against Arial's own 15.9%, which is Verdana's 27.4%
/// to within noise. Being visually close to the material bought 0.6 points.
///
/// The errors it makes are systematic rather than scattered: `t` read as `I` in every position,
/// `1` as `4`, `a` unmatched throughout. That is a confident wrong answer, which is the failure
/// mode §4 of #1 rejected general OCR to avoid, and neither figure the accuracy gate can see
/// detects it — Segoe UI matches the same 93.9% of glyphs as Arial and reads 2.4 times worse.
///
/// So an embedded set would trade a *detectable* failure for an undetectable one. With nothing
/// embedded a mismatched disc comes back entirely unmatched and the caller falls back to burn-in;
/// with a set embedded it comes back three-quarters right and nothing says which quarter.
///
/// This is not waiting on a better font. The unlock is reference data fitted to the material
/// itself rather than shipped in the binary.
#[must_use]
pub fn embedded() -> ReferenceSet {
    ReferenceSet::empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ReferenceSet {
        let mut a = FeatureVector::EMPTY;
        a.set(3);
        a.set(200);
        ReferenceSet::new(
            "fitted-2026",
            vec![
                ReferenceEntry {
                    character: 'A',
                    style: Style::Regular,
                    features: a,
                    metrics: LineMetrics::new(100, 0),
                    // A bare capital carries no mark, which is a fact about it rather than a gap.
                    mark: MarkSlope::NONE,
                    // Nearly as wide as the cap is tall, which is what an `A` is.
                    aspect: InkAspect::new(928),
                },
                ReferenceEntry {
                    character: '\u{e9}',
                    style: Style::Italic,
                    features: FeatureVector::EMPTY,
                    // An accented lowercase: taller than x-height, sitting on the baseline.
                    metrics: LineMetrics::new(96, -3),
                    // An acute rises left to right, so its cross term is negative.
                    mark: MarkSlope::new(-66),
                    aspect: InkAspect::new(652),
                },
            ],
        )
    }

    #[test]
    fn the_embedded_set_is_empty_because_shipping_one_measured_worse_than_shipping_none() {
        // Not a placeholder. `xtask reference-fit` priced every candidate and
        // `docs/reference-set.md` records why an empty set beats the best of them: a mismatched
        // disc read against a shipped set comes back three-quarters right with nothing saying
        // which quarter, and read against nothing at all comes back refused.
        let set = embedded();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        assert_eq!(set.name(), "empty", "and `--version` prints this, so a user can see it");
    }

    #[test]
    fn a_set_records_the_grid_it_was_generated_for() {
        let set = ReferenceSet::new("test", vec![]);
        assert_eq!(set.grid(), FEATURE_GRID);
        assert!(set.matches_build_grid());
        assert_eq!(set.name(), "test");
    }

    #[test]
    fn a_set_round_trips_through_the_on_disk_format() {
        let original = sample();
        let decoded = ReferenceSet::decode(&original.encode()).unwrap();

        assert_eq!(decoded.name(), "fitted-2026");
        assert_eq!(decoded.grid(), original.grid());
        assert_eq!(decoded.entries(), original.entries());
    }

    #[test]
    fn non_ascii_characters_survive_the_round_trip() {
        let decoded = ReferenceSet::decode(&sample().encode()).unwrap();
        assert_eq!(decoded.entries()[1].character, '\u{e9}');
        assert_eq!(decoded.entries()[1].style, Style::Italic);
    }

    #[test]
    fn an_aspect_ratio_survives_the_round_trip_at_the_resolution_it_was_measured_at() {
        // Tenths of a percent, and the round trip has to keep every one of them: the gap this
        // field exists to carry — an `l` against an `I` — is eight of them. A format that stored
        // whole percent would round the two together and the entry would say they are the same
        // width, which is exactly the claim #109 disproved.
        let decoded = ReferenceSet::decode(&sample().encode()).unwrap();
        assert_eq!(decoded.entries()[0].aspect, InkAspect::new(928));
        assert_eq!(decoded.entries()[1].aspect, InkAspect::new(652));
    }

    #[test]
    fn a_set_written_before_aspect_ratios_existed_reports_unknown_rather_than_zero() {
        // A version 3 set carries no width, and an entry from one has to say so. Zero would be a
        // measurement — a glyph with no ink — and every glyph on the disc would then be charged the
        // full width term against it. `InkAspect::difference` returning `None` is what keeps a term
        // from being applied to data that predates it, the same contract version 2 kept for the
        // mark.
        let mut bytes = sample().encode();
        assert_eq!(&bytes[..8], MAGIC, "the sample is written at the current version");

        // Rewrite it as version 3: the same entries with the last three bytes of each dropped.
        let v3: Vec<u8> = bytes[..HEADER_LEN + "fitted-2026".len()]
            .iter()
            .copied()
            .chain(
                bytes[HEADER_LEN + "fitted-2026".len()..]
                    .as_chunks::<ENTRY_LEN>()
                    .0
                    .iter()
                    .flat_map(|entry| entry[..13].iter().chain(&entry[16..]).copied()),
            )
            .collect();
        bytes = v3;
        bytes[..8].copy_from_slice(MAGIC_V3);

        let decoded = ReferenceSet::decode(&bytes).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded.entries()[0].aspect, InkAspect::UNKNOWN);
        assert_eq!(
            decoded.entries()[0].mark,
            MarkSlope::NONE,
            "and everything version 3 did carry still arrives"
        );
        assert_eq!(decoded.entries()[1].mark, MarkSlope::new(-66));
        assert_eq!(decoded.entries()[1].features, FeatureVector::EMPTY);
    }

    #[test]
    fn a_negative_slope_survives_the_round_trip() {
        // The sign is the whole feature. An encoding that dropped it would leave an acute and a
        // grave at the same value and separate nothing, with every distance still looking sane.
        let decoded = ReferenceSet::decode(&sample().encode()).unwrap();
        assert_eq!(decoded.entries()[1].mark, MarkSlope::new(-66));
        assert_eq!(decoded.entries()[0].mark, MarkSlope::NONE);
    }

    #[test]
    fn a_version_2_set_loads_with_no_mark_rather_than_a_fabricated_one() {
        // Sets generated before #48 carry no slope. Reading zero out of them would be worse than
        // reading nothing: zero is what a circumflex measures, so every unmarked character would
        // claim to carry a symmetric mark and the term would fire on data that never had it.
        let mut bytes = sample().encode();
        bytes[7] = 2;
        let stripped: Vec<u8> = bytes[..HEADER_LEN + "fitted-2026".len()]
            .iter()
            .copied()
            .chain(
                bytes[HEADER_LEN + "fitted-2026".len()..]
                    .as_chunks::<ENTRY_LEN>()
                    .0
                    .iter()
                    .flat_map(|entry| {
                        // Drop everything between the metrics and the vector: the mark-known flag
                        // and the slope, which version 2 predates, and the width-known flag and the
                        // width, which it predates by two more versions.
                        entry[..10].iter().chain(&entry[16..]).copied()
                    }),
            )
            .collect();

        let decoded = ReferenceSet::decode(&stripped).unwrap();
        assert_eq!(decoded.len(), 2);
        assert!(decoded.entries().iter().all(|e| e.mark == MarkSlope::NONE));
        // The metrics a version 2 set does carry must still arrive.
        assert_eq!(decoded.entries()[1].metrics, LineMetrics::new(96, -3));
        assert_eq!(decoded.entries()[1].features, FeatureVector::EMPTY);
    }

    #[test]
    fn every_style_round_trips_as_a_byte() {
        for style in [
            Style::Regular,
            Style::Bold,
            Style::Italic,
            Style::BoldItalic,
        ] {
            assert_eq!(Style::from_byte(style.to_byte()), Some(style));
        }
        assert_eq!(Style::from_byte(9), None);
    }

    #[test]
    fn a_truncated_set_is_rejected_rather_than_half_loaded() {
        // Half a set looks from the outside exactly like a typeface the matcher has never seen,
        // so it must not load at all.
        let encoded = sample().encode();
        for cut in [0, 8, 15, encoded.len() - 1] {
            assert!(ReferenceSet::decode(&encoded[..cut]).is_err(), "accepted {cut} bytes");
        }
    }

    #[test]
    fn a_file_that_is_not_a_reference_set_is_rejected() {
        assert!(ReferenceSet::decode(b"this is not a reference set at all").is_err());
    }

    #[test]
    fn an_unknown_style_byte_is_rejected() {
        let mut encoded = sample().encode();
        let style_at = HEADER_LEN + "fitted-2026".len() + 4;
        encoded[style_at] = 0x7F;
        assert!(ReferenceSet::decode(&encoded).is_err());
    }

    #[test]
    fn an_empty_set_round_trips_too() {
        let decoded = ReferenceSet::decode(&ReferenceSet::empty().encode()).unwrap();
        assert!(decoded.is_empty());
        assert_eq!(decoded.name(), "empty");
    }

    #[test]
    fn a_set_generated_for_another_grid_is_detectable() {
        let mut set = ReferenceSet::new("stale", vec![]);
        set.grid = FEATURE_GRID * 2;
        assert!(!set.matches_build_grid());
    }
}
