#!/usr/bin/env python3
"""Turn the scans into the tables #133 needs to decide a bench on.

Reads `scan.py`'s track records and, optionally, `sample.py`'s size or content measurements, and
prints what the issue asks for: how much of the library is SDH, how far a label undercounts it, and
where the size extremes are that the standing three discs cannot reach.

Usage:
    scripts/bench/analyse.py --tracks tracks.jsonl [--sizes sizes.jsonl] [--content content.jsonl]
"""

import argparse
import collections
import json
import re
import statistics

SDH_LABEL = re.compile(r"\bS\s*[DH]\s*[DH]\b|\bCC\b|caption|hard.of.hearing", re.I)
FORCED = re.compile(r"forced", re.I)
NON_ENGLISH = re.compile(
    r"french|german|spanish|italian|dutch|danish|swedish|norwegian|finnish|polish|czech|thai|"
    r"korean|japanese|portuguese|hungarian|russian|chinese|mandarin|latin|castilian|brazilian|"
    r"arabic|turkish|greek|hebrew|romanian|croatian|bulgarian|estonian|latvian|lithuanian|"
    r"slovak|slovenian|serbian|ukrainian|indonesian|vietnamese|malay|hindi|icelandic|catalan",
    re.I,
)

# What the standing bench holds, for comparison. Glyph counts as extracted.
BENCH = {"10 Cloverfield Lane": 22_851, "A Fish Called Wanda": 31_000, "Gone Girl": 66_438}

# The value `Confidence::ratio` used to saturate at, before #135.
U16_MAX = 65_535


def load(path):
    if not path:
        return []
    return [json.loads(line) for line in open(path, encoding="utf-8")]


def english(track):
    title = track.get("title") or ""
    # A title naming another language settles it, and that is the only reliable half. Otherwise an
    # `eng` tag counts, and so does an untagged track -- 891 of this library's 5,656 bitmap tracks
    # declare no language at all, and excluding them would throw away most of the SDH ones.
    if NON_ENGLISH.search(title):
        return False
    return track.get("lang") in ("eng", None)


def labels(records):
    """How much of the library declares SDH, and how much cannot be classified at all."""
    readable = [r for r in records if "tracks" in r]
    sdh, forced, untitled = [], [], []
    for record in readable:
        tracks = record["tracks"]
        if any(SDH_LABEL.search(t.get("title") or "") and english(t) for t in tracks):
            sdh.append(record)
        if any(FORCED.search(t.get("title") or "") for t in tracks):
            forced.append(record)
        if all(not (t.get("title") or "") for t in tracks):
            untitled.append(record)

    print("== What the labels say ==\n")
    print(f"  titles in the inventory          {len(records):5d}")
    print(f"  readable                         {len(readable):5d}")
    total = len(readable) or 1
    print(f"  English SDH-labelled track       {len(sdh):5d}   {len(sdh) / total * 100:5.1f}%")
    print(f"  no track titles at all           {len(untitled):5d}   {len(untitled) / total * 100:5.1f}%"
          "   <- invisible to any label scan")
    print(f"  a forced-labelled track          {len(forced):5d}   {len(forced) / total * 100:5.1f}%")

    spellings = collections.Counter(
        t["title"] for r in readable for t in r["tracks"]
        if t.get("title") and SDH_LABEL.search(t["title"])
    )
    print(f"\n  distinct SDH spellings           {len(spellings):5d}")
    for name, count in spellings.most_common(6):
        print(f"      {count:5d}  {name[:52]}")
    print("\n  The label is a remuxing convention rather than a property of the disc, so the")
    print("  figure above is a LOWER BOUND. --content puts a number on the undercount.")


def sizes(rows):
    """Where the size extremes are, and whether the bench can reach them."""
    ok = [r for r in rows if "glyphs" in r]
    if not ok:
        return
    glyphs = sorted(r["glyphs"] for r in ok)
    cues = sorted(r["cues"] for r in ok)
    print(f"\n\n== Track size, {len(ok)} sampled ==\n")

    def row(name, values):
        quantile = statistics.quantiles(values, n=20) if len(values) > 3 else [0] * 19
        print(f"  {name:8s} min {values[0]:7,d}   p5 {quantile[0]:8,.0f}   median "
              f"{statistics.median(values):8,.0f}   p95 {quantile[-1]:8,.0f}   max {values[-1]:7,d}")

    row("glyphs", glyphs)
    row("cues", cues)

    over = [r for r in ok if r["glyphs"] > U16_MAX]
    print(f"\n  over {U16_MAX:,} glyphs (what #135 saturated at): {len(over)} of {len(ok)}")
    for record in sorted(over, key=lambda r: -r["glyphs"])[:5]:
        print(f"      {record['glyphs']:7,d}  {record['folder'][:50]}")
    if not over:
        print("      none. A track that would have exposed #135 may not exist in this library,")
        print("      which argues for a synthetic guard rather than a disc.")

    print("\n  the standing bench, for comparison:")
    for name, count in sorted(BENCH.items(), key=lambda kv: kv[1]):
        percentile = sum(1 for g in glyphs if g < count) / len(glyphs) * 100
        print(f"      {count:7,d}  {name:22s}  p{percentile:.0f} of this sample")

    print("\n  smallest:")
    for record in sorted(ok, key=lambda r: r["glyphs"])[:5]:
        print(f"      {record['glyphs']:7,d}  {record['cues']:5,d} cues  {record['folder'][:46]}")


def content(rows):
    """How far the label undercounts, measured from the text rather than the header."""
    ok = [r for r in rows if "speaker_labels" in r]
    if not ok:
        return
    print(f"\n\n== What the ink says, {len(ok)} extracted ==\n")

    def bucket(name, subset):
        if not subset:
            return
        sdh_like = [r for r in subset if r["speaker_labels"] + r["sound_cues"] >= 5]
        print(f"  {name:24s} {len(subset):4d} tracks   SDH by content: {len(sdh_like):4d}"
              f"   {len(sdh_like) / len(subset) * 100:5.1f}%")

    bucket("SDH-labelled", [r for r in ok if r["sdh_labelled"]])
    bucket("not SDH-labelled", [r for r in ok if not r["sdh_labelled"]])
    print("\n  A track counts as SDH by content when it carries at least five speaker labels or")
    print("  bracketed sound cues. Both are conventions, so this is evidence and not ground truth")
    print("  -- but a label that disagrees with the ink is wrong about the disc either way.")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--tracks", required=True)
    ap.add_argument("--sizes")
    ap.add_argument("--content")
    args = ap.parse_args()
    labels(load(args.tracks))
    sizes(load(args.sizes))
    content(load(args.content))


if __name__ == "__main__":
    main()
