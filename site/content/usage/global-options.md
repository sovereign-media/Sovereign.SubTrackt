---
title: Global options
label: Global options
description: The flags every command takes, which stream everything goes to, exit status, and what to set when you are running this from a script.
---

# Global options

Four flags work on every command, and they work **after** the subcommand as well as before it —
which is where a hand reaches for them when a run turns out noisier than expected.

```console
$ subtrackt extract movie.mkv --reference movie.subtref -vv --plain
```

| Flag | Default | Meaning |
| :--- | :--- | :--- |
| `-v`, `--verbose` | warnings only | Increase log verbosity. Repeatable: `-v` info, `-vv` debug, `-vvv` trace. |
| `--color <WHEN>` | `auto` | Colour stderr by severity. `auto`, `always`, `never`. |
| `--progress <WHEN>` | `auto` | Draw a spinner and a progress bar on stderr. `auto`, `always`, `never`. |
| `--plain` | off | Neither colour nor progress. |

And two that stand alone:

| Flag | Meaning |
| :--- | :--- |
| `-h`, `--help` | Short help. `--help` on a subcommand gives the long form with the reasoning. |
| `-V`, `--version` | The binary's version **and the reference data compiled into it**. |

## Why `--version` names two things

```console
$ subtrackt --version
subtrackt 1.0.0 (reference set: empty, 0 glyphs)
```

Two different things decide what an extraction says, and they are fixed in different places: the
code, and the data it matched against. A bad read is one or the other, and a version string naming
only the first leaves the second untraceable.

That matters more here than it usually would, because the embedded set is empty — so somebody
watching every glyph come back unmatched should be able to find out why from the tool rather than
from the source.

## Output discipline

**Data goes to stdout. Everything the tool says about itself goes to stderr.**

That means the subtitle, or the glyph rows, or the stream list — and nothing else — is on stdout,
and progress, warnings, the report and errors are on stderr. A redirect is clean without any flag:

```console
$ subtrackt extract movie.mkv --reference movie.subtref > movie.en.srt
```

**Not one escape byte reaches stdout, ever.** A coloured `.srt` is a corrupt `.srt`, and no
combination of flags can produce one.

## Colour and progress detect their own environment

Both switch themselves off when stderr is not a terminal, so a pipe, a redirect, a cron job and CI
are already clean and need no flag at all. The flags exist for the case detection gets it wrong.

- `--color always` forces colour on into a pipe, which is what you want feeding a pager that
  understands it.
- `--color never` forces it off. So does the `NO_COLOR` environment variable, which is honoured
  without any flag.
- `--plain` is one flag for when detection gets *both* wrong at once, and it beats `--color always`.

Both decisions are made from one snapshot of the environment before anything is written, so colour
and progress cannot disagree about what kind of stream they are writing to.

## Verbosity, and `RUST_LOG`

`-v` raises the log level: warnings by default, then info, debug, trace. Log lines go through the
progress renderer rather than straight at the handle, so a line and a spinner sharing stderr do not
land on top of each other.

`RUST_LOG` overrides the level `-v` asks for, using the standard filter syntax, which is what you
want when you need one module loud and the rest quiet:

```console
$ RUST_LOG=subtrackt_demux=trace subtrackt list movie.mkv
```

## Exit status

| Status | Meaning |
| :--- | :--- |
| `0` | Success. |
| `1` | Failure, with the reason on stderr as `error: …`. |

The error line carries the **full context chain**, so a failing run says *where* it failed rather
than only that it did — `error: parsing reference set ./sets/arial.subtref: unexpected end of
input` rather than `error: unexpected end of input`.

Two things worth knowing for a script:

- A run that produced **no output because there was nothing to read** is not the same as a run that
  broke. [`list`](/usage/list) exits `0` and says so on stdout; `extract` on the same file exits
  `1`, because an empty subtitle file is indistinguishable from a track that was never found.
- A run that **read the track too poorly to trust** exits `1` by default and writes no file. That
  is [`--on-unmatched threshold`](/usage/extract#the-accuracy-gate) doing its job, and it is what
  makes this safe to run unattended: the caller gets a failure it can fall back from, not a
  plausible subtitle nobody checks.

## Running this from a script

The shape that behaves well unattended:

```console
$ subtrackt extract "$input" \
      --reference "$set" \
      --format srt \
      --output "$out" \
      --report \
      --plain \
  || fall_back_to_burn_in "$input"
```

`--report` on stderr gives you something to log; `--plain` removes anything a log file should not
carry; the default gate turns a bad read into a non-zero exit; and `||` is where the whole design
pays off, because a failure here is a *fact* about the material rather than a crash.

Pin a version if you are diffing extractions between runs. The command-line surface is frozen from
`v1.0.0`, but accuracy work can change what a disc reads as without changing a flag.
