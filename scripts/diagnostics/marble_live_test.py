"""Live test for GitHub issue #2 (Marble Joker): does Marble's Stone card join the deck before or
after the new round's 'nr' shuffle?

Drives the real game (BalatroBot) and the Jackdaw sim through the same steps: start seed, select the
small blind, add Marble Joker, clear the blind with one played card (chips set by `set`), cash out,
skip the shop, select the big blind. Marble fires there, then the deck is shuffled. Compares the
real deck order (hand + draw pile, a diagnostic only, never policy input) with the sim's under
two rules: Stone shuffled in, and Stone at the pile bottom after the shuffle (upstream Jackdaw). The
result (shuffled in, 3/3 seeds) is now src/sim_patches.marble_stone_joins_before_shuffle, so on
current code matches_jackdaw_after_shuffle is the patched sim and both columns agree.

  .venv/Scripts/python scripts/diagnostics/marble_live_test.py [SEED ...]
"""

import copy
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "src"))

from balatro import Balatro  # noqa: E402
from sim_backend import SimBalatro  # noqa: E402


def key(c):
    v = c.get("value") or {}
    m = c.get("modifier") or {}
    return f"{c.get('label')}|{v.get('rank')}|{v.get('suit')}|{m.get('enhancement') if isinstance(m, dict) else ''}"


def order(gs):
    """Hand (dealt) then the draw pile in draw order (next draw first)."""
    hand = [key(c) for c in (gs.get("hand") or {}).get("cards", [])]
    pile = [key(c) for c in reversed((gs.get("cards") or {}).get("cards", []))]
    return hand, pile


def run(seed):
    real, sim = Balatro(), SimBalatro()
    start = {"seed": seed, "deck": "RED", "stake": "WHITE"}
    try:
        real.call("menu")
    except Exception:
        pass
    real.call("start", start)
    sim.call("start", start)
    for g in (real, sim):
        g.call("select")
    real.call("add", {"key": "j_marble"})
    from jackdaw.engine.card_factory import create_joker
    sim.gs["jokers"].append(create_joker("j_marble"))
    target = real.state()["blinds"]["small"]["score"]
    real.call("set", {"chips": target})
    sim.gs["chips"] = target
    for g in (real, sim):
        g.call("play", {"cards": [0]})
        g.call("cash_out")
        g.call("next_round")
    # Alternative rule: run the sim twice from here, once as patched Jackdaw does, once with the
    # Stone shuffled in (pending_deck_bottom moved into the deck before the 'nr' shuffle).
    alt = copy.deepcopy(sim)
    from jackdaw.engine import game as engine
    orig = engine._apply_setting_blind_mutations

    def stone_before_shuffle(gs, muts, jokers):
        orig(gs, muts, jokers)
        gs.setdefault("deck", []).extend(gs.pop("pending_deck_bottom", []))
    real.call("select")
    sim.call("select")
    engine._apply_setting_blind_mutations = stone_before_shuffle
    try:
        alt.call("select")
    finally:
        engine._apply_setting_blind_mutations = orig
    r, s, a = order(real.state()), order(sim.state()), order(alt.state())
    stone_r = [i for i, k in enumerate(r[0] + r[1]) if k.startswith("Stone")]
    out = {"seed": seed, "stone_position_real": stone_r, "deck_size": len(r[0] + r[1]),
           "matches_jackdaw_after_shuffle": r == s, "matches_stone_shuffled_in": r == a,
           "real_hand": r[0], "sim_hand": s[0], "alt_hand": a[0]}
    print(json.dumps(out))
    return out


if __name__ == "__main__":
    res = [run(s) for s in (sys.argv[1:] or ["EPN12S4E", "KMTUIA73", "725LK1MF"])]
    Path(ROOT / "runs" / "marble-live-test-latest.json").write_text(json.dumps(res, indent=1))
