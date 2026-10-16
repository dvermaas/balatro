"""Offline regressions for v12. Run: .venv/Scripts/python -m unittest discover -s tests -v"""
import copy
import json
import random
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
import decisions
import hands
import jokers
import odds

ROOT = Path(__file__).resolve().parents[1]
LEVELS = json.loads((ROOT / "docs/samples/sample_gamestate.json").read_text())["result"]["hands"]


def card(rank, suit="H", enh=None):
    return {"value": {"rank": rank, "suit": suit}, "modifier": {"enhancement": enh}, "state": {}}


def joker(label, effect):
    return {"label": label, "value": {"effect": effect}, "modifier": {}}


def score_with(cards=(), rnd=None):
    js = jokers.build({}, list(cards))
    return lambda infos, combo, name, sc: jokers.score(
        infos, combo, name, sc, LEVELS, js, rnd or {"hands_left": 3, "discards_left": 2})


class CandidateTests(unittest.TestCase):
    def setUp(self):
        self.hand = [card("A", "H"), card("A", "D"), card("2", "C"), card("3", "S"),
                     card("5", "H"), card("7", "D"), card("9", "C"), card("K", "S")]

    def test_half_joker_short_play_survives_pruning(self):
        scorer = score_with([joker("Half Joker", "+20 Mult if played hand contains 3 or fewer cards")])
        best = hands.play_candidates(self.hand, LEVELS, scorer=scorer)[0]
        self.assertEqual(best["estimate"], 704)  # (10 + 11 + 11) * (2 + 20)
        self.assertLessEqual(len(best["cards"]), 3)

    def test_steel_stays_held_instead_of_becoming_padding(self):
        self.hand[2] = card("2", "C", "STEEL")
        best = hands.play_candidates(self.hand, LEVELS, scorer=score_with())[0]
        self.assertEqual(best["estimate"], 96)
        self.assertNotIn(2, best["cards"])
        plan = decisions._preserve_discard(self.hand, best, score_with())
        self.assertNotIn(2, plan["cards"])
        self.assertTrue(set(best["cards"][:2]).isdisjoint(plan["cards"]))

    def test_psychic_filters_before_short_play_dominance(self):
        scorer = score_with([joker("Half Joker", "+20 Mult if played hand contains 3 or fewer cards")])
        allow = lambda combo, name: decisions._boss_allows(combo, name, LEVELS, "The Psychic")
        plays = hands.play_candidates(self.hand, LEVELS, scorer=scorer, allowed=allow)
        self.assertTrue(plays)
        self.assertTrue(all(len(p["cards"]) == 5 for p in plays))
        self.assertEqual(plays[0]["estimate"], 64)

    def test_plain_ties_still_cycle_junk(self):
        best = hands.play_candidates(self.hand, LEVELS, scorer=score_with())[0]
        self.assertEqual(best["estimate"], 64)
        self.assertEqual(len(best["cards"]), 5)


class ForecastTests(unittest.TestCase):
    def forecast(self, hand, deck, toss, **kwargs):
        plans = [{"cards": toss}]
        odds.forecast(plans, [hands.card_info(c) for c in hand], [hands.card_info(c) for c in deck],
                      lambda discarded, drawn: score_with(), **kwargs)
        return plans[0]

    def test_exact_enumeration_counts_misses(self):
        result = self.forecast([card("A", "C"), card("7", "H")],
                               [card("A", "H"), card("2", "D")], [1], need=60, pace=40)
        self.assertEqual(result["samples"], 2)
        self.assertTrue(result["exact_draws"])
        self.assertEqual(result["expected_score"], 40)  # half Pair 64, half High Card 16
        self.assertEqual(result["low_score"], 16)
        self.assertEqual(result["chance_to_clear"], .5)
        self.assertEqual(result["chance_to_meet_pace"], .5)

    def test_held_steel_is_in_future_score(self):
        result = self.forecast([card("A", "C"), card("A", "H"), card("2", "S", "STEEL"), card("7")],
                               [card("3", "D"), card("5", "C")], [3])
        self.assertEqual(result["expected_score"], 96)

    def test_sampled_forecast_is_pile_order_invariant(self):
        hand = [card("A", "C"), card("7", "H"), card("9", "D")]
        deck = [card(r, s, enh) for r, s, enh in [("A", "H", None), ("2", "D", None),
                ("A", "H", "MULT"), ("3", "S", None), ("K", "C", "GLASS"), ("5", "D", None)]]
        original = copy.deepcopy(deck)
        expected = self.forecast(hand, deck, [1, 2], samples=8)
        self.assertFalse(expected["exact_draws"])
        for seed in range(5):
            random.Random(seed).shuffle(deck)
            self.assertEqual(self.forecast(hand, deck, [1, 2], samples=8), expected)
        self.assertEqual(sorted(map(json.dumps, deck)), sorted(map(json.dumps, original)))

    def test_duplicate_cards_keep_their_probability_mass(self):
        result = self.forecast([card("A", "C"), card("7")],
                               [card("A", "H"), card("A", "H"), card("2", "D")], [1], need=60)
        self.assertEqual(result["chance_to_clear"], 2 / 3)
        self.assertEqual(result["expected_score"], 48)

    def test_empty_deck_and_unscorable_boss(self):
        result = self.forecast([card("A"), card("7")], [], [1])
        self.assertEqual(result["drawn"], 0)
        self.assertEqual(result["expected_score"], 16)
        result = self.forecast([card("A"), card("7")], [], [1], allowed=lambda c, n: False)
        self.assertEqual(result["expected_score"], 0)

    def test_serpent_draws_three_even_after_one_discard(self):
        hand = [hands.card_info(card("2")) for _ in range(8)]
        deck = [hands.card_info(card("3")) for _ in range(4)]
        plans = [{"cards": [0]}]
        odds.forecast(plans, hand, deck, lambda discarded, drawn: lambda cs, combo, n, sc: len(cs), draw_count=3)
        self.assertEqual(plans[0]["drawn"], 3)
        self.assertEqual(plans[0]["expected_score"], 10)

    def test_eye_and_mouth_use_round_history(self):
        levels = {**LEVELS, "Pair": {**LEVELS["Pair"], "played_this_round": 1}}
        self.assertFalse(decisions._boss_allows([0, 1], "Pair", levels, "The Eye"))
        self.assertTrue(decisions._boss_allows([0], "High Card", levels, "The Eye"))
        self.assertTrue(decisions._boss_allows([0, 1], "Pair", levels, "The Mouth"))
        self.assertFalse(decisions._boss_allows([0], "High Card", levels, "The Mouth"))


class IntegrationTests(unittest.TestCase):
    def test_spent_discard_changes_banner_and_mystic_summit(self):
        base = {"seed": "OFFLINE", "state": "SELECTING_HAND", "hands": LEVELS,
                "round": {"hands_left": 3, "discards_left": 1, "chips": 0},
                "blinds": {"small": {"name": "Small Blind", "status": "CURRENT", "score": 10000}},
                "hand": {"cards": [card("A", "C"), card("A", "H"), card("7", "D")], "limit": 3},
                "cards": {"cards": [card("2", "D")]}}

        class Captured(Exception):
            pass

        class Bot:
            def query(self, gs, state, questions):
                self.forecast = state["best_draw_by_measurement"]
                raise Captured()

        for j, expected in [(joker("Banner", "+30 Chips for each remaining discard"), 64),
                             (joker("Mystic Summit", "+15 Mult when 0 discards remaining"), 544)]:
            bot = Bot()
            with self.assertRaises(Captured):
                decisions.selecting_hand(bot, {**base, "jokers": {"cards": [j]}})
            self.assertEqual(bot.forecast["modeled_average_next_play"], expected)

    def test_discard_effects_and_snapshot_immutability(self):
        gs = {"cards": {"cards": [card("2")] * 10}, "jokers": {"cards": [
            joker("Blue Joker", "+2 Chips for each remaining card in deck (Currently +20 Chips)"),
            joker("Ramen", "X2 Mult, loses X0.01 Mult per card discarded"),
            joker("Green Joker", "+1 Mult per hand played -1 Mult per discard (Currently +5 Mult)")]}}
        original = copy.deepcopy(gs)
        js = jokers.after_discard(gs, [hands.card_info(card("2"))] * 2, 3)
        ctx = {}
        self.assertEqual(jokers._hand(js[0]["rule"]["hand"], ctx), (14, 0, 1))
        self.assertEqual(jokers._hand(js[1]["rule"]["hand"], ctx), (0, 0, 1.98))
        self.assertEqual(jokers._hand(js[2]["rule"]["hand"], ctx), (0, 4, 1))
        self.assertEqual(gs, original)

    def test_both_questions_receive_the_forecast_without_network(self):
        gs = {"seed": "OFFLINE", "state": "SELECTING_HAND", "hands": LEVELS,
              "round": {"hands_left": 3, "discards_left": 2, "chips": 0},
              "blinds": {"small": {"name": "Small Blind", "status": "CURRENT", "score": 1000}},
              "hand": {"cards": [card("A", "C"), card("7", "H")], "limit": 2},
              "cards": {"cards": [card("A", "H"), card("2", "D")]}, "jokers": {"cards": []}}

        class Bot:
            def query(self, gs, state, questions):
                self.sent, self.questions = state, questions
                return {k: SimpleNamespace(noul=.1, probabilities={next(iter(q.criteria)): 1})
                        for k, q in questions.items()}, {}

            def record_decision(self, *args, **kwargs):
                pass

            def emit(self, *args, **kwargs):
                pass

            def act(self, method, params):
                self.action = method, params

        bot = Bot()
        decisions.selecting_hand(bot, gs)
        self.assertIn("play_now", bot.questions)
        self.assertIn("modeled_average_next_play", bot.sent["best_draw_by_measurement"])
        self.assertEqual(bot.action[0], "discard")
        if "best_discard" in bot.questions:
            self.assertTrue(all("modeled_average_next_play" in p
                                for p in bot.questions["best_discard"].criteria.values()))


if __name__ == "__main__":
    unittest.main()
