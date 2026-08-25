# Working rules

Conventions this project already runs on. They are written down because each one has already earned
its place — most of them by something breaking when they were not followed.

A fuller set, derived from a scan of the codebase, belongs here once the pipeline is complete. This
is the interim.

## Before pushing

Run `scripts/check.sh`. Every time.

It runs what CI runs, in CI order: fmt, clippy, tests, dependency discipline, docs. The gate easiest
to skip is the one
that catches the most: **clippy** runs at pedantic with `-D warnings`, and a `cargo test` that
passes says nothing about it. It has already broken `main`.

There is no MSRV job. `rust-version` is declared so Cargo gives a clean error, but nothing enforces
it: releases are built here on stable and dropped into a container with no Rust toolchain, so the
floor had no consumer to protect and the job only ever tested a constraint it was itself the sole
source of. See #71.

## Landing work

One issue, one pull request. Open it, wait for checks, merge when green. Do not push to `main`
directly.

Commit messages explain *why*, not what — the diff already says what. Record decisions that a
reader would otherwise have to re-derive, and non-obvious behaviour the format or the domain forced.

That style is load-bearing beyond review: release notes are generated from pull request titles, so
there is no `CHANGELOG.md` to maintain and a vague title is the only thing that would need one.
Releases are cut with `scripts/release.sh <version>`, then a tag once it lands — the version appears
in seven places and the script is what keeps them in step. `docs/distribution.md` has the rest.

## Dependencies

**Library crates take no dependencies.** `subtrackt-core` through `subtrackt` use only the standard
library. `clap`, `anyhow`, `tracing` and the `anstyle`/`anstream`/`anstyle-query` colour stack live
in `subtrackt-cli` and nowhere else. The last three cost nothing to name: `clap`'s default features
already compile all of them into the binary, so #83 declared what was in the tree rather than
adding to it.

This is not asceticism: #1 asks for a single static binary, and #16 had to decide between shipping a
CLI and a `cdylib` behind P/Invoke. A dependency-free core made that a question about one new crate
rather than an audit of a transitive tree. The cost is real and accepted — `Error` implements
`Display` by hand rather than deriving with `thiserror`.

#16 landed on the CLI, and the rule outlived the question that motivated it: static musl linking is
what makes "drop one file in the container" work, and it stays free only while nothing in the tree
wants a system library. `docs/distribution.md` has the numbers.

Adding one to a library crate needs a reason that outweighs the above.

**One has.** `subtrackt-demux` takes `miniz_oxide` for zlib. A scan of the library found 83% of PGS
tracks stored zlib-compressed inside Matroska, so refusing the dependency meant failing on most of
the library. The alternative considered was hand-rolling inflate; that was the wrong call. DEFLATE
is not this project's problem domain, a subtle Huffman bug produces garbage bitmaps, and
`miniz_oxide` is pure Rust with no build script and one tiny dependency — so the single-binary and
cross-compilation goals the rule exists to protect are all intact.

**A second has, optionally.** `subtrackt-glyph` takes `fontdue` behind an off-by-default `font`
feature, so a downloaded binary can render its own reference sets instead of needing this repository
and a toolchain to make one (#80). Same test as above: rasterising an outline is not this project's
problem domain, and the crate is pure Rust with no build script.

The word doing the work is *optional*. A consumer who does not opt in still gets the tree the rule
describes, and both `scripts/check.sh` and CI have a **dependency discipline** step that asserts
exactly that. It exists because Cargo unifies features across a workspace, so the property is
breakable from a manifest that never mentions `subtrackt-glyph` — which is not a change anyone would
think to look for. If it fails, the fix is to justify the new dependency here and update the list,
not to widen the list quietly.

The lesson worth keeping: the rule is "justify it", not "never". Reach for a crate when the work is
someone else's problem domain and the crate is pure Rust with a shallow tree.

## Failing

**Never invent data to avoid an error.** This is the project's whole thesis: a glyph matcher is
worth building over general OCR because an unmatched glyph is a *fact* rather than a confidence
score, and a fact is something a caller can act on.

Concretely:

- A truncated object is rejected, never padded with blank lines. A partially decoded subtitle reads
  as legitimately empty, and an empty subtitle is indistinguishable from one that had no text.
- Unimplemented stages return `Error::Unsupported { issue }` naming the tracking issue, never
  `todo!()`. A panic in a media pipeline is a crashed worker; an error is a fallback to burn-in.
- Malformed input names the presentation timestamp, so a failure can be located in the stream.
- Where an approximation is genuinely the lesser evil — the trailing cue of a stream that never
  clears — it is documented, uses a named constant, and is counted so it can be audited.

## Numbers

Thresholds are fractions of something measured, never absolute pixels or raw cell counts. The same
title ships at several resolutions, and `FEATURE_GRID` may yet change. A constant that works at
1080p and merges half a line at 480p is a bug waiting for a different disc.

## Tests

Test names state the property under test, not the function under test. `a_run_past_the_right_edge_is_rejected_rather_than_clipped`
tells a future reader what the code guarantees; `test_decode_3` does not.

Cover the failure modes, not just the happy path. Every codec parser needs truncated input,
over-long input and wrong-length input. Prefer a round trip over a hand-written expectation where an
encoder exists.

Pin surprising behaviour with a named test so the decision is visible rather than accidental —
8-connectivity fusing characters that touch at a corner, one display set becoming one cue. The
first of those is now *recovered* rather than merely pinned (#106), and the pin is what made the
recovery a bounded change rather than a rewrite: the test said what the behaviour was, so the fix
had something to be measured against.

## Accuracy

`cargo run -p xtask -- accuracy` generates a fixture and a reference set from the same font, runs
the pipeline, and scores the text against known ground truth. It is the only measurement in the
project that answers whether the *right characters* come out; everything else answers whether shapes
look alike, and those two diverge.

Treat its number as a ceiling. Fixture and reference share a font, so typeface mismatch is excluded
by construction and real material can only do worse. A change that improves coverage but worsens CER
has made things worse.

### The bench

The fixture is the ceiling. **`scripts/bench/` is the floor**, and it is what prices a change on real
material. `roster.json` names nine tracks and says what each one covers; `run.py` extracts and
scores all of them and diffs two runs.

```console
$ scripts/bench/run.py dump  --cache bench-cache            # once, ~30 min and 184 MB
$ scripts/bench/run.py score --cache bench-cache --reference arial-ri.subtref --out before.json
$ scripts/bench/run.py score --cache bench-cache --reference arial-ri.subtref --out after.json
$ scripts/bench/run.py compare before.json after.json
```

Dump first. A pass then costs about twenty seconds against hours of network reads, which is what
makes running it before *and* after the cheap option rather than the diligent one.

**Read the `worse` column, not the CER.** #110 gained character error on one disc while making 232
cues worse on another, and #113 found it only because two more discs were scored.

Two rules the roster exists to hold, both learned by breaking them:

- **A scored track needs a sidecar of matching convention**, not just any sidecar. The Prestige's
  track is SDH and its only sidecar is not, so correcting `[CROWD LAUGHlNG]` to `[CROWD LAUGHING]`
  scored as a *regression* against unrelated dialogue. It is a `smoke` entry for that reason: a
  track that puts false entries in the `worse` column poisons the one number you are told to read.
  #140 put a size on this: scoring one extraction against every English sidecar in its own folder
  moves CER by up to **77 points**, and the widest case is not SDH-versus-plain — Training Day's
  four sidecars are all from one disc, and the two cut from the Blu-ray read 3.1% and 5.0% where the
  two cut from the WEB release read 79.7% and 80.7%. A CER quoted without naming its sidecar means
  nothing. `docs/vobsub.md` has the table.

  **The rule was then applied to the bench itself, and two of nine entries were wrong.** #175 scored
  every track against every English sidecar in its own folder — the first time anything had checked
  the roster's own choices. A Fish Called Wanda is an SDH track carrying 85 bracketed sound cues,
  `(Door opens)`, `(Ken gasps)`, and was scored against a sidecar carrying none: **more than half of
  its measured error was the missing cues**, and it reads 1.7% rather than 4.2%. Airplane! is worse
  than a mismatch — the disc renders its sound cues as 72 bracketed lines and the disc's own SDH
  sidecar renders them as 197 musical notes and one bracket, two SDH conventions sharing almost no
  characters, so neither of its sidecars matches and it is a `smoke` entry now. **Check a new
  entry's sidecar against its extraction's shape before trusting the number**, and the check is one
  command: score against every candidate and read the spread.

  **The rule has a second instrument now, and there it is code.** #209 widened the alternatives
  corpus to twenty titles, which is too many to check by eye, so `scripts/alternatives/select.py`
  runs the check: cues paired, and sound-cue counts on both sides, both as fractions rather than
  counts. It reads structure and never text — brackets are structural and timings align even when
  the typeface does not fit — so a title an engine reads badly is *kept* and only a sidecar
  transcribing a different thing is dropped. That is the line worth holding: filter on the sidecar's
  convention, never on the score, or the corpus selects titles by the outcome being measured.

  It rejects about a third of what it walks past, and prints every rejection with its reason. A draw
  that quietly skipped them would read as a sample of the library when it is not.
- **A track with no scoreable sidecar still earns a place.** `smoke` entries claim no accuracy and
  assert only that the track survives its own shape — which is the whole point of the 106-cue forced
  track, where every per-line median has no population to work from.

Adding a track is cheap; adding one that duplicates another's coverage costs attention, which is the
expensive half. #133 has the survey the seven were chosen from: SDH is ~75% of the library and the
bench had none of it until then. #140 added the eighth and ninth for the same reason — every figure
this project had published was PGS, and the other codec had shipped unmeasured. Those two are read
from their containers rather than from the dump cache, because a `.sup` holds PGS and nothing else,
which takes a pass from about five seconds to fifty-four.

### The reader

The bench and the fixture are both English, and #189 measured what that hid: the library is **50
language tags over 1,316 files**, and two thirds of its tagged tracks are in a language
`charset()` cannot spell. `docs/language-coverage.md` has it.

Three instruments, and the reason there are three is that a sidecar is unavailable for almost every
non-English track in the library, so a CER is not on offer:

- `scripts/language/survey.py` — which languages the library carries.
- `xtask language-coverage` — what each orthography requires, whether the set has it, and, for the
  ones it lacks, whether the matcher **rejects** the character or finds a **confident wrong home**.
  150 of 213 absent characters rehome silently, and a rehoming is the one error class no coverage
  figure, gate or census downstream can see.
- `scripts/language/census.py` — reads an extraction as its declared language and counts what that
  orthography cannot spell. No sidecar, no alignment, no dictionary.
- `scripts/language/lexicon.py` — builds a word list for a language out of the library's own
  sidecars, which `census.py --lexicon` uses for the word layer. `docs/word-reader.md`.

Three rules came out of it, all the hard way:

- **A census figure is a floor, never a rate.** It cannot see a real word read as a different real
  word. Quoting one as an error rate is the invented-data failure this document opens with.
- **The orthography table errs towards including a character**, because the two mistakes cost
  different things: one character too many understates a gap by one, one too few makes the census
  call a real word impossible — a false entry in the only column it prints, which is the sidecar
  lesson `scripts/bench/roster.json` records at length. Swedish `é` was left out on the first pass
  and the Norwegian census flagged `én` nine times.
- **A reader's own false-positive rate is measured, never assumed.** `lexicon.py calibrate` scores
  each source sidecar against the others, so every miss is definitionally a false positive. Built
  out of eight films, a word list calls **one word in six** of real Swedish impossible — which makes
  an unattested rate worthless and a one-edit repair rate worth two to three times its floor. The
  English track is the control that proves it: it clears its own three floors by 0.14, 0.85 and 0.02
  points, on a track the pipeline reads at 1.4% character error.

And one finding worth carrying into unrelated work: **a non-English track is a better test of an
English defect.** The lost word gap before a tall narrow letter is 6 instances in Gone Girl's English
and 142 across its Swedish and Norwegian — same disc, same bug, and only one of the three can rank a
change.

## Gates

There are two, and they are two because #218 found that one of them could not be built out of the
other. `docs/script-guard.md` has it.

The **threshold gate** counts: a track whose matched fraction falls below `--min-matched` is refused
after the read. The **script guard** compares two declarations before the read: the container named a
language, the language has a known script, and the reference set holds not one character of it.

The rule they came from is the one to remember when the next gate is proposed here:

> **A wrong script and a wrong typeface are the same event to everything downstream — the set cannot
> spell this track — and nothing computed from the read can name which.**

Seven statistics have now been measured against a question of this shape and none separates; six are
in `docs/fit-confidence.md`. The seventh was `fit`, and it looked convincing — 26.5 to 37.3 on five
non-Latin tracks against 11.8 to 14.3 on eleven Latin ones, same disc. **The control killed it**:
the same English track read with six wrong typefaces scores 23.1 to 34.9, a band that *contains* the
non-Latin one. A statistic that separates on the material you sampled has not separated.

So a new gate here has to bring evidence from outside the read, and it has to refuse on a fact.
Every uncertainty resolves to a pass — untagged streams, unknown tags, languages written in two
scripts — because a wrong refusal costs a caller an expensive fallback on a track that would have
read, and a missed one costs only what the pipeline already did.

## Scope

Stages are traits in `subtrackt-core::stage`. No stage crate depends on another stage crate; the
graph is a fan with `subtrackt-core` at the hub. That is what lets an unimplemented stage be a stub
without its neighbours knowing, and it is worth preserving.

When a stage lands, the pipeline's boundary test moves to the next issue number. That test is the
honest measure of how far the pipeline actually reaches — keep it accurate.

## What needs real media

Almost nothing. Fixtures are generated in code, and `rle::encode` is public so decoders round-trip
against known bitmaps.

The exception is **#8**, the typeface survey, which asks what studios actually author against. No
synthetic fixture can answer that. It needs breadth rather than length: a few cues each from many
titles beats one full film. #14 needs font files, not media.
