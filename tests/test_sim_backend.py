"""Jackdaw sim backend checks: no JEV calls, no live game. Tests needing the game's localization
(scripts/extract_localization.py -> $BALATRO_LOC_DIR or runs/_loc) or the Jackdaw submodule skip
when it is missing."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "scripts"))

import sim_text  # noqa: E402

HAVE_LOC = (sim_text.LOC_DIR / "en-us.lua").exists()
HAVE_JACKDAW = (ROOT / "third_party" / "jackdaw" / "jackdaw").exists()
NEEDS_SIM = unittest.skipUnless(HAVE_LOC and HAVE_JACKDAW, "needs runs/_loc and third_party/jackdaw")


class LuaAndFormatting(unittest.TestCase):
    def test_parse_lua_table(self):
        t = sim_text.parse_lua_table('return { a = 1, ["b c"] = "x\\ny", list = { "p", "q", }, n = -2.5, t = true }')
        self.assertEqual(t["a"], 1)
        self.assertEqual(t["b c"], "x\ny")
        self.assertEqual(t["list"], ["p", "q"])
        self.assertEqual(t["n"], -2.5)
        self.assertIs(t["t"], True)

    def test_number_format(self):
        self.assertEqual(sim_text.number_format(1500), "1,500")
        self.assertEqual(sim_text.number_format(3), "3")
        self.assertEqual(sim_text.number_format(1.5), "1.5")

    def test_signed(self):
        self.assertEqual(sim_text.signed(4), "+4")
        self.assertEqual(sim_text.signed(-4), "-4")
        self.assertEqual(sim_text.signed_dollars(3), "$3")
        self.assertEqual(sim_text.signed_dollars(-3), "-$3")

    def test_parse_line_keeps_text_and_vars(self):
        parts = sim_text.parse_line("{C:mult}+#1#{} Mult")
        text = "".join(p if isinstance(p, str) else "" for p in sim_text.box_lines([parts], [4]))
        self.assertIn("+4", text)
        self.assertIn("Mult", text)


@NEEDS_SIM
class Backend(unittest.TestCase):
    def setUp(self):
        from sim_backend import SimBalatro
        self.game = SimBalatro()

    def test_menu_and_health(self):
        self.assertEqual(self.game.call("health"), {"status": "ok"})
        self.assertEqual(self.game.call("gamestate")["state"], "MENU")

    def test_start_shape(self):
        gs = self.game.call("start", {"seed": "7X7WSDCX", "deck": "RED", "stake": "WHITE"})
        self.assertEqual(gs["state"], "BLIND_SELECT")
        self.assertEqual(gs["seed"], "7X7WSDCX")
        self.assertEqual(gs["deck"], "RED")
        self.assertEqual(len(gs["hands"]), 12)
        self.assertEqual(gs["cards"]["count"], 52)
        card = gs["cards"]["cards"][0]
        self.assertEqual(card["label"], "Base Card")
        self.assertTrue(card["state"]["hidden"])
        self.assertIn(card["value"]["suit"], "HDCS")
        # BalatroBot serialises empty Lua tables as []
        self.assertEqual(gs["jokers"]["cards"], [])
        self.assertEqual(card["modifier"], [])
        json.dumps(gs)

    def test_same_seed_same_game(self):
        a = self.game.call("start", {"seed": "BAPQJMY5", "deck": "RED", "stake": "WHITE"})
        from sim_backend import SimBalatro
        b = SimBalatro().call("start", {"seed": "BAPQJMY5", "deck": "RED", "stake": "WHITE"})
        self.assertEqual(a["blinds"], b["blinds"])
        self.assertEqual([c["key"] for c in a["cards"]["cards"]], [c["key"] for c in b["cards"]["cards"]])

    def test_cerulean_bell_forced_card_joins_selection(self):
        # BalatroBot keeps the forced card highlighted and clicks ours up to 5; it never rejects.
        self.game.call("start", {"seed": "7X7WSDCX", "deck": "RED", "stake": "WHITE"})
        self.game.call("select")
        hand = self.game.gs["hand"]
        hand[7].ability["forced_selection"] = True
        forced = hand[7]
        self.assertEqual(self.game._with_forced({"cards": [0, 1, 2, 3, 4]})["cards"], [0, 1, 2, 3, 7])
        self.assertEqual(self.game._with_forced({"cards": [7, 2]})["cards"], [7, 2])
        self.game.call("play", {"cards": [0, 1]})
        self.assertNotIn(forced, self.game.gs.get("hand") or [])

    def test_errors_mirror_balatrobot(self):
        from balatro import RPCError
        with self.assertRaises(RPCError) as cm:
            self.game.call("play", {"cards": [0]})
        self.assertEqual(cm.exception.name, "INVALID_STATE")
        self.game.call("start", {"seed": "7X7WSDCX", "deck": "RED", "stake": "WHITE"})
        with self.assertRaises(RPCError) as cm:
            self.game.call("rearrange", {"hand": [0]})
        self.assertEqual(cm.exception.name, "INVALID_STATE")
        gs = self.game.call("select")
        self.assertEqual(gs["state"], "SELECTING_HAND")
        n = len(gs["hand"]["cards"])
        for bad in ({}, {"hand": list(range(n - 1))}, {"hand": [0] * n}):
            with self.assertRaises(RPCError) as cm:
                self.game.call("rearrange", bad)
            self.assertEqual(cm.exception.name, "BAD_REQUEST")
        keys = [c["key"] for c in gs["hand"]["cards"]]
        gs = self.game.call("rearrange", {"hand": list(range(n))[::-1]})
        self.assertEqual([c["key"] for c in gs["hand"]["cards"]], keys[::-1])
        self.assertEqual(self.game.engine_errors, [])

    def test_hand_effects_are_localized(self):
        gs = self.game.call("start", {"seed": "7X7WSDCX", "deck": "RED", "stake": "WHITE"})
        gs = self.game.call("select")
        for c in gs["hand"]["cards"]:
            self.assertRegex(c["value"]["effect"], r"^\+\d+ chips")


@NEEDS_SIM
@NEEDS_SIM
class IssueTwoMechanics(unittest.TestCase):
    """GitHub issue #2: mechanics where Jackdaw differed from the game's Lua (verified in the dump)."""

    def setUp(self):
        from sim_backend import SimBalatro
        from jackdaw.engine.card_factory import create_joker
        self.game, self.joker = SimBalatro(), create_joker
        self.game.call("start", {"seed": "7X7WSDCX", "deck": "RED", "stake": "WHITE"})

    def test_sliced_joker_does_not_act(self):
        # Dagger acts first and slices Madness, which then does nothing: only Madness is lost.
        gs = self.game.gs
        dagger, madness, plain = (self.joker(k) for k in ("j_ceremonial", "j_madness", "j_joker"))
        gs["jokers"] = [dagger, madness, plain]
        self.game.call("select")
        self.assertEqual([j.center_key for j in gs["jokers"]], ["j_ceremonial", "j_joker"])
        self.assertEqual(madness.ability.get("x_mult", 1), 1)

    def test_saved_round_pays_no_blind_reward(self):
        from jackdaw.engine import economy
        self.game.call("select")
        gs = self.game.gs
        blind = gs["blind"]
        kw = dict(blind=blind, hands_left=0, discards_left=0, money=0, jokers=[], game_state=gs)
        gs["chips"] = blind.chips - 1
        self.assertEqual(economy.calculate_round_earnings(**kw).blind_reward, 0)
        gs["chips"] = blind.chips
        self.assertEqual(economy.calculate_round_earnings(**kw).blind_reward, blind.dollars)
        self.assertGreater(blind.dollars, 0)

    def test_satellite_pays_per_distinct_planet(self):
        from jackdaw.engine import game as engine
        self.game.call("select")
        gs = self.game.gs
        sat = self.joker("j_satellite")
        gs["jokers"] = [sat]
        gs["consumable_usage"] = {"c_pluto": {"count": 3, "set": "Planet"},
                                  "c_mars": {"count": 1, "set": "Planet"},
                                  "c_fool": {"count": 2, "set": "Tarot"}}
        gs["chips"] = gs["blind"].chips
        engine._round_won(gs)
        self.assertEqual(sat.ability.get("planet_types_used"), 2)


class Patches(unittest.TestCase):
    def test_xmult_uses_smods_arithmetic(self):
        import sim_backend  # noqa: F401  (applies the patches)
        from jackdaw.engine import scoring
        self.assertEqual(getattr(scoring, "SMODS_XMULT_SITES", None), 10)

    def test_pairs_order_for_to_do_list(self):
        import sim_patches
        self.assertEqual(sim_patches.HANDS_PAIRS_ORDER[:3], ["Full House", "Flush", "Pair"])


@NEEDS_SIM
class SmokeGame(unittest.TestCase):
    def test_greedy_game_reaches_the_end(self):
        import sim_run
        with tempfile.TemporaryDirectory() as tmp:
            r = sim_run.play("7X7WSDCX", "greedy", out_root=Path(tmp))
            self.assertNotIn("error", r, r.get("trace"))
            self.assertIn(r["result"], ("won", "lost"))
            self.assertEqual(r["game_errors"], 0)
            self.assertEqual(r["engine_errors"], [])
            self.assertEqual(r["input_tokens"], 0)
            summ = json.loads(next(Path(tmp).glob("*/summary.json")).read_text(encoding="utf-8"))
            self.assertEqual(summ["backend"], "jackdaw")


if __name__ == "__main__":
    unittest.main()
