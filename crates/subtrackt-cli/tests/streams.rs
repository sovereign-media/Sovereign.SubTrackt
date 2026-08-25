//! What lands on each stream.
//!
//! The split between stdout and stderr is already right in the code; what this pins is that it
//! stays right. `extract` writes SRT to stdout and `glyphs` writes TSV, and a single escape byte
//! in either makes the file corrupt in a way that shows up much later and somewhere else. A habit
//! is not a guarantee, so the guarantee is here.
//!
//! Every run below has its stdout and stderr piped, which is exactly what a redirect looks like to
//! the detection in `style`. That is the point: automation is served by detection rather than by
//! flags, so these tests exercise the same path CI takes.

use std::path::PathBuf;
use std::process::{Command, Output};

/// The escape byte no subtitle file may contain.
const ESC: u8 = 0x1b;

/// The fixture the workspace already generates and checks in.
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../subtrackt/tests/fixtures/synthetic.sup")
}

/// Run the binary with the ambient colour environment cleared, so a maintainer who exports
/// `NO_COLOR` gets the same result as CI does.
fn run(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_subtrackt"));
    command
        .args(args)
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("RUST_LOG");
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("the binary runs")
}

/// `extract` over the fixture. Placeholders because nothing is embedded, so without them the
/// accuracy gate rejects the track and there is no stdout to inspect.
fn extract(extra: &[&str]) -> Output {
    let path = fixture();
    let mut args = vec![
        "extract",
        path.to_str().expect("the fixture path is UTF-8"),
        "--on-unmatched",
        "placeholder",
    ];
    args.extend_from_slice(extra);
    run(&args, &[])
}

fn glyphs(extra: &[&str]) -> Output {
    let path = fixture();
    let mut args = vec!["glyphs", path.to_str().expect("the fixture path is UTF-8")];
    args.extend_from_slice(extra);
    run(&args, &[])
}

#[test]
fn extract_styles_stderr_without_putting_one_escape_byte_into_the_srt() {
    let output = extract(&["--color", "always"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    assert!(
        !output.stdout.contains(&ESC),
        "a coloured .srt is a corrupt .srt: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stdout.starts_with(b"1\n"),
        "and it is still SubRip: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stderr.contains(&ESC),
        "while the status channel is styled: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn glyphs_styles_stderr_without_putting_one_escape_byte_into_the_rows() {
    let output = glyphs(&["--color", "always"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    assert!(!output.stdout.contains(&ESC), "the rows are data and stay undecorated");
    assert!(output.stderr.contains(&ESC), "while the summary line is styled");

    // Seven fields per row, on more than one row. Written as a split rather than a byte tally so
    // that clippy does not send the workspace to a crate for counting to six.
    let rows = String::from_utf8(output.stdout).expect("the rows are UTF-8");
    let widths: Vec<usize> = rows.lines().map(|row| row.split('\t').count()).collect();
    assert!(widths.len() > 1, "{rows:?}");
    assert!(widths.iter().all(|width| *width == 7), "{widths:?}");
}

#[test]
fn a_redirected_run_leaves_neither_escapes_nor_frames_in_the_log() {
    // `subtrackt extract ... 2> run.log` is the case that matters more than any flag: without
    // detection the log fills with thousands of carriage-return frames and nothing is readable.
    for output in [extract(&[]), glyphs(&[]), extract(&["--report"])] {
        let stderr = output.stderr;
        assert!(!stderr.contains(&ESC), "{:?}", String::from_utf8_lossy(&stderr));
        assert!(!stderr.contains(&b'\r'), "{:?}", String::from_utf8_lossy(&stderr));
    }
}

/// Replace every `<digits>.<digit>s` duration with `Ns`, so two runs of `--report` can be compared
/// byte for byte without comparing their clocks.
///
/// #199. The two runs below are separate processes, and `--report` prints wall-clock durations at
/// one decimal place -- so on a loaded runner one run's `segment` phase lands on the far side of
/// the 0.05s rounding boundary and the other does not, and a test about *stream routing* fails for
/// a reason that has nothing to do with streams. It flaked on Linux and Windows alike. Masking
/// rather than dropping the line keeps the rest of it -- the phase names, their order, the
/// separators, the resident-memory figures -- under the assertion, because that is the part that
/// would actually differ if `--plain` landed somewhere a redirect does not.
fn without_durations(stream: &[u8]) -> String {
    let text = String::from_utf8_lossy(stream);
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_ref();
    while let Some(s) = rest.find('s') {
        // A duration is the digits and one decimal place immediately before an `s`. Walk back over
        // them rather than reaching for a regex, which would be this crate's first new dependency.
        let head = &rest[..s];
        let digits = head.len() - head.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let stripped = &head[..head.len() - digits];
        let is_duration = digits == 1
            && stripped.ends_with('.')
            && stripped[..stripped.len() - 1].ends_with(|c: char| c.is_ascii_digit());
        if is_duration {
            let whole = &stripped[..stripped.len() - 1];
            let start = whole.trim_end_matches(|c: char| c.is_ascii_digit()).len();
            out.push_str(&head[..start]);
            out.push_str("Ns");
        } else {
            out.push_str(head);
            out.push('s');
        }
        rest = &rest[s + 1..];
    }
    out.push_str(rest);
    out
}

#[test]
fn plain_is_byte_identical_to_a_redirected_run() {
    // `--plain` exists for the case detection gets wrong -- a CI runner with a pty allocated, a
    // container with `TERM` set. It is only worth having if it lands in exactly the same place a
    // redirect does, rather than in a third place of its own.
    let redirected = extract(&["--report"]);
    let plain = extract(&["--report", "--plain"]);
    assert_eq!(redirected.stdout, plain.stdout);
    assert_eq!(without_durations(&redirected.stderr), without_durations(&plain.stderr));
}

#[test]
fn masking_durations_leaves_every_other_byte_of_a_report_alone() {
    // The mask is only safe if it is narrow. `0.0s` goes; `MiB`, the cue counts, the percentages
    // and the phase names all stay, because those are what the test above exists to compare.
    let line = "decode 0.0s; segment 12.5s; total 0.1s; resident 0.2 MiB images; cache 99%; cues 6";
    assert_eq!(
        without_durations(line.as_bytes()),
        "decode Ns; segment Ns; total Ns; resident 0.2 MiB images; cache 99%; cues 6"
    );
}

#[test]
fn plain_overrides_the_flags_it_contradicts() {
    let plain = extract(&["--plain", "--color", "always", "--progress", "always"]);
    assert!(
        !plain.stderr.contains(&ESC),
        "{:?}",
        String::from_utf8_lossy(&plain.stderr)
    );
    assert_eq!(plain.stderr, extract(&["--plain"]).stderr);
}

#[test]
fn no_color_is_honoured_and_color_always_overrides_it() {
    // The unit tests in `style` cover the decision; this covers the wiring from the environment
    // into it, which is the half a pure function cannot speak for.
    let path = fixture();
    let input = path.to_str().expect("the fixture path is UTF-8");
    let args = ["extract", input, "--on-unmatched", "placeholder"];

    let suppressed = run(&args, &[("NO_COLOR", "1")]);
    assert!(!suppressed.stderr.contains(&ESC));

    let mut forced = args.to_vec();
    forced.extend_from_slice(&["--color", "always"]);
    let forced = run(&forced, &[("NO_COLOR", "1")]);
    assert!(
        forced.stderr.contains(&ESC),
        "a flag typed on the command line is later and more specific than an exported variable"
    );
}

#[test]
fn a_failing_run_names_the_failure_on_stderr_and_leaves_stdout_empty() {
    // The error path is the one place a partial file on stdout would be worst: half an SRT that
    // parses is worse than no SRT at all.
    let output = run(&["extract", "no_such_file.sup", "--color", "always"], &[]);
    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "{:?}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no_such_file.sup"), "{stderr}");
    assert!(
        output.stderr.contains(&ESC),
        "and it is the one line drawn in red: {stderr}"
    );
}
