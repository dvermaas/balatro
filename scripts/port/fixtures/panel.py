"""Fixture for native/bot/tests/panel.rs: the Python panel.Handler (real HTTP server on an ephemeral
port) answering GET /api/runs and /api/runs/<id> on a COPY of 3 recorded run dirs (read only on the
source). Writes runs/_port/fixtures/panel.json of the main port worktree:
{"dirs": [names], "runs": <json>, "run": {name: <json>}, "missing": [code, json]}.
The Rust test copies the same dirs (the 3 smallest-by-size with decisions) into a temp root.
Run: C:/Users/admin/Documents/Repos/balatro-rust/.venv/Scripts/python.exe <worktree>/scripts/port/fixtures/panel.py
"""
import json
import shutil
import sys
import tempfile
import threading
import urllib.error
import urllib.request
from http.server import ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, "C:/Users/admin/Documents/Repos/balatro-rust/scripts/port")
import states  # noqa: E402,F401  (puts the frozen v27 src/ on sys.path)
import analytics  # noqa: E402
import recorder  # noqa: E402
import panel  # noqa: E402

MAIN = Path("C:/Users/admin/Documents/Repos/balatro/runs")
OUT = Path("C:/Users/admin/Documents/Repos/balatro-rust/runs/_port/fixtures/panel.json")

cands = [p.parent for p in sorted(MAIN.glob("*/summary.json")) if (p.parent / "decisions.jsonl").exists()]
cands.sort(key=lambda d: sum(f.stat().st_size for f in d.iterdir()))
names = sorted(d.name for d in cands[:3])

tmp = Path(tempfile.mkdtemp())
for n in names:
    shutil.copytree(MAIN / n, tmp / n)
analytics.RUNS = tmp
recorder.PROMPTS_DIR = tmp / "_prompts"


class Stub:
    backend = "balatrobot"


panel.ctl = Stub()
srv = ThreadingHTTPServer(("127.0.0.1", 0), panel.Handler)
threading.Thread(target=srv.serve_forever, daemon=True).start()
base = f"http://127.0.0.1:{srv.server_address[1]}"


def get(path):
    try:
        with urllib.request.urlopen(base + path) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        return [e.code, json.loads(e.read())]


out = {"dirs": names, "runs": get("/api/runs"), "run": {n: get("/api/runs/" + n) for n in names},
       "missing": get("/api/runs/nope")}
OUT.write_text(json.dumps(out), encoding="utf-8")
shutil.rmtree(tmp, ignore_errors=True)
print("wrote", OUT, names)
