"""Fixture for the Rust JevBot::clearly_worse: fabricated benchmark runs in a temp root, then Python's
JevBot._clearly_worse on them (stub recorder). Writes native/bot/tests/data/clearly_worse.json.
Each case: {"set", "versions": [{"id", "progress": [per-seed rounds won, in set order]}], "me": id, "expected"}.
The Rust test builds the same temp root with the same helper layout (summary.json + decisions.jsonl)."""
import json
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path[:0] = [str(ROOT / "src")]
import benchmark, recorder, analytics, jev_bot  # noqa: E402

SET = "bench30-a"
SEEDS = benchmark.SETS[SET]
A, B = "aaaa0001", "bbbb0002"


def case(me_prog, base_prog):
    return {"set": SET, "me": B, "versions": [{"id": A, "progress": base_prog}, {"id": B, "progress": me_prog}]}


CASES = [
    case([3] * 10, [6] * 30),                                   # clearly worse after 10 seeds
    case([3] * 9, [6] * 30),                                    # too few seeds
    case([5, 6, 7, 6, 5, 6, 8, 5, 6, 6], [6] * 30),             # not significant
    case([6, 6, 5, 6, 6, 5, 6, 6, 5, 6, 6, 6, 5, 6, 6, 6], [6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6] + [6] * 15),
    case([4, 6, 5, 7, 5, 6, 4, 6, 5, 6, 5, 4, 6, 5, 6, 5], [6] * 30),  # futility (>=15 seeds, trails by > 1 SE)
    case([7] * 10, [6] * 30),                                   # ahead
    case([3] * 12, [6] * 20),                                   # best version unfinished
]


def build(root, c):
    for v in c["versions"]:
        for i, p in enumerate(v["progress"]):
            seed = SEEDS[i]
            d = root / f"20260101-{v['id']}-{i:02d}"
            d.mkdir(parents=True)
            (d / "summary.json").write_text(json.dumps({
                "run_id": d.name, "seed": seed.split("#")[0], "prompt_version": v["id"], "result": "lost",
                "ante": 1 + p // 3, "round": p + 1, "started": 1000.0 + i, "ended": 1100.0 + i,
                "input_tokens": 10, "avg_confidence": 0.5, "output_tokens": 1, "benchmark": {"name": SET, "index": i, "of": len(SEEDS)},
            }), encoding="utf-8")
            lines = [json.dumps({"kind": "round_end"}) for _ in range(p)]
            lines.append(json.dumps({"kind": "decision", "question": "move", "choice": "play_1", "state_sent": {"discards_left": 1, "round": 1}}))
            (d / "decisions.jsonl").write_text("\n".join(lines) + "\n", encoding="utf-8")
    pd = root.parent / (root.name + "_prompts")
    pd.mkdir()
    ids = [v["id"] for v in c["versions"]]
    (pd / "index.json").write_text(json.dumps({"order": ids}), encoding="utf-8")
    for i in ids:
        (pd / f"{i}.json").write_text(json.dumps({"version": i, "created": 1.0, "note": "n"}), encoding="utf-8")
    return pd


class Stub:
    pass


out = []
for c in CASES:
    with tempfile.TemporaryDirectory() as t:
        root = Path(t) / "runs"
        root.mkdir()
        recorder.PROMPTS_DIR = build(root, c)
        recorder.PROMPT_VERSION = c["me"]
        bot = jev_bot.JevBot.__new__(jev_bot.JevBot)
        bot.bench_name, bot.backend = c["set"], "balatrobot"
        bot.recorder = Stub()
        bot.recorder.version, bot.recorder.root = c["me"], root
        out.append({**c, "expected": bot._clearly_worse()})
Path(ROOT / "native/bot/tests/data/clearly_worse.json").write_text(json.dumps(out, indent=1, ensure_ascii=False), encoding="utf-8")
print([o["expected"] for o in out])
