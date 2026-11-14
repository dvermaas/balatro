#!/usr/bin/env python3
"""behaviour.py DIR_A DIR_B [--names A,B]: what two policies do differently outside the rounds, from their
JEVLAB_RLLOG logs (schema v2) on the same seeds: per game and by ante, rerolls, jokers bought (shop buys, swaps
included), swaps, packs bought, vouchers bought, money left when leaving the shop, blinds skipped, plus wins / ante.

    /home/user/rlvenv/bin/python scripts/rl/behaviour.py runs/_rl/bc-v77-s2/games runs/_rl/diag-v4/games --names v77,clone
"""
import argparse
import collections
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import read_rlb as R  # noqa: E402

KEYS = ["reroll", "jokers_bought", "swap", "pack", "voucher", "skip_blind", "leave_money", "shops"]


def per_game(path, names):
    g = R.read_game(path)
    fam = lambda a: names[a].split(":")[0] if a != R.UNEXPRESSIBLE else "rearrange"
    by_ante = collections.defaultdict(lambda: collections.Counter())
    tot = collections.Counter()
    for r in range(len(g["head"])):
        ante, act = int(g["head"][r][1]), int(g["head"][r][8])
        money = int(g["head"][r][5])
        f = fam(act)
        c = collections.Counter()
        if f == "reroll":
            c["reroll"] += 1
        if f in ("buy_joker", "replace_joker"):
            c["jokers_bought"] += 1
        if f == "replace_joker":
            c["swap"] += 1
        if f == "buy_pack":
            c["pack"] += 1
        if f == "buy_voucher":
            c["voucher"] += 1
        if f == "skip_blind":
            c["skip_blind"] += 1
        if f == "leave_shop":
            c["leave_money"] += money
            c["shops"] += 1
        by_ante[min(ante, 8)].update(c)
        tot.update(c)
    return g["seed"], g, tot, by_ante


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("a")
    ap.add_argument("b")
    ap.add_argument("--names", default="A,B")
    a = ap.parse_args()
    na, nb = a.names.split(",")
    import json
    names = json.load(open(os.path.join(a.b, "schema.json")))["vocab"]["names"]
    fa = {f[:-7]: os.path.join(a.a, f) for f in os.listdir(a.a) if f.endswith(".rlb.gz")}
    fb = {f[:-7]: os.path.join(a.b, f) for f in os.listdir(a.b) if f.endswith(".rlb.gz")}
    seeds = sorted(set(fa) & set(fb))
    res = {}
    for side, files in ((na, fa), (nb, fb)):
        res[side] = [per_game(files[s], names) for s in seeds]
    print(f"{len(seeds)} games on the same seeds")
    for side in (na, nb):
        gs = [x[1] for x in res[side]]
        print(f"  {side}: wins {sum(g['won'] for g in gs)}, mean ante {np.mean([g['ante'] for g in gs]):.2f}, "
              f"mean blinds passed {np.mean([g['passed_final'] for g in gs]):.2f}")
    print("\nper game (mean)            " + "".join(f"{k:>14}" for k in KEYS[:-2]) + f"{'money@leave':>14}")
    for side in (na, nb):
        t = [x[2] for x in res[side]]
        vals = [np.mean([c[k] for c in t]) for k in KEYS[:-2]]
        lm = sum(c["leave_money"] for c in t) / max(1, sum(c["shops"] for c in t))
        print(f"  {side:24s} " + "".join(f"{v:14.2f}" for v in vals) + f"{lm:14.2f}")
    print("\nby ante, per game reaching it: " + ", ".join(KEYS[:-2]) + ", money at leave_shop")
    for ante in range(1, 9):
        line = f"  ante {ante}: "
        for side in (na, nb):
            reach = [x[3][ante] for x in res[side] if x[3][ante]["shops"] > 0 or x[1]["ante"] >= ante]
            n = max(1, len(reach))
            vals = [sum(c[k] for c in reach) / n for k in KEYS[:-2]]
            lm = sum(c["leave_money"] for c in reach) / max(1, sum(c["shops"] for c in reach))
            line += f"{side} (n={len(reach)}) " + " ".join(f"{v:5.2f}" for v in vals) + f" ${lm:5.1f}   "
        print(line)


if __name__ == "__main__":
    main()
