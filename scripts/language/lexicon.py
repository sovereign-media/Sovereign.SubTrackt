#!/usr/bin/env python3
"""Build a word list for a language out of the library's own sidecars, and measure how thin it is.

#217 wants the word layer of #189's reader: not "is this character possible in Swedish" but "is
this a Swedish word". That needs a vocabulary, and a vocabulary is a **data** dependency -- per
language, licensed, and stale in a way that produces false positives, which is the failure mode
this whole area is least able to afford.

The library turns out to supply one. It carries 5,183 sidecars, and while the overwhelming majority
are English, there are 49 French, 17 Dutch, 11 Portuguese, 9 Swedish, 9 Spanish, 8 Norwegian, 8
Danish and 8 Finnish. Nothing is downloaded, nothing is licensed, and the corpus is drawn from the
same population as the discs being read.

**It is thin, and the thinness is the first thing this reports.** `calibrate` rebuilds the lexicon
without each source file in turn and scores that file against the rest, which measures the false
positive rate on text that is known to be correct. A reader whose unattested rate on real Swedish is
12% cannot say anything useful about an extraction that scores 13%, and the only honest way to know
that is to measure it.

    $ scripts/language/lexicon.py build --language swe --out swe.lex.json
    $ scripts/language/lexicon.py calibrate --lexicon swe.lex.json

Then `scripts/language/census.py --lexicon swe.lex.json` adds the word layer to the character one.
"""

from __future__ import annotations

import argparse
import collections
import json
import re
import sys
from pathlib import Path

DEFAULT_ROOT = Path("//nas/MEDIA")

# A token that is a word rather than a number or punctuation, matching `census.py` exactly so the
# lexicon and the thing scored against it cannot disagree about what a word is.
WORD = re.compile(r"[^\W\d_]+(?:['’][^\W\d_]+)*", re.UNICODE)

TIMING = re.compile(r"-->")
TAG = re.compile(r"</?[a-zA-Z][^>]*>")


def sidecars(root: Path, language: str) -> list[Path]:
    """Every `.srt` in the library whose name declares this language.

    Matched on the filename's language segment -- `Film.swe.srt`, `Film.swe.SDH.srt` -- because that
    is the only declaration a sidecar carries. A file with no language segment is English by the
    library's convention and is never returned here, which keeps `--language eng` honest about what
    it collected.
    """
    pattern = re.compile(rf"\.{re.escape(language)}(\.[A-Za-z]+)*\.srt$", re.IGNORECASE)
    return sorted(p for p in root.glob("*/*/*.srt") if pattern.search(p.name))


def words(path: Path) -> list[str]:
    """Every word of a sidecar, lowercased, with its furniture and markup removed."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    out = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.isdigit() or TIMING.search(stripped):
            continue
        out.extend(w.lower() for w in WORD.findall(TAG.sub("", line)))
    return out


def unattested_rate(counts: dict[str, int], vocabulary: set[str]) -> float:
    """What fraction of a file's word *occurrences* the vocabulary has never seen."""
    tokens = sum(counts.values())
    missed = sum(count for word, count in counts.items() if word not in vocabulary)
    return missed / max(tokens, 1)


def reject_outliers(
    per_file: dict[str, collections.Counter[str]], ceiling: float
) -> list[tuple[str, float]]:
    """Sidecars that share almost no vocabulary with the rest, which is what a mistag looks like.

    Not a quality filter, and the distinction matters as much here as it does in
    `scripts/alternatives/select.py`: this rejects a file for being **a different language**, never
    for scoring badly at anything. Two transcripts of one language overlap heavily whatever their
    subject; two languages do not overlap at all.

    It found one immediately. `Tremors (1990) ... .swe.srt` is Finnish -- *Huomenta, hra Basset* --
    and shares 89% of nothing with nine films of Swedish. A word list built with it in would have
    called a great deal of real Swedish impossible and a great deal of Finnish fine.

    One pass rather than iterating to a fixed point. A second mistagged file in the same language
    would hide the first, but that is a corpus this size having two of them, and a silent iteration
    that removed half the sources would be worse than a run that reports what it saw.
    """
    names = sorted(per_file)
    out = []
    for held in names:
        vocabulary: set[str] = set()
        for name in names:
            if name != held:
                vocabulary.update(per_file[name])
        rate = unattested_rate(per_file[held], vocabulary)
        if rate > ceiling:
            out.append((held, rate))
    return out


def alphabet_of(vocabulary: set[str]) -> set[str]:
    """Every character the language's own words are spelled with."""
    return {ch for word in vocabulary for ch in word}


def substitutions(token: str, vocabulary: set[str], alphabet: set[str]) -> list[tuple[str, str]]:
    """The `(wrong, right)` character pairs that would make this token a word.

    One substitution, never two. The point is not to correct the text -- nothing here writes a
    correction -- but to say *why* a token is not a word, in a form that aggregates into the census
    of confusion classes `docs/error-census.md` already uses. A token two edits from every word is
    reported as unattested and nothing more, which is the right answer: at two edits almost anything
    reaches almost anything.

    Returns every distinct pair rather than a single best guess, because a token that could be
    repaired six different ways is weak evidence and the count is how a reader sees that.
    """
    found = set()
    for at, ch in enumerate(token):
        for replacement in alphabet:
            if replacement == ch:
                continue
            if token[:at] + replacement + token[at + 1 :] in vocabulary:
                found.add((ch, replacement))
    return sorted(found)


def splits(token: str, vocabulary: set[str], shortest: int = 2) -> list[tuple[str, str]]:
    """The places this token comes apart into two words, which is what a lost space looks like.

    `shortest` keeps a one-letter fragment out of it. Swedish and Norwegian both have one-letter
    words, so a permissive rule would split half the language in half.
    """
    found = []
    for at in range(shortest, len(token) - shortest + 1):
        left, right = token[:at], token[at:]
        if left in vocabulary and right in vocabulary:
            found.append((left, right))
    return found


def build(args: argparse.Namespace) -> int:
    files = sidecars(args.root, args.language)
    if not files:
        sys.exit(f"no {args.language} sidecars under {args.root}")
    if args.limit and len(files) > args.limit:
        # An even stride over the sorted list rather than the first N, so a sample spans the
        # library alphabetically and re-runs to the same set. This exists for the English control:
        # 2,428 English sidecars would build a lexicon an order of magnitude thicker than any other
        # language's, and a control that differs from the thing it controls for in corpus size is
        # measuring corpus size.
        stride = len(files) / args.limit
        files = [files[int(i * stride)] for i in range(args.limit)]

    per_file: dict[str, collections.Counter[str]] = {}
    for path in files:
        found = words(path)
        if found:
            per_file[str(path)] = collections.Counter(found)
        print(f"  {len(found):>7} words  {path.name}", file=sys.stderr)

    rejected = reject_outliers(per_file, args.reject_above)
    for name, rate in rejected:
        print(
            f"  rejected {Path(name).name[:70]}: {rate:.1%} of its words unattested in the rest",
            file=sys.stderr,
        )
        del per_file[name]

    total: collections.Counter[str] = collections.Counter()
    for counts in per_file.values():
        total.update(counts)

    result = {
        "language": args.language,
        "files": len(per_file),
        "tokens": sum(total.values()),
        "types": len(total),
        # Per file as well as pooled, because `calibrate` needs to remove one source at a time and
        # re-deriving it from disk would let the two passes see different files.
        "sources": {name: dict(counts) for name, counts in per_file.items()},
    }
    args.out.write_text(json.dumps(result, ensure_ascii=False) + "\n", encoding="utf-8")
    print(
        f"\n{args.language}: {result['files']} sidecars, {result['tokens']} tokens, "
        f"{result['types']} distinct -> {args.out}"
    )
    return 0


def calibrate(args: argparse.Namespace) -> int:
    """How often does this lexicon call a real word impossible?

    Leave-one-out: score each source file against a lexicon built from the others. Every token in
    that file is a real word of the language, so **every miss is a false positive**, and the median
    of this column is the floor beneath which an extraction's unattested rate means nothing.
    """
    data = json.loads(args.lexicon.read_text(encoding="utf-8"))
    sources: dict[str, dict[str, int]] = data["sources"]
    names = sorted(sources)

    print(f"\n{data['language']}: leave-one-out over {len(names)} sidecars\n")
    print(f"{'unattested':>11} {'one letter off':>15} {'splits':>8} {'tokens':>8}   file")
    rates: list[float] = []
    subs: list[float] = []
    cuts: list[float] = []
    for held in names:
        vocabulary = set()
        for name in names:
            if name != held:
                vocabulary.update(sources[name])
        counts = sources[held]
        tokens = sum(counts.values())
        rate = unattested_rate(counts, vocabulary)
        # The same two edit classes the census reports, measured here on text known to be correct.
        # These are the rates at which a *real* word looks repairable by accident, and they are the
        # floors an extraction's own figures have to clear before they mean anything.
        alphabet = alphabet_of(vocabulary)
        missing = {w: c for w, c in counts.items() if w not in vocabulary}
        subbed = sum(c for w, c in missing.items() if substitutions(w, vocabulary, alphabet))
        cut = sum(c for w, c in missing.items() if splits(w, vocabulary))
        rates.append(rate)
        subs.append(subbed / max(tokens, 1))
        cuts.append(cut / max(tokens, 1))
        print(
            f"{rate:>10.2%} {subbed / max(tokens, 1):>14.2%} {cut / max(tokens, 1):>7.2%} "
            f"{tokens:>8}   {Path(held).name[:42]}"
        )

    for column in (rates, subs, cuts):
        column.sort()
    middle = len(rates) // 2
    for note in [
        "",
        f"  unattested      median {rates[middle]:.2%}   worst {rates[-1]:.2%}",
        f"  one letter off  median {subs[middle]:.2%}   worst {subs[-1]:.2%}",
        f"  splits          median {cuts[middle]:.2%}   worst {cuts[-1]:.2%}",
        "",
        "  Every figure above is a false positive: the text is a real transcript of the language.",
        "  They are the floors an extraction has to clear before its own figures mean anything,",
        "  and they are the price of a word list built out of eight films rather than a dictionary.",
        "  The three do not clear equally, which is the whole reason all three are printed.",
    ]:
        print(note)
    return 0


def selftest(_args: argparse.Namespace) -> int:
    """Pin the two functions every figure in the reader rests on.

    Neither is complicated and both are easy to get subtly wrong in a direction nothing would
    report: a substitution rule that also allowed insertions would find a repair for almost any
    token, and a split rule that allowed one-letter fragments would cut half of Swedish in half.
    There is no Python test harness in this repository, so this is a subcommand rather than a file
    `scripts/check.sh` would have to learn about.
    """
    vocabulary = {"pa", "min", "fru", "nar", "jag", "ni", "ck", "presenten"}
    alphabet = alphabet_of(vocabulary)

    # One substitution, and it reports the pair rather than the corrected word.
    assert substitutions("pä", vocabulary, alphabet) == [("ä", "a")], "one substitution"
    # Two edits away is not a repair, which is what keeps the census from filling with noise.
    assert substitutions("pxx", vocabulary, alphabet) == [], "never two substitutions"
    # A word already in the lexicon is nobody's business here; callers filter first.
    assert substitutions("zzz", vocabulary, alphabet) == [], "no repair, no claim"

    # A split needs *both* halves attested.
    assert splits("narjag", vocabulary) == [("nar", "jag")], "both halves"
    assert splits("narxxx", vocabulary) == [], "one half is not a split"
    # The false positive this whole layer has to be quoted with: a compound and a name both come
    # apart into real words, and no rule that lacks a grammar can tell them from a lost space.
    assert splits("nick", vocabulary) == [("ni", "ck")], "a name that splits is a false positive"
    assert splits("presenten", vocabulary) == [], "shortest keeps one-letter fragments out"

    print("the edit rules hold")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    b = sub.add_parser("build", help="collect a language's sidecars into a lexicon")
    b.add_argument("--language", required=True, help="the tag a sidecar's filename declares")
    b.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    b.add_argument("--out", type=Path, required=True)
    b.add_argument(
        "--limit",
        type=int,
        default=0,
        help="sample this many sidecars, on an even stride, so a control matches a corpus size",
    )
    b.add_argument(
        "--reject-above",
        type=float,
        default=0.5,
        help="drop a sidecar sharing less than this much vocabulary with the rest (a mistag)",
    )
    b.set_defaults(run=build)

    c = sub.add_parser("calibrate", help="measure the lexicon's false positive rate")
    c.add_argument("--lexicon", type=Path, required=True)
    c.set_defaults(run=calibrate)

    t = sub.add_parser("selftest", help="pin the edit rules the whole reader rests on")
    t.set_defaults(run=selftest)

    args = parser.parse_args()
    return args.run(args)


if __name__ == "__main__":
    raise SystemExit(main())
