"""Paired early-run continuations from opaque first-shop saves, outside benchmark data.

Runs v17 and the frozen draft twice on three preselected seeds. Stops after
clearing ante 3 (or a loss). Every attempt is kept, including regressions.
Only the game loads saves; this script never reads their contents.
"""
import argparse
import hashlib
import json
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FILES = ["prompts.py", "hands.py", "strategy.py", "decisions.py", "jokers.py", "odds.py", "build.py", "tarots.py", "readiness.py"]
SEEDS = ["2RGEX6EN", "BAPQJMY5", "75WVKF42"]


def version(src):
    return hashlib.sha1(b"".join((src / f).read_bytes() for f in FILES)).hexdigest()[:8]


def paused():
    with urllib.request.urlopen("http://127.0.0.1:8765/api/live", timeout=5) as f:
        live = json.load(f)
    if live["mode"] != "paused" or live["benchmark"]["current"] or live["benchmark"]["remaining"]:
        raise RuntimeError("Panel must be paused with no queued benchmark")


def result_from_records(output, seed, condition, repeat, source_version, seconds=None):
    folders = list((output / "records").glob("*/summary.json"))
    assert len(folders) == 1
    summary = json.loads(folders[0].read_text(encoding="utf-8"))
    assert summary["result"] in ("horizon", "lost", "won"), "Do not silently replace an unfinished attempt"
    events = list(map(json.loads, folders[0].with_name("decisions.jsonl").read_text(encoding="utf-8").splitlines()))
    row = {"seed": seed, "condition": condition, "repeat": repeat,
           "version": source_version, "result": summary["result"], "ante": summary["ante"],
           "cleared_blinds_including_first": 1 + sum(e["kind"] == "round_end" for e in events),
           "skipped_blinds": sum(e["kind"] == "action" and e.get("method") == "skip" for e in events),
           "errors": summary["errors"], "decisions": summary["decisions"],
           "seconds": round(seconds if seconds is not None else summary["ended"] - summary["started"], 1),
           "cash_transactions": sum(e["kind"] == "decision" and (e.get("action") or [None])[0] == "buy_cash" for e in events),
           "riff_raff_taken": any(e["kind"] == "decision" and e.get("question") == "pack"
                                  and "Riff-raff" in e.get("options", {}).get(e.get("choice"), "") for e in events),
           "records": str(folders[0].parent.resolve().relative_to(ROOT))}
    (output / "result.json").write_text(json.dumps(row, indent=2) + "\n", encoding="utf-8")
    return row


def resume_folder(output, name, retry_interrupted):
    """Keep interrupted recordings and explicitly journal any replacement attempt."""
    journal_path = output / "interrupted-attempts.json"
    journal = json.loads(journal_path.read_text()) if journal_path.exists() else []
    replacements = [x for x in journal if x["trial"] == name]
    folder = output / (replacements[-1]["replacement"] if replacements else name)
    if not folder.exists():
        return folder
    summaries = list((folder / "records").glob("*/summary.json"))
    summary = json.loads(summaries[0].read_text()) if len(summaries) == 1 else None
    if summary and summary["result"] in ("horizon", "lost", "won"):
        return folder
    if not retry_interrupted:
        raise RuntimeError(f"Unfinished attempt retained at {folder}; explicitly retry with --retry-interrupted")
    replacement = f"{name}-recovery{len(replacements) + 1}"
    assert not (output / replacement).exists(), "Never overwrite another attempt"
    journal.append({"trial": name, "interrupted": folder.name, "replacement": replacement,
                    "recorded_at": time.time(), "partial_summary": summary,
                    "reason": "Explicit restart from original checkpoint after interruption; not a terminal performance result"})
    journal_path.write_text(json.dumps(journal, indent=2) + "\n")
    return output / replacement


def worker(args):
    src = Path(args.src).resolve()
    sys.path.insert(0, str(src))
    import recorder
    from jev_bot import JevBot, load_env
    from balatro import Balatro
    from hands import _d

    # Test records and labels must not enter the production benchmark/index.
    recorder.snapshot_prompts = lambda: None
    recorder.PROMPT_VERSION = version(src)
    load_env(ROOT / ".env")
    paused()
    game = Balatro(timeout=20)
    assert game.state()["state"] == "MENU", "Preserve an unrelated active game"
    cp = ROOT / "runs/_experiments/v18-checkpoints" / args.seed
    checkpoint = json.loads(cp.with_suffix(".json").read_text(encoding="utf-8"))
    output = Path(args.out).resolve()
    output.mkdir(parents=True, exist_ok=False)
    bot = JevBot(log_dir=output / "records")
    bot.auto_new_run = False
    bot.game = game
    bot._recent_seed, bot._recent = args.seed, checkpoint["recent"]
    bot._rerolls = 0
    started = time.monotonic()
    try:
        game.call("load", {"path": str(cp.with_suffix(".jkr"))})
        gs = game.state()
        actual = {"seed": gs["seed"], "state": gs["state"], "ante": gs["ante_num"], "round": gs["round_num"],
                  "money": gs["money"], "shop": [c.get("key") for c in _d(gs.get("shop")).get("cards", [])],
                  "packs": [c.get("key") for c in _d(gs.get("packs")).get("cards", [])],
                  "jokers": [c.get("key") for c in _d(gs.get("jokers")).get("cards", [])]}
        assert actual == checkpoint["public"], (actual, checkpoint["public"])
        bot.last_state = gs
        for _ in range(700):
            if time.monotonic() - started > 300:
                raise TimeoutError("Checkpoint continuation exceeded five minutes")
            acted = bot.step()
            gs = bot.last_state or {}
            if bot.stats["runs"]:
                break
            if gs.get("ante_num", 0) >= 4 and gs.get("state") in ("SHOP", "BLIND_SELECT"):
                bot.recorder.finish("horizon", gs)
                break
            if acted:
                time.sleep(.3)  # Same action delay as bench30-a.
        else:
            raise RuntimeError("Checkpoint continuation exceeded its step cap")
        row = result_from_records(output, args.seed, args.condition, args.repeat, version(src), time.monotonic() - started)
        print(json.dumps(row), flush=True)
    finally:
        game.call("menu")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--worker", action="store_true")
    p.add_argument("--src")
    p.add_argument("--seed")
    p.add_argument("--condition")
    p.add_argument("--repeat", type=int)
    p.add_argument("--resume", action="store_true")
    p.add_argument("--retry-interrupted", action="store_true")
    p.add_argument("--out", required=True)
    args = p.parse_args()
    if args.worker:
        worker(args)
        return
    paused()
    output = Path(args.out).resolve()
    output.mkdir(parents=True, exist_ok=args.resume)
    sources = {"v17": ROOT / "runs/_experiments/v18-baseline-src", "draft": ROOT / "src"}
    frozen = {k: version(v) for k, v in sources.items()}
    if args.resume:
        assert json.loads((output / "protocol.json").read_text())["versions"] == frozen
    else:
        (output / "protocol.json").write_text(json.dumps({"seeds": SEEDS, "repeats": 2, "horizon_ante": 4,
            "versions": frozen, "purpose": "Exploratory paired checkpoint continuations; not complete benchmark games"}, indent=2) + "\n")
    rows = []
    for repeat in range(2):
        for seed in SEEDS:
            for condition in (["v17", "draft"] if repeat == 0 else ["draft", "v17"]):
                assert {k: version(v) for k, v in sources.items()} == frozen, "Source changed during paired tests"
                name = f"{seed}-{condition}-{repeat}"
                folder = resume_folder(output, name, args.retry_interrupted) if args.resume else output / name
                if folder.exists() and args.resume:
                    result_from_records(folder, seed, condition, repeat, frozen[condition])
                else:
                    subprocess.run([sys.executable, "-u", "-B", __file__, "--worker", "--src", str(sources[condition]),
                        "--seed", seed, "--condition", condition, "--repeat", str(repeat), "--out", str(folder)], check=True)
                rows.append(json.loads((folder / "result.json").read_text()))
                (output / "results.json").write_text(json.dumps(rows, indent=2) + "\n")


if __name__ == "__main__":
    main()
