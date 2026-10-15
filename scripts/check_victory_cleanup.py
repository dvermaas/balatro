"""Replay a recorded victory only after the benchmark stops; never call JEV.

This is a live integration check, separate from benchmark results. It loads the
preserved game-17 checkpoint, repeats the already recorded actions, and refuses
any divergence in public outcome facts. It then exercises JevBot.on_round_eval
against the actual victory screen and returns the game to MENU.
"""
import argparse
import copy
import json
import sys
import time
import urllib.request
from collections import deque
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
from balatro import Balatro
from jev_bot import JevBot, current_blind
from recorder import PROMPT_VERSION

RUN = ROOT / "runs/20261013-072352-X62IZYJ4"
SAVE = ROOT / "runs/_checkpoints/20261013-072352-X62IZYJ4-autosave.jkr"
OUTPUT = ROOT / "docs/samples/v15-victory-cleanup-check.json"


class MemoryRecorder:
    def __init__(self):
        self.finished, self.raw = [], []

    def write(self, event):
        pass

    def write_raw(self, event_id, gs):
        self.raw.append({"id": event_id, **{k: copy.deepcopy(gs.get(k)) for k in
                         ("seed", "state", "won", "ante_num", "round_num", "round")}})

    def finish(self, result, gs):
        self.finished.append(result)


def facts(gs):
    return {k: v for k, v in JevBot._snapshot(gs, {}).items()
            if k in ("state", "ante", "round", "money", "chips", "hands_left", "discards_left", "jokers")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", action="store_true", help="Execute the live replay; default is read-only")
    parser.add_argument("--controlled-fixture", action="store_true",
                        help="Synthetic UI test: promote the loaded boss to ante 8 and supply its target chips; never a benchmark win")
    args = parser.parse_args()
    events = [json.loads(l) for l in (RUN / "decisions.jsonl").read_text(encoding="utf-8").splitlines()]
    by_id = {e["id"]: e for e in events}
    actions = [e for e in events if e["kind"] == "action" and 7895 < e["id"] < 8521]
    assert PROMPT_VERSION == "6725b5bb"
    assert SAVE.exists() and actions[-1]["id"] == 8519
    assert all(by_id[a["id"] + 1]["kind"] == "outcome" for a in actions)
    print(json.dumps({"recorded_actions": 0 if args.controlled_fixture else len(actions),
                      "controlled_fixture": args.controlled_fixture, "jev_calls": 0, "benchmark_writes": 0}), flush=True)
    if not args.run:
        return
    with urllib.request.urlopen("http://127.0.0.1:8765/api/live", timeout=8) as response:
        panel = json.load(response)
    if panel["mode"] != "paused" or panel["current_run"] or panel["benchmark"]["remaining"]:
        raise SystemExit("Benchmark must be conclusively stopped before this separate check")
    complete = panel["version"]["finished"] == 30 or any(
        e.get("msg", "").startswith(("Benchmark bench30-a complete", "Benchmark stopped early:"))
        for e in panel["events"])
    if panel["version"]["id"] != PROMPT_VERSION or not complete:
        raise SystemExit("No completed v15 benchmark is available for the separate check")
    game = Balatro(timeout=45)
    if game.state().get("state") != "MENU":
        raise SystemExit("Preserving active game: expected MENU")
    result = {"version": PROMPT_VERSION, "benchmark_run": False, "jev_calls": 0,
              "checkpoint": str(SAVE.relative_to(ROOT)), "replayed_actions": 0, "status": "running",
              "synthetic_test_state": args.controlled_fixture}
    output = OUTPUT.with_name("v15-victory-cleanup-controlled-check.json") if args.controlled_fixture else OUTPUT
    loaded = False
    try:
        loaded = True
        game.call("load", {"path": str(SAVE)})
        gs = game.state()
        assert gs.get("seed") == "X62IZYJ4"
        expected_start = by_id[7895]
        assert all(v == expected_start.get(k) for k, v in facts(gs).items())
        if args.controlled_fixture:
            blind = current_blind(gs)
            assert blind["type"] == "BOSS" and blind["score"] > 0
            overrides = {"ante": 8, "round": 24, "chips": blind["score"]}
            result["test_only_overrides"] = overrides
            game.call("set", overrides)
            gs = game.call("play", {"cards": [0, 1, 2, 3, 4]})
        else:
            for action in actions:
                blind = current_blind(gs)
                gs = game.call(action["method"], action.get("params"))
                expected = by_id[action["id"] + 1]
                difference = {k: {"expected": expected.get(k), "actual": v}
                              for k, v in facts(gs).items() if v != expected.get(k)}
                if difference:
                    raise RuntimeError(f"Replay diverged after event {action['id']}: {difference}")
                result["replayed_actions"] += 1
                if result["replayed_actions"] % 40 == 0:
                    print(json.dumps({"replayed": result["replayed_actions"], "ante": gs.get("ante_num")}), flush=True)
        assert gs.get("state") == "ROUND_EVAL" and gs.get("won") is True
        assert gs["round"]["chips"] >= blind["score"]
        if not args.controlled_fixture:
            assert gs["round"]["chips"] == 137034 and blind["score"] == 100000
        # Give the win overlay time to appear, then capture proof before cleanup.
        time.sleep(0.5)
        before_png = output.with_name(output.stem + "-before.png")
        game.call("screenshot", {"path": str(before_png)})
        bot = JevBot.__new__(JevBot)  # No TypeSafe client and no decisions.
        bot.game, bot.recorder = game, MemoryRecorder()
        bot.events, bot._event_id = deque(), 0
        bot.last_state, bot._finished_seed, bot._round_blind = gs, None, blind
        bot.stats = {"runs": 0, "wins": 0, "best_ante": 0}
        bot.on_round_eval(gs)
        terminal_actions = [e["method"] for e in bot.events if e["kind"] == "action"]
        assert terminal_actions == ["menu"] and bot.recorder.finished == ["won"]
        assert game.state().get("state") == "MENU"
        time.sleep(0.5)
        after_png = output.with_name(output.stem + "-after.png")
        game.call("screenshot", {"path": str(after_png)})
        result.update(status="passed", terminal_actions=terminal_actions, recorded_result=bot.recorder.finished,
                      winning_state=bot.recorder.raw, before_image=before_png.name, after_image=after_png.name,
                      final_state="MENU")
    except Exception as exc:
        result.update(status="failed", error=str(exc))
        raise
    finally:
        if loaded:
            try:
                if game.state().get("state") != "MENU":
                    game.call("menu")
            except Exception as exc:
                result["cleanup_error"] = str(exc)
        output.write_text(json.dumps(result, indent=2), encoding="utf-8")
        print(json.dumps(result), flush=True)


if __name__ == "__main__":
    main()
