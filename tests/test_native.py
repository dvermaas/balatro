"""The Rust scoring kernel (native/, src/native.py) must match the Python scorer exactly.

Random hands x random joker sets (every parse() branch, tests/joker_effects.json) x hand levels x
boss filters: every subset row of hands.evaluated_plays, best_score and best_play must be identical
with the kernel on and off. Skips when jev_native isn't built (cd native && maturin develop --release).
Run: .venv/Scripts/python -m unittest discover -s tests -p test_native.py -v
"""
import json
import random
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
import decisions  # noqa: E402
import hands  # noqa: E402
import jokers  # noqa: E402
import native  # noqa: E402
import strategy  # noqa: E402

EFFECTS = json.loads((ROOT / "tests" / "joker_effects.json").read_text())
LEVELS = json.loads((ROOT / "docs/samples/sample_gamestate.json").read_text())["result"]["hands"]
RANKS = list("23456789TJQKA")
BOSSES = [None, None, "The Psychic", "The Eye", "The Mouth", "The Hook"]


def random_case(rng):
    """(infos, joker cards, hand levels, round, boss) with every modifier the scorer reads."""
    n = rng.randint(1, 9)
    infos = []
    for _ in range(n):
        stone = rng.random() < 0.06
        infos.append({
            "rank": None if stone and rng.random() < 0.5 else rng.choice(RANKS),
            "suit": rng.choice("HDCS"),
            "enh": "STONE" if stone else rng.choice([None] * 6 + ["WILD", "BONUS", "MULT", "GLASS", "STEEL", "GOLD", "LUCKY"]),
            "edition": rng.choice([None] * 8 + ["FOIL", "HOLO", "POLYCHROME", "NEGATIVE"]),
            "seal": rng.choice([None] * 6 + ["RED", "GOLD", "BLUE"]),
            "debuff": rng.random() < 0.08,
        })
    cards = []
    for _ in range(rng.randint(0, 6)):
        if rng.random() < 0.08:
            label, effect = rng.choice(["Blueprint", "Brainstorm"]), "Copies the ability of Joker to the right"
        else:
            label, effect = "J", rng.choice(EFFECTS)
        cards.append({"label": label, "value": {"effect": effect},
                      "modifier": {"edition": rng.choice([None] * 6 + ["FOIL", "HOLO", "POLYCHROME", "NEGATIVE"])}})
    levels = {}
    for name, h in LEVELS.items():
        if rng.random() < 0.05:
            continue  # a missing hand falls back to {} in score()
        lv = rng.randint(1, 6)
        levels[name] = {**h, "level": lv, "chips": h["chips"] + 15 * (lv - 1), "mult": h["mult"] + (lv - 1),
                        "played": rng.randint(0, 9), "played_this_round": rng.choice([0, 0, 0, 1, 2])}
    rnd = {"hands_left": rng.choice([None, 0, 1, 1, 2, 3, 4]), "discards_left": rng.choice([None, 0, 1, 2, 3])}
    return infos, cards, levels, rnd, rng.choice(BOSSES)


def run_case(case, enabled):
    infos, cards, levels, rnd, boss = case
    native.ENABLED = enabled
    scorer = jokers.Scorer(levels, jokers.build({}, cards), rnd)
    allowed = decisions.BossAllow(levels, boss) if boss else None
    rows = list(hands.evaluated_plays(infos, levels, scorer, allowed))
    best = hands.best_score(infos, scorer, allowed)
    play = hands.best_play(infos, levels, scorer)
    return rows, best, play


@unittest.skipUnless(native._rs is not None, "jev_native not built")
class NativeMatchesPython(unittest.TestCase):
    def tearDown(self):
        native.ENABLED = native._rs is not None

    def test_hand_names_match_strategy(self):
        self.assertEqual(native.HAND_NAMES, strategy.HAND_NAMES)

    def test_random_cases_identical(self):
        rng = random.Random(23023)
        rows_seen = 0
        for _ in range(3000):
            case = random_case(rng)
            py = run_case(case, False)
            rs = run_case(case, True)
            self.assertEqual(py, rs, case)
            rows_seen += len(py[0])
        self.assertGreater(rows_seen, 100000)

    def test_score_many_matches_score(self):
        rng = random.Random(31031)
        for _ in range(500):
            infos, cards, levels, rnd, _ = random_case(rng)
            native.ENABLED = True
            scorer = jokers.Scorer(levels, jokers.build({}, cards), rnd)
            plays = []
            for _ in range(rng.randint(1, 12)):
                combo = sorted(rng.sample(range(len(infos)), rng.randint(1, min(5, len(infos)))))
                plays.append((combo, *hands.classify([infos[p] for p in combo])))
            self.assertEqual(native.score_many(scorer, infos, plays),
                             [jokers.score(infos, c, n, sc, levels, scorer.jokers, rnd) for c, n, sc in plays])

    def test_kernel_is_used(self):
        case = random_case(random.Random(5))
        native.ENABLED = True
        scorer = jokers.Scorer(case[2], jokers.build({}, case[1]), case[3])
        self.assertIsNotNone(native.evaluate(scorer, case[0], None))

    def test_unusual_inputs_fall_back_to_python(self):
        native.ENABLED = True
        scorer = jokers.Scorer(LEVELS, [], {})
        good = {"rank": "A", "suit": "H", "enh": None, "edition": None, "seal": None, "debuff": False}
        self.assertIsNotNone(native.evaluate(scorer, [good], None))
        for bad in ({**good, "rank": "1"}, {**good, "suit": "X"}, {**good, "enh": []}, {"rank": "A"}):
            self.assertIsNone(native.evaluate(scorer, [good, bad], None), bad)
        self.assertIsNone(native.evaluate(lambda *a: 0, [good], None))  # plain callables stay Python
        self.assertIsNone(native.evaluate(scorer, [good], lambda combo, name: True))
        self.assertIsNone(jokers.Scorer({"Pair": []}, [], {}).native())


if __name__ == "__main__":
    unittest.main()
