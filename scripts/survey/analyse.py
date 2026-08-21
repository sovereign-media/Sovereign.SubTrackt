"""Cluster sampled titles by glyph shape, to answer #8.

The question is not "which font is this" but "how many visually distinct glyph sets are there, and
would a handful cover most of the library".
"""
import json, glob, os, collections, itertools, statistics, sys

HERE = sys.argv[1] if len(sys.argv) > 1 else "shapes"
TOP_SHAPES = 60      # the core alphabet of a title; the tail is rare accents and noise
BITS = 256


def popcount(x):
    return bin(x).count("1")


def load():
    out = []
    for path in sorted(glob.glob(os.path.join(HERE, "*.json"))):
        d = json.load(open(path))
        shapes = [(int(h, 16), n) for h, n in d["shapes"]]
        shapes.sort(key=lambda t: -t[1])
        # Drop empty/degenerate vectors: they carry no shape information.
        shapes = [(v, n) for v, n in shapes if popcount(v) >= 8]
        d["top"] = [v for v, _ in shapes[:TOP_SHAPES]]
        d["n_shapes"] = len(shapes)
        if d["top"]:
            out.append(d)
    return out


def nearest(v, others):
    return min(popcount(v ^ o) for o in others)


def similarity(a, b, thresh):
    """Fraction of a's core alphabet with a close counterpart in b."""
    if not a["top"] or not b["top"]:
        return 0.0
    hits = sum(1 for v in a["top"] if nearest(v, b["top"]) <= thresh)
    return hits / len(a["top"])


def main():
    titles = load()
    print(f"titles analysed: {len(titles)}")
    print(f"  glyphs total  : {sum(t['glyphs'] for t in titles):,}")
    print(f"  shapes/title  : median {int(statistics.median(t['n_shapes'] for t in titles))}")
    heights = [t["median_height"] for t in titles if t["median_height"]]
    print(f"  glyph height  : min {min(heights)} median {int(statistics.median(heights))} max {max(heights)}")
    kinds = collections.Counter(t["kind"] for t in titles)
    print(f"  by codec      : {dict(kinds)}")
    print(f"  by decade     : {dict(sorted(collections.Counter(t['decade'] for t in titles).items()))}")

    # Calibrate: distance from each title's glyphs to their nearest counterpart in another title.
    print("\n--- nearest-neighbour distance between titles ---")
    samples = []
    for a, b in itertools.combinations(titles, 2):
        for v in a["top"][:20]:
            samples.append(nearest(v, b["top"]))
    samples.sort()
    for q in (5, 25, 50, 75, 95):
        print(f"  p{q:<3}: {samples[len(samples) * q // 100]:3d} cells of {BITS}")

    for thresh in (16, 24, 32, 40):
        print(f"\n=== clustering at Hamming <= {thresh} ({thresh * 100 // BITS}% of the vector) ===")
        sim = {}
        for a, b in itertools.combinations(range(len(titles)), 2):
            s = min(similarity(titles[a], titles[b], thresh),
                    similarity(titles[b], titles[a], thresh))
            sim[(a, b)] = s

        for link in (0.75,):
            parent = list(range(len(titles)))

            def find(x):
                while parent[x] != x:
                    parent[x] = parent[parent[x]]
                    x = parent[x]
                return x

            for (a, b), s in sim.items():
                if s >= link:
                    ra, rb = find(a), find(b)
                    if ra != rb:
                        parent[rb] = ra

            groups = collections.defaultdict(list)
            for i in range(len(titles)):
                groups[find(i)].append(i)
            sizes = sorted((len(v) for v in groups.values()), reverse=True)
            singles = sum(1 for s_ in sizes if s_ == 1)
            covered = sum(sizes[:3])
            print(f"  link>={link}: {len(groups)} clusters, sizes {sizes[:8]}"
                  f"{'...' if len(sizes) > 8 else ''}")
            print(f"    top-3 clusters cover {covered}/{len(titles)} titles"
                  f" ({covered * 100 // len(titles)}%); {singles} singletons")
            if thresh == 32:
                for root, members in sorted(groups.items(), key=lambda kv: -len(kv[1]))[:4]:
                    yrs = sorted(int(titles[i]["year"]) for i in members)
                    kinds = collections.Counter(titles[i]["kind"] for i in members)
                    print(f"      cluster of {len(members):2d}: years {yrs[0]}-{yrs[-1]}, {dict(kinds)}")


main()
