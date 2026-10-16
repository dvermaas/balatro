"""Jackdaw shop state -> rumbot-sim rollouts (stage B of the Rust port, docs/rust-port-scope.md).

`describe(gs)` turns a Jackdaw game state in the SHOP phase (rollout.build_state + apply_option) into
the JSON that `jev_sim.rollouts` builds a rumbot-sim game from; `rollouts(gs, policy, n, bosses)`
returns the same per-blind rows as `rollout.play_horizon` for seeds rollout._seed(0..n-1).
v24: rollout.run_options uses it for the shop forecast (rollout.forecast) when jev_sim is built.
Part of the prompt version hash (see recorder.PROMPT_VERSION), like sim_bridge_jokers.py and the Rust.
"""
from __future__ import annotations

import json
import os
import unicodedata

import sim_backend  # noqa: F401  (puts Jackdaw on sys.path, applies sim_patches)

from jackdaw.engine.data.hands import HandType  # noqa: E402

import native  # noqa: E402

try:
    import jev_sim as _rs
except ImportError:
    _rs = None

try:  # per-joker running state (counters); checked by scripts/diagnostics/rumbot_joker_parity.py
    from sim_bridge_jokers import RUMBOT_NAME, joker_fields
except ImportError:
    RUMBOT_NAME, joker_fields = {}, None

_CENTERS = None
RANK = {"Ace": "A", "King": "K", "Queen": "Q", "Jack": "J", "10": "T", **{str(n): str(n) for n in range(2, 10)}}
SUIT = {"Hearts": "H", "Diamonds": "D", "Clubs": "C", "Spades": "S"}
EDITIONS = ("foil", "holo", "polychrome", "negative")


def available():
    """jev_sim is built and not turned off (JEV_NATIVE=0 turns off every native path)."""
    return _rs is not None and os.environ.get("JEV_NATIVE") != "0"


def backend():
    return "rumbot" if available() else "jackdaw"


def _nm(x):
    return "".join(ch for ch in unicodedata.normalize("NFKD", x).lower() if ch.isalnum())


def _names():
    """{normalised name: rumbot name} for jokers and consumables, from jev_sim itself."""
    global _CENTERS
    if _CENTERS is None:
        jk, cons, vouchers, bosses = _rs.names()
        import jackdaw.engine.data as data
        path = os.path.join(os.path.dirname(data.__file__), "centers.json")
        centers = json.load(open(path, encoding="utf-8"))
        by_norm = {_nm(n): n for n in jk + cons}
        _CENTERS = {k: by_norm.get(_nm(v.get("name", ""))) for k, v in centers.items()}
        _CENTERS.update(RUMBOT_NAME)
    return _CENTERS


def _name(key):
    n = _names().get(key)
    if n is None:
        raise KeyError(f"no rumbot-sim name for {key}")
    return n


def _edition(c):
    ed = c.edition or {}
    return next((e for e in EDITIONS if ed.get(e)), None)


def _rank(r):
    return RANK.get(r, r)


def _suit(s):
    return SUIT.get(s, s)


def _playing(c):
    enh = c.center_key[2:] if c.center_key.startswith("m_") else None
    return {"sort_id": c.sort_id, "rank": _rank(c.base.rank.value), "suit": _suit(c.base.suit.value), "enh": enh,
            "edition": _edition(c), "seal": c.seal.lower() if c.seal else None,
            "extra_chips": int(c.ability.get("perma_bonus") or 0), "debuff": bool(c.debuff),
            "played_this_ante": bool(c.ability.get("played_this_ante"))}


def _joker(c):
    d = {"sort_id": c.sort_id, "name": _name(c.center_key), "edition": _edition(c), "eternal": bool(c.eternal),
         "perishable": bool(c.perishable), "perish_tally": int(c.perish_tally or 0), "rental": bool(c.rental),
         "debuffed": bool(c.debuff)}
    if joker_fields is not None:
        d.update({k: v for k, v in joker_fields(c).items() if v is not None})
    return d


def describe(gs):
    """JSON for jev_sim.rollouts from a Jackdaw state in the SHOP phase."""
    rr, cr = gs["round_resets"], gs.get("current_round", {})
    cards = []
    for area in ("deck", "hand", "discard_pile", "played_cards_area"):
        cards += [_playing(c) for c in gs.get(area, []) or []]
    usage = gs.get("consumeable_usage", {}) or {}
    planets = {k: v for k, v in usage.items() if (v or {}).get("set") == "Planet"}
    tarots = max(int(gs.get("consumable_usage_tarot") or 0), int((gs.get("consumable_usage_total") or {}).get("tarot", 0)))
    idol, mail = cr.get("idol_card") or {}, cr.get("mail_card") or {}
    hl = gs["hand_levels"]
    levels = {}
    for ht in HandType:
        s = hl.get_state(ht)
        levels[ht.value] = {"level": int(s.level), "played": int(s.played), "played_this_round": bool(s.played_this_round)}
    desc = {
        "deck": _CENTERS_NAME(gs.get("selected_back_key", "b_red")), "stake": int(gs.get("stake", 1)),
        "ante": int(rr["ante"]), "blind_index": {"Small": 0, "Big": 1, "Boss": 2}[gs.get("blind_on_deck") or "Small"],
        "round": int(gs.get("round", 0)), "money": int(gs.get("dollars", 0)), "hands_played": int(gs.get("hands_played", 0)),
        "skips": int(gs.get("skips", 0)), "boss": rr["blind_choices"].get("Boss") or "",
        "bosses_used": {k: int(v) for k, v in (gs.get("bosses_used") or {}).items()},
        "cards": cards, "jokers": [_joker(c) for c in gs.get("jokers", [])],
        "consumables": [{"sort_id": c.sort_id, "name": _name(c.center_key), "negative": _edition(c) == "negative"}
                        for c in gs.get("consumables", [])],
        "vouchers": [k for k, v in (gs.get("used_vouchers") or {}).items() if v],
        "levels": levels,
        "idol": [_rank(idol["rank"]), _suit(idol["suit"])] if idol.get("rank") and idol.get("suit") else None,
        "mail": _rank(mail["rank"]) if mail.get("rank") else None,
        "ancient": _suit((cr.get("ancient_card") or {}).get("suit")) if (cr.get("ancient_card") or {}).get("suit") else None,
        "castle": _suit((cr.get("castle_card") or {}).get("suit")) if (cr.get("castle_card") or {}).get("suit") else None,
        "tarots_used": tarots, "planets_used": sum(int((v or {}).get("count", 1)) for v in planets.values()),
        "unique_planets": [_name(k) for k in planets],
        "pool_flags": [k for k, v in (gs.get("pool_flags") or {}).items() if v],
        "most_played": cr.get("most_played_poker_hand"),
        "hand_size": int(gs.get("hand_size", 8)), "joker_slots": int(gs.get("joker_slots", 5)),
        "consumable_slots": int(gs.get("consumable_slots", 2)),
    }
    return json.dumps(desc)


def _CENTERS_NAME(back_key):
    path_names = {"b_red": "Red Deck", "b_blue": "Blue Deck", "b_yellow": "Yellow Deck", "b_green": "Green Deck",
                  "b_black": "Black Deck", "b_magic": "Magic Deck", "b_nebula": "Nebula Deck", "b_ghost": "Ghost Deck",
                  "b_abandoned": "Abandoned Deck", "b_checkered": "Checkered Deck", "b_zodiac": "Zodiac Deck",
                  "b_painted": "Painted Deck", "b_anaglyph": "Anaglyph Deck", "b_plasma": "Plasma Deck",
                  "b_erratic": "Erratic Deck"}
    return path_names.get(back_key, "Red Deck")


def policy_jokers(policy):
    """rollout.Policy's text-scorer jokers, encoded like native.scorer's."""
    return [(native._code(native._EDITION, j["edition"], 4), native._rule(j["rule"])) for j in policy.js]


def run(desc, policy, seeds, bosses):
    """[rows] for each seed, from a describe() JSON: rollout.play_horizon's per-blind dicts."""
    raw = _rs.rollouts(desc, policy_jokers(policy), seeds, bosses)
    return [[{"ante": a, "type": t, "boss": b, "target": tg, "chips": ch, "cleared": cl} for a, t, b, tg, ch, cl in r]
            for r in raw]


def rollouts(gs, policy, n, bosses, tag="RO"):
    """[rows] like [rollout.play_horizon(copy reseeded with rollout._seed(k, tag), policy, bosses)]."""
    return run(describe(gs), policy, [f"{tag}{k:06d}" for k in range(n)], bosses)

