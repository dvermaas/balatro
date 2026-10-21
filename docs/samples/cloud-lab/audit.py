"""Predicted vs actual score of every play: python audit.py <run dirs root>"""
import json, glob, re, sys, math, collections
rows = []
for f in glob.glob(sys.argv[1] + "/*/*/decisions.jsonl"):
    ev = [json.loads(l) for l in open(f)]
    seed = f.split("-")[-1].split("/")[0]
    blind, est, hand = None, None, None
    for e in ev:
        ss = e.get("state_sent") or {}
        if isinstance(ss, dict) and isinstance(ss.get("round"), dict) and ss["round"].get("blind"):
            blind = ss["round"]["blind"]
        if e["kind"] == "decision" and e.get("question") == "move" and str(e.get("choice", "")).startswith("play"):
            o = (e.get("options") or {}).get(e["choice"], "")
            m = re.search(r"~(\d+)", o)
            est, hand = (int(m.group(1)), o.split(":")[0]) if m else (None, None)
        if e["kind"] == "forced" and e.get("question") == "move" and "clears the blind" in str(e.get("desc")):
            m = re.match(r"(.+?) \((\d+)\) clears", e["desc"])
            est, hand = (int(m.group(2)), m.group(1)) if m else (None, None)
        if e["kind"] == "outcome" and e.get("method") == "play":
            if est and e.get("chips_delta") is not None:
                rows.append({"seed": seed, "blind": blind, "est": est, "act": e["chips_delta"], "hand": hand, "jokers": e.get("jokers") or []})
            est = None
print(len(rows), "plays with an estimate")
bad = [r for r in rows if r["act"] < r["est"] / 1.5]
over = [r for r in rows if r["act"] > r["est"] * 1.5]
print(f"actual < estimate/1.5: {len(bad)} ({100*len(bad)/len(rows):.1f}%), actual > 1.5x estimate: {len(over)} ({100*len(over)/len(rows):.1f}%)")
def share(rs, key):
    c = collections.Counter(key(r) for r in rs); t = collections.Counter(key(r) for r in rows)
    return sorted(((k, c[k], t[k], c[k] / t[k]) for k in c if t[k] >= 8), key=lambda x: -x[3])
print("\nby blind (overestimated plays / plays with that blind):")
for k, n, t, s in share(bad, lambda r: r["blind"])[:12]: print(f"  {k}: {n}/{t} = {s:.0%}")
jc = collections.Counter(j for r in bad for j in r["jokers"]); jt = collections.Counter(j for r in rows for j in r["jokers"])
print("\nby joker held (overestimated / plays holding it, >= 30 plays):")
for j, s in sorted(((j, jc[j] / jt[j]) for j in jc if jt[j] >= 30), key=lambda x: -x[1])[:15]: print(f"  {j}: {jc[j]}/{jt[j]} = {s:.0%}")
print("\nunderestimated by joker (actual > 1.5x):")
oc = collections.Counter(j for r in over for j in r["jokers"])
for j, s in sorted(((j, oc[j] / jt[j]) for j in oc if jt[j] >= 30), key=lambda x: -x[1])[:10]: print(f"  {j}: {oc[j]}/{jt[j]} = {s:.0%}")
json.dump(rows, open(sys.argv[1].rstrip("/") + "_audit.json", "w"))
