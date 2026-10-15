"""Start a fresh fixed benchmark with a verified version; refuse duplicate trials.

This launcher changes evaluation settings only, never the decision source.
Use resume_benchmark.py for a version/set that already has recorded attempts.
"""
import argparse
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
import benchmark
from recorder import PROMPT_VERSION


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--expected-version", required=True)
    parser.add_argument("--set", default=benchmark.NAME, choices=benchmark.SETS)
    parser.add_argument("--finish-set", action="store_true",
                        help="Complete every seed without performance early stopping")
    args = parser.parse_args()
    if PROMPT_VERSION != args.expected_version:
        raise SystemExit(f"Decision source is {PROMPT_VERSION}, expected {args.expected_version}")

    import panel
    from balatro import Balatro
    # Reserve the port before any game actions: never run alongside another panel.
    server = panel.ThreadingHTTPServer(("127.0.0.1", 8765), panel.Handler)
    try:
        for path in (ROOT / "runs").glob("*/summary.json"):
            row = json.loads(path.read_text(encoding="utf-8"))
            if (row.get("prompt_version") == PROMPT_VERSION
                    and (row.get("benchmark") or {}).get("name") == args.set):
                raise SystemExit(f"Existing attempt {path.parent.name}; resume instead of duplicating it")
        game = Balatro(timeout=8).state()
        if game.get("state") != "MENU":
            raise SystemExit("Game is not at MENU; preserve the active game")
        panel.ctl = ctl = panel.Controller()
        bot = ctl.bot
        assert (bot.deck, bot.stake, bot.model) == ("RED", "WHITE", "jev-latest")
        bot.auto_new_run = False
        bot.last_state = game
        ctl.delay = .3
        if args.finish_set:
            bot.EARLY_STOP_AFTER = len(benchmark.SETS[args.set]) + 1
        manifest = {"started": time.time(), "version": PROMPT_VERSION, "set": args.set,
                    "seeds": benchmark.SETS[args.set], "deck": bot.deck, "stake": bot.stake,
                    "model": bot.model, "delay": ctl.delay, "auto_new_run": False,
                    "performance_early_stop": not args.finish_set,
                    "early_stop_after": bot.EARLY_STOP_AFTER}
        path = ROOT / "runs/_experiments" / f"{PROMPT_VERSION}-{args.set}-launch.json"
        path.parent.mkdir(exist_ok=True, parents=True)
        with path.open("x", encoding="utf-8") as f:
            f.write(json.dumps(manifest, indent=2) + "\n")
        bot.start_benchmark(args.set)
        if args.finish_set:
            bot.emit("info", msg="Full fixed-set evaluation requested: performance early stopping disabled; pause after the final seed.")
        print(json.dumps(manifest), flush=True)
        ctl.mode = "running"
        server.serve_forever()
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
