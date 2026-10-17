"""Fixtures for native/pycompat: CPython reference results for fmt, math, random, statistics, json.

Writes UNCOMPRESSED runs/_port/fixtures/pycompat.jsonl (one row per line):
  {"fn": name, "args": [...], "ret": ...}
Floats are hex bit patterns ("3ff0000000000000") so Rust compares to_bits exactly; ints are JSON
ints (u128 values as decimal strings). Statistics results are ["int", n] or ["float", hex].
"""
import json
import math
import random
import statistics
import struct
import sys
from pathlib import Path

OUT = Path("C:/Users/admin/Documents/Repos/balatro-rust/runs/_port/fixtures/pycompat.jsonl")
rows = []


def hx(x):
    return struct.pack(">d", x).hex()


def unhx(h):
    return struct.unpack(">d", bytes.fromhex(h))[0]


def add(fn, args, ret):
    rows.append({"fn": fn, "args": args, "ret": ret})


R = random.Random(20260101)


def rbits():
    while True:
        x = struct.unpack(">d", R.getrandbits(64).to_bytes(8, "big"))[0]
        if math.isfinite(x):
            return x


def rf(mag=True):
    """A float of a random kind."""
    k = R.randrange(14)
    if k == 0:
        return R.random()
    if k == 1:
        return R.uniform(-1, 1) * 10 ** R.randint(-10, 15)
    if k == 2:
        return rbits() if mag else R.uniform(-1e6, 1e6)
    if k == 3:
        a, b = R.randint(0, 50), R.randint(1, 50)
        return a / b
    if k == 4:
        return round(R.uniform(0, 100000), 2)
    if k == 5:
        return R.randint(-10**6, 10**6) / 8  # exact ties at 0..2 digits
    if k == 6:
        return float(R.randint(-10**9, 10**9))
    if k == 7:
        return R.randint(0, 10**4) / 1000 + 0.0005
    if k == 8:
        return R.randint(-2000, 2000) / 100 + R.choice([0, 0.005, 0.015, 0.025, 0.125])
    if k == 9:
        return R.uniform(-1, 1) * 10 ** R.randint(-320, 300)
    if k == 10:
        return R.choice([0.1, 0.2, 0.3, 2.675, 1.005, 0.125, 0.5, 1.5, 2.5, 3.5, -0.5, -1.5, 1e16, 1e15,
                         123456789012345680.0, 9999999999999998.0, 1e-4, 1e-5, 0.0001234, 5e-324,
                         1.7976931348623157e308, 2.2250738585072014e-308, 0.0, -0.0, 100.0, 1e22, 1e23])
    if k == 11:
        return R.random() * R.choice([1, 10, 100, 1000]) * R.choice([-1, 1])
    if k == 12:
        return R.randint(1, 10**17) * 10.0 ** R.randint(-20, 5)
    return 1 / R.randint(1, 1000)


SPECIAL = [0.0, -0.0, 1.0, -1.0, 0.5, 1.5, 2.5, 3.5, -0.5, -2.5, 0.125, 0.375, 2.675, 1.005, 0.045, 1e16,
           1e-5, 1e-4, 5e-324, 1.7976931348623157e308, 123456789.125, 0.25, 0.35, 8.5, 9.5, 0.15, 0.45,
           1e21, 1e22, 1e23, 99999.995, 0.000123, 12345678901234567890.0, float("inf"), float("-inf"),
           float("nan")]

# ---- fmt
for x in SPECIAL + [rf() for _ in range(30000)]:
    add("pyfloat", [hx(x)], repr(x) if math.isfinite(x) or True else None)
for x in SPECIAL[:-3] + [rf() for _ in range(25000)]:
    p = R.randint(0, 6)
    add("fixed", [hx(x), p], f"{x:.{p}f}")
    if R.random() < 0.3:
        add("signed", [hx(x), p], f"{x:+.{p}f}")
for x in SPECIAL[:-3]:
    for p in range(0, 5):
        add("fixed", [hx(x), p], f"{x:.{p}f}")
        add("signed", [hx(x), p], f"{x:+.{p}f}")
for x in [v for v in SPECIAL if math.isfinite(v)] + [rf() for _ in range(25000)]:
    nd = R.choice([0, 1, 2, 2, 2, 3, 3, 4, 5, 6, -1, -2, 10, 15, 300, 330, -310, -4])
    try:
        r = round(x, nd)
    except Exception:
        continue
    add("pyround", [hx(x), nd], hx(r))
for x in [v for v in SPECIAL if math.isfinite(v)] + [rf() for _ in range(8000)] + \
        [R.randint(-200, 200) / 2 for _ in range(400)] + [R.randint(-1000, 1000) / 10 for _ in range(400)]:
    if abs(x) < 4e18:
        add("pyround_int", [hx(x)], round(x))

# ---- math
for _ in range(4000):
    n = R.randint(0, 60)
    xs = [rf(False) if R.random() < 0.5 else R.choice([1e100, -1e100, 1.0, 0.1, 1e-100, 3.0, -0.0]) for _ in range(n)]
    xs = [v for v in xs if abs(v) < 1e150]
    add("fsum", [[hx(v) for v in xs]], hx(math.fsum(xs)))
for _ in range(500):
    n = R.randint(0, 200)
    xs = [R.uniform(-1, 1) * 10 ** R.randint(-5, 8) for _ in range(n)]
    add("fsum", [[hx(v) for v in xs]], hx(math.fsum(xs)))
for n in range(0, 70):
    for k in range(0, n + 3):
        add("comb", [n, k], str(math.comb(n, k)))
for n, k in [(100, 50), (200, 5), (1000, 8), (52, 8), (52, 26), (10**6, 3)]:
    add("comb", [n, k], str(math.comb(n, k)))

# ---- random
SEEDS = [0, 1, 12012, 13013, 15015, -5, 2**40, 2**32, 2**32 - 1, 2**63 - 1, -(2**62), 7, 123456789,
         "SYNTH", "SYNTH0", "RO3", "", "a", "é€😀"]
for sd in SEEDS:
    g = random.Random(sd)
    add("random", [sd, 2000], [hx(g.random()) for _ in range(2000)])
    g = random.Random(sd)
    ks = [R.randint(0, 64) for _ in range(300)]
    add("getrandbits", [sd, ks], [str(g.getrandbits(k)) for k in ks])
    g = random.Random(sd)
    ns = [R.choice([1, 2, 3, 5, 52, 100, 2**31, 2**32, 2**32 + 1, 2**40 + 7, 10**9, 2**63 - 1, R.randint(1, 10**6)])
          for _ in range(300)]
    add("randbelow", [sd, ns], [str(g._randbelow(n)) for n in ns])
    g = random.Random(sd)
    rr = []
    ranges = []
    for _ in range(100):
        a = R.randint(-1000, 1000)
        b = a + R.randint(1, 5000)
        ranges.append([a, b])
        rr.append(g.randrange(a, b))
    add("randrange", [sd, ranges], rr)
    g = random.Random(sd)
    lens = [R.randint(1, 60) for _ in range(100)]
    add("choice", [sd, lens], [g.choice(range(n)) for n in lens])
    g = random.Random(sd)
    out = []
    ls = [R.randint(0, 60) for _ in range(30)]
    for n in ls:
        x = list(range(n))
        g.shuffle(x)
        out.append(x)
    add("shuffle", [sd, ls], out)
    g = random.Random(sd)
    pairs = []
    out = []
    for _ in range(120):
        n = R.choice([R.randint(1, 60), R.randint(1, 300), R.randint(50, 3000)])
        k = R.choice([0, 1, 2, 5, 6, 7, 8, 10, 13, min(n, 52), R.randint(0, n)])
        k = min(k, n)
        pairs.append([n, k])
        out.append(g.sample(range(n), k))
    add("sample", [sd, pairs], out)
# the bot's exact patterns: Random(12012).sample(range(len(deck)), count) repeatedly (one stream)
for sd, size in [(12012, 8), (13013, 8), (15015, 9), (15015, 10), (12012, 5), (13013, 12)]:
    for deck in (52, 40, 30, 20, 64):
        g = random.Random(sd)
        add("sample_stream", [sd, deck, size, 200], [g.sample(range(deck), size) for _ in range(200)])
# int-seed fuzz
for _ in range(60):
    sd = R.randint(-(2**62), 2**62)
    g = random.Random(sd)
    add("random", [sd, 30], [hx(g.random()) for _ in range(30)])
for _ in range(40):
    sd = "".join(R.choice("abcXYZ019_é") for _ in range(R.randint(0, 20)))
    g = random.Random(sd)
    add("random", [sd, 30], [hx(g.random()) for _ in range(30)])

# ---- statistics
def enc(v):
    return ["int", v] if isinstance(v, int) else ["float", hx(v)]


def fl_list(n, kind):
    if kind == 0:
        return [R.random() * 3 for _ in range(n)]
    if kind == 1:
        return [float(R.randint(0, 1) - R.randint(0, 1)) for _ in range(n)]
    if kind == 2:
        return [R.randint(0, 5000) / R.randint(1, 300) for _ in range(n)]
    if kind == 3:
        return [R.uniform(-1, 1) * 10 ** R.randint(-6, 9) for _ in range(n)]
    if kind == 4:
        return [round(R.uniform(0, 1000), 2) for _ in range(n)]
    return [float(R.randint(0, 10**7)) for _ in range(n)]


def int_list(n, kind):
    if kind == 0:
        return [R.randint(-100, 100) for _ in range(n)]
    if kind == 1:
        return [R.randint(0, 10**12) for _ in range(n)]
    if kind == 2:
        return [R.randint(-(2**62), 2**62) for _ in range(n)]
    return [R.randint(0, 3) for _ in range(n)]


for n in list(range(1, 201)) * 3:
    xs = int_list(n, R.randrange(4))
    add("mean_int", [xs], enc(statistics.mean(xs)))
    add("median_int", [xs], enc(statistics.median(xs)))
    if n >= 2:
        add("stdev_int", [xs], enc(statistics.stdev(xs)))
    fs = fl_list(n, R.randrange(6))
    add("mean_f64", [[hx(v) for v in fs]], enc(statistics.mean(fs)))
    add("median_f64", [[hx(v) for v in fs]], enc(statistics.median(fs)))
    if n >= 2:
        add("stdev_f64", [[hx(v) for v in fs]], enc(statistics.stdev(fs)))
# a few wide-range float lists (short: exact big-int sizes grow with the exponent spread)
for _ in range(40):
    fs = [rf(True) for _ in range(R.randint(2, 6))]
    fs = [v for v in fs if abs(v) < 1e150 and (abs(v) > 1e-150 or v == 0)]
    if len(fs) >= 2:
        add("mean_f64", [[hx(v) for v in fs]], enc(statistics.mean(fs)))
        add("stdev_f64", [[hx(v) for v in fs]], enc(statistics.stdev(fs)))
add("stdev_f64", [[hx(1.0), hx(1.0), hx(1.0)]], enc(statistics.stdev([1.0, 1.0, 1.0])))
add("stdev_int", [[5, 5]], enc(statistics.stdev([5, 5])))
add("mean_f64", [[hx(-0.0)]], enc(statistics.mean([-0.0])))
add("mean_f64", [[hx(5e-324), hx(5e-324)]], enc(statistics.mean([5e-324, 5e-324])))

# ---- json
STR_POOL = ["", "a", "Hello, world", 'quote"back\\slash', "line\nbreak\ttab\r", "\x00\x01\x1f\x7f", "é", "€", "日本語",
            "😀 emoji", "\u2028\u2029", "\x08\x0c", "mixed é😀\n"]


def rv(depth=0):
    k = R.randrange(9 if depth < 3 else 6)
    if k == 0:
        return None
    if k == 1:
        return R.choice([True, False])
    if k == 2:
        return R.randint(-10**6, 10**6) if R.random() < 0.8 else R.randint(-2**62, 2**62)
    if k == 3:
        return rf() if R.random() < 0.9 else R.choice([1.0, 0.1, 1e16, 1e-5, -0.0])
    if k == 4:
        return R.choice(STR_POOL)
    if k == 5:
        return "".join(chr(R.choice([R.randint(32, 126), R.randint(0, 31), R.randint(128, 0xd7ff),
                                     R.randint(0x10000, 0x10ffff)])) for _ in range(R.randint(0, 8)))
    if k in (6, 7):
        return [rv(depth + 1) for _ in range(R.randint(0, 5))]
    return {R.choice(STR_POOL + ["k%d" % i for i in range(5)]): rv(depth + 1) for _ in range(R.randint(0, 5))}


for _ in range(6000):
    v = rv()
    if isinstance(v, float) and not math.isfinite(v):
        continue
    add("json_dumps", [v], json.dumps(v))
add("json_dumps", [[]], "[]")
add("json_dumps", [{}], "{}")

OUT.parent.mkdir(parents=True, exist_ok=True)
with open(OUT, "w", encoding="utf-8", newline="\n") as f:
    for r in rows:
        f.write(json.dumps(r, ensure_ascii=False) + "\n")
print(len(rows), "rows ->", OUT, file=sys.stderr)
