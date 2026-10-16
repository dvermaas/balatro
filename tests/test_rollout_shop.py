"""src/rollout_shop.py (shopping rollouts, lab only). No JEV calls; the sim tests skip without the game's
localization or the Jackdaw submodule."""
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


def _first_shop(seed, money=30):
    import rollout
    import sim_backend
    from jackdaw.engine.actions import CashOut
    from jackdaw.engine.game import step
    game = sim_backend.SimBalatro()
    game.call("start", {"seed": seed, "deck": "RED", "stake": "WHITE"})
    gs = game.gs
    gs["dollars"] = money
    r = rollout.play_horizon(gs, rollout.Policy([]), bosses=0)
    assert r[-1]["cleared"], r
    step(gs, CashOut())
    return gs


def _joker(key):
    from jackdaw.engine.card import Card
    c = Card()
    c.set_ability(key)
    c.cost, c.sell_cost = 5, 2
    return c


@NEEDS_SIM
class ShopperRules(unittest.TestCase):
    def setUp(self):
        import rollout
        import rollout_shop
        import sim_backend
        self.gs = _first_shop("RSHOP001")
        self.pub = sim_backend.serialize(self.gs)
        self.state = rollout.build_state(self.pub)
        self.state["_rollout_no_shop"] = False
        self.shopper = rollout_shop.Shopper(self.pub)

    def _shop(self, cards, money, ante=1):
        self.state["shop_cards"] = cards
        self.state["dollars"] = money
        self.state["round_resets"]["ante"] = ante
        self.shopper.shop(self.state)
        return [j.center_key for j in self.state["jokers"]]

    def test_buys_the_more_valuable_affordable_joker(self):
        before = [j.center_key for j in self.state["jokers"]]
        got = self._shop([_joker("j_credit_card"), _joker("j_joker")], money=6)
        # +4 Mult outscores Credit Card on the synthetic hands; only one is affordable at $6
        self.assertEqual(got, before + ["j_joker"])
        self.assertEqual(self.shopper.stats["joker_buys"], 1)

    def test_interest_buffer_from_ante_3(self):
        before = [j.center_key for j in self.state["jokers"]]
        self.assertEqual(self._shop([_joker("j_joker")], money=28, ante=3), before)  # 28 - 5 < 25
        self.assertEqual(self._shop([_joker("j_joker")], money=30, ante=3), before + ["j_joker"])

    def test_full_slots_swap_only_for_a_clear_upgrade(self):
        import rollout_shop
        self.state["jokers"] = [_joker("j_credit_card") for _ in range(self.state["joker_slots"])]
        got = self._shop([_joker("j_joker")], money=10)
        self.assertEqual(got.count("j_joker"), 1)
        self.assertEqual(len(got), self.state["joker_slots"])
        self.assertEqual(self.shopper.stats["swaps"], 1)
        # a joker no better than the weakest owned one is not swapped in
        self.state["jokers"] = [_joker("j_joker") for _ in range(self.state["joker_slots"])]
        self.shopper.stats.clear()
        self._shop([_joker("j_joker")], money=10)
        self.assertEqual(self.shopper.stats["swaps"], 0)
        self.assertGreater(rollout_shop.SWAP_RATIO, 1)

    def test_rental_and_eternal_jokers_have_no_value(self):
        c = _joker("j_joker")
        c.rental = True
        self.assertEqual(self.shopper.value(c, self.state, "shop_cards"), 0.0)


@NEEDS_SIM
class Forecast(unittest.TestCase):
    def test_rollouts_shop_and_keep_the_forecast_shape(self):
        import rollout
        import rollout_shop
        import sim_backend
        pub = sim_backend.serialize(_first_shop("RSHOP002"))
        a = rollout_shop.forecast(pub, n=3, bosses=1)
        b = rollout.forecast(pub, n=3, bosses=1)
        self.assertEqual(set(a["options"]), set(b["options"]))
        self.assertEqual(set(a["options"]["skip"]), set(b["options"]["skip"]))
        self.assertGreater(a["shop_stats"].get("shops", 0), 0)  # later shops are populated and visited
        self.assertEqual(a["shop_stats"].get("errors", 0), 0)
        # deterministic: same public state, same seeds
        self.assertEqual(a["_runs"], rollout_shop.forecast(pub, n=3, bosses=1)["_runs"])

    def test_ignores_draw_pile_order(self):
        import rollout_shop
        import sim_backend
        pub = sim_backend.serialize(_first_shop("RSHOP003"))
        shuffled = copy.deepcopy(pub)
        random.Random(1).shuffle(shuffled["cards"]["cards"])
        a = rollout_shop.forecast(pub, n=3, bosses=1, keys={"skip"})
        b = rollout_shop.forecast(shuffled, n=3, bosses=1, keys={"skip"})
        self.assertEqual(a["_runs"], b["_runs"])

    def test_items_variant(self):
        import rollout_shop
        import sim_backend
        pub = sim_backend.serialize(_first_shop("RSHOP004"))
        f = rollout_shop.forecast_items(pub, n=2, bosses=1)
        self.assertIn("skip", f["options"])
        self.assertFalse(f["errors"])
        self.assertGreater(f["shop_stats"].get("shops", 0), 0)

    def test_production_flags_and_patch_restore(self):
        import rollout
        import rollout_items
        import rollout_shop
        import sim_backend
        pub = sim_backend.serialize(_first_shop("RSHOP005"))
        self.assertTrue(rollout.build_state(pub)["_rollout_no_shop"])  # production rollouts still skip shops
        orig = (rollout.forecast, rollout_items.forecast)
        with rollout_shop.Patched():
            self.assertIs(rollout.forecast, rollout_shop.forecast)
            self.assertIs(rollout_items.forecast, rollout_shop.forecast_items)
        self.assertEqual((rollout.forecast, rollout_items.forecast), orig)

    def test_live_card_ids_untouched(self):
        import jackdaw.engine.card as card_mod
        import rollout_shop
        import sim_backend
        pub = sim_backend.serialize(_first_shop("RSHOP006"))
        before = card_mod._sort_id_counter
        rollout_shop.forecast(pub, n=2, bosses=1, keys={"skip"})
        self.assertEqual(card_mod._sort_id_counter, before)


if __name__ == "__main__":
    unittest.main()
