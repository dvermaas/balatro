"""Jackdaw joker running state -> rumbot-sim JokerInstance fields (src/sim_bridge.py).

rumbot-sim keeps a joker's run-dependent value in `counter` (and Yorick's discard countdown in
`secondary`, Loyalty Card's start in `hands_at_create`, To Do List's hand in `named_hand`, sell
value growth in `extra_sell_value`); Jackdaw keeps it in the card's `ability` dict. Jokers not listed
have no state of their own (or derive it from the game state each time). Checked per joker by
scripts/diagnostics/rumbot_joker_parity.py.
"""

# Jackdaw keys whose name differs from rumbot-sim's (everything else matches after normalising).
RUMBOT_NAME = {"j_caino": "Canio"}


def _extra(a, key, default=0):
    e = a.get("extra")
    return e.get(key, default) if isinstance(e, dict) else default


def _f(v):
    return None if v is None else float(v)


# jackdaw key -> counter value from the ability dict
COUNTER = {
    # +Mult that grows/decays
    "j_ride_the_bus": lambda a: a.get("mult", 0),
    "j_green_joker": lambda a: a.get("mult", 0),
    "j_popcorn": lambda a: a.get("mult", 0),
    "j_trousers": lambda a: a.get("mult", 0),
    "j_ceremonial": lambda a: a.get("mult", 0),
    "j_flash": lambda a: a.get("mult", 0),
    "j_red_card": lambda a: a.get("mult", 0),
    # +Chips that grow/decay
    "j_runner": lambda a: _extra(a, "chips"),
    "j_ice_cream": lambda a: _extra(a, "chips", 100),
    "j_square": lambda a: _extra(a, "chips"),
    "j_wee": lambda a: _extra(a, "chips"),
    "j_castle": lambda a: _extra(a, "chips"),
    # xMult that grows/decays
    "j_ramen": lambda a: a.get("x_mult", 2),
    "j_obelisk": lambda a: a.get("x_mult", 1),
    "j_madness": lambda a: a.get("x_mult", 1),
    "j_yorick": lambda a: a.get("x_mult", 1),
    "j_hologram": lambda a: a.get("x_mult", 1),
    "j_constellation": lambda a: a.get("x_mult", 1),
    "j_lucky_cat": lambda a: a.get("x_mult", 1),
    "j_glass": lambda a: a.get("x_mult", 1),
    "j_campfire": lambda a: a.get("x_mult", 1),
    "j_vampire": lambda a: a.get("x_mult", 1),
    "j_hit_the_road": lambda a: a.get("x_mult", 1),
    "j_caino": lambda a: a.get("caino_xmult", 1),
    # counts
    "j_selzer": lambda a: a.get("extra", 10),
    "j_rocket": lambda a: _extra(a, "dollars", 1),
    "j_turtle_bean": lambda a: _extra(a, "h_size", 5),
    "j_invisible": lambda a: a.get("invis_rounds", 0),
}


def joker_fields(card):
    """rumbot-sim JokerInstance fields to set for a Jackdaw joker Card (None = keep rumbot's init)."""
    a, key = card.ability, card.center_key
    out = {"extra_sell_value": _f(a.get("extra_value") or 0)}
    if key in COUNTER:
        out["counter"] = _f(COUNTER[key](a))
    if key == "j_yorick":
        out["secondary"] = _f(a.get("yorick_discards", 23))
    if key == "j_loyalty_card":
        out["hands_at_create"] = int(a.get("hands_played_at_create") or 0)
    if key == "j_todo_list":
        out["named_hand"] = a.get("to_do_poker_hand") or _extra(a, "poker_hand", None)
    return out
