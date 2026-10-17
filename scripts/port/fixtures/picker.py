"""Fixture for native/bot/src/picker.rs: runs/_port/fixtures/picker.jsonl.gz. Rows {"fn","arm","i",...,"ret"}.
Gamestates are read by (arm, i) from trace::states (all states; i indexes states.states(arm)).
  synth_plays(pub) and pick_pub(pub) for every SMODS_BOOSTER_OPENED state and up to CAP evenly spaced SHOP
  states with packs; values(pack, hand, money, jokers, joker_limit) on the same states (SHOP: pack = the
  shop cards; also a variant whose hand is 8 draw-pile cards, so tarot plans run).

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/picker.py
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import states  # noqa: E402
import rollout_items as ri  # noqa: E402
from hands import _d  # noqa: E402

CAP = 120
rows, counts = [], {}
for arm in ("bot-forecast-items", "bot-forecast-long"):
    allst = states.states(arm)
    idx = [i for i, g in enumerate(allst) if g.get("state") == "SMODS_BOOSTER_OPENED"]
    shop = [i for i, g in enumerate(allst) if g.get("state") == "SHOP" and _d(g.get("packs")).get("cards")]
    step = max(1, len(shop) // CAP)
    idx += shop[::step]
    for i in idx:
        g = allst[i]
        p = ri.Picker(g)
        add = lambda r: (rows.append(r), counts.__setitem__(r["fn"], counts.get(r["fn"], 0) + 1))
        add({"fn": "synth_plays", "arm": arm, "i": i, "ret": ri.synth_plays(g)})
        if g["state"] == "SMODS_BOOSTER_OPENED":
            add({"fn": "pick_pub", "arm": arm, "i": i, "ret": p.pick_pub(g)})
            pack = ri._cards(g, "pack")
            hands = [ri._cards(g, "hand")]
        else:
            pack = ri._cards(g, "shop")
            hands = [[], list(ri._cards(g, "cards"))[-8:]]
        jl = _d(g.get("jokers"))
        for h in hands:
            a = dict(pack=pack, hand=h, money=g.get("money") or 0, jokers=ri._cards(g, "jokers"), joker_limit=jl.get("limit") or 5)
            add({"fn": "values", "arm": arm, "i": i, **a, "ret": p.values(**a)})
states.write_fixture("picker", rows)
print(counts)
