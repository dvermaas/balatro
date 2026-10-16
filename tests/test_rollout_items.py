"""src/rollout_items.py (pack/tarot/spectral forecast) and the policy lab's death tables.
No JEV calls; the sim tests skip without the game's localization or the Jackdaw submodule."""
import copy
import random
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "scripts"))

import sim_text  # noqa: E402

HAVE = (sim_text.LOC_DIR / "en-us.lua").exists() and (ROOT / "third_party" / "jackdaw" / "jackdaw").exists()
NEEDS_SIM = unittest.skipUnless(HAVE, "needs runs/_loc and third_party/jackdaw")


def _first_shop(seed="ITEMTEST"):
    """A true engine state in the first shop, with $30."""
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


def _card(key, set_, label=None, cost=3, effect=""):
    return {"key": key, "set": set_, "label": label or key, "value": {"effect": effect}, "modifier": [], "state": [],
            "cost": {"buy": cost, "sell": cost // 2}}


def _playing(rank, suit, enh=None):
    m = {"enhancement": enh} if enh else []
    return {"key": f"{suit}_{rank}", "set": "ENHANCED" if enh else "DEFAULT", "label": "Base Card",
            "value": {"rank": rank, "suit": suit}, "modifier": m, "state": [], "cost": {"buy": 1, "sell": 1}}


@NEEDS_SIM
class Options(unittest.TestCase):
    def test_packs_and_usable_consumables_only(self):
        import rollout_items
        pub = {"money": 6, "consumables": {"limit": 2, "cards": []},
               "shop": {"cards": [_card("c_hermit", "TAROT"), _card("c_ankh", "SPECTRAL", cost=4),
                                  _card("c_magician", "TAROT", cost=9), _card("j_joker", "JOKER")]},
               "packs": {"cards": [_card("p_arcana_normal_1", "BOOSTER", cost=4),
                                   _card("p_buffoon_mega_1", "BOOSTER", cost=8)]}}
        keys = [o["key"] for o in rollout_items.options(pub)]
        # Hermit yes; Ankh is never used by the bot; Magician is unaffordable; jokers are rollout.forecast's job
        self.assertEqual(keys, ["skip", "buy_card_0", "buy_pack_0"])
        pub["consumables"]["cards"] = [_card("c_fool", "TAROT"), _card("c_fool", "TAROT")]
        self.assertEqual([o["key"] for o in rollout_items.options(pub)], ["skip", "buy_pack_0"])


@NEEDS_SIM
class Forecast(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import sim_backend
        gs = _first_shop()
        cls.pub = sim_backend.serialize(gs)
        cls.assertTrue(cls, cls.pub["packs"]["cards"], "the first shop has packs")

    def _fc(self, pub):
        import rollout_items
        fc = rollout_items.forecast(pub, n=3, bosses=1)
        return {k: (r["exp_blinds_cleared"], r.get("vs_skip", {}).get("blinds")) for k, r in fc["options"].items()}

    def test_every_pack_option_runs(self):
        out = self._fc(self.pub)
        self.assertIn("skip", out)
        self.assertTrue(any(k.startswith("buy_pack_") for k in out), out)

    def test_public_only_draw_order_and_seed_are_not_read(self):
        """Pack contents and draws come from fresh rollout RNG: shuffling the draw pile or changing the real
        seed must not change the forecast."""
        base = self._fc(self.pub)
        pub = copy.deepcopy(self.pub)
        random.Random(1).shuffle(pub["cards"]["cards"])
        pub["seed"] = "ZZZZZZZZ"
        self.assertEqual(self._fc(pub), base)

    def test_pack_contents_differ_across_rollouts(self):
        import rollout
        import rollout_items
        from jackdaw.engine.actions import OpenBooster
        from jackdaw.engine.game import step
        from jackdaw.engine.rng import PseudoRandom
        base = rollout_items.build_state(self.pub)
        seen = set()
        for k in range(6):
            gs = copy.deepcopy(base)
            gs["rng"] = PseudoRandom(rollout._seed(k))
            step(gs, OpenBooster(card_index=0))
            seen.add(tuple(c.center_key for c in gs["pack_cards"]))
        self.assertGreater(len(seen), 1)


@NEEDS_SIM
class Picks(unittest.TestCase):
    def setUp(self):
        import rollout_items
        deck = [_playing(r, s) for r in "23456789TJQKA" for s in "HDCS"]
        self.pub = {"money": 12, "cards": {"cards": deck}, "hand": {"limit": 8, "cards": []},
                    "jokers": {"limit": 5, "cards": []}, "hands": {}, "consumables": {"limit": 2, "cards": []}}
        self.pk = rollout_items.Picker(self.pub)

    def test_black_hole_and_planets_are_taken(self):
        pack = [_card("c_ankh", "SPECTRAL"), _card("c_black_hole", "SPECTRAL")]
        self.assertEqual(self.pk.pick(pack, [], 12, [], 5), (1, None))
        pack = [_card("c_pluto", "PLANET", effect="(lvl.1) Level up High Card +1 Mult and +10 chips")]
        self.assertEqual(self.pk.pick(pack, [], 12, [], 5)[0], 0)

    def test_jokers_need_a_free_slot(self):
        pack = [_card("j_joker", "JOKER", effect="+4 Mult")]
        self.assertEqual(self.pk.pick(pack, [], 12, [], 5)[0], 0)
        full = [_card("j_joker", "JOKER", effect="+4 Mult")] * 5
        self.assertIsNone(self.pk.pick(pack, [], 12, full, 5))

    def test_targeted_tarots_need_a_plan_and_hermit_scales_with_money(self):
        hand = [_playing("2", "H"), _playing("K", "S"), _playing("9", "D")]
        pack = [_card("c_chariot", "TAROT"), _card("c_hermit", "TAROT")]
        i, targets = self.pk.pick(pack, hand, 1, [], 5)
        self.assertEqual(i, 0)
        self.assertTrue(targets)
        self.assertIsNone(self.pk.pick([_card("c_chariot", "TAROT")], [], 1, [], 5))  # no hand, no plan
        self.assertEqual(self.pk.pick(pack, hand, 20, [], 5), (1, None))  # +$20 beats one Steel card


@NEEDS_SIM
class Use(unittest.TestCase):
    def test_hermit_in_shop_and_targeted_tarot_in_hand(self):
        import rollout_items
        from jackdaw.engine.actions import NextRound, SelectBlind
        from jackdaw.engine.card import Card
        from jackdaw.engine.game import step
        gs = _first_shop("ITEMUSE1")
        b = {"hand": "Pair", "suit": None}
        hermit, magician = Card(), Card()
        hermit.set_ability("c_hermit")
        magician.set_ability("c_magician")
        gs["consumables"] = [hermit, magician]
        money = gs["dollars"]
        rollout_items.use_items(gs, b, "SHOP", set())
        self.assertEqual(gs["dollars"], money + 20)
        self.assertEqual([c.center_key for c in gs["consumables"]], ["c_magician"])  # needs a hand
        step(gs, NextRound())
        step(gs, SelectBlind())
        rollout_items.use_items(gs, b, "SELECTING_HAND", set())
        self.assertEqual(gs["consumables"], [])
        self.assertEqual(sum(c.center_key == "m_lucky" for c in gs["hand"]), 2)


class DeathTables(unittest.TestCase):
    def test_death_stats_and_tables(self):
        import policy_lab
        d = lambda name, typ, target, final, best: policy_lab._death(  # noqa: E731
            {"name": name, "type": typ, "effect": "x", "score": target}, final, [best, final - best], ["Joker"])
        rows = [{"seed": "A", "arm": "x", "won": False, "ante": 6, "blinds": 16, "rerolls": 0, "reroll_spent": 0,
                 "spent": 0, "buys": [], "forecasts": 0, "forecast_s": 0, "seconds": 1,
                 "death": d("The Water", "BOSS", 1000, 800, 500)},
                {"seed": "B", "arm": "x", "won": False, "ante": 2, "blinds": 4, "rerolls": 0, "reroll_spent": 0,
                 "spent": 0, "buys": [], "forecasts": 0, "forecast_s": 0, "seconds": 1,
                 "death": d("Big Blind", "BIG", 1000, 200, 150)},
                {"seed": "C", "arm": "x", "won": True, "ante": 8, "blinds": 24, "rerolls": 0, "reroll_spent": 0,
                 "spent": 0, "buys": [], "forecasts": 0, "forecast_s": 0, "seconds": 1, "death": None}]
        s = policy_lab.death_stats(rows)
        self.assertEqual((s["losses"], s["at_boss"], s["at_big"], s["water"], s["ante_5_7"]), (2, 1, 1, 1, 1))
        self.assertEqual(s["shortfall"], {"<25%": 1, "25-50%": 0, "50-75%": 0, "75-100%": 1})
        self.assertEqual(s["median_best_hand_share"], round((0.5 + 0.15) / 2, 3))
        self.assertEqual(s["by_ante_blind"], {"2 big": 1, "6 boss": 1})
        out = {"arms": {"x": {"games": 3, "mean_ante": 5.3, "se_ante": 1.0, "win_rate": 0.33, "mean_blinds": 14.7,
                              "mean_forecasts": 0, "forecast_seconds_per_call": 0, "death_ante": {"2": 1, "6": 1, "won": 1},
                              "deaths": s}}, "paired": {}}
        text = policy_lab.tables(out)
        self.assertIn("| The Water | 1 |", text)
        self.assertIn("| deaths to The Water | 1 |", text)


class SequentialStop(unittest.TestCase):
    def test_stops_on_clear_difference_or_no_gain(self):
        import policy_lab
        mk = lambda arm, i, ante: {"seed": f"S{i}", "arm": arm, "ante": ante}  # noqa: E731
        big = [mk("f", i, 6 + i % 2) for i in range(100)] + [mk("e", i, 5 + (i % 3 == 0)) for i in range(100)]
        self.assertTrue(policy_lab.sequential_check(big, "f", "e")[0])
        same = [mk("f", i, 5 + i % 2) for i in range(100)] + [mk("e", i, 5 + (i % 3 == 0)) for i in range(100)]
        stop, why, d = policy_lab.sequential_check(same, "f", "e")
        self.assertFalse(stop, why)  # +0.17 +- 0.06: not > 3 SE, CI reaches above +0.15
        worse = [mk("f", i, 5 + (i % 4 == 0)) for i in range(100)] + [mk("e", i, 5 + (i % 3 == 0)) for i in range(100)]
        stop, why, d = policy_lab.sequential_check(worse, "f", "e")
        self.assertTrue(stop)
        self.assertIn("upper bound", why)


if __name__ == "__main__":
    unittest.main()
