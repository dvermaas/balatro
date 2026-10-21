import json, glob, re, collections, sys
root = sys.argv[1]
rows = []
for f in glob.glob(f"{root}/*/*/decisions.jsonl"):
    ev = [json.loads(l) for l in open(f)]
    for i, e in enumerate(ev):
        if e.get("kind") not in ("decision", "forced") or e.get("question") != "move":
            continue
        act = next((x for x in ev[i+1:i+4] if x.get("kind") == "action"), None)
        if not act or act.get("method") != "play":
            continue
        out = next((x for x in ev[i+1:i+8] if x.get("kind") == "outcome" and x.get("method") == "play"), None)
        if not out:
            continue
        if e["kind"] == "forced":
            m = re.search(r"\((\d+)\) clears", str(e.get("desc", "")))
        else:
            m = re.search(r"~(\d+)", str((e.get("options") or {}).get(e.get("choice"), "")))
        if not m:
            continue
        est = int(m.group(1)); act_ = out.get("chips_delta") or 0
        if est <= 0:
            continue
        rows.append((act_ / est, tuple(sorted(out.get("jokers") or [])), est, act_, f.split("/")[-2]))
r = [x[0] for x in rows]
print(len(rows), "plays; exact(±1%):", sum(abs(x - 1) <= 0.01 for x in r), "under(<0.99):", sum(x < 0.99 for x in r), "over(>1.01):", sum(x > 1.01 for x in r))
# per joker: share of plays with that joker that miss by >1%, and mean |log ratio|
import math
stat = collections.defaultdict(lambda: [0, 0, 0.0, 0])
for ratio, js, est, a, run in rows:
    miss = abs(ratio - 1) > 0.01
    for j in set(js):
        s = stat[j]; s[0] += 1; s[1] += miss; s[3] += ratio < 0.99
for j, (n, miss, _, under) in sorted(stat.items(), key=lambda kv: -kv[1][1] / max(kv[1][0], 1)):
    if n >= 15 and miss / n > 0.15:
        print(f"{j:22s} plays {n:5d} miss {miss/n:5.0%} (under {under})")

RANDOM = {"Misprint", "Bloodstone"}
under = [x for x in rows if x[0] < 0.97 and not (set(x[1]) & RANDOM)]
print("\nunder by >3% without Misprint/Bloodstone:", len(under))
c = collections.Counter(j for x in under for j in set(x[1]))
tot = collections.Counter(j for x in rows if not (set(x[1]) & RANDOM) for j in set(x[1]))
for j, n in c.most_common(25):
    print(f"  {j:22s} {n:4d} of {tot[j]:5d} plays ({n/tot[j]:.0%})")
for x in sorted(under)[:12]:
    print(f"  {x[0]:.2f} est {x[2]} actual {x[3]} {x[1]} {x[4]}")
