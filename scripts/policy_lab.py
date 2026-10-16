"""Policy lab: compare shop valuation policies at scale on the Jackdaw sim, with no JEV calls.

Every arm plays the same fresh seeds with the same play/discard policy: rollout.Policy, the fast
code policy used inside the forecast, which keeps playing when no discards are left (The Water).
Only joker and voucher buying differs between arms.
  gain           v18-like: jokers.gain on recent plays with v18's thresholds (SURE_GAIN, $25 buffer,
                 early joker phase, strong vouchers from surplus, rerolls from surplus)
  forecast       v19 guard rails: rollout.forecast (bosses=2), buy the best option that beats skip by
                 > 2 SE in blinds cleared (sell-swaps of the 2 weakest jokers included), reroll when
                 nothing beats skip by 1 SE and money stays >= $25
  forecast-long  the same with a 3-boss horizon
  none           never buys jokers or vouchers (floor)
  bot-forecast-long   arm E: the bot's real code (JevBot + decisions.py) with JEV replaced by its top option
  bot-forecast-items  arm F: E plus rollout_items.forecast for packs, shop tarots and spectrals (bought at
                      > 2 SE over skip) and rollout_items.Picker for the card taken from an open pack
Shared by all arms: buy the shop planet for our most played hand when affordable (keeping $4) and
use planets at once; skip packs, tarots, spectrals and playing cards; never skip blinds.

  python scripts/policy_lab.py --seeds 50 --arms gain,forecast,forecast-long,none --name pilot
  python scripts/policy_lab.py --report pilot                     # tables from runs/_policy_lab/pilot
  python scripts/policy_lab.py --set sim60-a --arms bot-forecast-long --name sim60-e   # a benchmark seed set
Every lost game records its fatal blind (name, boss effect, target, final score, best single hand, jokers);
--report prints the loss-audit tables (death blind, shortfall, fatal bosses) per arm.
Results stream to runs/_policy_lab/<name>/results.jsonl (resumable: finished games are skipped).
"""
from __future__ import annotations

import argparse
import json
import math
import multiprocessing as mp
import os
import random
import statistics
import sys
import time
import traceback
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "src"))
OUT = ROOT / "runs" / "_policy_lab"

FORECAST_N = 16
RECENT = 10


# ---------------------------------------------------------------------------
# Shop arms
# ---------------------------------------------------------------------------

def _d(x):
    return x if isinstance(x, dict) else {}


def _cards(pub, area):
    return _d(pub.get(area)).get("cards") or []


class Arm:
    name = "none"

    def __init__(self, cfg):
        self.cfg = cfg

    def decide(self, pub, game):
        return ("leave",)


class GainArm(Arm):
    """v18's code thresholds on jokers.gain (JEV's item scores replaced by the measured gain)."""
    name = "gain"

    def decide(self, pub, game):
        import decisions
        import jokers
        money = pub.get("money") or 0
        jl = _d(pub.get("jokers"))
        owned = _cards(pub, "jokers")
        full = len(owned) >= (jl.get("limit") or 5)
        ante = pub.get("ante_num") or 1
        early = ante <= decisions.EARLY_ANTE and len(owned) < decisions.EARLY_JOKERS
        best = None
        for i, c in enumerate(_cards(pub, "shop")):
            cost = _d(c.get("cost")).get("buy", 0)
            neg = _d(c.get("modifier")).get("edition") == "NEGATIVE"
            if c.get("set") != "JOKER" or cost > money or (full and not neg):
                continue
            g = jokers.gain(pub, game["recent"], add=c)
            if g is None:
                continue
            sure = g >= decisions.SURE_GAIN
            loose = g >= self.cfg["useful_gain"] and (money - cost >= decisions.INTEREST_BUFFER or early)
            early_any = early and g > 0
            if (sure or loose or early_any) and (best is None or g > best[0]):
                best = (g, ("buy", i))
        if full and best is None:
            for j, o in enumerate(owned):
                m = _d(o.get("modifier"))
                if m.get("eternal") or m.get("edition") == "NEGATIVE":
                    continue
                sell = _d(o.get("cost")).get("sell", 0)
                for i, c in enumerate(_cards(pub, "shop")):
                    cost = _d(c.get("cost")).get("buy", 0)
                    if c.get("set") != "JOKER" or cost > money + sell:
                        continue
                    g = jokers.gain(pub, game["recent"], add=c, remove=j)
                    if g is not None and g >= decisions.SURE_GAIN and (best is None or g > best[0]):
                        best = (g, ("replace", j, i))
        if best:
            return best[1]
        for i, c in enumerate(_cards(pub, "vouchers")):
            cost = _d(c.get("cost")).get("buy", 0)
            if decisions.VOUCHERS.get(c.get("label"), (None, False))[1] and money - cost >= decisions.INTEREST_BUFFER:
                return ("voucher", i)
        reroll = _d(pub.get("round")).get("reroll_cost") or 5
        if money - reroll >= decisions.INTEREST_BUFFER and game["rerolls"] < decisions.MAX_REROLLS:
            return ("reroll",)
        return ("leave",)


class ForecastArm(Arm):
    """v19's guard rails on rollout.forecast, without JEV."""
    name = "forecast"

    def decide(self, pub, game):
        import decisions
        import rollout
        money = pub.get("money") or 0
        jl = _d(pub.get("jokers"))
        full = len(_cards(pub, "jokers")) >= (jl.get("limit") or 5)
        weak = decisions._weakest_jokers(pub, game["recent"]) if full else set()
        keys = {o["key"] for o in rollout.options(pub) if o["supported"] and o.get("set") in ("JOKER", "VOUCHER")
                and (not o["key"].startswith("replace_") or int(o["key"].split("_")[1]) in weak)}
        reroll = _d(pub.get("round")).get("reroll_cost") or 5
        can_reroll = money - reroll >= self.cfg["reroll_floor"] and game["rerolls"] < self.cfg["max_rerolls"]
        if not keys:
            return ("reroll",) if can_reroll else ("leave",)
        t = time.time()
        try:
            fc = rollout.forecast(pub, n=self.cfg["n"], bosses=self.cfg["bosses"], keys=keys)
        except Exception:  # noqa: BLE001  like decisions.shop: a failed forecast buys nothing here
            game["forecast_errors"] += 1
            return ("leave",)
        game["forecast_s"] += time.time() - t
        game["forecasts"] += 1
        rows = [(k, r["vs_skip"]["blinds"]) for k, r in fc["options"].items() if k != "skip"]
        rows.sort(key=lambda kv: -kv[1][0])
        if rows and rows[0][1][0] > self.cfg["buy_se"] * max(rows[0][1][1], 1e-9) + self.cfg.get("buy_margin", 0):
            k = rows[0][0]
            p = k.split("_")
            if k.startswith("replace_"):
                return ("replace", int(p[1]), int(p[2]))
            return ("voucher", int(p[2])) if k.startswith("buy_voucher_") else ("buy", int(p[2]))
        if can_reroll and not any(d > self.cfg["reroll_se"] * max(se, 1e-9) for _, (d, se) in rows):
            return ("reroll",)
        return ("leave",)


ARMS = {
    "none": (Arm, {}),
    "gain": (GainArm, {"useful_gain": 0.15}),
    "forecast": (ForecastArm, {"n": FORECAST_N, "bosses": 2, "buy_se": 2, "reroll_se": 1, "reroll_floor": 25,
                               "max_rerolls": 2}),
    "forecast-long": (ForecastArm, {"n": FORECAST_N, "bosses": 3, "buy_se": 2, "reroll_se": 1, "reroll_floor": 25,
                                    "max_rerolls": 2}),
    # diagnosis variants
    "forecast-noreroll": (ForecastArm, {"n": FORECAST_N, "bosses": 2, "buy_se": 2, "reroll_se": 1, "reroll_floor": 25,
                                        "max_rerolls": 0}),
    "forecast-long-noreroll": (ForecastArm, {"n": FORECAST_N, "bosses": 3, "buy_se": 2, "reroll_se": 1,
                                             "reroll_floor": 25, "max_rerolls": 0}),
    "forecast-bank": (ForecastArm, {"n": FORECAST_N, "bosses": 2, "buy_se": 2, "reroll_se": 1, "reroll_floor": 25,
                                    "max_rerolls": 0, "buy_margin": 0.25}),
}


# ---------------------------------------------------------------------------
# One game
# ---------------------------------------------------------------------------

def _shared_planets(gs, pub):
    """Shared rule: buy the shop planet for our most played hand (keep $4), then use held planets."""
    import jokers
    import rollout
    from jackdaw.engine.actions import BuyCard
    from jackdaw.engine.game import IllegalActionError, step
    hands = _d(pub.get("hands"))
    played = max(hands.items(), key=lambda kv: (_d(kv[1]).get("played") or 0, _d(kv[1]).get("level") or 0),
                 default=(None, {}))
    if played[0] and (_d(played[1]).get("played") or 0) > 0:
        for i, c in enumerate(_cards(pub, "shop")):
            lv = jokers.planet_level(_d(c.get("value")).get("effect")) if c.get("set") == "PLANET" else None
            cost = _d(c.get("cost")).get("buy", 0)
            if lv and lv[0] == played[0] and (pub.get("money") or 0) - cost >= 4 \
                    and len(gs.get("consumables", [])) < gs.get("consumable_slots", 2):
                try:
                    step(gs, BuyCard(shop_index=i))
                    return True
                except IllegalActionError:
                    return False
    rollout.use_planets(gs)
    return False


def play_game(task):
    """Play one seed with one arm to the end; returns a result dict (errors are returned, not raised)."""
    seed, arm_name = task
    t0 = time.time()
    try:
        if arm_name in BOT_ARMS:
            return _play_bot(seed, arm_name, t0)
        return _play(seed, arm_name, t0)
    except Exception as e:  # noqa: BLE001
        return {"seed": seed, "arm": arm_name, "error": f"{type(e).__name__}: {e}",
                "trace": traceback.format_exc(limit=6), "seconds": round(time.time() - t0, 1)}


def _play(seed, arm_name, t0):
    import rollout
    import sim_backend
    from jackdaw.engine.actions import (BuyCard, CashOut, Discard, GamePhase, NextRound, PlayHand, RedeemVoucher,
                                        Reroll, SelectBlind, SellCard, SkipPack)
    from jackdaw.engine.game import IllegalActionError, step
    from jackdaw.engine.run_init import initialize_run
    cls, cfg = ARMS[arm_name]
    arm = cls(cfg)
    gs = initialize_run("b_red", 1, seed)
    gs["phase"], gs["blind_on_deck"] = GamePhase.BLIND_SELECT, "Small"
    game = {"recent": [], "rerolls": 0, "forecast_s": 0.0, "forecasts": 0, "forecast_errors": 0}
    log = {"buys": [], "rerolls": 0, "reroll_spent": 0, "spent": 0, "blinds": 0, "deaths": None}
    pol, pol_sig = None, None
    rnd = {"blind": {}, "hands": []}  # the current blind and its per-hand scores (death stats)
    for _ in range(20000):
        ph = gs["phase"]
        if gs.get("won"):
            break
        if ph == GamePhase.BLIND_SELECT:
            typ = gs.get("blind_on_deck", "Small")
            rnd = {"blind": sim_backend._blinds(gs).get(typ.lower(), {}), "hands": []}
            step(gs, SelectBlind())
        elif ph == GamePhase.SELECTING_HAND:
            if not gs["hand"]:  # deck and hand exhausted: no legal play, the run is lost
                break
            sig = tuple((j.center_key, repr(sorted((j.ability or {}).items(), key=str))) for j in gs["jokers"])
            if pol is None or sig != pol_sig:
                jd = sim_backend._area(gs["jokers"], gs.get("joker_slots", 5), gs, "jokers")["cards"]
                pol, pol_sig = rollout.Policy(jd), sig
            kind, idx = pol.decide(gs)
            if kind == "play":
                cr = gs["current_round"]
                game["recent"] = (game["recent"] + [{
                    "infos": [rollout._info(c) for c in gs["hand"]], "combo": list(idx),
                    "rnd": {"hands_left": cr.get("hands_left"), "discards_left": cr.get("discards_left")}}])[-RECENT:]
            before = gs.get("chips") or 0
            try:
                step(gs, (PlayHand if kind == "play" else Discard)(card_indices=tuple(idx)))
            except IllegalActionError:
                forced = [p for p, c in enumerate(gs["hand"]) if c.ability.get("forced_selection")]
                step(gs, PlayHand(card_indices=tuple(forced[:1] or [0])))
                kind = "play"
            if kind == "play":
                rnd["hands"].append((gs.get("chips") or 0) - before)
        elif ph == GamePhase.ROUND_EVAL:
            log["blinds"] += 1
            step(gs, CashOut())
            game["rerolls"] = 0
        elif ph == GamePhase.SHOP:
            for _a in range(8):
                pub = sim_backend.serialize(gs)
                if _shared_planets(gs, pub):
                    continue
                pub = sim_backend.serialize(gs)
                act = arm.decide(pub, game)
                money = gs["dollars"]
                try:
                    if act[0] == "leave":
                        break
                    if act[0] == "reroll":
                        step(gs, Reroll())
                        game["rerolls"] += 1
                        log["rerolls"] += 1
                        log["reroll_spent"] += money - gs["dollars"]
                    elif act[0] == "voucher":
                        label = _cards(pub, "vouchers")[act[1]].get("label")
                        step(gs, RedeemVoucher(card_index=act[1]))
                        log["buys"].append((pub.get("ante_num"), "voucher", label))
                    elif act[0] == "replace":
                        sold = _cards(pub, "jokers")[act[1]].get("label")
                        label = _cards(pub, "shop")[act[2]].get("label")
                        step(gs, SellCard(area="jokers", card_index=act[1]))
                        step(gs, BuyCard(shop_index=act[2]))
                        log["buys"].append((pub.get("ante_num"), "swap", f"{sold}->{label}"))
                    else:
                        label = _cards(pub, "shop")[act[1]].get("label")
                        step(gs, BuyCard(shop_index=act[1]))
                        log["buys"].append((pub.get("ante_num"), "joker", label))
                    log["spent"] += max(0, money - gs["dollars"])
                except IllegalActionError:
                    break
            rollout.use_planets(gs)
            step(gs, NextRound())
        elif ph == GamePhase.PACK_OPENING:
            step(gs, SkipPack())
        elif ph == GamePhase.GAME_OVER:
            break
        else:
            raise RuntimeError(f"phase {ph}")
    ante = gs["round_resets"]["ante"]
    return {"seed": seed, "arm": arm_name, "won": bool(gs.get("won")), "ante": 8 if gs.get("won") else ante,
            "blinds": log["blinds"], "round": gs.get("round"), "rerolls": log["rerolls"],
            "reroll_spent": log["reroll_spent"], "spent": log["spent"], "buys": log["buys"],
            "forecasts": game["forecasts"], "forecast_errors": game["forecast_errors"], "forecast_s": round(game["forecast_s"], 1),
            "jokers": [j.ability.get("name") for j in gs["jokers"]], "seconds": round(time.time() - t0, 1),
            "death": None if gs.get("won") else _death(rnd["blind"], gs.get("chips") or 0, rnd["hands"],
                                                       [j.ability.get("name") for j in gs["jokers"]])}


def _death(blind, final, hands, jokers):
    """The fatal blind of a lost game, like docs/loss-audit.md: name, boss effect, target, final score,
    best single hand in that round, hands played and the jokers at death."""
    return {"blind": blind.get("name"), "type": blind.get("type"), "effect": blind.get("effect") or None,
            "target": blind.get("score") or 0, "final": final, "best_hand": max(hands) if hands else 0,
            "hands": len(hands), "jokers": jokers}


# ---------------------------------------------------------------------------
# Arm E: the bot's real code path (JevBot + decisions.py) with a code stand-in for JEV
# ---------------------------------------------------------------------------

# arm -> options: items = arm F's lab item forecast (redundant from v21, where production has it);
# planner = round_planner picks plays/discards when the round is at risk (arm G)
# compute scaling (arm H and a middle point): forecast rollouts n, horizon bosses, Picker synthetic hands
BOT_ARMS = {"bot-forecast-long": {}, "bot-forecast-items": {"items": True}, "bot-planner": {"planner": True},
            "bot-mid": {"n": 48, "bosses": 4, "synth": 36}, "bot-heavy": {"n": 128, "bosses": 5, "synth": 96},
            # shopping rollouts (src/rollout_shop.py): both shop forecasts buy jokers/planets in later shops
            "bot-forecast-shop": {"shop": True},
            # planet experiments (lab only): planets for the hand our deck scores most with (play_mix), and/or
            # Celestial packs bought outright while `celestial` dollars stay after the buy
            "bot-pl-mix": {"items": True, "planet_hand": "mix"},
            "bot-pl-cel10": {"items": True, "celestial": 10},
            "bot-pl-mix-cel10": {"items": True, "planet_hand": "mix", "celestial": 10},
            "bot-pl-mix-cel4": {"items": True, "planet_hand": "mix", "celestial": 4},
            # reroll experiments (lab only): decisions.MAX_REROLLS / INTEREST_BUFFER (in the lab the buffer only
            # gates rerolls: CodeJev's buys score >= BUY_SCORE, and v18 strong vouchers are off)
            "bot-rr4": {"items": True, "max_rerolls": 4},
            "bot-rr4-b15": {"items": True, "max_rerolls": 4, "buffer": 15},
            "bot-rr6-b10": {"items": True, "max_rerolls": 6, "buffer": 10},
            # pack planet picks valued with the best play re-chosen after the level-up, on deals of hand size + 3
            "bot-pl-reopt-cel4": {"items": True, "planet_hand": "mix", "celestial": 4, "planet_pick": "reopt"}}
_DEFAULTS = {}


def _reopt_gain(pub, lv, k=24, extra=3):
    """Mean best-play score ratio - 1 after levelling lv=(hand, chips, mult), the best play re-chosen per
    deal, over k deals of hand size + `extra` cards (roughly what a discard adds) from the deck multiset."""
    import random
    import jokers as text_jokers
    from hands import best_score, card_info
    d = lambda x: x if isinstance(x, dict) else {}  # noqa: E731
    deck = list(_cards(pub, "cards")) + list(_cards(pub, "hand"))
    deck.sort(key=lambda c: (str(d(c.get("value")).get("suit")), str(d(c.get("value")).get("rank")),
                             str(d(c.get("modifier")))))
    infos = [dict(card_info(c), debuff=False) for c in deck]
    size = min((d(pub.get("hand")).get("limit") or 8) + extra, len(infos))
    if size < 1:
        return 0.0
    js, levels = text_jokers.build(pub), d(pub.get("hands"))
    h, c, m = lv
    new = {**levels, h: {**d(levels.get(h)), "chips": d(levels.get(h)).get("chips", 0) + c,
                         "mult": d(levels.get(h)).get("mult", 1) + m}}
    rnd = {"hands_left": 2, "discards_left": 1}
    s0, s1 = text_jokers.Scorer(levels, js, rnd), text_jokers.Scorer(new, js, rnd)
    rng = random.Random("REOPT")
    before = after = 0.0
    for _ in range(k):
        hand = rng.sample(infos, size)
        before += best_score(hand, s0)
        after += best_score(hand, s1)
    return after / before - 1 if before else 0.0


def _code_jev(bot_ref, items=False, opts=None):
    """JEV replaced by 'take the top-ranked option': Choice -> the first option (the bot lists its best
    measured option first; cash tarot steps -> continue shopping), play_now -> play when the strongest
    play clears or beats the best measured discard, item scores -> 0 except the shared planet rule
    (the most played hand's planet, keeping $4), so shop buying is the forecast's code path alone.

    items=True (arm F): shop packs, tarots and spectrals get rollout_items.forecast; the option that
    beats buying nothing by > 2 SE in blinds cleared scores 3 (bought unless the planet rule's 4 comes
    first; a cash tarot is taken at the cash step instead). In an open pack the card rollout_items.Picker
    picks scores 4, the rest 0 (decisions.booster then takes it, with tarots.plan targets)."""
    sys.path.insert(0, str(ROOT / "scripts"))
    from sim_run import FirstOptionJev
    from typesafe_sdk import SystemOneResponse

    class CodeJev(FirstOptionJev):
        model = "code-top-option"

        def __init__(self):
            self.stats = {"item_forecasts": 0, "item_seconds": 0.0, "item_buys": [], "pack_picks": [], "item_errors": 0}

        def item_pick(self, questions):
            """{qid: score} for arm F's picks on this question set."""
            import rollout_items
            gs = bot_ref[0].last_state or {}
            st = gs.get("state")
            floor = (opts or {}).get("celestial")
            if floor is not None and st == "SHOP":
                money = gs.get("money") or 0
                cel = [(int(q.split("_")[-1]), q) for q in questions if q.startswith("buy_pack_")]
                cel = [(_cards(gs, "packs")[i], q) for i, q in cel]
                cel = [(c, q) for c, q in cel if "Celestial" in (c.get("label") or "")
                       and money - _d(c.get("cost")).get("buy", 0) >= floor]
                if cel:  # the biggest pack first (Mega > Jumbo > normal, by price)
                    c, q = max(cel, key=lambda cq: _d(cq[0].get("cost")).get("buy", 0))
                    self.stats["item_buys"].append([gs.get("ante_num"), c.get("label"), "celestial rule"])
                    return {q: 3.0}
            if st == "SMODS_BOOSTER_OPENED" and any(q.startswith("take_") for q in questions):
                p = rollout_items.Picker(gs).pick_pub(gs)
                if p:
                    c = _cards(gs, "pack")[p[0]]
                    self.stats["pack_picks"].append([gs.get("ante_num"), c.get("set"), c.get("label")])
                    return {f"take_{p[0]}": 4.0}
                return {}
            if st != "SHOP":
                return {}
            shop = _cards(gs, "shop")
            keys = {q for q in questions if q.startswith("buy_pack_") or (
                q.startswith("buy_card_") and shop[int(q.split("_")[-1])].get("set") in ("TAROT", "SPECTRAL"))}
            if not keys:
                return {}
            t = time.time()
            try:
                fc = rollout_items.forecast(gs, n=FORECAST_N, bosses=3, keys=keys)
            except Exception:  # noqa: BLE001  a failed item forecast buys nothing
                self.stats["item_errors"] += 1
                return {}
            self.stats["item_forecasts"] += 1
            self.stats["item_seconds"] += time.time() - t
            k = rollout_items.best(fc)
            if not k:
                return {}
            self.stats["item_buys"].append([gs.get("ante_num"), fc["options"][k]["desc"],
                                            fc["options"][k]["vs_skip"]["blinds"]])
            return {k: 3.0}

        def system_one(self, state, questions):
            answers = {}
            picks = self.item_pick(questions) if items else {}
            for qid, q in questions.items():
                if q.type == "choice":
                    keys = list(q.criteria)
                    c = "continue_shopping" if "continue_shopping" in keys else keys[0]
                    cash = [k for k in picks if k in keys]
                    if "continue_shopping" in keys and cash:
                        c = cash[0]
                    answers[qid] = {"type": "choice", "choice": c, "confidence": 1.0,
                                    "probabilities": {k: float(k == c) for k in keys}}
                elif q.type == "noul":
                    sp = _d(state.get("strongest_play"))
                    bd = _d(state.get("best_draw_by_measurement"))
                    play = sp.get("clears_blind_alone") or "modeled_average_next_play" not in bd or                         (sp.get("estimated_score") or 0) >= (bd.get("modeled_average_next_play") or 0)
                    answers[qid] = {"type": "noul", "noul": 1.0 if play else 0.0}
                else:
                    sc = max(self.item_score(q), picks.get(qid, 0.0))
                    answers[qid] = {"type": "score", "score": sc, "confidence": 1.0, "legend": {},
                                    "probabilities": {}}
            return SystemOneResponse.model_validate({"model": self.model, "answers": answers,
                                                     "usage": {"input_tokens": 0, "output_tokens": 0}})

        def item_score(self, q):
            item = _d(_d(q.instructions).get("item")) if isinstance(q.instructions, dict) else {}
            gs = bot_ref[0].last_state or {}
            hands = _d(gs.get("hands"))
            top = max(hands.items(), key=lambda kv: (_d(kv[1]).get("played") or 0, _d(kv[1]).get("level") or 0),
                      default=(None, {}))
            if (opts or {}).get("planet_hand") == "mix" and item.get("kind") == "planet":
                import readiness
                mix = readiness.play_mix(gs)
                if mix:
                    h = max(mix, key=mix.get)
                    top = (h, {**_d(hands.get(h)), "played": 1})
            if item.get("kind") == "planet" and top[0] and (_d(top[1]).get("played") or 0) > 0                     and f"Level up {top[0]} " in (item.get("what") or "") and (item.get("money_after") or 0) >= 4:
                return 4.0
            return 0.0

    return CodeJev()


def _play_bot(seed, arm_name, t0):
    import shutil
    import tempfile
    import decisions
    from jev_bot import BenchmarkComplete, JevBot
    from sim_backend import SimBalatro
    import rollout_items
    opts = BOT_ARMS[arm_name]
    # = arm C (n=16, 3 bosses, no time budget) unless the arm scales compute; set for every game, since a
    # worker process plays games of several arms
    decisions.FORECAST_N, decisions.FORECAST_BUDGET = opts.get("n", FORECAST_N), None
    decisions.FORECAST_BOSSES = opts.get("bosses", 3)
    _DEFAULTS.setdefault("rr", (decisions.MAX_REROLLS, decisions.INTEREST_BUFFER))
    decisions.MAX_REROLLS = opts.get("max_rerolls", _DEFAULTS["rr"][0])
    decisions.INTEREST_BUFFER = opts.get("buffer", _DEFAULTS["rr"][1])
    _DEFAULTS.setdefault("pv", rollout_items.Picker.planet_value)
    if opts.get("planet_pick") == "reopt":
        def planet_value(self, c):
            k = ("p", c.get("key"))
            if k not in self.memo:
                import jokers as text_jokers
                lv = text_jokers.planet_level(_d(c.get("value")).get("effect"))
                self.memo[k] = max(_reopt_gain(self.pub, lv) if lv else 0.0, 0.0) + 0.001
            return self.memo[k]
        rollout_items.Picker.planet_value = planet_value
    else:
        rollout_items.Picker.planet_value = _DEFAULTS["pv"]
    rollout_items.synth_plays.__defaults__ = (opts.get("synth", rollout_items.SYNTH_HANDS), "SYNTH")
    decisions.VOUCHERS = {k: (v[0], False) for k, v in decisions.VOUCHERS.items()}  # no v18 strong-voucher rule
    tmp = tempfile.mkdtemp(prefix="plab_")
    orig_selecting_hand = decisions.selecting_hand
    import rollout_shop
    shop_patch = rollout_shop.Patched() if opts.get("shop") else None
    if shop_patch:
        shop_patch.__enter__()
    try:
        ref = [None]
        jev = _code_jev(ref, items=opts.get("items", False), opts=opts)
        pstats = {"evals": 0, "plans": 0, "seconds": 0.0, "plays": 0, "discards": 0, "errors": 0}
        if opts.get("planner"):
            import round_planner
            decisions.selecting_hand = round_planner.wrap(orig_selecting_hand, pstats)
        bot = JevBot(deck="RED", stake="WHITE", game=SimBalatro(), jev=jev, log_dir=Path(tmp))
        ref[0] = bot
        bot.model = jev.model
        bot.act("start", {"seed": seed, "deck": "RED", "stake": "WHITE"})
        engine_won = False
        for _ in range(20000):
            try:
                bot.step()
            except BenchmarkComplete:
                break
            except RuntimeError as e:
                # jev_bot's victory guard pauses for review when it can't verify the final boss (e.g. after
                # Hieroglyph moved the ante counter); the engine's own won flag is the ground truth here.
                if "Victory reported" in str(e) and bot.game.gs.get("won"):
                    engine_won = True
                    break
                raise
            if bot.stats["runs"] >= 1 and (bot.last_state or {}).get("state") == "MENU":
                break
        r = bot.recorder
        summ = json.loads((r.dir / "summary.json").read_text(encoding="utf-8"))
        ev = [json.loads(l) for l in open(r.dir / "decisions.jsonl", encoding="utf-8")]
        blinds = sum(1 for e in ev if e["kind"] == "round_end" and (e.get("scored") or 0) >= (e.get("target") or 1))
        rerolls = sum(1 for e in ev if e["kind"] == "action" and e.get("method") == "reroll")
        buys = [(e.get("ante"), "auto", e.get("desc")) for e in ev if e["kind"] == "auto" and e.get("question") == "shop"]
        buys += [(e.get("ante"), "jev", e.get("choice")) for e in ev if e["kind"] == "decision"
                 and e.get("question") == "shop" and str(e.get("choice")).startswith(("buy", "replace"))]
        fs = [e.get("forecast_seconds") or 0 for e in ev if e.get("forecast_seconds")]
        ifs = [e.get("item_forecast_seconds") or 0 for e in ev if e.get("item_forecast_seconds")]
        won = summ.get("result") == "won" or engine_won
        death = None
        if not won:
            ends = [e for e in ev if e["kind"] == "round_end"]
            plays = [e for e in ev if e["kind"] == "outcome" and e.get("method") == "play"]
            last_end = ends[-1]["id"] if ends else -1
            fatal = [p for p in plays if p["id"] > last_end]
            death = _death(bot._round_blind or {}, (fatal[-1].get("chips") or 0) if fatal else 0,
                           [p.get("chips_delta") or 0 for p in fatal], (plays[-1].get("jokers") if plays else []) or [])
        return {"seed": seed, "arm": arm_name, "won": won, "ante": 8 if won else summ.get("ante"), "blinds": blinds,
                "round": summ.get("round"), "rerolls": rerolls, "reroll_spent": 0, "spent": 0,
                "buys": [[b[0], "swap" if "replace" in str(b[2]) else "joker", b[2]] for b in buys],
                "forecasts": len(fs), "forecast_errors": 0, "forecast_s": round(sum(fs), 1),
                "game_errors": bot.stats["errors"], "seconds": round(time.time() - t0, 1), "death": death,
                "prompt_version": summ.get("prompt_version"),
                "item_fc": len(ifs), "item_fc_s": round(sum(ifs), 1),
                **({"shop_stats": dict(rollout_shop.TOTALS)} if shop_patch else {}),
                **({"item_forecasts": jev.stats["item_forecasts"], "item_s": round(jev.stats["item_seconds"], 1),
                    "item_errors": jev.stats["item_errors"], "item_buys": jev.stats["item_buys"],
                    "pack_picks": jev.stats["pack_picks"]} if opts.get("items") else {}),
                **({"planner_evals": pstats["evals"], "planner_plans": pstats["plans"], "planner_s": round(pstats["seconds"], 1),
                    "planner_plays": pstats["plays"], "planner_discards": pstats["discards"],
                    "planner_errors": pstats["errors"]} if opts.get("planner") else {})}
    finally:
        decisions.selecting_hand = orig_selecting_hand  # workers play many games: never leak the wrapper
        if shop_patch:
            shop_patch.__exit__(None, None, None)
        keep = os.environ.get("PLAB_KEEP_LOGS")  # a dir: keep each game's decisions.jsonl for diagnostics
        if keep:
            for f in Path(tmp).rglob("decisions.jsonl"):
                Path(keep).mkdir(parents=True, exist_ok=True)
                shutil.copyfile(f, Path(keep) / f"{seed}-{arm_name}.jsonl")
        shutil.rmtree(tmp, ignore_errors=True)


# ---------------------------------------------------------------------------
# Runner and report
# ---------------------------------------------------------------------------

def seeds(n, salt=20261016):
    import sim_backend
    rng = random.Random(salt)
    return [sim_backend.random_seed(rng) for _ in range(n)]


def load(name):
    p = OUT / name / "results.jsonl"
    rows = [json.loads(l) for l in open(p, encoding="utf-8")] if p.exists() else []
    return [r for r in rows if not r.get("error")], [r for r in rows if r.get("error")]


def paired(rows, a, b, key="ante"):
    by = {}
    for r in rows:
        by.setdefault(r["seed"], {})[r["arm"]] = r
    d = [float(v[a][key]) - float(v[b][key]) for v in by.values() if a in v and b in v]
    if len(d) < 2:
        return None
    return {"n": len(d), "diff": round(statistics.mean(d), 3), "se": round(statistics.stdev(d) / math.sqrt(len(d)), 3),
            "sd": round(statistics.stdev(d), 3)}


def _item_name(desc):
    """'$4 Buffoon Pack' -> 'Buffoon Pack' (Jumbo/Mega folded in)."""
    d = str(desc).split(" ", 1)[-1]
    return d.replace("Jumbo ", "").replace("Mega ", "")


def _blind_kind(d):
    return "boss" if d["type"] == "BOSS" else (d["blind"] or "?").split()[0].lower()


def death_stats(rs):
    """Loss-audit tables for one arm's lost games (docs/loss-audit.md): where and by how much they die."""
    lost = [r for r in rs if not r["won"] and r.get("death")]
    if not lost:
        return None
    ds = [r["death"] for r in lost]
    ratio = [d["final"] / d["target"] for d in ds if d["target"]]
    share = [d["best_hand"] / d["target"] for d in ds if d["target"]]
    buckets = Counter("<25%" if x < .25 else "25-50%" if x < .5 else "50-75%" if x < .75 else "75-100%" for x in ratio)
    kind = Counter(_blind_kind(d) for d in ds)
    return {
        "losses": len(ds), "at_boss": kind.get("boss", 0), "at_small": kind.get("small", 0),
        "at_big": kind.get("big", 0), "ante_5_7": sum(5 <= r["ante"] <= 7 for r in lost),
        "median_ratio": round(statistics.median(ratio), 3) if ratio else None,
        "close_75_100": buckets.get("75-100%", 0),
        "median_best_hand_share": round(statistics.median(share), 3) if share else None,
        "water": sum(d["blind"] == "The Water" for d in ds),
        "shortfall": {k: buckets.get(k, 0) for k in ("<25%", "25-50%", "50-75%", "75-100%")},
        "fatal_bosses": dict(Counter(d["blind"] for d in ds if d["type"] == "BOSS").most_common()),
        "boss_effects": {d["blind"]: d["effect"] for d in ds if d["type"] == "BOSS" and d.get("effect")},
        "median_jokers_at_death": statistics.median(len(d["jokers"] or []) for d in ds),
        "by_ante_blind": {f"{a} {k}": n for (a, k), n in sorted(Counter((r["ante"], _blind_kind(r["death"]))
                                                                         for r in lost).items())},
    }


def tables(out):
    """Markdown tables of a report: arms, paired differences, death ante, death blind and shortfall."""
    arms = out["arms"]
    lines = ["| arm | games | mean ante (SE) | ante-8 clear | blinds cleared | forecasts/game | s/forecast |",
             "|---|---:|---:|---:|---:|---:|---:|"]
    for a, r in arms.items():
        lines.append(f"| {a} | {r['games']} | {r['mean_ante']:.2f} ({r['se_ante']:.2f}) | {100 * r['win_rate']:.1f}% "
                     f"| {r['mean_blinds']:.1f} | {r['mean_forecasts']} | {r['forecast_seconds_per_call']} |")
    lines += ["", "| paired | ante | blinds cleared | ante-8 clear |", "|---|---:|---:|---:|"]
    for k, v in out["paired"].items():
        if v["ante"]:
            lines.append(f"| {k} | {v['ante']['diff']:+.2f} ± {v['ante']['se']:.2f} | {v['blinds']['diff']:+.2f} ± "
                         f"{v['blinds']['se']:.2f} | {100 * v['won']['diff']:+.1f} ± {100 * v['won']['se']:.1f} pts |")
    cols = sorted({k for r in arms.values() for k in r["death_ante"]},
                  key=lambda k: (k == "won", int(k) if k != "won" else 0))
    lines += ["", "| death ante | " + " | ".join(cols) + " |", "|---|" + "---:|" * len(cols)]
    for a, r in arms.items():
        lines.append(f"| {a} | " + " | ".join(str(r["death_ante"].get(c, "")) for c in cols) + " |")
    dead = {a: r["deaths"] for a, r in arms.items() if r.get("deaths")}
    if dead:
        head = " | ".join(dead)
        lines += ["", f"| deaths | {head} |", "|---|" + "---:|" * len(dead)]
        for label, key in (("losses", "losses"), ("died at a boss", "at_boss"), ("died at a small blind", "at_small"),
                           ("died at a big blind", "at_big"), ("died at ante 5-7", "ante_5_7"),
                           ("median final score / target", "median_ratio"),
                           ("within 75-100% of target (close calls)", "close_75_100"),
                           ("median best hand in fatal round / target", "median_best_hand_share"),
                           ("deaths to The Water", "water"), ("median jokers at death", "median_jokers_at_death")):
            lines.append(f"| {label} | " + " | ".join(str(d.get(key)) for d in dead.values()) + " |")
        lines += ["", f"| final score / target | {head} |", "|---|" + "---:|" * len(dead)]
        for b in ("<25%", "25-50%", "50-75%", "75-100%"):
            lines.append(f"| {b} | " + " | ".join(str(d["shortfall"][b]) for d in dead.values()) + " |")
        bosses = Counter()
        for d in dead.values():
            bosses.update(d["fatal_bosses"])
        lines += ["", f"| fatal boss | {head} |", "|---|" + "---:|" * len(dead)]
        for b, _ in bosses.most_common(12):
            lines.append(f"| {b} | " + " | ".join(str(d["fatal_bosses"].get(b, 0)) for d in dead.values()) + " |")
    return chr(10).join(lines)


def sequential_check(rows, a, b, stop_se=3.0, min_gain=0.15):
    """Sequential stopping on the paired ante difference a - b over the seeds both arms finished.
    Stop when |mean| > stop_se SE (a clear difference either way) or when the 95% CI upper bound is
    below min_gain (no meaningful gain for a). Returns (stop, reason, paired dict or None)."""
    d = paired(rows, a, b)
    if not d:
        return False, "fewer than 2 pairs", None
    hi = d["diff"] + 1.96 * d["se"]
    if d["se"] > 0 and abs(d["diff"]) > stop_se * d["se"]:
        return True, f"|{d['diff']:+.3f}| > {stop_se} SE ({d['se']:.3f})", d
    if hi < min_gain:
        return True, f"95% CI upper bound {hi:+.3f} < +{min_gain} ante", d
    return False, f"{d['diff']:+.3f} ± {d['se']:.3f} (CI upper {hi:+.3f})", d


def report(name, pairs=None, only=None):
    rows, errors = load(name)
    if only:
        rows = [r for r in rows if r["arm"] in only]
    arms = sorted({r["arm"] for r in rows})
    seeds_all = {}
    for r in rows:
        seeds_all.setdefault(r["seed"], set()).add(r["arm"])
    complete = {s for s, a in seeds_all.items() if a >= set(arms)}
    rows = [r for r in rows if r["seed"] in complete]
    out = {"name": name, "seeds_complete": len(complete), "errors": len(errors), "arms": {}}
    for a in arms:
        rs = [r for r in rows if r["arm"] == a]
        if not rs:
            continue
        deaths = {}
        for r in rs:
            k = "won" if r["won"] else str(r["ante"])
            deaths[k] = deaths.get(k, 0) + 1
        out["arms"][a] = {
            "games": len(rs), "mean_ante": round(statistics.mean(r["ante"] for r in rs), 3),
            "se_ante": round(statistics.stdev(r["ante"] for r in rs) / math.sqrt(len(rs)), 3) if len(rs) > 1 else None,
            "win_rate": round(sum(r["won"] for r in rs) / len(rs), 3),
            "mean_blinds": round(statistics.mean(r["blinds"] for r in rs), 2),
            "death_ante": dict(sorted(deaths.items())),
            "mean_rerolls": round(statistics.mean(r["rerolls"] for r in rs), 2),
            "mean_reroll_spent": round(statistics.mean(r["reroll_spent"] for r in rs), 1),
            "mean_spent": round(statistics.mean(r["spent"] for r in rs), 1),
            "mean_buys": round(statistics.mean(len(r["buys"]) for r in rs), 2),
            "mean_swaps": round(statistics.mean(sum(b[1] == "swap" for b in r["buys"]) for r in rs), 2),
            "mean_vouchers": round(statistics.mean(sum(b[1] == "voucher" for b in r["buys"]) for r in rs), 2),
            "first_buy_ante_mean": round(statistics.mean([r["buys"][0][0] for r in rs if r["buys"] and r["buys"][0][0]] or [0]), 2),
            "mean_game_seconds": round(statistics.mean(r["seconds"] for r in rs), 1),
            "mean_forecasts": round(statistics.mean(r["forecasts"] for r in rs), 1),
            "forecast_seconds_per_call": round(sum(r["forecast_s"] for r in rs) / max(1, sum(r["forecasts"] for r in rs)), 2),
            "deaths": death_stats(rs),
        }
        if any("planner_plans" in r for r in rs):
            plans = sum(r.get("planner_plans", 0) for r in rs)
            evals = sum(r.get("planner_evals", 0) for r in rs)
            out["arms"][a].update({
                "mean_planner_evals": round(evals / len(rs), 1),
                "mean_planner_overrides": round(plans / len(rs), 1),
                "planner_seconds_per_eval": round(sum(r.get("planner_s", 0) for r in rs) / max(1, evals), 2),
                "planner_discard_share": round(sum(r.get("planner_discards", 0) for r in rs) / max(1, plans), 2),
                "planner_errors": sum(r.get("planner_errors", 0) for r in rs),
            })
        if any("item_forecasts" in r for r in rs):
            calls = sum(r.get("item_forecasts", 0) for r in rs)
            out["arms"][a].update({
                "mean_item_forecasts": round(calls / len(rs), 1),
                "item_seconds_per_call": round(sum(r.get("item_s", 0) for r in rs) / max(1, calls), 2),
                "item_errors": sum(r.get("item_errors", 0) for r in rs),
                "mean_item_buys": round(statistics.mean(len(r.get("item_buys", [])) for r in rs), 2),
                "mean_pack_picks": round(statistics.mean(len(r.get("pack_picks", [])) for r in rs), 2),
                "item_buys_top": dict(Counter(_item_name(b[1]) for r in rs for b in r.get("item_buys", [])).most_common(12)),
                "pack_picks_by_set": dict(Counter(p[1] for r in rs for p in r.get("pack_picks", [])).most_common()),
            })
    pairs = pairs or [(x, y) for x, y in (("forecast", "gain"), ("forecast-long", "gain"), ("forecast", "none"),
                                          ("forecast-long", "forecast"), ("gain", "none")) if x in arms and y in arms]
    pairs += [(x, "forecast-long") for x in arms if x.startswith("forecast-long-") and "forecast-long" in arms]
    pairs += [(x, "gain") for x in arms if x.startswith("forecast-long-") and "gain" in arms]
    pairs += [(x, y) for x, y in (("bot-forecast-items", "bot-forecast-long"), ("bot-forecast-long", "forecast-long"),
                                  ("bot-planner", "bot-forecast-long"), ("bot-heavy", "bot-forecast-long"),
                                  ("bot-mid", "bot-forecast-long"), ("bot-heavy", "bot-mid"),
                                  ("bot-forecast-shop", "bot-forecast-long"))
              if x in arms and y in arms and (x, y) not in pairs]
    out["paired"] = {f"{x} - {y}": {"ante": paired(rows, x, y), "blinds": paired(rows, x, y, "blinds"),
                                    "won": paired(rows, x, y, "won")} for x, y in pairs}
    return out


def main():
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seeds", type=int, default=50)
    ap.add_argument("--seed-offset", type=int, default=0, help="skip the first k generated seeds")
    ap.add_argument("--only-done", metavar="ARM",
                    help="restrict the seeds to those this run already finished for ARM (e.g. pair a new arm with "
                         "results copied in from another run)")
    ap.add_argument("--set", help="play a fixed seed set from src/benchmark.py (e.g. sim60-a) instead of generated "
                                  "seeds (--seeds and --seed-offset then ignored)")
    ap.add_argument("--arms", default="gain,forecast,forecast-long,none")
    ap.add_argument("--name", default="pilot")
    ap.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) - 1))
    ap.add_argument("--report", help="print the report of a run name and exit (restricted to --arms if given)")
    ap.add_argument("--stop-at-se", type=float, default=3.0,
                    help="sequential stopping on the paired ante difference of the first two --arms (first minus "
                         "second): stop when |diff| > this many SE; 0 disables")
    ap.add_argument("--check-every", type=int, default=100, help="check the stopping rule every N completed pairs")
    ap.add_argument("--min-gain", type=float, default=0.15,
                    help="also stop when the 95%% CI upper bound of the diff is below this (no meaningful gain)")
    a = ap.parse_args()
    if a.report:
        out = report(a.report, only=set(a.arms.split(",")) if "--arms" in sys.argv else None)
        print(json.dumps(out, indent=1))
        print(tables(out))
        return 0
    d = OUT / a.name
    d.mkdir(parents=True, exist_ok=True)
    done = {(r["seed"], r["arm"]) for r in load(a.name)[0]}
    arms = a.arms.split(",")
    if a.set:
        import benchmark
        ss = list(dict.fromkeys(x.split("#")[0] for x in benchmark.SETS[a.set]))
    else:
        ss = seeds(a.seed_offset + a.seeds)[a.seed_offset:]
    if a.only_done:
        have = {s for s, arm in done if arm == a.only_done}
        ss = [s for s in ss if s in have]
    # slow arms first so the pool drains evenly; the bot arms (slowest) interleaved by seed so pairs fill evenly
    order = sorted(arms, key=lambda x: (x not in BOT_ARMS, not x.startswith("forecast"), x))
    pos = {s: i for i, s in enumerate(ss)}
    tasks = sorted(((s, arm) for arm in order if arm in BOT_ARMS for s in ss if (s, arm) not in done),
                   key=lambda t: pos[t[0]])
    tasks += [(s, arm) for arm in order if arm not in BOT_ARMS for s in ss if (s, arm) not in done]
    print(f"{len(tasks)} games ({len(done)} done), {a.jobs} workers", flush=True)
    t0 = time.time()
    seq = a.stop_at_se > 0 and len(arms) >= 2
    rows = load(a.name)[0] if seq else []
    checked = len({r["seed"] for r in rows if r["arm"] == arms[0]} & {r["seed"] for r in rows if r["arm"] == arms[1]}) \
        // a.check_every if seq else 0
    with open(d / "results.jsonl", "a", encoding="utf-8") as f, mp.get_context("spawn").Pool(a.jobs) as pool:
        for i, r in enumerate(pool.imap_unordered(play_game, tasks), 1):
            f.write(json.dumps(r) + "\n")
            f.flush()
            if i % 25 == 0 or r.get("error"):
                print(json.dumps({"done": i, "of": len(tasks), "elapsed": round(time.time() - t0),
                                  **({"error": r["error"], "seed": r["seed"], "arm": r["arm"]} if r.get("error") else {})}),
                      flush=True)
            if seq and not r.get("error"):
                rows.append(r)
                n = len({x["seed"] for x in rows if x["arm"] == arms[0]} & {x["seed"] for x in rows if x["arm"] == arms[1]})
                if n // a.check_every > checked:
                    checked = n // a.check_every
                    stop, why, _ = sequential_check(rows, arms[0], arms[1], a.stop_at_se, a.min_gain)
                    print(json.dumps({"check": f"{arms[0]} - {arms[1]}", "pairs": n, "stop": stop, "why": why}),
                          flush=True)
                    if stop:
                        pool.terminate()
                        break
    out = report(a.name)
    print(json.dumps(out, indent=1))
    print(tables(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
