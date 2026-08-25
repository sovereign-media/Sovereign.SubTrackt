#!/usr/bin/env python3
"""The #131 driver: one `docker run` per (engine, item, repeat), strictly serial.

Deliberately NOT an extension of `scripts/accuracy/sweep.py`, for four reasons recorded in the
issue and worth keeping next to the code:

1. `sweep.py` defaults to `--workers 4`. This is a timing benchmark. Putting a stopwatch inside a
   script whose default is parallel is how a benchmark starts lying quietly -- so there is no
   worker option here at all, not even off by default. An option that invalidates the measurement
   will eventually be used.
2. Its unit of work is a title from an inventory, discovered by heuristic. The unit here is
   (engine, item, repeat): fixed input, fixed sidecar, fixed argv, repeated.
3. Its sidecar rule -- take the best-agreeing -- is correct there and would be poison here. See
   `corpus.json`.
4. `sweep.py` is the instrument behind `docs/library-accuracy.md` and its per-title JSON is a
   published shape. Bending it would put a document's reproducibility at risk to save a file.

What it does reuse is the conventions that work: resumable, one JSON per unit, keep every SRT that
was produced, and leave scoring to a separate pass.

**Scoring is not done here and not inside the container.** `xtask srt-score` is deterministic and is
not a subject of the benchmark, so it runs on the host in `analyse.py`, which also keeps the Rust
toolchain out of the image.

**Repeats are per item, not global** (#209). The first run of this benchmark took three repeats of
everything and the results say what that bought: all 64 (engine, item) pairs were byte-identical
across the three, so every accuracy figure published was the median of three identical numbers. The
repeats were not wasted everywhere, though -- `pgstosrt` read 254.8 / 67.5 / 67.3 s on `deathrace2`,
a 278% spread, with CPU-seconds moving with it (996 against 266), which is a four-fold difference in
work done rather than stopwatch noise. So an item marked `cost` in `corpus.json` is repeated and is
the only kind of item a timing figure may be drawn from; every other item runs once, which is what
makes a twenty-title corpus cost three hours instead of nine.

Usage:
    bench.py build                     # build the image
    bench.py stage [--force]           # dump any missing corpus `.sup` from the media share
    bench.py run   [--repeats 3] [--only ID] [--items KEY,KEY]
    bench.py floor                     # the instrument's own floor
    bench.py cold-warm                 # (k): does the page cache reach this measurement?
"""
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
BENCH = os.path.join(ROOT, 'bench')
CORPUS = os.path.join(BENCH, 'corpus')
RESULTS = os.path.join(BENCH, 'results')
IMAGE = 'subtrackt-bench:131'

sys.path.insert(0, HERE)
import engines as ENG  # noqa: E402

# One container per measurement, so cgroup `memory.peak` is attributable to it. Fixed limits so a
# figure means the same thing on the next machine, and recorded into every record.
CPUS = '4'
MEMORY = '4g'

# A hang is a result too, but only if the run survives it. The longest legitimate unit in this
# corpus is a Tesseract engine over Gone Girl's 2,442 cues, which at sovereign#328's worst measured
# 102 ms/frame is about four minutes; an hour is far outside that and still bounded, so an engine
# that wedges costs one unit rather than the night.
UNIT_TIMEOUT_S = 3600


def sh(cmd, **kw):
    return subprocess.run(cmd, shell=isinstance(cmd, str), capture_output=True, text=True, **kw)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as fh:
        for block in iter(lambda: fh.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def load_corpus():
    with open(os.path.join(HERE, 'corpus.json'), encoding='utf-8') as fh:
        return json.load(fh)


def build():
    commit = sh('git rev-parse HEAD', cwd=ROOT).stdout.strip()
    cmd = [
        'docker', 'build', '-f', os.path.join(HERE, 'Dockerfile'),
        '--build-arg', 'SUBTRACKT_COMMIT=' + commit,
        '-t', IMAGE, ROOT,
    ]
    print(' '.join(cmd))
    return subprocess.call(cmd)


# `/usr/bin/time -v` writes `  Key: value`, and several values themselves contain ': '. Split on the
# FIRST ': ' after stripping, which is what the format actually guarantees.
TIME_LINE = re.compile(r'^\s*(.+?): (.*)$')


def parse_time_v(text):
    out = {}
    for line in text.splitlines():
        m = TIME_LINE.match(line)
        if m:
            out[m.group(1).strip()] = m.group(2).strip()
    return out


def _seconds(value):
    """`h:mm:ss` or `m:ss.cc` into seconds. Note the centisecond resolution of the wall figure."""
    parts = value.split(':')
    try:
        parts = [float(p) for p in parts]
    except ValueError:
        return None
    total = 0.0
    for p in parts:
        total = total * 60 + p
    return total


def lift(raw):
    """The five numbers the tables need, out of the whole `time -v` dict."""
    wall = _seconds(raw.get('Elapsed (wall clock) time (h:mm:ss or m:ss)', ''))
    user = raw.get('User time (seconds)')
    sysv = raw.get('System time (seconds)')
    cpu = None
    if user is not None and sysv is not None:
        try:
            cpu = float(user) + float(sysv)
        except ValueError:
            cpu = None
    pct = raw.get('Percent of CPU this job got', '').rstrip('%')
    rss = raw.get('Maximum resident set size (kbytes)')
    return {
        'wall_s': wall,
        'cpu_s': cpu,
        'pct_cpu': float(pct) if pct.replace('.', '', 1).isdigit() else None,
        'max_rss_kb': int(rss) if rss and rss.isdigit() else None,
        'exit_status': raw.get('Exit status'),
    }


UNIT_SCRIPT = r'''set -u
/usr/bin/time -v -o /report/time.txt -- sh -c '{body}' >/report/stdout 2>/report/stderr
echo $? > /report/rc
cat /sys/fs/cgroup/memory.peak > /report/peak 2>/dev/null || echo unavailable > /report/peak
[ -f {out} ] && cp {out} /report/out.raw.srt
exit 0
'''


def run_unit(engine, item, repeat, outdir):
    """One measurement: one container, one engine, one item."""
    os.makedirs(outdir, exist_ok=True)
    sup = '/corpus/' + item['sup']
    body = ENG.render(engine, sup).replace("'", "'\\''")
    script = UNIT_SCRIPT.format(body=body, out=ENG.OUT)

    cmd = [
        'docker', 'run', '--rm',
        # Proves nothing was downloaded rather than assuming it. Several of these tools fetch
        # models on first run.
        '--network', 'none',
        '--cpus', CPUS, '--memory', MEMORY,
        '-v', CORPUS + ':/corpus:ro',
        '-v', outdir + ':/report',
        # Outputs to tmpfs; copied to the report directory only after `time` has exited, so no
        # engine is charged for a write across the Docker Desktop VM boundary.
        '--mount', 'type=tmpfs,destination=/tmp,tmpfs-size=1g',
        IMAGE, 'sh', '-c', script,
    ]
    env = dict(os.environ, MSYS_NO_PATHCONV='1')
    started = time.time()
    timed_out = False
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, env=env,
                              timeout=UNIT_TIMEOUT_S)
        docker_rc = proc.returncode
    except subprocess.TimeoutExpired:
        timed_out = True
        docker_rc = None
    outer = time.time() - started

    def read(name, limit=None):
        path = os.path.join(outdir, name)
        if not os.path.exists(path):
            return None
        with open(path, encoding='utf-8', errors='replace') as fh:
            data = fh.read()
        return data[:limit] if limit else data

    raw = parse_time_v(read('time.txt') or '')
    rc_text = (read('rc') or '').strip()
    record = {
        'engine': engine['id'],
        'tool': engine['tool'],
        'family': engine['family'],
        'item': item['key'],
        'repeat': repeat,
        'argv': ENG.render(engine, sup),
        'rc': int(rc_text) if rc_text.isdigit() else None,
        'docker_rc': docker_rc,
        'timed_out': timed_out,
        'outer_wall_s': round(outer, 3),
        'cgroup_peak_bytes': (read('peak') or '').strip(),
        'time_v': raw,
        'stderr_tail': (read('stderr') or '')[-4000:],
        'stdout_tail': (read('stdout') or '')[-2000:],
        'cpus': CPUS,
        'memory': MEMORY,
    }
    record.update(lift(raw))

    produced = os.path.join(outdir, 'out.raw.srt')
    if os.path.exists(produced):
        record['out_sha256'] = sha256_file(produced)
        record['out_bytes'] = os.path.getsize(produced)
    else:
        # (n): failures are results. An engine that produced nothing is recorded with its status,
        # never dropped from a pooled figure without a printed attempted/completed/failed count.
        record['out_sha256'] = None
        record['out_bytes'] = 0
        record['status'] = 'no-output'
    return record


def run(args):
    corpus = load_corpus()
    items = {i['key']: i for i in corpus['items']}
    wanted_items = args.items.split(',') if args.items else list(items)

    # An item's repeat count is a property of the item, not of the invocation. `cost` items are
    # repeated because a timing figure needs them to be; everything else runs once because the
    # output is provably identical every time. See the module docstring.
    def repeats_for(item):
        return args.repeats if item.get('cost') else 1

    plan = []
    for engine in ENG.ENGINES:
        if args.only and engine['id'] != args.only:
            continue
        for key in wanted_items:
            plan.append((engine, items[key]))
    for engine in ENG.SENSITIVITY:
        if args.only and engine['id'] != args.only:
            continue
        for key in ENG.SENSITIVITY_ITEMS:
            if key in wanted_items:
                plan.append((engine, items[key]))

    os.makedirs(RESULTS, exist_ok=True)
    total = sum(repeats_for(item) for _, item in plan)
    done = 0
    started = time.time()

    # Round-robin by repeat rather than three passes of one engine, so thermal or background drift
    # hits every engine equally instead of landing on whichever ran last.
    for repeat in range(1, args.repeats + 1):
        for engine, item in plan:
            if repeat > repeats_for(item):
                continue
            done += 1
            name = '{}--{}--{}'.format(engine['id'], item['key'], repeat)
            record_path = os.path.join(RESULTS, name + '.json')
            if os.path.exists(record_path) and not args.force:
                print('[{}/{}] {} (cached)'.format(done, total, name))
                continue
            outdir = os.path.join(RESULTS, name)
            if os.path.exists(outdir):
                shutil.rmtree(outdir, ignore_errors=True)
            print('[{}/{}] {} ... '.format(done, total, name), end='', flush=True)
            record = run_unit(engine, item, repeat, outdir)
            with open(record_path, 'w', encoding='utf-8', newline='\n') as fh:
                json.dump(record, fh, indent=2)
            elapsed = time.time() - started
            print('rc={} wall={} rss={}kB  [{:.0f}s elapsed]'.format(
                record['rc'], record['wall_s'], record['max_rss_kb'], elapsed))
    print('\n{} units, {:.0f}s total'.format(total, time.time() - started))


def media_path(root, folder):
    """The largest `.mkv` in the title's folder, which is the feature rather than an extra.

    Same rule as `scripts/bench/run.py:media_path`, and it has to be: the two benches must agree on
    which file a folder means or a figure from one cannot be set beside a figure from the other.
    """
    directory = os.path.join(root, folder)
    files = [f for f in os.listdir(directory) if f.lower().endswith('.mkv')]
    if not files:
        raise SystemExit('no .mkv in ' + directory)
    return max((os.path.join(directory, f) for f in files), key=os.path.getsize)


def stage(args):
    """Populate `bench/corpus/` from the media share.

    The corpus is not shareable and is not baked into the image, so it has always been built
    locally -- but until #209 it was built by hand, which was tolerable for six items and is not for
    twenty. `fixture.sup` is generated rather than dumped and is skipped here; `xtask make-fixture`
    makes it.
    """
    corpus = load_corpus()
    root = corpus['root']
    os.makedirs(CORPUS, exist_ok=True)
    for item in corpus['items']:
        out = os.path.join(CORPUS, item['sup'])
        if item.get('folder') is None:
            print('  {:15s} generated, not dumped'.format(item['key']))
            continue
        if os.path.exists(out) and os.path.getsize(out) > 0 and not args.force:
            print('  {:15s} cached'.format(item['key']))
            continue
        print('  {:15s} dumping...'.format(item['key']), flush=True)
        # Retried because a read over SMB fails about 1.7% of the time and succeeds next attempt.
        # #133 measured that; it is not a defect and it is not worth losing a long pass to.
        for attempt in range(3):
            cmd = [args.xtask, 'dump-sup', media_path(root, item['folder']), out]
            if item.get('stream') is not None:
                cmd += ['--stream', str(item['stream'])]
            result = sh(cmd)
            if os.path.exists(out) and os.path.getsize(out) > 0:
                print('  {:15s} {:.1f} MB'.format(item['key'], os.path.getsize(out) / 1048576))
                break
            print('  {:15s} retry {}: {}'.format(
                item['key'], attempt + 1, (result.stderr or '').strip()[:80]), flush=True)
        else:
            print('  {:15s} FAILED'.format(item['key']))


def floor(args):
    """(l): establish the instrument's floor. Anything within 3x of it is not a number."""
    script = (
        'for i in $(seq 1 50); do /usr/bin/time -v -o /tmp/t.txt -- /bin/true; '
        'grep "wall clock" /tmp/t.txt; done'
    )
    env = dict(os.environ, MSYS_NO_PATHCONV='1')
    proc = subprocess.run(
        ['docker', 'run', '--rm', '--network', 'none', '--cpus', CPUS, '--memory', MEMORY,
         IMAGE, 'sh', '-c', script],
        capture_output=True, text=True, env=env)
    times = []
    for line in proc.stdout.splitlines():
        m = TIME_LINE.match(line)
        if m:
            v = _seconds(m.group(2))
            if v is not None:
                times.append(v)
    times.sort()
    result = {
        'n': len(times),
        'min_s': times[0] if times else None,
        'median_s': times[len(times) // 2] if times else None,
        'max_s': times[-1] if times else None,
        'report_below_s': (times[len(times) // 2] * 3) if times else None,
    }
    os.makedirs(RESULTS, exist_ok=True)
    with open(os.path.join(RESULTS, 'floor.json'), 'w', encoding='utf-8', newline='\n') as fh:
        json.dump(result, fh, indent=2)
    print(json.dumps(result, indent=2))


def cold_warm(args):
    """(k): the house rule says measure cold or not at all. Demonstrate whether it reaches here.

    That rule was written for a demux comparison over a multi-gigabyte file, where a warm page cache
    deletes the thing being measured. This is CPU-bound recognition over a few-megabyte `.sup`, so
    the rule may not apply -- but the way to find that out is to measure it, not to assert it.
    """
    corpus = load_corpus()
    item = {i['key']: i for i in corpus['items']}['clover']
    engine = ENG.by_id('subtrackt-arial')
    out = {'cold': [], 'warm': []}
    env = dict(os.environ, MSYS_NO_PATHCONV='1')
    for n in range(1, args.repeats + 1):
        for mode in ('cold', 'warm'):
            if mode == 'cold':
                # Drop the LinuxKit VM's cache, not the container's.
                subprocess.run(
                    ['docker', 'run', '--rm', '--privileged', IMAGE,
                     'sh', '-c', 'sync; echo 3 > /proc/sys/vm/drop_caches || true'],
                    capture_output=True, text=True, env=env)
            outdir = os.path.join(RESULTS, 'coldwarm-{}-{}'.format(mode, n))
            shutil.rmtree(outdir, ignore_errors=True)
            rec = run_unit(engine, item, n, outdir)
            out[mode].append(rec['wall_s'])
            print('{} {}: {}'.format(mode, n, rec['wall_s']))
    both = {}
    for mode, values in out.items():
        clean = [v for v in values if v]
        both[mode] = sum(clean) / len(clean) if clean else None
    delta = None
    if both['cold'] and both['warm']:
        delta = abs(both['cold'] - both['warm']) / both['cold'] * 100
    result = {'runs': out, 'mean': both, 'delta_pct_of_cold': delta,
              'verdict': 'page cache does not reach this measurement' if (delta or 0) < 2
                         else 'RE-TAKE EVERYTHING COLD'}
    with open(os.path.join(RESULTS, 'cold-warm.json'), 'w', encoding='utf-8', newline='\n') as fh:
        json.dump(result, fh, indent=2)
    print(json.dumps(result, indent=2))


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    sub.add_parser('build')
    st = sub.add_parser('stage')
    st.add_argument('--xtask', default=os.path.normpath('target/release/xtask'))
    st.add_argument('--force', action='store_true')
    r = sub.add_parser('run')
    # The count for `cost` items only. Everything else runs once and this flag does not reach it,
    # because the repeat count of an accuracy item is a fact about the engines rather than a knob.
    r.add_argument('--repeats', type=int, default=3)
    r.add_argument('--only')
    r.add_argument('--items')
    r.add_argument('--force', action='store_true')
    sub.add_parser('floor')
    cw = sub.add_parser('cold-warm')
    cw.add_argument('--repeats', type=int, default=10)
    args = ap.parse_args()

    if args.cmd == 'build':
        sys.exit(build())
    if args.cmd == 'stage':
        stage(args)
    elif args.cmd == 'run':
        run(args)
    elif args.cmd == 'floor':
        floor(args)
    elif args.cmd == 'cold-warm':
        cold_warm(args)


if __name__ == '__main__':
    main()
