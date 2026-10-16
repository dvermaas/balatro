"""Rollout identity: Python rollout.play_horizon on Jackdaw vs jev_sim (rumbot-sim, src/sim_bridge.py).

Shop states come from lockstep-policy Jackdaw games (scripts/rust_spike/lockstep.py's policy: buys
jokers and vouchers, opens packs); each gets a rollout.Policy with 3 random joker texts; K rollouts
per state run in both engines with the same seeds and must give identical per-blind rows.

  python scripts/diagnostics/rumbot_rollout_parity.py [n_states] [k_rollouts]
"""
import json, random, sys, time
import os
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "src"))
LOCKSTEP = os.path.join(ROOT, "scripts", "rust_spike", "lockstep.py")
src = open(LOCKSTEP).read().split('if __name__ == "__main__":')[0]
__file__ = LOCKSTEP
exec(compile(src, "lockstep", "exec"))
import rollout, sim_bridge
from jackdaw.engine.rng import PseudoRandom
EFFECTS = json.load(open(os.path.join(ROOT, "tests", "joker_effects.json")))
N, K = 20, 10

def shop_states(seed, every=1):
    """Shop states from a lockstep-policy Jackdaw game (buys jokers, vouchers, packs)."""
    jd = Jackdaw(); s = jd.new(seed); flags = {}; out = []
    for _ in range(400):
        ph = s["phase"]
        if ph in ("OVER", "WON") or jd.gs.get("won") or s["err"]:
            break
        if ph == "SHOP" and not flags:
            out.append(rollout.Snapshot(jd.gs).copy())
        if ph == "BLIND":
            c, arg = ("skip", None) if s["blind"] == 0 and s["ante"] % 2 == 0 else ("select", None)
        elif ph == "PLAY":
            toss = choose_discard(s["hand"]) if jd.gs["current_round"].get("discards_left", 0) > 0 else None
            ids = toss or choose_play(s["hand"]); used = set(); arg = []
            for i in ids:
                j = next(k for k, h in enumerate(s["hand"]) if h == i and k not in used); used.add(j); arg.append(j)
            c, arg = ("discard" if toss else "play"), sorted(arg)
        elif ph == "EVAL":
            c, arg = "cash", None
        elif ph == "SHOP":
            buy = next((i for i, (st, cost) in enumerate(zip(s["_sets"], s["_costs"])) if st == "Joker" and cost <= s["money"] and len(s["jokers"]) < s["_jslots"]), None)
            c, arg = ("buy", buy) if buy is not None else ("leave", None)
            if buy is None and s["vouchers"] and s["_vcost"][0] <= s["money"] and not flags.get("v"):
                c, arg = "voucher", 0; flags["v"] = True
            if c == "leave":
                flags.clear()
        elif ph == "PACK":
            pick = next((i for i, st in enumerate(s["_pack_sets"]) if st in ("Planet", "Default", "Enhanced") or (st == "Joker" and len(s["jokers"]) < s["_jslots"])), None)
            c, arg = ("pick", pick) if pick is not None else ("skippack", None)
        s = jd.cmd(c, arg)
    return out

def main():
    global N, K
    N = int(sys.argv[1]) if len(sys.argv) > 1 else N
    K = int(sys.argv[2]) if len(sys.argv) > 2 else K
    rng = random.Random(3)
    states = []
    for i in range(400):
        if len(states) >= N: break
        sts = shop_states(f"BR{i:05d}")
        states += sts[1:][:3]
    states = states[:N]
    same = total = 0; tpy = trs = 0.0; diffs = []
    for si, gs in enumerate(states):
        gs["_rollout_no_shop"] = True
        jd_js = [{"label": "J", "value": {"effect": e}, "modifier": {}} for e in random.Random(si).sample(EFFECTS, 3)]
        pol = rollout.Policy(jd_js)
        snap = rollout.Snapshot(gs)
        t = time.perf_counter()
        py = []
        for k in range(K):
            g = snap.copy(); g["rng"] = PseudoRandom(rollout._seed(k)); py.append(rollout.play_horizon(g, pol, 2))
        tpy += time.perf_counter() - t
        t = time.perf_counter()
        try:
            rs = sim_bridge.rollouts(gs, pol, K, 2)
        except Exception as e:
            print("state", si, "error:", repr(e)[:200]); continue
        trs += time.perf_counter() - t
        for k in range(K):
            total += 1
            if py[k] == rs[k]: same += 1
            elif len(diffs) < 8: diffs.append((si, k, [j.center_key for j in gs["jokers"]], py[k][:3], rs[k][:3]))
    print(f"{same}/{total} rollouts identical; python {tpy/total*1e3:.2f} ms, rust {trs/total*1e3:.3f} ms per rollout")
    for d in diffs: print(d)


if __name__ == "__main__":
    main()
