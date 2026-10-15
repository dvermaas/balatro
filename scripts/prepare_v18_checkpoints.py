"""Reproduce recorded first shops and save opaque game checkpoints for paired tests.

Only replays the recorded actions up to the selected shop. Never inspects RNG
or draw order. Refuses an active panel/game; never modifies benchmark records.
"""
import json
import sys
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
from balatro import Balatro
from hands import _d, card_info

RUNS = ["20261014-140906-2RGEX6EN", "20261014-135701-BAPQJMY5", "20261014-143144-75WVKF42"]


def main():
    with urllib.request.urlopen("http://127.0.0.1:8765/api/live", timeout=5) as f:
        panel = json.load(f)
    assert panel["mode"] == "paused" and not panel["benchmark"]["current"] and not panel["benchmark"]["remaining"]
    game = Balatro(timeout=12)
    assert game.state()["state"] == "MENU", "Preserve the active game"
    output = ROOT / "runs/_experiments/v18-checkpoints"
    output.mkdir(parents=True, exist_ok=True)
    for name in RUNS:
        events = list(map(json.loads, (ROOT / "runs" / name / "decisions.jsonl").read_text(encoding="utf-8").splitlines()))
        shop = next(e for e in events if e.get("kind") == "decision" and e.get("question") == "shop")
        seed = name.rsplit("-", 1)[1]
        save = output / f"{seed}.jkr"
        if save.exists():
            raise RuntimeError(f"Checkpoint already exists: {save}")
        try:
            game.call("start", {"seed": seed, "deck": "RED", "stake": "WHITE"})
            recent = []
            for e in events:
                if e["id"] >= shop["id"]:
                    break
                if e["kind"] != "action":
                    continue
                if e["method"] == "play":
                    gs = game.state()
                    # Only the visible hand; no draw-pile data enters this history.
                    recent.append({"infos": [card_info(c) for c in _d(gs.get("hand")).get("cards", [])],
                                   "combo": e["params"]["cards"],
                                   "rnd": {k: _d(gs.get("round")).get(k) for k in ("hands_left", "discards_left")}})
                game.call(e["method"], e.get("params"))
            gs = game.state()
            assert gs["state"] == "SHOP" and gs["seed"] == seed
            assert gs["money"] == shop["state_sent"]["money"], (seed, gs["money"])
            assert gs["ante_num"] == shop["ante"] and gs["round_num"] == shop["round"]
            public = {"seed": seed, "state": gs["state"], "ante": gs["ante_num"], "round": gs["round_num"],
                      "money": gs["money"], "shop": [c.get("key") for c in _d(gs.get("shop")).get("cards", [])],
                      "packs": [c.get("key") for c in _d(gs.get("packs")).get("cards", [])],
                      "jokers": [c.get("key") for c in _d(gs.get("jokers")).get("cards", [])]}
            game.call("save", {"path": str(save)})
            time.sleep(.2)
            assert save.is_file() and save.stat().st_size > 0
            (output / f"{seed}.json").write_text(json.dumps({"recorded_run": name, "shop_event": shop["id"],
                "public": public, "recent": recent}, indent=2) + "\n", encoding="utf-8")
            print(seed, "saved", public["money"], "dollars", flush=True)
        finally:
            game.call("menu")


if __name__ == "__main__":
    main()
