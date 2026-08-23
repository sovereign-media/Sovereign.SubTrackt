//! What an extracted file says about its own making.
//!
//! Answers [#129]. A subtitle this tool produces is a derived artefact that outlives the run that
//! made it, and months later the two questions anyone asks of one are *what made this* and *what
//! did it match against*. The second is the one that is normally lost: a bad read is either the
//! code or the reference data, and `--version` already carries both for exactly that reason.
//!
//! # What is deliberately absent
//!
//! **There is no character error rate here, and there cannot be.** CER and WER are computed by
//! comparing against a reference transcript; an extraction has none, and `docs/fit-confidence.md`
//! is six statistics long on why nothing measurable grades a read without ground truth. A file
//! asserting its own accuracy would be the confident wrong answer this project rejects general OCR
//! for — worse here than in a log, because the claim would travel with the artefact forever.
//!
//! Every line written is a count or a measurement the extractor actually took.
//!
//! [#129]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/129

use std::time::{SystemTime, UNIX_EPOCH};

use subtrackt_core::Provenance;

use crate::report::Report;

/// Seconds in a day.
const SECS_PER_DAY: u64 = 86_400;

/// Build the note for a finished run.
///
/// `today` is passed in rather than read here so the whole of this is a pure function of its
/// inputs and can be tested against a fixed date. [`today_utc`] is the impure half, and it is one
/// line.
#[must_use]
pub fn note(report: &Report, today: (i64, u32, u32)) -> Provenance {
    let (y, m, d) = today;
    let mut lines = vec![
        format!(
            "Extracted by {} {} on {y:04}-{m:02}-{d:02}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
        ),
        format!("reference set: {}", report.reference_set),
    ];

    // Coverage and the tally, which are the two things a reader can act on: whether enough was
    // read, and whether what was read fitted. Written only when the run actually matched
    // something, because "0 of 0 glyphs (NaN%)" is noise rather than provenance.
    if report.glyphs > 0 {
        lines.push(format!(
            "glyphs: {} matched, {} unmatched, {} ambiguous ({:.1}% read)",
            report.matched,
            report.unmatched,
            report.ambiguous,
            report.confidence().ratio() * 100.0,
        ));
    }
    if report.matched > 0 {
        // The mean Hamming distance of the glyphs that matched. Coverage says how many found *a*
        // reference; this says how well they fitted it, and the two diverge — `docs/reference-set.md`
        // has a set matching 93.9% of glyphs and reading at 37.8% CER.
        lines.push(format!("mean match distance: {:.1}", report.mean_match_distance()));
    }

    Provenance::new(lines)
}

/// Today's date in UTC, as `(year, month, day)`.
///
/// UTC rather than local time, because the machine's zone is not a property of the extraction and
/// two runs of the same track should not disagree about the date because one crossed midnight in
/// Berlin.
#[must_use]
pub fn today_utc() -> (i64, u32, u32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    civil_from_days(i64::try_from(secs / SECS_PER_DAY).unwrap_or(0))
}

/// Convert days since the Unix epoch to a civil `(year, month, day)`.
///
/// Howard Hinnant's `civil_from_days`, which is the standard answer to this and is fifteen lines of
/// integer arithmetic. Written out rather than pulled in: `CLAUDE.md` asks that a library
/// dependency justify itself against a single static binary, and a date crate cannot — calendar
/// arithmetic is not this project's problem domain but it is also not `miniz_oxide`'s worth of
/// work, and unlike DEFLATE a subtle bug here is visible in the output as a wrong date rather than
/// invisible as a corrupt bitmap.
///
/// The era arithmetic shifts the year to start in March so the leap day lands at the end of it,
/// which is what removes the special-casing.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub const fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_is_the_first_of_january_nineteen_seventy() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn a_leap_day_is_not_skipped_or_doubled() {
        // 2000 is a leap year despite being a century, and 1900 is not. Both are the cases a
        // hand-rolled calendar gets wrong, which is the whole reason this has a test.
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(-25_509), (1900, 2, 28));
        assert_eq!(civil_from_days(-25_508), (1900, 3, 1));
    }

    #[test]
    fn dates_before_the_epoch_go_backwards_rather_than_wrapping() {
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(-365), (1969, 1, 1));
    }

    #[test]
    fn a_note_names_the_tool_the_date_and_the_data_it_matched_against() {
        let report = Report {
            reference_set: "arial-ri".into(),
            glyphs: 100,
            matched: 98,
            unmatched: 1,
            ambiguous: 1,
            distance_sum: 1049,
            ..Report::default()
        };
        let lines = note(&report, (2026, 8, 23)).lines;
        assert!(lines[0].contains("subtrackt"), "{lines:?}");
        assert!(lines[0].ends_with("on 2026-08-23"), "{lines:?}");
        assert!(lines[1].contains("arial-ri"), "the set is the half normally lost");
        assert!(lines[2].contains("98 matched"), "{lines:?}");
        assert!(lines[3].contains("10.7"), "{lines:?}");
    }

    #[test]
    fn a_note_claims_no_accuracy_it_cannot_measure() {
        // The property this whole module is shaped by. CER needs a reference transcript and an
        // extraction has none, so a number that looked like accuracy would be invented.
        let report = Report {
            reference_set: "arial".into(),
            glyphs: 10,
            matched: 10,
            ..Report::default()
        };
        let text = note(&report, (2026, 1, 1)).lines.join("\n").to_lowercase();
        assert!(!text.contains("cer"), "{text}");
        assert!(!text.contains("wer"), "{text}");
        assert!(!text.contains("accuracy"), "{text}");
        assert!(!text.contains("error rate"), "{text}");
    }

    #[test]
    fn a_run_that_matched_nothing_writes_no_ratio_at_all() {
        // Rather than "0 of 0 (NaN%)". A note is only worth having if every line in it is a fact.
        let lines = note(
            &Report { reference_set: "empty".into(), ..Report::default() },
            (2026, 1, 1),
        )
        .lines;
        assert_eq!(lines.len(), 2, "{lines:?}");
    }

    #[test]
    fn an_arrow_cannot_reach_the_note_and_end_the_block_early() {
        // A `-->` would close a WebVTT NOTE and read as a timing line in SubRip. The set name is
        // caller-supplied, so this is not a hypothetical the writer gets to ignore.
        let report = Report { reference_set: "weird --> name".into(), ..Report::default() };
        assert!(
            !note(&report, (2026, 1, 1))
                .lines
                .iter()
                .any(|l| l.contains("-->"))
        );
    }
}
