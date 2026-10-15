"""Regression checks for cash arithmetic, JEV's veto and actual buy/use sequencing."""
import copy
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
import decisions
import tarots
from balatro import RPCError


def cash_card(key="c_hermit", cost=3):
    return {"key": key, "label": "The Hermit" if key == "c_hermit" else "Temperance", "set": "TAROT",
            "value": {"effect": "Doubles money"}, "cost": {"buy": cost}}


def state(money=21, card=None):
    return {"seed": "REGRESSION", "state": "SHOP", "money": money, "ante_num": 2, "round_num": 4,
            "hands": {}, "jokers": {"count": 0, "limit": 5, "cards": []},
            "consumables": {"count": 0, "limit": 2, "cards": []},
            "shop": {"cards": [card or cash_card()]}}


class Bot:
    def __init__(self, take=True, item_score=.9):
        self._shop_key = self._recent_seed = None
        self._shop_actions = self._rerolls = self._last_pack_cost = 0
        self._failed, self._recent = set(), []
        self.take, self.item_score = take, item_score
        self.actions, self.recorded, self.queries = [], [], []
        self.bought = {"consumables": {"cards": [{"key": "c_fool"}, cash_card()]}}

    def query(self, gs, state, questions):
        self.queries.append((copy.deepcopy(state), questions))
        answers = {k: SimpleNamespace(score=self.item_score) for k in questions if k != "cash_step"}
        if "cash_step" in questions:
            answers["cash_step"] = SimpleNamespace(probabilities={"buy_card_0": .8 if self.take else .2,
                                                                 "continue_shopping": .2 if self.take else .8})
        return answers, {}

    def emit(self, *args, **kwargs):
        pass

    def record_decision(self, *args, **kwargs):
        self.recorded.append((args, kwargs))

    def act(self, method, params=None):
        self.actions.append((method, params))
        return self.bought if method == "buy" else {}


class CashTransactionTests(unittest.TestCase):
    def shop(self, gs, bot):
        with patch.object(decisions.build, "decide", return_value={"hand": "Pair"}), \
             patch.object(decisions.readiness, "assess", return_value=None):
            decisions.shop(bot, gs)

    def test_hermit_subtracts_price_before_doubling(self):
        self.assertEqual(tarots.cash_outcome(cash_card(), 21, 3), {
            "cash_before": 21, "purchase_cost": 3, "cash_after_purchase": 18,
            "payout_if_used_now": 18, "net_cash_gain": 15, "cash_after_use": 36})

    def test_caps_and_low_cash(self):
        for money, cost, payout, profit in [(50, 3, 20, 17), (5, 3, 2, -1), (6, 3, 3, 0), (-5, 0, 0, 0)]:
            with self.subTest(money=money):
                o = tarots.cash_outcome(cash_card(), money, cost)
                self.assertEqual((o["payout_if_used_now"], o["net_cash_gain"]), (payout, profit))

    def test_temperance_values_and_cap(self):
        c = cash_card("c_temperance")
        self.assertEqual(tarots.cash_outcome(c, 37, 3, [{"cost": {"sell": 9}}])["net_cash_gain"], 6)
        self.assertEqual(tarots.cash_outcome(c, 37, 3, [{"cost": {"sell": 99}}])["payout_if_used_now"], 50)
        self.assertEqual(tarots.cash_outcome(c, 37, 3, [])["net_cash_gain"], -3)
        self.assertIsNone(tarots.cash_outcome({"key": "c_death"}, 20))

    def test_recorded_profitable_case_buys_and_uses_actual_slot(self):
        gs, bot = state(), Bot()
        before = copy.deepcopy(gs)
        self.shop(gs, bot)
        self.assertEqual(bot.actions, [("buy", {"card": 0}), ("use", {"consumable": 1})])
        self.assertEqual(gs, before)
        qs = bot.queries[0][1]
        item = qs["buy_card_0"].instructions["item"]
        self.assertEqual(item["cash_outcome"]["cash_after_use"], 36)
        self.assertEqual(item["cash_outcome"]["interest_after_use"], 5)
        self.assertIn("+$18", item["use"])
        self.assertEqual(bot.recorded[0][0][3], ("buy_cash", {"card": 0, "key": "c_hermit"}))

    def test_jev_can_decline_even_when_item_rating_is_high(self):
        bot = Bot(take=False, item_score=4)
        self.shop(state(), bot)
        self.assertEqual(bot.actions, [("next_round", None)])

    def test_loss_and_break_even_do_not_get_profit_question(self):
        for money in (5, 6):
            with self.subTest(money=money):
                bot = Bot()
                self.shop(state(money), bot)
                self.assertNotIn("cash_step", bot.queries[0][1])
                self.assertEqual(bot.actions, [("next_round", None)])

    def test_full_consumables_cannot_offer_cash_purchase(self):
        gs, bot = state(), Bot()
        gs["consumables"].update(count=2, cards=[{"key": "c_fool"}, {"key": "c_death"}])
        self.shop(gs, bot)
        self.assertFalse(bot.queries)
        self.assertEqual(bot.actions, [("next_round", None)])

    def test_missing_acquired_card_never_uses_an_unrelated_consumable(self):
        bot = Bot()
        bot.bought = {"consumables": {"cards": [{"key": "c_fool"}]}}
        with self.assertRaises(RPCError):
            self.shop(state(), bot)
        self.assertEqual(bot.actions, [("buy", {"card": 0})])
        self.assertTrue(bot._failed)

    def test_riff_raff_capacity_and_replacement(self):
        card = {"key": "j_riff_raff", "label": "Riff-raff", "set": "JOKER",
                "value": {"effect": "When Blind is selected, create 2 Common Jokers (Must have room)"}}
        for count, remove, negative, expected in [(0, None, False, 2), (3, None, False, 1),
                                                  (4, None, False, 0), (5, 0, False, 0),
                                                  (4, None, True, 1)]:
            with self.subTest(count=count, remove=remove, negative=negative):
                gs = state()
                gs["jokers"].update(count=count, cards=[{} for _ in range(count)])
                c = {**card, "modifier": {"edition": "NEGATIVE" if negative else None}}
                f = decisions._gain(gs, [], c, remove)
                self.assertEqual(f["next_blind_generation"]["common_jokers_created"], expected)
                self.assertNotIn("_gain", f)  # Unknown future Common jokers are not guaranteed score.


if __name__ == "__main__":
    unittest.main()
