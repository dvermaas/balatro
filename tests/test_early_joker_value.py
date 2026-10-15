"""Offline regressions for the early pack skips and missing item information in v15."""
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
import decisions
import strategy


def joker(key, label, effect, **modifier):
    return dict(key=key, label=label, set="JOKER", value={"effect": effect}, modifier=modifier,
                cost={"buy": 6, "sell": 3})


class Bot:
    def __init__(self):
        self.actions, self.decisions = [], []
        self._recent_seed, self._recent = "TEST", []

    def act(self, method, params=None):
        self.actions.append((method, params))

    def emit(self, *args, **kwargs):
        pass

    def record_decision(self, *args, **kwargs):
        self.decisions.append(args)


class EarlyJokerValueTests(unittest.TestCase):
    def pick(self, cards, scores, occupied=0):
        bot = Bot()
        gs = dict(seed="TEST", ante_num=1, round_num=1, money=5, hands={},
                  jokers={"count": occupied, "limit": 5, "cards": []}, pack={"cards": cards})
        with patch.object(decisions.build, "decide", return_value={"hand": "Pair"}), \
             patch.object(decisions, "_score_items", return_value=(scores, {})) as query:
            decisions.booster(bot, gs)
        return bot, query

    def test_recorded_riff_raff_keeps_restored_threshold(self):
        # V15 2RGEX6EN event 3569: empty lineup, Riff-raff 1.22/4, Square Joker 1.17/4.
        cards = [joker("j_square", "Square Joker", "This Joker gains +4 Chips if played hand has exactly 4 cards (Currently 0 Chips)"),
                 joker("j_riff_raff", "Riff-raff", "When Blind is selected, create 2 Common Jokers (Must have room)")]
        bot, query = self.pick(cards, {"take_0": 1.17, "take_1": 1.22})
        self.assertEqual(bot.actions, [("pack", {"skip": True})])
        state, items = query.call_args.args[3:5]
        self.assertIn("costs $0", state["pack_selection"])
        self.assertIn("joker generation", items["take_1"][1]["measured_gain"])

    def test_recorded_burglar_keeps_restored_threshold(self):
        cards = [joker("j_burglar", "Burglar", "When Blind is selected, gain +3 Hands and lose all discards"),
                 joker("j_superposition", "Superposition", "Create a Tarot card if poker hand contains an Ace and a Straight (Must have room)")]
        bot, _ = self.pick(cards, {"take_0": 1.37, "take_1": .52})
        self.assertEqual(bot.actions, [("pack", {"skip": True})])

    def test_joker_at_original_threshold_is_taken(self):
        bot, _ = self.pick([joker("j_burglar", "Burglar", "Gain +3 Hands and lose all discards")], {"take_0": 1.5})
        self.assertEqual(bot.actions, [("pack", {"card": 0})])

    def test_low_jev_rating_still_skips(self):
        bot, _ = self.pick([joker("j_flash", "Flash Card", "+0 Mult")], {"take_0": .7})
        self.assertEqual(bot.actions, [("pack", {"skip": True})])

    def test_no_relaxed_threshold_for_rentals_eternals_or_destruction(self):
        for card in [joker("j_flash", "Flash Card", "+0 Mult", rental=True),
                     joker("j_flash", "Flash Card", "+0 Mult", eternal=True),
                     joker("j_ceremonial", "Ceremonial Dagger", "When Blind is selected, destroys Joker to the right")]:
            with self.subTest(card=card):
                bot, _ = self.pick([card], {"take_0": 1.2})
                self.assertEqual(bot.actions, [("pack", {"skip": True})])

    def test_full_slots_do_not_take_another_joker(self):
        bot, query = self.pick([joker("j_flash", "Flash Card", "+0 Mult")], {"take_0": 3}, occupied=5)
        self.assertEqual(bot.actions, [("pack", {"skip": True})])
        query.assert_not_called()

    def test_misprint_blank_text_has_range_without_guaranteed_gain(self):
        facts = decisions._gain({}, [], joker("j_misprint", "Misprint", "  +"))
        self.assertIn("0 to 23 Mult", facts["known_effect"])
        self.assertNotIn("_gain", facts)
        self.assertIn("not guaranteed", facts["known_effect"])

    def test_scoring_support_is_not_ranked_below_unconditional_xmult(self):
        missing = strategy.engine([])["missing"]
        self.assertIn("+Mult", missing[0])
        self.assertFalse(any("biggest gap" in s for s in missing))
        self.assertIn("activates", missing[-1])


if __name__ == "__main__":
    unittest.main()
