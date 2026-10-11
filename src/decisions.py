"""v6 decision handlers: narrow typed questions batched per Jev call, combined by code.

Hand: Noul play_now + Choice best_play + Choice best_discard in one call.
Shop / packs: one 0-4 Score per item in one call; code applies the buy/skip thresholds.
"""

import json
from collections import Counter

from typesafe_sdk import Choice, Noul, Score

import strategy
from balatro import RPCError
from hands import RANK_NAMES, SUIT_NAMES, _d, card_info, describe, discard_candidates, play_candidates
from jev_bot import SHOP_ACTION_CAP, area, cards_of, current_blind, item_desc
from prompts import GAME_RULES, INSTRUCTIONS, ITEM_VALUE_LEVELS, OPTIONS, PLAY_NOW_CRITERIA

# Jev's 0-4 scores cluster around 2-3, so thresholds sit mid-scale (v6 used 3 and starved: 1 buy in 14 shops).
BUY_SCORE = 2.5        # buy the best-rated item at this score...
BUY_SCORE_LOOSE = 2    # ...or at "Useful" when it keeps the $25 buffer, or is a joker while we own fewer than 3
EARLY_JOKERS = 3
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
        "by_suit": dict(Counter(SUIT_NAMES.get(i["suit"], "none") for i in infos if i["rank"])),
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

def _play_obj(p, info):
    pace = info["chips_needed_per_remaining_hand"] or 1
    return {
        "hand": p["hand"], "scoring_cards": p["scoring"], "also_played": p["kicked"],
        "estimate_before_jokers": p["estimate"],
        "share_of_pace": f"{round(100 * p['estimate'] / max(1, pace))}%",
        "clears_blind_alone": p["estimate"] >= info["chips_still_needed"],
    }


def selecting_hand(bot, gs):
    hand = cards_of(gs, "hand")
    info = _round_info(gs)
    plays = play_candidates(hand, _d(gs.get("hands")))
    discards = discard_candidates(hand) if (info["discards_left"] or 0) > 0 else []
    clearing = [p for p in plays if p["estimate"] >= info["chips_still_needed"]]
    if clearing:  # jokers only add, so this wins the round: no question needed
        bot.emit("forced", question="move", choice="play_1",
                 desc=f"{clearing[0]['hand']} ({clearing[0]['estimate']}) clears the blind")
        bot.act("play", {"cards": clearing[0]["cards"]})
        return

    play_opts = {f"play_{n}": p for n, p in enumerate(plays, 1)}
    disc_opts = {f"discard_{n}": d for n, d in enumerate(discards, 1)}
    mains = strategy.main_hands(gs)
    state = {
        "game_rules": GAME_RULES, "round": info,
        "jokers": [item_desc(c, mains) for c in cards_of(gs, "jokers")],
        "hand_levels": _levels(gs, {p["hand"] for p in plays}),
        "cards_in_hand": [describe(c) for c in hand],
        "deck_remaining": _deck_remaining(gs),
        "strongest_play": _play_obj(plays[0], info),
    }
    questions = {}
    if len(play_opts) > 1:
        questions["best_play"] = Choice(instructions=INSTRUCTIONS["best_play"],
                                        criteria={k: _play_obj(p, info) for k, p in play_opts.items()})
    if disc_opts:
        questions["play_now"] = Noul(instructions=INSTRUCTIONS["play_now"], criteria=PLAY_NOW_CRITERIA)
        if len(disc_opts) > 1:
            questions["best_discard"] = Choice(
                instructions=INSTRUCTIONS["best_discard"],
                criteria={k: {"plan": d["label"], "throws_away": d["tossed"]} for k, d in disc_opts.items()})
    if not questions:
        bot.emit("forced", question="move", choice="play_1", desc=plays[0]["hand"])
        bot.act("play", {"cards": plays[0]["cards"]})
        return

    answers, meta = bot.query(gs, state, questions)
    p_play = answers["play_now"].noul if "play_now" in answers else 1.0
    pp = dict(answers["best_play"].probabilities) if "best_play" in answers else {"play_1": 1.0}
    dp = dict(answers["best_discard"].probabilities) if "best_discard" in answers else (
        {"discard_1": 1.0} if disc_opts else {})
    probs = {k: p_play * pp.get(k, 0) for k in play_opts} | {k: (1 - p_play) * dp.get(k, 0) for k in disc_opts}
    best = max(pp, key=pp.get) if p_play >= 0.5 else max(dp, key=dp.get)
    if best.startswith("play"):
        p = play_opts[best]
        action = ("play", {"cards": p["cards"]})
    else:
        action = ("discard", {"cards": disc_opts[best]["cards"]})
    options = {k: f"{p['hand']}: {', '.join(p['scoring'])}" + (f" (+{', '.join(p['kicked'])})" if p["kicked"] else "")
               + f" ~{p['estimate']}" for k, p in play_opts.items()}
    options |= {k: f"{d['label']}: toss {', '.join(d['tossed'])}" for k, d in disc_opts.items()}
    bot.record_decision("move", gs, best, action, options, probs, probs[best], meta)
    bot.act(*action)


# ---- shop and packs ---------------------------------------------------------

def _score_items(bot, gs, qname, state, items):
    """items: {key: (desc, item_obj, action)}. One Score per item in one call; returns (scores, meta)."""
    questions = {k: Score(instructions={"question": INSTRUCTIONS["item_value"], "item": obj},
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

    def obj(c, cost, kind, **extra):
        left = money - cost
        return {"kind": kind, "what": item_desc(c, mains), "cost": cost, "money_after": left,
                "interest_per_round_after": strategy.interest(left), **extra}

    items = {}
    if bot._shop_actions < SHOP_ACTION_CAP:
        for i, c in enumerate(cards_of(gs, "shop")):
            cost = _d(c.get("cost")).get("buy", 0)
            if cost > money or (c.get("set") == "JOKER" and jokers_full) or (
                    c.get("set") in ("TAROT", "PLANET", "SPECTRAL") and cons_full):
                continue
            items[f"buy_card_{i}"] = (item_desc(c, mains), obj(c, cost, c.get("set", "card").lower()),
                                      ("buy", {"card": i}), cost)
        for i, c in enumerate(cards_of(gs, "vouchers")):
            cost = _d(c.get("cost")).get("buy", 0)
            if cost <= money:
                items[f"buy_voucher_{i}"] = (item_desc(c), obj(c, cost, "voucher (permanent upgrade)"),
                                             ("buy", {"voucher": i}), cost)
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
                            obj(c, cost - sell, "joker swap", sells=item_desc(owned, mains), sell_value=sell),
                            ("replace", {"joker": j, "card": i}), cost - sell)
    items = {k: v for k, v in items.items() if json.dumps(v[2]) not in bot._failed}

    shown = [f"${_d(c.get('cost')).get('buy', 0)} {item_desc(c)}"
             for name in ("shop", "vouchers", "packs") for c in cards_of(gs, name)]
    bot.emit("info", msg=f"Shop (${money}): " + "; ".join(shown))

    reroll = _d(gs.get("round")).get("reroll_cost") or 5
    can_reroll = money - reroll >= INTEREST_BUFFER and bot._rerolls < MAX_REROLLS
    fallback = ("reroll", ("reroll", None)) if can_reroll else ("leave", ("next_round", None))
    if not items:
        bot.emit("forced", question="shop", choice=fallback[0], desc="nothing affordable to rate")
        _shop_act(bot, gs, fallback[1])
        return

    scores, meta = _score_items(bot, gs, "shop", state, {k: v[:3] for k, v in items.items()})
    best = max(scores, key=scores.get)
    s, cost = scores[best], items[best][3]
    early_joker = best.startswith(("buy_card", "replace")) and "joker" in items[best][1]["kind"] and jok.get("count", 0) < EARLY_JOKERS
    if s >= BUY_SCORE or (s >= BUY_SCORE_LOOSE and (money - cost >= INTEREST_BUFFER or early_joker)):
        choice, action = best, items[best][2]
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
    for i, c in enumerate(cards_of(gs, "pack")):
        if c.get("set") == "JOKER" and jokers_full:
            continue
        items[f"take_{i}"] = (item_desc(c, mains),
                              {"kind": (c.get("set") or "card").lower(), "what": item_desc(c, mains), "cost": 0},
                              ("pack", {"card": i}))
    skip = ("pack", {"skip": True})
    if not items:
        bot.emit("forced", question="pack", choice="skip", desc="nothing we can take")
        bot.act(*skip)
        return
    scores, meta = _score_items(bot, gs, "pack", state, items)
    best = max(scores, key=scores.get)
    choice, (method, params) = (best, items[best][2]) if scores[best] >= PACK_SCORE else ("skip", skip)
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
