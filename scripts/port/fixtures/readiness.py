"""Fixture for native/bot/src/readiness.rs (upcoming, power, assess, play_mix, lasting, base_amount):
runs/_port/fixtures/readiness.jsonl.gz.

Rows are {"fn", "args", "ret"}. A gamestate is referenced by its index "gs" in ALL, the concatenation of
the SHOP states then the SELECTING_HAND states of both arms (bot-forecast-items, then bot-forecast-long),
which is the order of jevbot::trace::states in Rust. Rows are evaluated in file order: play_mix keeps a
process-wide cache (_MIX_CACHE), so the Rust side replays the rows in the same order.
A spread of at most ~1500 states is used (evenly spaced over ALL).

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/readiness.py
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # scripts/port: the states module
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import readiness  # noqa: E402

ALL = (states.states("bot-forecast-items", "SHOP") + states.states("bot-forecast-long", "SHOP")
       + states.states("bot-forecast-items", "SELECTING_HAND") + states.states("bot-forecast-long", "SELECTING_HAND"))
CAP = 1500
step = max(1, len(ALL) // CAP)
PICK = list(range(0, len(ALL), step))[:CAP]
rows = []


def add(fn, args, ret):
    rows.append({"fn": fn, "args": args, "ret": ret})


for a in range(-1, 14):
    add("base_amount", {"ante": a}, readiness.base_amount(a))

for gain in (None, 0, 1.5, 3, -2.0, 0.25):
    for kind in (None, "flat", "xmult", "scaling"):
        for wr in (None, 0.0, 0.3, 0.5, 1.0, 2.0):
            add("lasting", {"gain": gain, "kind": kind, "wall_ratio": wr},
                readiness.lasting(gain, kind, wr))

for gi in PICK:
    gs = ALL[gi]
    add("upcoming", {"gs": gi}, [[label, target] for label, target in readiness.upcoming(gs)])
    add("power", {"gs": gi, "samples": 24}, readiness.power(gs, 24))
    add("power", {"gs": gi, "samples": 7}, readiness.power(gs, 7))
    add("assess", {"gs": gi, "samples": 24}, readiness.assess(gs))
    if gi % 10 == 0:
        add("assess", {"gs": gi, "samples": 24, "hands": 3}, readiness.assess(gs, hands_per_round=3))
    add("play_mix", {"gs": gi}, readiness.play_mix(gs))

path = states.write_fixture("readiness", rows)
print(len(ALL), "states,", len(PICK), "picked,", len(rows), "rows ->", path)
