//! Drawing a spinner and a bar on stderr.
//!
//! Hand-rolled rather than taken from a crate, and the arithmetic below is the argument: drawing
//! `[####----] 43%` is forty lines of division, which is neither someone else's problem domain nor
//! a shallow dependency tree — the two halves of the test `CLAUDE.md` applies. The obvious crate
//! costs five more in the binary, one of which pulls `libc` and `windows-sys`.
//!
//! Two things here are not obvious.
//!
//! **There is one renderer, and it is a `static`.** A spinner and a log line share stderr and will
//! overwrite each other, so the frame has to be erased before anything else writes and redrawn
//! afterwards — which means the `tracing` subscriber's writer has to go through the renderer, and
//! `with_writer` wants `'static`. See [`StatusWriter`].
//!
//! **Nothing animates on a clock.** The spinner advances when work arrives, not on a timer, so
//! there is no thread to start and none to shut down. A phase that blocks for a minute inside one
//! call shows a frozen spinner, which is still the difference between "working" and "hung".

use std::io::Write;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use subtrackt_core::progress::{Phase, Progress};

/// Columns to draw within.
///
/// Fixed rather than queried: the width costs a dependency (`terminal_size`, and `libc` behind it)
/// and buys a bar that is occasionally the right length instead of occasionally too short. That is
/// not a trade worth making for a decoration.
const WIDTH: usize = 80;

/// Spinner frames. ASCII, because the same binary draws into a Windows console.
const SPINNER: [char; 4] = ['-', '\\', '|', '/'];

/// The shortest gap between two frames.
///
/// A film is tens of thousands of glyphs and the inner loop advances once per image; redrawing on
/// every one would cost more than the work being reported on.
const REDRAW_INTERVAL: Duration = Duration::from_millis(80);

/// Widest phase label, so the bars of consecutive phases line up.
const LABEL_WIDTH: usize = 10;

/// The one renderer.
pub struct Renderer {
    state: Mutex<State>,
}

/// What is currently on screen.
struct State {
    /// Off entirely: not a terminal, `--progress never`, or a verbosity that would drown it.
    enabled: bool,
    /// Columns the last frame occupied, so it can be erased exactly rather than by guessing.
    drawn: usize,
    /// The phase in flight. Phases do not overlap, so one slot is enough.
    active: Option<Active>,
}

/// A phase being drawn.
struct Active {
    phase: Phase,
    /// `None` for an indeterminate phase: a spinner rather than a bar.
    total: Option<u64>,
    position: u64,
    started: Instant,
    /// Which spinner frame comes next.
    spin: usize,
    /// When the last frame was drawn, for the redraw throttle.
    last: Option<Instant>,
}

static RENDERER: OnceLock<Renderer> = OnceLock::new();

/// The renderer, created disabled on first use.
///
/// Disabled is the safe default: anything that reaches for the renderer before `main` has decided
/// draws nothing, rather than drawing into a pipe.
pub fn renderer() -> &'static Renderer {
    RENDERER.get_or_init(|| Renderer {
        state: Mutex::new(State { enabled: false, drawn: 0, active: None }),
    })
}

/// Turn drawing on or off, and hand back the renderer to attach to a run.
pub fn install(enabled: bool) -> &'static Renderer {
    let renderer = renderer();
    renderer.with(|state| state.enabled = enabled);
    renderer
}

impl Renderer {
    /// Run `f` against the state, recovering rather than propagating a poisoned lock.
    ///
    /// Nothing here is worth failing a run over: the whole module draws decoration, and a panic in
    /// one thread should not take the extraction down with it.
    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        f(&mut self.state.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Erase the current frame so something else can write to stderr.
    ///
    /// Paired with [`Self::redraw`]. Every path that writes a line to stderr — the status channel
    /// and the `tracing` subscriber both — has to bracket itself in these two, or the line and the
    /// frame land on top of each other.
    pub fn clear(&self) {
        self.with(|state| {
            let mut out = std::io::stderr().lock();
            state.erase(&mut out);
        });
    }

    /// Redraw whatever [`Self::clear`] erased, if a phase is still running.
    pub fn redraw(&self) {
        self.with(|state| {
            let mut out = std::io::stderr().lock();
            state.draw(&mut out, true);
        });
    }
}

impl Progress for Renderer {
    fn begin(&self, phase: Phase, total: Option<u64>) {
        self.with(|state| {
            let mut out = std::io::stderr().lock();
            state.erase(&mut out);
            state.active = Some(Active {
                phase,
                total,
                position: 0,
                started: Instant::now(),
                spin: 0,
                last: None,
            });
            // Drawn straight away rather than on the first advance: a phase whose first unit of
            // work takes ten seconds would otherwise look exactly like a hang, which is the thing
            // this exists to rule out.
            state.draw(&mut out, true);
        });
    }

    fn advance(&self, position: u64) {
        self.with(|state| {
            let Some(active) = state.active.as_mut() else {
                return;
            };
            active.position = position;
            // The last unit always redraws, whatever the throttle says. A bar that stops at 97%
            // because the final frame was rate-limited away is a lie about where the work went.
            let complete = active.total == Some(position);
            let mut out = std::io::stderr().lock();
            state.draw(&mut out, complete);
        });
    }

    fn end(&self) {
        self.with(|state| {
            let mut out = std::io::stderr().lock();
            state.erase(&mut out);
            state.active = None;
        });
    }
}

impl State {
    /// Blank the line the last frame occupied, leaving the cursor at column zero.
    fn erase(&mut self, out: &mut dyn Write) {
        if self.drawn == 0 {
            return;
        }
        let _ = write!(out, "\r{:width$}\r", "", width = self.drawn);
        let _ = out.flush();
        self.drawn = 0;
    }

    /// Draw the current frame, subject to the redraw throttle unless `force`.
    fn draw(&mut self, out: &mut dyn Write, force: bool) {
        if !self.enabled {
            return;
        }
        let Some(active) = self.active.as_mut() else {
            return;
        };

        let now = Instant::now();
        if !force
            && active
                .last
                .is_some_and(|last| now.duration_since(last) < REDRAW_INTERVAL)
        {
            return;
        }
        active.last = Some(now);

        let text = frame(
            active.phase,
            active.position,
            active.total,
            now.duration_since(active.started),
            active.spin,
            WIDTH,
        );
        active.spin = active.spin.wrapping_add(1);

        // Padded out to whatever the previous frame occupied rather than erased first, so a frame
        // is one write and the line never blinks. Frames are ASCII, so bytes are columns.
        let pad = self.drawn.saturating_sub(text.len());
        let _ = write!(out, "\r{text}{:pad$}", "", pad = pad);
        let _ = out.flush();
        self.drawn = text.len();
    }
}

/// One frame, without the carriage return that positions it.
///
/// Pure, so the shape of every case can be tested without a terminal — which is the same reason
/// [`crate::style::use_color`] takes its environment as an argument.
fn frame(
    phase: Phase,
    position: u64,
    total: Option<u64>,
    elapsed: Duration,
    spin: usize,
    width: usize,
) -> String {
    let label = phase.label();
    match total {
        // Determinate: a bar, a percentage, and how long the rest is expected to take.
        Some(total) if total > 0 => {
            let done = position.min(total);
            let percent = done * 100 / total;
            let tail = format!(
                "  {percent:>3}%  {done}/{total} {}  {}{}",
                phase.unit(),
                short(elapsed),
                eta(elapsed, done, total)
            );
            let cells = width
                .saturating_sub(LABEL_WIDTH + 3 + tail.len())
                .clamp(8, 40);
            #[allow(clippy::cast_possible_truncation)]
            let filled = ((done * cells as u64) / total) as usize;
            format!(
                "{label:<LABEL_WIDTH$} [{}{}]{tail}",
                "#".repeat(filled),
                "-".repeat(cells - filled),
            )
        }
        // Indeterminate, or determinate over nothing at all — which is the same picture, since a
        // bar over zero units has nowhere to go.
        _ => {
            let spinner = SPINNER[spin % SPINNER.len()];
            let counted = if position == 0 {
                String::new()
            } else {
                format!("{position} {}  ", phase.unit())
            };
            format!("{label:<LABEL_WIDTH$} {spinner}  {counted}{}", short(elapsed))
        }
    }
}

/// How long is left, from how long the work so far took.
///
/// Empty until something has been done, because an estimate from a sample of nothing is a made-up
/// number, and this project does not print those.
#[allow(clippy::cast_precision_loss)]
fn eta(elapsed: Duration, done: u64, total: u64) -> String {
    if done == 0 || done >= total {
        return String::new();
    }
    let remaining = elapsed.as_secs_f64() * (total - done) as f64 / done as f64;
    format!("  eta {}", short(Duration::from_secs_f64(remaining)))
}

/// A duration at the precision a person watching a terminal can use.
fn short(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{:.1}s", duration.as_secs_f32())
    } else {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    }
}

/// Sends `tracing` output to stderr around whatever the renderer has on screen.
///
/// This is why the renderer is a `static`. A subscriber's writer has to outlive everything, so it
/// cannot borrow a renderer owned by `main`; and without going through the renderer at all, a log
/// line emitted mid-phase lands on top of the frame and both become unreadable.
#[derive(Debug, Clone, Copy)]
pub struct StatusWriter;

impl tracing_subscriber::fmt::MakeWriter<'_> for StatusWriter {
    type Writer = StatusLine;

    fn make_writer(&self) -> Self::Writer {
        renderer().clear();
        StatusLine(anstream::stderr())
    }
}

/// One log line's worth of stderr, which redraws the frame when it is dropped.
///
/// Goes through `anstream` rather than straight to the handle so that a Windows console gets the
/// colours rather than the escape codes that would have produced them.
pub struct StatusLine(anstream::Stderr);

impl Write for StatusLine {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl Drop for StatusLine {
    fn drop(&mut self) {
        let _ = self.0.flush();
        renderer().redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A state that draws, and a buffer to draw into. Every test here works on the writer rather
    /// than on a terminal, which is the whole reason [`State::draw`] takes one.
    fn drawing() -> (State, Vec<u8>) {
        (State { enabled: true, drawn: 0, active: None }, Vec::new())
    }

    fn drawn(buffer: &[u8]) -> String {
        String::from_utf8(buffer.to_vec()).expect("frames are ASCII")
    }

    #[test]
    fn a_determinate_frame_reaches_a_hundred_percent_at_the_last_unit() {
        // The acceptance criterion of #83, as arithmetic: a bar that stops at 97% is a lie about
        // where the work went.
        let full = frame(Phase::Read, 28, Some(28), Duration::from_secs(3), 0, WIDTH);
        assert!(full.contains("100%"), "{full}");
        assert!(!full.contains('-'), "at the end the bar has no empty cells: {full}");

        let half = frame(Phase::Read, 14, Some(28), Duration::from_secs(3), 0, WIDTH);
        assert!(half.contains(" 50%"), "{half}");
        assert!(half.contains('#') && half.contains('-'), "{half}");
    }

    #[test]
    fn a_frame_names_the_phase_and_what_it_is_counting() {
        // A bare number is not a status. `1483` could be packets, cues or milliseconds.
        let bar = frame(Phase::Segment, 3, Some(10), Duration::from_secs(1), 0, WIDTH);
        assert!(bar.starts_with("segmenting"), "{bar}");
        assert!(bar.contains("3/10 images"), "{bar}");

        let spinner = frame(Phase::Decode, 1483, None, Duration::from_secs(1), 0, WIDTH);
        assert!(spinner.starts_with("decoding"), "{spinner}");
        assert!(spinner.contains("1483 packets"), "{spinner}");
    }

    #[test]
    fn a_frame_fits_the_width_it_was_given() {
        // Wrapping would leave the previous line on screen, and every erase after it would blank
        // the wrong row.
        for position in [0, 1, 500, 999, 1000] {
            for total in [None, Some(1000)] {
                let text = frame(Phase::Read, position, total, Duration::from_secs(90), 2, WIDTH);
                assert!(text.len() <= WIDTH, "{} columns: {text}", text.len());
            }
        }
    }

    #[test]
    fn an_indeterminate_phase_that_has_done_nothing_yet_still_says_it_is_running() {
        // What `clustering` looks like: one call, no way to see inside it, nothing to count.
        let text = frame(Phase::Cluster, 0, None, Duration::from_millis(200), 0, WIDTH);
        assert!(text.contains("clustering"), "{text}");
        assert!(
            !text.contains("0 shapes"),
            "counting nothing is worse than not counting: {text}"
        );
    }

    #[test]
    fn a_total_of_zero_spins_rather_than_dividing_by_it() {
        let text = frame(Phase::Read, 0, Some(0), Duration::ZERO, 0, WIDTH);
        assert!(text.contains("reading"), "{text}");
        assert!(!text.contains('%'), "a bar over no units has nowhere to go: {text}");
    }

    #[test]
    fn the_estimate_waits_until_there_is_something_to_estimate_from() {
        assert_eq!(eta(Duration::from_secs(10), 0, 100), "", "no sample, no number");
        assert_eq!(eta(Duration::from_secs(10), 100, 100), "", "nothing left to wait for");
        assert_eq!(eta(Duration::from_secs(10), 25, 100), "  eta 30.0s");
    }

    #[test]
    fn durations_switch_to_minutes_before_they_stop_being_readable() {
        assert_eq!(short(Duration::from_millis(1234)), "1.2s");
        assert_eq!(short(Duration::from_secs(59)), "59.0s");
        assert_eq!(short(Duration::from_secs(63)), "1m03s");
        assert_eq!(short(Duration::from_secs(3600)), "60m00s");
    }

    #[test]
    fn a_phase_appears_the_moment_it_begins_rather_than_on_its_first_unit_of_work() {
        // Progress has to appear within a second of a phase starting. The first unit of work is
        // not a bound on that -- decoding a Blu-ray's first packet can take longer than the
        // patience the spinner exists to buy.
        let (mut state, mut buffer) = drawing();
        state.active = Some(Active {
            phase: Phase::Decode,
            total: None,
            position: 0,
            started: Instant::now(),
            spin: 0,
            last: None,
        });
        state.draw(&mut buffer, true);
        assert!(drawn(&buffer).contains("decoding"), "{:?}", drawn(&buffer));
    }

    #[test]
    fn erasing_blanks_exactly_what_was_drawn_and_nothing_else() {
        // Too few spaces leaves the tail of the frame behind a log line; too many wrap the line
        // and scroll something else off the top.
        let (mut state, mut buffer) = drawing();
        state.active = Some(Active {
            phase: Phase::Read,
            total: Some(4),
            position: 2,
            started: Instant::now(),
            spin: 0,
            last: None,
        });
        state.draw(&mut buffer, true);
        let width = state.drawn;
        assert!(width > 0);

        buffer.clear();
        state.erase(&mut buffer);
        assert_eq!(drawn(&buffer), format!("\r{}\r", " ".repeat(width)));
        assert_eq!(state.drawn, 0);

        buffer.clear();
        state.erase(&mut buffer);
        assert!(buffer.is_empty(), "erasing twice writes nothing the second time");
    }

    #[test]
    fn a_disabled_renderer_writes_no_bytes_at_all() {
        // Not "writes escape-free bytes": none. This is what makes a redirected run identical to
        // a `--plain` one, and what keeps `2> run.log` free of carriage returns.
        let (mut state, mut buffer) = drawing();
        state.enabled = false;
        state.active = Some(Active {
            phase: Phase::Read,
            total: Some(4),
            position: 1,
            started: Instant::now(),
            spin: 0,
            last: None,
        });
        state.draw(&mut buffer, true);
        assert!(buffer.is_empty());
        assert_eq!(state.drawn, 0);
    }

    #[test]
    fn the_throttle_holds_frames_back_but_never_the_last_one() {
        let (mut state, mut buffer) = drawing();
        state.active = Some(Active {
            phase: Phase::Read,
            total: Some(2),
            position: 1,
            started: Instant::now(),
            spin: 0,
            last: None,
        });
        state.draw(&mut buffer, true);

        buffer.clear();
        state.draw(&mut buffer, false);
        assert!(buffer.is_empty(), "a second frame inside the interval is dropped");

        buffer.clear();
        if let Some(active) = state.active.as_mut() {
            active.position = 2;
        }
        state.draw(&mut buffer, true);
        assert!(drawn(&buffer).contains("100%"), "{:?}", drawn(&buffer));
    }
}
