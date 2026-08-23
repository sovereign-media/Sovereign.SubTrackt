#!/usr/bin/env python3
"""Characterise the library's bitmap subtitle tracks, so a bench can be chosen rather than guessed.

Answers #133. The three discs every accuracy figure rests on are all non-SDH, mid-length and
mid-density, and two defects in one week (#130, #135) were invisible to them for exactly that
reason. Choosing better material needs a picture of what is out there.

Reads track headers only -- `subtrackt list` is ~60 ms per title, so the whole library is minutes
rather than the hours a decode pass would cost. What that buys is the *declared* shape of every
track: language, plane size, and the title a remuxer wrote.

**The SDH figure this produces is a lower bound and must be reported as one.** A track is counted
as SDH when its title says so, and a title is a remuxing convention rather than a property of the
disc: 10 Cloverfield Lane's English track carries no title at all. `sample.py` measures the
undercount on a sample by looking at the ink instead.

Usage:
    scripts/bench/scan.py --csv image-based-subs-report.csv --out tracks.jsonl
"""

import argparse
import csv
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

# The inventory records the library as the NAS sees it; this machine reaches it over SMB.
NAS_PREFIX = "//nas/MEDIA/movies"
CSV_PREFIX = "/media/movies"

# `subtrackt list` writes one row per bitmap track: index, codec, language, WxH, then an optional
# title that may itself contain spaces.
ROW = re.compile(r"^\s*(\d+)\s+(\S+)\s+(\S+)\s+(\d+)x(\d+)\s*(.*)$")

# What a remuxer writes when a track carries captions for the deaf and hard of hearing. Matched
# case-insensitively against the whole title.
SDH = re.compile(r"\bSDH\b|\bCC\b|caption", re.IGNORECASE)
FORCED = re.compile(r"\bforced\b", re.IGNORECASE)


def local(path):
    """Rewrite an inventory path to something this machine can open."""
    return path.replace(CSV_PREFIX, NAS_PREFIX, 1)


def parse(text):
    """Turn `subtrackt list` output into track records."""
    tracks = []
    for line in text.splitlines():
        m = ROW.match(line)
        if not m:
            continue
        index, codec, lang, w, h, title = m.groups()
        title = title.strip()
        tracks.append(
            {
                "index": int(index),
                "codec": codec,
                # `list` prints `--` for a track that declares no language.
                "lang": None if lang == "--" else lang,
                "width": int(w),
                "height": int(h),
                "title": title or None,
                "sdh": bool(SDH.search(title)),
                "forced": bool(FORCED.search(title)),
            }
        )
    return tracks


def one(binary, row):
    """Read one title's tracks, or record why it could not be read."""
    path = local(row["path"])
    record = {"folder": os.path.basename(os.path.dirname(path)), "year": row.get("year")}
    if not os.path.exists(path):
        return {**record, "error": "missing"}
    try:
        out = subprocess.run(
            [binary, "list", path],
            capture_output=True,
            text=True,
            timeout=120,
            errors="replace",
        )
    except subprocess.TimeoutExpired:
        return {**record, "error": "timeout"}
    if out.returncode != 0:
        # The stub formats and malformed files land here. Kept rather than dropped: a scan that
        # silently skips what it cannot read reports a cleaner library than exists.
        return {**record, "error": (out.stderr or "").strip()[:200] or "failed"}
    return {**record, "tracks": parse(out.stdout)}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--csv", required=True, help="the library inventory")
    ap.add_argument("--out", required=True, help="where to write one JSON record per title")
    ap.add_argument("--binary", default="target/release/subtrackt.exe")
    ap.add_argument("--workers", type=int, default=8, help="concurrent header reads")
    ap.add_argument("--limit", type=int, default=0, help="stop after N titles, for a dry run")
    args = ap.parse_args()

    with open(args.csv, newline="", encoding="utf-8") as fh:
        rows = [r for r in csv.DictReader(fh) if r.get("path")]
    if args.limit:
        rows = rows[: args.limit]

    done = 0
    with open(args.out, "w", encoding="utf-8") as out:
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            for record in pool.map(lambda r: one(args.binary, r), rows):
                out.write(json.dumps(record) + "\n")
                done += 1
                if done % 100 == 0:
                    print(f"  {done}/{len(rows)}", file=sys.stderr, flush=True)
    print(f"{done} titles -> {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
