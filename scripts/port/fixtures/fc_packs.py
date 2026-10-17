"""Fixture for native/bot/tests/fc_packs.rs: runs/_port/fixtures/fc_packs.jsonl.gz.

Rows {"pub","key","k","seed","pack","taken","lite","error"}: for ~300 evenly spaced rollout_items.forecast oracle
lines (pub = "ok" of the preceding "call" line), each buy_pack/buy_card option and k in 0..3: Python snapshot copy,
rng=PseudoRandom("RO00000k"), rollout_items.apply_option (OpenBooster, open_and_pick). "pack" = the opened pack's
cards (key, edition, enhancement, seal, rank, suit), "taken" = keys picked, "lite" = the comparable part of
sim_bridge.describe(gs, rng=True).

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/fc_packs.py
"""
import json
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import states  # noqa: E402
import rollout  # noqa: E402
import rollout_items as ri  # noqa: E402
import sim_bridge  # noqa: E402
from jackdaw.engine.rng import PseudoRandom  # noqa: E402
from jackdaw.engine.actions import OpenBooster  # noqa: E402
from jackdaw.engine.game import step  # noqa: E402
from fc_stats import load_trace_py  # noqa: E402

CAP = 300
KS = range(4)


def sig(c):
    m, v = c.get("modifier") or {}, c.get("value") or {}
    if not isinstance(m, dict):
        m = {}
    if not isinstance(v, dict):
        v = {}
    return [c.get("key"), m.get("edition"), m.get("enhancement"), m.get("seal"), v.get("rank"), v.get("suit")]


def lite(gs):
    d = json.loads(sim_bridge.describe(gs, rng=True))
    return {"money": d["money"], "jokers": [j["name"] for j in d["jokers"]],
            "consumables": [c["name"] for c in d["consumables"]], "n_cards": len(d["cards"]),
            "hand_size": d["hand_size"], "joker_slots": d["joker_slots"], "rng_pools": d["rng_pools"]}


def main():
    pubs = []
    for arm in ("bot-forecast-items", "bot-forecast-long"):
        for path in states.trace_files(arm):
            pub = None
            for r in load_trace_py(path):
                if r.get("k") == "call":
                    pub = r.get("ok")
                elif r.get("k") == "oracle" and r.get("fn") == "rollout_items.forecast":
                    pubs.append(pub)
    n = len(pubs)
    idx = range(n) if n <= CAP else sorted({i * n // CAP for i in range(CAP)})
    rows, kinds = [], Counter()
    for i in idx:
        pub = pubs[i]
        snap = rollout.Snapshot(ri.build_state(pub))
        picker = ri.Picker(pub)
        for o in ri.options(pub):
            if o["key"] == "skip":
                continue
            for k in KS:
                gs = snap.copy()
                gs["rng"] = PseudoRandom(rollout._seed(k))
                row = {"pub": pub, "key": o["key"], "k": k, "seed": rollout._seed(k), "pack": [], "taken": []}
                try:
                    for kind, j in o["actions"]:
                        if kind == "pack":
                            step(gs, OpenBooster(card_index=j))
                            row["pack"] = [sig(c) for c in ri._area_cards(gs, "pack_cards")]
                            row["taken"] = ri.open_and_pick(gs, picker)
                        else:
                            ri.apply_option(gs, {"actions": [(kind, j)]}, picker)
                    row["lite"] = lite(gs)
                except ri.IllegalActionError as e:
                    row["error"] = str(e)
                rows.append(row)
                kinds[o["key"].split("_")[1] + (":error" if "error" in row else "")] += 1
    print(n, "oracle calls;", len(rows), "rows;", dict(kinds), "->", states.write_fixture("fc_packs", rows))


if __name__ == "__main__":
    main()
