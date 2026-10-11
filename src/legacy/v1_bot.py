"""Run prompt version v1 (f67d4d14) on today's plumbing: `python panel.py --legacy v1`.

legacy/v1/ holds the exact v1 jev_bot.py and hands.py (their bytes hash to f67d4d14). We load them
under private module names and graft v1's decision handlers and state summary onto the current
JevBot, so recording, the benchmark and error handling are current but every decision is v1's.
"""

import importlib.util
import sys
from pathlib import Path

from jev_bot import JevBot

HERE = Path(__file__).parent / "v1"
VERSION = "f67d4d14"


def _load(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def make_bot():
    v1_hands = _load("v1_hands", "hands.py")
    saved = sys.modules.get("hands")
    sys.modules["hands"] = v1_hands  # v1's jev_bot does `from hands import ...`
    try:
        v1 = _load("v1_jev_bot", "jev_bot.py")
    finally:
        sys.modules["hands"] = saved

    class V1Bot(JevBot):
        auto_actions = False
        on_blind_select = v1.JevBot.on_blind_select
        on_selecting_hand = v1.JevBot.on_selecting_hand
        on_shop = v1.JevBot.on_shop
        on_smods_booster_opened = v1.JevBot.on_smods_booster_opened

        def build_state(self, gs):
            return {"rules": v1.GAME_RULES, "game": v1.summarize(gs)}

    bot = V1Bot()
    bot.recorder.version = VERSION
    return bot
