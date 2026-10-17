"""Fixture for native/bot/src/hands.rs (card_info, describe, classify, estimate, play_candidates,
discard_candidates): runs/_port/fixtures/hands.jsonl.gz.

Rows are {"fn", "args", "ret"}. A gamestate is referenced by its index "gs" in ALL, the concatenation of
the SELECTING_HAND states of both arms (bot-forecast-items, then bot-forecast-long), which is the order of
jevbot::trace::states in Rust. Sets are written as sorted lists (None last). Subsets for classify and
estimate: up to 20 per hand, evenly spaced over all subsets of size 1-5 (combo = positions in the hand).

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/hands.py
"""

import sys
from itertools import combinations
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # scripts/port: the states module
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import hands  # noqa: E402

ALL = (states.states("bot-forecast-items", "SELECTING_HAND")
       + states.states("bot-forecast-long", "SELECTING_HAND"))
SUBSETS_PER_HAND = 20
rows = []


def add(fn, args, ret):
    rows.append({"fn": fn, "args": args, "ret": ret})


def _d(x):
    return x if isinstance(x, dict) else {}


def hand_cards(gs):
    c = _d(gs.get("hand")).get("cards")
    return c if isinstance(c, list) else []


def levels(gs):
    return _d(gs.get("hands"))


def sample_subsets(n):
    subs = [c for r in range(1, min(5, n) + 1) for c in combinations(range(n), r)]
    if len(subs) <= SUBSETS_PER_HAND:
        return subs
    step = len(subs) / SUBSETS_PER_HAND
    return [subs[int(k * step)] for k in range(SUBSETS_PER_HAND)]


def sort_set(s):
    return sorted(s, key=lambda x: (x is None, x or ""))


for gi, gs in enumerate(ALL):
    cards = hand_cards(gs)
    infos_all = [hands.card_info(c) for c in cards]
    for k, c in enumerate(cards):
        add("card_info", {"gs": gi, "i": k}, infos_all[k])
        add("describe", {"gs": gi, "i": k}, hands.describe(c))
        add("describe", {"info": infos_all[k]}, hands.describe(infos_all[k]))
    for sub in sample_subsets(len(cards)):
        cs = [infos_all[i] for i in sub]
        name, sc = hands.classify(cs)
        add("classify", {"gs": gi, "combo": list(sub)}, [name, sc])
        add("estimate", {"gs": gi, "combo": list(sub), "name": name, "scoring": sc},
            hands.estimate(cs, name, sc, levels(gs)))
    add("play_candidates", {"gs": gi}, hands.play_candidates(cards, levels(gs)))
    plans = []
    for plan in hands.discard_candidates(cards):
        kind, val = plan["goal"]
        if isinstance(val, set):
            val = sort_set(val)
        plans.append({"label": plan["label"], "cards": plan["cards"],
                      "goal": [kind, val], "tossed": plan["tossed"]})
    add("discard_candidates", {"gs": gi}, plans)

states.write_fixture("hands", rows)
print(f"{len(ALL)} SELECTING_HAND gamestates, {len(rows)} rows")
