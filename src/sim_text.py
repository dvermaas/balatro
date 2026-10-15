"""BalatroBot-style effect text for the Jackdaw sim backend.

BalatroBot reads a card's `value.effect` from the game's tooltip UI: Card:generate_UIBox_ability_table
-> generate_card_ui -> localize(), keeping every plain text node of the main description box (one
string per line, lines joined by a space). This module re-implements that pipeline from the game's
own localization file filled with each card's current values (ports of the loc_vars chains in
card.lua / common_events.lua as patched by Steamodded), plus the blind and tag strings BalatroBot
builds in utils/gamestate.lua.

The localization is the game's copyrighted text, so it is never committed: it is loaded at runtime
from $BALATRO_LOC_DIR (default runs/_loc/), see scripts/extract_localization.py.
"""

from __future__ import annotations

import math
import os
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOC_DIR = Path(os.environ.get("BALATRO_LOC_DIR") or ROOT / "runs" / "_loc")


# ---------------------------------------------------------------------------
# Lua table-literal parser (enough for localization files)
# ---------------------------------------------------------------------------

_TOKEN = re.compile(r"""
    (?P<ws>\s+|--\[\[.*?\]\]|--[^\n]*)
  | (?P<str>"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|\[\[.*?\]\])
  | (?P<num>-?0x[0-9a-fA-F]+|-?\d+\.?\d*(?:[eE][-+]?\d+)?)
  | (?P<name>[A-Za-z_][A-Za-z0-9_]*)
  | (?P<op>[{}\[\]=,;])
""", re.S | re.X)

_ESC = {"n": "\n", "t": "\t", "\\": "\\", '"': '"', "'": "'", "r": "\r", "\n": "\n"}


def _unescape(s):
    if s.startswith("[["):
        return s[2:-2]
    body = s[1:-1]
    return re.sub(r"\\(.)", lambda m: _ESC.get(m.group(1), m.group(1)), body, flags=re.S)


def parse_lua_table(text):
    """Parse `return { ... }` (constructors, strings, numbers, booleans) into dicts/lists."""
    toks = []
    pos = 0
    while pos < len(text):
        m = _TOKEN.match(text, pos)
        if not m:
            raise ValueError(f"Lua parse error at {pos}: {text[pos:pos + 40]!r}")
        pos = m.end()
        kind = m.lastgroup
        if kind != "ws":
            toks.append((kind, m.group(kind)))
    i = 0
    if toks and toks[0] == ("name", "return"):
        i = 1

    def value(i):
        kind, v = toks[i]
        if kind == "str":
            return _unescape(v), i + 1
        if kind == "num":
            return (int(v, 16) if "x" in v.lower() else (float(v) if any(c in v for c in ".eE") else int(v))), i + 1
        if kind == "name":
            return {"true": True, "false": False, "nil": None}.get(v, v), i + 1
        if v == "{":
            return table(i + 1)
        raise ValueError(f"unexpected token {v!r}")

    def table(i):
        arr, d = [], {}
        while toks[i][1] != "}":
            if toks[i][1] == "[":
                k, i = value(i + 1)
                i += 2  # ] =
                v, i = value(i)
                d[k] = v
            elif toks[i][0] == "name" and toks[i + 1][1] == "=":
                k = toks[i][1]
                v, i = value(i + 2)
                d[k] = v
            else:
                v, i = value(i)
                arr.append(v)
            if toks[i][1] in (",", ";"):
                i += 1
        if d and arr:
            for n, v in enumerate(arr, 1):
                d[n] = v
            return d, i + 1
        return (d if d or not arr else arr), i + 1

    v, _ = value(i)
    return v


# ---------------------------------------------------------------------------
# Loading and parsing (G.localization, init_localization, loc_parse_string)
# ---------------------------------------------------------------------------

_LOC = None
_HANDS = None


def _merge(dst, src):
    for k, v in src.items():
        if isinstance(v, dict) and isinstance(dst.get(k), dict):
            _merge(dst[k], v)
        else:
            dst[k] = v


class LocalizationMissing(RuntimeError):
    pass


def loc():
    global _LOC
    if _LOC is None:
        f = LOC_DIR / "en-us.lua"
        if not f.exists():
            raise LocalizationMissing(
                f"Balatro localization not found at {f}. Run scripts/extract_localization.py on a machine "
                "with the game, or set BALATRO_LOC_DIR to a folder holding en-us.lua (+ smods-en-us.lua).")
        data = parse_lua_table(f.read_text(encoding="utf8"))
        s = LOC_DIR / "smods-en-us.lua"
        if s.exists():  # Steamodded overlays its own strings (SMODS.handle_loc_file)
            _merge(data, parse_lua_table(s.read_text(encoding="utf8")))
        # Steamodded's process_loc_text for The Wheel (game_objects/blind.lua): "#1# in #2# ..."
        wheel = data.get("descriptions", {}).get("Blind", {}).get("bl_wheel")
        if wheel and wheel.get("text"):
            t = wheel["text"][0]
            t = t if t.startswith("#1#") else "#1#" + t
            wheel["text"][0] = t.replace("7", "#2#")
        _LOC = data
    return _LOC


def game_hands():
    """Default G.GAME.hands table (for `example`), from runs/_loc/game-hands.lua."""
    global _HANDS
    if _HANDS is None:
        f = LOC_DIR / "game-hands.lua"
        _HANDS = parse_lua_table(f.read_text(encoding="utf8")) if f.exists() else {}
    return _HANDS


def parse_line(line):
    """Port of misc_functions.lua loc_parse_string: [{'strings': [str | [n]], 'control': {}}].
    An empty line parses to None (Lua returns nil and the line is dropped)."""
    if not line:
        return None
    parts, control, strs, buf = [], {}, [], None
    i, n = 0, len(line)
    while i < n:
        ch = line[i]
        if ch == "{":
            if buf is not None:
                strs.append(buf)
                buf = None
            if strs or "element" in control:
                parts.append({"strings": strs, "control": control})
            strs, control = [], {}
            j = line.index("}", i)
            for item in line[i + 1:j].split(","):
                if ":" in item:
                    k, v = item.split(":", 1)
                    if k:
                        control[k] = v
            i = j + 1
            continue
        if ch == "#":
            j = line.find("#", i + 1)
            if j < 0:
                break
            if buf is not None:
                strs.append(buf)
                buf = None
            ref = line[i + 1:j]
            if ref:
                strs.append([ref])
            i = j + 1
            continue
        if control.get("X") is not None and ch.isspace():
            ch = ""
        buf = (buf or "") + ch
        i += 1
    if buf is not None:
        strs.append(buf)
    if strs or "element" in control:
        parts.append({"strings": strs, "control": control})
    return parts


def _text_parsed(entry):
    """text_parsed for a descriptions entry (multi-box text = list of lists: only box 1 is 'main')."""
    text = entry.get("text") or []
    if isinstance(text, str):
        text = [text]
    if text and isinstance(text[0], list):
        text = text[0]
    return [p for p in (parse_line(t) for t in text) if p is not None]


# ---------------------------------------------------------------------------
# Number formatting (number_format / format_ui_value / Lua tostring)
# ---------------------------------------------------------------------------

def lua_tostring(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if v is None:
        return "nil"
    if isinstance(v, (int, float)):
        if isinstance(v, float) and math.isinf(v):
            return "inf" if v > 0 else "-inf"
        if float(v).is_integer() and abs(v) < 1e15:
            return str(int(v))
        return "%.14g" % v
    return str(v)


def number_format(num, e_switch_point=None):
    if not isinstance(num, (int, float)) or isinstance(num, bool):
        return num
    sign = "" if num >= 0 else "-"
    num = abs(num)
    if num >= (e_switch_point or 100000000000):
        x = float("%.4g" % num)
        fac = math.floor(math.log10(x))
        if num == math.inf:
            return sign + "naneinf"
        mantissa = round(x / 10 ** fac, 3)
        if mantissa >= 10:
            mantissa /= 10
            fac += 1
        return sign + (("%.1fe%i" if fac >= 100 else "%.2fe%i" if fac >= 10 else "%.3fe%i") % (mantissa, fac))
    if num != math.floor(num) and num < 100:
        formatted = ("%.1f" if num >= 10 else "%.2f") % num
        if formatted.endswith("0"):
            formatted = re.sub(r"\.?0+$", "", formatted)
        if num < 0.01:
            return lua_tostring(num)
    else:
        formatted = "%.0f" % num
    rev = re.sub(r"(\d\d\d)", r"\1,", formatted[::-1])
    rev = re.sub(r",$", "", rev)
    return sign + rev[::-1]


def format_ui_value(v):
    if not isinstance(v, (int, float)) or isinstance(v, bool):
        return lua_tostring(v)
    return number_format(v, 1000000)


def signed(v):  # SMODS.signed
    return ("+" + lua_tostring(v) if v >= 0 else lua_tostring(v)) if v is not None else "+0"


def signed_dollars(v):  # SMODS.signed_dollars
    return (("-" if v < 0 else "") + "$" + lua_tostring(abs(v))) if v is not None else "$0"


# ---------------------------------------------------------------------------
# localize()
# ---------------------------------------------------------------------------

def _var(vars_, sub):
    n = sub[0]
    try:
        v = vars_[int(n) - 1] if int(n) - 1 < len(vars_) else None
    except ValueError:
        v = None
    return format_ui_value(v)


def box_lines(lines, vars_):
    """Text BalatroBot extracts from localize'd UI lines: every part's assembled string (DynaText
    parts such as {E:1} included), markup removed, one string per line."""
    return ["".join("".join(x if isinstance(x, str) else _var(vars_, x) for x in part["strings"]) for part in line)
            for line in lines]


def raw_lines(lines, vars_):
    """localize{type='raw_descriptions'} (used for tags): every part, markup removed."""
    return ["".join("".join(x if isinstance(x, str) else _var(vars_, x) for x in part["strings"]) for part in line)
            for line in lines]


_PARSED = {}


def desc(set_, key):
    k = (set_, key)
    if k not in _PARSED:
        entry = loc()["descriptions"].get(set_, {}).get(key)
        _PARSED[k] = _text_parsed(entry) if isinstance(entry, dict) else None
    return _PARSED[k]


def localize_desc(set_, key, vars_=()):
    lines = desc(set_, key)
    return box_lines(lines, list(vars_)) if lines else []


def localize_other(key, vars_=()):
    return localize_desc("Other", key, vars_)


def misc(cat, key):
    return (loc()["misc"].get(cat) or {}).get(key, "ERROR")


def dictionary(key):
    return loc()["misc"]["dictionary"].get(key, "ERROR")


def variable(key, vars_):
    t = loc()["misc"]["v_dictionary"].get(key)
    if t is None:
        return "ERROR"
    parts = parse_line(t) or []
    return re.sub(r"{.-}", "", "".join("".join(x if isinstance(x, str) else _var(vars_, x) for x in p["strings"])
                                        for p in parts))


def name_text(set_, key):
    e = loc()["descriptions"].get(set_, {}).get(key)
    if not e:
        return "ERROR"
    n = e.get("name")
    return " ".join(n) if isinstance(n, list) else n


def poker_hand(h):
    return misc("poker_hands", h)


# ---------------------------------------------------------------------------
# Game-state helpers
# ---------------------------------------------------------------------------

def _playing_cards(gs):
    return list(gs.get("deck") or []) + list(gs.get("hand") or []) + list(gs.get("discard_pile") or []) + \
        list(gs.get("played_cards_area") or [])


def _prob(gs, base, odds):
    """SMODS.get_probability_vars: numerator scaled by G.GAME.probabilities.normal (Oops! All 6s)."""
    normal = (gs.get("probabilities") or {}).get("normal", 1)
    return base * normal, odds


def _cr(gs):
    return gs.get("current_round") or {}


def _ab(card):
    return card.ability or {}


def _ex(card):
    e = _ab(card).get("extra")
    return e if isinstance(e, dict) else {}


# ---------------------------------------------------------------------------
# Jokers (Card:generate_UIBox_ability_table)
# ---------------------------------------------------------------------------

def _joker_vars(card, gs, in_jokers):
    a, x, e = _ab(card), _ab(card).get("extra"), _ex(card)
    name = a.get("name")
    G = gs
    lv = None
    main_start = main_end = None
    if name == "Joker":
        lv = [a.get("mult")]
    elif name in ("Jolly Joker", "Zany Joker", "Mad Joker", "Crazy Joker", "Droll Joker"):
        lv = [a.get("t_mult"), poker_hand(a.get("type"))]
    elif name in ("Sly Joker", "Wily Joker", "Clever Joker", "Devious Joker", "Crafty Joker"):
        lv = [a.get("t_chips"), poker_hand(a.get("type"))]
    elif name == "Half Joker":
        lv = [e.get("mult"), e.get("size")]
    elif name == "Fortune Teller":
        lv = [x, (G.get("consumable_usage_total") or {}).get("tarot", 0)]
    elif name == "Steel Joker":
        tally = sum(1 for c in _playing_cards(G) if c.center_key == "m_steel")
        lv = [x, 1 + x * tally]
    elif name == "Chaos the Clown":
        lv = [x]
    elif name == "Space Joker":
        lv = list(_prob(G, 1, x))
    elif name == "Stone Joker":
        tally = sum(1 for c in _playing_cards(G) if c.center_key == "m_stone")
        lv = [x, x * tally]
    elif name == "Drunkard":
        lv = [a.get("d_size")]
    elif name == "Green Joker":
        lv = [e.get("hand_add"), e.get("discard_sub"), a.get("mult")]
    elif name == "Credit Card":
        lv = [x]
    elif name in ("Greedy Joker", "Lusty Joker", "Wrathful Joker", "Gluttonous Joker"):
        lv = [e.get("s_mult"), misc("suits_singular", e.get("suit"))]
    elif name == "Blue Joker":
        lv = [x, x * len(G.get("deck") or [])]
    elif name == "Sixth Sense":
        lv = []
    elif name == "Hack":
        lv = [x + 1]
    elif name == "Faceless Joker":
        lv = [e.get("dollars"), e.get("faces")]
    elif name == "Juggler":
        lv = [a.get("h_size")]
    elif name == "Golden Joker":
        lv = [x]
    elif name == "Joker Stencil":  # Card:update: empty slots + every Stencil (can go to 0 or below)
        js = G.get("jokers") or []
        lv = [G.get("joker_slots", 5) - len(js) + sum(1 for j in js if _ab(j).get("name") == "Joker Stencil")]
    elif name == "Ceremonial Dagger":
        lv = [a.get("mult")]
    elif name == "Banner":
        lv = [x]
    elif name == "Misprint":
        main_start = ["  +"]  # the rest of the line is DynaText (random numbers)
    elif name == "Mystic Summit":
        lv = [e.get("mult"), e.get("d_remaining")]
    elif name == "Loyalty Card":
        # Lua recomputes it while scoring each hand, before G.GAME.hands_played counts that hand:
        # (every - 1 - (hands_played - at_create)) % (every + 1)
        every = e.get("every", 5)
        rem = (every - (G.get("hands_played", 0) - a.get("hands_played_at_create", 0))) % (every + 1)
        lv = [e.get("Xmult"), e.get("every", 0) + 1,
              variable("loyalty_active" if rem == 0 else "loyalty_inactive", [rem])]
    elif name == "8 Ball":
        lv = list(_prob(G, 1, x))
    elif name == "Dusk":
        lv = [x + 1]
    elif name == "Fibonacci":
        lv = [x]
    elif name == "Scary Face":
        lv = [x]
    elif name == "Abstract Joker":
        lv = [x, len(G.get("jokers") or []) * x]
    elif name == "Delayed Gratification":
        lv = [x]
    elif name == "Gros Michel":
        lv = [e.get("mult"), *_prob(G, 1, e.get("odds"))]
    elif name in ("Even Steven", "Odd Todd"):
        lv = [x]
    elif name == "Scholar":
        lv = [e.get("mult"), e.get("chips")]
    elif name == "Business Card":
        lv = list(_prob(G, 1, x))
    elif name == "Spare Trousers":
        lv = [x, poker_hand("Two Pair"), a.get("mult")]
    elif name == "Superposition":
        lv = [x]
    elif name == "Ride the Bus":
        lv = [x, a.get("mult")]
    elif name in ("Egg", "Burglar"):
        lv = [x]
    elif name == "Blackboard":
        lv = [x, misc("suits_plural", "Spades"), misc("suits_plural", "Clubs")]
    elif name in ("Runner", "Ice Cream"):
        lv = [e.get("chips"), e.get("chip_mod")]
    elif name == "DNA":
        lv = [x]
    elif name == "Constellation":
        lv = [x, a.get("x_mult")]
    elif name == "Hiker":
        lv = [x]
    elif name == "To Do List":
        lv = [e.get("dollars"), poker_hand(a.get("to_do_poker_hand"))]
    elif name in ("Blueprint", "Brainstorm"):
        main_end = [""] if in_jokers else None
    elif name == "Astronomer":
        lv = [x]
    elif name in ("Golden Ticket", "Acrobat"):
        lv = [x]
    elif name == "Sock and Buskin":
        lv = [x + 1]
    elif name == "Swashbuckler":  # Card:update: sell value of every other joker in G.jokers
        lv = [sum(j.sell_cost or 0 for j in G.get("jokers") or [] if j is not card)]
    elif name == "Troubadour":
        lv = [e.get("h_size"), -(e.get("h_plays") or 0)]
    elif name == "Certificate":
        lv = [x]
    elif name == "Throwback":
        lv = [x, a.get("x_mult")]
    elif name in ("Hanging Chad", "Rough Gem", "Arrowhead", "Onyx Agate"):
        lv = [x]
    elif name == "Bloodstone":
        lv = [*_prob(G, 1, e.get("odds")), e.get("Xmult")]
    elif name == "Glass Joker":
        lv = [x, a.get("x_mult")]
    elif name == "Flower Pot":
        lv = [x]
    elif name == "Wee Joker":
        lv = [e.get("chips"), e.get("chip_mod")]
    elif name == "Merry Andy":
        lv = [a.get("d_size"), a.get("h_size")]
    elif name == "The Idol":
        idol = _cr(G).get("idol_card") or {}
        lv = [x, misc("ranks", idol.get("rank")), misc("suits_plural", idol.get("suit"))]
    elif name in ("Seeing Double", "Matador"):
        lv = [x]
    elif name == "Hit the Road":
        lv = [x, a.get("x_mult")]
    elif name in ("The Duo", "The Trio", "The Family", "The Order", "The Tribe"):
        lv = [a.get("x_mult"), poker_hand(a.get("type"))]
    elif name == "Cavendish":
        lv = [e.get("Xmult"), *_prob(G, 1, e.get("odds"))]
    elif name == "Card Sharp":
        lv = [e.get("Xmult")]
    elif name == "Red Card":
        lv = [x, a.get("mult")]
    elif name == "Madness":
        lv = [x, a.get("x_mult")]
    elif name == "Square Joker":
        lv = [e.get("chips"), e.get("chip_mod")]
    elif name == "Seance":
        lv = [poker_hand(e.get("poker_hand"))]
    elif name == "Riff-raff":
        lv = [x]
    elif name in ("Vampire", "Hologram", "Obelisk", "Lucky Cat", "Campfire"):
        lv = [x, a.get("x_mult")]
    elif name in ("Vagabond", "Baron"):
        lv = [x]
    elif name == "Cloud 9":
        tally = sum(1 for c in _playing_cards(G) if c.get_id() == 9)
        lv = [x, x * tally]
    elif name == "Rocket":
        lv = [e.get("dollars"), e.get("increase")]
    elif name == "Luchador":
        if in_jokers:
            main_end = [""]  # its ' Active! ' text sits two node levels deep: BalatroBot keeps an empty line
    elif name in ("Photograph", "Gift Card"):
        lv = [x]
    elif name == "Turtle Bean":
        lv = [e.get("h_size"), e.get("h_mod")]
    elif name == "Erosion":
        start = G.get("starting_deck_size", 52)
        lv = [x, max(0, x * (start - len(_playing_cards(G)))), start]
    elif name == "Reserved Parking":
        lv = [e.get("dollars"), *_prob(G, 1, e.get("odds"))]
    elif name == "Mail-In Rebate":
        lv = [x, misc("ranks", (_cr(G).get("mail_card") or {}).get("rank"))]
    elif name == "To the Moon":
        lv = [x]
    elif name == "Hallucination":
        lv = list(_prob(G, 1, x))
    elif name == "Baseball Card":
        lv = [x]
    elif name == "Bull":
        lv = [x, x * max(0, G.get("dollars", 0))]
    elif name == "Diet Cola":
        lv = [name_text("Tag", "tag_double")]
    elif name == "Trading Card":
        lv = [x]
    elif name == "Flash Card":
        lv = [x, a.get("mult")]
    elif name == "Popcorn":
        lv = [a.get("mult"), x]
    elif name == "Ramen":
        lv = [a.get("x_mult"), x]
    elif name == "Ancient Joker":
        lv = [x, misc("suits_singular", (_cr(G).get("ancient_card") or {}).get("suit"))]
    elif name == "Walkie Talkie":
        lv = [e.get("chips"), e.get("mult")]
    elif name == "Seltzer":
        lv = [x]
    elif name == "Castle":
        lv = [e.get("chip_mod"), misc("suits_singular", (_cr(G).get("castle_card") or {}).get("suit")), e.get("chips")]
    elif name == "Smiley Face":
        lv = [x]
    elif name == "Stuntman":
        lv = [e.get("chip_mod"), e.get("h_size")]
    elif name == "Invisible Joker":
        lv = [x, a.get("invis_rounds")]
        if any(j.edition and j.edition.get("negative") for j in (G.get("jokers") or [])):
            main_end = localize_other("remove_negative")
    elif name == "Satellite":
        used = G.get("consumeable_usage") or {}
        n = sum(1 for k, v in used.items() if (isinstance(v, dict) and v.get("set") == "Planet")
                or (not isinstance(v, dict) and str(k).startswith("c_") and _is_planet(k)))
        lv = [x, n * x]
    elif name == "Shoot the Moon":
        lv = [x]
    elif name == "Driver's License":
        tally = sum(1 for c in _playing_cards(G) if c.center_key != "c_base")
        lv = [x, tally]
    elif name == "Bootstraps":
        lv = [e.get("mult"), e.get("dollars"), e.get("mult") * math.floor(G.get("dollars", 0) / e.get("dollars"))]
    elif name == "Caino":
        lv = [x, a.get("caino_xmult")]
    elif name == "Triboulet":
        lv = [x]
    elif name == "Yorick":
        lv = [e.get("xmult"), e.get("discards"), a.get("yorick_discards"), a.get("x_mult")]
    elif name == "Perkeo":
        lv = [x]
    return lv, main_start, main_end


def _is_planet(key):
    from jackdaw.engine.data.prototypes import CENTER_POOLS
    return key in CENTER_POOLS.get("Planet", [])


# ---------------------------------------------------------------------------
# Consumables, vouchers, boosters, playing cards (generate_card_ui)
# ---------------------------------------------------------------------------

_CENTERS = None


def center(key):
    global _CENTERS
    if _CENTERS is None:
        from jackdaw.engine.data.prototypes import _load_json
        _CENTERS = _load_json("centers.json")
    return _CENTERS.get(key) or {}


def _cfg(card):
    c = center(card.center_key).get("config") or {}
    return c if isinstance(c, dict) else {}


def _tarot_vars(card, gs):
    cfg, name = _cfg(card), _ab(card).get("name")
    main_end = None
    lv = []
    if name == "The Fool":
        ltp = gs.get("last_tarot_planet")
        c = center(ltp) if ltp else None
        lv = [name_text(c.get("set"), ltp) if c else dictionary("k_none")]
        main_end = [""]
    elif name in ("The Magician", "The Empress", "The Hierophant", "The Lovers", "The Chariot", "Justice",
                  "The Devil", "The Tower"):
        lv = [cfg.get("max_highlighted"), name_text("Enhanced", cfg.get("mod_conv"))]
    elif name == "The High Priestess":
        lv = [cfg.get("planets")]
    elif name == "The Emperor":
        lv = [cfg.get("tarots")]
    elif name == "The Hermit":
        lv = [cfg.get("extra")]
    elif name == "The Wheel of Fortune":
        lv = list(_prob(gs, 1, cfg.get("extra")))
    elif name in ("Strength", "The Hanged Man", "Death"):
        lv = [cfg.get("max_highlighted")]
    elif name == "Temperance":
        money = sum(j.sell_cost for j in (gs.get("jokers") or []) if _ab(j).get("set") == "Joker")
        lv = [cfg.get("extra"), min(cfg.get("extra"), money)]
    elif name in ("The Star", "The Moon", "The Sun", "The World"):
        lv = [cfg.get("max_highlighted"), misc("suits_plural", cfg.get("suit_conv"))]
    return lv, main_end


def _spectral_vars(card, gs):
    cfg, name = _cfg(card), _ab(card).get("name")
    lv, main_end = [], None
    ex = cfg.get("extra")
    if name in ("Familiar", "Grim", "Incantation", "Cryptid"):
        lv = [ex]
    elif name == "Immolate":
        lv = [ex.get("destroy"), ex.get("dollars")]
    elif name == "Ankh":
        if any(j.edition and j.edition.get("negative") for j in (gs.get("jokers") or [])):
            main_end = localize_other("remove_negative")
    if name == "Ectoplasm":
        lv = [gs.get("ecto_minus") or 1]
    return lv, main_end


def _planet_vars(card, gs):
    from jackdaw.engine.data.hands import HAND_BASE, HandType
    ht = _cfg(card).get("hand_type")
    hl = gs.get("hand_levels")
    level = hl.get_state(HandType(ht)).level if hl is not None else 1
    base = HAND_BASE[HandType(ht)]
    return [level, poker_hand(ht), base.l_mult, base.l_chips]


_VOUCHER_EXTRA = {"Tarot Merchant", "Tarot Tycoon", "Planet Merchant", "Planet Tycoon"}


def _voucher_vars(card):
    cfg, name = _cfg(card), _ab(card).get("name") or center(card.center_key).get("name")
    if name in _VOUCHER_EXTRA:
        return [cfg.get("extra_disp")]
    if name in ("Seed Money", "Money Tree"):
        return [cfg.get("extra") / 5]
    if name in ("Overstock", "Overstock Plus", "Blank", "Antimatter"):
        return []
    return [cfg.get("extra")]


def _booster(card):
    c = center(card.center_key)
    key = re.sub(r"_\d+$", "", card.center_key)  # p_arcana_normal_2 -> p_arcana_normal
    cfg = c.get("config") or {}
    return key, [cfg.get("choose"), cfg.get("extra")]


_ENH_LABEL = {"c_base": "Base Card", "m_bonus": "Bonus Card", "m_mult": "Mult Card", "m_wild": "Wild Card",
              "m_glass": "Glass Card", "m_steel": "Steel Card", "m_stone": "Stone Card", "m_gold": "Gold Card",
              "m_lucky": "Lucky Card"}


def card_label(card):
    if card.base is not None:
        return _ENH_LABEL.get(card.center_key, "Base Card")
    return _ab(card).get("name") or center(card.center_key).get("name") or ""


def _playing_card_lines(card, gs):
    a = _ab(card)
    nominal = card.base.nominal if card.base else 0
    bonus = (a.get("bonus") or 0) + (a.get("perma_bonus") or 0)
    k = card.center_key
    out = []
    chips_line = localize_other("card_chips" + ("_minus" if nominal < 0 else ""), [nominal]) if nominal else []
    if k == "c_base":
        out += chips_line
        if bonus:
            out += localize_other("card_extra_chips", [signed(bonus)])
    elif k == "m_lucky":  # Steamodded owns Lucky (SMODS.Enhancement generate_ui)
        out += chips_line
        nm, dm = _prob(gs, 1, 5)
        nd, dd = _prob(gs, 1, 15)
        out += localize_desc("Enhanced", k, [nm, a.get("mult"), dm, a.get("p_dollars"), dd, nd])
        rem = bonus - (_cfg(card).get("bonus") or 0)
        if rem:
            out += localize_other("card_extra_chips", [signed(rem)])
    else:
        if k != "m_stone":
            out += chips_line
        cfg = _cfg(card)
        lv = []
        if k == "m_mult":
            lv = [signed(cfg.get("mult"))]
        elif k == "m_glass":
            lv = [cfg.get("Xmult"), *_prob(gs, 1, cfg.get("extra"))]
        elif k == "m_steel":
            lv = [cfg.get("h_x_mult")]
        elif k == "m_stone":
            lv = [signed(bonus) if bonus else signed(cfg.get("bonus"))]
        elif k == "m_gold":
            lv = [signed_dollars(cfg.get("h_dollars"))]
        out += localize_desc("Enhanced", k, lv)
        if k != "m_stone" and bonus:
            out += localize_other("card_extra_chips", [signed(bonus)])
    for key, field, fmt in (("card_extra_mult", "perma_mult", signed), ("card_extra_x_mult", "perma_x_mult", None),
                            ("card_extra_h_chips", "perma_h_chips", signed),
                            ("card_extra_p_dollars", "perma_p_dollars", signed_dollars),
                            ("card_extra_h_dollars", "perma_h_dollars", signed_dollars)):
        v = a.get(field) or 0
        if v:
            out += localize_other(key, [fmt(v) if fmt else v + 1])
    return out


def card_effect(card, gs, area=None):
    """The `value.effect` string BalatroBot reports for this card in this game state."""
    a = _ab(card)
    set_ = a.get("set") or ""
    if card.debuff:
        lines = localize_other("debuffed_playing_card" if card.base is not None else "debuffed_default")
        return " ".join(lines)
    main_start = main_end = None
    if card.base is not None:
        lines = _playing_card_lines(card, gs)
    elif set_ == "Joker":
        lv, main_start, main_end = _joker_vars(card, gs, area == "jokers")
        lines = localize_desc("Joker", card.center_key, lv or [])
    elif set_ == "Tarot":
        lv, main_end = _tarot_vars(card, gs)
        lines = localize_desc("Tarot", card.center_key, lv)
    elif set_ == "Planet":
        lines = localize_desc("Planet", card.center_key, _planet_vars(card, gs))
    elif set_ == "Spectral":
        lv, main_end = _spectral_vars(card, gs)
        lines = localize_desc("Spectral", card.center_key, lv)
    elif set_ == "Voucher":
        lines = localize_desc("Voucher", card.center_key, _voucher_vars(card))
    elif set_ == "Booster":
        key, lv = _booster(card)
        lines = localize_other(key, lv)
    else:
        lines = localize_desc(center(card.center_key).get("set", set_), card.center_key, [])
    lines = (main_start or []) + lines + (main_end or [])
    return " ".join(lines)


# ---------------------------------------------------------------------------
# Blinds and tags (BalatroBot utils/gamestate.lua)
# ---------------------------------------------------------------------------

def blind_effect(key):
    """get_blind_effect_from_ui: the raw (unformatted) description lines of G.localization."""
    if not key or key in ("bl_small", "bl_big"):
        return ""
    e = loc()["descriptions"].get("Blind", {}).get(key)
    if not e or not isinstance(e.get("text"), list):
        return ""
    return " ".join(t for t in e["text"] if t)


def tag_effect(key, gs, tag_cfg=None):
    """get_tag_info: localize raw_descriptions with BalatroBot's per-tag vars."""
    if not key:
        return ""
    from jackdaw.engine.data.prototypes import TAGS
    t = TAGS.get(key)
    name = getattr(t, "name", "")
    cfg = tag_cfg if tag_cfg is not None else (getattr(t, "config", None) or {})
    lv = []
    if name == "Investment Tag":
        lv = [cfg.get("dollars", 0)]
    elif name == "Handy Tag":
        d = cfg.get("dollars_per_hand", 0)
        lv = [d, d * gs.get("hands_played", 0)]
    elif name == "Garbage Tag":
        d = cfg.get("dollars_per_discard", 0)
        lv = [d, d * gs.get("unused_discards", 0)]
    elif name == "Juggle Tag":
        lv = [cfg.get("h_size", 0)]
    elif name == "Top-up Tag":
        lv = [cfg.get("spawn_jokers", 0)]
    elif name == "Skip Tag":
        b = cfg.get("skip_bonus", 0)
        lv = [b, b * (gs.get("skips", 0) + 1)]
    elif name == "Orbital Tag":
        lv = ["Poker Hand", cfg.get("levels", 0)]
    elif name == "Economy Tag":
        lv = [cfg.get("max", 0)]
    lines = desc("Tag", key)
    return " ".join(raw_lines(lines, lv)) if lines else ""
