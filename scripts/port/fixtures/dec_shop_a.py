"""Fixture for native/bot/src/decisions/shop.rs pure helpers (dec-shop-a).

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe scripts/port/fixtures/dec_shop_a.py

Rows: pct, discard_obj, weakest_jokers (recent=[]), economic, forecast_fields and forecast_note on the
SHOP states of bot-forecast-items (each with its recorded rollout.forecast / rollout_items.forecast ret).
Each row's `want` is the Python helper's output. Writes runs/_port/fixtures/dec_shop_a.jsonl.gz.
"""
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import decisions as D  # noqa: E402

ARM = "bot-forecast-items"
MAX_SHOP = 800
rng = random.Random(1)


def shop_oracle(arm):
    """shop index -> {oracle fn: ret} for the oracle lines that follow each SHOP state's call line."""
    out, n = {}, 0
    for p in states.trace_files(arm):
        prev, cur = None, None
        for r in states.read_jsonl(p):
            if r.get("k") == "call" and "ok" in r:
                gs = r["ok"]
                if gs != prev:
                    if gs.get("state") == "SHOP":
                        cur = n
                        n += 1
                    else:
                        cur = None
                prev = gs
            elif r.get("k") == "oracle" and cur is not None and r.get("fn") in ("rollout.forecast", "rollout_items.forecast"):
                ret = dict(r["ret"])
                ret.pop("_runs", None)
                out.setdefault(cur, {}).setdefault(r["fn"], ret)
    return out


shop = states.states(ARM, state="SHOP")[:MAX_SHOP]
oracle = shop_oracle(ARM)
rows = []

# pct: oracle probabilities plus edge values
ps = [0.0, 0.005, 0.0049, 0.125, 0.5, 2 / 3, 0.875, 0.995, 1.0]
for ret in oracle.values():
    for fc in ret.values():
        for o in fc["options"].values():
            ps.append(o["p_survive"])
            ps.append(o["p_next_boss"])
for p in ps[:400]:
    rows.append({"fn": "pct", "p": p, "want": D._pct(p)})

# discard_obj: synthetic discard plans with and without a modelled outcome
for k in range(60):
    d = {"label": f"discard {k}", "tossed": [1, 3], "fits_build": bool(k % 2)}
    if k % 3:
        d.update(expected_score=round(rng.uniform(50, 900), 1), low_score=rng.randint(10, 300),
                 gain_percent=rng.choice([12.5, -3.0, 0.0, 7, -41.25, round(rng.uniform(-60, 80), 2)]),
                 chance_to_clear=rng.random(), chance_to_meet_pace=rng.random(),
                 drawn=[4, 9], samples=rng.choice([12, 252, 1000]), exact_draws=bool(k % 2))
    rows.append({"fn": "discard_obj", "d": d, "want": D._discard_obj(d)})

# weakest_jokers, economic on the shop's cards
for i, gs in enumerate(shop):
    if gs.get("jokers"):
        n = 2
        rows.append({"fn": "weakest_jokers", "i": i, "n": n, "recent": [],
                     "want": sorted(D._weakest_jokers(gs, [], n))})
    cards = []
    for a in ("shop", "packs", "vouchers", "consumables", "jokers"):
        cards += (gs.get(a) or {}).get("cards", []) if isinstance(gs.get(a), dict) else []
    for c in cards[:6]:
        obj = {"key": c.get("key"), "label": c.get("label")}
        if c.get("set") in ("TAROT", "SPECTRAL") and rng.random() < 0.5:
            obj["cash_outcome"] = {"net_cash_gain": rng.choice([0, 3, 7])}
        rows.append({"fn": "economic", "obj": obj, "c": c, "want": D._economic(obj, c)})

# forecast_fields / forecast_note on the recorded oracle returns
for i in sorted(oracle):
    if i >= len(shop):
        continue
    for fn, fc in oracle[i].items():
        keys = list(fc["options"])
        rows.append({"fn": "forecast_fields", "i": i, "oracle": fn, "fc": fc,
                     "want": {k: D._forecast_fields(fc, k) for k in keys}})
        rows.append({"fn": "forecast_note", "i": i, "oracle": fn, "fc": fc,
                     "want": D._forecast_note(shop[i], fc)})

path = states.write_fixture("dec_shop_a", rows)
from collections import Counter  # noqa: E402
print(path, len(rows), dict(Counter(r["fn"] for r in rows)))
