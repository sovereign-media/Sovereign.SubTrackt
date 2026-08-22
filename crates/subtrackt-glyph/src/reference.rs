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

use subtrackt_core::{Error, FEATURE_GRID, FeatureVector, LineMetrics, Result};

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
/// Version 2 added line metrics. Version 1 sets still load — their entries report metrics as
/// unknown, which is the truth about them rather than a default standing in for one.
const MAGIC: &[u8; 8] = b"SUBTREF\x02";

/// The version 1 magic, still readable.
const MAGIC_V1: &[u8; 8] = b"SUBTREF\x01";

/// Bytes per entry: codepoint, style, metrics-known flag, height, descent, then the vector.
const ENTRY_LEN: usize = 4 + 1 + 1 + 2 + 2 + 32;

/// Bytes per entry in version 1: codepoint, style, two reserved, then the vector.
const ENTRY_LEN_V1: usize = 4 + 1 + 2 + 32;

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

            // A version 1 entry has no metrics, so it reports unknown rather than a stand-in.
            let metrics = if entry_len == ENTRY_LEN && chunk[5] != 0 {
                LineMetrics::new(
                    u32::from(u16::from_le_bytes([chunk[6], chunk[7]])),
                    i32::from(i16::from_le_bytes([chunk[8], chunk[9]])),
                )
            } else {
                LineMetrics::UNKNOWN
            };

            let words_at = if entry_len == ENTRY_LEN { 10 } else { 7 };
            let mut words = [0u64; 4];
            for (index, word) in words.iter_mut().enumerate() {
                let at = words_at + index * 8;
                *word = u64::from_le_bytes(chunk[at..at + 8].try_into().expect("8 bytes"));
            }
            entries.push(ReferenceEntry {
                character,
                style,
                features: FeatureVector::from_words(words),
                metrics,
            });
        }

        Ok(Self { name, grid, entries })
    }
}

/// The reference set compiled into this binary.
///
/// Still empty. #8 established that a fixed set is worth having; it also established that the set
/// has to be fitted to the measured evidence rather than guessed, and until that is done shipping
/// nothing is the honest option.
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
                },
                ReferenceEntry {
                    character: '\u{e9}',
                    style: Style::Italic,
                    features: FeatureVector::EMPTY,
                    // An accented lowercase: taller than x-height, sitting on the baseline.
                    metrics: LineMetrics::new(96, -3),
                },
            ],
        )
    }

    #[test]
    fn the_embedded_set_is_empty_until_one_is_fitted_to_the_survey() {
        let set = embedded();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
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
