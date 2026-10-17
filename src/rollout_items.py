"""Item forecast: booster packs, shop tarots and shop spectrals (v21; part of the prompt version hash).

rollout.forecast values jokers, planets, vouchers and playing cards; it lists packs, tarots and
spectrals as unsupported. This module forecasts those from the same PUBLIC shop gamestate, with the
same frozen-build rollouts (rollout.build_state, rollout.Policy, common random numbers), plus:

- Packs: the pack is opened in the engine with the rollout's fresh RNG (seed RO{k}), so its contents
  are sampled from the engine's pools and never come from the real seed's future. The cards are
  picked by `Picker`, a cheap rule that the lab's arm F also uses for real pack openings:
  jokers by measured gain on synthetic hands dealt from the deck multiset, planets by the gain of
  their level-up, tarots and spectrals only when the bot will use them (tarots.TARGETED with a
  plan for the dealt hand, or tarots.SAFE_UNTARGETED), with a fixed value table; Standard-pack
  cards when they carry an enhancement, edition or seal that fits the build. decisions.booster uses the
  same values for real pack openings.
- Held consumables are used as the bot uses them (tarots.auto_use): untargeted safe cards at once,
  targeted cards on a dealt hand with tarots.plan's targets (enhancements on the cards the build
  values most, Steel/Gold on low cards, Death/Strength/Hanged Man deck shaping, suit changes
  toward the flush suit).

Values are on one scale, "fractional score gain per hand", with $1 = 0.01.
"""
from __future__ import annotations

import random
import re
import time

import rollout
import sim_bridge  # noqa: E402  (v25: item rollouts finish on rumbot-sim when jev_sim is built)
import sim_backend
from jackdaw.engine.actions import (BuyCard, CashOut, Discard, GamePhase, NextRound, OpenBooster,  # noqa: E402
                                    PickPackCard, PlayHand, SelectBlind, SkipPack, UseConsumable)
from jackdaw.engine.game import IllegalActionError, step  # noqa: E402
from jackdaw.engine.rng import PseudoRandom  # noqa: E402

import build  # noqa: E402  (read-only use of the bot's policy modules)
import jokers as text_jokers  # noqa: E402
import tarots  # noqa: E402
from hands import best_score, card_info, classify  # noqa: E402

MONEY = 0.01  # value of $1 on the per-hand gain scale
SYNTH_HANDS = 12
PLANET_DEALS = 24  # v28: deals for a planet's value (hand size + PLANET_EXTRA cards each)
PLANET_EXTRA = 3   # roughly what a discard adds, as in readiness.play_mix
# Flat values (per-hand gain scale) of consumables the bot uses; targeted ones only count with a plan.
FLAT = {
    "c_high_priestess": 0.08, "c_emperor": 0.04, "c_fool": 0.02, "c_judgement": 0.10, "c_wheel_of_fortune": 0.03,
    "c_black_hole": 0.30,
    "c_lovers": 0.02, "c_tower": 0.0, "c_devil": 0.03, "c_hanged_man": 0.02, "c_strength": 0.015, "c_death": 0.03,
    "c_star": 0.03, "c_moon": 0.03, "c_sun": 0.03, "c_world": 0.03,
    "c_aura": 0.08, "c_deja_vu": 0.03, "c_talisman": 0.03, "c_medium": 0.01, "c_trance": 0.03, "c_cryptid": 0.04,
}
EDITION_V = {"POLYCHROME": 0.04, "HOLO": 0.02, "FOIL": 0.01}
ENH_V = {"GLASS": 0.03, "STEEL": 0.02, "LUCKY": 0.015, "MULT": 0.015, "BONUS": 0.01, "GOLD": 0.01, "WILD": 0.01}
SEAL_V = {"RED": 0.02, "GOLD": 0.015, "BLUE": 0.01, "PURPLE": 0.005}


def _d(x):
    return x if isinstance(x, dict) else {}


def _cards(pub, area):
    return _d(pub.get(area)).get("cards") or []


def usable(key):
    """Consumables the bot ever uses (tarots.auto_use); anything else would sit in a slot."""
    return key in tarots.TARGETED or key in tarots.SAFE_UNTARGETED


# ---------------------------------------------------------------------------
# Pack picks
# ---------------------------------------------------------------------------

def _deck_infos(pub):
    """The deck multiset (cards + hand), canonically sorted so the real draw order is never read."""
    deck = list(_cards(pub, "cards")) + list(_cards(pub, "hand"))
    deck.sort(key=lambda c: (str(_d(c.get("value")).get("suit")), str(_d(c.get("value")).get("rank")),
                             str(_d(c.get("modifier")))))
    return [dict(card_info(c), debuff=False) for c in deck]


def planet_gain(pub, lv, k=PLANET_DEALS, extra=PLANET_EXTRA, seed="PLANET"):
    """v28: mean best-play score ratio - 1 after levelling lv=(hand, chips, mult), with the best play
    re-chosen in each deal (Rust kernel), over k deals of hand size + `extra` cards from the deck multiset.
    jokers.gain keeps each sample's play fixed, so a level for a hand we don't play yet was worth 0 and
    the picks spread over Pair, High Card and ties."""
    infos = _deck_infos(pub)
    size = min((_d(pub.get("hand")).get("limit") or 8) + extra, len(infos))
    if size < 1:
        return 0.0
    js, levels = text_jokers.build(pub), _d(pub.get("hands"))
    h, c, m = lv
    new = {**levels, h: {**_d(levels.get(h)), "chips": _d(levels.get(h)).get("chips", 0) + c,
                         "mult": _d(levels.get(h)).get("mult", 1) + m}}
    rnd = {"hands_left": 2, "discards_left": 1}
    s0, s1 = text_jokers.Scorer(levels, js, rnd), text_jokers.Scorer(new, js, rnd)
    rng = random.Random(seed)
    before = after = 0.0
    for _ in range(k):
        hand = rng.sample(infos, size)
        before += best_score(hand, s0)
        after += best_score(hand, s1)
    return after / before - 1 if before else 0.0


def synth_plays(pub, k=SYNTH_HANDS, seed="SYNTH"):
    """Recent-play stand-ins: k hands dealt from the deck MULTISET (cards + hand, canonically sorted,
    so the real draw order is never read) with a fixed RNG, each with its best text-scored play."""
    deck = list(_cards(pub, "cards")) + list(_cards(pub, "hand"))
    deck.sort(key=lambda c: (str(_d(c.get("value")).get("suit")), str(_d(c.get("value")).get("rank")),
                             str(_d(c.get("modifier")))))
    infos = [dict(card_info(c), debuff=False) for c in deck]
    if not infos:
        return []
    rng = random.Random(seed)
    size = min(_d(pub.get("hand")).get("limit") or 8, len(infos))
    js, levels = text_jokers.build(pub), _d(pub.get("hands"))
    rnd = {"hands_left": 3, "discards_left": 2}
    out = []
    for _ in range(k):
        h = rng.sample(infos, size)
        best = None
        for combo in rollout.candidates(h):
            combo = sorted(set(combo))
            name, sc = classify([h[p] for p in combo])
            s = text_jokers.score(h, combo, name, sc, levels, js, rnd)
            if best is None or s > best[0]:
                best = (s, combo)
        out.append({"infos": h, "combo": best[1], "rnd": rnd})
    return out


class Picker:
    """Which card to take from an open pack (and the tarot targets), from public information.
    One instance per public state; values that do not depend on the dealt hand are memoised."""

    def __init__(self, pub):
        self.pub = pub
        self.b = build.decide(pub)
        self.synth = synth_plays(pub)
        self.memo = {}

    def _gain(self, **kw):
        try:
            return text_jokers.gain(self.pub, self.synth, **kw)
        except Exception:  # noqa: BLE001  unparseable text: no measured gain
            return None

    def joker_value(self, c):
        m = _d(c.get("modifier"))
        if m.get("rental") or m.get("eternal") or re.search(r"destroys? (a random )?Joker", _d(c.get("value")).get(
                "effect") or "", re.I):
            return 0.0  # rental upkeep, a slot we can never free, or a joker that eats jokers: not a free pick
        k = ("j", c.get("key"), str(m.get("edition")))
        if k not in self.memo:
            g = self._gain(add=c)
            # any joker in a free slot is worth a little (sellable, often scores in ways text can't see)
            self.memo[k] = (0.05 if g is None else max(g, 0.0)) + 0.01
        return self.memo[k]

    def planet_value(self, c):
        k = ("p", c.get("key"))
        if k not in self.memo:
            lv = text_jokers.planet_level(_d(c.get("value")).get("effect"))
            try:
                g = planet_gain(self.pub, lv) if lv else None
            except Exception:  # noqa: BLE001  unparseable state: no measured gain
                g = None
            self.memo[k] = max(g or 0.0, 0.0) + 0.001  # a paid-for planet is always taken
        return self.memo[k]

    def consumable_value(self, c, hand, money, jokers, joker_room):
        """(value, targets) for a tarot/spectral the bot would use right now; (0, None) otherwise."""
        key = c.get("key") or ""
        if key in tarots.TARGETED:
            p = tarots.plan(key, hand, self.b) if hand else None
            if not p:
                return 0.0, None
            kind, n, arg = tarots.TARGETED[key]
            v = FLAT.get(key, 0.02)
            if kind == "enhance" and arg in tarots.MEASURABLE:
                mk = ("m", key)
                if mk not in self.memo:
                    try:
                        m = tarots.measured(c, self.pub, self.synth, self.b)
                    except Exception:  # noqa: BLE001
                        m = None
                    self.memo[mk] = max(m[1], 0.0) if m else 0.02
                v = self.memo[mk]
            return max(v, 0.001), p[0]
        if key in tarots.SAFE_UNTARGETED:
            if key == "c_hermit":
                return max(0.001, MONEY * max(0, min(money, 20))), None
            if key == "c_temperance":
                return max(0.001, MONEY * min(sum(_d(j.get("cost")).get("sell", 0) for j in jokers), 50)), None
            if key == "c_judgement" and joker_room < 1:
                return 0.0, None
            if key == "c_wheel_of_fortune" and not jokers:
                return 0.0, None
            return FLAT.get(key, 0.01), None
        return 0.0, None

    def card_value(self, c):
        i = card_info(c)
        v = EDITION_V.get(i["edition"], 0) + ENH_V.get(i["enh"], 0) + SEAL_V.get(i["seal"], 0)
        if i["enh"] == "WILD" and not self.b.get("suit"):
            v -= ENH_V["WILD"]
        if self.b.get("suit") and i["suit"] != self.b["suit"] and i["enh"] != "WILD":
            v *= 0.3
        return v

    def values(self, pack, hand, money, jokers, joker_limit):
        """[(value, index, targets)] for every card we could take (jokers only with a free slot)."""
        room = joker_limit - len(jokers)
        out = []
        for i, c in enumerate(pack):
            st, targets = c.get("set"), None
            if st == "JOKER":
                if room < 1 and _d(c.get("modifier")).get("edition") != "NEGATIVE":
                    continue
                v = self.joker_value(c)
            elif st == "PLANET":
                v = self.planet_value(c)
            elif st in ("TAROT", "SPECTRAL"):
                v, targets = self.consumable_value(c, hand, money, jokers, room)
                if c.get("key") == "c_black_hole":
                    v = FLAT["c_black_hole"]
            else:
                v = self.card_value(c)
            out.append((v, i, targets))
        return out

    def pick(self, pack, hand, money, jokers, joker_limit):
        """(index, targets) of the card to take, or None to skip. Mirrors decisions.booster's options:
        jokers only with a free slot; targeted tarots only with a plan for this hand."""
        best = None
        for v, i, targets in self.values(pack, hand, money, jokers, joker_limit):
            if v > 0 and (best is None or v > best[0]):
                best = (v, i, targets)
        return (best[1], best[2]) if best else None

    def pick_pub(self, pub):
        """pick() on a public SMODS_BOOSTER_OPENED gamestate."""
        jl = _d(pub.get("jokers"))
        return self.pick(_cards(pub, "pack"), _cards(pub, "hand"), pub.get("money") or 0, _cards(pub, "jokers"),
                         jl.get("limit") or 5)


def _area_cards(gs, name):
    return sim_backend._area(gs.get(name, []), 0, gs, name)["cards"]


def open_and_pick(gs, picker, max_picks=6):
    """Pick from the engine's open pack with `picker` until it closes. Returns the cards taken (keys)."""
    taken = []
    for _ in range(max_picks):
        if gs["phase"] != GamePhase.PACK_OPENING:
            break
        pack = _area_cards(gs, "pack_cards")
        choice = picker.pick(pack, _area_cards(gs, "hand"), gs.get("dollars", 0), _area_cards(gs, "jokers"),
                             gs.get("joker_slots", 5))
        if choice is None:
            step(gs, SkipPack())
            break
        idx, targets = choice
        try:
            step(gs, PickPackCard(card_index=idx, target_indices=tuple(targets) if targets else None))
            taken.append(pack[idx].get("key"))
        except IllegalActionError:
            step(gs, SkipPack())
            break
    return taken


# ---------------------------------------------------------------------------
# Held consumables, as tarots.auto_use / strategy.auto_action use them
# ---------------------------------------------------------------------------

def use_items(gs, b, state, failed):
    """Use held consumables the way the bot does: planets, safe untargeted cards (any state), targeted
    cards on the dealt hand with tarots.plan targets (SELECTING_HAND only). failed: keys that raised."""
    for _ in range(8):
        cons = gs.get("consumables", [])
        if not cons:
            return
        act = None
        jokers = gs.get("jokers", [])
        free = gs.get("joker_slots", 5) - len(jokers)
        hand_pub = None
        for i, c in enumerate(cons):
            key = c.center_key
            if c.ability.get("set") == "Planet":
                act = (i, None)
                break
            if (key, state) in failed:
                continue
            if key in tarots.SAFE_UNTARGETED:
                if key == "c_hermit" and gs.get("dollars", 0) < 4:
                    continue
                if key == "c_judgement" and free < 1:
                    continue
                if key == "c_wheel_of_fortune" and not jokers:
                    continue
                act = (i, None)
                break
            if key in tarots.TARGETED and state == "SELECTING_HAND":
                if hand_pub is None:
                    hand_pub = _area_cards(gs, "hand")
                p = tarots.plan(key, hand_pub, b)
                if p:
                    act = (i, tuple(p[0]))
                    break
        if act is None:
            return
        try:
            step(gs, UseConsumable(card_index=act[0], target_indices=act[1]))
        except IllegalActionError:
            failed.add((cons[act[0]].center_key, state))


# ---------------------------------------------------------------------------
# Rollouts
# ---------------------------------------------------------------------------

def _jsig(gs):
    return tuple(j.center_key for j in gs.get("jokers", []))


_ITEMS = {}
# use_items' conditions on the safe untargeted cards, for the Rust copy (jev_sim use_items)
_SAFE_COND = {"c_hermit": "money4", "c_judgement": "joker_room", "c_wheel_of_fortune": "jokers"}


def _items_desc(b):
    suit = b.get("suit")
    if suit not in _ITEMS:
        _ITEMS[suit] = sim_bridge.items_desc(tarots.TARGETED, {k: _SAFE_COND.get(k) for k in tarots.SAFE_UNTARGETED},
                                             suit)
    return _ITEMS[suit]


def _refresh(gs, pol, held, policy_for):
    """The rollout policy after the jokers may have changed. v27: when jokers were only removed (Madness,
    Ceremonial Dagger, Popcorn or Gros Michel running out), the policy keeps the other jokers' entries
    as they were; otherwise it is rebuilt from the jokers' current text (policy_for). Rust does the
    same (jev_sim refresh_policy), so these rollouts no longer hand back to Python."""
    cur = gs.get("jokers", [])
    if len(cur) == len(held) and all(a is b for a, b in zip(cur, held)):
        return pol, held
    keep = [next((i for i, h in enumerate(held) if h is c), None) for c in cur]
    if None in keep or any(j["label"] in ("Blueprint", "Brainstorm") for j in pol.js):
        return policy_for(gs), list(cur)
    return rollout.Policy.of_entries([pol.js[i] for i in keep]), list(cur)


def play_horizon(gs, policy_for, b, bosses=3, max_steps=900):
    """rollout.play_horizon plus the bot's consumable use and policy rebuilds when jokers change.

    v25: with jev_sim the rollout runs in Rust from its first shop (sim_bridge.run_strict: RNG streams
    carried over, consumables used like use_items). Rust gives up when the jokers change, since Python
    rebuilds its policy then; this loop carries on from the unchanged state. Same rows either way."""
    out, cur, done = [], None, 0
    failed = set()
    pol = policy_for(gs)
    held = list(gs.get("jokers", []))  # the jokers pol was made for, in order
    native = sim_bridge.available()
    for _ in range(max_steps):
        ph = gs["phase"]
        if ph == GamePhase.SHOP and native:
            rest = sim_bridge.run_strict(gs, policy_for(gs), bosses - done, _items_desc(b))
            if rest is not None:
                return out + rest
            native = False  # it would give up again at the next shop for the same reason, mostly
        if ph == GamePhase.SHOP:
            use_items(gs, b, "SHOP", failed)
            step(gs, NextRound())
        elif ph == GamePhase.BLIND_SELECT:
            if gs.get("consumables"):
                use_items(gs, b, "BLIND_SELECT", failed)
            typ = gs.get("blind_on_deck", "Small")
            step(gs, SelectBlind())
            bl = gs.get("blind")
            cur = {"ante": gs["round_resets"]["ante"], "type": typ, "boss": getattr(bl, "name", typ),
                   "target": getattr(bl, "chips", 0)}
            failed = {f for f in failed if f[1] != "SELECTING_HAND"}  # the bot retries each round
            pol, held = _refresh(gs, pol, held, policy_for)
        elif ph == GamePhase.SELECTING_HAND:
            if not gs["hand"]:
                out.append({**cur, "chips": gs.get("chips", 0), "cleared": False})
                return out
            if gs.get("consumables"):
                use_items(gs, b, "SELECTING_HAND", failed)
                pol, held = _refresh(gs, pol, held, policy_for)
            kind, idx = pol.decide(gs)
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


def options(pub):
    """skip, plus each affordable pack and each affordable shop tarot/spectral the bot would use."""
    money = pub.get("money") or 0
    cons = _d(pub.get("consumables"))
    free_c = len(_cards(pub, "consumables")) < (cons.get("limit") or 2)
    out = [{"key": "skip", "desc": f"buy nothing (keep ${money})", "actions": [], "cost": 0}]
    for i, c in enumerate(_cards(pub, "shop")):
        cost = _d(c.get("cost")).get("buy", 0)
        neg = _d(c.get("modifier")).get("edition") == "NEGATIVE"
        if c.get("set") in ("TAROT", "SPECTRAL") and cost <= money and (free_c or neg) and usable(c.get("key")):
            out.append({"key": f"buy_card_{i}", "desc": f"${cost} {c.get('label')}", "actions": [("buy", i)],
                        "cost": cost, "set": c.get("set")})
    for i, c in enumerate(_cards(pub, "packs")):
        cost = _d(c.get("cost")).get("buy", 0)
        if cost <= money:
            out.append({"key": f"buy_pack_{i}", "desc": f"${cost} {c.get('label')}", "actions": [("pack", i)],
                        "cost": cost, "set": "BOOSTER"})
    return out


def build_state(pub):
    """rollout.build_state plus the shop's booster packs."""
    gs = rollout.build_state(pub)
    ctx = {"hands_played": 0}
    gs["shop_boosters"] = [rollout.item_card(c, ctx) for c in _cards(pub, "packs")]
    return gs


def apply_option(gs, opt, picker):
    for kind, i in opt["actions"]:
        if kind == "pack":
            step(gs, OpenBooster(card_index=i))
            open_and_pick(gs, picker)
        else:
            step(gs, BuyCard(shop_index=i))


def forecast(pub, n=16, bosses=3, keys=None, budget_s=None):
    """Per-option forecast (same output shape as rollout.forecast) for packs and shop tarots/spectrals."""
    import jackdaw.engine.card as card_mod
    counter = card_mod._sort_id_counter
    try:
        return _forecast(pub, n, bosses, keys, budget_s)
    finally:
        card_mod._sort_id_counter = counter


def _forecast(pub, n, bosses, keys, budget_s):
    t0 = time.time()
    base = build_state(pub)
    snap = rollout.Snapshot(base)
    picker = Picker(pub)
    b = picker.b
    opts = [o for o in options(pub) if keys is None or o["key"] in keys or o["key"] == "skip"]
    pols = {}

    def policy_for(gs):
        sig = _jsig(gs)
        if sig not in pols:
            pols[sig] = rollout.Policy(_area_cards(gs, "jokers"))
        return pols[sig]

    runs = {o["key"]: [] for o in opts}
    k = 0
    while k < n:
        for o in opts:
            if o.get("error"):
                continue
            gs = snap.copy()
            gs["rng"] = PseudoRandom(rollout._seed(k))
            try:
                apply_option(gs, o, picker)
                runs[o["key"]].append(play_horizon(gs, policy_for, b, bosses))
            except IllegalActionError as e:
                o["error"] = str(e)
        k += 1
        if budget_s and time.time() - t0 > budget_s and k >= 8:
            break
    res = {}
    for o in opts:
        if runs[o["key"]] and not o.get("error"):
            res[o["key"]] = {"desc": o["desc"], "cost": o["cost"], **rollout.summarize(runs[o["key"]], bosses)}
    if "skip" in res:
        sk = runs["skip"]
        for key, row in res.items():
            r = runs[key]
            row["vs_skip"] = {name: [round(x, 3) for x in rollout.paired(r, sk, fn)] for name, fn in (
                ("next_boss", rollout.m_next_boss), ("survive", lambda x: rollout.m_survive(x, bosses)),
                ("blinds", rollout.m_blinds))}
    return {"options": res, "rollouts": k, "bosses": bosses, "seconds": round(time.time() - t0, 2),
            "errors": {o["key"]: o["error"] for o in opts if o.get("error")}}


def best(fc, se=1.0):
    """v31: se 2 -> 1, as decisions.SURE_SE. The option key that beats skip by more than `se` paired SEs in blinds cleared (largest gain), or None."""
    rows = [(k, r["vs_skip"]["blinds"]) for k, r in fc["options"].items() if k != "skip" and "vs_skip" in r]
    rows = [(k, d) for k, (d, s) in rows if d > se * max(s, 1e-9)]
    return max(rows, key=lambda kv: kv[1])[0] if rows else None
