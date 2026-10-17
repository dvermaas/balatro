"""Fixture for native/bot/tests/fc_apply.rs: runs/_port/fixtures/fc_apply.jsonl.gz.

Rows {"pub", "key", "seed", "desc", "drew", "error"}: for ~400 evenly spaced rollout.forecast oracle lines
(pub = "ok" of the preceding "call" line) and every supported option: the Python snapshot copy gets
rng=PseudoRandom("RO000000"), rollout.apply_option, then json.loads(sim_bridge.describe(gs, rng=True)).
sort_ids are stored relative to the first deck card. "drew" = the option consumed RNG (pools differ from
a fresh PseudoRandom(seed)); "error" = Jackdaw refused the option (then no desc).

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/fc_apply.py
"""
import json
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import states  # noqa: E402
import rollout  # noqa: E402
import sim_bridge  # noqa: E402
from jackdaw.engine.rng import PseudoRandom  # noqa: E402
from fc_stats import load_trace_py  # noqa: E402

CAP = 400
SEED = "RO000000"


def main():
    pubs = []
    for arm in ("bot-forecast-items", "bot-forecast-long"):
        for path in states.trace_files(arm):
            pub = None
            for r in load_trace_py(path):
                if r.get("k") == "call":
                    pub = r.get("ok")
                elif r.get("k") == "oracle" and r.get("fn") == "rollout.forecast":
                    pubs.append(pub)
    n = len(pubs)
    idx = range(n) if n <= CAP else sorted({i * n // CAP for i in range(CAP)})
    rows, kinds = [], Counter()
    fresh = {k: v for k, v in PseudoRandom(SEED).get_state().items() if k not in ("seed", "hashed_seed")}
    for i in idx:
        pub = pubs[i]
        snap = rollout.Snapshot(rollout.build_state(pub))
        base = snap.gs["deck"][0].sort_id
        for o in rollout.options(pub)[0] if isinstance(rollout.options(pub), tuple) else rollout.options(pub):
            if not o.get("supported", True):
                continue
            gs = snap.copy()
            gs["rng"] = PseudoRandom(SEED)
            row = {"pub": pub, "key": o["key"], "seed": SEED, "actions": o["actions"]}
            try:
                rollout.apply_option(gs, o)
            except rollout.IllegalActionError as e:
                row["error"] = str(e)
                rows.append(row)
                kinds[o["key"].split("_")[0] + ":error"] += 1
                continue
            desc = json.loads(sim_bridge.describe(gs, rng=True))
            pools = desc.pop("rng_pools")
            row["drew"] = pools != fresh
            if row["drew"]:
                row["rng_pools"] = pools
            for c in desc["cards"] + desc["jokers"] + desc["consumables"]:
                c["sort_id"] -= base
            row["desc"] = desc
            rows.append(row)
            kinds[o["key"].split("_")[0] + (":drew" if row["drew"] else "")] += 1
    print(n, "oracle calls;", len(rows), "rows;", dict(kinds), "->", states.write_fixture("fc_apply", rows))


if __name__ == "__main__":
    main()
