#!/usr/bin/env python3
"""cmp4.py NAME BASE [BASE ...]: compare a lab run with the mean of several plays of the baseline on the same seeds.

Each BASE is one play of the baseline (v77 and its `fc-salt=S` replays: c2k-v77, c2k-v77-salt1..3). Per seed, the
difference is NAME's win (0/1) minus the baseline's win rate over its plays, so the baseline's own rollout luck
averages out and the test needs fewer seeds than cmp.py's one-against-one. Also splits the seeds by how often the
baseline won them (0..k of k plays): a real gain should show on the seeds the baseline only sometimes wins.

    python3 scripts/lab/cmp4.py c2k-xm05 c2k-v77 c2k-v77-salt1 c2k-v77-salt2 c2k-v77-salt3
"""
import collections
import json
import math
import os
import sys

LAB = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "runs/_policy_lab")


def rows(n):
    return {r["seed"]: r for r in (json.loads(l) for l in open(os.path.join(LAB, n, "results.jsonl")) if l.strip())}


def main(a):
    if len(a) < 2:
        sys.exit(__doc__)
    x, bases = rows(a[0]), [rows(b) for b in a[1:]]
    seeds = [s for s in x if all(s in b for b in bases)]
    k = len(bases)
    d, groups = [], collections.defaultdict(lambda: [0, 0])
    for s in seeds:
        w = sum(bool(b[s]["won"]) for b in bases)
        d.append(bool(x[s]["won"]) - w / k)
        groups[w][0] += bool(x[s]["won"])
        groups[w][1] += 1
    n = len(d)
    if not n:
        sys.exit("no common seeds")
    m = sum(d) / n
    se = math.sqrt(sum((v - m) ** 2 for v in d) / max(1, n - 1) / n)
    print(f"{a[0]} vs mean of {k} baseline plays: n={n} wins {sum(bool(x[s]['won']) for s in seeds)} vs "
          f"{sum(bool(b[s]['won']) for b in bases for s in seeds) / k:.1f}  diff {100 * m:+.1f} +- {100 * se:.1f} pts "
          f"z={m / se if se else 0:+.2f}")
    print("  by baseline wins (of %d): " % k + "  ".join(
        f"{w}/{k}: {g[0]}/{g[1]} (base {100 * w / k:.0f}%)" for w, g in sorted(groups.items())))


if __name__ == "__main__":
    main(sys.argv[1:])
