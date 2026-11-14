#!/usr/bin/env python3
"""offers.py NAME=DIR [NAME=DIR ...] [--ref NAME]: what policies do with the shop offers at antes 5-8, from JEVLAB_RLLOG logs
(schema v2; milestone 6 part 4).

  - rerolls and what follows: jokers bought / swaps taken before the next reroll or leaving, per reroll
  - the reference set D: the jokers the --ref policy acquired (bought or swapped for) most at antes 5-8 (top 30)
  - per shop visit at antes 5-8 with a D joker on offer: how often the policy acquired a D joker in that visit
  - swaps at antes 5-8: which jokers it sells most and which it buys
"""
import argparse
import collections
import json
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import read_rlb as R  # noqa: E402


def analyse(d):
    ds = R.load_dir(d)
    names = ds["schema"]["vocab"]["names"]
    blocks = {b["name"]: b for b in ds["schema"]["blocks"]}
    ss = blocks["shop_slot_jokers"]
    joker_keys = ss["cols"]
    n = len(ds["action"])
    act = ds["action"]
    fam = np.array([names[a].split(":")[0] if a != R.UNEXPRESSIBLE else "rearrange" for a in act])

    def slots(r):
        idx = ds["obs_indices"][ds["obs_indptr"][r]:ds["obs_indptr"][r + 1]]
        out = {}
        for i in idx:
            if ss["offset"] <= i < ss["offset"] + len(ss["rows"]) * len(ss["cols"]):
                k = i - ss["offset"]
                out[k // len(ss["cols"])] = joker_keys[k % len(ss["cols"])]
        return out

    acq = collections.Counter()  # (key) acquired at antes 5-8
    sold = collections.Counter()
    bought_by_swap = collections.Counter()
    visits = []  # (offered keys set, acquired keys set) per shop visit at antes 5-8
    rr = [0, 0, 0]  # rerolls, joker buys after, swaps after
    cur = None
    after_reroll = False
    for r in range(n):
        late = 5 <= ds["ante"][r] <= 8
        if ds["phase"][r] != 3:  # not the shop
            if cur is not None and fam[r] != "pick_pack_joker":
                pass
            continue
        key = (ds["game"][r], ds["round"][r])
        if cur is None or cur[0] != key:
            if cur is not None and cur[3]:
                visits.append((cur[1], cur[2]))
            cur = [key, set(), set(), late]
            after_reroll = False
        sl = slots(r)
        if late:
            cur[1].update(sl.values())
        f = fam[r]
        nm = names[act[r]] if act[r] != R.UNEXPRESSIBLE else ""
        got = None
        if f == "buy_joker":
            got = nm.split(":", 1)[1]
        elif f == "replace_joker":
            s_key, slot = nm.split(":", 1)[1].split("@slot")
            got = sl.get(int(slot))
            if late:
                sold[s_key] += 1
                if got:
                    bought_by_swap[got] += 1
        if got and late:
            acq[got] += 1
            cur[2].add(got)
        if late:
            if f == "reroll":
                rr[0] += 1
                after_reroll = True
            elif f == "leave_shop":
                after_reroll = False
            elif after_reroll and f == "buy_joker":
                rr[1] += 1
            elif after_reroll and f == "replace_joker":
                rr[2] += 1
    if cur is not None and cur[3]:
        visits.append((cur[1], cur[2]))
    return dict(games=len(ds["game_seed"]), acq=acq, sold=sold, bought_by_swap=bought_by_swap, visits=visits, rr=rr)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dirs", nargs="+")
    ap.add_argument("--ref", required=True)
    ap.add_argument("--top", type=int, default=30)
    a = ap.parse_args()
    res = {}
    for x in a.dirs:
        nm, d = x.split("=", 1)
        res[nm] = analyse(d)
    D = set(k for k, _ in res[a.ref]["acq"].most_common(a.top))
    print(f"D = the {len(D)} jokers {a.ref} acquired most at antes 5-8")
    for nm, r in res.items():
        rr = r["rr"]
        vis = [v for v in r["visits"] if v[0] & D]
        took = sum(1 for off, got in vis if off & got & D)
        any_got = sum(1 for off, got in r["visits"] if got)
        print(f"\n{nm} ({r['games']} games), antes 5-8:")
        print(f"  rerolls {rr[0]} ({rr[0] / r['games']:.2f}/game): joker buys after a reroll {rr[1] / max(1, rr[0]):.3f}, swaps {rr[2] / max(1, rr[0]):.3f} per reroll")
        print(f"  shop visits {len(r['visits'])}, with a D joker on offer {len(vis)}: took one in {took / max(1, len(vis)):.3f} of them; "
              f"any joker acquired in {any_got / max(1, len(r['visits'])):.3f} of all visits")
        tot_s = sum(r["sold"].values())
        print(f"  swaps {tot_s} ({tot_s / r['games']:.2f}/game); sells most: " + ", ".join(f"{k[2:]} {v / tot_s:.1%}" for k, v in r["sold"].most_common(8)))
        tb = sum(r["bought_by_swap"].values())
        print(f"     swaps in most: " + ", ".join(f"{k[2:]} {v / max(1, tb):.1%}" for k, v in r["bought_by_swap"].most_common(8)))
        ta = sum(r["acq"].values())
        print(f"  acquires most: " + ", ".join(f"{k[2:]} {v / ta:.1%}" for k, v in r["acq"].most_common(10)))


if __name__ == "__main__":
    main()
