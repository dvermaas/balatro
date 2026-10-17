"""Fixture for native/bot/src/forecast_state.rs: runs/_port/fixtures/fc_state.jsonl.gz.

Rows {"pub", "desc"}: for forecast oracle lines (rollout.forecast, rollout_items.forecast) in the traces of
both arms, "pub" is the "ok" of the most recent preceding "call" line (same semantics as fc_stats.py), and
"desc" = json.loads(sim_bridge.describe(rollout.build_state(pub))) (rng=False) minus "jokers" (task 5).
At most ~1500 calls, evenly spaced over all oracle lines.

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/fc_state.py
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import states  # noqa: E402
import rollout  # noqa: E402
import sim_bridge  # noqa: E402
from fc_stats import load_trace_py  # noqa: E402

CAP = 1500


def main():
    pubs = []
    for arm in ("bot-forecast-items", "bot-forecast-long"):
        for path in states.trace_files(arm):
            pub = None
            for r in load_trace_py(path):
                if r.get("k") == "call":
                    pub = r.get("ok")
                elif r.get("k") == "oracle" and r.get("fn") in ("rollout.forecast", "rollout_items.forecast"):
                    pubs.append(pub)
    n = len(pubs)
    idx = range(n) if n <= CAP else sorted({i * n // CAP for i in range(CAP)})
    rows = []
    for i in idx:
        pub = pubs[i]
        desc = json.loads(sim_bridge.describe(rollout.build_state(pub)))
        desc.pop("jokers")
        # sort_id comes from Jackdaw's process-wide counter: store it relative to the first deck card
        # (deck cards are created first, then jokers, then consumables)
        base = desc["cards"][0]["sort_id"] if desc["cards"] else None
        for c in desc["cards"] + desc["consumables"]:
            c["sort_id"] -= base
        rows.append({"pub": pub, "desc": desc})
    print(n, "oracle calls;", len(rows), "rows ->", states.write_fixture("fc_state", rows))


if __name__ == "__main__":
    main()
