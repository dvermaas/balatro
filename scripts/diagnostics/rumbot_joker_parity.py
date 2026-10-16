"""Per-joker rollout identity, Jackdaw vs jev_sim: which jokers' running state src/sim_bridge.py maps.

For each joker key: play a lockstep-policy Jackdaw game (scripts/rust_spike/lockstep.py), add the
joker at the first shop (the only joker held: shops buy nothing), keep playing so its state evolves, and at the next shops compare K
rollouts of rollout.play_horizon (Python) with jev_sim (Rust) under the same seeds.

  python scripts/diagnostics/rumbot_joker_parity.py [--jokers j_a,j_b] [--seeds 3] [--k 6] [--out FILE]
"""
import argparse
import json
import os
import random
import zlib
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "src"))
LOCKSTEP = os.path.join(ROOT, "scripts", "rust_spike", "lockstep.py")
src = open(LOCKSTEP).read().split('if __name__ == "__main__":')[0]
__file__ = LOCKSTEP
exec(compile(src, "lockstep", "exec"))

import rollout  # noqa: E402
import sim_bridge  # noqa: E402
from jackdaw.engine.card_factory import create_joker  # noqa: E402
from jackdaw.engine.rng import PseudoRandom  # noqa: E402

EFFECTS = json.load(open(os.path.join(ROOT, "tests", "joker_effects.json")))


def step_policy(jd, s, flags):
    ph = s["phase"]
    if ph == "BLIND":
        return ("skip", None) if s["blind"] == 0 and s["ante"] % 2 == 0 else ("select", None)
    if ph == "PLAY":
        toss = choose_discard(s["hand"]) if jd.gs["current_round"].get("discards_left", 0) > 0 else None
        ids = toss or choose_play(s["hand"])
        used, arg = set(), []
        for i in ids:
            j = next(k for k, h in enumerate(s["hand"]) if h == i and k not in used)
            used.add(j)
            arg.append(j)
        return ("discard" if toss else "play"), sorted(arg)
    if ph == "EVAL":
        return "cash", None
    if ph == "SHOP":  # buy nothing: the joker under test is the only one held
        flags.clear()
        return "leave", None
    if ph == "PACK":  # skip-tag packs: take a planet or playing card, never a joker
        pick = next((i for i, st in enumerate(s["_pack_sets"]) if st in ("Planet", "Default", "Enhanced")), None)
        return ("pick", pick) if pick is not None else ("skippack", None)
    return None, None


def joker_states(key, seed, max_shops=4):
    """Shop states (after the first) of a game that holds `key` from its first shop on."""
    jd = Jackdaw()
    s = jd.new(seed)
    flags, out, added = {}, [], False
    for _ in range(500):
        if s["phase"] in ("OVER", "WON") or jd.gs.get("won") or s["err"] or len(out) >= max_shops:
            break
        if s["phase"] == "SHOP" and not flags.get("seen"):
            flags["seen"] = True
            if not added:
                j = create_joker(key)
                j.add_to_deck(jd.gs)
                jd.gs["jokers"].append(j)
                added = True
                s = jd.dump("")
            else:
                out.append(rollout.Snapshot(jd.gs).copy())
        c, arg = step_policy(jd, s, flags)
        if c is None:
            break
        if c == "leave":
            flags.pop("seen", None)
        s = jd.cmd(c, arg)
    return out


def compare(gs, k, policy_seed):
    gs["_rollout_no_shop"] = True
    texts = random.Random(policy_seed).sample(EFFECTS, 3)
    pol = rollout.Policy([{"label": "J", "value": {"effect": e}, "modifier": {}} for e in texts])
    snap = rollout.Snapshot(gs)
    py = []
    for i in range(k):
        g = snap.copy()
        g["rng"] = PseudoRandom(rollout._seed(i))
        py.append(rollout.play_horizon(g, pol, 2))
    rs = sim_bridge.rollouts(gs, pol, k, 2)
    return sum(a == b for a, b in zip(py, rs)), k


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--jokers", default=None)
    ap.add_argument("--seeds", type=int, default=3)
    ap.add_argument("--k", type=int, default=6)
    ap.add_argument("--out", default=None)
    a = ap.parse_args()
    keys = a.jokers.split(",") if a.jokers else sorted(k for k in CENTERS if k.startswith("j_"))
    rows = []
    for key in keys:
        same = total = 0
        err = None
        for si in range(a.seeds):
            try:
                for n, gs in enumerate(joker_states(key, f"JP{si:02d}{zlib.crc32(key.encode()) % 9973:04d}")):
                    s, t = compare(gs, a.k, si * 100 + n)
                    same, total = same + s, total + t
            except Exception as e:  # noqa: BLE001
                err = f"{type(e).__name__}: {e}"[:160]
                break
        rows.append({"joker": key, "same": same, "total": total, "error": err})
        flag = "ok " if total and same == total and not err else "!! "
        print(f"{flag}{key:24} {same:3}/{total:<3} {err or ''}", flush=True)
    bad = [r for r in rows if r["error"] or r["same"] != r["total"]]
    print(f"{len(rows) - len(bad)}/{len(rows)} jokers identical on every rollout")
    if a.out:
        json.dump(rows, open(a.out, "w"), indent=1)


if __name__ == "__main__":
    main()
