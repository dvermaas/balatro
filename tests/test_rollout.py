"""src/rollout.py: public-state reconstruction, options, and same-seed agreement with the true sim state.
No JEV calls; skips without the game's localization or the Jackdaw submodule."""
import copy
import random
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

import sim_text  # noqa: E402

HAVE = (sim_text.LOC_DIR / "en-us.lua").exists() and (ROOT / "third_party" / "jackdaw" / "jackdaw").exists()
NEEDS_SIM = unittest.skipUnless(HAVE, "needs runs/_loc and third_party/jackdaw")


def _first_shop(seed):
    """A true engine state in the first shop (small blind of ante 1 cleared by the code policy)."""
    import rollout
    import sim_backend
    from jackdaw.engine.actions import CashOut
    from jackdaw.engine.game import step
    game = sim_backend.SimBalatro()
    game.call("start", {"seed": seed, "deck": "RED", "stake": "WHITE"})
    gs = game.gs
    gs["dollars"] = 30
    r = rollout.play_horizon(gs, rollout.Policy([]), bosses=0)
    assert r[-1]["cleared"], r
    step(gs, CashOut())
    return gs


@NEEDS_SIM
class Reconstruction(unittest.TestCase):
    def test_dynamic_joker_values_round_trip_through_text(self):
        import rollout
        from jackdaw.engine.card import Card
        from jackdaw.engine.run_init import initialize_run
        gs = initialize_run("b_red", 1, "TESTSEED")
        cases = {"j_green_joker": ("mult", 7), "j_ride_the_bus": ("mult", 12), "j_runner": ("extra.chips", 45),
                 "j_square": ("extra.chips", 16), "j_ice_cream": ("extra.chips", 70), "j_constellation": ("x_mult", 1.4),
                 "j_hologram": ("x_mult", 1.75), "j_ramen": ("x_mult", 1.62), "j_popcorn": ("mult", 12),
                 "j_wee": ("extra.chips", 24), "j_obelisk": ("x_mult", 2.2), "j_caino": ("caino_xmult", 2)}
        for key, (field, val) in cases.items():
            c = Card()
            c.set_ability(key)
            if field.startswith("extra."):
                c.ability["extra"][field[6:]] = val
            else:
                c.ability[field] = val
            text = sim_text.card_effect(c, gs, "jokers")
            src = {"key": key, "value": {"effect": text}, "modifier": [], "state": [], "cost": {"buy": 6, "sell": 3}}
            back = rollout.joker_card(src, {"hands_played": 0})
            got = back.ability["extra"][field[6:]] if field.startswith("extra.") else back.ability[field]
            self.assertAlmostEqual(float(got), float(val), msg=f"{key}: {text}")

    def test_options_respect_money_slots_and_support(self):
        import rollout
        pub = {"money": 6, "jokers": {"limit": 1, "cards": [
            {"key": "j_joker", "label": "Joker", "modifier": [], "cost": {"buy": 2, "sell": 1}}]},
            "consumables": {"limit": 2, "cards": []},
            "shop": {"cards": [
                {"key": "j_greedy_joker", "set": "JOKER", "label": "Greedy Joker", "modifier": [], "cost": {"buy": 5}},
                {"key": "c_jupiter", "set": "PLANET", "label": "Jupiter", "modifier": [], "cost": {"buy": 3}},
                {"key": "c_tower", "set": "TAROT", "label": "The Tower", "modifier": [], "cost": {"buy": 3}},
                {"key": "j_blueprint", "set": "JOKER", "label": "Blueprint", "modifier": [], "cost": {"buy": 10}}]},
            "vouchers": {"cards": [{"key": "v_grabber", "label": "Grabber", "cost": {"buy": 10}}]}}
        keys = {o["key"]: o for o in rollout.options(pub)}
        self.assertIn("skip", keys)
        self.assertNotIn("buy_card_0", keys)          # no free joker slot
        self.assertIn("replace_0_0", keys)            # $6 + $1 sell >= $5
        self.assertNotIn("replace_0_3", keys)         # Blueprint unaffordable even after selling
        self.assertTrue(keys["buy_card_1"]["supported"])
        self.assertFalse(keys["buy_card_2"]["supported"])   # tarots not simulated in phase 1
        self.assertNotIn("buy_voucher_0", keys)

    def test_reconstruction_matches_true_state_with_same_seeds(self):
        """Branching the TRUE engine state and the public reconstruction with the same RNG seed
        must play out identically in a plain early state (no hidden information involved)."""
        import rollout
        import sim_backend
        from jackdaw.engine.rng import PseudoRandom
        true_gs = _first_shop("RTEST111")
        pub = sim_backend.serialize(true_gs)
        base = rollout.build_state(pub)
        for o in [o for o in rollout.options(pub) if o["supported"]][:3]:
            pol = rollout.Policy(rollout.option_jokers(pub, o))
            for k in range(2):
                a, b = copy.deepcopy(true_gs), copy.deepcopy(base)
                for g in (a, b):
                    g["rng"] = PseudoRandom(rollout._seed(k))
                    rollout.apply_option(g, o)
                ra, rb = rollout.play_horizon(a, pol, 1), rollout.play_horizon(b, pol, 1)
                self.assertEqual([(x["type"], x["cleared"], x["chips"]) for x in ra],
                                 [(x["type"], x["cleared"], x["chips"]) for x in rb], o["key"])

    def test_forecast_ignores_draw_pile_order(self):
        import rollout
        import sim_backend
        pub = sim_backend.serialize(_first_shop("RTEST222"))
        shuffled = copy.deepcopy(pub)
        random.Random(1).shuffle(shuffled["cards"]["cards"])
        a = rollout.forecast(pub, n=3, bosses=1, keys={"skip"})
        b = rollout.forecast(shuffled, n=3, bosses=1, keys={"skip"})
        self.assertEqual(a["_runs"], b["_runs"])
        f = a["options"]["skip"]
        self.assertEqual(f["n"], 3)
        self.assertTrue(0 <= f["p_survive"] <= 1)
        self.assertTrue(f["blinds"])


class _Bot:
    def __init__(self):
        self.events = []

    def emit(self, kind, **kw):
        self.events.append((kind, kw))


@NEEDS_SIM
class ShopWiring(unittest.TestCase):
    """v19: decisions.shop's forecast helpers (cache, fields JEV sees, fallback to the v18 path)."""

    def setUp(self):
        import sim_backend
        self.pub = sim_backend.serialize(_first_shop("RTEST333"))

    def test_fields_cache_and_note(self):
        import decisions
        import rollout
        bot = _Bot()
        old = (decisions.FORECAST_N, decisions.FORECAST_BUDGET)
        decisions.FORECAST_N, decisions.FORECAST_BUDGET = 4, None
        try:
            fc = decisions._shop_forecast(bot, self.pub, [], False)
            self.assertIsNotNone(fc)
            self.assertIs(decisions._shop_forecast(bot, self.pub, [], False), fc)  # cached per shop state
        finally:
            decisions.FORECAST_N, decisions.FORECAST_BUDGET = old
        self.assertIn("skip", fc["options"])
        for key in fc["options"]:
            f = decisions._forecast_fields(fc, key)
            for field in ("next_boss_clear", "survive_2_bosses", "blinds_cleared_next_6", "_fc_delta", "_fc_se"):
                self.assertIn(field, f)
            self.assertIn("skip", f["next_boss_clear"])
        self.assertEqual(fc["options"]["skip"]["vs_skip"]["blinds"], [0.0, 0.0])
        self.assertIn("build frozen", decisions._forecast_note(self.pub, fc))
        self.assertFalse(any(k.startswith(("buy_pack", "_")) for k in fc["options"]))
        self.assertTrue(set(rollout.options(self.pub)[0]) >= {"key", "supported"})

    def test_failure_falls_back(self):
        import decisions
        import rollout
        bot = _Bot()
        orig = rollout.forecast
        rollout.forecast = lambda *a, **k: (_ for _ in ()).throw(RuntimeError("boom"))
        try:
            self.assertIsNone(decisions._shop_forecast(bot, self.pub, [], False))
        finally:
            rollout.forecast = orig
        self.assertTrue(any("Forecast unavailable" in kw.get("msg", "") for _, kw in bot.events))

    def test_no_discards_policy_plays_spare_cards(self):
        import rollout
        info = lambda r, s: {"rank": r, "suit": s, "enh": None, "edition": None, "seal": None, "debuff": False}
        infos = [info("K", "H"), info("K", "S"), info("2", "C"), info("3", "D"), info("7", "S"), info("9", "H")]
        pol = rollout.Policy([])
        levels = {"Pair": {"chips": 10, "mult": 2, "played": 0}, "High Card": {"chips": 5, "mult": 1, "played": 0}}
        out = pol.cycle(infos, [0, 1], "Pair", 0, levels, {"hands_left": 3, "discards_left": 0})
        self.assertEqual(len(out), 5)
        self.assertTrue({0, 1} <= set(out))


if __name__ == "__main__":
    unittest.main()
