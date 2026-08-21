#!/usr/bin/env python3
"""Collect distinct glyph shapes from a stratified sample of a media library.

Writes one JSON file of shapes per sampled title, so `analyse.py` can be re-run without touching
the media again. Sampling is deterministic — titles sorted by folder name, then an even stride
within each decade — so a re-run against the same library reproduces the same numbers.

Reads the library inventory CSV produced elsewhere; it needs `year`, `folder`, `codecs` and `path`
columns. The CSV is deliberately not committed: it is an inventory of somebody's media library.
"""
import argparse
import collections
import csv
import json
import os
import subprocess
import time


def parse_args():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--csv", required=True, help="library inventory CSV")
    ap.add_argument("--out", default="shapes", help="directory for per-title shape files")
    ap.add_argument("--bin", default="target/release/subtrackt", help="the subtrackt binary")
    ap.add_argument(
        "--media-root",
        default="//nas/MEDIA/",
        help="what to substitute for the /media/ prefix recorded in the CSV",
    )
    ap.add_argument("--limit", type=int, default=100, help="cues to read per title")
    ap.add_argument("--per-decade", type=int, default=9, help="PGS titles sampled per decade")
    ap.add_argument("--vobsub", type=int, default=12, help="VOBSUB titles sampled in total")
    return ap.parse_args()


def pick(args):
    """Choose a stratified, deterministic sample."""
    rows = [
        r
        for r in csv.DictReader(open(args.csv, encoding="utf-8"))
        if r["path"].endswith(".mkv")
    ]
    pgs = [r for r in rows if "hdmv_pgs" in r["codecs"]]
    vob = [r for r in rows if "dvd_subtitle" in r["codecs"]]

    by_decade = collections.defaultdict(list)
    for r in pgs:
        try:
            by_decade[int(r["year"]) // 10 * 10].append(r)
        except ValueError:
            pass

    chosen = []
    for decade in sorted(by_decade):
        group = sorted(by_decade[decade], key=lambda r: r["folder"])
        step = max(1, len(group) // args.per_decade)
        chosen += [(decade, "pgs", g) for g in group[::step][: args.per_decade]]

    vob = sorted(vob, key=lambda r: r["folder"])
    step = max(1, len(vob) // args.vobsub)
    chosen += [
        (int(r["year"]) // 10 * 10, "vobsub", r) for r in vob[::step][: args.vobsub]
    ]
    return chosen


def collect_one(args, decade, kind, row, dest):
    path = args.media_root + row["path"].split("/media/", 1)[1]
    if not os.path.exists(path):
        return None, "not found"

    try:
        proc = subprocess.run(
            [args.bin, "glyphs", path, "--limit", str(args.limit)],
            capture_output=True,
            timeout=300,
        )
    except subprocess.TimeoutExpired:
        return None, "timeout"

    if proc.returncode != 0:
        tail = proc.stderr.decode(errors="replace").strip().splitlines()
        return None, (tail[-1][:80] if tail else "failed")

    counts = collections.Counter()
    heights = []
    for line in proc.stdout.decode(errors="replace").splitlines():
        f = line.split("\t")
        if len(f) == 7:
            counts[f[6]] += 1
            heights.append(int(f[5]))

    summary = proc.stderr.decode(errors="replace").strip().split("\t")
    json.dump(
        {
            "folder": row["folder"],
            "year": row["year"],
            "decade": decade,
            "kind": kind,
            "plane": summary[3] if len(summary) > 3 else "?",
            "glyphs": sum(counts.values()),
            "median_height": sorted(heights)[len(heights) // 2] if heights else 0,
            "shapes": counts.most_common(),
        },
        open(dest, "w"),
    )
    return counts, None


def main():
    args = parse_args()
    os.makedirs(args.out, exist_ok=True)

    picks = pick(args)
    print(f"sampling {len(picks)} titles", flush=True)
    done = failed = 0
    reasons = collections.Counter()
    start = time.time()

    for decade, kind, row in picks:
        key = "".join(c if c.isalnum() else "_" for c in row["folder"])[:70]
        dest = os.path.join(args.out, key + ".json")
        if os.path.exists(dest):
            done += 1
            continue

        counts, error = collect_one(args, decade, kind, row, dest)
        if error:
            failed += 1
            reasons[error] += 1
            print(f"  FAIL {row['folder'][:45]}: {error}", flush=True)
            continue

        done += 1
        print(
            f"  [{done + failed}/{len(picks)}] {decade}s {kind} "
            f"{sum(counts.values())} glyphs, {len(counts)} shapes  {row['folder'][:40]}",
            flush=True,
        )

    print(f"\ndone={done} failed={failed} in {time.time() - start:.0f}s")
    for reason, n in reasons.most_common():
        print(f"  {n:3d}  {reason}")


main()
