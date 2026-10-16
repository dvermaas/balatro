"""Offline mechanics/fairness regressions; production remains on frozen v18."""
import copy
import hashlib
import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / "src"), str(ROOT / "scripts")]
import hands
from diagnostics.mechanics_candidate import candidate_module
from diagnostics.positions import public_observation

BANK = ROOT / "docs/samples/diagnostic-positions-v1.json"
BANK_SHA = "beefe3ef4a3234eb7062d8f56032c1329f8042cb830b2b3e6349cae84174f072"  # LF bytes
POSITIONS = json.loads(BANK.read_text(encoding="utf-8"))["positions"]


def card(rank, suit="H", enhancement=None, debuff=False, seal=None):
    return {"rank": rank, "suit": suit, "enh": enhancement, "debuff": debuff, "edition": None, "seal": seal}


def joker(label, text, debuff=False):
    return {"label": label, "value": {"effect": text}, "state": {"debuff": debuff}, "modifier": {}}


FIST = joker("Raised Fist", "Adds double the rank of lowest ranked card held in hand to Mult")
PAREIDOLIA = joker("Pareidolia", "All cards are considered face cards")
SMILEY = joker("Smiley Face", "Played face cards give +5 Mult when scored")
PHOTO = joker("Photograph", "First played face card gives X2 Mult when scored")


class CandidateMechanicsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scorer = candidate_module()

    def score(self, held=(), jokers=(FIST,), played=None):
        played = played or [card("A"), card("A", "S")]
        infos = played + list(held)
        name, sc = hands.classify(played)
        return self.scorer.score(infos, list(range(len(played))), name, sc,
            POSITIONS[0]["observation"]["hands"] | {"Pair": {"chips": 10, "mult": 2}, "High Card": {"chips": 5, "mult": 1}},
            self.scorer.build({}, list(jokers)), {"hands_left": 2, "discards_left": 1})

    def test_debuffed_lowest_does_not_fall_back_to_higher_card(self):
        self.assertEqual(self.score([card("2", debuff=True), card("K")]), 64)

    def test_last_equal_rank_is_the_trigger(self):
        self.assertEqual(self.score([card("2"), card("2", "S", debuff=True)]), 64)
        self.assertEqual(self.score([card("2", debuff=True), card("2", "S")]), 192)

    def test_rank_not_chip_value_selects_lowest(self):
        # Jack beats Queen/King even though all three have nominal value 10.
        self.assertEqual(self.score([card("J", debuff=True), card("Q"), card("K")]), 64)

    def test_stone_excluded_and_empty_held_safe(self):
        self.assertEqual(self.score([card("2", enhancement="STONE"), card("K")]), 704)
        self.assertEqual(self.score([card("2", enhancement="STONE")]), 64)
        self.assertEqual(self.score([]), 64)

    def test_held_order_with_steel_changes_score(self):
        self.assertEqual(self.score([card("2"), card("K", enhancement="STEEL")]), 288)
        self.assertEqual(self.score([card("K", enhancement="STEEL"), card("2")]), 224)

    def test_fist_retriggers_on_red_seal_and_mime(self):
        self.assertEqual(self.score([card("2", seal="RED")]), 320)
        mime = joker("Mime", "Retrigger all card held in hand abilities")
        self.assertEqual(self.score([card("2")], [FIST, mime]), 320)

    def test_pareidolia_respects_joker_order(self):
        pair = [card("9"), card("9", "S")]
        self.assertEqual(self.score(jokers=[PAREIDOLIA, SMILEY, PHOTO], played=pair), 532)
        self.assertEqual(self.score(jokers=[PAREIDOLIA, PHOTO, SMILEY], played=pair), 392)

    def test_pareidolia_enables_face_retriggers(self):
        sock = joker("Sock and Buskin", "Retrigger all played face cards")
        self.assertEqual(self.score(jokers=[PAREIDOLIA, SMILEY, sock], played=[card("9"), card("9", "S")]), 1012)

    def test_debuffed_pareidolia_does_not_enable_faces(self):
        disabled = copy.deepcopy(PAREIDOLIA)
        disabled["state"]["debuff"] = True
        self.assertEqual(self.score(jokers=[disabled, SMILEY, PHOTO], played=[card("9"), card("9", "S")]), 56)

    def test_scoring_does_not_mutate_input_or_accumulate_face_flags(self):
        infos = [card("9"), card("9", "S")]
        before = copy.deepcopy(infos)
        self.score(jokers=[PAREIDOLIA, SMILEY], played=infos)
        self.assertEqual(infos, before)
        self.assertEqual(self.score(jokers=[SMILEY], played=infos), 56)


class ObservationBoundaryTests(unittest.TestCase):
    def test_frozen_selection_is_not_silently_changed(self):
        self.assertEqual(hashlib.sha256(BANK.read_bytes().replace(b"\r\n", b"\n")).hexdigest(), BANK_SHA)
        self.assertEqual(len(POSITIONS), 40)

    def test_draw_pile_is_never_accessed(self):
        class PublicOnly(dict):
            def get(self, key, default=None):
                if key in {"cards", "seed", "rng", "future_shop"}:
                    raise AssertionError("Private information accessed")
                return super().get(key, default)

            def __getitem__(self, key):
                if key in {"cards", "seed", "rng", "future_shop"}:
                    raise AssertionError("Private information accessed")
                return super().__getitem__(key)
        observed = public_observation(PublicOnly(POSITIONS[0]["observation"]))
        self.assertNotIn("seed", observed)
        self.assertNotIn("cards", observed)


@unittest.skipUnless((ROOT / "runs/_research/jackdaw/jackdaw").exists(), "Pinned optional Jackdaw research checkout absent")
class ReferenceAdapterTests(unittest.TestCase):
    def test_recorded_actual_scores_and_explicit_exclusions(self):
        from diagnostics.jackdaw_reference import Unsupported, evaluate, verify_checkout
        verify_checkout()
        matches, exclusions = [], []
        for p in POSITIONS[:30]:
            with self.subTest(case=p["case_id"]):
                try:
                    result = evaluate(p["observation"], p["recorded"]["action"][1]["cards"])
                except Unsupported:
                    exclusions.append(p["case_id"])
                else:
                    self.assertEqual(result["total"], p["actual"]["chips_delta"])
                    matches.append(p["case_id"])
        self.assertEqual(len(matches), 27)
        self.assertEqual(exclusions, ["D03", "D14", "D18"])

    def test_fresh_state_per_evaluation(self):
        from diagnostics.jackdaw_reference import evaluate
        p = copy.deepcopy(POSITIONS[0])
        before = copy.deepcopy(p)
        first = evaluate(p["observation"], p["recorded"]["action"][1]["cards"])
        self.assertEqual(evaluate(p["observation"], p["recorded"]["action"][1]["cards"]), first)
        self.assertEqual(p, before)

    def test_unknown_joker_and_hidden_card_fail_closed(self):
        from diagnostics.jackdaw_reference import Unsupported, evaluate
        p = copy.deepcopy(POSITIONS[0])
        p["observation"]["jokers"]["cards"][0]["key"] = "j_not_adapted"
        with self.assertRaises(Unsupported):
            evaluate(p["observation"], [0])
        p = copy.deepcopy(POSITIONS[0])
        p["observation"]["hand"]["cards"][0]["state"]["face_down"] = True
        with self.assertRaises(Unsupported):
            evaluate(p["observation"], [0])

    def test_invalid_action_is_rejected(self):
        from diagnostics.jackdaw_reference import evaluate
        for indices in ([], [0, 0], [-1], [8], [0, 1, 2, 3, 4, 5]):
            with self.subTest(indices=indices), self.assertRaises(ValueError):
                evaluate(POSITIONS[0]["observation"], indices)


if __name__ == "__main__":
    unittest.main()
