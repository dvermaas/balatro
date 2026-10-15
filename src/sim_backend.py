"""Headless game backend: Jackdaw (pure-Python Balatro engine) behind BalatroBot's JSON-RPC interface.

`SimBalatro` is a drop-in for `balatro.Balatro` (same `call(method, params)` / `state()`, raises
`balatro.RPCError`), so JevBot runs unchanged against it. Gamestates are serialised in BalatroBot's
exact JSON shape (utils/gamestate.lua), including its quirks: empty Lua tables as `[]`, falsy card
fields omitted, "Base Card" labels, the draw pile in draw order (next draw = last entry), every deck
card `hidden`, and effect text built from the game's localization (sim_text.py).

Engine gaps found by the fidelity audit are fixed here at import time by monkeypatching (the pinned
checkout in third_party/jackdaw stays pristine), see `apply_patches()`.

Jackdaw is found at $JACKDAW_PATH or third_party/jackdaw (git submodule, pinned e66de788).
"""

from __future__ import annotations

import os
import random
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
JACKDAW_PATH = Path(os.environ.get("JACKDAW_PATH") or ROOT / "third_party" / "jackdaw")
JACKDAW_COMMIT = "e66de78855df84755d3af7d9a016129f3449f878"
if str(JACKDAW_PATH) not in sys.path:
    sys.path.insert(0, str(JACKDAW_PATH))

from jackdaw.bridge.backend import RPCError as _JRPCError, SimBackend  # noqa: E402
from jackdaw.engine.actions import GamePhase  # noqa: E402

import sim_text  # noqa: E402
from balatro import RPCError  # noqa: E402

BACKEND_NAME = "jackdaw"
_ERR_NAMES = {-32001: "BAD_REQUEST", -32002: "INVALID_STATE", -32003: "NOT_ALLOWED"}

# ---------------------------------------------------------------------------
# Engine patches (see docs/jackdaw-fidelity.md "Closing the gaps")
# ---------------------------------------------------------------------------

_PATCHED = False


def apply_patches():
    global _PATCHED
    if _PATCHED:
        return
    _PATCHED = True
    import sim_patches
    sim_patches.apply()


# ---------------------------------------------------------------------------
# Serialisation (BalatroBot utils/gamestate.lua)
# ---------------------------------------------------------------------------

_SUIT = {"Hearts": "H", "Diamonds": "D", "Clubs": "C", "Spades": "S"}
_RANK = {"2": "2", "3": "3", "4": "4", "5": "5", "6": "6", "7": "7", "8": "8", "9": "9", "10": "T",
         "Jack": "J", "Queen": "Q", "King": "K", "Ace": "A"}
_SET = {"Joker": "JOKER", "Tarot": "TAROT", "Planet": "PLANET", "Spectral": "SPECTRAL", "Voucher": "VOUCHER",
        "Booster": "BOOSTER", "Edition": "EDITION"}
_ENH = {"m_bonus": "BONUS", "m_mult": "MULT", "m_wild": "WILD", "m_glass": "GLASS", "m_steel": "STEEL",
        "m_stone": "STONE", "m_gold": "GOLD", "m_lucky": "LUCKY"}
_EDITION = ("foil", "holo", "polychrome", "negative")
_STATE = {GamePhase.BLIND_SELECT: "BLIND_SELECT", GamePhase.SELECTING_HAND: "SELECTING_HAND",
          GamePhase.ROUND_EVAL: "ROUND_EVAL", GamePhase.SHOP: "SHOP",
          GamePhase.PACK_OPENING: "SMODS_BOOSTER_OPENED", GamePhase.GAME_OVER: "GAME_OVER"}
_DECKS = {"b_red": "RED", "b_blue": "BLUE", "b_yellow": "YELLOW", "b_green": "GREEN", "b_black": "BLACK",
          "b_magic": "MAGIC", "b_nebula": "NEBULA", "b_ghost": "GHOST", "b_abandoned": "ABANDONED",
          "b_checkered": "CHECKERED", "b_zodiac": "ZODIAC", "b_painted": "PAINTED", "b_anaglyph": "ANAGLYPH",
          "b_plasma": "PLASMA", "b_erratic": "ERRATIC"}
_STAKES = {1: "WHITE", 2: "RED", 3: "GREEN", 4: "BLACK", 5: "BLUE", 6: "PURPLE", 7: "ORANGE", 8: "GOLD"}


def _lua(d):
    """BalatroBot (Lua) serialises an empty table as []."""
    return d if d else []


def serialize_card(card, gs, area):
    ab = card.ability or {}
    playing = card.base is not None
    st = ab.get("set", "")
    if st in _SET:
        set_ = _SET[st]
    elif card.center_key not in ("c_base", None) and playing:
        set_ = "ENHANCED"
    else:
        set_ = "DEFAULT"
    value = {}
    if playing:
        value["suit"] = _SUIT.get(card.base.suit.value)
        value["rank"] = _RANK.get(card.base.rank.value)
    value["effect"] = sim_text.card_effect(card, gs, area)
    mod = {}
    if card.seal:
        mod["seal"] = str(card.seal).upper()
    if card.edition:
        ed = next((e for e in _EDITION if card.edition.get(e)), None)
        if ed:
            mod["edition"] = ed.upper()
    # BalatroBot: any card whose ability.effect isn't "Base" gets modifier.enhancement =
    # upper(effect minus " Card"), so jokers carry their effect class too ("TYPE MULT", ...).
    eff = None if playing else sim_text.center(card.center_key).get("effect")
    if playing and card.center_key in _ENH:
        mod["enhancement"] = _ENH[card.center_key]
    elif eff is not None and eff != "Base":
        mod["enhancement"] = eff.replace(" Card", "").upper()  # "" stays "" (e.g. Gros Michel)
    if card.eternal or ab.get("eternal"):
        mod["eternal"] = True
    if (card.perishable or ab.get("perishable")) and (card.perish_tally or 0) > 0:
        mod["perishable"] = card.perish_tally
    if card.rental or ab.get("rental"):
        mod["rental"] = True
    state = {}
    if card.debuff:
        state["debuff"] = True
    if card.facing == "back" or area == "cards":
        state["hidden"] = True
    return {
        "id": card.sort_id,
        "key": card.card_key if playing else card.center_key,
        "set": set_,
        "label": sim_text.card_label(card),
        "value": value,
        "modifier": _lua(mod),
        "state": _lua(state),
        "cost": {"sell": card.sell_cost or 0, "buy": card.cost or 0},
    }


def _area(cards, limit, gs, name, highlighted_limit=1, count=None):
    return {"count": len(cards) if count is None else count, "limit": limit,
            "cards": [serialize_card(c, gs, name) for c in cards], "highlighted_limit": highlighted_limit}


def _hands(gs):
    from jackdaw.engine.data.hands import HAND_BASE, HandType
    hl = gs.get("hand_levels")
    ex = sim_text.game_hands()
    out = {}
    for ht in HandType:
        s = hl.get_state(ht)
        out[ht.value] = {
            "order": HAND_BASE[ht].order, "level": s.level, "chips": s.chips, "mult": s.mult,
            "played": s.played, "played_this_round": s.played_this_round,
            "example": (ex.get(ht.value) or {}).get("example") or [],
        }
    return out


def _blinds(gs):
    from jackdaw.engine.blind import Blind
    from jackdaw.engine.data.prototypes import BLINDS
    rr = gs.get("round_resets", {})
    ante = rr.get("ante", 1)
    scaling = gs.get("modifiers", {}).get("scaling", 1)
    ante_scaling = gs.get("starting_params", {}).get("ante_scaling", 1)
    out = {}
    for typ, default_mult in (("Small", 1), ("Big", 1.5), ("Boss", 2)):
        key = rr.get("blind_choices", {}).get(typ) or ("bl_small" if typ == "Small" else "bl_big" if typ == "Big" else None)
        proto = BLINDS.get(key) if key else None
        b = {"type": typ.upper(), "status": "UPCOMING", "name": "", "effect": "", "score": 0,
             "tag_name": "", "tag_effect": ""}
        if proto:
            b["name"] = proto.name
            b["score"] = Blind.create(key, ante, scaling, ante_scaling).chips if True else 0
            b["effect"] = sim_text.blind_effect(key)
            status = rr.get("blind_states", {}).get(typ)
            if status:
                b["status"] = {"Defeated": "DEFEATED", "Skipped": "SKIPPED", "Current": "CURRENT",
                               "Select": "SELECT"}.get(status, "UPCOMING")
            tag = rr.get("blind_tags", {}).get(typ) if typ != "Boss" else None
            if tag:
                from jackdaw.engine.data.prototypes import TAGS
                b["tag_name"] = getattr(TAGS.get(tag), "name", "")
                b["tag_effect"] = sim_text.tag_effect(tag, gs)
        elif typ == "Boss":
            b["name"] = "Boss Blind"
        out[typ.lower()] = b
    return out


def serialize(gs):
    """Engine game_state -> BalatroBot gamestate JSON."""
    phase = gs.get("phase")
    cr = gs.get("current_round", {})
    deck = gs.get("deck", [])
    in_shop = phase == GamePhase.SHOP or (phase == GamePhase.PACK_OPENING and gs.get("shop_return_phase") == GamePhase.SHOP)
    out = {
        "state": _STATE.get(phase, str(phase)),
        "round_num": gs.get("round", 0),
        "ante_num": gs.get("round_resets", {}).get("ante", 1),
        "money": gs.get("dollars", 0),
        "won": bool(gs.get("won", False)),
        "deck": _DECKS.get(gs.get("selected_back_key", ""), None),
        "stake": _STAKES.get(gs.get("stake", 1), "WHITE"),
        "seed": gs["rng"].seed_str if "rng" in gs else gs.get("seed", ""),
        "used_vouchers": _lua({k: "" for k in gs.get("used_vouchers", {})}),
        "hands": _hands(gs),
        "round": {"hands_left": cr.get("hands_left", 0), "hands_played": cr.get("hands_played", 0),
                  "discards_left": cr.get("discards_left", 0), "discards_used": cr.get("discards_used", 0),
                  "reroll_cost": cr.get("reroll_cost", 5), "chips": gs.get("chips", 0)},
        "blinds": _blinds(gs),
        "jokers": _area(gs.get("jokers", []), gs.get("joker_slots", 5), gs, "jokers"),
        "consumables": _area(gs.get("consumables", []), gs.get("consumable_slots", 2), gs, "consumables"),
        "cards": _area(deck, gs.get("deck_limit", 52), gs, "cards", 5),
        "hand": _area(gs.get("hand", []), gs.get("hand_size", 8), gs, "hand", 5),
    }
    if in_shop:
        shop = gs.get("shop", {})
        out["shop"] = _area(gs.get("shop_cards", []), shop.get("joker_max", 2), gs, "shop")
        out["vouchers"] = _area(gs.get("shop_vouchers", []), max(1, len(gs.get("shop_vouchers", []))), gs, "vouchers")
        out["packs"] = _area(gs.get("shop_boosters", []), 2, gs, "packs")
        # after the first shop the game keeps these areas around, emptied but with stale counts
        gs["_bb_shop_frozen"] = {k: (out[k]["count"], out[k]["limit"]) for k in ("shop", "vouchers", "packs")}
    elif gs.get("_bb_shop_frozen"):
        for k, (count, limit) in gs["_bb_shop_frozen"].items():
            out[k] = {"count": count, "limit": limit, "cards": [], "highlighted_limit": 1}
    if phase == GamePhase.PACK_OPENING:
        pc = gs.get("pack_cards", [])
        out["pack"] = _area(pc, gs.get("pack_size", len(pc)), gs, "pack")
    return out


# ---------------------------------------------------------------------------
# Backend
# ---------------------------------------------------------------------------

_SEED_CHARS = "123456789ABCDEFGHIJKLMNPQRSTUVWXYZ"


def random_seed(rng=random):
    """A seed in the game's format (8 chars, no 0/O)."""
    return "".join(rng.choice(_SEED_CHARS) for _ in range(8))


class SimBalatro:
    """Drop-in for balatro.Balatro backed by Jackdaw."""

    backend = BACKEND_NAME
    url = "jackdaw://local"

    def __init__(self, timeout=None):
        apply_patches()
        sim_text.loc()  # fail fast if the localization is missing
        self.sim = SimBackend()
        self.timeout = timeout
        self.calls = 0
        self.engine_errors = []  # unexpected engine exceptions (reported as INTERNAL_ERROR)

    @property
    def gs(self):
        return self.sim._gs

    def state(self):
        return self.call("gamestate")

    def call(self, method, params=None):
        self.calls += 1
        params = dict(params or {})
        try:
            if method == "health":
                return {"status": "ok"}
            if method == "gamestate":
                return self._state()
            if method == "menu":
                self.sim._gs = None
                return self._state()
            if method == "start":
                params.setdefault("seed", random_seed())
                self.sim.handle("start", params)
                self.gs["deck_limit"] = len(self.gs.get("deck", []))
                return self._state()
            if self.gs is None:
                raise RPCError(-32002, f"Method '{method}' requires an active run", "INVALID_STATE")
            if method == "rearrange":
                return self._rearrange(params)
            self.sim.handle(method, params)
            return self._state()
        except _JRPCError as e:
            raise RPCError(e.code, e.message, _ERR_NAMES.get(e.code)) from e
        except RPCError:
            raise
        except Exception as e:  # noqa: BLE001  engine bug: surface as a game error, keep the details
            self.engine_errors.append(f"{method} {params}: {type(e).__name__}: {e}")
            raise RPCError(-32000, f"{type(e).__name__}: {e}", "INTERNAL_ERROR") from e

    def _state(self):
        if self.gs is None:
            return {"state": "MENU", "round_num": 0, "ante_num": 0, "money": 0}
        return serialize(self.gs)

    def _rearrange(self, params):
        """BalatroBot endpoints/rearrange.lua: a full permutation (new[i] = old[p[i]]) of one area."""
        n = sum(k in params for k in ("hand", "jokers", "consumables"))
        if n != 1:
            raise RPCError(-32001, "Must provide exactly one of: hand, jokers, or consumables" if n == 0
                           else "Can only rearrange one type at a time", "BAD_REQUEST")
        phase = self.gs.get("phase")
        if phase not in (GamePhase.SELECTING_HAND, GamePhase.SHOP, GamePhase.PACK_OPENING):
            raise RPCError(-32002, f"Method 'rearrange' not allowed in state {_STATE.get(phase, phase)}",
                           "INVALID_STATE")
        k = next(k for k in ("hand", "jokers", "consumables") if k in params)
        if k == "hand":
            if phase not in (GamePhase.SELECTING_HAND, GamePhase.PACK_OPENING):
                raise RPCError(-32002, "Can only rearrange hand during hand selection", "INVALID_STATE")
            if phase == GamePhase.PACK_OPENING and not self.gs.get("hand"):
                raise RPCError(-32003, "No cards to rearrange. You can only rearrange hand in Arcana and "
                               "Spectral packs.", "NOT_ALLOWED")
        cards = self.gs.get(k, [])
        perm = list(params[k])
        if len(perm) != len(cards):
            raise RPCError(-32001, f"Must provide exactly {len(cards)} indices for {k}", "BAD_REQUEST")
        seen = set()
        for i in perm:
            if not isinstance(i, int) or i < 0 or i >= len(cards):
                raise RPCError(-32001, f"Index out of range for {k}: {i}", "BAD_REQUEST")
            if i in seen:
                raise RPCError(-32001, f"Duplicate index in {k}: {i}", "BAD_REQUEST")
            seen.add(i)
        self.gs[k] = [cards[i] for i in perm]
        return self._state()
