"""Fixture for native/bot/src/recorder.rs: drives the Python RunRecorder (frozen v27 src/recorder.py)
through the events of reference traces with a fixed clock, into temp dirs, and stores every file it
produced (byte for byte; text when UTF-8 and not gzipped, else hex).

Rows: {"fn": "version_hash", ...} and {"fn": "trace", "trace", "init", "ops", "files"}; ops replay
ensure / progress / write_raw / write / finish exactly as jev_bot.py calls them.
Run: C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/recorder.py
"""
import importlib.util
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # scripts/port: states.py
import states  # noqa: E402  (puts the frozen v27 src/ on sys.path)

FIXED_T = 1760000000.25
STAMP = "20260101-120000"
time.time = lambda: FIXED_T  # started / ended / ev["t"] / gzip mtime
time.strftime = lambda fmt, *a: STAMP  # run dir name

spec = importlib.util.spec_from_file_location("recorder_py", states.ROOT / "src" / "recorder.py")
rec = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rec)
rec.snapshot_prompts = lambda: None  # would write runs/_prompts

DECK, STAKE, MODEL, TRACES = "RED", "WHITE", "test-model", 3
VERSION_FILES = ["prompts.py", "hands.py", "strategy.py", "decisions.py", "jokers.py", "odds.py", "build.py",
                 "tarots.py", "readiness.py", "rollout.py", "rollout_items.py", "native.py", "../native/Cargo.toml"]


def replay(trace_path, tmp):
    r = rec.RunRecorder(root=tmp, backend="balatrobot")
    ops, ev_id, last_gs = [], 0, None
    for x in states.read_jsonl(trace_path):
        if x.get("k") == "event":
            ev_id += 1
            ev = {"id": ev_id, "t": FIXED_T, "kind": x["kind"], **x["data"]}
            if x["kind"] == "decision" and last_gs is not None:  # jev_bot.query: raw state before the decision
                ops.append({"op": "write_raw", "id": ev_id, "gs": last_gs})
                r.write_raw(ev_id, last_gs)
            ops.append({"op": "write", "ev": ev})
            r.write(ev)
            if x["kind"] == "run_end":
                res = "won" if x["data"].get("won") else "lost"
                ops.append({"op": "finish", "result": res, "gs": last_gs})
                r.finish(res, last_gs)
        elif x.get("k") == "call" and isinstance(x.get("ok"), dict) and "state" in x["ok"]:
            gs = last_gs = x["ok"]
            ops.append({"op": "ensure", "gs": gs, "deck": DECK, "stake": STAKE, "model": MODEL, "bench": None})
            r.ensure(gs, DECK, STAKE, MODEL, None)
            ops.append({"op": "progress", "gs": gs})
            r.progress(gs)
    files = {}
    for p in sorted(Path(tmp).rglob("*")):
        if not p.is_file():
            continue
        key, b = p.relative_to(tmp).as_posix(), p.read_bytes()
        try:
            files[key] = {"hex": b.hex()} if key.endswith(".gz") else {"text": b.decode("utf-8")}
        except UnicodeDecodeError:
            files[key] = {"hex": b.hex()}
    return ops, files


def main():
    rows = []
    src = states.ROOT / "src"
    rows.append({"fn": "version_hash", "files": VERSION_FILES, "root": str(src),
                 "ret": rec.version_hash(VERSION_FILES, src)})
    traces = sorted((states.DATA / "traces" / "bot-forecast-items").glob("*.jsonl.gz"))[:TRACES]
    for path in traces:
        tmp = tempfile.mkdtemp(prefix="recorder_fixture_")
        ops, files = replay(path, tmp)
        rows.append({"fn": "trace", "trace": path.name.split(".")[0],
                     "init": {"backend": "balatrobot", "version": rec.PROMPT_VERSION,
                              "scorer": rec.native.backend(), "rollouts": rec.native.rollout_backend(),
                              "time": FIXED_T, "stamp": STAMP},
                     "ops": ops, "files": files})
        print(path.name, len(ops), "ops", len(files), "files")
    out = states.write_fixture("recorder", rows)
    print("wrote", out, len(rows), "rows")


if __name__ == "__main__":
    main()
