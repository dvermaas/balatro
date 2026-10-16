"""Remaining-round planner (policy lab only, not hashed): pick the play or discard that maximises
P(clear this blind), by sampled rollouts of the rest of the round.

At a SELECTING_HAND decision where the fast estimate says the round is at risk, each candidate action
(the bot's own play candidates from hands.play_candidates and its discard plans from
hands.discard_candidates, plus the fast policy's discard) is applied to R copies of the engine state and
the rest of the round is played with rollout.Policy. Common random numbers: copy k of every candidate
gets the same fresh deck order and the same fresh RNG.

Information: the copies keep what is public (hand, jokers, hand levels, money, blind, chips, hands and
discards left, the deck's CONTENTS) and replace what is hidden: the draw pile is re-sorted into a
canonical order and shuffled with a fresh RNG, and the engine RNG is reseeded, so the real draw order
and the real seed's future are never used. Known gap (as in rollout.Policy): face-down cards in hand
(The House, The Wheel, The Mark, The Fish) are seen by the planner. Production would rebuild the state
from the public gamestate instead of copying the engine state.
"""
from __future__ import annotations

import copy
import random
import time

import rollout
import sim_backend  # noqa: F401  (Jackdaw on sys.path)
from jackdaw.engine.actions import Discard, GamePhase, PlayHand  # noqa: E402
from jackdaw.engine.game import IllegalActionError, step  # noqa: E402
from jackdaw.engine.rng import PseudoRandom  # noqa: E402

import jokers  # noqa: E402  (read-only use of the bot's modules)
from hands import _d, discard_candidates, play_candidates  # noqa: E402

R = 32                 # rollouts per candidate
MAX_PLAYS = 4          # the bot's top play candidates by estimate
MAX_DISCARDS = 6       # discard plans
RISK = 1.5             # plan only when best estimate x hands left < RISK x chips still needed
BUDGET_S = 2.0         # stop adding rollouts (all candidates keep the same count; at least 8)
MARGIN = 0.1           # override the bot only when the pick beats its strongest play's P(clear) by this much
OVERRIDE_SE = 2        # ... and by more than this many paired (common-random-numbers) standard errors


def _canon(c):
    b = c.base
    return (str(getattr(getattr(b, "suit", None), "value", "")), str(getattr(getattr(b, "rank", None), "value", "")),
            str(c.center_key), str(c.edition), str(c.seal), str((c.ability or {}).get("perma_bonus", 0)))


def _fresh(gs, k):
    """A copy with the hidden parts replaced: canonical deck order shuffled by seed k, engine RNG reseeded."""
    g = copy.deepcopy(gs)
    g["deck"].sort(key=_canon)
    random.Random(f"RP{k}").shuffle(g["deck"])
    g["rng"] = PseudoRandom(f"RP{k:06d}")
    return g


def _forced(gs, cards):
    hand = gs["hand"]
    f = [i for i, c in enumerate(hand) if (c.ability or {}).get("forced_selection")]
    if f and f[0] not in cards:  # Cerulean Bell, as BalatroBot / SimBalatro apply it
        return sorted(([f[0]] + list(cards))[:5])
    return list(cards)


def finish_round(gs, pol, max_steps=80):
    """Play the rest of the current round with the fast policy: (cleared, chips / target)."""
    target = getattr(gs.get("blind"), "chips", 0) or 1
    for _ in range(max_steps):
        ph = gs["phase"]
        if ph == GamePhase.ROUND_EVAL or gs.get("won"):
            return True, (gs.get("chips") or 0) / target
        if ph != GamePhase.SELECTING_HAND or not gs["hand"]:
            return False, (gs.get("chips") or 0) / target
        kind, idx = pol.decide(gs)
        try:
            step(gs, (PlayHand if kind == "play" else Discard)(card_indices=tuple(idx)))
        except IllegalActionError:
            forced = [p for p, c in enumerate(gs["hand"]) if c.ability.get("forced_selection")]
            step(gs, PlayHand(card_indices=tuple(forced[:1] or [0])))
    return False, (gs.get("chips") or 0) / target


def candidates(pub, gs, pol):
    """[(kind, cards, label)]: the bot's top plays and its discard plans (plus the fast policy's discard)."""
    import decisions
    hand = _d(pub.get("hand")).get("cards") or []
    info = decisions._round_info(pub)
    boss = info["blind"]
    rnd, js = _d(pub.get("round")), jokers.build(pub)
    levels = decisions._boss_levels(_d(pub.get("hands")), boss)
    scorer = lambda infos, combo, name, sc: jokers.score(infos, combo, name, sc, levels, js, rnd)  # noqa: E731
    allowed = lambda combo, name: decisions._boss_allows(combo, name, _d(pub.get("hands")), boss)  # noqa: E731
    plays = play_candidates(hand, levels, limit=8, scorer=scorer, allowed=allowed) or \
        [{**p, "estimate": 0} for p in play_candidates(hand, levels, scorer=scorer)]
    out, seen = [], set()
    for p in plays[:MAX_PLAYS]:
        key = ("play", tuple(sorted(p["cards"])))
        if key not in seen:
            seen.add(key)
            out.append(("play", list(p["cards"]), f"{p['hand']} ~{p['estimate']}"))
    if (info["discards_left"] or 0) > 0:
        cap = _d(pub.get("hand")).get("highlighted_limit") or 5
        ds = discard_candidates(hand, max_discard=cap)[:MAX_DISCARDS]
        own = pol.discard([rollout._info(c) for c in gs["hand"]],
                          [i for i, c in enumerate(gs["hand"]) if c.ability.get("forced_selection")])
        for d in ds + ([{"cards": own, "label": "fast policy discard"}] if own else []):
            key = ("discard", tuple(sorted(d["cards"])))
            if d["cards"] and key not in seen:
                seen.add(key)
                out.append(("discard", list(d["cards"]), d.get("label", "discard")))
    return out, plays, info


def at_risk(plays, info, margin=1.0):
    need = info["chips_still_needed"] or 0
    best = plays[0]["estimate"] if plays else 0
    if need <= 0 or best >= need * margin:  # a sure clear: the bot's own fast path plays it
        return False
    return best * max(1, info["hands_left"] or 1) < RISK * need


def plan(pub, gs, r=R, budget_s=BUDGET_S):
    """Evaluate the candidate actions; returns a dict with the pick and every candidate's numbers, or None
    when the round is not at risk or there is nothing to choose."""
    import decisions
    t0 = time.time()
    jd = _d(pub.get("jokers")).get("cards") or []
    pol = rollout.Policy(jd)
    cands, plays, info = candidates(pub, gs, pol)
    margin = decisions.CLEAR_MARGIN if jd else 1.0
    if len(cands) < 2 or not at_risk(plays, info, margin):
        return None
    res = [[] for _ in cands]
    k = 0
    while k < r:
        base = _fresh(gs, k)
        for n, (kind, cards, _) in enumerate(cands):
            g = copy.deepcopy(base) if n < len(cands) - 1 else base
            try:
                step(g, (PlayHand if kind == "play" else Discard)(card_indices=tuple(_forced(g, cards))))
                res[n].append(finish_round(g, pol))
            except IllegalActionError:
                res[n].append((False, 0.0))
        k += 1
        if budget_s and time.time() - t0 > budget_s and k >= 8:
            break
    rows = []
    for (kind, cards, label), rs in zip(cands, res):
        p = sum(c for c, _ in rs) / len(rs)
        ratio = sum(min(x, 1.5) for _, x in rs) / len(rs)
        rows.append({"kind": kind, "cards": cards, "label": label, "p_clear": round(p, 3), "ratio": round(ratio, 3)})
    best = max(range(len(rows)), key=lambda i: (rows[i]["p_clear"], rows[i]["ratio"]))
    # paired difference against the bot's strongest play (play_candidates order), same futures
    d = [float(a[0]) - float(b[0]) for a, b in zip(res[best], res[0])]
    mean = sum(d) / len(d)
    se = (sum((x - mean) ** 2 for x in d) / (len(d) - 1) / len(d)) ** 0.5 if len(d) > 1 else 0.0
    rows[best]["vs_top"] = [round(mean, 3), round(se, 3)]
    return {"pick": rows[best], "rows": rows, "rollouts": k, "seconds": round(time.time() - t0, 2),
            "all_equal": len({r["p_clear"] for r in rows}) == 1,
            # not clearly better than the bot's strongest play: the bot's own handler decides
            "defer": mean < MARGIN or mean <= OVERRIDE_SE * se}


def wrap(orig, stats):
    """decisions.selecting_hand with the planner first (lab arm G): the planner's pick when the round is at
    risk and it beats the bot's strongest play by MARGIN in P(clear); otherwise the bot's own handler."""
    import decisions

    def selecting_hand(bot, gs):
        try:
            out = plan(gs, bot.game.gs)
        except Exception:  # noqa: BLE001  the bot's own handler decides
            stats["errors"] += 1
            out = None
        if out is not None:
            stats["evals"] += 1
            stats["seconds"] += out["seconds"]
        if out is None or out["all_equal"] or out["defer"]:
            return orig(bot, gs)
        stats["plans"] += 1
        pk = out["pick"]
        stats["discards" if pk["kind"] == "discard" else "plays"] += 1
        bot.emit("auto", question="move", choice=pk["kind"], action=[pk["kind"], {"cards": pk["cards"]}],
                 desc=pk["label"], reason=f"round planner: P(clear) {pk['p_clear']:.2f} over {out['rollouts']} futures, "
                                          f"{pk['vs_top'][0]:+.2f} ± {pk['vs_top'][1]:.2f} vs the strongest play "
                                          f"(candidates {sorted((r['p_clear'] for r in out['rows']), reverse=True)})",
                 planner_seconds=out["seconds"])
        hand = _d(gs.get("hand")).get("cards") or []
        if pk["kind"] == "play":
            decisions._play(bot, gs, hand, pk["cards"])
        else:
            bot.act("discard", {"cards": pk["cards"]})

    return selecting_hand
