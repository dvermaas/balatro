"""vmeval.py MODEL.json RESULTS... : per-ante weighted log loss / AUC of a value_model.json-format model (base features)."""
import json, sys, collections, numpy as np
from sklearn.metrics import roc_auc_score, log_loss
HANDS = ["High Card", "Pair", "Two Pair", "Three of a Kind", "Straight", "Flush", "Full House", "Four of a Kind", "Straight Flush"]
M = json.load(open(sys.argv[1])); rows = []
for f in sys.argv[2:]:
    for gm in map(json.loads, open(f)):
        if "snaps" not in gm or "won" not in gm: continue
        first = {}
        for s in gm["snaps"]: first.setdefault(s["ante"], s)
        for a, s in first.items():
            if not s.get("power"): continue
            lv = [v[0] for v in s["levels"].values()]
            r = {"seed": gm["seed"], "ante": a, "won": int(gm["won"]), "money": min(s["money"], 200), "n_j": len(s["jokers"]),
                 "lv_max": max(lv), "lv_total": sum(l - 1 for l in lv), "jkeys": [j[0] for j in s["jokers"]],
                 "neg": sum(j[1] == "NEGATIVE" for j in s["jokers"]), "ed": sum(bool(j[1]) for j in s["jokers"]),
                 "deck": s["deck"], "enh": s["enh"], "hs": s["hand_size"] or 8,
                 **{f"lv:{h}": s["levels"].get(h, [1, 0])[0] for h in HANDS}, **{f"pl:{h}": s["levels"].get(h, [1, 0])[1] for h in HANDS}}
            rows.append(r)
for a in range(2, 9):
    m = M["models"].get(str(a))
    if not m: continue
    rs = [r for r in rows if r["ante"] == a]; c = collections.Counter(r["seed"] for r in rs); w = np.array([1 / c[r["seed"]] for r in rs])
    X = np.array([[r[f] for f in M["features"]] + [k in r["jkeys"] for k in M["keys"]] for r in rs], float)
    z = ((X - np.array(m["mean"])) / np.array(m["scale"])) @ np.array(m["coef"]) + m["intercept"]; p = 1 / (1 + np.exp(-z))
    y = np.array([r["won"] for r in rs]); base = np.average(y, weights=w)
    print(f"ante {a}: n {len(rs)} win {base:.3f} const wll {log_loss(y, np.full(len(y), base), sample_weight=w):.4f} model wll {log_loss(y, p, sample_weight=w):.4f} AUC {roc_auc_score(y, p, sample_weight=w):.3f} mean p {np.average(p, weights=w):.3f}")
