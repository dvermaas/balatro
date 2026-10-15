"""Recovery must preserve completed seeds and require explicit victory evidence."""
import gzip
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from resume_benchmark import plan, validate_attachment


class ResumePlanTests(unittest.TestCase):
    def setUp(self):
        test_dir = Path(__file__).resolve().parent
        self.tmp = tempfile.TemporaryDirectory(prefix="resume-test-", dir=test_dir)
        assert Path(self.tmp.name).resolve().parent == test_dir
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def record(self, name="run", index=0, seed="7X7WSDCX", result="lost", won=False, version="test"):
        folder = self.root / name
        folder.mkdir()
        row = dict(run_id=name, prompt_version=version, seed=seed, result=result,
                   benchmark=dict(name="bench30-a", index=index, of=30),
                   deck="RED", stake="WHITE", model="jev-latest")
        (folder / "summary.json").write_text(json.dumps(row), encoding="utf-8")
        events = [dict(id=99, kind="round_end", ante=9, target=100000, scored=122980)] if won else []
        (folder / "decisions.jsonl").write_text("\n".join(json.dumps(e) for e in events), encoding="utf-8")
        with gzip.open(folder / "states.jsonl.gz", "wt", encoding="utf-8") as f:
            f.write(json.dumps(dict(id=100, gs=dict(seed=seed, won=won, ante_num=9, state="SHOP"))))
        return folder

    def test_existing_results_not_replayed_and_plan_does_not_write(self):
        self.record()
        folder = self.record("won", 2, "BAPQJMY5", "in_progress", True)
        before = {p.name: p.read_bytes() for p in folder.iterdir()}
        rows, recoveries, queue, settings = plan(self.root, "test", "bench30-a")
        self.assertEqual(set(rows), {0, 2})
        self.assertEqual(queue[0], (1, "6CAXN9I6"))
        self.assertEqual(len(queue), 28)
        self.assertEqual(len(recoveries), 1)
        self.assertEqual(settings, ("RED", "WHITE", "jev-latest"))
        self.assertEqual(before, {p.name: p.read_bytes() for p in folder.iterdir()})

    def test_high_ante_without_won_flag_is_not_a_result(self):
        self.record(result="in_progress")
        with self.assertRaisesRegex(ValueError, "no confirmed win"):
            plan(self.root, "test", "bench30-a")

    def test_duplicate_results_refused(self):
        self.record()
        self.record("duplicate")
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            plan(self.root, "test", "bench30-a")

    def test_wrong_seed_refused(self):
        self.record(seed="OTHER")
        with self.assertRaisesRegex(ValueError, "Invalid seed"):
            plan(self.root, "test", "bench30-a")

    def test_other_versions_do_not_suppress_seeds(self):
        self.record()
        self.record("older", 1, "6CAXN9I6", version="older")
        self.assertEqual(plan(self.root, "test", "bench30-a")[2][0], (1, "6CAXN9I6"))

    def test_explicit_attachment_is_not_replayed(self):
        self.record(result="in_progress")
        rows, recoveries, queue, _ = plan(self.root, "test", "bench30-a", "run")
        self.assertEqual(rows[0]["result"], "in_progress")
        self.assertEqual(recoveries, [])
        self.assertEqual(queue[0], (1, "6CAXN9I6"))

    def test_unknown_attachment_refused(self):
        self.record()
        with self.assertRaisesRegex(ValueError, "not an unfinished"):
            plan(self.root, "test", "bench30-a", "unknown")

    def test_attachment_requires_matching_post_discard_counters(self):
        row = dict(seed="S", deck="RED", stake="WHITE")
        events = [dict(kind="outcome", state="SELECTING_HAND", ante=2, round=6,
                       money=25, chips=0, hands_left=4, discards_left=3, jokers=[])]
        gs = dict(seed="S", deck="RED", stake="WHITE", state="SELECTING_HAND", ante_num=2,
                  round_num=6, money=25, round=dict(chips=0, hands_left=4, discards_left=3),
                  jokers=dict(cards=[]))
        validate_attachment(row, events, gs)
        gs["round"]["discards_left"] = 4
        with self.assertRaisesRegex(ValueError, "discards_left"):
            validate_attachment(row, events, gs)


if __name__ == "__main__":
    unittest.main()
