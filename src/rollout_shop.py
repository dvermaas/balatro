"""Shopping rollouts (lab only, NOT part of the prompt version hash).

rollout.forecast and rollout_items.forecast play the horizon with a FROZEN build: shops reached inside a
rollout are not even populated (sim_patches.rollout_skips_shop_population), so money left over, interest,
and jokers that pay off through later purchases are worth nothing to them. This module gives the
rollouts a cheap shop policy at every shop they pass through AFTER the current one:

- Shops are populated by the engine with the rollout's own fresh RNG (seed RO{k}), never the real
  seed's future. The no-shop flag is switched off for these states only.
- `Shopper` buys the best affordable joker by a static value cached per (joker key, edition) for the
  whole forecast call (no nested forecasts): rollout_items.Picker's joker value (jokers.gain on 12
  synthetic hands dealt from the deck multiset, 0 for rental/eternal/joker-destroying jokers), plus a
  bonus for scaling jokers whose current value the text scorer can't see grow.
- Free slot: buy when the value is at least MIN_VALUE. Full slots: sell the weakest owned joker when
  the new one is clearly better (SWAP_RATIO x weakest + SWAP_MARGIN). Owned jokers of the real build
  are valued by the loss from removing them on the same synthetic hands.
- An interest buffer ($25 from ante 3) is kept; then the planet for the most played hand is bought
  from what is left of the surplus. No rerolls, no packs, no vouchers.

forecast() / forecast_items() are drop-in replacements for rollout.forecast / rollout_items.forecast
(same arguments, same output shape plus `shop_stats`), used by the policy lab arm bot-forecast-shop.
"""
from __future__ import annotations

import re
import time
from collections import Counter

import rollout
import rollout_items
import sim_backend
from jackdaw.engine.actions import (BuyCard, CashOut, Discard, GamePhase, NextRound, PlayHand,  # noqa: E402
                                    SelectBlind, SellCard, SkipPack)
from jackdaw.engine.data.hands import HandType  # noqa: E402
from jackdaw.engine.game import IllegalActionError, step  # noqa: E402
from jackdaw.engine.rng import PseudoRandom  # noqa: E402

import jokers as text_jokers  # noqa: E402  (read-only)
import strategy  # noqa: E402  (read-only: joker_kind)

BUFFER = 25            # money kept for interest ...
BUFFER_FROM_ANTE = 3   # ... from this ante on
MIN_VALUE = 0.03       # a joker into a free slot must be worth at least this (per-hand gain scale)
SWAP_RATIO, SWAP_MARGIN = 1.5, 0.05
SCALING_BONUS = 0.10   # scaling jokers start small; the text scorer only sees their current value
MAX_BUYS = 2           # joker buys per shop
PLANETS = True
TOTALS = Counter()     # shop stats summed over forecast calls (reset by Patched; read by the lab)


def _d(x):
    return x if isinstance(x, dict) else {}


def _ed(card):
    return next((e.upper() for e in rollout.EDITIONS if card.edition and card.edition.get(e)), None)


class Shopper:
    """Cheap in-rollout shop policy. One per forecast call: values are cached per joker key for every
    option and rollout of that call (they are measured against the real build's synthetic hands)."""

    def __init__(self, pub, picker=None):
        self.picker = picker or rollout_items.Picker(pub)
        self.memo = {}
        self.planet_memo = {}
        self.owned = {}
        self.stats = Counter()
        for j, c in enumerate(rollout_items._cards(pub, "jokers")):
            g = self.picker._gain(remove=j)
            v = (0.05 if g is None else max(-g, 0.0)) + 0.01
            self.owned[c.get("key")] = max(self.owned.get(c.get("key"), 0.0), v + self._bonus(c))

    @staticmethod
    def _bonus(d):
        eff = _d(d.get("value")).get("effect") or ""
        kind = strategy.joker_kind(eff)
        return SCALING_BONUS if kind == "scaling" or (kind == "xmult" and re.search(r"\bgains\b", eff)) else 0.0

    def value(self, card, gs, area):
        """Static value of an engine joker card (cached per key and edition)."""
        k = (card.center_key, _ed(card))
        if k not in self.memo:
            d = sim_backend.serialize_card(card, gs, area)
            v = self.picker.joker_value(d)
            self.memo[k] = v + self._bonus(d) if v > 0 else 0.0
        return self.memo[k]

    def owned_value(self, card, gs):
        if card.center_key in self.owned:
            return self.owned[card.center_key]
        return self.value(card, gs, "jokers")

    def _planet_hand(self, card, gs):
        if card.center_key not in self.planet_memo:
            lv = text_jokers.planet_level(sim_backend.serialize_card(card, gs, "shop_cards")["value"].get("effect"))
            self.planet_memo[card.center_key] = lv[0] if lv else None
        return self.planet_memo[card.center_key]

    def shop(self, gs):
        try:
            self._shop(gs)
        except IllegalActionError:
            self.stats["illegal"] += 1
        except Exception:  # noqa: BLE001  a shopping bug must not cost the whole forecast
            self.stats["errors"] += 1

    def _shop(self, gs):
        self.stats["shops"] += 1
        ante = gs["round_resets"]["ante"]
        buffer = BUFFER if ante >= BUFFER_FROM_ANTE else 0
        for _ in range(MAX_BUYS):
            money = gs.get("dollars", 0)
            jokers = gs.get("jokers", [])
            free = gs.get("joker_slots", 5) - len(jokers) > 0
            weak = None
            for j, c in enumerate(jokers):
                if c.eternal or c.ability.get("eternal") or _ed(c) == "NEGATIVE":
                    continue
                w = self.owned_value(c, gs)
                if weak is None or w < weak[0]:
                    weak = (w, j, c.sell_cost or 0)
            best = None
            for i, c in enumerate(gs.get("shop_cards", [])):
                if c.ability.get("set") != "Joker":
                    continue
                v = self.value(c, gs, "shop_cards")
                cost = c.cost or 0
                if free or _ed(c) == "NEGATIVE":
                    if v >= MIN_VALUE and cost <= money - buffer and (best is None or v > best[0]):
                        best = (v, i, None)
                elif weak and v > SWAP_RATIO * weak[0] + SWAP_MARGIN and cost <= money + weak[2] - buffer \
                        and (best is None or v > best[0]):
                    best = (v, i, weak[1])
            if best is None:
                break
            _, i, sell = best
            if sell is not None:
                step(gs, SellCard(area="jokers", card_index=sell))
                self.stats["swaps"] += 1
            step(gs, BuyCard(shop_index=i))
            self.stats["joker_buys"] += 1
        if not PLANETS or len(gs.get("consumables", [])) >= gs.get("consumable_slots", 2):
            return
        hl = gs["hand_levels"]
        top = max(HandType, key=lambda ht: (hl.get_state(ht).played, hl.get_state(ht).level))
        if not hl.get_state(top).played:
            return
        for i, c in enumerate(gs.get("shop_cards", [])):
            if c.ability.get("set") == "Planet" and (c.cost or 0) <= gs.get("dollars", 0) - max(buffer, 4) \
                    and self._planet_hand(c, gs) == top.value:
                step(gs, BuyCard(shop_index=i))
                self.stats["planet_buys"] += 1
                break


def play_horizon(gs, policy_for, bosses, shopper, items_b=None, max_steps=900):
    """rollout.play_horizon (items_b None: planets only) or rollout_items.play_horizon (items_b: the
    bot's consumable use), plus `shopper` at every shop reached after a cash-out."""
    out, cur, done = [], None, 0
    failed = set()
    shopping = False  # the current (real) shop's purchase is the option itself
    pol = policy_for(gs)
    for _ in range(max_steps):
        ph = gs["phase"]
        if ph == GamePhase.SHOP:
            if shopping:
                shopper.shop(gs)
            if items_b is None:
                rollout.use_planets(gs)
            else:
                rollout_items.use_items(gs, items_b, "SHOP", failed)
            step(gs, NextRound())
        elif ph == GamePhase.BLIND_SELECT:
            if items_b is not None and gs.get("consumables"):
                rollout_items.use_items(gs, items_b, "BLIND_SELECT", failed)
            typ = gs.get("blind_on_deck", "Small")
            step(gs, SelectBlind())
            bl = gs.get("blind")
            cur = {"ante": gs["round_resets"]["ante"], "type": typ, "boss": getattr(bl, "name", typ),
                   "target": getattr(bl, "chips", 0)}
            failed = {f for f in failed if f[1] != "SELECTING_HAND"}
            pol = policy_for(gs)
        elif ph == GamePhase.SELECTING_HAND:
            if not gs["hand"]:
                out.append({**cur, "chips": gs.get("chips", 0), "cleared": False})
                return out
            if items_b is not None and gs.get("consumables"):
                rollout_items.use_items(gs, items_b, "SELECTING_HAND", failed)
                pol = policy_for(gs)
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
            shopping = True
        elif ph == GamePhase.PACK_OPENING:
            step(gs, SkipPack())
        else:
            raise RuntimeError(f"unexpected phase {ph}")
    raise RuntimeError("rollout did not finish")


def _policies(initial=None):
    """policy_for(gs): rollout.Policy per joker line-up (engine jokers serialised); `initial` maps a
    joker signature to a policy built from the public joker dicts."""
    pols = dict(initial or {})

    def policy_for(gs):
        sig = rollout_items._jsig(gs)
        if sig not in pols:
            pols[sig] = rollout.Policy(rollout_items._area_cards(gs, "jokers"))
        return pols[sig]
    return policy_for


def _run(pub, opts, snap, apply, policy_for, shopper, n, bosses, budget_s, items_b, t0):
    runs = {o["key"]: [] for o in opts}
    k = 0
    while k < n:
        for o in opts:
            if o.get("error"):
                continue
            gs = snap.copy()
            gs["rng"] = PseudoRandom(rollout._seed(k))
            try:
                apply(gs, o)
                runs[o["key"]].append(play_horizon(gs, policy_for(o, gs), bosses, shopper, items_b))
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
            row["vs_skip"] = {name: [round(x, 3) for x in rollout.paired(runs[key], sk, fn)] for name, fn in (
                ("next_boss", rollout.m_next_boss), ("survive", lambda x: rollout.m_survive(x, bosses)),
                ("blinds", rollout.m_blinds))}
    return res, runs, k


def _keep_counter(fn):
    def wrapped(*a, **kw):
        import jackdaw.engine.card as card_mod
        counter = card_mod._sort_id_counter  # rollouts must not shift a live sim game's card ids
        try:
            return fn(*a, **kw)
        finally:
            card_mod._sort_id_counter = counter
    wrapped.__name__, wrapped.__doc__ = fn.__name__, fn.__doc__
    return wrapped


@_keep_counter
def forecast(pub, n=40, bosses=2, keys=None, budget_s=None):
    """rollout.forecast with shopping rollouts (joker, planet, voucher and playing-card options)."""
    t0 = time.time()
    base = rollout.build_state(pub)
    base["_rollout_no_shop"] = False
    snap = rollout.Snapshot(base)
    shopper = Shopper(pub)
    opts = [o for o in rollout.options(pub) if o["supported"] and (keys is None or o["key"] in keys or o["key"] == "skip")]
    init = {o["key"]: rollout.Policy(rollout.option_jokers(pub, o)) for o in opts}
    shared = _policies()

    def policy_for(o, gs):
        first = {"sig": rollout_items._jsig(gs)}

        def pf(g):
            sig = rollout_items._jsig(g)
            return init[o["key"]] if sig == first["sig"] else shared(g)
        return pf

    res, runs, k = _run(pub, opts, snap, rollout.apply_option, policy_for, shopper, n, bosses, budget_s, None, t0)
    skipped = [o["key"] for o in rollout.options(pub) if not o["supported"]]
    TOTALS.update(shopper.stats, joker_calls=1)
    return {"options": res, "unsupported": skipped, "rollouts": k, "bosses": bosses,
            "seconds": round(time.time() - t0, 2), "_runs": runs, "shop_stats": dict(shopper.stats)}


@_keep_counter
def forecast_items(pub, n=16, bosses=3, keys=None, budget_s=None):
    """rollout_items.forecast with shopping rollouts (packs, shop tarots and spectrals)."""
    t0 = time.time()
    base = rollout_items.build_state(pub)
    base["_rollout_no_shop"] = False
    snap = rollout.Snapshot(base)
    picker = rollout_items.Picker(pub)
    shopper = Shopper(pub, picker)
    opts = [o for o in rollout_items.options(pub) if keys is None or o["key"] in keys or o["key"] == "skip"]
    shared = _policies()
    res, runs, k = _run(pub, opts, snap, lambda gs, o: rollout_items.apply_option(gs, o, picker),
                        lambda o, gs: shared, shopper, n, bosses, budget_s, picker.b, t0)
    TOTALS.update(shopper.stats, item_calls=1)
    return {"options": res, "rollouts": k, "bosses": bosses, "seconds": round(time.time() - t0, 2),
            "errors": {o["key"]: o["error"] for o in opts if o.get("error")}, "shop_stats": dict(shopper.stats)}


class Patched:
    """Context manager: rollout.forecast / rollout_items.forecast -> the shopping variants (the lab arm)."""

    def __enter__(self):
        self.saved = (rollout.forecast, rollout_items.forecast)
        TOTALS.clear()
        rollout.forecast, rollout_items.forecast = forecast, forecast_items
        return self

    def __exit__(self, *exc):
        rollout.forecast, rollout_items.forecast = self.saved
        return False
