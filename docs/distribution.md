# Distribution

Answers [#16][issue-16]: what Sovereign actually consumes, how it is built, and what it costs to
call. §4 of [#1][issue-1] priced three options and did not pick one, because the numbers that decide
it did not exist yet. They do now, and they are lopsided enough that the decision is not close.

[issue-1]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1
[issue-16]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/16

## The measurements

| | |
| :--- | ---: |
| Release binary | **1.45 MB** |
| Cold start, process spawn to exit | **15.8 ms** |
| Reference set on disk | **6.3 KB** |
| Extraction, 5.5 GB film, 1,111 cues | **22 s** |

Cold start is the mean of three runs of fifty `subtrackt --version` invocations, 14.7–16.5 ms across
runs. That is a Windows figure and Windows has the slowest process creation of the three platforms
here, so it is an upper bound on what Linux will do rather than an estimate of it.

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

| Platform | Target |
| :--- | :--- |
| Linux x86-64 | `x86_64-unknown-linux-musl` |
| Linux ARM64 | `aarch64-unknown-linux-musl` |
| Windows x86-64 | `x86_64-pc-windows-msvc` |
| Windows ARM64 | `aarch64-pc-windows-msvc` |

Plus `SHA256SUMS` covering all four, so `sha256sum -c` verifies the set in one pass.

No macOS build, because nobody has asked for one. It is a row in the matrix if that changes.

Both ARM64 targets are cross-compiled from x86-64 runners rather than built natively: GitHub's
ARM64 runners are a paid add-on for private repositories, and cross-compiling a tree with no C in it
is the cheaper half of that trade. All four targets are type-checked on every pull request by the
`cross` and `windows` jobs in `ci.yml`, so a target cannot rot silently between tags.

## What a consumer still has to supply

The binary embeds no reference sets, on purpose — see [`reference-set.md`](reference-set.md). A
release artifact on its own extracts cues and names none of the glyphs in them. Whatever integrates
this needs a directory of `.subtref` files alongside the binary, and `subtrackt fit` picks between
them per title.

That is the real remaining integration cost, and it is worth being clear that it is larger than
anything on the table above. It is not a distribution problem, so it is not solved here.

## Caveats

- Every figure was taken on a Windows development machine. The deployment target is Linux
  containers, where process creation is cheaper and the binary is a different size. The direction of
  both differences favours the decision, so the conclusion does not depend on re-measuring — but the
  numbers in the table should be re-taken from a release artifact once one exists.
- All four targets compile. Only the two that a developer builds locally have been *linked* so far;
  the first tag is what proves the other two. `fail-fast: false` is set so that a link failure on
  one platform still yields the other three artifacts.
