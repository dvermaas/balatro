"""Decision handlers (v6+): narrow typed questions batched per Jev call, combined by code.

Hand: Noul play_now + Choice best_play + Choice best_discard in one call. v12: boss-aware candidate
scoring and full-hand forecasts after each discard, shared with the play-now question.
Shop / packs: one 0-4 Score per item in one call; code applies the buy/skip thresholds. v9: each item
carries its measured gain on this run's recent hands, and early antes prioritise filling joker slots.
"""

import json
from collections import Counter

from typesafe_sdk import Choice, Noul, Score

import build
import jokers
import odds
import tarots
import strategy
from balatro import RPCError
from hands import RANK_NAMES, SUIT_NAMES, _d, card_info, classify, describe, discard_candidates, play_candidates
from jev_bot import SHOP_ACTION_CAP, area, cards_of, current_blind, item_desc
from prompts import GAME_RULES, INSTRUCTIONS, ITEM_VALUE_LEVELS, OPTIONS, PLAY_NOW_CRITERIA

# Jev's 0-4 scores cluster around 2-3, so thresholds sit mid-scale (v6 used 3 and starved: 1 buy in 14 shops).
BUY_SCORE = 2.5        # buy the best-rated item at this score...
BUY_SCORE_LOOSE = 2    # ...or at "Useful" when it keeps the $25 buffer, or is a joker while we own fewer than 3
EARLY_JOKERS = 4       # v9: through ante 3, fill joker slots before saving for interest
EARLY_ANTE = 3
SURE_GAIN = 0.5        # a joker that adds 50%+ to our recent hands is bought whenever a slot is free
RECENT_PLAYS = 10
NEAR_PACE = 0.8         # v10: near pace with no spare discards, digging must clearly pay off

# v10: what each voucher is worth, in terms Jev can weigh; strong ones are bought by code when affordable.
VOUCHERS = {
    "Grabber": ("+1 hand every round for the rest of the run (~+25% chips per round)", True),
    "Nacho Tong": ("+1 more hand every round for the rest of the run", True),
    "Wasteful": ("+1 discard every round for the rest of the run", True),
    "Recyclomancy": ("+1 more discard every round for the rest of the run", True),
    "Paint Brush": ("+1 hand size: more cards to build hands from, every round", True),
    "Palette": ("+1 more hand size, every round", True),
    "Clearance Sale": ("everything in the shop costs 25% less for the rest of the run", True),
    "Liquidation": ("everything in the shop costs 50% less for the rest of the run", True),
    "Overstock": ("+1 card slot in every shop: more jokers to choose from", True),
    "Overstock Plus": ("+1 more card slot in every shop", True),
    "Antimatter": ("+1 joker slot", True),
    "Reroll Surplus": ("rerolls cost $2 less", False), "Reroll Glut": ("rerolls cost $2 less again", False),
    "Crystal Ball": ("+1 consumable slot", False),
    "Telescope": ("Celestial packs always contain the planet for our most played hand", False),
    "Observatory": ("useless for us: we use planets right away instead of holding them", False),
    "Hone": ("Foil, Holo and Polychrome cards appear 2x more often", False),
    "Glow Up": ("Foil, Holo and Polychrome cards appear 4x more often", False),
    "Tarot Merchant": ("Tarot cards appear more often in the shop", False),
    "Tarot Tycoon": ("Tarot cards appear much more often in the shop", False),
    "Planet Merchant": ("Planet cards appear more often in the shop", False),
    "Planet Tycoon": ("Planet cards appear much more often in the shop", False),
    "Seed Money": ("interest cap $5 -> $10: only pays if we hold $50+", False),
    "Money Tree": ("interest cap -> $20: only pays if we hold $100+", False),
    "Blank": ("does nothing by itself (unlocks Antimatter, +1 joker slot)", False),
    "Magic Trick": ("playing cards can appear in the shop", False),
    "Illusion": ("shop playing cards may have editions or seals", False),
    "Hieroglyph": ("-1 ante (easier blinds) but -1 hand every round", False),
    "Petroglyph": ("-1 ante but -1 discard every round", False),
    "Director's Cut": ("reroll the boss blind once per ante for $10", False),
    "Retcon": ("reroll the boss blind any number of times for $10", False),
}
PACK_SCORE = 1.5       # the pack is already paid for: take anything better than marginal
INTEREST_BUFFER = 25
MAX_REROLLS = 2        # per shop, only from surplus above the buffer


def _round_info(gs):
    rnd, blind = _d(gs.get("round")), current_blind(gs)
    need = max(0, (blind.get("score") or 0) - (rnd.get("chips") or 0))
    hands_left = rnd.get("hands_left") or 0
    return {
        "blind": blind.get("name"), "boss_effect": blind.get("effect") or "none",
        "chip_target": blind.get("score"), "chips_scored": rnd.get("chips"), "chips_still_needed": need,
        "hands_left": hands_left, "discards_left": rnd.get("discards_left"),
        "chips_needed_per_remaining_hand": round(need / hands_left) if hands_left else None,
    }


def _deck_remaining(gs):
    infos = [card_info(c) for c in cards_of(gs, "cards")]
    return {
        "cards": len(infos),
        # fixed suit order: Counter order would reveal the bottom card of the pile
        "by_suit": {SUIT_NAMES[s]: n for s in "HDCS"
                    if (n := sum(i["suit"] == s for i in infos if i["rank"]))},
        "by_rank": {RANK_NAMES.get(r, r): n for r, n in sorted(
            Counter(i["rank"] for i in infos if i["rank"]).items(), key=lambda kv: "23456789TJQKA".index(kv[0]))},
    }


def _levels(gs, names):
    return {n: f"level {h.get('level')}: {h.get('chips')} chips x {h.get('mult')} mult"
            for n, h in _d(gs.get("hands")).items() if n in names}


def _run_context(gs):
    mains = strategy.main_hands(gs)
    jok, cons = area(gs, "jokers"), area(gs, "consumables")
    levelled = {n for n, h in _d(gs.get("hands")).items() if (h.get("level") or 1) > 1}
    return mains, {
        "game_rules": GAME_RULES, "ante": gs.get("ante_num"), "round": gs.get("round_num"),
        "money": gs.get("money"), "interest_per_round": strategy.interest(gs.get("money")),
        "main_hands": mains or "none yet",
        "engine": strategy.engine([_d(c.get("value")).get("effect") for c in cards_of(gs, "jokers")]),
        "jokers": [item_desc(c, mains) for c in cards_of(gs, "jokers")],
        "joker_slots": f"{jok.get('count', 0)}/{jok.get('limit', 5)}",
        "consumables": [item_desc(c) for c in cards_of(gs, "consumables")],
        "consumable_slots": f"{cons.get('count', 0)}/{cons.get('limit', 2)}",
        "hand_levels": _levels(gs, set(mains) | levelled),
    }


# ---- blind ------------------------------------------------------------------

def blind_select(bot, gs):
    b = current_blind(gs)
    if b.get("type") == "BOSS":
        bot.act("select")
        return
    fields = {k: b.get(k) for k in ("name", "score", "tag_name", "tag_effect")}
    opts = {
        "play": (OPTIONS["blind_play"].format(**fields), ("select", None)),
        "skip": (OPTIONS["blind_skip"].format(**fields), ("skip", None)),
    }
    bot.act(*bot.ask("blind", gs, INSTRUCTIONS["blind"], opts))


# ---- hand -------------------------------------------------------------------

CLEAR_MARGIN = 1.1  # joker-aware estimates are within 10% on ~90% of recorded plays
PLAY_NOW_UNDER_PACE = 0.85


def _clears(p, info, margin):
    return p["estimate"] >= info["chips_still_needed"] * margin


def _play_obj(p, info, margin=1.0):
    pace = info["chips_needed_per_remaining_hand"] or 1
    return {
        "hand": p["hand"], "scoring_cards": p["scoring"], "also_played": p["kicked"],
        "estimated_score": p["estimate"],
        "share_of_pace": f"{round(100 * p['estimate'] / max(1, pace))}%",
        "clears_blind_alone": _clears(p, info, margin),
    }


def _remember(bot, gs, hand, combo):
    """Keep this run's recent plays: the shop measures items on them."""
    if getattr(bot, "_recent_seed", None) != gs.get("seed"):
        bot._recent_seed, bot._recent = gs.get("seed"), []
    rnd = _d(gs.get("round"))
    bot._recent = (bot._recent + [{"infos": [card_info(c) for c in hand], "combo": combo,
                                   "rnd": {"hands_left": rnd.get("hands_left"),
                                           "discards_left": rnd.get("discards_left")}}])[-RECENT_PLAYS:]


def _recent(bot, gs):
    return bot._recent if getattr(bot, "_recent_seed", None) == gs.get("seed") else []


def _play(bot, gs, hand, cards):
    _remember(bot, gs, hand, cards)
    bot.act("play", {"cards": cards})


def _boss_levels(levels, boss):
    if boss == "The Flint":  # base chips and mult are halved
        return {n: {**h, "chips": int(h.get("chips", 0) / 2 + 0.5), "mult": max(1, int(h.get("mult", 1) / 2 + 0.5))}
                for n, h in levels.items()}
    return levels


def _boss_allows(combo, name, levels, boss):
    """Apply boss restrictions before candidate pruning and to every forecasted hand."""
    played = {n for n, h in levels.items() if h.get("played_this_round")}
    if boss == "The Psychic":
        return len(combo) == 5
    if boss == "The Eye":
        return name not in played
    if boss == "The Mouth" and played:
        return name in played
    return True


def _preserve_discard(hand, play, scorer, max_discard=5):
    """Cycle noncontributing cards without throwing away the current scoring core.

    Held Steel/Baron/etc. contributors are detected by scoring the same play without them.
    The forecast still evaluates the complete combined discard, including interactions.
    """
    infos = [card_info(c) for c in hand]
    combo = play["cards"]
    _, sc = classify([infos[i] for i in combo])
    scoring = {combo[p] for p in sc}
    toss = []
    for i in range(len(infos)):
        if i in scoring:
            continue
        remaining = infos[:i] + infos[i + 1:]
        sub = [p - (p > i) for p in combo if p != i]
        sub_name, sub_sc = classify([remaining[p] for p in sub])
        if scorer(remaining, sub, sub_name, sub_sc) >= play["estimate"]:
            toss.append(i)
    toss = sorted(toss, key=lambda i: (not infos[i]["debuff"],
                  "23456789TJQKA".find(infos[i]["rank"] or "")))[:max_discard]
    return {"label": "Keep current scoring cards and useful held cards; cycle the rest",
            "cards": toss, "tossed": [describe(hand[i]) for i in toss], "goal": ("preserve", None)}


def selecting_hand(bot, gs):
    hand = cards_of(gs, "hand")
    info = _round_info(gs)
    boss = info["blind"]
    rnd, js = _d(gs.get("round")), jokers.build(gs)
    levels = _boss_levels(_d(gs.get("hands")), boss)
    infos = [card_info(c) for c in hand]
    scorer = lambda infos, combo, name, sc: jokers.score(infos, combo, name, sc, levels, js, rnd)
    allowed = lambda combo, name: _boss_allows(combo, name, _d(gs.get("hands")), boss)
    plays = play_candidates(hand, levels, limit=8, scorer=scorer, allowed=allowed)
    if not plays:
        # Nothing scores under Eye/Mouth yet: a play may still cycle cards, but cannot be a sure clear.
        plays = [{**p, "estimate": 0} for p in play_candidates(hand, levels, scorer=scorer)]
    margin = CLEAR_MARGIN if js else 1.0
    clearing = [p for p in plays if _clears(p, info, margin)]
    if clearing:  # a sure clear wins the round: no question needed
        bot.emit("forced", question="move", choice="play_1",
                 desc=f"{clearing[0]['hand']} ({clearing[0]['estimate']}) clears the blind")
        _play(bot, gs, hand, clearing[0]["cards"])
        return

    cap = area(gs, "hand").get("highlighted_limit") or 5
    discards = discard_candidates(hand, max_discard=cap) if (info["discards_left"] or 0) > 0 else []
    b = build.decide(gs)
    if discards:
        preserve = _preserve_discard(hand, plays[0], scorer, cap)
        if preserve["cards"] and all(set(preserve["cards"]) != set(d["cards"]) for d in discards):
            discards.append(preserve)
        next_round = {**rnd, "discards_left": max(0, (rnd.get("discards_left") or 0) - 1)}

        def score_after_discard(tossed, drawn):
            next_js = jokers.after_discard(gs, tossed, drawn)
            return lambda cs, combo, name, sc: jokers.score(cs, combo, name, sc, levels, next_js, next_round)

        hand_size = area(gs, "hand").get("limit") or len(hand)
        odds.forecast(discards, infos, [card_info(c) for c in cards_of(gs, "cards")],
                      score_after_discard, allowed, need=info["chips_still_needed"] * margin,
                      pace=info["chips_needed_per_remaining_hand"] or 0,
                      draw_count=lambda n: 3 if boss == "The Serpent" else max(0, hand_size - len(hand) + n))
        for d in discards:
            d["fits_build"] = build.fits(d.get("goal"), b)
            d["gain_percent"] = round(100 * (d["expected_score"] / max(1, plays[0]["estimate"]) - 1))
        # Order the alternatives by measured survival value, not suit enumeration or build preference.
        discards.sort(key=lambda d: (d["chance_to_clear"], d["expected_score"])
                      if info["hands_left"] <= 1 else (d["expected_score"], d["chance_to_clear"]), reverse=True)

    play_opts = {f"play_{n}": p for n, p in enumerate(plays, 1)}
    disc_opts = {f"discard_{n}": d for n, d in enumerate(discards, 1)}
    mains = strategy.main_hands(gs)
    state = {
        "game_rules": GAME_RULES, "round": info,
        "jokers": [item_desc(c, mains) for c in cards_of(gs, "jokers")],
        "hand_levels": _levels(gs, {p["hand"] for p in plays}),
        "cards_in_hand": [describe(c) for c in hand],
        "deck_remaining": _deck_remaining(gs),
        "strongest_play": _play_obj(plays[0], info, margin),
        "best_draw_by_measurement": _discard_obj(discards[0]) if discards else None,
        "our_build": build.describe(b),
    }
    if jokers.unmodelled(js):
        state["jokers_not_in_estimates"] = jokers.unmodelled(js)
    questions = {}
    if len(play_opts) > 1:
        questions["best_play"] = Choice(instructions=INSTRUCTIONS["best_play"],
                                        criteria={k: _play_obj(p, info, margin) for k, p in play_opts.items()})
    if disc_opts:
        questions["play_now"] = Noul(instructions=INSTRUCTIONS["play_now"], criteria=PLAY_NOW_CRITERIA)
        if len(disc_opts) > 1:
            questions["best_discard"] = Choice(
                instructions=INSTRUCTIONS["best_discard"],
                criteria={k: _discard_obj(d) for k, d in disc_opts.items()})
    if not questions:
        bot.emit("forced", question="move", choice="play_1", desc=plays[0]["hand"])
        _play(bot, gs, hand, plays[0]["cards"])
        return

    answers, meta = bot.query(gs, state, questions)
    p_play = answers["play_now"].noul if "play_now" in answers else 1.0
    pp = dict(answers["best_play"].probabilities) if "best_play" in answers else {"play_1": 1.0}
    dp = dict(answers["best_discard"].probabilities) if "best_discard" in answers else (
        {"discard_1": 1.0} if disc_opts else {})
    probs = {k: p_play * pp.get(k, 0) for k in play_opts} | {k: (1 - p_play) * dp.get(k, 0) for k in disc_opts}
    # Jev's play_now leans yes (0.5-0.8) even far below pace, and v7 lost ante 1 with discards unused.
    # Below pace with hands to spare, discards are free: only play when Jev is clearly sure.
    # On the last hand a play that can't clear loses anyway, so dig with the discards first (The Needle).
    under_pace = plays[0]["estimate"] < (info["chips_needed_per_remaining_hand"] or 0)
    pace = info["chips_needed_per_remaining_hand"] or 0
    spare = (info["discards_left"] or 0) >= info["hands_left"]
    payoff = max((d["expected_score"] for d in discards), default=0)
    near_no_gain = plays[0]["estimate"] >= NEAR_PACE * pace and not spare and payoff < 1.2 * plays[0]["estimate"]
    if disc_opts and info["hands_left"] <= 1:
        bar = 1.01
    elif under_pace and near_no_gain:
        bar = 0.5  # close to pace, discards scarce and no plan clearly better: let Jev decide normally
    elif under_pace:
        bar = PLAY_NOW_UNDER_PACE
    else:
        bar = 0.5
    best = max(pp, key=pp.get) if p_play >= bar else max(dp, key=dp.get)
    options = {k: f"{p['hand']}: {', '.join(p['scoring'])}" + (f" (+{', '.join(p['kicked'])})" if p["kicked"] else "")
               + f" ~{p['estimate']}" for k, p in play_opts.items()}
    options |= {k: f"{d['label']}: toss {', '.join(d['tossed'])}"
                + f" (modeled next play ~{d['expected_score']}, {d['gain_percent']:+}% vs playing now)"
                for k, d in disc_opts.items()}
    if best.startswith("play"):
        bot.record_decision("move", gs, best, ("play", {"cards": play_opts[best]["cards"]}), options, probs,
                            probs[best], meta)
        _play(bot, gs, hand, play_opts[best]["cards"])
    else:
        action = ("discard", {"cards": disc_opts[best]["cards"]})
        bot.record_decision("move", gs, best, action, options, probs, probs[best], meta)
        bot.act(*action)


def _discard_obj(d):
    o = {"plan": d["label"], "throws_away": d["tossed"], "fits_our_build": d.get("fits_build", False)}
    if "expected_score" in d:
        o.update(modeled_average_next_play=d["expected_score"], poor_draw_next_play=d["low_score"],
                 poor_draw_definition="20th percentile, not a guaranteed minimum",
                 average_change_vs_playing_now=f"{d['gain_percent']:+}%",
                 chance_next_play_clears=f"{round(100 * d['chance_to_clear'])}%",
                 chance_next_play_meets_pace=f"{round(100 * d['chance_to_meet_pace'])}%",
                 cards_drawn=d["drawn"],
                 basis=f"{d['samples']} {'all possible' if d['exact_draws'] else 'sampled'} unordered draws; scoring model estimate")
    return o


# ---- shop and packs ---------------------------------------------------------

def _score_items(bot, gs, qname, state, items):
    """items: {key: (desc, item_obj, action)}. One Score per item in one call; returns (scores, meta)."""
    questions = {k: Score(instructions={"question": INSTRUCTIONS["item_value"],
                                        "item": {f: v for f, v in obj.items() if not f.startswith("_")}},
                          criteria=ITEM_VALUE_LEVELS) for k, (_, obj, _) in items.items()}
    answers, meta = bot.query(gs, state, questions)
    return {k: answers[k].score for k in items}, meta


def shop(bot, gs):
    key = (gs.get("ante_num"), gs.get("round_num"))
    if key != bot._shop_key:
        bot._shop_key, bot._shop_actions, bot._failed, bot._rerolls = key, 0, set(), 0
    money = gs.get("money") or 0
    jok, cons = area(gs, "jokers"), area(gs, "consumables")
    jokers_full = jok.get("count", 0) >= jok.get("limit", 5)
    cons_full = cons.get("count", 0) >= cons.get("limit", 2)
    mains, state = _run_context(gs)
    boss = next((b for b in _d(gs.get("blinds")).values() if b.get("type") == "BOSS"), {})
    state["upcoming_boss"] = {"name": boss.get("name"), "effect": boss.get("effect"), "chip_target": boss.get("score")}
    b = build.decide(gs)
    state["our_build"] = build.describe(b)

    recent = _recent(bot, gs)

    def obj(c, cost, kind, remove=None, **extra):
        left = money - cost
        return {"kind": kind, "what": item_desc(c, mains), "cost": cost, "money_after": left,
                "interest_per_round_after": strategy.interest(left), **extra, **_gain(gs, recent, c, remove)}

    items = {}
    if bot._shop_actions < SHOP_ACTION_CAP:
        for i, c in enumerate(cards_of(gs, "shop")):
            cost = _d(c.get("cost")).get("buy", 0)
            if cost > money or (c.get("set") == "JOKER" and jokers_full) or (
                    c.get("set") in ("TAROT", "PLANET", "SPECTRAL") and cons_full):
                continue
            o = obj(c, cost, c.get("set", "card").lower())
            n = tarots.note(c, b, money, cards_of(gs, "jokers"))
            if n:
                o["use"] = n
            items[f"buy_card_{i}"] = (item_desc(c, mains), o, ("buy", {"card": i}), cost)
        for i, c in enumerate(cards_of(gs, "vouchers")):
            cost = _d(c.get("cost")).get("buy", 0)
            if cost <= money:
                v = VOUCHERS.get(c.get("label"), (None, False))
                o = obj(c, cost, "voucher (permanent upgrade for the rest of the run)")
                if v[0]:
                    o["worth"], o["_strong"] = v[0], v[1]
                items[f"buy_voucher_{i}"] = (item_desc(c), o, ("buy", {"voucher": i}), cost)
        for i, c in enumerate(cards_of(gs, "packs")):
            cost = _d(c.get("cost")).get("buy", 0)
            if cost <= money:
                items[f"buy_pack_{i}"] = (item_desc(c), obj(c, cost, "booster pack (pick from its cards)"),
                                          ("buy", {"pack": i}), cost)
        if jokers_full:
            for j, owned in enumerate(cards_of(gs, "jokers")):
                mod = _d(owned.get("modifier"))
                if mod.get("eternal") or mod.get("edition") == "NEGATIVE":  # selling a Negative frees no slot
                    continue
                sell = _d(owned.get("cost")).get("sell", 0)
                for i, c in enumerate(cards_of(gs, "shop")):
                    cost = _d(c.get("cost")).get("buy", 0)
                    if c.get("set") == "JOKER" and cost <= money + sell:
                        items[f"replace_{j}_{i}"] = (
                            f"sell {owned.get('label')} for {c.get('label')}",
                            obj(c, cost - sell, "joker swap", remove=j, sells=item_desc(owned, mains), sell_value=sell),
                            ("replace", {"joker": j, "card": i}), cost - sell)
    items = {k: v for k, v in items.items() if json.dumps(v[2]) not in bot._failed}

    shown = [f"${_d(c.get('cost')).get('buy', 0)} {item_desc(c)}"
             for name in ("shop", "vouchers", "packs") for c in cards_of(gs, name)]
    bot.emit("info", msg=f"Shop (${money}): " + "; ".join(shown))

    reroll = _d(gs.get("round")).get("reroll_cost") or 5
    # v11: back to v9's rule. v10's early rerolls down to $5 cost ~$7.50 a run in ante 1-2, led to a joker
    # buy only 1 time in 5, and left ante 2-3 balances at $7-12 instead of $16-23 (lost interest).
    can_reroll = money - reroll >= INTEREST_BUFFER and bot._rerolls < MAX_REROLLS
    fallback = ("reroll", ("reroll", None)) if can_reroll else ("leave", ("next_round", None))
    if not items:
        bot.emit("forced", question="shop", choice=fallback[0], desc="nothing affordable to rate")
        _shop_act(bot, gs, fallback[1])
        return

    scores, meta = _score_items(bot, gs, "shop", state, {k: v[:3] for k, v in items.items()})
    best = max(scores, key=scores.get)
    s, cost = scores[best], items[best][3]
    early = (gs.get("ante_num") or 1) <= EARLY_ANTE and jok.get("count", 0) < EARLY_JOKERS
    early_joker = early and best.startswith(("buy_card", "replace")) and "joker" in items[best][1]["kind"]
    # A joker that measurably lifts our recent hands a lot is bought whenever a slot is free.
    strong_v = [k for k, v in items.items() if v[1].get("_strong")
                and money - v[3] >= INTEREST_BUFFER]  # v11: from surplus only, never during the joker phase
    sure = [k for k, v in items.items() if k.startswith("buy_card") and v[1]["kind"] == "joker"
            and (v[1].get("_gain") or 0) >= SURE_GAIN and scores[k] >= 1]
    if s >= BUY_SCORE or (s >= BUY_SCORE_LOOSE and (money - cost >= INTEREST_BUFFER or early_joker)):
        choice, action = best, items[best][2]
    elif strong_v:
        choice = strong_v[0]
        action = items[choice][2]
        bot.emit("info", msg=f"Buying voucher {items[choice][0][:40]}: {items[choice][1]['worth']}")
    elif sure:
        choice = max(sure, key=lambda k: items[k][1]["_gain"])
        action = items[choice][2]
        bot.emit("info", msg=f"Buying {items[choice][0][:40]}: +{round(100 * items[choice][1]['_gain'])}% on recent hands")
    else:
        choice, action = fallback
    options = {k: f"${v[3]} {v[0]}" for k, v in items.items()}
    bot.record_decision("shop", gs, choice, action, options, {k: v / 4 for k, v in scores.items()},
                        s / 4, meta, display="score")
    _shop_act(bot, gs, action)


def _shop_act(bot, gs, action):
    bot._shop_actions += 1
    if action[0] == "reroll":
        bot._rerolls += 1
    if action[0] == "buy" and "pack" in (action[1] or {}):
        bot._last_pack_cost = _d(cards_of(gs, "packs")[action[1]["pack"]].get("cost")).get("buy", 0)
    try:
        if action[0] == "replace":
            bot.act("sell", {"joker": action[1]["joker"]})
            bot.act("buy", {"card": action[1]["card"]})
        else:
            bot.act(*action)
    except RPCError:
        bot._failed.add(json.dumps(action))
        raise


def booster(bot, gs):
    jok = area(gs, "jokers")
    jokers_full = jok.get("count", 0) >= jok.get("limit", 5)
    mains, state = _run_context(gs)
    if cards_of(gs, "hand"):  # only arcana/spectral packs deal a hand to target
        state["cards_in_hand"] = [describe(c) for c in cards_of(gs, "hand")]
    items = {}
    b = build.decide(gs)
    state["our_build"] = build.describe(b)
    hand = cards_of(gs, "hand")
    for i, c in enumerate(cards_of(gs, "pack")):
        if c.get("set") == "JOKER" and jokers_full:
            continue
        o = {"kind": (c.get("set") or "card").lower(), "what": item_desc(c, mains), "cost": 0,
             **_gain(gs, _recent(bot, gs), c)}
        params = {"card": i}
        if (c.get("key") or "") in tarots.TARGETED:
            p = tarots.plan(c.get("key"), hand, b)
            o["use"] = p[1] if p else "no good target in this hand"
            if p:
                params["targets"] = p[0]
        else:
            n = tarots.note(c, b, gs.get("money") or 0, cards_of(gs, "jokers"))
            if n:
                o["use"] = n
        items[f"take_{i}"] = (item_desc(c, mains), o, ("pack", params))
    skip = ("pack", {"skip": True})
    if not items:
        bot.emit("forced", question="pack", choice="skip", desc="nothing we can take")
        bot.act(*skip)
        return
    scores, meta = _score_items(bot, gs, "pack", state, items)
    best = max(scores, key=scores.get)
    choice, (method, params) = (best, items[best][2]) if scores[best] >= PACK_SCORE else ("skip", skip)
    if choice == "skip":
        # v11: the pack is paid for, so a tarot we have a good use for is free value: take the best-rated one.
        usable = [k for k, v in items.items() if "targets" in v[2][1]
                  or (cards_of(gs, "pack")[v[2][1]["card"]].get("key") or "") in tarots.SAFE_UNTARGETED]
        if usable:
            choice = max(usable, key=scores.get)
            method, params = items[choice][2]
    bot.record_decision("pack", gs, choice, (method, params), {k: v[0] for k, v in items.items()},
                        {k: v / 4 for k, v in scores.items()}, scores[best] / 4, meta, display="score")
    try:
        bot.act(method, params)
    except RPCError as e:
        if params.get("skip"):
            raise
        # Tarot/Spectral that needs hand targets: aim at the first two cards, else give up on the pack.
        bot.emit("info", msg=f"Pack pick needs targets ({e}); retrying with targets")
        try:
            bot.act("pack", {**params, "targets": [0, 1]})
        except RPCError:
            try:
                bot.act("pack", {**params, "targets": [0]})
            except RPCError:
                bot.act("pack", {"skip": True})


def _gain(gs, recent, c, remove=None):
    """Measured effect of a joker or planet on this run's recent hands: {measured_gain: text, _gain: ratio}."""
    if c.get("set") == "JOKER":
        g = jokers.gain(gs, recent, add=c, remove=remove)
        if g is None:
            return {"measured_gain": "can't be measured from its text; judge it yourself"}
    elif c.get("set") == "PLANET" and jokers.planet_level(_d(c.get("value")).get("effect")):
        g = jokers.gain(gs, recent, level_up=jokers.planet_level(_d(c.get("value")).get("effect")))
    elif c.get("set") in ("TAROT", "SPECTRAL"):
        m = tarots.measured(c, gs, recent, build.decide(gs))
        return {"measured_gain": m[0], "_gain": m[1]} if m else {}
    else:
        return {}
    if not g:
        what = ("levels a hand we haven't played recently" if c.get("set") == "PLANET"
                else "no immediate effect on our recent hands' scores (scaling jokers may grow)")
        return {"measured_gain": what, "_gain": 0.0}
    basis = f"our last {len(recent)} hands" if len(recent) >= 4 else "our recent and typical hands"
    return {"measured_gain": f"{'+' if g >= 0 else ''}{round(100 * g)}% score on {basis}", "_gain": g}
