//! What a run promises whoever is watching it.
//!
//! The CLI draws the frames and tests them on its own side; what it cannot test is whether the
//! pipeline tells the truth about where it is. Two properties matter, and both are invariants of
//! the pipeline rather than of the renderer:
//!
//! - phases are opened and closed in pairs and do not overlap, so an observer needs no stack;
//! - a determinate phase reaches its total before it reports done. A bar that stops at 97% is a
//!   lie about where the work went, and the only place that can be guaranteed is here.

use std::path::PathBuf;
use std::sync::Mutex;

use subtrackt::core::progress::{Phase, Progress};
use subtrackt::{Config, Pipeline, UnmatchedPolicy};

/// One call, as it arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    Begin(Phase, Option<u64>),
    Advance(u64),
    End,
}

/// An observer that draws nothing and remembers everything.
#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<Event>>,
}

impl Recorder {
    fn events(&self) -> Vec<Event> {
        self.events
            .lock()
            .expect("no test panics while holding this")
            .clone()
    }

    fn push(&self, event: Event) {
        self.events
            .lock()
            .expect("no test panics while holding this")
            .push(event);
    }
}

impl Progress for Recorder {
    fn begin(&self, phase: Phase, total: Option<u64>) {
        self.push(Event::Begin(phase, total));
    }

    fn advance(&self, position: u64) {
        self.push(Event::Advance(position));
    }

    fn end(&self) {
        self.push(Event::End);
    }
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synthetic.sup")
}

/// Placeholders because nothing is embedded: without them the gate rejects the track and the run
/// never reaches the phases under test.
fn config() -> Config {
    Config { unmatched: UnmatchedPolicy::Placeholder, ..Config::default() }
}

fn recorded() -> (Vec<Event>, subtrackt::Outcome) {
    let recorder = Recorder::default();
    let outcome = Pipeline::new(config())
        .run_watched(fixture(), &recorder)
        .expect("the fixture extracts");
    (recorder.events(), outcome)
}

#[test]
fn a_run_opens_and_closes_every_phase_it_starts_and_never_nests_them() {
    let (events, _) = recorded();
    let mut open: Option<Phase> = None;
    let mut seen = Vec::new();

    for event in &events {
        match event {
            Event::Begin(phase, _) => {
                assert!(open.is_none(), "{phase:?} began while {open:?} was still open");
                open = Some(*phase);
                seen.push(*phase);
            }
            Event::Advance(_) => assert!(open.is_some(), "advanced with no phase open"),
            Event::End => {
                assert!(open.is_some(), "a phase ended twice");
                open = None;
            }
        }
    }

    assert!(open.is_none(), "the run finished with {open:?} still open");
    assert_eq!(
        seen,
        vec![Phase::Decode, Phase::Segment, Phase::Cluster, Phase::Read],
        "the order an observer can rely on"
    );
}

#[test]
fn a_determinate_phase_reaches_its_total_before_it_reports_done() {
    // The acceptance criterion of #83 at the only boundary that can guarantee it. The renderer can
    // draw whatever it is told; if the pipeline stops counting one short, every front end lies.
    let (events, outcome) = recorded();
    let mut checked = 0;

    let mut current: Option<(Phase, Option<u64>)> = None;
    let mut last = 0u64;
    for event in &events {
        match event {
            Event::Begin(phase, total) => {
                current = Some((*phase, *total));
                last = 0;
            }
            Event::Advance(position) => {
                assert!(*position > last, "positions are absolute and only ever climb");
                last = *position;
            }
            Event::End => {
                if let Some((phase, Some(total))) = current {
                    assert_eq!(last, total, "{phase:?} finished at {last} of {total}");
                    checked += 1;
                }
                current = None;
            }
        }
    }

    assert_eq!(checked, 2, "segmenting and reading are the determinate pair");
    // And the total was the real one, not a number the observer was handed for its own comfort.
    assert_eq!(
        events
            .iter()
            .filter(|e| **e == Event::Begin(Phase::Read, Some(outcome.report.images)))
            .count(),
        1,
        "the total is the image count the report ends up publishing"
    );
}

#[test]
fn the_indeterminate_phases_say_so_rather_than_inventing_a_total() {
    // Packets are streamed until the source is exhausted and clustering is one opaque call. A
    // total for either would be a made-up number, and this project does not print those.
    let (events, _) = recorded();
    for phase in [Phase::Decode, Phase::Cluster] {
        assert!(
            events.contains(&Event::Begin(phase, None)),
            "{phase:?} should have begun with no total: {events:?}"
        );
    }
}

#[test]
fn an_unobserved_run_produces_exactly_what_a_watched_one_does() {
    // The library keeps working with nothing attached, and observing it changes no output. If
    // these diverged, the observer would have become part of the pipeline rather than a witness.
    let (_, watched) = recorded();
    let plain = Pipeline::new(config())
        .run(fixture())
        .expect("the fixture extracts");

    assert_eq!(plain.track.cues.len(), watched.track.cues.len());
    assert_eq!(plain.report.to_string(), watched.report.to_string());
}
