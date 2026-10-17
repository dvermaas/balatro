"""Fixture for native/bot/src/odds.rs (forecast, annotate, p_at_least, p_all_ranks, arg_window):
runs/_port/fixtures/odds.jsonl.gz.

Rows are {"fn", "args", "ret"}. A gamestate is referenced by its index "gs" in ALL, the concatenation of
the SELECTING_HAND states of both arms (bot-forecast-items, then bot-forecast-long), as in
jevbot::trace::states. Per gamestate (evenly spaced, capped at CAP):
- forecast: the discard plans of hands.discard_candidates(hand, 5), forecast as decisions.selecting_hand
  does (need = chips_still_needed * margin, pace = chips_needed_per_remaining_hand or 0, draw_count
  3 for The Serpent else max(0, hand_size - len(hand) + n), scorer jokers.Scorer(boss levels, build(gs),
  round), allowed BossAllow(hands, boss); the after_discard joker changes are NOT modelled, see odds.rs).
  Rust rebuilds the plans from the gamestate; "ret" is the plans after forecast.
- annotate: the same plans, scored with hands.best_score(cards, scorer, None).
- arg_window: missing = the three ranks after the hand's first card, keep = the plan-0 complement.
Pure functions: p_at_least and p_all_ranks over a small grid. Sets are written as sorted lists, None first.

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/odds.py
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # scripts/port: the states module
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import decisions  # noqa: E402
import hands  # noqa: E402
import jokers  # noqa: E402
import odds  # noqa: E402

ALL = (states.states("bot-forecast-items", "SELECTING_HAND")
       + states.states("bot-forecast-long", "SELECTING_HAND"))
CAP = 400
SAMPLES = 32
rows = []


def jsonable(x):
    """Tuples as lists; sets as sorted lists with None first (the order of Rust's Option)."""
    if isinstance(x, set):
        return sorted((jsonable(v) for v in x), key=lambda v: (v is not None, v or ""))
    if isinstance(x, (list, tuple)):
        return [jsonable(v) for v in x]
    if isinstance(x, dict):
        return {k: jsonable(v) for k, v in x.items()}
    return x


def add(fn, args, ret):
    rows.append({"fn": fn, "args": jsonable(args), "ret": jsonable(ret)})


step = max(1, len(ALL) // CAP)
for gi in range(0, len(ALL), step):
    gs = ALL[gi]
    info = decisions._round_info(gs)
    boss = info["blind"]
    if boss is None:
        continue
    hand = decisions.cards_of(gs, "hand")
    infos = [hands.card_info(c) for c in hand]
    deck = [hands.card_info(c) for c in decisions.cards_of(gs, "cards")]
    js = jokers.build(gs)
    rnd = decisions._d(gs.get("round"))
    margin = decisions.CLEAR_MARGIN if js else 1.0
    need = info["chips_still_needed"] * margin
    pace = info["chips_needed_per_remaining_hand"] or 0
    hand_size = decisions.area(gs, "hand").get("limit") or len(hand)
    levels = decisions._boss_levels(decisions._d(gs.get("hands")), boss)
    allowed = decisions.BossAllow(decisions._d(gs.get("hands")), boss)

    def scorer_factory(discarded, drawn):
        return jokers.Scorer(levels, js, rnd)

    def draw_count(n):
        return 3 if boss == "The Serpent" else max(0, hand_size - len(hand) + n)

    plans = hands.discard_candidates(hand, 5)
    odds.forecast(plans, infos, deck, scorer_factory, allowed, need=need, pace=pace,
                  draw_count=draw_count, samples=SAMPLES)
    add("forecast", {"gs": gi, "boss": boss, "need": need, "pace": pace, "hand_size": hand_size,
                     "hand_len": len(hand), "samples": SAMPLES}, plans)

    plans = hands.discard_candidates(hand, 5)
    sc = jokers.Scorer(levels, js, rnd)
    odds.annotate(plans, infos, deck, lambda cards: hands.best_score(cards, sc, None))
    add("annotate", {"gs": gi, "boss": boss}, plans)

    first = hands.discard_candidates(hand, 5)
    keep = [i for i in range(len(hand)) if not (first and i in first[0]["cards"])]
    missing = ["5", "6", "7"]
    window = odds.arg_window(missing, infos, keep)
    add("arg_window", {"gs": gi, "missing": missing, "keep": keep},
        sorted(window, key=lambda x: (x is None, x or "")))

for n in (1, 2, 5, 8, 13, 20, 30, 52):
    for k in sorted({0, 1, n // 4, n // 2, n - 1, n}):
        if not 0 <= k <= n:
            continue
        for d in (0, 1, 2, 3, 5, 8, n):
            for need in (0, 1, 2, 3):
                add("p_at_least", {"n": n, "k": k, "d": d, "need": need}, odds.p_at_least(n, k, d, need))

for n in (10, 52):
    for counts in ([], [4], [4, 4], [4, 4, 4, 4, 4], [1, 2, 3], [0, 4], [4, 0, 3]):
        if sum(counts) > n:  # a group can't be larger than the pile (Python comb would raise)
            continue
        for d in (0, 1, 2, 3, 5):
            add("p_all_ranks", {"n": n, "counts": counts, "d": d}, odds.p_all_ranks(n, counts, d))

print(f"{len(rows)} rows, {sum(r['fn'] == 'forecast' for r in rows)} forecast states")
print(states.write_fixture("odds", rows))
