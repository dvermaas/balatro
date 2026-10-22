"""ptrain.py RESULTS.jsonl... [--out OUT.json]: does the build's measured scoring power (jevlab JEVLAB_SNAPS `power`:
log10 best-play score on 32 hands dealt from the full deck) predict P(win) beyond the value model's features?
5-fold CV by seed at the first shop of each ante: log loss and AUC for base (the value model's RICH + joker keys),
power alone (+ money, jokers, hand size), and both. --out writes the power model (power alone: the best
held-out set at antes 6-8; all games) in value_model.json's format (value_model.rs picks its features by name).

On 712 v60 games (c2k-pw): power alone AUC 0.69-0.72 at antes 6-8 vs base 0.66-0.67 (base overfits: log loss 0.62-0.72
vs 0.55-0.63); antes 2-4 ~0.5-0.6 for every set. The median alone is worse (0.61-0.67): the spread counts."""
import json, sys, collections, numpy as np
from sklearn.linear_model import LogisticRegression
from sklearn.preprocessing import StandardScaler
from sklearn.pipeline import make_pipeline
from sklearn.metrics import roc_auc_score, log_loss
from sklearn.model_selection import GroupKFold
HANDS = ["High Card", "Pair", "Two Pair", "Three of a Kind", "Straight", "Flush", "Full House", "Four of a Kind", "Straight Flush"]
args = sys.argv[1:]; out_path = None
if "--out" in args: i = args.index("--out"); out_path = args[i + 1]; args = args[:i] + args[i + 2:]
def rows_of(files):
    rows = []
    for f in files:
        for gm in map(json.loads, open(f)):
            if "snaps" not in gm or "won" not in gm: continue
            first = {}
            for s in gm["snaps"]: first.setdefault(s["ante"], s)
            for a, s in first.items():
                if not s.get("power"): continue
                lv = [v[0] for v in s["levels"].values()]; p = s["power"]
                rows.append({"seed": gm["seed"], "ante": a, "won": int(gm["won"]), "money": min(s["money"], 200), "n_j": len(s["jokers"]),
                    "lv_max": max(lv), "lv_total": sum(l - 1 for l in lv), "jkeys": [j[0] for j in s["jokers"]],
                    "neg": sum(j[1] == "NEGATIVE" for j in s["jokers"]), "ed": sum(bool(j[1]) for j in s["jokers"]),
                    "deck": s["deck"], "enh": s["enh"], "hs": s["hand_size"] or 8,
                    "p25": p[len(p) // 4], "p50": p[len(p) // 2], "p75": p[3 * len(p) // 4], "pmean": float(np.mean(p)),
                    **{f"lv:{h}": s["levels"].get(h, [1, 0])[0] for h in HANDS}, **{f"pl:{h}": s["levels"].get(h, [1, 0])[1] for h in HANDS}})
    return rows
rows = rows_of(args)
RICH = ["money", "n_j", "lv_max", "lv_total", "neg", "ed", "deck", "enh", "hs"] + [f"lv:{h}" for h in HANDS] + [f"pl:{h}" for h in HANDS]
POW = ["p25", "p50", "p75", "pmean"]
cnt = collections.Counter(k for r in rows for k in set(r["jkeys"]))
KEYS = sorted(k for k, c in cnt.items() if c >= 30)
SETS = {"base": (RICH, KEYS), "power": (POW + ["money", "n_j", "hs"], []), "power+base": (POW + RICH, KEYS)}
def X(rs, cols, keys): return np.hstack([np.array([[r[c] for c in cols] for r in rs], float), np.array([[k in r["jkeys"] for k in keys] for r in rs], float).reshape(len(rs), len(keys))])
def fit(Xa, y): return make_pipeline(StandardScaler(), LogisticRegression(C=0.05, max_iter=4000)).fit(Xa, y)
print(f"{len(set(r['seed'] for r in rows))} games")
for a in range(2, 9):
    rs = [r for r in rows if r["ante"] == a]; y = np.array([r["won"] for r in rs]); g = [r["seed"] for r in rs]
    line = f"ante {a}: n {len(rs)} win {y.mean():.3f}"
    for name, (cols, keys) in SETS.items():
        Xa = X(rs, cols, keys); p = np.zeros(len(rs))
        for tr, te in GroupKFold(5).split(Xa, y, g): p[te] = fit(Xa[tr], y[tr]).predict_proba(Xa[te])[:, 1]
        line += f" | {name} ll {log_loss(y, p):.4f} AUC {roc_auc_score(y, p):.3f}"
    print(line)
if out_path:
    cols, keys = SETS["power"]
    out = {"features": cols, "keys": keys, "hands": HANDS, "power": POW, "trained_on": f"{len(set(r['seed'] for r in rows))} lab games, first shop of each ante", "models": {}}
    for a in range(2, 9):
        rs = [r for r in rows if r["ante"] == a]; y = np.array([r["won"] for r in rs])
        m = fit(X(rs, cols, keys), y); sc, lr = m[0], m[-1]
        out["models"][str(a)] = {"n": len(rs), "win_rate": float(y.mean()), "mean": sc.mean_.tolist(), "scale": sc.scale_.tolist(),
                                 "coef": lr.coef_[0].tolist(), "intercept": float(lr.intercept_[0])}
    json.dump(out, open(out_path, "w"))
