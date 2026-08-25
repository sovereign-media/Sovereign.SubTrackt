#!/usr/bin/env python3
"""Turn the library inventory CSV into the JSON `sample.py` and `sweep.py` read.

`docs/library-accuracy.md` reproduces from an `inventory.json` that nothing in the tree created --
it was built by hand the first time, which meant the published sample could not actually be
re-derived from a clean checkout. #209 needed it a second time to widen the alternatives corpus, so
it is a script now.

The CSV records the library as the NAS sees it and this machine reaches it over SMB, so paths are
rewritten the way `scripts/bench/scan.py` already does. The CSV itself is deliberately not
committed: it is an inventory of somebody's media library.

Usage:
    inventory.py --csv image-based-subs-report.csv --out inventory.json
"""
import argparse
import csv
import json
import os
from concurrent.futures import ThreadPoolExecutor

# Same rewrite as `scripts/bench/scan.py`, and for the same reason.
NAS_PREFIX = "//nas/MEDIA/movies"
CSV_PREFIX = "/media/movies"


def local(path):
    return path.replace(CSV_PREFIX, NAS_PREFIX, 1)


def sidecars(mkv):
    """Every `.srt` beside the file. Which of them is English is `sweep.py`'s decision, not ours."""
    try:
        names = os.listdir(os.path.dirname(mkv))
    except OSError:
        return []
    return sorted(n for n in names if n.lower().endswith(".srt"))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--csv", required=True, help="library inventory CSV")
    ap.add_argument("--out", required=True, help="where to write the JSON inventory")
    ap.add_argument("--workers", type=int, default=16, help="folders listed in parallel")
    args = ap.parse_args()

    with open(args.csv, encoding="utf-8") as fh:
        rows = list(csv.DictReader(fh))

    titles = []
    for row in rows:
        titles.append({
            "folder": row["folder"],
            "year": row["year"],
            "codecs": row["codecs"],
            "mkv": local(row["path"]),
        })

    # Listing is a network round trip per title and nothing else here touches the disc, so this is
    # the one place a thread pool earns itself: about 90 seconds serial against a few.
    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        for title, found in zip(titles, pool.map(lambda t: sidecars(t["mkv"]), titles)):
            title["srts"] = found

    with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(titles, fh, indent=1)

    withsrt = sum(1 for t in titles if t["srts"])
    print("inventory %d titles, %d carrying at least one sidecar" % (len(titles), withsrt))


main()
