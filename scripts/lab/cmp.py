"""cmp.py BASE NAME: NAME vs BASE (runs/_policy_lab) on the seeds both hold.

Prints the paired mean-ante difference with its SE, wins, and the games won by only one side with
z = (a - b) / sqrt(a + b), the statistic every lab decision uses (docs/lab-lessons.md, method lessons).
"""
import json, math, os, sys

L = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "runs", "_policy_lab")


def load(n):
    rows = (json.loads(l) for l in open(os.path.join(L, n, "results.jsonl")) if l.strip())
    return {r["seed"]: r for r in rows if "won" in r}


b, a = load(sys.argv[1]), load(sys.argv[2])
s = [k for k in a if k in b]
d = [a[k]["ante"] - b[k]["ante"] for k in s]
n = len(d)
m = sum(d) / n if n else 0.0
se = math.sqrt(sum((x - m) ** 2 for x in d) / (n - 1) / n) if n > 1 else 0.0
wa, wb = sum(a[k]["won"] for k in s), sum(b[k]["won"] for k in s)
oa = sum(a[k]["won"] and not b[k]["won"] for k in s)
ob = sum(b[k]["won"] and not a[k]["won"] for k in s)
z = (oa - ob) / math.sqrt(oa + ob) if oa + ob else 0.0
print(f"{sys.argv[2]} vs {sys.argv[1]}: n={n} ante {m:+.3f} +- {se:.3f} wins {wa} vs {wb} onlywins {oa}/{ob} z={z:+.2f}")
