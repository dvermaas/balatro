"""Offline controller checks: no JEV calls, no live game, no recording writes."""
import copy
import sys
import unittest
from collections import deque
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
from jev_bot import JevBot


class Recorder:
    def __init__(self):
        self.run = {"seed": "TEST"}
        self.finished = []
        self.ensures = 0
        self.raw = []

    def ensure(self, *args):
        self.ensures += 1

    def progress(self, gs):
        pass

    def write(self, event):
        pass

    def write_raw(self, event_id, gs):
        self.raw.append((event_id, copy.deepcopy(gs)))

    def finish(self, result, gs):
        self.finished.append(result)
        self.run = None


class Game:
    def __init__(self, gs, fail_menu=False):
        self.gs = copy.deepcopy(gs)
        self.calls = []
        self.fail_menu = fail_menu

    def state(self):
        return self.gs

    def call(self, method, params=None):
        self.calls.append(method)
        if method == "menu" and self.fail_menu:
            self.fail_menu = False
            raise TimeoutError("menu timed out")
        self.gs = {"state": "MENU"} if method == "menu" else {**self.gs, "state": "SHOP"}
        return self.gs


def bot_for(state="ROUND_EVAL", ante=9, won=True, chips=137034, fail_menu=False):
    gs = dict(seed="TEST", state=state, ante_num=ante, round_num=24, won=won, money=30,
              round=dict(chips=chips, hands_left=0, discards_left=0), jokers=dict(cards=[]))
    bot = JevBot.__new__(JevBot)  # Deliberately skip the API client constructor.
    bot.game, bot.recorder = Game(gs, fail_menu), Recorder()
    bot.deck, bot.stake, bot.model = "RED", "WHITE", "jev-latest"
    bot.events, bot._event_id = deque(), 0
    bot.last_state, bot._finished_seed = None, None
    bot._round_blind = dict(type="BOSS", name="Amber Acorn", score=100000)
    bot.stats = dict(runs=0, wins=0, best_ante=0, errors=0)
    bot.auto_actions = False
    bot.bench_current = dict(name="bench30-a", index=18, of=30)
    bot.bench_queue = [(19, "NEXT")]
    return bot


class RunCompletionTests(unittest.TestCase):
    def test_verified_win_returns_to_menu_without_cashout(self):
        bot = bot_for()
        bot.step()
        self.assertEqual(bot.game.calls, ["menu"])
        self.assertEqual(bot.recorder.finished, ["won"])
        self.assertEqual(bot.stats["wins"], 1)
        self.assertEqual(bot.recorder.raw[0][1]["round"]["chips"], 137034)
        self.assertEqual(bot.bench_queue, [(19, "NEXT")])
        self.assertEqual([e["kind"] for e in bot.events], ["round_end", "run_end", "action", "outcome"])

    def test_final_boss_loss_with_bad_win_flag_stays_loss(self):
        bot = bot_for(state="GAME_OVER", ante=8, won=True, chips=32942)
        bot.step()
        self.assertEqual(bot.recorder.finished, ["lost"])
        self.assertEqual(bot.stats["wins"], 0)
        self.assertEqual(bot.game.calls, ["menu"])

    def test_ordinary_round_still_cashes_out(self):
        bot = bot_for(ante=3, won=False)
        bot.step()
        self.assertEqual(bot.game.calls, ["cash_out"])
        self.assertEqual(bot.recorder.finished, [])

    def test_ambiguous_victory_pauses_without_more_game_actions(self):
        bot = bot_for(chips=50000)
        with self.assertRaisesRegex(RuntimeError, "verified final-boss"):
            bot.step()
        self.assertEqual(bot.game.calls, [])
        self.assertEqual(bot.recorder.finished, [])

    def test_menu_timeout_retries_cleanup_without_replaying_or_double_counting(self):
        bot = bot_for(fail_menu=True)
        with self.assertRaises(TimeoutError):
            bot.step()
        bot.step()
        self.assertEqual(bot.recorder.finished, ["won"])
        self.assertEqual(bot.stats["runs"], 1)
        self.assertEqual(bot.stats["wins"], 1)
        self.assertEqual(bot.recorder.ensures, 1)
        self.assertEqual(bot.game.calls, ["menu", "menu"])
        self.assertEqual(sum(e["kind"] == "round_end" for e in bot.events), 1)

    def test_existing_endless_run_keeps_prior_victory(self):
        bot = bot_for(state="GAME_OVER", ante=9, won=True, chips=100)
        bot.step()
        self.assertEqual(bot.recorder.finished, ["won"])


if __name__ == "__main__":
    unittest.main()
