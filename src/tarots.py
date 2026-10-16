"""Tarot and spectral use (v10): code knows each card's mechanics and picks targets that fit the build.

Top LLM players use 11-18 consumables a run; we used none. Jev decides what to buy or take (with the
plan text from here as a fact); code decides when and on which cards to use them.
Part of the prompt version hash (see recorder.PROMPT_VERSION).
"""

from collections import Counter

import build
from hands import SUIT_NAMES, _d, card_info, describe

# key: (kind, count, arg). Kinds: enhance (card enhancement), suit, rank_up, remove, copy, plus untargeted.
TARGETED = {
    "c_magician": ("enhance", 2, "LUCKY"), "c_empress": ("enhance", 2, "MULT"),
    "c_heirophant": ("enhance", 2, "BONUS"), "c_lovers": ("enhance", 1, "WILD"),
    "c_chariot": ("enhance", 1, "STEEL"), "c_justice": ("enhance", 1, "GLASS"),
    "c_devil": ("enhance", 1, "GOLD"), "c_tower": ("enhance", 1, "STONE"),
    "c_strength": ("rank_up", 2, None), "c_hanged_man": ("remove", 2, None), "c_death": ("copy", 2, None),
    "c_star": ("suit", 3, "D"), "c_moon": ("suit", 3, "C"), "c_sun": ("suit", 3, "H"), "c_world": ("suit", 3, "S"),
    # spectrals that target one card
    "c_aura": ("best", 1, "edition"), "c_deja_vu": ("best", 1, "red seal"), "c_talisman": ("held", 1, "gold seal"),
    "c_medium": ("best", 1, "purple seal"), "c_trance": ("best", 1, "blue seal"), "c_cryptid": ("best", 1, "2 copies"),
}
ENHANCE_TEXT = {"LUCKY": "Lucky (chance of +20 Mult or $20)", "MULT": "Mult cards (+4 Mult when scored)",
                "BONUS": "Bonus cards (+30 Chips when scored)", "WILD": "Wild (counts as every suit)",
                "STEEL": "Steel (x1.5 Mult while held in hand)", "GLASS": "Glass (x2 Mult when scored, may break)",
                "GOLD": "Gold ($3 if held at end of round)", "STONE": "Stone (+50 Chips, no rank or suit)"}
# Enhancements that pay off when the card is held rather than played go on low-value cards.
HELD_ENH = {"STEEL", "GOLD"}
# Untargeted cards that are always worth using right away (when the condition holds).
SAFE_UNTARGETED = {"c_hermit", "c_temperance", "c_high_priestess", "c_emperor", "c_judgement",
                   "c_wheel_of_fortune", "c_fool", "c_black_hole"}


def plan(key, hand_cards, b):
    """(targets, text) for a targeted card given the cards in hand, or None when no use fits the build."""
    if key not in TARGETED:
        return None
    kind, n, arg = TARGETED[key]
    infos = [card_info(c) for c in hand_cards]
    if len(infos) < (2 if kind == "copy" else 1):
        return None
    val = {i: build.card_value(infos[i], b) for i in range(len(infos)) if not infos[i]["debuff"]}
    order = sorted(val, key=lambda i: -val[i])  # best first
    name = lambda idx: ", ".join(describe(hand_cards[i]) for i in idx)
    if kind == "enhance":
        fresh = [i for i in order if not infos[i]["enh"]]
        if arg == "WILD":
            if not b.get("suit"):
                return None  # Wild only matters for flushes
            pick = [i for i in fresh if infos[i]["suit"] != b["suit"]][:n]
        elif arg in HELD_ENH or arg == "STONE":
            pick = list(reversed(fresh))[:n]
        else:
            pick = fresh[:n]
        return (pick, f"turns {name(pick)} into {ENHANCE_TEXT[arg]}") if pick else None
    if kind == "suit":
        off = [i for i in reversed(order) if infos[i]["suit"] != arg and infos[i]["enh"] not in ("WILD", "STONE")]
        if b.get("suit") and b["suit"] != arg:
            return None  # would pull cards away from our flush suit
        pick = off[:n]
        return (pick, f"converts {name(pick)} to {SUIT_NAMES[arg]}") if pick else None
    if kind == "remove":
        pick = list(reversed(order))[:n]
        return pick, f"destroys {name(pick)} (thins the deck of junk)"
    if kind == "rank_up":
        pick = [i for i in order if infos[i]["rank"] and infos[i]["rank"] != "A"][:n]
        return (pick, f"raises the rank of {name(pick)} by one") if pick else None
    if kind == "copy":
        # Death copies the rightmost selected card onto the other: needs a good card right of a bad one.
        best = max(((i, j) for i in val for j in val if i < j), key=lambda p: val[p[1]] - val[p[0]], default=None)
        if not best or val[best[1]] - val[best[0]] < 0.3:
            return None
        return list(best), f"turns {name([best[0]])} into a copy of {name([best[1]])}"
    if kind == "best":
        return [order[0]], f"adds {arg} to {name([order[0]])}"
    if kind == "held":
        return [order[-1]], f"adds {arg} to {name([order[-1]])}"
    return None


def cash_outcome(c, money, cost=0, jokers=None):
    """Cash from taking and immediately using a money tarot, after paying its price."""
    key = c.get("key")
    left = money - cost
    if key == "c_hermit":
        payout = max(0, min(left, 20))
    elif key == "c_temperance":
        payout = max(0, min(sum(_d(j.get("cost")).get("sell", 0) for j in jokers or []), 50))
    else:
        return None
    return {"cash_before": money, "purchase_cost": cost, "cash_after_purchase": left,
            "payout_if_used_now": payout, "net_cash_gain": payout - cost,
            "cash_after_use": left + payout}


def note(c, b, money=0, jokers=None):
    """Short fact about what a tarot/spectral would do for us, for Jev's item scoring."""
    key = c.get("key") or ""
    if key in TARGETED:
        kind, n, arg = TARGETED[key]
        if kind == "suit":
            if b.get("suit") and b["suit"] != arg:
                return f"converts cards to {SUIT_NAMES[arg]}, against our {build.describe(b)} build"
            return f"converts up to 3 cards to {SUIT_NAMES[arg]}" + (", feeding our flush" if b.get("suit") else "")
        if kind == "enhance":
            return f"makes {n} of our best cards {ENHANCE_TEXT[arg]}" if arg not in HELD_ENH \
                else f"makes {n} low card {ENHANCE_TEXT[arg]}"
        return {"remove": "removes 2 junk cards from the deck", "rank_up": "raises 2 cards by one rank",
                "copy": "copies a strong card over a weak one", "best": f"adds {arg} to our best card",
                "held": f"adds {arg} to a card we hold"}[kind]
    if key == "c_hermit":
        return f"gives +${max(0, min(money, 20))} now (doubles money, max $20)"
    if key == "c_temperance":
        total = sum(_d(j.get("cost")).get("sell", 0) for j in jokers or [])
        return f"gives +${min(total, 50)} now (our jokers' sell value)"
    return None


def auto_use(gs):
    """(method, params, reason) to use a held consumable now, or None. Untargeted ones in the shop or
    blind select; targeted ones while a hand is showing (they need cards selected)."""
    state = gs.get("state")
    cons = _d(gs.get("consumables")).get("cards") or []
    jok = _d(gs.get("jokers"))
    jokers_free = (jok.get("limit") or 5) - (jok.get("count") or 0)
    cons_free = (_d(gs.get("consumables")).get("limit") or 2) - len(cons)
    b = None  # v22 speed: build.decide only when a targeted card needs a plan (it ran on every bot step)
    for i, c in enumerate(cons):
        key = c.get("key") or ""
        label = c.get("label") or key
        if key in SAFE_UNTARGETED and state in ("SHOP", "BLIND_SELECT", "SELECTING_HAND"):
            if key == "c_hermit" and (gs.get("money") or 0) < 4:
                continue
            if key == "c_judgement" and jokers_free < 1:
                continue
            if key == "c_wheel_of_fortune" and not jok.get("count"):
                continue
            if key in ("c_high_priestess", "c_emperor", "c_fool") and cons_free < 0:
                continue
            return "use", {"consumable": i}, f"use {label}"
        if key in TARGETED and state == "SELECTING_HAND":
            b = b or build.decide(gs)
            p = plan(key, _d(gs.get("hand")).get("cards") or [], b)
            if p:
                return "use", {"consumable": i, "cards": p[0]}, f"use {label}: {p[1]}"
    return None


# ---- measured value (v11) -------------------------------------------------------

# Chance that a given card is among the scored cards of a hand (8-card hands from a ~52-card deck,
# with discards): the enhanced card only pays when it shows up and scores.
SHOW_RATE = 0.12
MEASURABLE = {"MULT": "MULT", "BONUS": "BONUS", "GLASS": "GLASS", "LUCKY": "MULT", "STEEL": "STEEL"}


def measured(c, gs, recent, b):
    """(text, per_hand_gain) for an enhancement tarot, measured on our recent hands with jokers, or None.
    Lucky counts as its average (+4 Mult). Steel is measured on a held card."""
    import jokers
    key = c.get("key") or ""
    if key not in TARGETED or TARGETED[key][0] != "enhance" or TARGETED[key][2] not in MEASURABLE:
        return None
    _, n, arg = TARGETED[key]
    enh = MEASURABLE[arg]
    js, levels = jokers.build(gs), _d(gs.get("hands"))
    before = after = 0
    for infos, combo, name, sc, rnd in jokers.sample_plays(gs, recent):
        new = [dict(i) for i in infos]
        if enh == "STEEL":
            held = [i for i in range(len(new)) if i not in combo]
            if not held:
                continue
            t = min(held, key=lambda i: build.card_value(new[i], b))
        else:
            scoring = [combo[p] for p in sc]
            if not scoring:
                continue
            t = max(scoring, key=lambda i: build.card_value(new[i], b))
        new[t]["enh"] = enh
        before += jokers.score(infos, combo, name, sc, levels, js, rnd)
        after += jokers.score(new, combo, name, sc, levels, js, rnd)
    if not before:
        return None
    when = after / before - 1
    per_hand = when * n * SHOW_RATE
    where = "is held" if enh == "STEEL" else "scores"
    risk = " (Glass has a 1 in 4 chance to break when it scores)" if enh == "GLASS" else ""
    return (f"permanent: +{round(100 * when)}% on a hand where the enhanced card {where}{risk}; "
            f"{n} card{'s' if n > 1 else ''} = about +{round(100 * per_hand)}% per hand on average"), per_hand
