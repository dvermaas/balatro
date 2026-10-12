"""Discard forecasts (v12), plus the original v9 analytic draw-odds helpers.

Uses only order-free facts about the draw pile (which cards remain, never their order: the pile is
in draw order, see CLAUDE.md). Forecasts score complete future hands after uniform unordered draws.

Part of the prompt version hash (see recorder.PROMPT_VERSION).
"""

from itertools import combinations
from math import comb, fsum
from random import Random

from hands import RANK_ORDER, best_score, classify


def _card_key(c):
    # All observable scoring properties, never card IDs or the pile's input position.
    return tuple(str(c.get(k) or "") for k in ("rank", "suit", "enh", "edition", "seal", "debuff"))


def forecast(discards, hand_infos, deck_infos, scorer_factory, allowed=None, *,
             need=0, pace=0, draw_count=None, samples=32):
    """Model the best next play over unordered draws, including misses and held cards.

    Small draw spaces are enumerated exactly; otherwise use reproducible uniform samples.
    These are one-discard forecasts under our scoring model, not full-round win probabilities.
    scorer_factory(discarded_infos, number_drawn) accounts for the spent discard and joker changes.
    """
    if samples < 1:
        raise ValueError("samples must be positive")
    deck = sorted(deck_infos, key=_card_key)
    for plan in discards:
        tossed = set(plan["cards"])
        kept = [c for i, c in enumerate(hand_infos) if i not in tossed]
        discarded = [hand_infos[i] for i in sorted(tossed)]
        requested = draw_count(len(tossed)) if callable(draw_count) else draw_count
        count = min(len(deck), max(0, len(tossed) if requested is None else requested))
        possibilities = comb(len(deck), count)
        exact = possibilities <= samples
        if exact:
            draws = combinations(range(len(deck)), count)
        else:
            rng = Random(12012)  # common random numbers reduce noise between competing plans
            draws = (rng.sample(range(len(deck)), count) for _ in range(samples))
        scorer = scorer_factory(discarded, count)
        scores = []
        for draw in draws:
            future = kept + [deck[i] for i in draw]
            # The live bot uses rank-sorted hands. Stable modifier ties also avoid pile-order leaks.
            future.sort(key=lambda c: (-(RANK_ORDER.index(c["rank"]) + 2)
                        if c.get("rank") and c.get("enh") != "STONE" else 0, _card_key(c)))
            scores.append(best_score(future, scorer, allowed))
        scores.sort()
        plan.update(expected_score=round(fsum(scores) / len(scores)),
                    low_score=scores[int((len(scores) - 1) * 0.2)],
                    chance_to_clear=sum(s >= need for s in scores) / len(scores),
                    chance_to_meet_pace=sum(s >= pace for s in scores) / len(scores),
                    samples=len(scores), exact_draws=exact, drawn=count)
    return discards


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
    deck_infos = sorted(deck_infos, key=_card_key)
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
