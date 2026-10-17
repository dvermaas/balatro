"""Fixture for native/bot/src/jokers.rs (Scorer, score) and hands.rs (evaluated_plays, best_score,
best_play, score_many): runs/_port/fixtures/scorer.jsonl.gz.

Inputs are the SELECTING_HAND states of both arms (bot-forecast-items, then bot-forecast-long), indexed
"gs" in ALL. Each row also names the boss ("boss") that decisions._round_info gives the state, and
"suit_x": the hand card index whose suit is replaced by 'X' (the kernel can't encode it), or null.
allowed: "none" (no filter) or "boss" (decisions.BossAllow(hands, boss)). The scorer is
jokers.Scorer(decisions._boss_levels(hands, boss), jokers.build(gs), rnd) as in decisions.py.

Every case is computed twice in Python (native kernel on, native.ENABLED = False); the two must agree,
and the stored "ret" is the pure-Python result. Rust compares with hands::native::set_enabled(true/false).

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/scorer.py
"""

import copy
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # scripts/port: the states module
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import decisions  # noqa: E402
import hands  # noqa: E402
import jokers  # noqa: E402
import native  # noqa: E402

ALL = states.states("bot-forecast-items", state="SELECTING_HAND") + states.states("bot-forecast-long", state="SELECTING_HAND")
rows = []
MAX_FULL = 200  # states with full evaluated_plays rows (keeps the fixture small)


def setup(gs, boss, allowed, suit_x):
    """(infos, levels, scorer, allowed_or_None) as decisions.py builds them for this state."""
    if suit_x is not None:
        gs = copy.deepcopy(gs)
        gs["hand"]["cards"][suit_x]["value"]["suit"] = "X"
    hand = gs["hand"]["cards"]
    infos = [hands.card_info(c) for c in hand]
    levels = decisions._boss_levels(hands._d(gs.get("hands")), boss)
    scorer = jokers.Scorer(levels, jokers.build(gs), hands._d(gs.get("round")))
    allow = decisions.BossAllow(hands._d(gs.get("hands")), boss) if allowed == "boss" else None
    return gs, infos, scorer, allow


def run_all(gs, boss, allowed, suit_x, full):
    """Returns {fn: ret} computed in the current native mode."""
    _, infos, scorer, allow = setup(gs, boss, allowed, suit_x)
    plays = list(hands.evaluated_plays(infos, {}, scorer, allow))
    out = {
        "best_score": hands.best_score(infos, scorer, allow),
        "best_play": list(hands.best_play(infos, {}, scorer, allow)),
    }
    if full:
        out["evaluated_plays"] = [[s, n, list(c), sorted(sc)] for s, n, c, sc in plays]
        sub = [(list(c), n, [c.index(i) for i in sorted(sc)]) for s, n, c, sc in plays[:40]]  # score() wants positions within the combo
        out["score_many"] = [scorer(infos, c, n, sc) for c, n, sc in sub]  # reference: jokers.score
    return out


def modes(gs, boss, allowed, suit_x, full):
    """Python reference (native off) and native-on run; they must agree."""
    native.ENABLED = False
    ref = run_all(gs, boss, allowed, suit_x, full)
    native.ENABLED = True
    got = run_all(gs, boss, allowed, suit_x, full)
    native.ENABLED = True
    if got != ref:
        bad = [k for k in ref if got.get(k) != ref[k]]
        raise SystemExit(f"python native != python pure for gs {gs_index} boss={boss} allowed={allowed} "
                         f"suit_x={suit_x}: {bad}")
    return ref


if __name__ == "__main__":
    n_full = 0
    cases = 0
    for gs_index, gs in enumerate(ALL):
        boss = decisions._round_info(gs)["blind"]
        full = gs_index % max(1, len(ALL) // MAX_FULL) == 0 and n_full < MAX_FULL
        for allowed in ("none", "boss"):
            ref = modes(gs, boss, allowed, None, full)
            base = {"gs": gs_index, "boss": boss, "allowed": allowed, "suit_x": None}
            rows.append({"fn": "best_score", "args": base, "ret": ref["best_score"]})
            rows.append({"fn": "best_play", "args": base, "ret": ref["best_play"]})
            if full:
                rows.append({"fn": "evaluated_plays", "args": base, "ret": ref["evaluated_plays"]})
                rows.append({"fn": "score_many", "args": base, "ret": ref["score_many"]})
            cases += 1
        if full:
            n_full += 1
    # unknown suit: a few states, first card and last card replaced by 'X' (native encode must refuse)
    for gs_index in range(0, len(ALL), max(1, len(ALL) // 6))[:6]:
        gs = ALL[gs_index]
        n = len(gs["hand"]["cards"])
        boss = decisions._round_info(gs)["blind"]
        for suit_x in sorted({0, n - 1}):
            for allowed in ("none", "boss"):
                ref = modes(gs, boss, allowed, suit_x, True)
                base = {"gs": gs_index, "boss": boss, "allowed": allowed, "suit_x": suit_x}
                rows.append({"fn": "best_score", "args": base, "ret": ref["best_score"]})
                rows.append({"fn": "best_play", "args": base, "ret": ref["best_play"]})
                rows.append({"fn": "evaluated_plays", "args": base, "ret": ref["evaluated_plays"]})
                rows.append({"fn": "score_many", "args": base, "ret": ref["score_many"]})
    path = states.write_fixture("scorer", rows)
    from collections import Counter
    print(len(ALL), "states,", len(rows), "rows", dict(Counter(r["fn"] for r in rows)), path)
    print("size", path.stat().st_size)
