"""vtrain.py OUT.json TEST.jsonl TRAIN.jsonl...: per-ante logistic P(win) on first-shop snapshots; prints held-out
log loss and AUC on TEST for the new model and for native/bot/data/value_model.json."""
import json, sys, collections, numpy as np
from sklearn.linear_model import LogisticRegression
from sklearn.preprocessing import StandardScaler
from sklearn.pipeline import make_pipeline
from sklearn.metrics import roc_auc_score, log_loss
HANDS = ["High Card", "Pair", "Two Pair", "Three of a Kind", "Straight", "Flush", "Full House", "Four of a Kind", "Straight Flush"]
def rows_of(files):
    rows = []
    for f in files:
        for gm in map(json.loads, open(f)):
            if "snaps" not in gm or "won" not in gm: continue
            first = {}
            for s in gm["snaps"]: first.setdefault(s["ante"], s)
            for a, s in first.items():
                lv = [v[0] for v in s["levels"].values()]
                rows.append({"seed": gm["seed"], "ante": a, "won": int(gm["won"]), "money": min(s["money"], 200), "n_j": len(s["jokers"]),
                    "lv_max": max(lv), "lv_total": sum(l - 1 for l in lv), "jkeys": [j[0] for j in s["jokers"]],
                    "neg": sum(j[1] == "NEGATIVE" for j in s["jokers"]), "ed": sum(bool(j[1]) for j in s["jokers"]),
                    "deck": s["deck"], "enh": s["enh"], "hs": s["hand_size"] or 8,
                    **{f"lv:{h}": s["levels"].get(h, [1, 0])[0] for h in HANDS}, **{f"pl:{h}": s["levels"].get(h, [1, 0])[1] for h in HANDS}})
    return rows
out_path, test_f, train_fs = sys.argv[1], sys.argv[2], sys.argv[3:]
tr, te = rows_of(train_fs), rows_of([test_f])
RICH = ["money", "n_j", "lv_max", "lv_total", "neg", "ed", "deck", "enh", "hs"] + [f"lv:{h}" for h in HANDS] + [f"pl:{h}" for h in HANDS]
cnt = collections.Counter(k for r in tr for k in set(r["jkeys"]))
KEYS = sorted(k for k, c in cnt.items() if c >= 30)
def X(rs, keys): return np.hstack([np.array([[r[c] for c in RICH] for r in rs], float), np.array([[k in r["jkeys"] for k in keys] for r in rs], float)])
old = json.load(open("/home/user/balatro/native/bot/data/value_model.json"))
def pred(model, rs, a):
    md = model["models"][str(a)]; xs = X(rs, model["keys"])
    z = ((xs - np.array(md["mean"])) / np.array(md["scale"])) @ np.array(md["coef"]) + md["intercept"]
    return 1 / (1 + np.exp(-z))
out = {"features": RICH, "keys": KEYS, "hands": HANDS, "trained_on": f"{len(set(r['seed'] for r in tr))} seeds of v52-v54 lab games ({len(train_fs)} runs, arm F), first shop of each ante", "models": {}}
for a in range(2, 9):
    rs = [r for r in tr if r["ante"] == a]
    Xa, y = X(rs, KEYS), np.array([r["won"] for r in rs])
    m = make_pipeline(StandardScaler(), LogisticRegression(C=0.05, max_iter=4000)).fit(Xa, y)
    sc, lr = m[0], m[-1]
    out["models"][str(a)] = {"n": len(rs), "win_rate": float(y.mean()), "mean": sc.mean_.tolist(), "scale": sc.scale_.tolist(),
                             "coef": lr.coef_[0].tolist(), "intercept": float(lr.intercept_[0])}
    ts = [r for r in te if r["ante"] == a]; yt = np.array([r["won"] for r in ts])
    pn, po = pred(out, ts, a), pred(old, ts, a)
    print(f"ante {a}: train {len(rs)} test {len(ts)} win {yt.mean():.3f} | new logloss {log_loss(yt, pn):.4f} AUC {roc_auc_score(yt, pn):.3f} | old logloss {log_loss(yt, po):.4f} AUC {roc_auc_score(yt, po):.3f} mean p {po.mean():.3f}")
json.dump(out, open(out_path, "w"))
