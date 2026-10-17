"""Unit fixture for native/bot/src/prompts.rs and strategy.rs: runs/_port/fixtures/strategy.jsonl.gz.

Rows are {"fn", "args", "ret"}. A gamestate is referenced by its index "gs" in ALL, the
concatenation of the states of both arms (bot-forecast-items, then bot-forecast-long), which is the
order of jevbot::trace::states in Rust. Sets are written as sorted lists.

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/strategy.py
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # scripts/port: the states module
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import prompts  # noqa: E402
import strategy  # noqa: E402

ALL = states.states("bot-forecast-items") + states.states("bot-forecast-long")
AREAS = ("hand", "jokers", "consumables", "shop", "pack", "missing_area")
rows = []


def add(fn, args, ret):
    rows.append({"fn": fn, "args": args, "ret": ret})


# prompts: every module-level constant (strings, lists, dicts), in the order of the module's names.
for name in sorted(n for n in dir(prompts) if n.isupper()):
    add(f"prompts.{name}", {}, getattr(prompts, name))

add("HAND_NAMES", {}, list(strategy.HAND_NAMES))
add("CONTAINED_IN", {}, {k: sorted(v) for k, v in strategy.CONTAINED_IN.items()})
add("COPIERS", {}, list(strategy.COPIERS))

# joker_kind for every distinct effect text in the states (plus None).
effect_texts = [None]
seen_texts = {None}
for gs in ALL:
    for c in strategy._cards(gs, "jokers"):
        e = strategy._d(c.get("value")).get("effect")
        if e not in seen_texts:
            seen_texts.add(e)
            effect_texts.append(e)
for e in effect_texts:
    add("joker_kind", {"effect": e}, strategy.joker_kind(e))

seen_tags = set()
n_states = 0
for i, gs in enumerate(ALL):
    n_states += 1
    mains = strategy.main_hands(gs)
    add("main_hands", {"gs": i}, mains)
    for n in (1, 2, 3):
        add("main_hands", {"gs": i, "n": n}, strategy.main_hands(gs, n))

    effects = [strategy._d(c.get("value")).get("effect") for c in strategy._cards(gs, "jokers")]
    add("engine", {"effects": effects}, strategy.engine(effects))

    for e in effects:
        key = (json.dumps(e), tuple(mains))
        if key in seen_tags:
            continue
        seen_tags.add(key)
        add("joker_tags", {"effect": e, "mains": mains}, strategy.joker_tags(e, mains))

    for area in AREAS:
        add("_cards", {"gs": i, "area": area}, strategy._cards(gs, area))

    r = strategy.auto_action(gs)
    add("auto_action", {"gs": i}, list(r) if r is not None else None)

for money in range(-5, 201):
    add("interest", {"money": money}, strategy.interest(money))

path = states.write_fixture("strategy", rows)
print(f"{len(rows)} rows, {n_states} states -> {path}")
