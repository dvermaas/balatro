"""Inputs for the Rust port's unit fixtures: the gamestates in the reference traces.

    import sys; sys.path.insert(0, "scripts/port"); import states
    for gs in states.states("bot-forecast-items", state="SHOP"): ...
    states.write_fixture("hands", rows)   # -> runs/_port/fixtures/hands.jsonl.gz

The order matches `jevbot::trace::states` in Rust, so fixture row i can carry its input itself or
just its index. Import this module first: it puts the frozen v27 `src/` of the main port worktree
on sys.path. Run fixture scripts with that worktree's interpreter, from any directory:
    C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <your worktree>/scripts/port/fixtures/<name>.py
"""

import gzip
import json
import sys
from pathlib import Path

# Always the frozen v27 sources of the main port worktree (it has the Jackdaw submodule and the
# localization in runs/_loc), wherever the fixture script itself lives.
ROOT = Path("C:/Users/admin/Documents/Repos/balatro-rust")
DATA = ROOT / "runs" / "_port"
sys.path[:0] = [str(ROOT / "src"), str(ROOT / "scripts")]


def trace_files(arm):
    return sorted((DATA / "traces" / arm).glob("*.jsonl.gz"))


def read_jsonl(path):
    op = gzip.open if str(path).endswith(".gz") else open
    with op(path, "rt", encoding="utf-8") as f:
        return [json.loads(l) for l in f if l.strip()]


def states(arm="bot-forecast-items", state=None):
    """Distinct consecutive call results, in trace order (optionally only one game state)."""
    out = []
    for p in trace_files(arm):
        prev = None
        for r in read_jsonl(p):
            if r.get("k") != "call" or "ok" not in r:
                continue
            gs = r["ok"]
            if gs != prev and (state is None or gs.get("state") == state):
                out.append(gs)
            prev = gs
    return out


def write_fixture(name, rows):
    """rows: JSON-able dicts. Written gzipped, one per line."""
    path = DATA / "fixtures" / f"{name}.jsonl.gz"
    path.parent.mkdir(parents=True, exist_ok=True)
    with gzip.open(path, "wt", encoding="utf-8") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    return path
