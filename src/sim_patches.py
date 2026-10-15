"""Runtime fixes for the pinned Jackdaw checkout (applied once by sim_backend.apply_patches()).

Each patch mirrors what the real game + BalatroBot do, as found by the fidelity audit
(docs/jackdaw-fidelity.md) and re-checked by scripts/diagnostics/sim_parity.py.
"""

from __future__ import annotations

from jackdaw.engine import consumables as _cons
from jackdaw.engine import game as _game
from jackdaw.engine.actions import GamePhase

PATCHES = []


def patch(fn):
    PATCHES.append(fn)
    return fn


@patch
def pack_created_cards_to_hand():
    """Familiar / Grim / Incantation / Cryptid used from an Arcana/Spectral pack: Lua emplaces the
    created cards in G.hand (the pack's dealt hand, appended at the end, unsorted); they go back to
    the deck with the rest of that hand when the pack closes. Jackdaw put them in the deck."""
    orig = _game._apply_consumable_result

    def wrapped(gs, result, card=None):
        if gs.get("phase") != GamePhase.PACK_OPENING or not gs.get("pack_hand"):
            return orig(gs, result, card)
        before = {id(c) for c in gs.get("deck", [])}
        orig(gs, result, card)
        deck = gs.get("deck", [])
        new = [c for c in deck if id(c) not in before and getattr(c, "base", None) is not None]
        if new:
            ids = {id(c) for c in new}
            gs["deck"] = [c for c in deck if id(c) not in ids]
            gs["hand"].extend(new)
            gs["pack_hand"].extend(new)
    _game._apply_consumable_result = wrapped


@patch
def balatrobot_pack_legality():
    """BalatroBot's `pack` endpoint calls G.FUNCS.use_card without the game's can_use gate. It only
    rejects: a Joker when joker slots are full (Negative included), Ankh with no Joker, and a
    targeting card whose target count is outside min_highlighted..max_highlighted (Aura: exactly 1).
    So Judgement/Wraith/The Soul from a pack create a Joker even with full slots, and The Fool with
    nothing to copy creates a random Tarot/Planet (both observed in recorded real-game runs)."""
    def block_reason(card, gs, targets=None):
        ab = getattr(card, "ability", None) or {}
        key = getattr(card, "center_key", "") or ""
        if ab.get("set") == "Joker":
            if len(gs.get("jokers", [])) >= gs.get("joker_slots", 5):
                return "Cannot select joker, joker slots are full"
            return None
        if key == "c_ankh" and not gs.get("jokers"):
            return f"Card '{key}' requires at least 1 joker"
        if key == "c_aura":
            lo = hi = 1
        else:
            from jackdaw.engine.card import _resolve_center
            cfg = (_resolve_center(key).get("config") or {}) if key else {}
            if not isinstance(cfg, dict) or not cfg.get("max_highlighted"):
                return None
            lo, hi = cfg.get("min_highlighted", 1), cfg["max_highlighted"]
        n = len(targets or ())
        if not lo <= n <= hi:
            return f"Card '{key}' requires {lo}-{hi} target card(s). Provided: {n}"
        return None
    _cons.pack_pick_block_reason = block_reason

    # Lua use_consumeable creates the Judgement/Soul/Wraith joker unconditionally (the room check
    # lives in can_use_consumeable, which the pack endpoint skips).
    orig = _game._resolve_create_descriptors

    def wrapped(gs, descriptors):
        overflow = gs.get("phase") == GamePhase.PACK_OPENING and any(
            d.get("type") == "Joker" and d.get("seed") in ("jud", "soul", "wra") for d in descriptors)
        if not overflow:
            return orig(gs, descriptors)
        big = 10 ** 6
        slots = gs.get("joker_slots", 5)
        gs["joker_slots"] = big
        try:
            orig(gs, descriptors)
        finally:
            gs["joker_slots"] = slots + (gs.get("joker_slots", big) - big)
    _game._resolve_create_descriptors = wrapped

    # The Fool with no last Tarot/Planet: create_card('Tarot_Planet', ..., forced_key=nil, 'fool')
    # draws a random Tarot/Planet; with no consumable room nothing is created (no RNG used).
    def fool(_card, ctx):
        gs = ctx.game_state or {}
        held = gs.get("consumables", [])
        if len(held) - any(c is _card for c in held) >= gs.get("consumable_slots", 2):
            return _cons.ConsumableResult()
        forced = gs.get("last_tarot_planet")
        d = {"type": "Tarot_Planet", "seed": "fool"}
        if forced:
            d["forced_key"] = forced
        return _cons.ConsumableResult(create=[d])
    _cons._CONSUMABLE_REGISTRY["c_fool"] = fool


@patch
def consumable_editions_score():
    """Wheel of Fortune / Aura / Hex / Ectoplasm: Jackdaw stored the bare edition flag
    ({'holo': True}) without Card:set_edition's scoring fields, so e.g. a Holographic joker from
    Wheel of Fortune never added its +10 Mult (the audit's 'Flush + Photograph + Abstract: real
    mult 41, sim 31' case: a Holo Lusty Joker)."""
    orig = _game._apply_consumable_result

    def wrapped(gs, result, card=None):
        orig(gs, result, card)
        ae = getattr(result, "add_edition", None)
        t = ae and ae.get("target")
        if t is not None and ae.get("edition") and hasattr(t, "set_edition"):
            t.set_edition(dict(ae["edition"]))
    _game._apply_consumable_result = wrapped


@patch
def smods_xmult_arithmetic():
    """Steamodded applies an xMult as `mult + mult * (x - 1)` (Scoring_Parameter mult:modify), not
    `mult * x`. The last-bit difference flips floor(chips * mult) when the product is an exact
    integer (the audit's Ramen 14742 vs 14741 cases). Re-exec scoring.py with that arithmetic."""
    import re
    from pathlib import Path
    from jackdaw.engine import scoring
    src = Path(scoring.__file__).read_text(encoding="utf8")
    new, n = re.subn(r"^(\s*)mult \*= (.+)$", r"\1mult = mult + mult * ((\2) - 1)", src, flags=re.M)
    if n != 10:  # the pinned commit has exactly these sites; anything else means the source moved
        raise RuntimeError(f"smods_xmult_arithmetic: expected 10 'mult *=' sites, found {n}")
    exec(compile(new, scoring.__file__, "exec"), scoring.__dict__)
    scoring.SMODS_XMULT_SITES = n


# pairs(G.GAME.hands) iteration order in the real (SMODS) game, recovered from 127 recorded To Do
# List rolls (scripts/diagnostics/sim_parity.py history; a unique permutation fits all of them).
# Where the three secret hands sit is unobserved (they only count once played): appended last.
HANDS_PAIRS_ORDER = ["Full House", "Flush", "Pair", "High Card", "Straight Flush", "Straight", "Two Pair",
                     "Three of a Kind", "Four of a Kind", "Five of a Kind", "Flush House", "Flush Five"]


@patch
def hands_pairs_order():
    """To Do List rolls pseudorandom_element over the visible hands in pairs(G.GAME.hands) order;
    Jackdaw used HAND_BASE order, so the same roll picked a different hand."""
    from jackdaw.engine import tags
    from jackdaw.engine.data.hands import HandType
    tags._ORBITAL_HANDS[:] = [HandType(h) for h in HANDS_PAIRS_ORDER]


@patch
def secret_hands_visible_only_when_played():
    """Lua sets G.GAME.hands[h].visible only when the hand is played (state_events.lua); leveling
    (Black Hole) does not reveal a secret hand. Visibility feeds the To Do List / Orbital pools."""
    from jackdaw.engine.hand_levels import HandLevels
    orig_level_up, orig_record = HandLevels.level_up, HandLevels.record_play

    def level_up(self, hand_type, amount=1):
        from jackdaw.engine.data.hands import HandType
        ht = HandType(hand_type) if isinstance(hand_type, str) else hand_type
        was = self._hands[ht].visible
        orig_level_up(self, hand_type, amount)
        self._hands[ht].visible = was

    def record_play(self, hand_type):
        from jackdaw.engine.data.hands import HandType
        orig_record(self, hand_type)
        self._hands[HandType(hand_type) if isinstance(hand_type, str) else hand_type].visible = True
    HandLevels.level_up, HandLevels.record_play = level_up, record_play


@patch
def orbital_choices():
    """Orbital Tag: Lua rolls G.GAME.orbital_choices[ante][Small|Big|Boss] (pseudoseed 'orbital',
    visible hands in pairs order) when each ante's blind-select screen is built, three rolls per
    ante whether or not an Orbital Tag exists, and the tag levels its blind's entry. Jackdaw rolled
    once per use over all twelve hands."""
    from jackdaw.engine import tags
    from jackdaw.engine.data.hands import HandType
    orig = tags.Tag.apply

    def choice(gs, rng):
        ante = gs.get("round_resets", {}).get("ante", 1)
        oc = gs.setdefault("orbital_choices", {})
        hl = gs.get("hand_levels")
        for a in range(1, ante + 1):
            if a in oc:
                continue
            pool = [h for h in HANDS_PAIRS_ORDER if hl is None or hl.get_state(HandType(h)).visible]
            oc[a] = {t: rng.element(pool, rng.seed("orbital"))[0] for t in ("Small", "Big", "Boss")}
        return oc[ante].get(gs.get("blind_on_deck") or "Small", oc[ante]["Small"])

    def apply(self, context, game_state, rng=None, **kw):
        if self.key == "tag_orbital" and context == "immediate" and rng is not None:
            return tags.TagResult(level_up=(HandType(choice(game_state, rng)), self.config["levels"]))
        return orig(self, context, game_state, rng=rng, **kw)
    tags.Tag.apply = apply


@patch
def ox_most_played_hand():
    """The Ox compares against G.GAME.current_round.most_played_poker_hand, which Lua recomputes
    only when a Boss Blind ends (state_events.lua end_round: max `played`, ties going to the later
    hand in pairs order); it starts as High Card. Jackdaw used the live count, so plays earlier in
    the Ox round itself could change the hand."""
    from jackdaw.engine.data.hands import HandType
    from jackdaw.engine.hand_levels import HandLevels
    orig = _game._round_won

    def round_won(gs):
        boss = gs.get("blind_on_deck") == "Boss"
        orig(gs)
        hl = gs.get("hand_levels")
        if boss and hl is not None:
            best, played = "High Card", -1
            for h in HANDS_PAIRS_ORDER:
                p = hl.get_state(HandType(h)).played
                if p >= played:  # Lua: `v.played > _played or (v.played == _played and 100 > v.order)`
                    best, played = h, p
            _MOST_PLAYED[id(hl)] = (hl, HandType(best))  # HandLevels has __slots__
            while len(_MOST_PLAYED) > 4096:
                _MOST_PLAYED.pop(next(iter(_MOST_PLAYED)))
            gs.setdefault("current_round", {})["most_played_poker_hand"] = best
    _game._round_won = round_won
    HandLevels.most_played = lambda self: _MOST_PLAYED.get(id(self), (self, HandType.HIGH_CARD))[1]


_MOST_PLAYED = {}


@patch
def amber_acorn_shuffles_three_times():
    """Amber Acorn: Lua queues G.jokers:shuffle('aajk') three times (blind.lua set_blind)."""
    orig = _game._apply_boss_blind_effects

    def wrapped(gs, blind):
        orig(gs, blind)
        jokers, rng = gs.get("jokers", []), gs.get("rng")
        if getattr(blind, "name", "") == "Amber Acorn" and rng and len(jokers) > 1:
            for _ in range(2):
                rng.shuffle(jokers, rng.seed("aajk"))
    _game._apply_boss_blind_effects = wrapped


@patch
def debuffed_purple_seal_inert():
    """A debuffed card's seal does not trigger (SMODS skips cards that cannot calculate): discarding
    a debuffed Purple Seal card (e.g. a face card under The Plant) creates no Tarot."""
    def muted(cards):
        hit = [c for c in cards if getattr(c, "debuff", False) and getattr(c, "seal", None) == "Purple"]
        for c in hit:
            c.seal = None
        return hit

    orig_discard, orig_fire = _game._handle_discard, _game._fire_discard_effects

    def handle_discard(gs, indices):
        hand = gs.get("hand", [])
        hit = muted([hand[i] for i in indices if isinstance(i, int) and 0 <= i < len(hand)])
        try:
            return orig_discard(gs, indices)
        finally:
            for c in hit:
                c.seal = "Purple"

    def fire_discard_effects(gs, discarded, *, hook):
        hit = muted(discarded)
        try:
            return orig_fire(gs, discarded, hook=hook)
        finally:
            for c in hit:
                c.seal = "Purple"
    _game._handle_discard, _game._fire_discard_effects = handle_discard, fire_discard_effects


@patch
def tarot_planet_pool_softlock():
    """get_current_pool('Tarot_Planet') (The Fool with nothing to copy) applies the Planet softlock
    gate to its planets like any pool does (common_events.lua: `v.set == 'Planet'`); Jackdaw only
    gated the 'Planet' pool, so it could create e.g. Planet X before any Five of a Kind."""
    from jackdaw.engine import pools
    from jackdaw.engine.data.prototypes import CENTER_POOLS
    planets = set(CENTER_POOLS.get("Planet", []))
    orig = pools._filter_key

    def filter_key(*, key, pool_type, **kw):
        res = orig(key=key, pool_type=pool_type, **kw)
        if pool_type == "Tarot_Planet" and key in planets and res != pools.UNAVAILABLE:
            return pools._filter_planet(key=key, played_hand_types=kw.get("played_hand_types") or set())
        return res
    pools._filter_key = filter_key


@patch
def crimson_heart():
    """Crimson Heart (SMODS blind.lua drawn_to_hand): re-picks its debuffed joker only when the
    blind is `prepped` (set by set_blind and by press_play, i.e. not after a discard), never
    re-picks the previous choice unless nothing else is left, and draws with
    pseudorandom_element over the joker Cards (sorted by sort_id). Jackdaw re-rolled on every
    draw, discards included, over joker indices."""
    from jackdaw.engine.blind import Blind
    orig_dth = Blind.drawn_to_hand

    def drawn_to_hand(self, hand_cards, joker_cards=None, rng=None):
        if self.name != "Crimson Heart":
            return orig_dth(self, hand_cards, joker_cards, rng)
        result = {}
        # set_blind leaves it prepped for the first draw; game.py preps it again on each play
        ready = getattr(self, "prepped", False) or not getattr(self, "_bb_drawn", False)
        self._bb_drawn = True
        self.prepped = False
        if ready and joker_cards and rng and not self.disabled:
            prev = []
            for j in joker_cards:
                if j.ability.pop("crimson_heart_chosen", None):
                    prev.append(j)
                    j.set_debuff(False)
            fallback = [j for j in joker_cards if not j.debuff]
            pool = [j for j in fallback if all(j is not p for p in prev)] or fallback
            if pool:
                chosen, _ = rng.element(pool, rng.seed("crimson_heart"))
                chosen.ability["crimson_heart_chosen"] = True
                chosen.set_debuff(True)
                self.triggered = True
                result["debuffed_joker_index"] = next(i for i, j in enumerate(joker_cards) if j is chosen)
        return result
    Blind.drawn_to_hand = drawn_to_hand

    orig_won = _game._round_won

    def round_won(gs):
        for j in gs.get("jokers", []):
            if j.ability.pop("crimson_heart_chosen", None):
                j.set_debuff(False)
        return orig_won(gs)
    _game._round_won = round_won


def apply():
    for fn in PATCHES:
        fn()
