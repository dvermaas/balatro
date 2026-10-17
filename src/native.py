"""Bridge to the Rust scoring kernel (native/, built with maturin as the `jev_native` module).

v24: hands.evaluated_plays / best_score / best_play run jokers.score over every play subset in Rust
when the module is installed, with results identical to the Python loop (tests/test_native.py checks
every subset). Everything here only encodes inputs; whenever an input is unusual (an unknown rank or
suit, a missing key) the encoder returns None and the caller keeps the Python path, which then
behaves (and fails) exactly as before. JEV_NATIVE=0 turns the kernel off.

Part of the prompt version hash (see recorder.PROMPT_VERSION), like the Rust sources.
"""

import os

try:
    import jev_native as _rs
except ImportError:  # not built: pure Python
    _rs = None

ENABLED = _rs is not None and os.environ.get("JEV_NATIVE") != "0"

# Same order as strategy.HAND_NAMES (not imported: strategy imports hands, which imports this).
HAND_NAMES = ["Flush Five", "Flush House", "Five of a Kind", "Straight Flush", "Four of a Kind", "Full House",
              "Flush", "Straight", "Three of a Kind", "Two Pair", "Pair", "High Card"]
_HAND = {n: i for i, n in enumerate(HAND_NAMES)}
_RANK = {r: i + 2 for i, r in enumerate("23456789TJQKA")}
_SUIT = {None: 0, "H": 1, "D": 2, "C": 3, "S": 4}
# Only these values change a score or a hand type; any other value behaves like "none of these".
_ENH = {None: 0, "STONE": 1, "WILD": 2, "BONUS": 3, "MULT": 4, "GLASS": 5, "STEEL": 6}
_EDITION = {None: 0, "FOIL": 1, "HOLO": 2, "POLYCHROME": 3}
_BOSS = {"The Psychic": 1, "The Eye": 2, "The Mouth": 3}


def backend():
    return "rust" if ENABLED else "python"


def rollout_backend():
    """v24: which engine runs the shop forecast's rollouts (sim_bridge.available, without importing it)."""
    import importlib.util
    return "rumbot" if importlib.util.find_spec("jev_sim") and os.environ.get("JEV_NATIVE") != "0" else "jackdaw"


def _code(table, v, other):
    return table.get(v, other)  # TypeError for an unhashable value: callers fall back to Python


def encode_cards(infos):
    """[(rank, suit, enh, edition, red seal, debuff)] or None when a card can't be encoded exactly."""
    out = []
    try:
        for i in infos:
            r, s = i["rank"], i["suit"]
            rank = 0 if not r else _RANK.get(r) if isinstance(r, str) else None
            suit = _SUIT.get(s) if s is None or isinstance(s, str) else None
            if rank is None or suit is None:
                return None
            out.append((rank, suit, _code(_ENH, i["enh"], 7), _code(_EDITION, i["edition"], 4),
                        i["seal"] == "RED", bool(i["debuff"])))
    except (KeyError, TypeError):
        return None
    return out


def _mask(values, table):
    m = 0
    for v in values:
        m |= 1 << table[v]
    return m


def _pred(pred):
    if pred == "first_face":
        return 3, 0
    if pred == "first":
        return 4, 0
    if pred == "final":
        return 5, 0
    kind = pred[0]
    if kind == "suit":
        return 1, _SUIT[pred[1]]
    if kind == "ranks":
        return 2, _mask(pred[1], _RANK)
    if kind == "card":  # v28: one rank of one suit (The Idol)
        return 6, _RANK[pred[1]] | (_SUIT[pred[2]] << 8)
    return 0, 0  # all


def _eff(e):
    return float(e[0]), float(e[1]), float(e[2])


_COND = {"names": 1, "le3": 2, "final": 3, "no_discards": 4, "held_suits": 5, "four_suits": 6, "played_this_round": 7,
         "seeing_double": 11}


def _hand(spec):
    kind = spec[0]
    if kind == "const":
        return 0, 0, 0.0, _eff(spec[1]), _eff(spec[1])
    if kind == "cond":
        _, cond, arg, yes, no = spec
        mask = _mask(arg, _HAND) if cond == "names" else _mask(arg, _SUIT) if cond == "held_suits" else 0
        return _COND[cond], mask, 0.0, _eff(yes), _eff(no)
    if kind == "times_played":
        return 8, 0, 0.0, (0.0, 0.0, 1.0), (0.0, 0.0, 1.0)
    if kind == "per_discard":
        return 9, 0, float(spec[1]), (0.0, 0.0, 1.0), (0.0, 0.0, 1.0)
    return 10, 0, 0.0, (0.0, 0.0, 1.0), (0.0, 0.0, 1.0)  # lowest_held


def _rule(r):
    if not r:
        return None
    return ([(*_pred(p), float(c), float(m), float(x)) for p, c, m, x in r["card"]],
            [(*_pred(p), int(t)) for p, t in r["retrigger"]],
            [(*_pred(p), float(m), float(x)) for p, m, x in r["held"]],
            int(r.get("held_retrigger", 0)),
            _hand(r["hand"]) if r["hand"] else None)


def scorer(hand_levels, jokers, rnd):
    """A native jokers.Scorer, or None (module missing, or inputs it can't take exactly)."""
    if not ENABLED or _rs is None:
        return None
    try:
        levels = []
        for n in HAND_NAMES:
            h = hand_levels.get(n, {})
            levels.append((float(h.get("chips", 0)), float(h.get("mult", 1)), float(h.get("played", 0)),
                           bool(h.get("played_this_round", 0))))
        js = [(_code(_EDITION, j["edition"], 4), _rule(j["rule"])) for j in jokers]
        hands_left = float(rnd.get("hands_left") or 1)
        discards_left = float(rnd.get("discards_left") or 0)
    except (AttributeError, KeyError, TypeError, ValueError, IndexError):
        return None
    return _rs.Scorer(levels, js, hands_left, discards_left)


def allow_spec(allowed):
    """(boss code, played-hand mask, any hand played) for a decisions.BossAllow, (0, 0, False) for no
    filter, None for a filter the kernel can't evaluate."""
    if allowed is None:
        return 0, 0, False
    spec = getattr(allowed, "native_spec", None)
    return spec() if spec else None


def boss_spec(levels, boss):
    try:
        played = [n for n, h in levels.items() if h.get("played_this_round")]
    except AttributeError:
        return None
    return _BOSS.get(boss, 0), sum(1 << _HAND[n] for n in played if n in _HAND), bool(played)


def _inputs(scorer, infos, allowed):
    get = getattr(scorer, "native", None) if ENABLED else None
    ns = get() if get else None
    if ns is None:
        return None
    cards = encode_cards(infos)
    allow = allow_spec(allowed)
    if cards is None or allow is None:
        return None
    return ns, cards, allow


def evaluate(scorer, infos, allowed):
    """hands.evaluated_plays rows [(score, name, combo, scoring set)], or None for the Python path."""
    inp = _inputs(scorer, infos, allowed)
    if inp is None:
        return None
    return [(int(s), HAND_NAMES[n], tuple(combo), frozenset(sc)) for s, n, combo, sc in _rs.evaluate(*inp)]


def best_score(scorer, infos, allowed):
    inp = _inputs(scorer, infos, allowed)
    return None if inp is None else int(_rs.best_score(*inp))


def best_play(scorer, infos, allowed):
    """max((score, name)) over the allowed plays, default (0, None); None for the Python path."""
    inp = _inputs(scorer, infos, allowed)
    if inp is None:
        return None
    s, n = _rs.best_play(*inp)
    return (int(s), HAND_NAMES[n]) if n >= 0 else (0, None)


def score_many(scorer, infos, plays):
    """[jokers.score(infos, combo, name, scoring)] for plays [(combo, name, scoring)], or None."""
    inp = _inputs(scorer, infos, None)
    if inp is None:
        return None
    try:
        enc = [(list(combo), _HAND[name], list(sc)) for combo, name, sc in plays]
    except KeyError:
        return None
    return [int(v) for v in _rs.score_many(inp[0], inp[1], enc)]
