"""Audit recorded no-discard states without JEV, game actions, or draw-order knowledge."""
import argparse
import json
import statistics
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
import decisions
from replay import recorded


class Captured(Exception):
    pass


class Bot:
    def __init__(self):
        self.questions = {}

    def query(self, gs, state, questions):
        self.questions = questions
        raise Captured

    def emit(self, *args, **kwargs):
        pass

    def act(self, *args, **kwargs):
        pass


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--version", default="503b35ff")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    times, reports = [], []
    eligible = 0
    for path in sorted((ROOT / "runs").glob("*/summary.json")):
        summary = json.loads(path.read_text(encoding="utf-8"))
        if summary.get("prompt_version") != args.version or summary.get("result") not in ("won", "lost"):
            continue
        for event, gs in recorded(path.parent, "move", 1):
            rnd = gs.get("round") or {}
            if rnd.get("discards_left") != 0 or (rnd.get("hands_left") or 0) < 2:
                continue
            eligible += 1
            original = json.dumps(gs, sort_keys=True)
            bot = Bot()
            start = time.perf_counter()
            try:
                decisions.selecting_hand(bot, gs)
            except Captured:
                pass
            times.append(time.perf_counter() - start)
            assert json.dumps(gs, sort_keys=True) == original, "mutated recorded state"
            options = bot.questions.get("best_play")
            if options is None:
                continue
            setup = {k: v for k, v in options.criteria.items() if v.get("setup")}
            if not setup:
                continue
            best = max(options.criteria, key=lambda k: options.criteria[k]["next_hand_outlook"]["average_two_play_total"])
            reports.append({"run": path.parent.name, "event": event["id"],
                            "new_setup_options": len(setup), "best_two_play_option": best,
                            "best": options.criteria[best], "strongest_now": options.criteria["play_1"]})
    result = {"recorded_version": args.version, "eligible_no_discard_states": eligible,
              "states_with_setup_choices": len(reports),
              "seconds_median": round(statistics.median(times), 3) if times else None,
              "seconds_max": round(max(times), 3) if times else None,
              "note": "Candidate coverage and model estimates, not counterfactual wins. No JEV or game calls.",
              "reports": reports}
    Path(args.out).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in result.items() if k != "reports"}))


if __name__ == "__main__":
    main()
