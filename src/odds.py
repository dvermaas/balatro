"""Draw odds for discard plans (v9): chance to hit the plan's goal and the score if it hits.

Uses only order-free facts about the draw pile (which cards remain, never their order: the pile is
in draw order, see CLAUDE.md). Hypergeometric odds for drawing as many cards as we discard.

Part of the prompt version hash (see recorder.PROMPT_VERSION).
"""

from itertools import combinations
from math import comb

from hands import RANK_ORDER, classify


def _p_none(n, k, d):
    """P(no card from a group of k) when drawing d of n."""
    return comb(n - k, d) / comb(n, d) if d <= n else 0.0


def p_at_least(n, k, d, need):
    """Hypergeometric P(X >= need): X = cards from a group of k among d drawn from n."""
    d = min(d, n)
    return sum(comb(k, x) * comb(n - k, d - x) for x in range(need, min(k, d) + 1)) / comb(n, d) if d else float(need <= 0)


def p_all_ranks(n, counts, d):
    """P(drawing at least one of each listed rank group) by inclusion-exclusion."""
    d = min(d, n)
    total = 0.0
    for r in range(len(counts) + 1):
        for sub in combinations(counts, r):
            total += (-1) ** r * _p_none(n, sum(sub), d)
    return max(0.0, total)


def annotate(discards, hand_infos, deck_infos, score_fn):
    """Add chance_to_hit, goal and score_if_hit to each discard plan from hands.discard_candidates.

    score_fn(cards) scores a list of card infos played as a hand (jokers included)."""
    n = len(deck_infos)
    for dsc in discards:
        keep = [i for i in range(len(hand_infos)) if i not in dsc["cards"]]
        d = len(dsc["cards"])
        kind, arg = dsc.get("goal", (None, None))
        chance, filler = None, []
        if kind == "flush":
            held = [i for i in keep if hand_infos[i]["suit"] == arg or hand_infos[i]["enh"] == "WILD"]
            pool = sorted((c for c in deck_infos if c["suit"] == arg and c["rank"]),
                          key=lambda c: RANK_ORDER.index(c["rank"]))
            need = max(0, 5 - len(held))
            chance = p_at_least(n, len(pool), d, need)
            mid = len(pool) // 2
            filler = pool[max(0, mid - need // 2):][:need]
            hit = [hand_infos[i] for i in held][:5] + filler
        elif kind == "straight":
            missing = arg
            groups = [[c for c in deck_infos if c["rank"] == r] for r in missing]
            chance = p_all_ranks(n, [len(g) for g in groups], d)
            filler = [g[0] for g in groups if g]
            ranks = set()
            hit = []
            for i in keep:
                r = hand_infos[i]["rank"]
                if r in arg_window(missing, hand_infos, keep) and r not in ranks:
                    ranks.add(r)
                    hit.append(hand_infos[i])
            hit += filler
        elif kind in ("sets", "high"):
            ranks = arg
            pool = [c for c in deck_infos if c["rank"] in ranks]
            chance = 1 - _p_none(n, len(pool), min(d, n))
            best = max(pool, key=lambda c: RANK_ORDER.index(c["rank"]), default=None)
            hit = [hand_infos[i] for i in keep if best and hand_infos[i]["rank"] in ranks][:4] + ([best] if best else [])
        else:
            continue
        dsc["chance_to_hit"] = round(chance, 2)
        if hit and chance:
            name, _ = classify(hit[:5])
            dsc["goal_hand"] = name
            dsc["score_if_hit"] = score_fn(hit[:5])
    return discards


def arg_window(missing, hand_infos, keep):
    """The 5-rank window a straight plan targets: kept ranks plus the missing ones."""
    return {hand_infos[i]["rank"] for i in keep} | set(missing)
