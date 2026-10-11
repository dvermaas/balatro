"""The run's committed build (v10): one target hand type (and suit for flushes) that discards, tarot
targets and shop choices steer toward. Top LLM players commit like this ("Flush, level 9").

Re-decided at every decision from hand levels, jokers and deck makeup, so it settles as the run
develops. Part of the prompt version hash (see recorder.PROMPT_VERSION).
"""

import re
from collections import Counter

from hands import RANK_ORDER, _d, card_info
from strategy import main_hands

PAIR_FAMILY = {"Pair", "Two Pair", "Three of a Kind", "Full House", "Four of a Kind", "Five of a Kind"}
SUITS = {"H": "Hearts", "D": "Diamonds", "C": "Clubs", "S": "Spades"}


def _all_cards(gs):
    cards = []
    for area in ("cards", "hand"):
        cards += [card_info(c) for c in _d(gs.get(area)).get("cards") or []]
    return [c for c in cards if c["rank"]]


def decide(gs):
    """{"hand": target hand type, "suit": suit letter for flush builds or None}."""
    hands = _d(gs.get("hands"))
    effects = " ".join(_d(c.get("value")).get("effect") or "" for c in _d(gs.get("jokers")).get("cards") or [])
    deck = _all_cards(gs)
    suit_counts = Counter(c["suit"] for c in deck)
    for letter, name in SUITS.items():  # suit jokers (Greedy, Lusty, ...) pull the build toward their suit
        suit_counts[letter] += 4 * len(re.findall(name[:-1], effects))
    suit = max(SUITS, key=lambda s: suit_counts[s])
    fl = hands.get("Flush", {})
    flush_signal = ((fl.get("level") or 1) - 1) + (fl.get("played") or 0) / 3 \
        + len(re.findall(r"Flush|Smeared|suit", effects))
    mains = main_hands(gs)
    if flush_signal >= 1 or (mains and mains[0] in ("Flush", "Straight Flush", "Flush House", "Flush Five")):
        return {"hand": "Flush", "suit": suit}
    return {"hand": mains[0] if mains else "Pair", "suit": None}


def describe(b):
    return f"{b['hand']}" + (f" in {SUITS[b['suit']]}" if b.get("suit") else "")


def card_value(info, b):
    """How much a card matters to the build: high = keep and enhance, low = junk to convert or remove."""
    if info.get("debuff"):
        return -1.0
    if info["enh"] == "STONE" or not info["rank"]:
        return 0.3
    v = (RANK_ORDER.index(info["rank"]) + 2) / 14
    if info["enh"] or info["edition"] or info["seal"]:
        v += 0.5
    if b.get("suit"):
        v += 2.0 if info["suit"] == b["suit"] or info["enh"] == "WILD" else 0.0
    return v


def fits(plan_goal, b):
    """Does a discard plan's goal fit the build?"""
    kind, arg = plan_goal or (None, None)
    if b["hand"] == "Flush":
        return kind == "flush" and arg == b.get("suit")
    if b["hand"] in PAIR_FAMILY:
        return kind == "sets"
    if b["hand"] == "Straight":
        return kind == "straight"
    return kind == "high"
