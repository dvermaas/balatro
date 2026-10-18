"""Learning curve of a P(win) model on our own games (jevlab JEVLAB_SNAPS rows): python vcurve.py results.jsonl [more.jsonl ...]"""
import json, sys, random, collections, numpy as np
from sklearn.model_selection import GroupKFold
from sklearn.ensemble import HistGradientBoostingClassifier
from sklearn.metrics import roc_auc_score
HANDS = ["High Card", "Pair", "Two Pair", "Three of a Kind", "Straight", "Flush", "Full House", "Four of a Kind", "Straight Flush"]
games = [r for f in sys.argv[1:] for r in map(json.loads, open(f)) if "snaps" in r]
rows = []
for gm in games:
    first = {}
    for s in gm["snaps"]:
        first.setdefault(s["ante"], s)  # first shop of each ante
    for a, s in first.items():
        lv = [v[0] for v in s["levels"].values()]
        rows.append({"seed": gm["seed"], "ante": a, "won": int(gm["won"]), "money": min(s["money"], 200), "n_j": len(s["jokers"]),
                     "lv_max": max(lv), "lv_total": sum(l - 1 for l in lv), "jkeys": [j[0] for j in s["jokers"]],
                     "neg": sum(j[1] == "NEGATIVE" for j in s["jokers"]), "ed": sum(bool(j[1]) for j in s["jokers"]),
                     "deck": s["deck"], "enh": s["enh"], "hs": s["hand_size"] or 8,
                     **{f"lv:{h}": s["levels"].get(h, [1, 0])[0] for h in HANDS}, **{f"pl:{h}": s["levels"].get(h, [1, 0])[1] for h in HANDS}})
print(len(games), "games, win rate", round(np.mean([g["won"] for g in games]), 3), "| mean ante", round(np.mean([g["ante"] for g in games]), 2))
cnt = collections.Counter(k for r in rows for k in set(r["jkeys"]))
KEYS = sorted(k for k, c in cnt.items() if c >= 30)
SIMPLE = ["money", "n_j", "lv_max", "lv_total"]
RICH = SIMPLE + ["neg", "ed", "deck", "enh", "hs"] + [f"lv:{h}" for h in HANDS] + [f"pl:{h}" for h in HANDS]
def X(rs, cols):
    return np.hstack([np.array([[r[c] for c in cols] for r in rs], float), np.array([[k in r["jkeys"] for k in KEYS] for r in rs], float)])
rnd = random.Random(1)
for name, cols in (("simple", SIMPLE), ("rich", RICH)):
    print(f"\n{name} features (+{len(KEYS)} joker keys): held-out AUC by training games")
    for a in (3, 4, 5, 6, 7):
        rs = [r for r in rows if r["ante"] == a]
        Xa, y, g = X(rs, cols), np.array([r["won"] for r in rs]), np.array([r["seed"] for r in rs])
        out = []
        for frac in (0.125, 0.25, 0.5, 1.0):  # of the 80% training folds
            aucs = []
            for rep in range(1 if frac == 1.0 else 3):
                for tr, te in GroupKFold(5).split(Xa, y, g):
                    trs = sorted(set(g[tr])); keep = set(rnd.sample(trs, int(len(trs) * frac)))
                    tr2 = [i for i in tr if g[i] in keep]
                    m = HistGradientBoostingClassifier(max_iter=200, learning_rate=0.05, max_leaf_nodes=15).fit(Xa[tr2], y[tr2])
                    aucs.append(roc_auc_score(y[te], m.predict_proba(Xa[te])[:, 1]))
            out.append(f"{int(len(set(g)) * 0.8 * frac)}: {np.mean(aucs):.3f}")
        print(f"  ante {a} (games reaching it {len(set(g))}): " + "  ".join(out))
