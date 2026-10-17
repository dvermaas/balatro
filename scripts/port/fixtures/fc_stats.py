"""Fixture for native/bot/src/forecast_stats.rs: runs/_port/fixtures/forecast_stats.jsonl.gz.

Rows are {"fn", "args", "ret"}. For every oracle line of rollout.forecast ("rollout") and
rollout_items.forecast ("items") in the traces of both arms, "pub" is the "ok" of the most recent
preceding "call" line (load_trace semantics: "same" repeats the previous result). Rows:
- options: rollout.options(pub) / rollout_items.options(pub) as returned (mode "rollout" | "items").
- filtered: the oracle's args.keys and the recorded ret.options keys (and ret.errors keys).
- option_jokers: rollout.option_jokers(pub, opt) for every rollout option.
- stats: rollout.summarize per option of the recorded _runs, and rollout.paired / paired_delta of
  each option vs skip (ret: {"summarize": {key: ...}, "vs_skip": {key: {next_boss, survive, blinds,
  delta}}}, paired results as [mean, se] unrounded).
- assemble: args {pub, keys, runs, bosses, rollouts}; ret = recorded ret minus seconds and _runs.
- best: rollout_items.best(ret) of every items ret.
- usable: rollout_items.usable(key) for every shop card key seen in the pubs, plus None.

    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/fc_stats.py
"""

import gzip
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # scripts/port: the states module
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)
import rollout  # noqa: E402
import rollout_items  # noqa: E402


def load_trace_py(path):
    out = [json.loads(line) for line in gzip.open(path, "rt", encoding="utf-8") if line.strip()]
    last = None
    for r in out:
        if r.get("k") != "call":
            continue
        if "same" in r:
            r.pop("same")
            r["ok"] = last
        elif "ok" in r:
            last = r["ok"]
        else:
            last = None
    return out


def main():
    rows = []
    usable_keys = set()
    for arm in ("bot-forecast-items", "bot-forecast-long"):
        for path in states.trace_files(arm):
            pub = None
            for r in load_trace_py(path):
                if r.get("k") == "call":
                    pub = r.get("ok")
                    continue
                if r.get("k") != "oracle" or r.get("fn") not in ("rollout.forecast", "rollout_items.forecast"):
                    continue
                mode = "rollout" if r["fn"] == "rollout.forecast" else "items"
                args, ret = r["args"], r["ret"]
                keys = args.get("keys")
                for c in (pub.get("shop") or {}).get("cards") or [] if isinstance(pub, dict) else []:
                    usable_keys.add(c.get("key"))
                mod = rollout if mode == "rollout" else rollout_items
                opts = mod.options(pub)
                rows.append({"fn": "options", "args": {"mode": mode, "pub": pub}, "ret": opts})
                rows.append({"fn": "filtered", "args": {"mode": mode, "pub": pub, "keys": keys},
                             "ret": {"recorded": list(ret["options"].keys()),
                                     "errors": list((ret.get("errors") or {}).keys()),
                                     # options whose rollouts all failed (no runs) are dropped too
                                     "no_runs": [k for k, v in (ret.get("_runs") or {}).items() if not v]}})
                if mode == "items":
                    rows.append({"fn": "best", "args": {"fc": ret}, "ret": rollout_items.best(ret)})
                    continue
                for o in opts:
                    if o["supported"]:
                        rows.append({"fn": "option_jokers", "args": {"pub": pub, "opt": o},
                                     "ret": rollout.option_jokers(pub, o)})
                runs = ret["_runs"]
                bosses = ret["bosses"]
                summ, vs = {}, {}
                for key, rs in runs.items():
                    summ[key] = rollout.summarize(rs, bosses) if rs else None
                if "skip" in runs:
                    base = runs["skip"]
                    for key, rs in runs.items():
                        vs[key] = {
                            "next_boss": list(rollout.paired(rs, base, rollout.m_next_boss)),
                            "survive": list(rollout.paired(rs, base, lambda x: rollout.m_survive(x, bosses))),
                            "blinds": list(rollout.paired(rs, base, rollout.m_blinds)),
                            "delta": list(rollout.paired_delta(rs, base, bosses)),
                        }
                rows.append({"fn": "stats", "args": {"runs": runs, "bosses": bosses},
                             "ret": {"summarize": summ, "vs_skip": vs}})
                rest = {k: v for k, v in ret.items() if k not in ("seconds", "_runs")}
                rows.append({"fn": "assemble", "args": {"pub": pub, "keys": keys, "runs": runs,
                                                        "bosses": bosses, "rollouts": ret["rollouts"]},
                             "ret": rest})
    for key in sorted(usable_keys, key=lambda k: "" if k is None else k) + [None]:
        rows.append({"fn": "usable", "args": {"key": key}, "ret": rollout_items.usable(key)})
    states.write_fixture("forecast_stats", rows)
    print(len(rows), "rows")


if __name__ == "__main__":
    main()
