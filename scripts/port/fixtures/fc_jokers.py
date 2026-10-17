"""Fixture for native/bot/src/forecast_jokers.rs: runs/_port/fixtures/fc_jokers.jsonl.gz.

Rows are {"pub", "ret", "deck"}: pub = {"jokers", "hands"} reduced from a forecast pub (hands only keeps
the played total), ret = sim_bridge.describe(rollout.build_state(full pub))["jokers"], deck = the
number of deck cards built before the jokers (for the sort_id offset). Pubs are those of every
forecast oracle line (rollout.forecast / rollout_items.forecast) of both arms; identical
(joker cards, hands played) inputs are kept once.

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/fc_jokers.py
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import states  # noqa: E402
import rollout  # noqa: E402
import sim_bridge  # noqa: E402
from fc_stats import load_trace_py  # noqa: E402


def main():
    rows, seen = [], set()
    for arm in ("bot-forecast-items", "bot-forecast-long"):
        for path in states.trace_files(arm):
            pub = None
            for r in load_trace_py(path):
                if r.get("k") == "call":
                    pub = r.get("ok")
                    continue
                if r.get("k") != "oracle" or r.get("fn") not in ("rollout.forecast", "rollout_items.forecast"):
                    continue
                if not isinstance(pub, dict):
                    continue
                jk = (pub.get("jokers") or {}).get("cards") or []
                hp = sum((rollout._d(h).get("played") or 0) for h in rollout._d(pub.get("hands")).values())
                sig = json.dumps([jk, hp], sort_keys=True)
                if sig in seen or not jk:
                    continue
                seen.add(sig)
                gs = rollout.build_state(pub)
                rows.append({"pub": {"jokers": pub["jokers"], "hands": {"all": {"played": hp}}},
                             "ret": json.loads(sim_bridge.describe(gs))["jokers"], "deck": len(gs["deck"])})
    states.write_fixture("fc_jokers", rows)
    print(len(rows), "rows", sorted({j["name"] for r in rows for j in r["ret"]}).__len__(), "joker names")
    print([(r["deck"], r["ret"][0]["sort_id"]) for r in rows[:6]])


if __name__ == "__main__":
    main()
