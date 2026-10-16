"""Poker-hand evaluation and candidate move generation.

Code enumerates moves; Jev picks between them. The active bot supplies a
joker-aware scorer. Without one, estimates use hand levels and card modifiers.
"""

from collections import Counter
from itertools import combinations

import native

RANK_ORDER = "23456789TJQKA"
RANK_NAMES = {"T": "10", "J": "Jack", "Q": "Queen", "K": "King", "A": "Ace"}
SUIT_NAMES = {"H": "Hearts", "D": "Diamonds", "C": "Clubs", "S": "Spades"}
RANK_CHIPS = {r: int(r) for r in "23456789"} | {"T": 10, "J": 10, "Q": 10, "K": 10, "A": 11}


def _d(x):
    # BalatroBot serialises empty Lua tables as [] rather than {}.
    return x if isinstance(x, dict) else {}


def card_info(c):
    v, m, s = _d(c.get("value")), _d(c.get("modifier")), _d(c.get("state"))
    return {
        "rank": v.get("rank"),
        "suit": v.get("suit"),
        "enh": m.get("enhancement"),
        "edition": m.get("edition"),
        "seal": m.get("seal"),
        "debuff": bool(s.get("debuff")),
    }


def describe(c):
    i = card_info(c) if "value" in c else c
    if i["enh"] == "STONE" or not i["rank"]:
        name = "Stone card"
    else:
        name = f"{RANK_NAMES.get(i['rank'], i['rank'])} of {SUIT_NAMES.get(i['suit'], i['suit'])}"
    extras = [x.lower() for x in (i["enh"] if i["enh"] != "STONE" else None, i["edition"]) if x]
    if i["seal"]:
        extras.append(f"{i['seal'].lower()} seal")
    if i["debuff"]:
        extras.append("debuffed")
    return name + (f" ({', '.join(extras)})" if extras else "")


def _rank_value(i):
    return 10 if i["enh"] == "STONE" or not i["rank"] else RANK_ORDER.index(i["rank"]) + 2


def _is_straight(ranks):
    idx = sorted({RANK_ORDER.index(r) for r in ranks})
    if len(idx) != 5:
        return False
    return idx[-1] - idx[0] == 4 or idx == [0, 1, 2, 3, 12]  # A-2-3-4-5


_CLASSIFY = {}


def classify(cards):
    """Return (hand name, positions of scoring cards) for 1-5 card infos. v22: memoised on the cards'
    rank, suit and enhancement (all it reads)."""
    key = tuple((c["rank"], c["suit"], c["enh"]) for c in cards)
    hit = _CLASSIFY.get(key)
    if hit is None:
        if len(_CLASSIFY) > 200000:
            _CLASSIFY.clear()
        hit = _CLASSIFY[key] = _classify(cards)
    return hit[0], list(hit[1])


def _classify(cards):
    ranked = [p for p, c in enumerate(cards) if c["enh"] != "STONE" and c["rank"]]
    stones = [p for p in range(len(cards)) if p not in ranked]
    by_rank = Counter(cards[p]["rank"] for p in ranked)
    counts = sorted(by_rank.values(), reverse=True) or [0]
    five = len(cards) == 5 and len(ranked) == 5
    flush = five and any(all(cards[p]["suit"] == s or cards[p]["enh"] == "WILD" for p in ranked) for s in "HDCS")
    straight = five and _is_straight([cards[p]["rank"] for p in ranked])

    def with_count(n):
        return [p for p in ranked if by_rank[cards[p]["rank"]] >= n]

    if counts[0] == 5:
        name, scoring = ("Flush Five" if flush else "Five of a Kind"), ranked
    elif flush and counts[:2] == [3, 2]:
        name, scoring = "Flush House", ranked
    elif straight and flush:
        name, scoring = "Straight Flush", ranked
    elif counts[0] == 4:
        name, scoring = "Four of a Kind", with_count(4)
    elif counts[:2] == [3, 2]:
        name, scoring = "Full House", ranked
    elif flush:
        name, scoring = "Flush", ranked
    elif straight:
        name, scoring = "Straight", ranked
    elif counts[0] == 3:
        name, scoring = "Three of a Kind", with_count(3)
    elif counts[:2] == [2, 2]:
        name, scoring = "Two Pair", with_count(2)
    elif counts[0] == 2:
        name, scoring = "Pair", with_count(2)
    else:
        name = "High Card"
        scoring = [max(ranked, key=lambda p: _rank_value(cards[p]))] if ranked else []
    return name, scoring + stones


def estimate(cards, name, scoring, hand_levels):
    h = hand_levels.get(name, {})
    chips, mult, xmult = h.get("chips", 0), h.get("mult", 1), 1.0
    for p in scoring:
        c = cards[p]
        if c["debuff"]:
            continue
        for _ in range(2 if c["seal"] == "RED" else 1):
            chips += 50 if c["enh"] == "STONE" or not c["rank"] else RANK_CHIPS[c["rank"]]
            chips += {"BONUS": 30}.get(c["enh"], 0) + {"FOIL": 50}.get(c["edition"], 0)
            mult += {"MULT": 4}.get(c["enh"], 0) + {"HOLO": 10}.get(c["edition"], 0)
            xmult *= {"GLASS": 2}.get(c["enh"], 1) * {"POLYCHROME": 1.5}.get(c["edition"], 1)
    return int(chips * mult * xmult)


def evaluated_plays(infos, hand_levels, scorer=None, allowed=None):
    """Score every subset before pruning; held cards and play length can change its value.

    v23: a jokers.Scorer (with decisions.BossAllow or no filter) runs in the native kernel when built."""
    rows = native.evaluate(scorer, infos, allowed)
    if rows is not None:
        yield from rows
        return
    for r in range(1, min(5, len(infos)) + 1):
        for combo in combinations(range(len(infos)), r):
            cs = [infos[i] for i in combo]
            name, sc = classify(cs)
            if allowed is not None and not allowed(combo, name):
                continue
            score = scorer(infos, combo, name, sc) if scorer else estimate(cs, name, sc, hand_levels)
            yield score, name, combo, frozenset(combo[p] for p in sc)


def best_score(infos, scorer, allowed=None):
    """Best modeled score in a complete future hand, including cards left held."""
    best = native.best_score(scorer, infos, allowed)
    if best is not None:
        return best
    return max((row[0] for row in evaluated_plays(infos, {}, scorer, allowed)), default=0)


def best_play(infos, hand_levels, scorer, allowed=None):
    """max((score, hand name)) over the plays, (0, None) without any: ties go to the larger name."""
    best = native.best_play(scorer, infos, allowed)
    if best is not None:
        return best
    return max(((s, n) for s, n, _, _ in evaluated_plays(infos, hand_levels, scorer, allowed)), default=(0, None))


def play_candidates(hand_cards, hand_levels, limit=8, scorer=None, allowed=None):
    """Best play per (hand type, scoring cards); cycle low junk only when scores tie.

    scorer(infos, combo, name, scoring) replaces the joker-blind estimate when given (jokers.score)."""
    infos = [card_info(c) for c in hand_cards]
    best = {}
    for est, name, combo, scoring in evaluated_plays(infos, hand_levels, scorer, allowed):
        junk = [i for i in combo if i not in scoring]
        pref = (est, len(combo), -sum(_rank_value(infos[i]) for i in junk))
        key = (name, scoring)
        if key not in best or pref > best[key][1]:
            best[key] = (est, pref, list(combo))
    ranked = sorted(((v[0], k[0], v[2], k[1]) for k, v in best.items()), key=lambda x: -x[0])
    # One best play per hand type first, then fill with the next best overall.
    picked, seen = [], set()
    for row in ranked:
        if row[1] not in seen:
            seen.add(row[1])
            picked.append(row)
    per_type = Counter(row[1] for row in picked)
    for row in ranked:
        if len(picked) >= limit:
            break
        if row not in picked and per_type[row[1]] < 2:
            per_type[row[1]] += 1
            picked.append(row)
    picked = sorted(picked, key=lambda x: -x[0])[:limit]
    return [
        {
            "hand": name,
            "cards": idxs,
            "estimate": score,
            "scoring": [describe(hand_cards[i]) for i in idxs if i in scoring],
            "kicked": [describe(hand_cards[i]) for i in idxs if i not in scoring],
        }
        for score, name, idxs, scoring in picked
    ]


def discard_candidates(hand_cards, max_discard=5):
    """Discard plans. Debuffed cards (boss effects) count as junk: never kept for a draw, tossed first.
    Each plan carries a goal for odds.annotate()."""
    infos = [card_info(c) for c in hand_cards]
    allidx = list(range(len(infos)))
    live = [i for i in allidx if not infos[i]["debuff"]]

    def toss(keep, label, goal):
        others = sorted((i for i in allidx if i not in keep),
                        key=lambda i: (not infos[i]["debuff"], _rank_value(infos[i])))
        return label, others[:max_discard], goal

    out = []
    for s in "HDCS":
        keep = [i for i in live if infos[i]["suit"] == s or infos[i]["enh"] == "WILD"]
        if 3 <= len(keep) < 5:  # 5+ means the flush is already playable
            out.append(toss(keep, f"Chase a {SUIT_NAMES[s]} flush (holding {len(keep)} of 5)", ("flush", s)))

    by_rank = Counter(infos[i]["rank"] for i in live if infos[i]["rank"])
    sets = [i for i in live if infos[i]["rank"] and by_rank[infos[i]["rank"]] >= 2]
    if sets:
        out.append(toss(sets, "Keep pairs/sets, dig for more of a kind", ("sets", {infos[i]["rank"] for i in sets})))

    # Straight draw: the 5-rank window holding the most distinct ranks (A counts low too).
    best_keep, best_window = [], set()
    for lo in range(-1, 9):
        window = {RANK_ORDER[(lo + k) % 13] for k in range(5)} if lo >= 0 else set("A2345")
        keep, used = [], set()
        for i in live:
            r = infos[i]["rank"]
            if r in window and r not in used:
                keep.append(i)
                used.add(r)
        if len(keep) > len(best_keep):
            best_keep, best_window = keep, window
    if 3 <= len(best_keep) < 5:  # 5 means the straight is already playable
        missing = sorted(best_window - {infos[i]["rank"] for i in best_keep}, key=RANK_ORDER.index)
        out.append(toss(best_keep, f"Chase a straight (holding {len(best_keep)} of 5 ranks)", ("straight", missing)))

    high = sorted(live, key=lambda i: -_rank_value(infos[i]))[: max(0, len(allidx) - max_discard)]
    out.append(toss(high, "Keep high cards, throw the lowest", ("high", {infos[i]["rank"] for i in high})))

    seen, result = set(), []
    for label, cards, goal in out:
        key = frozenset(cards)
        if cards and key not in seen:
            seen.add(key)
            result.append({"label": label, "cards": cards, "goal": goal,
                           "tossed": [describe(hand_cards[i]) for i in cards]})
    return result
