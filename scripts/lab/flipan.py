"""flipan.py [GLOB]: paired-decision analysis (flipq.sh data). Each flip-SET-sS row is one game whose flipped shop
decision is recorded in row["flip"]; its partner is the same seed in SET-v77. Prints, per item kind, direction
(buy->skip, skip->buy) and ante, the forecast's predicted gain of the alternative (fc_delta, blinds over the horizon)
against the realised change in blinds cleared and wins (flipped minus baseline)."""
import json, glob, os, sys, collections, math
L = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "runs", "_policy_lab")
pat = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith("--") else "flip-*-s*"
base = {}
def rows(n):
    return [json.loads(l) for l in open(os.path.join(L, n, "results.jsonl")) if l.strip()]
out = []
for d in sorted(glob.glob(os.path.join(L, pat))):
    n = os.path.basename(d)
    if n.endswith("-part") or not os.path.isfile(os.path.join(d, "results.jsonl")): continue
    s = n.split("-")[1]
    if s not in base: base[s] = {r["seed"]: r for r in rows(f"{s}-v77")}
    for r in rows(n):
        f = r.get("flip"); b = base[s].get(r["seed"])
        if not f or not b: continue
        items = {it["key"]: it for it in f["items"]}
        if f["was_buy"]:
            it = items.get(f["chosen"]); pred = -(it["fc_delta"] or 0) if it and it["fc_delta"] is not None else None
        else:
            it = items.get(f["alt"]); pred = it["fc_delta"] if it else None
        out.append({"set": s, "seed": r["seed"], "dir": "buy->skip" if f["was_buy"] else "skip->buy", "kind": (it or {}).get("kind") or "?",
                    "ante": f["ante"], "money": f["money"], "pred": pred, "se": (it or {}).get("fc_se"),
                    "db": r["blinds"] - b["blinds"], "dw": int(r["won"]) - int(b["won"]), "desc": (it or {}).get("desc", "")})
print(f"{len(out)} paired flips")
def summ(name, xs):
    if not xs: return
    n = len(xs); db = [x["db"] for x in xs]; dw = [x["dw"] for x in xs]; pr = [x["pred"] for x in xs if x["pred"] is not None]
    m = sum(db) / n; sd = math.sqrt(sum((v - m) ** 2 for v in db) / max(n - 1, 1)) / math.sqrt(n)
    print(f"{name:<44} n {n:5d}  pred {sum(pr)/max(len(pr),1):+.3f}  real blinds {m:+.3f} +- {sd:.3f}  wins {sum(dw):+d} ({sum(v>0 for v in dw)}/{sum(v<0 for v in dw)})")
summ("all", out)
for k in ["buy->skip", "skip->buy"]:
    summ(k, [x for x in out if x["dir"] == k])
def kname(k):
    k = k.lower()
    for w in ["joker", "pack", "planet", "tarot", "voucher", "spectral"]:
        if w in k: return w
    return k[:12]
for dr in ["buy->skip", "skip->buy"]:
    g = collections.defaultdict(list)
    for x in out:
        if x["dir"] == dr: g[kname(x["kind"])].append(x)
    for k in sorted(g): summ(f"{dr} {k}", g[k])
for dr in ["buy->skip", "skip->buy"]:
    g = collections.defaultdict(list)
    for x in out:
        if x["dir"] == dr: g[x["ante"]].append(x)
    for a in sorted(g): summ(f"{dr} ante {a}", g[a])
# calibration: realised vs predicted, binned by predicted gain
bins = [(-9, -1), (-1, -0.5), (-0.5, -0.2), (-0.2, 0), (0, 0.2), (0.2, 0.5), (0.5, 1), (1, 9)]
for lo, hi in bins:
    summ(f"pred in [{lo}, {hi})", [x for x in out if x["pred"] is not None and lo <= x["pred"] < hi])
if "--dump" in sys.argv:
    json.dump(out, open(sys.argv[sys.argv.index("--dump") + 1], "w"))
