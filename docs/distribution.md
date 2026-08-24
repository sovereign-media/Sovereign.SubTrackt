# Distribution

Answers [#16][issue-16]: what Sovereign actually consumes, how it is built, and what it costs to
call. §4 of [#1][issue-1] priced three options and did not pick one, because the numbers that decide
it did not exist yet. They do now, and they are lopsided enough that the decision is not close.

[issue-1]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1
[issue-16]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/16

## The measurements

| | |
| :--- | ---: |
| Release binary | **1.35–1.96 MB**, by platform |
| Cold start, process spawn to exit | **16.3 ms** |
| Reference set on disk | **6.3 KB** |
| Extraction, 5.5 GB film, 1,111 cues | **22 s** |

Taken from published artifacts rather than from a development build. The sizes are `v0.0.2-alpha`;
the other three rows are `v0.0.1-alpha` and have not been re-measured since, because nothing between
the two tags touched what they measure. Cold start is the mean of three runs of fifty
`subtrackt --version` invocations of the downloaded Windows binary, 16.1–16.4 ms across runs —
Windows has the slowest process creation of the platforms here, so it is an upper bound on what
Linux will do rather than an estimate of it.

The comparison that matters is against the 37-track file §4 names as the worst case in the queue.
Thirty-seven spawns cost **0.6 s**. The extraction those spawns exist to start costs on the order of
**13 minutes**. Process overhead is 0.07% of the work — three parts in four thousand.

## CLI, not `cdylib`

**The CLI ships.** It already exists, and the entire argument for the alternative was avoiding a
cost that turns out to be 0.07%.

| Option | Integration cost |
| :--- | :--- |
| **Standalone CLI** (chosen) | A build stage that downloads a release artifact; two arch artifacts per platform; process spawn per track, measured at 15.8 ms. No Rust in Sovereign's build. |
| `cdylib` + P/Invoke | Everything above except the spawn, **plus** a stable C ABI to design and never break, a header to keep in sync with it, marshalling for a partially-read track and its confidence tally, and a crash surface that takes the host worker down with it instead of returning an exit code. |
| Managed reimplementation | No Rust anywhere, and a rewrite of every measurement in `docs/` along with the code. Loses the reason the project exists. |

The steelman for `cdylib` is not really spawn cost — it is state that could be held across tracks
instead of rebuilt per track. There is one such piece of state, the reference set, and it is 6.3 KB.
Reading it is a single sequential pass over fixed-size records. There is nothing to amortise.

The asymmetry worth stating plainly: the CLI's costs are all *paid already*, and the `cdylib`'s are
all *ongoing*. An ABI is not written once; it is a promise not to change a struct layout, enforced
by nothing, for as long as both sides ship separately. Trading that for 0.6 s per worst-case file
would be a bad deal even if the ABI were free to write.

This also keeps [#1][issue-1]'s failure story intact. An unmatched glyph is a fact rather than a
confidence score, and a process that exits non-zero is a fallback to burn-in that the queue already
knows how to handle. In-process, the same condition is an exception crossing a P/Invoke boundary.

## Static musl, not glibc

Linux artifacts are `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`, statically linked.

A dynamically linked glibc binary carries a version floor set by whichever machine built it. Built
on `ubuntu-latest` and dropped into a Debian container, that floor is invisible until the base image
moves and the binary stops loading — a failure that appears at deploy time and points at nothing.
A static binary has no floor and runs on any distribution, which is a better answer to "the common
Linux variants" than choosing two of them and hoping.

Static linking is normally a trade. [#16][issue-16] names the tension exactly: a demuxer pulling in
system libraries and a static musl binary pull in opposite directions. That tension does not apply
here, because there is no such demuxer — Matroska and PGS are parsed in-tree, and the library crates
take one pure-Rust dependency between them (`miniz_oxide`, for zlib). Nothing is being given up.

The same property is why `release.yml` installs no C toolchain for the x86-64 build: there is no C
to compile. If that step ever needs `musl-tools`, a C dependency has crept in somewhere and the
build is telling you so.

## What ships

Four artifacts per tag, from `.github/workflows/release.yml`:

| Platform | Target | `v0.0.1-alpha` | `v0.0.2-alpha` |
| :--- | :--- | ---: | ---: |
| Linux x86-64 | `x86_64-unknown-linux-musl` | 1.94 MB | 1.96 MB |
| Linux ARM64 | `aarch64-unknown-linux-musl` | 1.63 MB | 1.63 MB |
| Windows x86-64 | `x86_64-pc-windows-msvc` | 1.56 MB | 1.59 MB |
| Windows ARM64 | `aarch64-pc-windows-msvc` | 1.32 MB | 1.35 MB |

Both columns are kept rather than the newer one replacing the older. A single column says how big
the binary is; two say which way it is moving, which is the question this section exists to answer.

Plus `SHA256SUMS` covering all four, so `sha256sum -c` verifies the set in one pass.

The two Linux rows are the larger ones, and that difference is the measured price of linking
statically: musl and the startup files it brings cost roughly 380 KB over the dynamically linked
Windows build of the same architecture. Against not having a glibc version to match, that is cheap.

What the binary has *not* grown on is decoration. #83 added colour by severity, a spinner and a
progress bar for **30.5 KB** on `x86_64-pc-windows-msvc` — the same figure measured locally before
and after, and then between the two published artifacts above. The other three targets moved by
less, down to 168 bytes on `aarch64-unknown-linux-musl`. Colour is free because `clap` already
compiles `anstyle`, `anstream` and `anstyle-query` in; the bar is forty lines of arithmetic rather
than the five extra crates `indicatif` would have brought. The rule that produced that number is in
`CLAUDE.md`: reach for a crate when the work is someone else's problem domain, and drawing
`[####----] 43%` is not.

Sizes throughout this document are binary megabytes, which is what the numbers in the table divide
out to and what a file listing reports.

No macOS build, because nobody has asked for one. It is a row in the matrix if that changes.

Both ARM64 targets are cross-compiled from x86-64 runners rather than built natively: GitHub's
ARM64 runners are a paid add-on for private repositories, and cross-compiling a tree with no C in it
is the cheaper half of that trade. All four targets are type-checked on every pull request by the
`cross` and `windows` jobs in `ci.yml`, so a target cannot rot silently between tags.

## Cutting a release

```console
$ scripts/release.sh 0.2.0     # bump, prove, open the pull request
$ # ...merge it...
$ git tag v0.2.0 && git push origin v0.2.0
```

The tag is the trigger; everything after it is automatic. Nothing else creates one, on purpose — a
tag names a commit, so it has to come after the version bump has landed on `main` rather than
alongside it.

The version lives in **seven** places: `[workspace.package]` sets it, and each of the six path
dependencies in `[workspace.dependencies]` repeats it as a constraint. Bumping only the first leaves
Cargo unable to resolve the workspace; bumping all seven by hand is a transcription exercise with
six chances to get it wrong. That is what `scripts/release.sh` is for, and it verifies it changed
exactly seven before going on.

A tag that disagrees with `Cargo.toml` fails the release in its first job, before anything is built.
That check is not bureaucracy: the artifacts are named for the *tag* while `--version` reports what
the *manifest* said, nothing downstream compares the two, and once published the mismatch is
permanent.

### The changelog

There is no `CHANGELOG.md`, and that is a decision rather than an omission. GitHub generates release
notes from the pull requests merged since the previous tag, and this project writes pull request
titles as sentences about *why* — so that list already reads as release notes. A hand-maintained
file would duplicate commit messages that carry more reasoning than it would, and then drift from
them.

A tag carrying a pre-release suffix — `v0.2.0-rc.1` — is published as a pre-release, so it does not
become the "latest" that a download link resolves to.

## What a consumer still has to supply

The binary embeds no reference sets, on purpose — see [`reference-set.md`](reference-set.md). A
release artifact on its own extracts cues and names none of the glyphs in them.

When this document was first written that was the largest remaining integration cost, because making
a `.subtref` needed the repository and a Rust toolchain — precisely what §4 establishes Sovereign
does not have. #80 closed it: `subtrackt gen-reference` renders a font, or a directory of fonts,
using the same normalisation `extract` applies to a decoded bitmap. Three commands with nothing
installed but the artifact:

```console
$ subtrackt gen-reference /usr/share/fonts ./sets
$ subtrackt fit movie.mkv --references ./sets -o movie.subtref
$ subtrackt extract movie.mkv --reference movie.subtref --format srt -o movie.srt
```

What is still the consumer's to supply is the *fonts* — which is a licensing question rather than a
distribution one, and the reason it stays theirs is in `reference-set.md`.

## What the first tag proved

`v0.0.1-alpha` was cut on 2026-08-22 and settled the two things this document could previously only
assert.

**All four targets link, not just compile.** The two ARM64 rows had never been through a linker
before that tag — they are cross-compiled and CI only type-checks them — so this was the open
question. `fail-fast: false` remains set, so a future link failure on one platform still yields the
other three.

**A published artifact does the whole job.** The downloaded Windows binary reports
`subtrackt 0.0.1-alpha`, generates reference sets from a font directory, and fits them against a
Blu-ray to the same numbers a development build gives. All four checksums verify against
`SHA256SUMS`.

The remaining caveat is now closed. #131 built the `x86_64-unknown-linux-musl` binary into an
`ubuntu:24.04` container to benchmark it against five other tools, which put a Linux process on a
stopwatch for the first time:

| | Windows | Linux, musl, in a container |
| :--- | ---: | ---: |
| `subtrackt --version`, cold | 16.1–16.4 ms | **below 10 ms**, `time`'s resolution |
| peak RSS for that invocation | — | **1,204 kB** |
| binary | 1.56 MB | 2.07 MB, statically linked |

Linux process creation is cheaper, as predicted, and by enough that the instrument cannot see it:
twenty invocations all reported `0.00` against `/usr/bin/time`'s centisecond resolution, so the
figure is a ceiling rather than a measurement. The Windows number stays as the quotable one, because
it is the only one large enough to quote. The argument in *CLI, not `cdylib`* rests on process
creation being cheap relative to the work, and on the platform where that argument was weakest it is
weaker still than assumed.

The extraction figure above is a separate matter and is **not** comparable to the per-track seconds
in [`alternatives.md`](alternatives.md): 22 s was measured natively on Windows reading a 5.5 GB rip
over SMB, and those were measured in a container reading a flat `.sup` off a local volume.
