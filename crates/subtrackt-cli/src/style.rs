//! Whether stderr is decorated, and what severity looks like when it is.
//!
//! Two decisions live here, kept apart from the code that acts on them. Both are pure functions of
//! a flag, the environment and whether stderr is a terminal, so the whole matrix is unit-testable
//! and no test needs a pty. Detection is the part that serves automation: a piped or redirected
//! run switches both off on its own, so CI needs no flag at all and `subtrackt extract ... 2>
//! run.log` does not fill the log with escape codes.

use std::fmt;
use std::io::{IsTerminal, Write as _};

use clap::ValueEnum;

/// A three-state switch, as `--color` and `--progress` both take it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum When {
    /// Decide from the environment: on for a terminal, off for a pipe or a file.
    Auto,
    /// On regardless, for the case detection gets wrong.
    Always,
    /// Off regardless.
    Never,
}

/// Everything outside the program that the decisions depend on.
///
/// Captured into a value at the edge so that the decisions themselves are pure. The alternative is
/// a decision that reads the environment as it goes, which can only be exercised by a test that
/// owns the process environment and a terminal — and a `cargo test` run has neither.
///
/// Four independent probes, so four bools. Folding them into two-variant enums to satisfy the lint
/// would name each state twice and make the test matrix below unreadable, which is the opposite of
/// what this type is for.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Env {
    /// `NO_COLOR` is set to something non-empty.
    pub no_color: bool,
    /// `CLICOLOR_FORCE` is set to something other than `0`.
    pub clicolor_force: bool,
    /// `TERM` names a terminal that cannot render escape codes, or names nothing at all.
    pub dumb_terminal: bool,
    /// Stderr is attached to a terminal rather than to a pipe or a file.
    pub stderr_is_terminal: bool,
}

impl Env {
    /// Read the environment this process was actually started in.
    #[must_use]
    pub fn detect() -> Self {
        Self {
            no_color: anstyle_query::no_color(),
            clicolor_force: anstyle_query::clicolor_force(),
            dumb_terminal: !anstyle_query::term_supports_color(),
            stderr_is_terminal: std::io::stderr().is_terminal(),
        }
    }
}

/// Whether stderr gets colour.
///
/// `NO_COLOR` is honoured under `auto`, and `--color always` overrides it — a flag typed on the
/// command line is a later and more specific instruction than an exported variable, and nobody who
/// wanted no colour would have typed it.
#[must_use]
pub fn use_color(choice: When, env: Env) -> bool {
    match choice {
        When::Always => true,
        When::Never => false,
        When::Auto => {
            if env.no_color {
                false
            } else if env.clicolor_force {
                true
            } else {
                env.stderr_is_terminal && !env.dumb_terminal
            }
        }
    }
}

/// Above this verbosity, `auto` stops drawing progress.
///
/// `-vv` turns on debug logging, which emits lines faster than a frame can be redrawn between
/// them. The bar is not wrong at that point so much as useless, and it costs a redraw per log line
/// to stay that way.
const PROGRESS_VERBOSITY_CEILING: u8 = 2;

/// Whether stderr gets a spinner and a bar.
///
/// Deliberately not the same function as [`use_color`], even though the two agree over most of the
/// matrix. Progress is animation and colour is not: a redirected run that could survive a stray
/// escape code still cannot survive thousands of carriage-return frames, and only progress cares
/// how much is already being logged.
#[must_use]
pub fn show_progress(choice: When, env: Env, verbosity: u8) -> bool {
    match choice {
        When::Always => true,
        When::Never => false,
        When::Auto => {
            env.stderr_is_terminal && !env.dumb_terminal && verbosity < PROGRESS_VERBOSITY_CEILING
        }
    }
}

/// How much a line matters. Three of them, because three colours that mean exactly one thing each
/// stay readable and a palette does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// A fact: what was loaded, what was written, how many of something there are.
    Info,
    /// Something the run carried on through that the reader should still know about.
    Warn,
    /// The run stopped.
    Error,
}

impl Severity {
    /// The style this severity is drawn in.
    #[must_use]
    pub const fn style(self) -> anstyle::Style {
        let colour = match self {
            Self::Info => anstyle::AnsiColor::Blue,
            Self::Warn => anstyle::AnsiColor::Yellow,
            Self::Error => anstyle::AnsiColor::Red,
        };
        anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(colour)))
    }
}

/// The status channel: everything the tool says about itself, as opposed to the data it produces.
///
/// Every line written through here goes to stderr. Not one escape byte may reach stdout, ever —
/// `extract` writes SRT there, `glyphs` writes TSV, and a coloured `.srt` is a corrupt `.srt`. The
/// split is kept by there being no path from this type to stdout at all.
#[derive(Debug, Clone, Copy)]
pub struct Ui {
    color: bool,
}

impl Ui {
    /// A status channel that colours its output, or does not.
    #[must_use]
    pub const fn new(color: bool) -> Self {
        Self { color }
    }

    /// A fact.
    pub fn info(self, message: impl fmt::Display) {
        self.emit(Some(Severity::Info), &message);
    }

    /// Something worth knowing that did not stop the run.
    pub fn warn(self, message: impl fmt::Display) {
        self.emit(Some(Severity::Warn), &message);
    }

    /// Why the run stopped.
    pub fn error(self, message: impl fmt::Display) {
        self.emit(Some(Severity::Error), &message);
    }

    /// Output that carries no severity, and so gets no colour.
    ///
    /// The `--report` counters and `fit`'s score table go through here. Colour marks severity, not
    /// data: a coloured number invites the reader to look for a meaning that is not there.
    pub fn plain(self, message: impl fmt::Display) {
        self.emit(None, &message);
    }

    /// One line, written around whatever the progress renderer has on screen.
    fn emit(self, severity: Option<Severity>, message: &dyn fmt::Display) {
        let renderer = crate::progress::renderer();
        renderer.clear();
        // Scoped so the stderr lock is released before the redraw asks for the renderer's. Taking
        // the two in opposite orders on two threads is the classic way to hang a process, and
        // nothing about this being decoration would make that any less permanent.
        {
            let mut out = anstream::stderr().lock();
            // A status line that will not write is not worth failing a run over, and there is
            // nowhere left to report the failure to.
            let _ = match severity.filter(|_| self.color) {
                Some(severity) => {
                    let style = severity.style();
                    writeln!(out, "{style}{message}{style:#}")
                }
                None => writeln!(out, "{message}"),
            };
            let _ = out.flush();
        }
        renderer.redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terminal, with nothing in the environment saying otherwise.
    const TERMINAL: Env = Env {
        no_color: false,
        clicolor_force: false,
        dumb_terminal: false,
        stderr_is_terminal: true,
    };
    /// A pipe or a redirect, which is what CI and `2> run.log` both look like.
    const REDIRECTED: Env = Env { stderr_is_terminal: false, ..TERMINAL };

    #[test]
    fn colour_follows_the_terminal_when_nothing_overrides_it() {
        assert!(use_color(When::Auto, TERMINAL));
        assert!(!use_color(When::Auto, REDIRECTED));
        assert!(
            !use_color(When::Auto, Env { dumb_terminal: true, ..TERMINAL }),
            "a terminal that cannot render escapes is not one to send them to"
        );
    }

    #[test]
    fn the_flag_beats_the_environment_in_both_directions() {
        for env in [TERMINAL, REDIRECTED, Env { no_color: true, ..TERMINAL }] {
            assert!(use_color(When::Always, env), "{env:?}");
            assert!(!use_color(When::Never, env), "{env:?}");
        }
    }

    #[test]
    fn no_color_is_honoured_and_color_always_overrides_it() {
        // The half of the NO_COLOR convention that gets forgotten: it sets the default, it is not
        // a veto. Someone who exported it and then typed `--color always` typed the later and more
        // specific of the two.
        let env = Env { no_color: true, ..TERMINAL };
        assert!(!use_color(When::Auto, env));
        assert!(use_color(When::Always, env));
    }

    #[test]
    fn clicolor_force_turns_colour_on_for_a_pipe_but_no_color_still_wins() {
        assert!(use_color(When::Auto, Env { clicolor_force: true, ..REDIRECTED }));
        assert!(
            !use_color(When::Auto, Env { clicolor_force: true, no_color: true, ..REDIRECTED }),
            "NO_COLOR is checked first, which is what its specification asks for"
        );
    }

    #[test]
    fn progress_switches_itself_off_for_anything_that_is_not_a_terminal() {
        // The property that matters more than the flag does: without it, a redirected run fills
        // the log with thousands of carriage-return frames and CI would need a flag to stop it.
        assert!(show_progress(When::Auto, TERMINAL, 0));
        assert!(!show_progress(When::Auto, REDIRECTED, 0));
        assert!(!show_progress(When::Auto, Env { dumb_terminal: true, ..TERMINAL }, 0));
    }

    #[test]
    fn progress_gives_up_once_the_log_is_loud_enough_to_drown_it() {
        assert!(show_progress(When::Auto, TERMINAL, 1), "-v is still quiet enough");
        assert!(!show_progress(When::Auto, TERMINAL, 2), "-vv is debug logging");
        assert!(
            show_progress(When::Always, TERMINAL, 9),
            "asked for explicitly, it is drawn anyway"
        );
    }

    #[test]
    fn a_plain_run_and_a_redirected_one_land_in_the_same_place() {
        // What `--plain` resolves to, against what a redirected run reaches on its own. The two
        // have to agree or `--plain` is not what it claims to be.
        assert_eq!(use_color(When::Never, TERMINAL), use_color(When::Auto, REDIRECTED));
        assert_eq!(
            show_progress(When::Never, TERMINAL, 0),
            show_progress(When::Auto, REDIRECTED, 0)
        );
        assert!(!use_color(When::Never, TERMINAL));
        assert!(!show_progress(When::Never, TERMINAL, 0));
    }

    #[test]
    fn each_severity_renders_a_different_escape_sequence() {
        let rendered = |severity: Severity| severity.style().render().to_string();
        assert_ne!(rendered(Severity::Info), rendered(Severity::Warn));
        assert_ne!(rendered(Severity::Warn), rendered(Severity::Error));
        assert!(rendered(Severity::Error).contains('\u{1b}'));
    }
}
