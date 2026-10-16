"""src/round_planner.py: candidates, public-only rollouts (the real draw order is never used), the risk gate.
No JEV calls; skips without the game's localization or the Jackdaw submodule."""
import random
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

import sim_text  # noqa: E402

HAVE = (sim_text.LOC_DIR / "en-us.lua").exists() and (ROOT / "third_party" / "jackdaw" / "jackdaw").exists()
NEEDS_SIM = unittest.skipUnless(HAVE, "needs runs/_loc and third_party/jackdaw")


def _first_hand(seed):
    import sim_backend
    game = sim_backend.SimBalatro()
    game.call("start", {"seed": seed, "deck": "RED", "stake": "WHITE"})
    game.call("select", {})
    return game


@NEEDS_SIM
class Planner(unittest.TestCase):
    def setUp(self):
        import round_planner
        self.rp = round_planner
        self.game = _first_hand("PLANTES1")  # first hand below 1.5x the target
        self.pub = self.game.state()
        self.assertEqual(self.pub["state"], "SELECTING_HAND")

    def test_plan_rows_and_pick(self):
        out = self.rp.plan(self.pub, self.game.gs, r=8, budget_s=None)
        self.assertIsNotNone(out)
        self.assertGreaterEqual(len(out["rows"]), 2)
        self.assertTrue(any(r["kind"] == "discard" for r in out["rows"]))
        for r in out["rows"]:
            self.assertTrue(0 <= r["p_clear"] <= 1)
        self.assertEqual(out["pick"]["p_clear"], max(r["p_clear"] for r in out["rows"]))
        self.assertEqual(out["rollouts"], 8)
        self.assertIn("defer", out)

    def test_real_draw_order_is_not_used(self):
        a = self.rp.plan(self.pub, self.game.gs, r=6, budget_s=None)
        random.Random(3).shuffle(self.game.gs["deck"])
        b = self.rp.plan(self.game.state(), self.game.gs, r=6, budget_s=None)
        self.assertEqual([r["p_clear"] for r in a["rows"]], [r["p_clear"] for r in b["rows"]])

    def test_no_plan_when_a_play_clears(self):
        game = _first_hand("PLANTEST")  # dealt a Full House that clears the small blind
        self.assertIsNone(self.rp.plan(game.state(), game.gs, r=4, budget_s=None))


if __name__ == "__main__":
    unittest.main()
