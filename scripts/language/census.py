#!/usr/bin/env python3
"""Read an extraction as the language its container declared, and count what is impossible in it.

#189 proposes a **reader**: judge an extraction as language rather than against a sidecar, because
a sidecar is unavailable for almost every non-English track in the library and `pa` is simply not a
Swedish word whether or not anyone transcribed the film. This is that instrument's cheapest layer,
and the only one that needs no dictionary.

It asks three questions of every character, and each answer is a **fact about an orthography**
rather than a score:

    foreign         a letter this orthography does not contain at all
    inner capital   an accented capital inside an otherwise-lowercase word
    placeholder     what the pipeline already told us it could not read

The first catches the defect #189 §1 found -- `a` with a grave in a Swedish track, where the
language has no grave accent anywhere -- and, wholesale, the defect §4 found, where a Cyrillic
track comes back as confident Latin. The second catches §3, `presencIa` and `SIempre`, which is a
confusion between two characters the reference set *has* and which no missing-character analysis
can see. Both are unambiguous: neither needs to know which word was on the screen.

**What it cannot say, and the reason it is a floor rather than a measurement.** A real word read as
a different real word is invisible to it -- `den` for `det`, `sin` for `din`. So is a wrong letter
that happens to be in the alphabet: a Swedish `a` read as an `o` is a perfectly ordinary Swedish
character in a perfectly ordinary Swedish position. Every number this prints is therefore a
**lower bound on the error**, and quoting one as an error rate would be the invented-data failure
`CLAUDE.md` forbids. It bounds; it does not measure.

The orthographies come from `xtask language-coverage --emit-alphabets`, which is the single copy.

    $ scripts/language/census.py extracted.srt --language swe
    $ scripts/language/census.py extracted.srt --language rus --examples 20
"""

from __future__ import annotations

import argparse
import collections
import os
import re
import string
import subprocess
import sys
import unicodedata
from pathlib import Path

# The digits, spaces and punctuation every one of these orthographies shares. A subtitle carries
# numerals and dashes in every language, and charging those to the language would bury the letters.
SHARED = set(string.digits + string.punctuation + string.whitespace)

# What the pipeline emits for a glyph it could not read. Counted separately from everything else,
# because it is the one error class that is already honest -- see `--on-unmatched placeholder`.
PLACEHOLDER = "�"

# SubRip's furniture: the index line, the timing line, and the inline tags. None of it came off a
# bitmap, so none of it is evidence about the matcher.
TIMING = re.compile(r"-->")
TAG = re.compile(r"</?[a-zA-Z][^>]*>")

# A token that is a word rather than a number or a piece of punctuation. Split on anything that is
# not a letter or an apostrophe, so `d'accord` and `it's` stay whole.
WORD = re.compile(r"[^\W\d_]+(?:['’][^\W\d_]+)*", re.UNICODE)


def alphabets(xtask: str) -> dict[str, tuple[str, str, str, str]]:
    """tag -> (name, script, letters, punctuation), from the table xtask holds."""
    done = subprocess.run(
        [xtask, "language-coverage", "--emit-alphabets"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )
    if done.returncode != 0:
        sys.exit(f"{xtask} language-coverage --emit-alphabets failed:\n{done.stderr}")
    out: dict[str, tuple[str, str, str, str]] = {}
    for line in done.stdout.splitlines():
        parts = line.split("\t")
        if len(parts) == 5:
            out[parts[0]] = (parts[1], parts[2], parts[3], parts[4])
    return out


def cues(path: Path) -> list[str]:
    """The text lines of a SubRip file, with its furniture and its tags removed."""
    lines = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        stripped = line.strip()
        if not stripped or stripped.isdigit() or TIMING.search(stripped):
            continue
        lines.append(TAG.sub("", line))
    return lines


def allowed(script: str, letters: str) -> set[str]:
    """Every character the orthography can spell a word with.

    ASCII letters are in it for a Latin-script language and out of it for every other, which is the
    whole of the wrong-script test: a Cyrillic track has no business emitting `c` any more than a
    Swedish one has emitting a grave accent.
    """
    base = set(letters)
    if script == "Latin":
        base |= set(string.ascii_letters)
    return base


def census(lines: list[str], script: str, letters: str) -> dict[str, object]:
    """Count what this orthography cannot spell."""
    spellable = allowed(script, letters)
    accented_capitals = {c for c in letters if c.isupper() and len(unicodedata.normalize("NFD", c)) > 1}

    foreign: collections.Counter[str] = collections.Counter()
    inner: collections.Counter[str] = collections.Counter()
    examples: dict[str, list[str]] = collections.defaultdict(list)
    placeholders = 0
    characters = 0

    for line in lines:
        for ch in line:
            characters += 1
            if ch == PLACEHOLDER:
                placeholders += 1
            elif ch not in spellable and ch not in SHARED:
                foreign[ch] += 1
                if len(examples[ch]) < 8:
                    examples[ch].append(line.strip())
        for word in WORD.findall(line):
            # An accented capital mid-word. Restricted to *accented* capitals on purpose: a bare
            # capital inside a word is ordinary in a name -- McDonald, LaFarge, DVDs -- and only the
            # accented ones are the confusion #189 §3 measured, where `i` and `I` differ by height
            # alone once the tittle is replaced by a mark.
            body = word[1:]
            if not body:
                continue
            for ch in body:
                if ch in accented_capitals and body.replace(ch, "").islower():
                    inner[ch] += 1
                    if len(examples[f"inner:{ch}"]) < 8:
                        examples[f"inner:{ch}"].append(word)
                    break

    return {
        "characters": characters,
        "placeholders": placeholders,
        "foreign": foreign,
        "inner": inner,
        "examples": examples,
    }


def report(result: dict[str, object], name: str, script: str, show: int) -> None:
    characters = result["characters"]
    foreign: collections.Counter[str] = result["foreign"]  # type: ignore[assignment]
    inner: collections.Counter[str] = result["inner"]  # type: ignore[assignment]
    examples: dict[str, list[str]] = result["examples"]  # type: ignore[assignment]
    placeholders: int = result["placeholders"]  # type: ignore[assignment]

    impossible = sum(foreign.values())
    print(f"\n{name} ({script}): {characters} characters read\n")
    print(f"  {'placeholder':<22} {placeholders:>7}   {placeholders / max(characters, 1):>7.2%}")
    print(f"  {'foreign letter':<22} {impossible:>7}   {impossible / max(characters, 1):>7.2%}")
    print(
        f"  {'inner capital':<22} {sum(inner.values()):>7}   "
        f"{sum(inner.values()) / max(characters, 1):>7.2%}"
    )

    if foreign:
        print("\n  Letters this orthography does not contain\n")
        for ch, count in foreign.most_common(show):
            sample = examples.get(ch, [""])[0][:56]
            print(f"    {ch}  U+{ord(ch):04X}  {count:>6}   {sample}")
        if len(foreign) > show:
            rest = sum(count for _, count in foreign.most_common()[show:])
            print(f"    ... and {len(foreign) - show} more letters, at {rest} instances")

    if inner:
        print("\n  Accented capitals inside a lowercase word\n")
        for ch, count in inner.most_common(show):
            sample = " ".join(examples.get(f"inner:{ch}", [])[:4])
            print(f"    {ch}  U+{ord(ch):04X}  {count:>6}   {sample}")

    print(
        "\n  Every count here is a lower bound. A real word read as a different real word, and a\n"
        "  wrong letter that is still in the alphabet, are both invisible to this."
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("extraction", type=Path, help="a .srt written by subtrackt extract")
    parser.add_argument("--language", required=True, help="the ISO 639-2 tag the container declared")
    parser.add_argument(
        "--xtask", default="target/release/xtask.exe", type=os.path.normpath,
        help="where the alphabet table comes from",
    )
    parser.add_argument("--examples", type=int, default=12, help="how many rows each table shows")
    args = parser.parse_args()

    table = alphabets(args.xtask)
    if args.language not in table:
        sys.exit(
            f"no orthography for {args.language!r}. "
            f"Add it to LANGUAGES in xtask/src/language.rs; known: {' '.join(sorted(table))}"
        )
    name, script, letters, _punctuation = table[args.language]
    report(census(cues(args.extraction), script, letters), name, script, args.examples)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
