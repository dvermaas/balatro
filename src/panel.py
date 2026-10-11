"""Control plane: run with `.venv/Scripts/python panel.py`, open http://127.0.0.1:8765."""

import json
import threading
import time
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from typesafe_sdk import TypeSafeError

import analytics
import benchmark
from jev_bot import BenchmarkComplete, JevBot, summarize
from recorder import PROMPT_VERSION, prompt_versions

HERE = Path(__file__).parent


class Controller:
    def __init__(self, legacy=None):
        if legacy:
            import importlib
            self.bot = importlib.import_module(f"legacy.{legacy}_bot").make_bot()
        else:
            self.bot = JevBot()
        self.mode = "paused"  # paused | running
        self.pending_steps = 0
        self.delay = 0.3
        self.lock = threading.Lock()
        threading.Thread(target=self._loop, daemon=True).start()

    def _loop(self):
        while True:
            with self.lock:
                go = self.mode == "running" or self.pending_steps > 0
            if not go:
                time.sleep(0.1)
                continue
            try:
                acted = self.bot.step()
            except BenchmarkComplete:
                self.mode, self.pending_steps = "paused", 0
                continue
            except TypeSafeError as e:
                self.bot.emit("error", msg=f"Jev API: {e}")
                self.mode, self.pending_steps = "paused", 0
                continue
            except (TimeoutError, OSError) as e:
                hint = ("Balatro didn't respond for 60s. Usually a popup (e.g. 'Joker unlocked') is open in the "
                        "game: click it away, then press Run." if "timed out" in str(e).lower() or
                        isinstance(e, TimeoutError) else f"Can't reach Balatro ({e}). Is the game running?")
                self.bot.emit("error", msg=hint)
                self.mode, self.pending_steps = "paused", 0
                continue
            except Exception as e:
                self.bot.emit("error", msg=f"{type(e).__name__}: {e}", trace=traceback.format_exc(limit=3))
                self.mode, self.pending_steps = "paused", 0
                continue
            if acted:
                with self.lock:
                    self.pending_steps = max(0, self.pending_steps - 1)
                time.sleep(self.delay)

    def command(self, cmd, body):
        with self.lock:
            if cmd == "run":
                self.mode = "running"
            elif cmd == "pause":
                self.mode, self.pending_steps = "paused", 0
            elif cmd == "step":
                self.mode, self.pending_steps = "paused", self.pending_steps + 1
            elif cmd == "settings":
                self.delay = float(body.get("delay", self.delay))
                self.bot.auto_new_run = bool(body.get("auto_new_run", self.bot.auto_new_run))
            elif cmd == "abandon":
                self.mode, self.pending_steps = "paused", 0
                self.bot.emit("info", msg="Abandoning run (back to menu)")
                self.bot.bench_queue, self.bot.bench_done_pending = [], False
                self.bot.recorder.finish("abandoned")
                self.bot.game.call("menu")
            elif cmd == "benchmark":
                self.bot.start_benchmark(body.get("set"))
                self.mode = "running"
            elif cmd == "cancel_benchmark":
                self.bot.bench_queue, self.bot.bench_done_pending = [], False
                self.bot.emit("info", msg="Benchmark cancelled; the current run will finish normally")
            else:
                raise ValueError(f"unknown command {cmd}")

    def live(self, since):
        gs = self.bot.last_state
        now = time.time()
        events = list(self.bot.events)
        recent = [e for e in events if e["kind"] == "decision" and now - e["t"] <= 60]
        version = self.bot.recorder.version
        label = next((v["label"] for v in prompt_versions() if v["version"] == version), "?")
        on_version = [r for r in analytics.all_runs() if r.get("prompt_version") == version]
        durations = [r["ended"] - r["started"] for r in on_version if r.get("ended")]
        return {
            "mode": self.mode,
            "delay": self.delay,
            "auto_new_run": self.bot.auto_new_run,
            "stats": self.bot.stats,
            "game_state": gs.get("state") if gs else None,
            "game": summarize(gs) if gs else None,
            "current_run": self.bot.recorder.run,
            "version": {"id": version, "label": label, "legacy": version != PROMPT_VERSION, "runs": len(on_version),
                        "finished": sum(r["result"] in ("won", "lost") for r in on_version)},
            "rate": {"decisions_per_min": len(recent),
                     "avg_run_seconds": round(sum(durations) / len(durations)) if durations else None},
            "benchmark": {"name": getattr(self.bot, "bench_name", benchmark.NAME), "current": self.bot.bench_current,
                          "remaining": len(self.bot.bench_queue)},
            "last_id": events[-1]["id"] if events else 0,
            "events": [{k: v for k, v in e.items() if k != "state_sent"} for e in events if e["id"] > since][-150:],
        }


ctl = None


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, code, body, ctype="application/json"):
        data = body if isinstance(body, bytes) else json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        url = urlparse(self.path)
        q = parse_qs(url.query)
        p = url.path
        if p == "/":
            self._send(200, (HERE / "panel.html").read_bytes(), "text/html; charset=utf-8")
        elif p == "/api/live":
            self._send(200, ctl.live(int(q.get("since", ["0"])[0])))
        elif p == "/api/runs":
            labels = {v["version"]: v["label"] for v in prompt_versions()}
            self._send(200, [{**r, "prompt_label": labels.get(r.get("prompt_version"), "?")}
                             for r in analytics.all_runs()])
        elif p.startswith("/api/runs/"):
            run_dir = analytics.RUNS / p[len("/api/runs/"):]
            if run_dir.parent != analytics.RUNS or not (run_dir / "summary.json").exists():
                return self._send(404, {"error": "no such run"})
            summary = json.loads((run_dir / "summary.json").read_text(encoding="utf-8"))
            self._send(200, {"summary": summary, "metrics": analytics.run_metrics(run_dir),
                             "events": analytics.load_decisions(run_dir)})
        elif p == "/api/benchmark":
            self._send(200, analytics.benchmark_matrix(q.get("set", [None])[0]))
        elif p == "/api/prompts":
            self._send(200, analytics.version_stats())
        else:
            self._send(404, {"error": "not found"})

    def do_POST(self):
        if not self.path.startswith("/api/"):
            return self._send(404, {"error": "not found"})
        n = int(self.headers.get("Content-Length") or 0)
        body = json.loads(self.rfile.read(n) or b"{}")
        try:
            ctl.command(self.path[len("/api/"):], body)
            self._send(200, {"ok": True})
        except Exception as e:
            self._send(500, {"error": str(e)})


if __name__ == "__main__":
    import sys
    legacy = sys.argv[sys.argv.index("--legacy") + 1] if "--legacy" in sys.argv else None
    ctl = Controller(legacy)
    srv = ThreadingHTTPServer(("127.0.0.1", 8765), Handler)
    print("JEV Balatro control panel: http://127.0.0.1:8765  (bot starts paused)")
    srv.serve_forever()
