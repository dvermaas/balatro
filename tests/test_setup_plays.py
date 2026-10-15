"""Offline v17 checks: no game/API calls and no knowledge of actual future draws."""
import copy
import json
import random
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
import decisions
import hands
import jokers


def fixture():
    return json.loads((ROOT / "docs/samples/v17-setup-state.json").read_text())["gs"]


def candidates(gs):
    hand = gs["hand"]["cards"]
    info = decisions._round_info(gs)
    levels = decisions._boss_levels(gs["hands"], info["blind"])
    js = jokers.build(gs)
    scorer = lambda cs, combo, name, sc: jokers.score(cs, combo, name, sc, levels, js, gs["round"])
    allowed = lambda combo, name: decisions._boss_allows(combo, name, gs["hands"], info["blind"])
    original = hands.play_candidates(hand, levels, scorer=scorer, allowed=allowed)
    result = decisions._setup_plays(gs, hand, original, levels, scorer, allowed, info, 1.1)
    return original, result


def set_boss(gs, name):
    for blind in gs["blinds"].values():
        if blind.get("status") == "CURRENT":
            blind["name"] = name


class Captured(Exception):
    pass


class Bot:
    def query(self, gs, state, questions):
        self.state, self.questions = state, questions
        raise Captured

    def emit(self, *args, **kwargs):
        pass

    def act(self, method, params):
        self.action = method, params


class SetupTests(unittest.TestCase):
    def test_recorded_water_failure_exposes_four_heart_setup(self):
        gs = fixture()
        before = copy.deepcopy(gs)
        original, result = candidates(gs)
        setups = [p for p in result if p.get("setup") and "Hearts" in p["setup"]]
        self.assertTrue(setups)
        best = max(setups, key=lambda p: p["outlook"]["average_two_play_total"])
        self.assertEqual(best["estimate"], 15)
        self.assertEqual(len(best["outlook"]["keeps"]), 4)
        self.assertTrue(all("Hearts" in c for c in best["outlook"]["keeps"]))
        strongest_now = result[0]
        self.assertGreater(best["outlook"]["average_two_play_total"],
                           strongest_now["outlook"]["average_two_play_total"] * 1.5)
        self.assertEqual([p["cards"] for p in result[:len(original)]], [p["cards"] for p in original])
        self.assertEqual(gs, before)

    def test_complete_candidates_and_forecasts_ignore_pile_order(self):
        gs = fixture()
        expected = candidates(gs)[1]
        for seed in range(3):
            random.Random(seed).shuffle(gs["cards"]["cards"])
            self.assertEqual(candidates(gs)[1], expected)

    def test_setup_and_forecast_are_sent_to_jev(self):
        bot = Bot()
        with self.assertRaises(Captured):
            decisions.selecting_hand(bot, fixture())
        options = bot.questions["best_play"].criteria.values()
        self.assertTrue(any(p.get("setup") for p in options))
        self.assertTrue(all("next_hand_outlook" in p for p in options))
        self.assertNotIn("play_now", bot.questions)

    def test_last_hand_discards_and_unsupported_transitions_keep_normal_plays(self):
        for change in (lambda g: g["round"].update(hands_left=1),
                       lambda g: g["round"].update(discards_left=1),
                       lambda g: set_boss(g, "The Hook"),
                       lambda g: set_boss(g, "The Arm"),
                       lambda g: g["hand"]["cards"][0]["state"].update(hidden=True),
                       lambda g: g["jokers"]["cards"].append({"label": "Green Joker", "value": {"effect": "+1 Mult per hand played (Currently +5 Mult)"}})):
            gs = fixture()
            change(gs)
            with self.subTest(gs=gs["round"]):
                original, result = candidates(gs)
                self.assertEqual(result, original)

    def test_sure_clear_bypasses_all_speculative_forecasts(self):
        gs = fixture()
        gs["round"]["chips"] = 1599
        bot = Bot()
        with patch.object(decisions, "_setup_plays", side_effect=AssertionError("must not forecast")):
            decisions.selecting_hand(bot, gs)
        self.assertEqual(bot.action[0], "play")

    def test_psychic_rejects_short_setup_plays(self):
        gs = fixture()
        set_boss(gs, "The Psychic")
        _, result = candidates(gs)
        self.assertTrue(all(len(p["cards"]) == 5 for p in result))

    def test_next_hand_spends_hand_and_updates_history_for_supernova(self):
        gs = fixture()
        gs["round"]["hands_left"] = 2
        gs["jokers"]["cards"] = [{"label": "Acrobat", "value": {"effect": "X3 Mult on final hand of round"}},
                                 {"label": "Supernova", "value": {"effect": "Adds the number of times poker hand has been played to Mult"}}]
        seen = []

        def inspect(plans, infos, deck, scorer_factory, allowed, **kw):
            combo = plans[0]["cards"]
            name, _ = hands.classify([infos[i] for i in combo])
            # Model playing exactly the same hand type next turn to expose its incremented history.
            probe = scorer_factory([], 0)
            _, sc = hands.classify([infos[i] for i in combo])
            levels = copy.deepcopy(gs["hands"])
            levels[name]["played"] = (levels[name].get("played") or 0) + 1
            expected = jokers.score(infos, combo, name, sc, levels, jokers.build(gs), {**gs["round"], "hands_left": 1})
            self.assertEqual(probe(infos, combo, name, sc), expected)
            seen.append(name)
            plans[0].update(expected_score=expected, low_score=expected, chance_to_clear=0,
                            drawn=0, samples=1, exact_draws=True)
        with patch.object(decisions.odds, "forecast", side_effect=inspect):
            candidates(gs)
        self.assertTrue(seen)

    def test_eye_and_mouth_advance_the_played_hand_restriction(self):
        for boss in ("The Eye", "The Mouth"):
            gs = fixture()
            set_boss(gs, boss)
            for h in gs["hands"].values():
                h["played_this_round"] = 0
            seen = []

            def inspect(plans, infos, deck, scorer_factory, allowed, **kw):
                combo = plans[0]["cards"]
                name, _ = hands.classify([infos[i] for i in combo])
                self.assertEqual(allowed(combo, name), boss == "The Mouth")
                other = "Flush" if name != "Flush" else "Pair"
                self.assertEqual(allowed(combo, other), boss == "The Eye")
                seen.append(name)
                plans[0].update(expected_score=0, low_score=0, chance_to_clear=0,
                                drawn=0, samples=1, exact_draws=True)
            with patch.object(decisions.odds, "forecast", side_effect=inspect):
                candidates(gs)
            self.assertTrue(seen)

    def test_blue_joker_tracks_draws_without_mutating_state(self):
        gs = fixture()
        gs["jokers"]["cards"] = [{"label": "Blue Joker", "value": {"effect": "+2 Chips for each remaining card in deck (Currently +60 Chips)"}}]
        before = copy.deepcopy(gs)
        js = jokers.after_play(gs, 4)
        self.assertEqual(js[0]["rule"]["hand"]({}), (2 * (len(gs["cards"]["cards"]) - 4), 0, 1))
        self.assertEqual(gs, before)

    def test_serpent_forecasts_three_draws_after_short_plays(self):
        gs = fixture()
        set_boss(gs, "The Serpent")
        _, result = candidates(gs)
        self.assertTrue(any("outlook" in p for p in result))
        self.assertTrue(all(p["outlook"]["cards_drawn"] == 3 for p in result))

    def test_unsupported_state_keeps_original_question(self):
        gs = fixture()
        set_boss(gs, "The Hook")
        bot = Bot()
        with self.assertRaises(Captured):
            decisions.selecting_hand(bot, gs)
        self.assertEqual(bot.questions["best_play"].instructions, decisions.INSTRUCTIONS["best_play"])
        self.assertTrue(all("next_hand_outlook" not in p for p in bot.questions["best_play"].criteria.values()))


if __name__ == "__main__":
    unittest.main()
