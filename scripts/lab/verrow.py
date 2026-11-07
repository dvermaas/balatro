"""verrow.py RUN VERSION CHANGE [SEED_SET]: append a version row for a complete 2,000-seed run to
docs/samples/cloud-versions.json. SEED_SET: "fresh 2000" (b2k, the default) or "fresh 2000 (2nd)" (c2k)."""
import json, os, statistics as st, sys

R = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
run, ver, change = sys.argv[1:4]
sset = sys.argv[4] if len(sys.argv) > 4 else "fresh 2000"
rows = [json.loads(l) for l in open(os.path.join(R, "runs", "_policy_lab", run, "results.jsonl")) if l.strip()]
a = [r["ante"] for r in rows]
lost = [r for r in rows if not r["won"]]
row = {"seed_set": sset, "version": ver, "version_id": "rust", "change": change, "games": len(rows),
       "mean_ante": round(st.mean(a), 3), "se": round(st.stdev(a) / len(a) ** .5, 3), "wins": sum(r["won"] for r in rows),
       "win_rate": round(sum(r["won"] for r in rows) / len(rows), 3), "mean_round": round(st.mean(r["round"] for r in rows), 2),
       "died_ante_1_4": sum(r["ante"] <= 4 for r in lost), "died_ante_5_7": sum(5 <= r["ante"] <= 7 for r in lost),
       "died_ante_8": sum(r["ante"] >= 8 for r in lost)}
p = os.path.join(R, "docs", "samples", "cloud-versions.json")
d = json.load(open(p))
d.append(row)
json.dump(d, open(p, "w"), indent=1)
open(p, "a").write("\n")
print(row)
