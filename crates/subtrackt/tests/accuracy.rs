//! Regression cover over the checked-in fixture.
//!
//! These assertions are structural — cue count, line splitting, timing, glyph count — and need no
//! reference set, so they run everywhere including CI. They are what catches a decode, segmentation
//! or layout regression as a failing test rather than as a quietly worse accuracy number.
//!
//! The *correctness* half lives in `cargo run -p xtask -- accuracy`, which generates a fixture and
//! a reference set from the same font and scores the text. It needs a font on the machine, which is
//! why it is a CI step rather than a test here.

use std::path::PathBuf;

use subtrackt::score::score_text;
use subtrackt::{Config, Pipeline, UnmatchedPolicy};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn ground_truth() -> String {
    std::fs::read_to_string(fixture("synthetic.txt")).expect("ground truth is checked in")
}

/// Run the pipeline over the fixture with placeholders, so nothing is dropped by the gate.
fn extract() -> subtrackt::Outcome {
    let config = Config { unmatched: UnmatchedPolicy::Placeholder, ..Config::default() };
    Pipeline::new(config)
        .run(fixture("synthetic.sup"))
        .expect("the fixture extracts")
}

#[test]
fn the_fixture_decodes_to_the_expected_cues() {
    let outcome = extract();
    let truth = ground_truth();

    assert_eq!(outcome.report.packets, 36, "6 cues, each a display set and a clear");
    assert_eq!(outcome.report.images, 6);
    assert_eq!(outcome.track.cues.len(), 6);

    // Cue three is the two-speaker exchange and cue six the post-correction case; those are the
    // two with a second line.
    let lines: Vec<usize> = outcome.track.cues.iter().map(|c| c.lines.len()).collect();
    assert_eq!(
        lines,
        vec![1, 1, 2, 1, 1, 2],
        "line splitting comes from the row projection"
    );
    assert_eq!(truth.trim().lines().count(), 8, "eight lines across six cues");
}

#[test]
fn cue_timings_are_two_seconds_three_seconds_apart() {
    let outcome = extract();
    let spans: Vec<(u64, u64)> = outcome
        .track
        .cues
        .iter()
        .map(|c| (c.span.start.as_millis(), c.span.end.as_millis()))
        .collect();

    assert_eq!(spans[0], (1_000, 3_000));
    assert_eq!(spans[1], (4_000, 6_000));
    assert_eq!(spans[4], (13_000, 15_000));
    assert_eq!(spans[5], (16_000, 18_000));
}

#[test]
fn segmentation_finds_about_one_glyph_per_visible_character() {
    // Not exact: a colon is one glyph from two components, an accented letter is one from two, and
    // spaces are not glyphs at all. A large drift either way means segmentation has regressed —
    // fusing characters together or shattering them apart.
    let outcome = extract();
    let visible = ground_truth()
        .chars()
        .filter(|c| !c.is_whitespace())
        .count();
    let glyphs = usize::try_from(outcome.report.glyphs).unwrap_or(usize::MAX);

    // The upper bound is above 1:1 on purpose: a diacritic that fails to group, or punctuation
    // that clusters imperfectly, yields more glyphs than characters. Both directions are the
    // regression this guards.
    assert!(
        glyphs > visible * 3 / 4 && glyphs < visible * 5 / 4,
        "{glyphs} glyphs for {visible} visible characters is outside the expected range"
    );
}

#[test]
fn every_glyph_is_unmatched_without_a_reference_set() {
    // Nothing is embedded, and #9 closed by measuring that shipping a set would read worse than
    // shipping none. This pins that the pipeline says so honestly rather than guessing.
    let outcome = extract();
    assert_eq!(outcome.report.matched, 0);
    assert!(outcome.report.unmatched > 0);
    assert!(!outcome.report.confidence().is_complete());
}

#[test]
fn glyphs_are_measured_against_their_own_text_line() {
    // The feature #37 added, pinned end to end. Without it `o` and `O` normalise to nearly the same
    // vector; with it they are separated by how tall each stands in its line.
    let outcome = extract();
    let measured = outcome
        .track
        .cues
        .iter()
        .flat_map(|c| c.lines.iter())
        .count();
    assert!(measured > 0, "the fixture produces lines to measure");

    // Every cue in the fixture is a full line of mixed-case text, which is exactly the case the
    // anchors can be found for. A fixture that stopped producing measurable lines would silently
    // disable the feature rather than fail, so it is worth asserting.
    assert_eq!(
        outcome.report.glyphs_without_metrics, 0,
        "every glyph in the fixture sits on a measurable line"
    );
}

#[test]
fn post_correction_is_off_unless_it_is_asked_for_and_the_report_says_which() {
    // The switch, end to end. Nothing matches without a reference set, so neither run has an
    // ambiguous glyph to work on and both correct nothing — which is the point: `0 corrections`
    // is ambiguous on its own, and the corrector's name in the report is what resolves it.
    let base = Config { unmatched: UnmatchedPolicy::Placeholder, ..Config::default() };

    let off = Pipeline::new(base)
        .run(fixture("synthetic.sup"))
        .expect("the fixture extracts");
    assert_eq!(off.report.corrector, "none");
    assert!(off.corrections.is_empty());

    let on = Pipeline::new(Config { post_correct: true, ..base })
        .run(fixture("synthetic.sup"))
        .expect("the fixture extracts");
    assert_eq!(on.report.corrector, "context");
    assert_eq!(on.report.corrections, 0, "an unread track offers nothing to correct");
    assert_eq!(
        on.track, off.track,
        "with nothing flagged ambiguous, running the corrector must change nothing at all"
    );
}

#[test]
fn the_default_gate_refuses_a_track_it_cannot_read_and_says_by_how_much() {
    // With nothing embedded no glyph matches, so the floor must reject rather than emit a track of
    // placeholders — and the caller being told to fall back to burn-in gets the numbers behind it.
    let err = Pipeline::new(Config::default())
        .run(fixture("synthetic.sup"))
        .unwrap_err();

    let subtrackt::Error::TrackRejected { policy, matched, unmatched, required } = &err else {
        panic!("got {err:?}");
    };
    assert_eq!(*policy, "threshold");
    assert_eq!(*matched, 0, "the embedded reference set is empty");
    assert!(*unmatched > 0);
    assert!((*required - subtrackt::config::DEFAULT_MIN_MATCHED).abs() < f32::EPSILON);
    assert!(err.to_string().contains("floor is 90.0%"), "{err}");
}

#[test]
fn scoring_an_unread_track_against_the_truth_reports_total_failure() {
    // The harness has to give the honest answer in the worst case, or it cannot be trusted in the
    // good one.
    let outcome = extract();
    let text: Vec<String> = outcome
        .track
        .cues
        .iter()
        .map(subtrackt::core::Cue::text)
        .collect();
    let score = score_text(ground_truth().trim(), text.join("\n").trim());

    // Not 1.0: whitespace and line breaks are reproduced correctly even when every glyph is a
    // placeholder, so a perfectly-failed read still scores below total.
    assert!(
        score.character_error_rate() > 0.7,
        "nothing was read, so most of it must be wrong; got {:.2}",
        score.character_error_rate()
    );
    assert!(!score.is_exact());
}
