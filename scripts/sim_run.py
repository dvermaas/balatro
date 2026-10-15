"""Play full games headless on the Jackdaw sim with the normal bot (JevBot) and the normal recorder.

  python scripts/sim_run.py --seed 7X7WSDCX                      # one JEV game (costs JEV calls)
  python scripts/sim_run.py --seed A --seed B --jobs 2            # several seeds, parallel processes
  python scripts/sim_run.py --set bench30-a --jobs 8              # a benchmark set (summary.benchmark set)
  python scripts/sim_run.py --seed A --policy greedy              # no JEV: every question takes option 1
  python scripts/sim_run.py --seed A --policy random              # no JEV: best play, random shop/packs

Runs are recorded like real ones (decisions.jsonl, states.jsonl.gz, summary.json) with
summary.backend = "jackdaw", under runs/sim/ (JEV) or runs/sim_smoke/ (no-JEV policies), never in runs/
itself, so real-game benchmarks and analyses never see them. The sim profile is all-unlocked.
Works with plain Python 3.12+ (Linux) given typesafe-sdk for --policy jev, and the localization
from scripts/extract_localization.py ($BALATRO_LOC_DIR).
"""
from __future__ import annotations

import argparse
import json
import multiprocessing as mp
import os
import sys
import time
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "src"))


class FirstOptionJev:
    """Stand-in for TypeSafeClient: answers every question without an API call. Choice -> the
    first option (the bot lists its best estimate first), Noul -> 1.0, Score -> 0.5."""

    model = "greedy-first-option"

    def pick(self, qid, keys):
        return keys[0]

    def score(self, qid):
        return 0.5

    def system_one(self, state, questions):
        from typesafe_sdk import SystemOneResponse
        answers = {}
        for qid, q in questions.items():
            if q.type == "choice":
                keys = list(q.criteria)
                c = self.pick(qid, keys)
                answers[qid] = {"type": "choice", "choice": c, "confidence": 1.0,
                                "probabilities": {k: float(k == c) for k in keys}}
            elif q.type == "noul":
                answers[qid] = {"type": "noul", "noul": 1.0}
            else:
                answers[qid] = {"type": "score", "score": self.score(qid), "confidence": 1.0, "legend": {},
                                "probabilities": {}}
        return SystemOneResponse.model_validate({"model": self.model, "answers": answers,
                                                 "usage": {"input_tokens": 0, "output_tokens": 0}})


class RandomShopJev(FirstOptionJev):
    """Smoke-test policy: plays every blind and the bot's best-estimated hand/discard (option 1), but
    answers every other question (shop, packs, sells, consumables, cash-out steps) at random,
    seeded per game, so shops, packs, vouchers, rerolls and consumables all get exercised."""

    model = "random-shop"
    PLAY_QUESTIONS = ("move", "best_play", "best_discard", "blind")

    def __init__(self, seed):
        import random
        self.rng = random.Random(seed)

    def pick(self, qid, keys):
        return keys[0] if qid in self.PLAY_QUESTIONS else self.rng.choice(keys)

    def score(self, qid):
        return self.rng.random()


def play(seed, policy="jev", bench=None, deck="RED", stake="WHITE", max_steps=20000, out_root=None):
    """Play one seeded game to the end on the sim; returns a result dict (never raises)."""
    import benchmark  # noqa: F401  (JevBot imports it too)
    from jev_bot import BenchmarkComplete, JevBot
    from sim_backend import SimBalatro

    t0 = time.time()
    res = {"seed": seed, "policy": policy, "pid": os.getpid()}
    bot = None
    try:
        game = SimBalatro()
        jev = {"greedy": FirstOptionJev, "random": lambda: RandomShopJev(seed)}.get(policy, lambda: None)()
        if out_root is None:
            out_root = ROOT / "runs" / ("sim" if policy == "jev" else "sim_smoke")
        bot = JevBot(deck=deck, stake=stake, game=game, jev=jev, log_dir=Path(out_root))
        if jev is not None:
            bot.model = jev.model
        bot.bench_current = bench
        bot.act("start", {"seed": seed, "deck": deck, "stake": stake})
        steps = 0
        while steps < max_steps:
            steps += 1
            try:
                bot.step()
            except BenchmarkComplete:
                break
            if bot.stats["runs"] >= 1 and (bot.last_state or {}).get("state") == "MENU":
                break
        else:
            res["error"] = f"no game end after {max_steps} steps"
            bot.recorder.finish("abandoned", bot.last_state)
        r = bot.recorder
        summ = json.loads((r.dir / "summary.json").read_text(encoding="utf-8")) if r.dir else {}
        res.update(run_id=summ.get("run_id"), result=summ.get("result"), ante=summ.get("ante"),
                   round=summ.get("round"), steps=steps)
    except Exception as e:  # noqa: BLE001
        res["error"] = f"{type(e).__name__}: {e}"
        res["trace"] = traceback.format_exc(limit=6)
        if bot is not None and bot.recorder.run:
            bot.recorder.finish("abandoned", bot.last_state)
    if bot is not None:
        st = bot.stats
        res.update(decisions=st["decisions"], input_tokens=st["input_tokens"], output_tokens=st["output_tokens"],
                   jev_seconds=round(st["jev_ms"] / 1000, 1), game_errors=st["errors"],
                   engine_errors=list(getattr(bot.game, "engine_errors", [])))
    res["seconds"] = round(time.time() - t0, 1)
    return res


def _worker(args):
    return play(*args)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--seed", action="append", default=[], help="seed (repeatable, or comma-separated)")
    ap.add_argument("--set", help="play a benchmark set from src/benchmark.py (marks summary.benchmark)")
    ap.add_argument("--policy", choices=("jev", "greedy", "random"), default="jev",
                    help="jev (API calls), greedy (option 1 everywhere), random (best play, random elsewhere)")
    ap.add_argument("--jobs", type=int, default=1, help="parallel worker processes")
    ap.add_argument("--deck", default="RED")
    ap.add_argument("--stake", default="WHITE")
    ap.add_argument("--out", help="recording root (default runs/sim, or runs/sim_smoke for greedy/random)")
    ap.add_argument("--max-steps", type=int, default=20000)
    a = ap.parse_args()

    import benchmark
    from recorder import snapshot_prompts
    snapshot_prompts()  # once, before workers start (they would race on runs/_prompts/index.json)

    tasks = []
    if a.set:
        seeds = benchmark.SETS[a.set]
        tasks = [(s.split("#")[0], a.policy, {"name": a.set, "index": i, "of": len(seeds)}) for i, s in enumerate(seeds)]
    for s in a.seed:
        tasks += [(x.strip(), a.policy, None) for x in s.split(",") if x.strip()]
    if not tasks:
        ap.error("give --seed or --set")
    tasks = [t + (a.deck, a.stake, a.max_steps, a.out) for t in tasks]
    t0 = time.time()
    results = []
    if a.jobs > 1 and len(tasks) > 1:
        with mp.get_context("spawn").Pool(min(a.jobs, len(tasks))) as pool:
            for r in pool.imap_unordered(_worker, tasks):
                results.append(r)
                print(json.dumps({k: v for k, v in r.items() if k != "trace"}), flush=True)
    else:
        for t in tasks:
            r = play(*t)
            results.append(r)
            print(json.dumps({k: v for k, v in r.items() if k != "trace"}), flush=True)
    for r in results:
        if r.get("trace"):
            print(f"--- {r['seed']}\n{r['trace']}", file=sys.stderr)
    done = [r for r in results if r.get("result") in ("won", "lost")]
    print(json.dumps({"games": len(results), "finished": len(done), "won": sum(r["result"] == "won" for r in done),
                      "avg_ante": round(sum(r["ante"] or 0 for r in done) / len(done), 2) if done else None,
                      "errors": sum(bool(r.get("error")) for r in results),
                      "engine_errors": sum(len(r.get("engine_errors") or []) for r in results),
                      "input_tokens": sum(r.get("input_tokens") or 0 for r in results),
                      "output_tokens": sum(r.get("output_tokens") or 0 for r in results),
                      "wall_seconds": round(time.time() - t0, 1)}))
    return 0 if all(not r.get("error") for r in results) else 1


if __name__ == "__main__":
    sys.exit(main())
