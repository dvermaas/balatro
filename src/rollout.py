"""Rollout-based shop forecast (phase 1: offline, not wired into the policy, no JEV calls).

From a PUBLIC BalatroBot gamestate in the shop (the dict the bot receives), build a Jackdaw game
state, apply each candidate shop option (buy item i, sell-swap, voucher, skip/save), and play the
next blinds through the next `bosses` boss blinds with the build FROZEN (no further purchases;
planets are used as soon as they are held; scaling jokers scale in the engine) using a fast code
policy for play/discard. Returns, per option, P(clear) per upcoming blind, P(survive the horizon)
and the score/target margin at each boss, with standard errors.

What the reconstruction may use (public only): deck CONTENTS as an order-free multiset (the draw
pile order is never read: every rollout reshuffles with its own fresh RNG), hand levels, jokers
with their dynamic values read from effect text, enhancements/editions/seals, money, ante, round,
vouchers, the known current-ante boss. Future bosses come from the engine's own pool sampling with
the rollout's RNG. Rollout k of every option uses the same seed (common random numbers: Balatro's
RNG is keyed by stream name, so shuffles and boss draws line up across options).

Not reconstructable from public state (documented gaps): past bosses (boss-pool usage counts),
skips without a Throwback, unique planets used without a Satellite, tarots used without a Fortune
Teller, Obelisk/Loyalty-style hidden counters beyond what their text shows.

The policy is shared with the ground-truth validator (scripts/validate_rollout.py): it reads the
public joker texts once (jokers.build) and the engine's hand cards each decision, so a branch of
the TRUE engine state and a rollout of the public reconstruction play by identical rules.
"""
from __future__ import annotations

import copy
import math
import re
import statistics
import time
from collections import Counter

import sim_backend  # puts Jackdaw on sys.path

sim_backend.apply_patches()

from jackdaw.engine.actions import (  # noqa: E402
    BuyCard, CashOut, Discard, GamePhase, NextRound, PlayHand, RedeemVoucher, SelectBlind, SellCard,
    SkipPack, UseConsumable)
from jackdaw.engine.card import Card  # noqa: E402
from jackdaw.engine.data.hands import HandType  # noqa: E402
from jackdaw.engine.data.prototypes import BLINDS  # noqa: E402
from jackdaw.engine.game import IllegalActionError, step  # noqa: E402
from jackdaw.engine.rng import PseudoRandom  # noqa: E402
from jackdaw.engine.run_init import initialize_run  # noqa: E402
from jackdaw.engine.vouchers import apply_voucher  # noqa: E402

import jokers as text_jokers  # noqa: E402  (read-only use of the bot's text scorer)
from hands import RANK_ORDER, classify  # noqa: E402

SUIT_NAME = {"H": "Hearts", "D": "Diamonds", "S": "Spades", "C": "Clubs"}
SUIT_CHAR = {v: k for k, v in SUIT_NAME.items()}
RANK_NAME = {"A": "Ace", "K": "King", "Q": "Queen", "J": "Jack", "T": "10"}
RANK_CHAR = {"Ace": "A", "King": "K", "Queen": "Q", "Jack": "J", "10": "T", **{str(n): str(n) for n in range(2, 10)}}
RANK_ID = {"Ace": 14, "King": 13, "Queen": 12, "Jack": 11, **{str(n): n for n in range(2, 11)}}
ENH_KEY = {k.upper(): "m_" + k for k in ("bonus", "mult", "wild", "glass", "steel", "stone", "gold", "lucky")}
ENH_NAME = {v: k for k, v in ENH_KEY.items()}
EDITIONS = ("foil", "holo", "polychrome", "negative")
DECK_KEY = {v: k for k, v in sim_backend._DECKS.items()}
STAKE_NUM = {v: k for k, v in sim_backend._STAKES.items()}
BOSS_KEY = {p.name: k for k, p in BLINDS.items()}
NUM = r"(\d+(?:\.\d+)?)"


def _d(x):
    return x if isinstance(x, dict) else {}


def _cards(gs, area):
    return _d(gs.get(area)).get("cards") or []


# ---------------------------------------------------------------------------
# Public gamestate -> Jackdaw objects
# ---------------------------------------------------------------------------

def _edition(card, mod):
    ed = (mod.get("edition") or "").lower()
    if ed in EDITIONS:
        card.set_edition({ed: True})


def playing_card(src):
    """A playing card from its public dict (rank, suit, enhancement, edition, seal, Hiker chips)."""
    v, m = _d(src.get("value")), _d(src.get("modifier"))
    rank, suit = v.get("rank"), v.get("suit")
    c = Card()
    c.set_base(f"{suit}_{rank}", SUIT_NAME[suit], RANK_NAME.get(rank, rank))
    enh = m.get("enhancement")
    c.set_ability(ENH_KEY.get(enh, "c_base"))
    extra = re.search(r"\+(\d+) extra chips", v.get("effect") or "", re.I)
    if extra:
        c.ability["perma_bonus"] = int(extra[1]) - (30 if enh == "BONUS" else 0)
    _edition(c, m)
    if m.get("seal"):
        c.seal = str(m["seal"]).title()
    c.debuff = bool(_d(src.get("state")).get("debuff"))
    cost = _d(src.get("cost"))
    c.cost, c.sell_cost = cost.get("buy", 0), cost.get("sell", 0)
    return c


def _num(pattern, text, default=None, cast=float):
    m = re.search(pattern, text)
    return cast(m[1]) if m else default


def _joker_dynamic(c, key, text, ctx):
    """Set a joker's run-dependent values from its public effect text (see sim_text._joker_vars)."""
    a = c.ability
    ex = a.get("extra")
    if key in ("j_green_joker", "j_ceremonial", "j_trousers", "j_ride_the_bus", "j_red_card", "j_flash"):
        a["mult"] = _num(r"Currently \+" + NUM + " Mult", text, a.get("mult"))
    elif key == "j_popcorn":
        a["mult"] = _num(r"^\+" + NUM + " Mult", text, a.get("mult"))
    elif key in ("j_runner", "j_wee", "j_castle", "j_square") and isinstance(ex, dict):
        ex["chips"] = _num(r"Currently \+?" + NUM + " Chips", text, ex.get("chips"))
    elif key == "j_ice_cream" and isinstance(ex, dict):
        ex["chips"] = _num(r"^\+" + NUM + " Chips", text, ex.get("chips"))
    elif key in ("j_constellation", "j_throwback", "j_glass", "j_hit_the_road", "j_madness", "j_vampire",
                 "j_hologram", "j_obelisk", "j_lucky_cat", "j_campfire", "j_yorick"):
        a["x_mult"] = _num(r"Currently X" + NUM + " Mult", text, a.get("x_mult"))
        if key == "j_throwback":
            ctx["skips"] = round((a["x_mult"] - 1) / 0.25)
        if key == "j_yorick":
            a["yorick_discards"] = _num(r"\[(\d+)\]", text, a.get("yorick_discards"), int)
    elif key == "j_caino":
        a["caino_xmult"] = _num(r"Currently X" + NUM + " Mult", text, a.get("caino_xmult"))
    elif key == "j_ramen":
        a["x_mult"] = _num(r"^X" + NUM + " Mult", text, a.get("x_mult"))
    elif key == "j_loyalty_card" and isinstance(ex, dict):
        every = ex.get("every", 5)
        rem = 0 if "Active" in text else _num(r"(\d+) remaining", text, every, int)
        # sim_text: rem = (every - (hands_played - at_create)) % (every + 1)
        a["hands_played_at_create"] = ctx["hands_played"] - (every - rem)
    elif key == "j_invisible":
        a["invis_rounds"] = _num(r"Currently (\d+)/", text, 0, int)
    elif key == "j_selzer":
        a["extra"] = _num(r"next (\d+) hands", text, ex, int)
    elif key == "j_turtle_bean" and isinstance(ex, dict):
        ex["h_size"] = _num(r"^\+(\d+) hand size", text, ex.get("h_size"), int)
    elif key == "j_rocket" and isinstance(ex, dict):
        ex["dollars"] = _num(r"Earn \$(\d+) at end", text, ex.get("dollars"), int)
    elif key == "j_fortune_teller":
        ctx["tarots_used"] = _num(r"Currently \+(\d+)", text, 0, int)
    elif key == "j_satellite":
        ctx["planets_used"] = _num(r"Currently \$(\d+)", text, 0, int)
    elif key == "j_ancient":
        s = _num(r"with (Heart|Diamond|Club|Spade) suit", text, None, str)
        if s:
            ctx["ancient"] = s + "s"
    elif key == "j_idol":
        m = re.search(r"Each played (\w+) of (Hearts|Diamonds|Clubs|Spades)", text)
        if m:
            ctx["idol"] = (m[1], m[2])
    elif key == "j_mail":
        r = _num(r"discarded (\w+), rank", text, None, str)
        if r:
            ctx["mail"] = r
    elif key == "j_castle":
        pass
    elif key == "j_todo_list":
        h = _num(r"poker hand is an? (.+?), poker hand", text, None, str)
        if h and h in {t.value for t in HandType}:
            a["to_do_poker_hand"] = h
    if key == "j_castle":
        s = _num(r"discarded (Heart|Diamond|Club|Spade) card", text, None, str)
        if s:
            ctx["castle"] = s + "s"


def joker_card(src, ctx):
    key = src.get("key")
    c = Card()
    c.set_ability(key, hands_played=ctx["hands_played"])
    m, st = _d(src.get("modifier")), _d(src.get("state"))
    _edition(c, m)
    if m.get("eternal"):
        c.eternal = True
    if m.get("perishable"):
        c.perishable, c.perish_tally = True, int(m["perishable"])
    if m.get("rental"):
        c.rental = True
    cost = _d(src.get("cost"))
    c.cost, c.sell_cost = cost.get("buy", 0), cost.get("sell", 0)
    c.ability["extra_value"] = max(0, c.sell_cost - max(1, c.cost // 2))
    _joker_dynamic(c, key, _d(src.get("value")).get("effect") or "", ctx)
    c.debuff = bool(st.get("debuff"))
    return c


def item_card(src, ctx):
    """A shop / consumable item (joker, tarot, planet, spectral, voucher, playing card)."""
    if src.get("set") in ("DEFAULT", "ENHANCED"):
        return playing_card(src)
    if src.get("set") == "JOKER":
        return joker_card(src, ctx)
    c = Card()
    c.set_ability(src.get("key"))
    _edition(c, _d(src.get("modifier")))
    cost = _d(src.get("cost"))
    c.cost, c.sell_cost = cost.get("buy", 0), cost.get("sell", 0)
    return c


def build_state(pub, seed="ROLLOUT"):
    """Jackdaw game state (SHOP phase) from a public BalatroBot shop gamestate. Never reads the
    order of pub['cards'] (sorted into a canonical order) nor any engine-internal state."""
    gs = initialize_run(DECK_KEY.get(pub.get("deck") or "RED", "b_red"), STAKE_NUM.get(pub.get("stake") or "WHITE", 1),
                        seed)
    hands = _d(pub.get("hands"))
    ctx = {"hands_played": sum((_d(h).get("played") or 0) for h in hands.values())}
    for k in _d(pub.get("used_vouchers")):
        gs["used_vouchers"][k] = True
        apply_voucher(k, gs)
    # deck: order-free multiset (canonical sort), the hand area (empty in the shop) included
    deck_src = list(_cards(pub, "cards")) + list(_cards(pub, "hand"))
    deck_src.sort(key=lambda c: (str(_d(c.get("value")).get("suit")), str(_d(c.get("value")).get("rank")),
                                 str(_d(c.get("modifier")))))
    gs["deck"] = [playing_card(c) for c in deck_src]
    gs["hand"], gs["discard_pile"] = [], []
    gs["playing_cards_count"] = len(gs["deck"])
    gs["jokers"] = []
    for src in _cards(pub, "jokers"):
        j = joker_card(src, ctx)
        j.add_to_deck(gs)
        gs["jokers"].append(j)
    gs["consumables"] = [item_card(c, ctx) for c in _cards(pub, "consumables")]
    gs["hand_size"] = _d(pub.get("hand")).get("limit") or gs["hand_size"]
    gs["joker_slots"] = _d(pub.get("jokers")).get("limit") or gs["joker_slots"]
    gs["consumable_slots"] = _d(pub.get("consumables")).get("limit") or gs["consumable_slots"]
    gs["dollars"] = pub.get("money") or 0
    gs["round"] = pub.get("round_num") or 0
    gs["hands_played"] = ctx["hands_played"]
    gs["skips"] = ctx.get("skips", 0)
    if ctx.get("tarots_used"):
        gs["consumable_usage_total"] = {**gs.get("consumable_usage_total", {}), "tarot": ctx["tarots_used"]}
    if ctx.get("planets_used"):  # Satellite counts unique planets used
        planets = [k for k in ("c_pluto", "c_mercury", "c_uranus", "c_venus", "c_saturn", "c_jupiter",
                               "c_earth", "c_mars", "c_neptune", "c_planet_x", "c_ceres", "c_eris")]
        for k in planets[:ctx["planets_used"]]:
            gs.setdefault("consumeable_usage", {})[k] = {"count": 1, "set": "Planet"}
    hl = gs["hand_levels"]
    for name, h in hands.items():
        s = hl.get_state(HandType(name))
        for attr in ("level", "chips", "mult", "played", "played_this_round"):
            setattr(s, attr, _d(h).get(attr, getattr(s, attr)))
    cr = gs["current_round"]
    if ctx.get("idol"):
        r, s = ctx["idol"]
        cr["idol_card"] = {"rank": r, "suit": s, "id": RANK_ID.get(r, 14)}
    if ctx.get("mail"):
        cr["mail_card"] = {"rank": ctx["mail"], "id": RANK_ID.get(ctx["mail"], 14)}
    if ctx.get("ancient"):
        cr["ancient_card"] = {"suit": ctx["ancient"]}
    if ctx.get("castle"):
        cr["castle_card"] = {"suit": ctx["castle"]}
    # ante, blinds: this ante's known boss and which blinds are done
    ante = pub.get("ante_num") or 1
    rr = gs["round_resets"]
    rr["ante"] = rr["blind_ante"] = ante
    blinds = _d(pub.get("blinds"))
    boss = BOSS_KEY.get(_d(blinds.get("boss")).get("name"))
    if boss:
        rr["blind_choices"]["Boss"] = boss
        gs["bosses_used"][boss] = gs["bosses_used"].get(boss, 0) + 1
    status = {"DEFEATED": "Defeated", "SKIPPED": "Skipped"}
    on_deck = None
    for typ in ("Small", "Big", "Boss"):
        st = _d(blinds.get(typ.lower())).get("status")
        if st in status:
            rr["blind_states"][typ] = status[st]
        else:
            rr["blind_states"][typ] = "Upcoming"
            on_deck = on_deck or typ
    gs["blind_on_deck"] = on_deck or "Small"
    gs["phase"] = GamePhase.SHOP
    gs["shop_cards"] = [item_card(c, ctx) for c in _cards(pub, "shop")]
    gs["shop_vouchers"] = [item_card(c, ctx) for c in _cards(pub, "vouchers")]
    gs["shop_boosters"] = []
    return gs


# ---------------------------------------------------------------------------
# Options
# ---------------------------------------------------------------------------

def options(pub):
    """Candidate shop options from the public state: [{key, desc, actions, supported, cost}].
    actions are engine-level: ('buy', i) / ('voucher', i) / ('sell', j)."""
    money = pub.get("money") or 0
    jok, cons = _d(pub.get("jokers")), _d(pub.get("consumables"))
    free_j = len(_cards(pub, "jokers")) < (jok.get("limit") or 5)
    free_c = len(_cards(pub, "consumables")) < (cons.get("limit") or 2)
    out = [{"key": "skip", "desc": f"buy nothing (keep ${money})", "actions": [], "supported": True, "cost": 0}]
    for i, c in enumerate(_cards(pub, "shop")):
        cost = _d(c.get("cost")).get("buy", 0)
        neg = _d(c.get("modifier")).get("edition") == "NEGATIVE"
        st = c.get("set")
        if cost > money or (st == "JOKER" and not free_j and not neg) or (
                st in ("TAROT", "PLANET", "SPECTRAL") and not free_c and not neg):
            continue
        out.append({"key": f"buy_card_{i}", "desc": f"${cost} {c.get('label')}", "actions": [("buy", i)],
                    "supported": st in ("JOKER", "PLANET", "DEFAULT", "ENHANCED"), "cost": cost, "set": st})
    for i, c in enumerate(_cards(pub, "vouchers")):
        cost = _d(c.get("cost")).get("buy", 0)
        if cost <= money:
            out.append({"key": f"buy_voucher_{i}", "desc": f"${cost} {c.get('label')}", "actions": [("voucher", i)],
                        "supported": True, "cost": cost, "set": "VOUCHER"})
    if not free_j:
        for j, owned in enumerate(_cards(pub, "jokers")):
            m = _d(owned.get("modifier"))
            if m.get("eternal") or m.get("edition") == "NEGATIVE":
                continue
            sell = _d(owned.get("cost")).get("sell", 0)
            for i, c in enumerate(_cards(pub, "shop")):
                cost = _d(c.get("cost")).get("buy", 0)
                if c.get("set") == "JOKER" and cost <= money + sell:
                    out.append({"key": f"replace_{j}_{i}", "desc": f"sell {owned.get('label')} for {c.get('label')}",
                                "actions": [("sell", j), ("buy", i)], "supported": True, "cost": cost - sell,
                                "set": "JOKER"})
    return out


def apply_option(gs, opt):
    """Apply an option's engine actions (indices match the public areas' order); planets are used."""
    for kind, i in opt["actions"]:
        if kind == "sell":
            step(gs, SellCard(area="jokers", card_index=i))
        elif kind == "voucher":
            step(gs, RedeemVoucher(card_index=i))
        else:
            step(gs, BuyCard(shop_index=i))


def option_jokers(pub, opt):
    """The public joker dicts the build has after the option (the policy's text-scorer input)."""
    js = list(_cards(pub, "jokers"))
    for kind, i in opt["actions"]:
        if kind == "sell":
            js = [c for n, c in enumerate(js) if n != i]
        elif kind == "buy" and _cards(pub, "shop")[i].get("set") == "JOKER":
            js.append(_cards(pub, "shop")[i])
    return js


# ---------------------------------------------------------------------------
# Fast code policy (identical for rollouts and ground-truth branches)
# ---------------------------------------------------------------------------

def _info(c):
    ed = next((e.upper() for e in EDITIONS if c.edition and c.edition.get(e)), None)
    if c.base is None:
        return {"rank": None, "suit": None, "enh": "STONE", "edition": ed, "seal": None, "debuff": c.debuff}
    return {"rank": RANK_CHAR[c.base.rank.value], "suit": SUIT_CHAR[c.base.suit.value],
            "enh": ENH_NAME.get(c.center_key), "edition": ed, "seal": c.seal.upper() if c.seal else None,
            "debuff": bool(c.debuff)}


def _rv(i):
    return RANK_ORDER.index(i["rank"]) if i["rank"] and i["enh"] != "STONE" else -1


def _straight(infos, live):
    order = "A23456789TJQKA"
    by = {}
    for p in live:
        r = infos[p]["rank"]
        if r and infos[p]["enh"] != "STONE":
            by.setdefault(r, p)
    for top in range(len(order) - 1, 3, -1):
        run = order[top - 4:top + 1]
        if all(r in by for r in run):
            return [by[r] for r in run]
    return None


def candidates(infos):
    """Structural play candidates (index lists): rank groups, two pair, full house, flush, straight,
    high card. A small fixed set instead of all 218 subsets, scored by the bot's text scorer."""
    n = len(infos)
    ranked = [p for p in range(n) if infos[p]["rank"] and infos[p]["enh"] != "STONE"]
    groups = {}
    for p in ranked:
        groups.setdefault(infos[p]["rank"], []).append(p)
    gl = sorted(groups.values(), key=lambda g: (len(g), _rv(infos[g[0]])), reverse=True)
    out = []
    multi = [g for g in gl if len(g) >= 2]
    for g in multi[:3]:
        out.append(g[:5])
    if len(multi) >= 2:
        a, b = multi[0], multi[1]
        if len(a) >= 3:
            out.append(a[:3] + b[:2])
        else:
            out.append(a[:2] + b[:2])
    for s in "HDCS":
        cards = [p for p in ranked if infos[p]["suit"] == s or infos[p]["enh"] == "WILD"]
        if len(cards) >= 5:
            out.append(sorted(cards, key=lambda p: -_rv(infos[p]))[:5])
    st = _straight(infos, ranked)
    if st:
        out.append(st)
    if ranked:
        out.append([max(ranked, key=lambda p: _rv(infos[p]))])
    elif n:
        out.append([0])
    return out


class Policy:
    """Play the best text-scored structural candidate; discard toward a flush or rank groups when
    the best play is below the per-hand pace the blind needs."""

    def __init__(self, joker_dicts):
        self.js = text_jokers.build({}, list(joker_dicts))

    @staticmethod
    def levels(gs):
        hl = gs["hand_levels"]
        out = {}
        for ht in HandType:
            s = hl.get_state(ht)
            out[ht.value] = {"chips": s.chips, "mult": s.mult, "level": s.level, "played": s.played,
                             "played_this_round": s.played_this_round}
        return out

    def decide(self, gs):
        hand = gs["hand"]
        infos = [_info(c) for c in hand]
        cr = gs["current_round"]
        blind = gs.get("blind")
        bname = getattr(blind, "name", "") if blind is not None and not getattr(blind, "disabled", False) else ""
        levels = self.levels(gs)
        rnd = {"hands_left": cr.get("hands_left", 1), "discards_left": cr.get("discards_left", 0)}
        need = max(0, (getattr(blind, "chips", 0) or 0) - (gs.get("chips") or 0))
        played_round = {k for k, v in levels.items() if v["played_this_round"]}
        forced = [p for p, c in enumerate(hand) if c.ability.get("forced_selection")]
        best = None
        for combo in candidates(infos):
            combo = list(combo)
            if bname == "The Psychic" and len(combo) < 5:
                rest = sorted((p for p in range(len(infos)) if p not in combo), key=lambda p: _rv(infos[p]))
                combo = combo + rest[:5 - len(combo)]
            for f in forced:
                if f not in combo:
                    combo = (combo[:4] if len(combo) >= 5 else combo) + [f]
            combo = sorted(set(combo))
            name, sc = classify([infos[p] for p in combo])
            s = text_jokers.score(infos, combo, name, sc, levels, self.js, rnd)
            if (bname == "The Eye" and name in played_round) or (
                    bname == "The Mouth" and played_round and name not in played_round):
                s = -1
            if best is None or s > best[0]:
                best = (s, combo, name)
        score, combo, name = best
        hands_left, discards_left = rnd["hands_left"], rnd["discards_left"]
        if discards_left > 0 and hands_left > 1 and score * hands_left < need:
            toss = self.discard(infos, forced)
            if toss:
                return "discard", toss
        if discards_left == 0 and hands_left > 1 and score * hands_left < need and len(combo) < 5:
            combo = self.cycle(infos, combo, name, score, levels, rnd)
        return "play", combo

    def cycle(self, infos, combo, name, score, levels, rnd):
        """No discards left (The Water, or all used) and short of pace: play the lowest spare cards
        alongside the hand, as long as the hand type and its score don't drop, to dig deeper."""
        spare = sorted((p for p in range(len(infos)) if p not in combo), key=lambda p: (not infos[p]["debuff"], _rv(infos[p])))
        out = list(combo)
        for p in spare:
            if len(out) >= 5:
                break
            trial = sorted(out + [p])
            n2, sc2 = classify([infos[q] for q in trial])
            if n2 == name and text_jokers.score(infos, trial, n2, sc2, levels, self.js, rnd) >= score:
                out = trial
        return out

    @staticmethod
    def discard(infos, forced):
        n = len(infos)
        live = [p for p in range(n) if not infos[p]["debuff"]]
        suits = Counter()
        for p in live:
            i = infos[p]
            if i["enh"] == "WILD":
                for s in "HDCS":
                    suits[s] += 1
            elif i["suit"] and i["enh"] != "STONE":
                suits[i["suit"]] += 1
        s, cnt = suits.most_common(1)[0] if suits else (None, 0)
        if cnt >= 4:
            keep = {p for p in live if infos[p]["suit"] == s or infos[p]["enh"] == "WILD"}
        else:
            ranks = Counter(infos[p]["rank"] for p in live if infos[p]["rank"])
            keep = {p for p in live if ranks.get(infos[p]["rank"], 0) >= 2}
            if not keep:  # nothing paired: keep the two highest cards
                keep = set(sorted(live, key=lambda p: -_rv(infos[p]))[:2])
        toss = sorted((p for p in range(n) if p not in keep), key=lambda p: (not infos[p]["debuff"], _rv(infos[p])))[:5]
        for f in forced:  # Cerulean Bell: the forced card is part of every selection, discards too
            if f not in toss:
                toss = toss[:4] + [f]
        return toss


# ---------------------------------------------------------------------------
# Simulation
# ---------------------------------------------------------------------------

def use_planets(gs):
    i = 0
    while i < len(gs.get("consumables", [])):
        c = gs["consumables"][i]
        if c.ability.get("set") == "Planet":
            try:
                step(gs, UseConsumable(card_index=i))
                continue
            except IllegalActionError:
                pass
        i += 1


def play_horizon(gs, policy, bosses=2, max_steps=600):
    """Play from a SHOP state with a frozen build until `bosses` boss blinds are done or the run ends.
    Returns [{ante, type, boss, target, chips, cleared}] per blind."""
    out, cur, done = [], None, 0
    for _ in range(max_steps):
        ph = gs["phase"]
        if ph == GamePhase.SHOP:
            use_planets(gs)
            step(gs, NextRound())
        elif ph == GamePhase.BLIND_SELECT:
            typ = gs.get("blind_on_deck", "Small")
            step(gs, SelectBlind())
            b = gs.get("blind")
            cur = {"ante": gs["round_resets"]["ante"], "type": typ, "boss": getattr(b, "name", typ),
                   "target": getattr(b, "chips", 0)}
        elif ph == GamePhase.SELECTING_HAND:
            kind, idx = policy.decide(gs)
            try:
                step(gs, (PlayHand if kind == "play" else Discard)(card_indices=tuple(idx)))
            except IllegalActionError:
                forced = [p for p, c in enumerate(gs["hand"]) if c.ability.get("forced_selection")]
                step(gs, PlayHand(card_indices=tuple(forced[:1] or [0])))
        elif ph in (GamePhase.ROUND_EVAL, GamePhase.GAME_OVER) or gs.get("won"):
            cleared = ph != GamePhase.GAME_OVER
            out.append({**cur, "chips": gs.get("chips", 0), "cleared": cleared})
            if cur["type"] == "Boss":
                done += 1
            if not cleared or done >= bosses or gs.get("won"):
                return out
            step(gs, CashOut())
        elif ph == GamePhase.PACK_OPENING:
            step(gs, SkipPack())
        else:
            raise RuntimeError(f"unexpected phase {ph}")
    raise RuntimeError("rollout did not finish")


def _seed(k, tag="RO"):
    return f"{tag}{k:06d}"


def summarize(runs, bosses):
    """Aggregate per-option rollouts: [[blind dicts]] -> forecast dict."""
    n = len(runs)
    surv = [1.0 if len(r) and r[-1]["cleared"] and sum(b["type"] == "Boss" for b in r) >= bosses else 0.0
            for r in runs]
    p = sum(surv) / n
    out = {"n": n, "p_survive": round(p, 3), "se_survive": round(math.sqrt(max(p * (1 - p), 1e-9) / n), 3),
           "blinds": []}
    longest = max(runs, key=len)
    for k, b in enumerate(longest):
        reached = [r[k] for r in runs if len(r) > k]
        # P(clear this blind) over all rollouts (a rollout that died earlier did not clear it)
        pc = sum(1 for x in reached if x["cleared"]) / n
        row = {"blind": f"ante {b['ante']} {b['type'].lower()}", "p_clear": round(pc, 3),
               "p_clear_given_reached": round(sum(x["cleared"] for x in reached) / len(reached), 3)}
        if b["type"] == "Boss":
            margins = sorted(x["chips"] / x["target"] for x in reached if x["target"])
            if margins:
                row["median_margin"] = round(statistics.median(margins), 2)
                row["bosses_seen"] = dict(Counter(x["boss"] for x in reached).most_common(4))
        out["blinds"].append(row)
    out["blind_survival"] = [r["p_clear"] for r in out["blinds"]]
    # A continuous score for ranking ties: expected number of blinds cleared + boss margin
    out["exp_blinds_cleared"] = round(sum(sum(b["cleared"] for b in r) for r in runs) / n, 2)
    out["horizon_blinds"] = len(longest)
    boss = next((b for b in out["blinds"] if b["blind"].endswith("boss")), None)
    out["p_next_boss"] = round(sum(m_next_boss(r) for r in runs) / n, 3)
    if boss:
        out["next_boss"] = boss["blind"]
        out["next_boss_margin"] = boss.get("median_margin")
        out["next_boss_names"] = boss.get("bosses_seen")
    return out


def m_survive(r, bosses):
    return 1.0 if len(r) and r[-1]["cleared"] and sum(b["type"] == "Boss" for b in r) >= bosses else 0.0


def m_next_boss(r):
    """1 when the first boss blind of the horizon was cleared (a rollout that died before it: 0)."""
    return 1.0 if any(b["type"] == "Boss" and b["cleared"] for b in r) else 0.0


def m_blinds(r):
    return float(sum(b["cleared"] for b in r))


def paired(runs_a, runs_b, fn):
    """Common-random-numbers mean difference fn(a) - fn(b) and its paired standard error."""
    d = [fn(a) - fn(b) for a, b in zip(runs_a, runs_b)]
    if len(d) < 2:
        return 0.0, 0.0
    return statistics.mean(d), statistics.stdev(d) / math.sqrt(len(d))


def rollouts(base_gs, opt, joker_dicts, n, bosses, tag="RO", reseed=True):
    """n frozen-build rollouts of one option from an engine state (deep-copied, reseeded per k)."""
    pol = Policy(joker_dicts)
    runs = []
    for k in range(n):
        gs = copy.deepcopy(base_gs)
        if reseed:
            gs["rng"] = PseudoRandom(_seed(k, tag))
        apply_option(gs, opt)
        runs.append(play_horizon(gs, pol, bosses))
    return runs


def forecast(pub, n=40, bosses=2, keys=None, budget_s=None):
    """Per-option rollout forecast from a public shop gamestate.

    keys: restrict to these option keys. budget_s: stop adding rollouts when the time budget is hit
    (all options always get the same number, for common random numbers)."""
    import jackdaw.engine.card as card_mod
    counter = card_mod._sort_id_counter  # restored after: rollouts must not shift a live sim game's card ids
    try:
        return _forecast(pub, n, bosses, keys, budget_s)
    finally:
        card_mod._sort_id_counter = counter


def _forecast(pub, n, bosses, keys, budget_s):
    t0 = time.time()
    base = build_state(pub)
    opts = [o for o in options(pub) if o["supported"] and (keys is None or o["key"] in keys or o["key"] == "skip")]
    res = {}
    runs = {o["key"]: [] for o in opts}
    pols = {o["key"]: Policy(option_jokers(pub, o)) for o in opts}
    k = 0
    while k < n:
        for o in opts:
            if o.get("error"):
                continue
            gs = copy.deepcopy(base)
            gs["rng"] = PseudoRandom(_seed(k))
            try:
                apply_option(gs, o)
                runs[o["key"]].append(play_horizon(gs, pols[o["key"]], bosses))
            except IllegalActionError as e:
                o["error"] = str(e)
        k += 1
        if budget_s and time.time() - t0 > budget_s and k >= 8:
            break
    for o in opts:
        if runs[o["key"]] and not o.get("error"):
            res[o["key"]] = {"desc": o["desc"], "cost": o["cost"], **summarize(runs[o["key"]], bosses)}
    if "skip" in res:
        base = runs["skip"]
        for key, row in res.items():
            r = runs[key]
            row["vs_skip"] = {name: [round(x, 3) for x in paired(r, base, fn)] for name, fn in (
                ("next_boss", m_next_boss), ("survive", lambda x: m_survive(x, bosses)), ("blinds", m_blinds))}
    skipped = [o["key"] for o in options(pub) if not o["supported"]]
    return {"options": res, "unsupported": skipped, "rollouts": k, "bosses": bosses,
            "seconds": round(time.time() - t0, 2), "_runs": runs}


def paired_delta(runs_a, runs_b, bosses):
    """Common-random-numbers difference in P(survive) between two options with its paired SE."""
    def s(r):
        return 1.0 if len(r) and r[-1]["cleared"] and sum(b["type"] == "Boss" for b in r) >= bosses else 0.0
    d = [s(a) - s(b) for a, b in zip(runs_a, runs_b)]
    if len(d) < 2:
        return 0.0, 0.0
    return statistics.mean(d), statistics.stdev(d) / math.sqrt(len(d))
