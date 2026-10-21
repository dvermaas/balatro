"""Death rate per boss when faced: python bossrate.py results.jsonl..."""
import json, sys, collections
faced, died = collections.Counter(), collections.Counter()
games = [r for f in sys.argv[1:] for r in map(json.loads, open(f)) if "snaps" in r]
for g in games:
    bosses = {}
    for s in g["snaps"]:
        if s.get("boss"):
            bosses[s["ante"]] = s["boss"]
    d = g.get("death") or {}
    for a, b in bosses.items():
        if g["won"] or a < g["ante"] or (a == g["ante"] and d.get("type") == "BOSS"):
            faced[b] += 1
            if not g["won"] and a == g["ante"] and d.get("type") == "BOSS":
                died[b] += 1
print(len(games), "games")
for b, n in sorted(faced.items(), key=lambda kv: -died[kv[0]] / kv[1]):
    if n >= 20:
        print(f"  {b:16s} {died[b]:4d}/{n:4d} = {died[b]/n:5.1%}")
