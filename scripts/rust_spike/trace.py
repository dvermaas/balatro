"""Print the last few lockstep states (Jackdaw vs rumbot vs gambit) before a seed's first divergence.

  python scripts/rust_spike/trace.py SEED [n_rows]
"""
import json, os, sys
LOCKSTEP = os.path.join(os.path.dirname(os.path.abspath(__file__)), "lockstep.py")
src = open(LOCKSTEP).read().split('if __name__ == "__main__":')[0]
__file__ = LOCKSTEP
exec(compile(src, "lockstep", "exec"))
seed = sys.argv[1]
log = []
_orig = Jackdaw.cmd
def cmd(self, c, arg=None):
    r = _orig(self, c, arg)
    log.append(("jd", c, arg, r, [str(getattr(j, "center_key", j)) for j in self.gs.get("jokers", [])], dict(self.gs.get("current_round", {})).get("dollars_to_be_earned")))
    return r
Jackdaw.cmd = cmd
procs = {k: Proc(v) for k, v in DRIVERS.items()}
_pc = Proc.cmd
def pcmd(self, c, arg=None, ids=None):
    r = _pc(self, c, arg, ids)
    log.append((self.p.args[0].rsplit("/", 1)[1][:6], c, arg, r))
    return r
Proc.cmd = pcmd
res = run_seed(seed, procs)
print(json.dumps(res["first_diff"]))
keep = ("phase", "ante", "blind", "boss", "money", "hand", "jokers", "slots", "vouchers", "packs", "tags", "total")
for row in log[-int(sys.argv[2]) if len(sys.argv) > 2 else -9:]:
    st = row[3]
    print(row[0], row[1], row[2], {k: st[k] for k in keep if st.get(k) not in ([], None)})
