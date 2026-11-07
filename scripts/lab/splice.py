"""splice.py BASE PATCH NAME TAG: runs/_policy_lab/NAME = BASE's rows with PATCH's rows in place of the same seeds
(a boss-subset run, subset.py), tagged TAG in NAME/.chunk (e.g. "jevlab-v68 "), so chunk.sh can extend it."""
import json, os, sys

L = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "runs", "_policy_lab")
base, patch, name, tag = sys.argv[1:5]
rd = lambda n: [l if l.endswith("\n") else l + "\n" for l in open(os.path.join(L, n, "results.jsonl")) if l.strip()]
p = {json.loads(l)["seed"]: l for l in rd(patch)}
out = [p.get(json.loads(l)["seed"], l) for l in rd(base)]
os.makedirs(os.path.join(L, name), exist_ok=True)
open(os.path.join(L, name, "results.jsonl"), "w").write("".join(out))
open(os.path.join(L, name, ".chunk"), "w").write(tag + "\n")
print(name, len(out), sum(json.loads(l)["won"] for l in out), "wins")
