"""Unit fixture for native/bot/src/jokers.rs, rest side: play_forecast_supported, after_discard, after_play,
sample_plays, gain, planet_level. Runs over states of both arms (bot-forecast-items then bot-forecast-long).

Rows {"fn", "args", "ret"}; "gs" is the index into ALL. Recent plays are built the way decisions._recent
builds them (card_info per hand card, combo, round hands_left/discards_left), pooled over the run's
SELECTING_HAND states seen so far in trace order (last 5). Tuples become lists; sample_plays rows are
[infos, combo, name, scoring positions, rnd]. Discard candidates are prefixes of the hand. Rules are
encoded as in jokers_parse.py (same jokers::Rule::to_json shape).

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/jokers_rest.py
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import jokers  # noqa: E402
from hands import card_info, classify  # noqa: E402,F401

ALL = states.states("bot-forecast-items") + states.states("bot-forecast-long")


def f3(e):
    return [float(v) for v in e]


def pred(p):
    if isinstance(p, str):
        return p
    if p[0] == "suit":
        return ["suit", p[1]]
    if p[0] == "ranks":
        return ["ranks", sorted(p[1])]
    assert p == ("all",), p
    return ["all"]


def spec(s):
    if s is None:
        return None
    k = s[0]
    if k == "const":
        return ["const", f3(s[1])]
    if k == "cond":
        _, cond, arg, yes, no = s
        return ["cond", cond, None if arg is None else sorted(str(x) for x in arg), f3(yes), f3(no)]
    if k == "per_discard":
        return ["per_discard", float(s[1])]
    return [k]


def rule(r):
    if r is None:
        return None
    return {"card": [[pred(p), float(c), float(m), float(x)] for p, c, m, x in r["card"]],
            "retrigger": [[pred(p), t] for p, t in r["retrigger"]],
            "held": [[pred(p), float(m), float(x)] for p, m, x in r["held"]],
            "held_retrigger": r.get("held_retrigger"),
            "hand": spec(r["hand"])}


def ent(e):
    return {"label": e["label"], "rule": rule(e["rule"]), "edition": e["edition"], "scoring": e["scoring"]}


def cards_of(gs, key):
    return (gs.get(key) or {}).get("cards") or []


# every effect text on any card of any state (planet and non-planet), plus the empty text
effects = {""}
for gs in ALL:
    for val in gs.values():
        if isinstance(val, dict) and isinstance(val.get("cards"), list):
            for c in val["cards"]:
                eff = (c.get("value") or {}).get("effect")
                if isinstance(eff, str):
                    effects.add(eff)

rows = []
for eff in sorted(effects):
    rows.append({"fn": "planet_level", "args": {"effect": eff}, "ret": (lambda p: None if p is None else list(p))(jokers.planet_level(eff))})

pool = []  # recent plays, decisions._recent shape, last 5
for n, gs in enumerate(ALL):
    rows.append({"fn": "play_forecast_supported", "args": {"gs": n}, "ret": jokers.play_forecast_supported(gs)})
    hand = cards_of(gs, "hand")
    rnd = gs.get("round") or {}
    if gs.get("state") == "SELECTING_HAND" and hand:
        for k in range(1, min(5, len(hand)) + 1):
            rows.append({"fn": "after_discard", "args": {"gs": n, "discarded": hand[:k], "drawn": k},
                         "ret": [ent(e) for e in jokers.after_discard(gs, hand[:k], k)]})
        for k in range(0, min(5, len(hand)) + 1):
            rows.append({"fn": "after_play", "args": {"gs": n, "drawn": k},
                         "ret": [ent(e) for e in jokers.after_play(gs, k)]})
        pool = (pool + [{"infos": [card_info(c) for c in hand], "combo": list(range(min(5, len(hand)))),
                         "rnd": {"hands_left": rnd.get("hands_left"), "discards_left": rnd.get("discards_left")}}])[-5:]
    if gs.get("state") == "SHOP":
        recents = [[], pool[-2:], pool[-5:]]
        for r in recents:
            rows.append({"fn": "sample_plays", "args": {"gs": n, "recent": r},
                         "ret": [[list(s[0]), list(s[1]), s[2], list(s[3]), s[4]] for s in jokers.sample_plays(gs, r)]})
        owned = cards_of(gs, "jokers")
        for c in cards_of(gs, "shop"):
            lv = jokers.planet_level((c.get("value") or {}).get("effect"))
            for r in recents:
                args = {"gs": n, "recent": r, "add": c, "remove": None, "level_up": None}
                rows.append({"fn": "gain", "args": args, "ret": jokers.gain(gs, r, add=c)})
                if lv is not None:
                    args = {"gs": n, "recent": r, "add": None, "remove": None, "level_up": list(lv)}
                    rows.append({"fn": "gain", "args": args, "ret": jokers.gain(gs, r, level_up=lv)})
        for j in range(len(owned)):
            args = {"gs": n, "recent": recents[2], "add": None, "remove": j, "level_up": None}
            rows.append({"fn": "gain", "args": args, "ret": jokers.gain(gs, recents[2], remove=j)})

counts = {}
for r in rows:
    counts[r["fn"]] = counts.get(r["fn"], 0) + 1
print(len(rows), "rows", counts)
states.write_fixture("jokers_rest", rows)
