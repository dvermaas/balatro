"""Boss readiness (v13 draft): can our current deck + jokers + hand levels beat the next blinds?

Samples random hands from the full deck (order-free, like odds.forecast: the pile is sorted by
observable properties before sampling, never read in draw order) and scores the best play in each
with jokers.score. Compares that typical per-hand power with the per-hand pace the upcoming blinds
need. The shop policy can then spend when we're short ("invest") and bank interest when we're
comfortably ahead ("bank"), instead of fixed money rules.

Not wired into the hashed decision path yet; see docs/v13-concept.md.
"""

from random import Random
from statistics import median

import jokers
from hands import RANK_ORDER, _d, best_score, card_info

# Balatro's base blind amounts at White stake (get_blind_amount, scaling 1), antes 1-8.
BASE = [300, 800, 2000, 5000, 11000, 20000, 35000, 50000]
BLIND_MULT = {"small": 1.0, "big": 1.5, "boss": 2.0}


def _key(c):
    return tuple(str(c.get(k) or "") for k in ("rank", "suit", "enh", "edition", "seal", "debuff"))


def base_amount(ante):
    return BASE[min(max(ante, 1), 8) - 1] if ante <= 8 else BASE[-1] * 2 ** (ante - 8)


def upcoming(gs, ahead=3):
    """The next `ahead` blinds as (label, target): this ante's unbeaten blinds, then the next ante's."""
    out = []
    order = ("small", "big", "boss")
    blinds = _d(gs.get("blinds"))
    ante = gs.get("ante_num") or 1
    for k in order:
        b = _d(blinds.get(k))
        if b and b.get("status") not in ("DEFEATED", "SKIPPED"):
            out.append((f"ante {ante} {b.get('name') or k}", b.get("score") or base_amount(ante) * BLIND_MULT[k]))
    a = ante + 1
    while len(out) < ahead:
        for k in order:
            out.append((f"ante {a} {k} blind", int(base_amount(a) * BLIND_MULT[k])))
        a += 1
    return out[:ahead]


def power(gs, samples=24, js=None, levels=None):
    """Distribution of the best single-hand score from random hands of our current deck."""
    deck = [card_info(c) for c in (_d(gs.get("cards")).get("cards") or [])]
    deck += [card_info(c) for c in (_d(gs.get("hand")).get("cards") or [])]
    deck = sorted(deck, key=_key)
    size = _d(gs.get("hand")).get("limit") or 8
    if len(deck) < size:
        return None
    js = jokers.build(gs) if js is None else js
    levels = _d(gs.get("hands")) if levels is None else levels
    rnd = {"hands_left": 2, "discards_left": 1}

    def scorer(infos, combo, name, sc):
        return jokers.score(infos, combo, name, sc, levels, js, rnd)

    rng = Random(13013)  # fixed local seed: the same deck always gets the same estimate
    scores = []
    for _ in range(samples):
        hand = rng.sample(deck, size)
        hand.sort(key=lambda c: (-(RANK_ORDER.index(c["rank"]) + 2) if c.get("rank") and c.get("enh") != "STONE"
                                 else 0, _key(c)))
        scores.append(best_score(hand, scorer))
    scores.sort()
    return {"median": median(scores), "p20": scores[int(0.2 * (len(scores) - 1))], "samples": len(scores)}


def assess(gs, hands_per_round=None, samples=24):
    """Readiness for the next blinds: ratio of typical per-hand power to the per-hand pace each needs."""
    p = power(gs, samples)
    if not p:
        return None
    hands = hands_per_round or 4
    rows = []
    for label, target in upcoming(gs):
        need = target / hands
        rows.append({"blind": label, "target": int(target), "ratio": round(p["median"] / need, 2)})
    worst = min(r["ratio"] for r in rows)
    # v14: the wall. Targets grow 2-2.5x an ante from ante 3, so judge lasting power against the boss
    # WALL_AHEAD antes on (v13 data: readiness peaks ~1.0 at ante 3 and falls to ~0.5 by ante 6).
    a = min((gs.get("ante_num") or 1) + WALL_AHEAD, 8)
    target = base_amount(a) * BLIND_MULT["boss"]
    wall = {"blind": f"ante {a} boss", "target": int(target), "ratio": round(p["median"] / (target / hands), 2)}
    return {"typical_hand": int(p["median"]), "weak_hand": int(p["p20"]), "hands": hands,
            "blinds": rows, "worst_ratio": worst, "wall": wall}


WALL_AHEAD = 2


def lasting(gain, kind, wall_ratio):
    """A flat +Mult/+Chips gain shrinks as the rest of the build grows to meet the wall; xMult and
    scaling jokers keep (or grow) their share. Rough model: flat gains divide by sqrt(growth needed)."""
    if not gain or kind in ("xmult", "scaling") or not wall_ratio:
        return gain
    return gain / max(1.0, 1 / wall_ratio) ** 0.5
