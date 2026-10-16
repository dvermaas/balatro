"""Re-run each diverging seed from a lockstep result file and label the first divergence by cause.

  python scripts/rust_spike/classify.py docs/samples/rust-spike-lockstep.json

Labels are heuristics from the jokers held up to that step (see docs/rust-port-scope.md); "other"
rows are printed for a manual look with trace.py.
"""
import collections
import json
import os
import sys

LOCKSTEP = os.path.join(os.path.dirname(os.path.abspath(__file__)), "lockstep.py")
src = open(LOCKSTEP).read().split('if __name__ == "__main__":')[0]
__file__ = LOCKSTEP
rows = json.load(open(sys.argv[1]))
exec(compile(src, "lockstep", "exec"))

held = {}
_cmd, _new = Jackdaw.cmd, Jackdaw.new  # noqa: F821  (defined by lockstep)


def cmd(self, c, arg=None):
    r = _cmd(self, c, arg)
    held[self.seed].append({j[2:] for j in r["jokers"]})
    return r


def new(self, seed):
    self.seed = seed
    held[seed] = []
    return _new(self, seed)


Jackdaw.cmd, Jackdaw.new = cmd, new  # noqa: F821


def label(seen, d):
    f = d["fields"]
    if "mr_bones" in seen and "money" in f:
        return "Mr. Bones: blind reward when saved"
    if "marble" in seen and "hand" in f:
        return "Marble Joker: Stone card vs shuffle"
    if "madness" in seen and "jokers" in f:
        return "Madness + Ceremonial Dagger"
    if "score" in f and isinstance(f["score"][0], (int, float)) and isinstance(f["score"][1], (int, float)) \
            and abs(f["score"][0] - f["score"][1]) <= 1:
        return "SMODS xMult rounding (expected)"
    if "satellite" in seen and "money" in f:
        return "Satellite payout"
    if "todo_list" in seen:
        return "To Do List"
    return "other"


procs = {k: Proc(v) for k, v in DRIVERS.items()}  # noqa: F821
counts = {e: collections.Counter() for e in procs}
for r in rows:
    if not any(r["first_diff"].values()):
        continue
    res = run_seed(r["seed"], procs)  # noqa: F821
    for e, d in res["first_diff"].items():
        if not d:
            continue
        seen = set().union(*held[r["seed"]][:d["step"]]) if d["step"] else set()
        lab = label(seen, d)
        counts[e][lab] += 1
        if lab == "other":
            print(f"{e:7} {r['seed']} after {d['after']}: {json.dumps(d['fields'])[:120]} jokers={sorted(seen)}")
for e, c in counts.items():
    print(e, sum(c.values()), dict(c.most_common()))
