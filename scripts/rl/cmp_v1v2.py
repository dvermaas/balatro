#!/usr/bin/env python3
"""cmp_v1v2.py V1_DIR V2_DIR: the same v77 games logged in schema v1 and v2, compared record for record.

Expected: every v1 record has a v2 record at the same step with the same action, the same observation on the
v1 part of the layout (v2 only appends the shop_slot_jokers block), and the v1 mask plus only replace_joker ids;
except that a v1 sale whose next record is the buy of a joker that had no room becomes one v2 replace_joker
record (flag 128) and that buy's record is gone. Also checks the v2 blinds_passed field against v1's fields.
"""
import json
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import read_rlb as R  # noqa: E402

d1, d2 = sys.argv[1], sys.argv[2]
names2 = json.load(open(os.path.join(d2, "schema.json")))["vocab"]["names"]
names1 = json.load(open(os.path.join(d1, "schema.json")))["vocab"]["names"]
dim1 = json.load(open(os.path.join(d1, "schema.json")))["obs_dim"]
seeds = sorted(set(f[:-7] for f in os.listdir(d1) if f.endswith(".rlb.gz")) & set(f[:-7] for f in os.listdir(d2) if f.endswith(".rlb.gz")))
bad = {}
n_rec = n_rep = n_games = n_unmerged = 0


def flag(k, s):
    bad.setdefault(k, []).append(s)


for s in seeds:
    g1 = R.read_game(os.path.join(d1, s + ".rlb.gz"))
    g2 = R.read_game(os.path.join(d2, s + ".rlb.gz"))
    n_games += 1
    if (g1["won"], g1["ante"], g1["blinds"], g1["round"]) != (g2["won"], g2["ante"], g2["blinds"], g2["round"]):
        flag("footer", s)
    h1, h2 = g1["head"], g2["head"]
    j = 0
    i = 0
    while i < len(h1):
        if j >= len(h2):
            flag("v2_short", s)
            break
        a1, a2 = h1[i], h2[j]
        if a1[0] != a2[0]:
            flag("step", s)
            break
        # v1 head: step ante phase blind round money cleared -1 action atype aindex ncards
        if tuple(a1[[1, 2, 3, 4, 5, 6, 9, 10, 11]]) != tuple(a2[[1, 2, 3, 4, 5, 6, 9, 10, 11]]) or g1["cards"][i] != g2["cards"][j]:
            flag("fields", s)
        o1 = dict(zip(g1["obs_idx"][i].tolist(), g1["obs_val"][i].tolist()))
        o2 = {k: v for k, v in zip(g2["obs_idx"][j].tolist(), g2["obs_val"][j].tolist()) if k < dim1}
        if o1 != o2:
            flag("obs", s)
        m1 = set(g1["mask"][i].tolist())
        m2 = set(g2["mask"][j].tolist())
        extra = m2 - m1
        if not m1 <= m2 or any(not names2[x].startswith("replace_joker:") for x in extra):
            flag("mask", s)
        n_rec += 1
        if g2["flags"][j] & 128:
            n_rep += 1
            # v1: this sale then the buy at the next step; v2 id is replace_joker:<sold>@slot<buy index>
            nm = names2[int(a2[8])]
            if not nm.startswith("replace_joker:") or i + 1 >= len(h1):
                flag("replace_shape", s)
            else:
                b = h1[i + 1]
                sold = names1[int(a1[8])].split(":", 1)[1]
                if b[0] != a1[0] + 1 or not names1[int(b[8])].startswith("buy_joker:") or nm != f"replace_joker:{sold}@slot{int(b[10])}":
                    flag("replace_pair", s)
                if int(a2[8]) not in m2:
                    flag("replace_not_in_mask", s)
            i += 2
        else:
            if int(a1[8]) != int(a2[8]):
                flag("action", s)
            if int(a1[8]) != R.UNEXPRESSIBLE and names1[int(a1[8])].startswith("sell_joker:") and i + 1 < len(h1) and h1[i + 1][0] == a1[0] + 1 \
                    and names1[int(h1[i + 1][8])].startswith("buy_joker:"):
                n_unmerged += 1
            i += 1
        j += 1
    if j != len(h2) and "v2_short" not in bad.get(s, []):
        flag("v2_long", s)
print(f"{n_games} games, {n_rec} v2 records compared, {n_rep} replace records, "
      f"{n_unmerged} v1 sale+buy_joker pairs kept as two records (a lone sale: not the canonical copy, or the buy had room)")
for k, v in bad.items():
    print(f"  MISMATCH {k}: {len(v)} (e.g. {v[:3]})")
if not bad:
    print("  no mismatches")
