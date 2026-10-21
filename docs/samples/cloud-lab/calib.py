"""calib54.py ROOT: forecast P(clear next boss) for the chosen option vs whether it was cleared."""
import json, glob, re, sys, collections
rows = []
for f in glob.glob(sys.argv[1] + "/*/*/decisions.jsonl"):
    ev = [json.loads(l) for l in open(f)]
    for i, e in enumerate(ev):
        fc = e.get("forecast")
        if not isinstance(fc, dict) or "next_boss_clear" not in fc:
            continue
        m = re.match(r"(\d+)%", fc["next_boss_clear"])
        if not m:
            continue
        p = int(m.group(1)) / 100
        ante = None
        for b in ev[:i][::-1]:
            if b.get("ante"):
                ante = int(b["ante"]); break
        out = None
        for b in ev[i + 1:]:
            if b.get("kind") == "round_end" and b.get("blind") not in ("Small Blind", "Big Blind"):
                out = int(float(b.get("scored") or 0) >= float(b.get("target") or 1)); break
            if b.get("kind") == "run_end":
                out = 1 if str(b.get("won")) == "True" else 0; break
        if out is not None:
            rows.append((p, out, ante or 0))
print(len(rows), "forecasts")
bins = collections.defaultdict(list)
for p, o, a in rows:
    bins[min(int(p * 10), 9)].append(o)
for k in sorted(bins):
    v = bins[k]
    print(f"  predicted {k*10:3d}-{k*10+9:3d}%: n={len(v):5d} actual {100*sum(v)/len(v):5.1f}%")
by = collections.defaultdict(list)
for p, o, a in rows:
    by[a].append((p, o))
for a in sorted(by):
    v = by[a]
    print(f"  ante {a}: n={len(v):4d} mean predicted {100*sum(p for p,_ in v)/len(v):5.1f}% actual {100*sum(o for _,o in v)/len(v):5.1f}%")
