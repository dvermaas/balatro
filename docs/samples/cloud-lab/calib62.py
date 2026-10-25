"""calib62.py ROOT: forecast P(clear next boss) and median boss margin for the chosen shop option vs what happened,
by ante and by blinds left before the boss (2 = shop after the previous boss, 1 = after Small, 0 = after Big)."""
import json, glob, re, sys, collections, statistics as st
rows = []
for f in glob.glob(sys.argv[1] + "/*/*/decisions.jsonl"):
    ev = [json.loads(l) for l in open(f)]
    for i, e in enumerate(ev):
        fc = e.get("forecast")
        if not isinstance(fc, dict) or "next_boss_clear" not in fc: continue
        m = re.match(r"(\d+)%", fc["next_boss_clear"])
        if not m: continue
        p = int(m.group(1)) / 100
        mm = re.match(r"median ([\d.]+)x", fc.get("boss_margin", ""))
        pm = float(mm.group(1)) if mm else None
        last = None; ante = None
        for b in ev[:i][::-1]:
            if b.get("kind") == "round_end": last = b; break
        left = 2 if last is None or last["blind"] not in ("Small Blind", "Big Blind") else (1 if last["blind"] == "Small Blind" else 0)
        out = None; am = None; died_before = False; boss = None
        for b in ev[i + 1:]:
            if b.get("kind") == "round_end":
                if b.get("blind") not in ("Small Blind", "Big Blind"):
                    am = float(b.get("scored") or 0) / float(b.get("target") or 1); out = int(am >= 1); boss = b.get("blind"); break
                if float(b.get("scored") or 0) < float(b.get("target") or 1): died_before = True
            if b.get("kind") == "run_end":
                if out is None: out = 1 if str(b.get("won")) == "True" else 0
                break
        if out is None: continue
        ante = (last or {}).get("ante", 1)  # a boss round_end already reports the next ante
        rows.append(dict(p=p, out=out, ante=ante, left=left, pm=pm, am=am, kind=e.get("kind"), q=e.get("question"), boss=boss))
print(len(rows), "forecasts;", collections.Counter(r["q"] for r in rows).most_common(5))
def show(name, key):
    by = collections.defaultdict(list)
    for r in rows: by[key(r)].append(r)
    print(name)
    for k in sorted(by):
        v = by[k]; pm = [r["pm"] / r["am"] for r in v if r["pm"] and r["am"]]
        print(f"  {k}: n={len(v):5d} predicted {100*st.mean(r['p'] for r in v):5.1f}% actual {100*st.mean(r['out'] for r in v):5.1f}%"
              + (f"   median margin predicted/actual {st.median(pm):.2f} (n {len(pm)})" if pm else ""))
show("by ante", lambda r: r["ante"])
show("by blinds left before the boss", lambda r: r["left"])
show("ante x left", lambda r: (r["ante"], r["left"]))
bins = collections.defaultdict(list)
for r in rows: bins[min(int(r["p"] * 10), 9)].append(r["out"])
print("reliability")
for k in sorted(bins): print(f"  predicted {k*10:3d}-{k*10+9:3d}%: n={len(bins[k]):5d} actual {100*st.mean(bins[k]):5.1f}%")

import os
if os.environ.get("BYBOSS"):
    by = collections.defaultdict(list)
    for r in rows:
        if r["left"] == 0 and r["boss"]: by[r["boss"]].append(r)
    print("last shop before the boss, by boss")
    for k, v in sorted(by.items(), key=lambda x: -len(x[1])):
        if len(v) < 25: continue
        pm = [r["pm"] / r["am"] for r in v if r["pm"] and r["am"]]
        print(f"  {k:15s} n={len(v):4d} predicted {100*st.mean(r['p'] for r in v):5.1f}% actual {100*st.mean(r['out'] for r in v):5.1f}%  margin ratio {st.median(pm):.2f}")
