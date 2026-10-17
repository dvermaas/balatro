"""Fixture for native/bot/tests/recorder.rs (list_runs): the Python RunRecorder.list_runs (frozen v27
src/recorder.py) on a temp runs root with 3 valid run dirs, 1 with a broken summary.json and 1 without a
summary.json. Writes runs/_port/fixtures/recorder_list.json of the main port worktree:
{"dirs": {name: summary text or null}, "all": <json>, "two": <json>, "unlimited": <json>}, where each json is
the compact json.dumps of list_runs(limit) (50, 2, 0) with key order kept. The Rust test writes the same dirs.
Run: C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/recorder_list.py
"""
import json
import shutil
import sys
import tempfile
import types
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))  # scripts/port: states.py
import states  # noqa: E402,F401  (puts the frozen v27 src/ on sys.path)
import recorder  # noqa: E402

OUT = Path("C:/Users/admin/Documents/Repos/balatro-rust/runs/_port/fixtures/recorder_list.json")


def summary(name, prompt_version, result, started):
    return json.dumps({
        "run_id": name, "seed": "SEED" + name[-3:], "deck": "Red Deck", "stake": "White Stake", "model": "m",
        "prompt_version": prompt_version, "backend": "rumbot", "scorer": "rust", "rollouts": "rust",
        "benchmark": None, "started": started, "ended": started + 100.25, "result": result,
        "ante": 3, "round": 7, "money": 12, "decisions": 4, "forced": 1, "errors": 0,
    }, indent=2)


dirs = {
    "20260101-120000-111": summary("20260101-120000-111", "v-old", "lost", 1767268800.5),
    "20260101-120000-222": summary("20260101-120000-222", None, "won", 1767268800.5),
    "20260101-150000-333": summary("20260101-150000-333", "v-new", "in_progress", 1767279000.0),
    "20260101-140000-555": '{"run_id": "20260101-140000-555", "seed": ',  # broken: skipped
    "20260101-160000-999": None,  # no summary.json: not listed
}

root = Path(tempfile.mkdtemp(prefix="listruns-"))
try:
    for name, text in dirs.items():
        (root / name).mkdir()
        if text is not None:
            (root / name / "summary.json").write_text(text, encoding="utf-8")
    fake = types.SimpleNamespace(root=root)  # list_runs only reads self.root
    out = {"dirs": dirs}
    for key, limit in (("all", 50), ("two", 2), ("unlimited", 0)):
        out[key] = json.dumps(recorder.RunRecorder.list_runs(fake, limit=limit), separators=(",", ":"), ensure_ascii=False)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, indent=1, ensure_ascii=False), encoding="utf-8")
    print({k: len(v) for k, v in out.items() if k != "dirs"}, "->", OUT)
finally:
    shutil.rmtree(root, ignore_errors=True)
