"""Fixture for native/bot/src/tarots.rs (auto_use, plan, note, cash_outcome, measured): runs/_port/fixtures/tarots.jsonl.gz.

Rows are {"fn", ...args, "ret"}. auto_use runs on every gamestate of both arms (the Rust test reads the
gamestate by (arm, i) from trace::states). The other functions get the call shapes of decisions.py,
jev_bot.py and rollout_items.py, for every "c_" consumable, shop or pack card in SHOP,
SMODS_BOOSTER_OPENED and SELECTING_HAND states (capped to ~1500 gamestates per arm, evenly spaced):
  plan(key, hand_cards, b) for the dealt hand and two shorter prefixes, with the build decide() returned
  and synthetic flush / pair builds (so every suit and WILD branch runs);
  note(c, b, money, jokers) and cash_outcome(c, money, cost, jokers);
  measured(c, gs, recent, b) with recent=[] and with four synthetic plays of the hand.
A call that raises in Python is recorded as {"raises": "<ExceptionType>"}.

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/tarots.py
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # scripts/port: the states module
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import build  # noqa: E402
import tarots  # noqa: E402
from hands import card_info, _d  # noqa: E402

CAP = 1500
ARMS = ("bot-forecast-items", "bot-forecast-long")
ITEM_STATES = ("SHOP", "SMODS_BOOSTER_OPENED", "SELECTING_HAND")
SYNTH_BUILDS = [{"hand": "Flush", "suit": s} for s in ("H", "D", "C", "S")] + [
    {"hand": "Pair", "suit": None}, {"hand": "Flush", "suit": None}]


def run(fn, **kw):
    """(ret, None) or (None, exception name)."""
    try:
        return fn(**kw), None
    except Exception as e:  # noqa: BLE001
        return None, type(e).__name__


def recent_plays(gs):
    hand = list(_d(gs.get("hand")).get("cards") or [])
    rnd = gs.get("round") or {}
    rnd = {"hands_left": rnd.get("hands_left"), "discards_left": rnd.get("discards_left")}
    if len(hand) < 3:
        return []
    combos = [[0, 1], [0, 1, 2], [1, 2, 3, 4], [0, 2, 4]]
    return [{"infos": [card_info(c) for c in hand], "combo": [i for i in combo if i < len(hand)],
             "rnd": rnd} for combo in combos]


def item_cards(gs):
    """(c, state) for every c_ card in the consumables, shop or pack areas of an item state."""
    st = gs.get("state")
    if st not in ITEM_STATES:
        return []
    areas = ["consumables"]
    if st == "SHOP":
        areas.append("shop")
    if st == "SMODS_BOOSTER_OPENED":
        areas.append("pack")
    out = []
    for a in areas:
        for c in _d(gs.get(a)).get("cards") or []:
            key = c.get("key") or ""
            if key.startswith("c_"):
                out.append({"key": key, "label": c.get("label")})
    return out


rows = []
counts = {}


def add(row, ret, exc):
    if exc:
        row = dict(row, raises=exc)
        counts["raised_" + exc] = counts.get("raised_" + exc, 0) + 1
    else:
        row = dict(row, ret=ret)
    rows.append(row)
    counts[row["fn"]] = counts.get(row["fn"], 0) + 1


for arm in ARMS:
    arm_states = states.states(arm)
    # auto_use: every gamestate
    for i, gs in enumerate(arm_states):
        ret, exc = run(tarots.auto_use, gs=gs)
        add({"fn": "auto_use", "arm": arm, "i": i}, ret, exc)

    # per-item functions: evenly spaced gamestates
    step = max(1, len(arm_states) // CAP)
    for i in range(0, len(arm_states), step):
        gs = arm_states[i]
        items = item_cards(gs)
        if not items:
            continue
        hand = list(_d(gs.get("hand")).get("cards") or [])
        money = gs.get("money") or 0
        jokers = list(_d(gs.get("jokers")).get("cards") or [])
        b_real = build.decide(gs)
        hands_variants = []
        for h in (hand, hand[:1], hand[:2]):
            if h not in hands_variants:
                hands_variants.append(h)
        builds = [b_real] + SYNTH_BUILDS
        recent = recent_plays(gs)
        for c in items:
            key = c["key"]
            for h in hands_variants:
                for b in builds:
                    ret, exc = run(tarots.plan, key=key, hand_cards=h, b=b)
                    add({"fn": "plan", "key": key, "hand": h, "b": b}, ret, exc)
            for b in builds[:2]:
                ret, exc = run(tarots.note, c=c, b=b, money=money, jokers=jokers)
                add({"fn": "note", "c": c, "b": b, "money": money, "jokers": jokers}, ret, exc)
            for cost in (0, 3):
                ret, exc = run(tarots.cash_outcome, c=c, money=money, cost=cost, jokers=jokers)
                add({"fn": "cash_outcome", "c": c, "money": money, "cost": cost, "jokers": jokers}, ret, exc)
            for rec in ([], recent):
                for b in (b_real, {"hand": "Flush", "suit": "H"}):
                    ret, exc = run(tarots.measured, c=c, gs=gs, recent=rec, b=b)
                    add({"fn": "measured", "arm": arm, "i": i, "c": c, "recent": rec, "b": b}, ret, exc)

states.write_fixture("tarots", rows)
print(counts)
