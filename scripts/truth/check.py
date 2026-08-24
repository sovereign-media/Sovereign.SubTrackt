#!/usr/bin/env python3
"""Score the post-correction arms against ground truth a person read off the disc.

#185. `docs/post-correction.md` has named the same criterion for the life of the file:

> **What would flip it:** the same table, produced over real tracks with hand-verified ground
> truth, still showing zero lines made worse.

Every other figure in this repository is scored against a **release sidecar** -- another transcript
of the same dialogue, frequently read off the same bitmaps by some other tool -- so a systematic
error the corrector introduced could be matched by the same systematic error in the comparison and
score as agreement. `wanda-0000-0299.srt` is the answer: the first 300 cues of A Fish Called Wanda,
transcribed from the images by eye.

    $ scripts/truth/check.py --sup bench-cache/wanda.sup --reference arial-ri.subtref

It runs the arms in the order they compose -- nothing, then the context corrector, then the
one-character word -- and prints what each is worth against the truth and against the arm below it.
Read the `worse` column.
"""

import argparse
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
TRUTH = os.path.join(HERE, "wanda-0000-0299.srt")

# The arms, in the order they compose. Each row is (name, extra flags for `extract`).
# Every arm names every flag it wants, including the ones it wants *off*. The defaults moved once
# already -- #185 turned two of these on, on the evidence this script prints -- and an arm that
# said nothing would have quietly become a copy of the row below it.
ARMS = [
    ("off", ["--no-post-correct"]),
    ("context", ["--post-correct", "--no-lone-words"]),
    # `--assume-english` because a `.sup` is a codec dump with no container around it, so it carries
    # no language tag for the contraction half to read. #180 has why that half needs one.
    ("lone words", ["--post-correct", "--lone-words", "--assume-english"]),
]

CER = re.compile(r"^\s+all\s+\S+\s+\S+\s+(\S+)%\s+\S+%\s+\S+\s+(\S+)%", re.M)
COMPARE = re.compile(r"cues improved : (\d+).*?cues worse\s+: (\d+)", re.S)


def run(cmd):
    result = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if result.returncode != 0:
        sys.exit(f"failed: {' '.join(cmd)}\n{result.stderr}")
    return (result.stdout or "") + (result.stderr or "")


def cues(path):
    """How many cues an SRT holds."""
    text = open(path, encoding="utf-8-sig").read().replace("\r\n", "\n").strip()
    return len(text.split("\n\n")) if text else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--sup", default="bench-cache/wanda.sup")
    ap.add_argument("--reference", default="arial-ri.subtref")
    ap.add_argument("--truth", default=TRUTH)
    ap.add_argument("--out-dir", default="truth-check")
    ap.add_argument("--binary", default=os.path.normpath("target/release/subtrackt.exe"))
    ap.add_argument("--xtask", default=os.path.normpath("target/release/xtask.exe"))
    args = ap.parse_args()

    limit = cues(args.truth)
    os.makedirs(args.out_dir, exist_ok=True)
    build = os.path.join(HERE, "build.py")

    trimmed = {}
    for name, flags in ARMS:
        key = name.replace(" ", "-")
        full = os.path.join(args.out_dir, f"{key}.srt")
        run([args.binary, "extract", args.sup, "--reference", args.reference,
             "--format", "srt", "-o", full] + flags)
        cut = os.path.join(args.out_dir, f"{key}-{limit}.srt")
        run([sys.executable, build, "--trim", full, "--to", str(limit), "--out", cut])
        trimmed[name] = cut

    print(f"\n{limit} cues of A Fish Called Wanda, against ground truth read off the disc\n")
    print(f"  {'arm':12s} {'CER':>7s} {'WER':>7s} {'better':>8s} {'worse':>7s}")
    previous = None
    for name, _flags in ARMS:
        scored = run([args.xtask, "srt-score", trimmed[name], args.truth])
        match = CER.search(scored)
        cer, wer = (match.group(1), match.group(2)) if match else ("?", "?")
        better = worse = "-"
        if previous is not None:
            compared = run([args.xtask, "srt-score", trimmed[previous], args.truth,
                            "--compare", trimmed[name]])
            found = COMPARE.search(compared)
            if found:
                better, worse = found.group(1), found.group(2)
        print(f"  {name:12s} {cer:>6s}% {wer:>6s}% {better:>8s} {worse:>7s}")
        previous = name
    print("\n  better/worse are against the arm above, cue by cue.")


if __name__ == "__main__":
    main()
