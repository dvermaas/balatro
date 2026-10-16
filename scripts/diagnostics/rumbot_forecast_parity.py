"""v24 check: rollout.run_options (the shop forecast's rollouts) is identical with jev_sim and without.

Shop states from lockstep-policy Jackdaw games (scripts/rust_spike/lockstep.py); options as the shop
forecast lists them: skip, buy each shop card, redeem the voucher, sell owned joker j and buy shop
joker i (an owned Invisible Joker is added sometimes, so selling it exercises the per-seed path).

  python scripts/diagnostics/rumbot_forecast_parity.py [n_states] [n_rollouts]
"""
import json
import os
import random
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "src"))
sys.path.insert(0, os.path.join(ROOT, "scripts", "diagnostics"))
import rumbot_rollout_parity as rp  # noqa: E402  (shop_states, EFFECTS)

import rollout  # noqa: E402
import sim_bridge  # noqa: E402
from jackdaw.engine.card_factory import create_joker  # noqa: E402



def options(gs):
    out = [{"key": "skip", "actions": []}]
    out += [{"key": f"buy_{i}", "actions": [("buy", i)]} for i in range(len(gs.get("shop_cards", [])))]
    out += [{"key": f"voucher_{i}", "actions": [("voucher", i)]} for i in range(len(gs.get("shop_vouchers", [])))]
    for j in range(len(gs.get("jokers", []))):
        for i, c in enumerate(gs.get("shop_cards", [])):
            if c.ability.get("set") == "Joker":
                out.append({"key": f"swap_{j}_{i}", "actions": [("sell", j), ("buy", i)]})
    return out


def main():
    N = int(sys.argv[1]) if len(sys.argv) > 1 else 30
    K = int(sys.argv[2]) if len(sys.argv) > 2 else 12
    rng = random.Random(24)
    states = []
    for i in range(400):
        if len(states) >= N:
            break
        states += rp.shop_states(f"FP{i:05d}")[:2]
    same = total = 0
    t_py = t_rs = 0.0
    for si, gs in enumerate(states[:N]):
        gs["_rollout_no_shop"] = True
        gs["dollars"] = max(gs.get("dollars", 0), 30)
        if si % 3 == 0:  # an owned Invisible Joker: selling it draws a random joker to copy
            j = create_joker("j_invisible")
            j.ability["invis_rounds"] = 3
            j.add_to_deck(gs)
            gs["jokers"].append(j)
        snap = rollout.Snapshot(gs)
        texts = [{"label": "J", "value": {"effect": e}, "modifier": {}} for e in rng.sample(rp.EFFECTS, 3)]
        results = {}
        for native in (False, True):
            opts = options(gs)
            pols = {o["key"]: rollout.Policy(texts) for o in opts}
            avail = sim_bridge.available
            sim_bridge.available = (lambda: True) if native else (lambda: False)
            t = time.perf_counter()
            try:
                runs, k = rollout.run_options(snap, opts, pols, K, 2, time.time(), None)
            finally:
                sim_bridge.available = avail
            dt = time.perf_counter() - t
            if native:
                t_rs += dt
            else:
                t_py += dt
            results[native] = (runs, k, {o["key"]: o.get("error") for o in opts})
        for key in results[False][0]:
            total += 1
            same += results[False][0][key] == results[True][0][key]
        if results[False][1:] != results[True][1:]:
            print("state", si, "k/errors differ", results[False][1:], results[True][1:])
    print(f"{same}/{total} options identical over {K} rollouts each; run_options python {t_py:.1f} s, rust {t_rs:.1f} s "
          f"({t_py / max(t_rs, 1e-9):.0f}x)")


if __name__ == "__main__":
    main()
