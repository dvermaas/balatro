"""Per-run recording: runs/<started>-<seed>/{decisions.jsonl, summary.json}.

decisions.jsonl has every Jev request (full state, instructions, option texts) with its answer
(choice, per-option probabilities, confidence), plus actions, errors and game events.
summary.json is rewritten as the run progresses, so an interrupted run still has one.
"""

import gzip
import hashlib
import json
import time
from pathlib import Path

import native

SRC = Path(__file__).resolve().parent  # the hashed decision files live here
ROOT = SRC.parent  # repo root: runs/
PROMPTS_DIR = ROOT / "runs" / "_prompts"
# Any edit to the prompts or candidate generation changes this, so runs can be grouped by version.
def version_hash(files, root=SRC):
    """sha1 of the files' exact bytes (line endings included; .gitattributes stores them unconverted)."""
    return hashlib.sha1(b"".join((root / f).read_bytes() for f in files if (root / f).exists())).hexdigest()[:8]


# decisions.py joined in v6; earlier versions lack it, so their ids are unchanged. v23 adds the native
# scoring kernel (src/native.py and the Rust sources), which computes the same scores as jokers.score.
PROMPT_VERSION = version_hash(("prompts.py", "hands.py", "strategy.py", "decisions.py", "jokers.py", "odds.py", "build.py", "tarots.py", "readiness.py", "rollout.py", "rollout_items.py",
                               "native.py", "../native/src/lib.rs", "../native/Cargo.toml"))


def snapshot_prompts():
    """Save the current prompt text under its version, labelled v1, v2... in order of first use."""
    import prompts

    PROMPTS_DIR.mkdir(parents=True, exist_ok=True)
    index_path = PROMPTS_DIR / "index.json"
    index = json.loads(index_path.read_text(encoding="utf-8")) if index_path.exists() else {"order": []}
    if PROMPT_VERSION not in index["order"]:
        index["order"].append(PROMPT_VERSION)
        index_path.write_text(json.dumps(index, indent=2), encoding="utf-8")
        (PROMPTS_DIR / f"{PROMPT_VERSION}.json").write_text(json.dumps({
            "version": PROMPT_VERSION, "created": time.time(), "note": prompts.VERSION_NOTE,
            "rules": prompts.GAME_RULES, "instructions": prompts.INSTRUCTIONS, "options": prompts.OPTIONS,
        }, indent=2), encoding="utf-8")


def prompt_versions():
    index_path = PROMPTS_DIR / "index.json"
    if not index_path.exists():
        return []
    out = []
    for n, v in enumerate(json.loads(index_path.read_text(encoding="utf-8"))["order"], 1):
        snap = json.loads((PROMPTS_DIR / f"{v}.json").read_text(encoding="utf-8"))
        out.append({**snap, "label": f"v{n}", "current": v == PROMPT_VERSION})
    return out


class RunRecorder:
    def __init__(self, root=ROOT / "runs", backend="balatrobot"):
        self.root = Path(root)
        self.root.mkdir(parents=True, exist_ok=True)
        self.backend = backend  # "balatrobot" (real game) or "jackdaw" (sim); stamped on every run
        snapshot_prompts()
        self.version = PROMPT_VERSION  # legacy bots override this
        self.run = None
        self.dir = None

    def ensure(self, gs, deck, stake, model, bench=None):
        seed = gs.get("seed")
        if not seed or gs.get("state") == "MENU" or (self.run and self.run["seed"] == seed):
            return
        if self.run:
            self.finish("abandoned", gs)
        started = time.strftime("%Y%m%d-%H%M%S")
        self.dir = self.root / f"{started}-{seed}"
        n = 1
        while self.dir.exists():  # parallel sim workers can start the same seed in the same second
            n += 1
            self.dir = self.root / f"{started}-{seed}-{n}"
        self.dir.mkdir()
        self.run = {
            "run_id": self.dir.name, "seed": seed, "deck": gs.get("deck") or deck,
            "stake": gs.get("stake") or stake, "model": model, "prompt_version": self.version,
            "benchmark": bench, "backend": self.backend, "scorer": native.backend(),
            "started": time.time(), "ended": None, "result": "in_progress",
            "ante": gs.get("ante_num"), "round": gs.get("round_num"), "money": gs.get("money"),
            "decisions": 0, "forced": 0, "errors": 0, "input_tokens": 0, "output_tokens": 0, "jev_ms": 0,
            "avg_confidence": None, "low_confidence_decisions": 0, "by_question": {},
        }
        self._conf_sum = 0.0
        self._save()

    def write_raw(self, event_id, gs):
        """Full BalatroBot gamestate (incl. remaining deck) per Jev decision, gzipped, keyed by event id."""
        if not self.run:
            return
        with gzip.open(self.dir / "states.jsonl.gz", "at", encoding="utf-8") as f:
            f.write(json.dumps({"id": event_id, "gs": gs}) + "\n")

    def write(self, ev):
        if not self.run:
            return
        with open(self.dir / "decisions.jsonl", "a", encoding="utf-8") as f:
            f.write(json.dumps(ev) + "\n")
        r = self.run
        if ev["kind"] == "decision":
            r["decisions"] += 1
            r["jev_ms"] += ev.get("ms", 0)
            r["input_tokens"] += ev.get("input_tokens", 0)
            r["output_tokens"] += ev.get("output_tokens", 0)
            self._conf_sum += ev.get("confidence") or 0
            r["avg_confidence"] = round(self._conf_sum / r["decisions"], 3)
            r["low_confidence_decisions"] += (ev.get("confidence") or 0) < 0.5
            r["by_question"][ev["question"]] = r["by_question"].get(ev["question"], 0) + 1
            self._save()
        elif ev["kind"] == "forced":
            r["forced"] += 1
        elif ev["kind"] == "error":
            r["errors"] += 1
            self._save()

    def progress(self, gs):
        if self.run and gs.get("seed") == self.run["seed"]:
            self.run.update(ante=gs.get("ante_num"), round=gs.get("round_num"), money=gs.get("money"))

    def finish(self, result, gs=None):
        if not self.run:
            return
        if gs and gs.get("seed") == self.run["seed"]:
            self.progress(gs)
        self.run.update(result=result, ended=time.time())
        self._save()
        self.run = None

    def _save(self):
        (self.dir / "summary.json").write_text(json.dumps(self.run, indent=2), encoding="utf-8")

    def list_runs(self, limit=50):
        out = []
        for p in sorted(self.root.glob("*/summary.json"), reverse=True)[:limit or None]:
            try:
                out.append(json.loads(p.read_text(encoding="utf-8")))
            except (OSError, json.JSONDecodeError):
                pass
        return out
