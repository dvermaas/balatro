"""Side-by-side decision log for one state/rollout of rumbot_rollout_parity.py: Jackdaw vs jev_sim.

  python scripts/diagnostics/rumbot_rollout_trace.py STATE_INDEX ROLLOUT_INDEX
"""
import json, random, sys
sys.argv = [sys.argv[0], "40", "1"] + sys.argv[1:]
si, k = int(sys.argv[3]), int(sys.argv[4])
import os
src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "rumbot_rollout_parity.py")).read()
src = src.split("same = total = 0")[0]
__file__ = os.path.join(os.path.dirname(os.path.abspath(__file__)), "rumbot_rollout_parity.py")
exec(compile(src, "bc", "exec"))
import jev_sim
gs = states[si]; gs["_rollout_no_shop"] = True
jd_js = [{"label": "J", "value": {"effect": e}, "modifier": {}} for e in random.Random(si).sample(EFFECTS, 3)]
pol = rollout.Policy(jd_js)
log = []
orig = pol.decide
def decide(g):
    kind, idx = orig(g)
    hand = ",".join(rollout._info(c)["rank"] + rollout._info(c)["suit"] if rollout._info(c)["rank"] else "ST" for c in g["hand"])
    log.append(f"{kind} {list(idx)} hand={hand} chips={g.get('chips')} hl={g['current_round']['hands_left']} dl={g['current_round']['discards_left']}")
    return kind, idx
pol.decide = decide
g = rollout.Snapshot(gs).copy(); g["rng"] = PseudoRandom(rollout._seed(k))
py = rollout.play_horizon(g, pol, 2)
rows, rlog = jev_sim.rollout_trace(sim_bridge.describe(gs), sim_bridge.policy_jokers(pol), rollout._seed(k), 2)
print("jokers", [j.center_key for j in gs["jokers"]], "boss", gs["round_resets"]["blind_choices"]["Boss"], "on deck", gs["blind_on_deck"])
for a, b in zip(log, rlog):
    print(("   " if a.replace("ST", "") == b else "!! ") + a); 
    if a != b: print("   " + b)
