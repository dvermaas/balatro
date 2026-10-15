import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from audit_wins import classify


class WinAuditTests(unittest.TestCase):
    def test_terminal_flag_does_not_override_failed_final_boss(self):
        events = [dict(id=1, kind="outcome", method="play", state="GAME_OVER", ante=8, round=24, chips=32942),
                  dict(id=2, kind="run_end", won=True)]
        states = [dict(seed="S", state="SELECTING_HAND", won=False, ante_num=8, round_num=24,
                       blinds=dict(boss=dict(status="CURRENT", score=300000, name="Violet Vessel")))]
        self.assertEqual(classify("S", events, states)["verdict"], "false_positive")

    def test_clear_and_continuation_verify_win_even_after_later_loss(self):
        events = [dict(id=1, kind="round_end", ante=9, round=24, scored=122980, target=100000),
                  dict(id=2, kind="outcome", method="play", state="GAME_OVER", ante=9, round=25, chips=0)]
        states = [dict(seed="S", state="SHOP", won=True, ante_num=9, round_num=24)]
        self.assertEqual(classify("S", events, states)["verdict"], "verified_win")

    def test_flag_alone_is_unverified(self):
        self.assertEqual(classify("S", [dict(kind="run_end", won=True)], [])['verdict'], "unverified")

    def test_other_seed_evidence_is_not_used(self):
        events = [dict(id=1, kind="round_end", ante=9, scored=100, target=100)]
        states = [dict(seed="OTHER", state="SHOP", won=True, ante_num=9)]
        self.assertEqual(classify("S", events, states)["verdict"], "unverified")


if __name__ == "__main__":
    unittest.main()
