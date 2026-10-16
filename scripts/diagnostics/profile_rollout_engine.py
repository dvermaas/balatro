"""Engine-only cost of a frozen-build rollout, as a baseline for a native port (docs/rust-port-scope.md).

Plays seed PROFSEED to the first shop, adds a fixed joker build, then runs n rollouts through
rollout.play_horizon (2 bosses ahead, pickled snapshot, reseeded per rollout, shop population off)
with a trivial policy: play the first 5 cards, never discard. The bot's text scorer is not used, so
no localization is needed and the timing is Jackdaw plus the rollout loop.

  python scripts/diagnostics/profile_rollout_engine.py [--jokers j_joker,j_baron] [--n 200] [--profile]
"""
from __future__ import annotations

import argparse
import cProfile
import os
import pstats
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "src"))

import sim_backend  # noqa: E402  (puts Jackdaw on sys.path)

sim_backend.apply_patches()

import rollout  # noqa: E402
from jackdaw.engine.actions import CashOut, GamePhase, PlayHand, SelectBlind  # noqa: E402
from jackdaw.engine.card_factory import create_joker  # noqa: E402
from jackdaw.engine.game import step  # noqa: E402
from jackdaw.engine.rng import PseudoRandom  # noqa: E402
from jackdaw.engine.run_init import initialize_run  # noqa: E402

BUILDS = {
    "5": "j_joker,j_ride_the_bus,j_photograph,j_baron,j_blueprint",
    "8": "j_joker,j_ride_the_bus,j_photograph,j_baron,j_blueprint,j_hanging_chad,j_greedy_joker,j_cavendish",
}


class FirstFive:
    def decide(self, gs):
        return "play", list(range(min(5, len(gs["hand"]))))


def shop_state(seed, jokers):
    gs = initialize_run("b_red", 1, seed)
    while gs["phase"] != GamePhase.SHOP:
        ph = gs["phase"]
        if ph == GamePhase.BLIND_SELECT:
            step(gs, SelectBlind())
        elif ph == GamePhase.SELECTING_HAND:
            step(gs, PlayHand(card_indices=tuple(range(min(5, len(gs["hand"]))))))
        elif ph == GamePhase.ROUND_EVAL:
            step(gs, CashOut())
        else:
            raise RuntimeError(ph)
    gs["dollars"] = 50
    gs["jokers"].extend(create_joker(k) for k in jokers)
    gs["_rollout_no_shop"] = True
    return gs


def run(snap, n):
    for k in range(n):
        gs = snap.copy()
        gs["rng"] = PseudoRandom(rollout._seed(k))
        rollout.play_horizon(gs, FirstFive(), bosses=2)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--jokers", default="5", help="a build name (5, 8) or comma-separated joker keys")
    ap.add_argument("--n", type=int, default=200)
    ap.add_argument("--profile", action="store_true")
    a = ap.parse_args()
    snap = rollout.Snapshot(shop_state("PROFSEED", BUILDS.get(a.jokers, a.jokers).split(",")))
    t = time.perf_counter()
    for _ in range(200):
        snap.copy()
    copy_ms = (time.perf_counter() - t) / 200 * 1e3
    t = time.perf_counter()
    run(snap, a.n)
    ro_ms = (time.perf_counter() - t) / a.n * 1e3
    print(f"pickle {len(snap.blob)} B, unpickle {copy_ms:.2f} ms, rollout {ro_ms:.2f} ms (incl. unpickle)")
    if a.profile:
        pr = cProfile.Profile()
        pr.enable()
        run(snap, max(1, a.n // 2))
        pr.disable()
        pstats.Stats(pr).sort_stats("tottime").print_stats(20)


if __name__ == "__main__":
    main()
