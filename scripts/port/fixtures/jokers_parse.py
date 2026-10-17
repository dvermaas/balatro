"""Unit fixture for native/bot/src/jokers.rs, parse side: runs/_port/fixtures/jokers_parse.jsonl.gz.

Rows {"fn", "args", "ret"} for parse(effect), entry(card) and build(gs) over every distinct joker card
(key + effect text) and every distinct joker line-up in the states of both arms. "gs" is the index into
ALL = states of bot-forecast-items then bot-forecast-long (the order of jevbot::trace::states).

Encoding of rules (the same shape as jokers::Rule::to_json): dict tuples become lists, sets sorted lists
(str-sorted, None -> "None"), every number inside an effect tuple is a float (Python mixes ints and
floats there; they are only ever added/multiplied into float totals), retrigger times and
held_retrigger are ints. There are no lambdas in parse() output (v24 uses data tuples), so no
callable descriptor is needed.

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/jokers_parse.py
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import jokers  # noqa: E402

ALL = states.states("bot-forecast-items") + states.states("bot-forecast-long")


def f3(e):
    return [float(v) for v in e]


def pred(p):
    if isinstance(p, str):
        return p
    if p[0] == "suit":
        return ["suit", p[1]]
    if p[0] == "ranks":
        return ["ranks", sorted(p[1])]
    assert p == ("all",), p
    return ["all"]


def spec(s):
    if s is None:
        return None
    k = s[0]
    if k == "const":
        return ["const", f3(s[1])]
    if k == "cond":
        _, cond, arg, yes, no = s
        return ["cond", cond, None if arg is None else sorted(str(x) for x in arg), f3(yes), f3(no)]
    if k == "per_discard":
        return ["per_discard", float(s[1])]
    return [k]


def rule(r):
    if r is None:
        return None
    assert not any(callable(x) for p in r["card"] + r["retrigger"] + r["held"] for x in p)
    return {"card": [[pred(p), float(c), float(m), float(x)] for p, c, m, x in r["card"]],
            "retrigger": [[pred(p), t] for p, t in r["retrigger"]],
            "held": [[pred(p), float(m), float(x)] for p, m, x in r["held"]],
            "held_retrigger": r.get("held_retrigger"),
            "hand": spec(r["hand"])}


def ent(e):
    return {"label": e["label"], "rule": rule(e["rule"]), "edition": e["edition"], "scoring": e["scoring"]}


rows = []
seen_cards, seen_effects, seen_lineups = set(), set(), set()
for n, gs in enumerate(ALL):
    cards = (gs.get("jokers") or {}).get("cards") or []
    if not cards:
        continue
    for c in cards:
        eff = (c.get("value") or {}).get("effect")
        if eff not in seen_effects:
            seen_effects.add(eff)
            rows.append({"fn": "parse", "args": {"effect": eff}, "ret": rule(jokers.parse(eff))})
        if (c.get("key"), eff) not in seen_cards:
            seen_cards.add((c.get("key"), eff))
            rows.append({"fn": "entry", "args": {"card": c}, "ret": ent(jokers.entry(c))})
    sig = json.dumps(cards, sort_keys=True)
    if sig not in seen_lineups:
        seen_lineups.add(sig)
        rows.append({"fn": "build", "args": {"gs": n}, "ret": [ent(e) for e in jokers.build(gs)]})
for eff in ("", None):
    rows.append({"fn": "parse", "args": {"effect": eff}, "ret": rule(jokers.parse(eff))})
print(len(rows), "rows")
states.write_fixture("jokers_parse", rows)
