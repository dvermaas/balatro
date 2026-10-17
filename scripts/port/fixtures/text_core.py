"""Fixture for native/bot/src/simtext.rs (sim_text.py lines 1-348): runs/_port/fixtures/text_core.jsonl.gz.

Rows {"fn", "args", "ret"}. The three localization files are parsed in full (ret = the table); loc() and
game_hands() too. Every descriptions (set, key) goes through _text_parsed / desc / localize_desc (with
several variable lists), every misc / dictionary / v_dictionary / name_text key and every poker hand
through their functions, and a few thousand numbers through number_format, lua_tostring, signed,
signed_dollars, format_ui_value. Parts are written as {"strings": [str | [ref]], "control": {}}.

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/text_core.py
"""

import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import states  # noqa: E402
import sim_text as T  # noqa: E402

rows = []


def add(fn, args, ret):
    rows.append({"fn": fn, "args": args, "ret": ret})


for f in ("en-us.lua", "smods-en-us.lua", "game-hands.lua"):
    add("parse_lua_table_file", [f], T.parse_lua_table((T.LOC_DIR / f).read_text(encoding="utf8")))
add("loc", [], T.loc())
add("game_hands", [], T.game_hands())

SNIPPETS = [
    "return {}", "return {1,2,3}", "return {a=1,b=2.5,c='x',d=\"y\n\t\\z\q\"}", "return {[1]='a',[2]='b'}",
    "return {a=1, 'x', 'y'}", "return {true,false,nil}", "return {a=0x1F,b=-0xff,c=-3,d=1e3,e=1.5E-2,f=2.}",
    "-- c\nreturn { --[[ block ]] a = [[long\nstring]], -- tail\n b = {c={d={}}}; }", "return {['k']=1,[3]=2}",
    "return {x=-1.0e+2, y=-.5e0}".replace("-.5e0", "5e0"), "{a='it''s'}", "return {text={'a','b'},name='n'}",
    "return {[true]=1,[2.5]=3}", "return {a=b}", "return 5", "return 'str'", "return {{1,2},{3,{4}}}",
    "return {\"a\\nb\"}", "return {a=1;b=2;}",
]
for s in SNIPPETS:
    try:
        add("parse_lua_table", [s], T.parse_lua_table(s))
    except Exception:
        pass

LINES = ["", "plain", "{C:red}x{} y", "#1# in #2#", "{C:attention,s:1.2}#1#{} Hands", "{X:mult,C:white}X#1#{} Mult",
         "a #1 b", "##", "{E:1,E:2}t", "{X:red}  a b  {}  c", "{s:1}", "{}", "{:x}", "{a:}", "{ele:1,element:y}",
         "{C:a,C:b}dup", "uni\u00e9 #3#\u2192", "{X:r,C:w}\u00a0\tx y"]
d = T.loc()["descriptions"]
m = T.loc()["misc"]
strs = list(LINES)
for sset, ents in d.items():
    for k, e in ents.items():
        if isinstance(e, dict):
            tx = e.get("text") or []
            if isinstance(tx, str):
                tx = [tx]
            if tx and isinstance(tx[0], list):
                tx = tx[0]
            strs += [t for t in tx if isinstance(t, str)]
strs += [v for v in m.get("v_dictionary", {}).values() if isinstance(v, str)]
strs += [v for v in m.get("dictionary", {}).values() if isinstance(v, str)]
seen = set()
for s in strs:
    if s in seen:
        continue
    seen.add(s)
    try:
        r = T.parse_line(s)
    except ValueError:
        continue
    add("parse_line", [s], None if r is None else [{"strings": p["strings"], "control": p["control"]} for p in r])

VARS = [[], [1, 2, 3, 4, 5], [1.5, 0.1, -2, 100000000, 2.5e9, "x", None, True],
        [0.25, 3.14159, 1234567, 0, -0.5, 12.0, 99999.5, 1e15, 7, 8], [None, False, "s", 0.005, 10.5, 1e11, 123456789012],
        [0.1, 0.2, 1e-5, -1e12, 5e20, 1e100]]
for sset, ents in d.items():
    for k, e in ents.items():
        try:
            tp = T._text_parsed(e) if isinstance(e, dict) else None
        except Exception:
            tp = "ERR"
        if tp == "ERR":
            continue
        if isinstance(e, dict):
            add("_text_parsed", [sset, k], [[{"strings": p["strings"], "control": p["control"]} for p in line] for line in tp])
        dd = T.desc(sset, k)
        add("desc", [sset, k], None if dd is None else [len(line) for line in dd])
        for v in VARS:
            add("localize_desc", [sset, k, v], T.localize_desc(sset, k, v))
            add("raw_lines", [sset, k, v], T.raw_lines(dd, v) if dd else [])
            if sset == "Other":
                add("localize_other", [k, v], T.localize_other(k, v))
        add("name_text", [sset, k], T.name_text(sset, k))
for sset, k in (("Joker", "nope"), ("Nope", "x"), ("Other", "zzz")):
    add("desc", [sset, k], None)
    add("localize_desc", [sset, k, [1]], [])
    add("name_text", [sset, k], "ERROR")
for cat, ent in m.items():
    if isinstance(ent, dict):
        for k in list(ent) + ["__missing__"]:
            add("misc", [cat, k], T.misc(cat, k))
add("misc", ["nocat", "x"], "ERROR")
for k in list(m["dictionary"]) + ["__missing__"]:
    add("dictionary", [k], T.dictionary(k))
for k in list(m["v_dictionary"]) + ["__missing__"]:
    for v in VARS:
        add("variable", [k, v], T.variable(k, v))
for h in list(m["poker_hands"]) + ["Nope"]:
    add("poker_hand", [h], T.poker_hand(h))

rng = random.Random(7)
nums = [0, 1, -1, 5, 9, 10, 99, 100, 999, 1000, 1234, 99999, 100000, 999999, 1000000, 1234567, 12345678, 99999999999,
        100000000000, 123456789012, 10 ** 12, 10 ** 15 - 1, 10 ** 15, 10 ** 15 + 1, 10 ** 17, 0.0, -0.0, 0.5, 1.5, 0.1,
        0.01, 0.005, 0.004, 0.0099, 0.009, 0.995, 9.995, 9.999, 10.05, 10.5, 99.95, 99.99, 99.999, 100.5, 100.4999,
        1e-5, 1e-7, 123.456, 999999.5, 1e11, 9.9995e11, 9.99949e11, 1e12, 1e15, 1e16, 1.5e20, 1e100, 9.9999e99, 1e300,
        2.5, 0.125, 1.005, 2.675, 1e14, 123456789012345.6, 0.3, 0.7, 12.0, 1e22, 1e23, 5e-324]
for _ in range(1500):
    nums.append(rng.choice([rng.randint(-10 ** 6, 10 ** 6), rng.randint(0, 10 ** 13), rng.randint(0, 10 ** 17),
                            round(rng.uniform(-200, 200), rng.choice([1, 2, 3, 6])), rng.uniform(0, 1),
                            rng.uniform(0, 10 ** rng.randint(1, 30)), rng.random() * 10 ** rng.randint(-8, 3),
                            float(rng.randint(0, 10 ** 6)), rng.randint(0, 10 ** 6) / 8]))
nums += [-n for n in nums[:200] if n]
ESW = [None, 1000000, 1000, 1e15, 0, 10 ** 7, 100, 1e13]
for n in nums:
    add("lua_tostring", [n], T.lua_tostring(n))
    add("format_ui_value", [n], T.format_ui_value(n))
    add("signed", [n], T.signed(n))
    add("signed_dollars", [n], T.signed_dollars(n))
    add("number_format", [n, None], T.number_format(n))
    e = rng.choice(ESW)
    add("number_format", [n, e], T.number_format(n, e))
for v in [None, True, False, "abc", "", "1.5"]:
    add("lua_tostring", [v], T.lua_tostring(v))
    add("format_ui_value", [v], T.format_ui_value(v))
    add("number_format", [v, None], T.number_format(v))
add("signed", [None], T.signed(None))
add("signed_dollars", [None], T.signed_dollars(None))

print(len(rows), states.write_fixture("text_core", rows))
