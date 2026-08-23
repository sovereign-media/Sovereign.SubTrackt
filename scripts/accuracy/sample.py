#!/usr/bin/env python3
"""Choose a sample of titles that stands for the library rather than for whatever ran first.

Deterministic: titles are ordered within each decade by a hash of the folder name, then taken by an
even stride, with each decade allocated a share of the sample proportional to its share of the
library. A re-run picks the same titles, and the ordering is independent of year, size, codec and of
anything the pipeline does -- so the sample is not selected on the outcome being measured.

Reads the same inventory `sweep.py` does: a JSON list of objects carrying `folder`, `year`,
`codecs`, `mkv` and `srts`.
"""
import argparse
import collections
import hashlib
import json


def parse_args():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--inventory", required=True, help="JSON list of every candidate title")
    ap.add_argument("--out", required=True, help="where to write the sampled subset")
    ap.add_argument("--count", type=int, default=50, help="titles to sample")
    return ap.parse_args()


def key(title):
    return hashlib.sha256(title["folder"].encode("utf-8")).hexdigest()


def main():
    args = parse_args()
    pool = [t for t in json.load(open(args.inventory, encoding="utf-8")) if t.get("srts")]

    by_decade = collections.defaultdict(list)
    for title in pool:
        try:
            by_decade[int(title["year"]) // 10 * 10].append(title)
        except (ValueError, TypeError, KeyError):
            by_decade[0].append(title)

    chosen = []
    for decade in sorted(by_decade):
        group = sorted(by_decade[decade], key=key)
        take = max(1, round(args.count * len(group) / len(pool)))
        step = max(1, len(group) // take)
        chosen += group[::step][:take]

    chosen = sorted(chosen, key=key)[: args.count]
    json.dump(chosen, open(args.out, "w", encoding="utf-8"), indent=1)

    print("pool %d -> sample %d" % (len(pool), len(chosen)))
    counts = collections.Counter(int(t["year"]) // 10 * 10 for t in chosen)
    for decade in sorted(counts):
        print("  %ss %d" % (decade, counts[decade]))


main()
