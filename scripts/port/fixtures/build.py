"""Fixture for native/bot/src/build.rs (decide, describe, card_value, fits): runs/_port/fixtures/build.jsonl.gz.

Rows are {"fn", "args", "ret"}. Inputs: every gamestate of both arms (bot-forecast-items, then
bot-forecast-long), evenly spaced to at most ~1500 per arm. decide(gs) is the call decisions.py and
tarots.py make; describe, card_value and fits get the build it returned, with the call shapes of
decisions.py (fits gets the discard plan goals) and tarots.py (card_value gets hands.card_info dicts).
Synthetic card infos cover the STONE, no-rank, debuff and WILD branches.

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/build.py
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # scripts/port: the states module
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import build  # noqa: E402
from hands import card_info  # noqa: E402

CAP = 1500
ALL = []
for arm in ("bot-forecast-items", "bot-forecast-long"):
    arm_states = states.states(arm)
    step = max(1, len(arm_states) // CAP)
    ALL += arm_states[::step][:CAP]

GOALS = [None, ["flush", "H"], ["flush", "S"], ["flush", None], ["sets", None], ["straight", None],
         ["high", None], ["other", None]]
SYNTH_BUILDS = [{"hand": "Flush", "suit": "H"}, {"hand": "Flush", "suit": None}, {"hand": "Pair", "suit": None},
                {"hand": "Pair", "suit": "C"}, {"hand": "Two Pair", "suit": None},
                {"hand": "Straight", "suit": None}, {"hand": "High Card", "suit": None}]
SYNTH_INFOS = [
    {"rank": None, "suit": None, "enh": "STONE", "edition": None, "seal": None, "debuff": False},
    {"rank": "A", "suit": "S", "enh": "WILD", "edition": None, "seal": None, "debuff": False},
    {"rank": "7", "suit": "H", "enh": None, "edition": "FOIL", "seal": None, "debuff": False},
    {"rank": "K", "suit": "D", "enh": None, "edition": None, "seal": "RED", "debuff": False},
    {"rank": "T", "suit": "C", "enh": "", "edition": "", "seal": "", "debuff": False},
    {"rank": "5", "suit": "H", "enh": "GLASS", "edition": None, "seal": None, "debuff": True},
    {"rank": "", "suit": "H", "enh": None, "edition": None, "seal": None, "debuff": False},
    {"rank": "2", "suit": "S", "enh": None, "edition": None, "seal": None, "debuff": False},
]

rows = []
for g in ALL:
    b = build.decide(g)
    rows.append({"fn": "decide", "args": [g], "ret": b})
    rows.append({"fn": "describe", "args": [b], "ret": build.describe(b)})
    cards = list(g.get("hand", {}).get("cards") or [])[:3] + list(g.get("cards", {}).get("cards") or [])[:3]
    for c in cards:
        info = card_info(c)
        rows.append({"fn": "card_value", "args": [info, b], "ret": build.card_value(info, b)})
    for goal in GOALS:
        rows.append({"fn": "fits", "args": [goal, b], "ret": build.fits(goal, b)})

for b in SYNTH_BUILDS:
    rows.append({"fn": "describe", "args": [b], "ret": build.describe(b)})
    for info in SYNTH_INFOS:
        rows.append({"fn": "card_value", "args": [info, b], "ret": build.card_value(info, b)})
    for goal in GOALS:
        rows.append({"fn": "fits", "args": [goal, b], "ret": build.fits(goal, b)})

states.write_fixture("build", rows)
from collections import Counter  # noqa: E402
print(len(ALL), "states,", len(rows), "rows", dict(Counter(r["fn"] for r in rows)))
print("builds", dict(Counter(r["ret"]["hand"] for r in rows if r["fn"] == "decide")))
