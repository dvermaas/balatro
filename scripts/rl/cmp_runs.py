#!/usr/bin/env python3
"""cmp_runs.py NAME [NAME...] [--base B1,B2,...]: for jevlab runs in runs/_policy_lab, wins, mean ante, deaths by
ante, and the paired comparison (games won by only one side, z = (p - q) / sqrt(p + q)) against each base on the
seeds both hold; plus per game the network's decisions and v77's states (JEVLAB_RLANTES runs).
Default bases: rl-ppoa74-nors (iteration 74 alone, c2k 0-449), rl-ho-p14 (it74 antes 1-4, v77 5+), rl-v77-nors."""
import argparse, collections, json, math
from pathlib import Path

L = Path(__file__).resolve().parents[2] / "runs/_policy_lab"


def rows(n):
    return {r["seed"]: r for r in (json.loads(l) for l in open(L / n / "results.jsonl") if l.strip()) if "won" in r}


ap = argparse.ArgumentParser()
ap.add_argument("names", nargs="+")
ap.add_argument("--base", default="rl-ppoa74-nors,rl-ho-p14,rl-v77-nors")
a = ap.parse_args()
for n in a.names:
    x = rows(n)
    d = collections.Counter(r["ante"] for r in x.values() if not r["won"])
    print(f"{n}: n={len(x)} wins={sum(r['won'] for r in x.values())} mean_ante={sum(r['ante'] for r in x.values()) / len(x):.2f} "
          f"deaths by ante 1-8: {' '.join(str(d[k]) for k in range(1, 9))} (by ante 4: {sum(d[k] for k in range(1, 5))})")
    for base in filter(None, a.base.split(",")):
        b = rows(base)
        c = [s for s in x if s in b]
        p = sum(x[s]["won"] and not b[s]["won"] for s in c)
        q = sum(b[s]["won"] and not x[s]["won"] for s in c)
        print(f"   vs {base}: wins {sum(x[s]['won'] for s in c)} vs {sum(b[s]['won'] for s in c)} on {len(c)}, only one side {p}/{q}, "
              f"z={(p - q) / math.sqrt(max(1, p + q)):+.2f}")
    print(f"   per game: network decisions {sum(r.get('rl_decisions', 0) for r in x.values()) / len(x):.1f}, "
          f"v77 states {sum(r.get('rl_v77_states', 0) for r in x.values()) / len(x):.1f}")
