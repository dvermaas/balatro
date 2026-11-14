#!/usr/bin/env python3
"""decomp.py NAME [NAME...]: wins, mean ante, deaths by ante, seconds per game and games won by only one
side vs c2k-v77, for jevlab runs in runs/_policy_lab (milestone 3 decomposition table)."""
import json, sys, collections
from pathlib import Path

L = Path(__file__).resolve().parents[2] / "runs/_policy_lab"


def rows(name):
    return {r["seed"]: r for r in (json.loads(l) for l in open(L / name / "results.jsonl") if l.strip())}


base = rows("c2k-v77")
for name in sys.argv[1:]:
    r = rows(name)
    n = len(r)
    w = sum(bool(x["won"]) for x in r.values())
    ante = sum(x["ante"] for x in r.values()) / n
    sec = sum(x["seconds"] for x in r.values()) / n
    deaths = collections.Counter(x["ante"] for x in r.values() if not x["won"])
    common = [s for s in r if s in base]
    only_a = sum(1 for s in common if r[s]["won"] and not base[s]["won"])
    only_b = sum(1 for s in common if base[s]["won"] and not r[s]["won"])
    ex = {k: sum(x.get(k, 0) for x in r.values()) / n for k in ("rl_decisions", "rl_reorders", "rl_targets_planned", "rl_targets_fallback")}
    print(f"{name}: n={n} wins={w} ({100*w/n:.1f}%) mean_ante={ante:.2f} sec/game={sec:.2f} "
          f"onlywins {only_a}/{only_b} (vs v77 {sum(bool(base[s]['won']) for s in common)})")
    print("   deaths by ante:", " ".join(f"{a}:{deaths[a]}" for a in sorted(deaths)))
    if any(ex.values()):
        print("   per game:", " ".join(f"{k}={v:.1f}" for k, v in ex.items()))
