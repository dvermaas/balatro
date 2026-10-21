"""cmpbase.py BASE.jsonl[.gz] RUN.jsonl...: paired comparison of lab runs by seed (mean ante, wins won by one side)."""
import json, gzip, statistics as st, sys
def load(p): return {r["seed"]: r for r in map(json.loads, (gzip.open(p, "rt") if p.endswith(".gz") else open(p))) if "ante" in r}
b = load(sys.argv[1])
for p in sys.argv[2:]:
    a = load(p); ks = [k for k in a if k in b]; d = [a[k]["ante"] - b[k]["ante"] for k in ks]
    oa = sum(a[k]["won"] and not b[k]["won"] for k in ks); ob = sum(b[k]["won"] and not a[k]["won"] for k in ks)
    ch = sum((a[k]["ante"], a[k]["round"]) != (b[k]["ante"], b[k]["round"]) for k in ks)
    print(f"{p}: n={len(ks)} d_ante={st.mean(d):+.3f}+-{st.stdev(d)/len(d)**.5:.3f} wins {sum(a[k]['won'] for k in ks)} vs {sum(b[k]['won'] for k in ks)} "
          f"onlywins {oa}/{ob} z={(oa-ob)/max(1,oa+ob)**.5:+.2f} changed {ch}")
