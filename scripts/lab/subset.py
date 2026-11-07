"""subset.py RUN SET NAME BOSS [BOSS ...]: write the seeds of RUN (a complete run of seed set SET, b2k or c2k) whose
games met any of the named bosses to runs/_policy_lab/NAME.txt, in the set's order. A change that only acts under some
bosses can only change those games: run it on this list (chunk.sh BIN <that file> ...) and splice its rows into the
baseline for the next version's full-set numbers (v65 Dagger, v67 Bell/Hook, v68 Verdant Leaf)."""
import json, os, sys

R = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
run, sset, name, bosses = sys.argv[1], sys.argv[2], sys.argv[3], set(sys.argv[4:])
rows = [json.loads(l) for l in open(os.path.join(R, "runs", "_policy_lab", run, "results.jsonl")) if l.strip()]
met = {r["seed"] for r in rows if any(x.get("boss") in bosses for x in r.get("snaps", []))}
order = open(os.path.join(R, "docs", "samples", "cloud-lab", f"seeds-{sset}.txt")).read().split()
out = os.path.join(R, "runs", "_policy_lab", f"{name}.txt")
open(out, "w").write("\n".join(s for s in order if s in met) + "\n")
print(out, len(met))
