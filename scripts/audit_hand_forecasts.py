"""Replay saved hand states through v12 without calling JEV or the game.

Usage: .venv/Scripts/python scripts/audit_hand_forecasts.py --limit 60
This measures candidate coverage and runtime, NOT the counterfactual run win rate.
"""
import argparse
import gzip
import json
import statistics
import sys
import time
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
import decisions


class OfflineBot:
    def __init__(self):
        self.sent = None

    def query(self, gs, state, questions):
        self.sent = state
        return {k: SimpleNamespace(noul=.5, probabilities={next(iter(q.criteria)): 1})
                for k, q in questions.items()}, {}

    def emit(self, *args, **kwargs):
        pass

    def record_decision(self, *args, **kwargs):
        pass

    def act(self, *args):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", default="8cfd03bf")
    parser.add_argument("--limit", type=int, default=60)
    args = parser.parse_args()
    states = []
    for p in sorted((ROOT / "runs").glob("*/summary.json")):
        summary = json.loads(p.read_text(encoding="utf-8"))
        if summary.get("prompt_version") != args.version or summary["result"] not in ("won", "lost"):
            continue
        raw = p.parent / "states.jsonl.gz"
        if not raw.exists():
            continue
        events = {e["id"]: e for e in map(json.loads, (p.parent / "decisions.jsonl").read_text(encoding="utf-8").splitlines())}
        with gzip.open(raw, "rt", encoding="utf-8") as f:
            for entry in map(json.loads, f):
                if entry["gs"]["state"] == "SELECTING_HAND":
                    states.append((entry["gs"], events[entry["id"]], p.parent.name))
    if args.limit and len(states) > args.limit:
        states = [states[i * len(states) // args.limit] for i in range(args.limit)]
    times, gains, changes = [], [], []
    for gs, event, run in states:
        before = json.dumps(gs, sort_keys=True)
        bot = OfflineBot()
        start = time.perf_counter()
        decisions.selecting_hand(bot, gs)
        times.append(time.perf_counter() - start)
        if json.dumps(gs, sort_keys=True) != before:
            raise AssertionError("forecast mutated a recorded state")
        if bot.sent:
            old = event["state_sent"]["strongest_play"]["estimated_score"]
            new = bot.sent["strongest_play"]["estimated_score"]
            gains.append(new - old)
            forecast = bot.sent.get("best_draw_by_measurement")
            if event.get("choice", "").startswith("discard") and forecast:
                original = (event.get("questions") or {}).get("best_discard", {}).get("criteria", {}).get(event["choice"], {})
                if original.get("throws_away") and set(original["throws_away"]) != set(forecast["throws_away"]):
                    changes.append({"run": run, "event": event["id"], "v11_plan": original.get("plan"),
                                    "measured_plan": forecast["plan"], "forecast": forecast})
    report = {"version_replayed": args.version, "states": len(states),
              "seconds_median": round(statistics.median(times), 3) if times else None,
              "seconds_p95": round(sorted(times)[int(.95 * (len(times) - 1))], 3) if times else None,
              "candidate_scores_improved": sum(g > 0 for g in gains),
              "candidate_scores_reduced": sum(g < 0 for g in gains),
              "different_measured_draw_plan": len(changes), "examples": changes[:5]}
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
