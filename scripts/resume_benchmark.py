"""Resume a stopped panel from recorded terminal runs, without replaying their seeds.

Defaults to a read-only plan. --serve starts the panel and remaining queue.
With --attach-run, the loaded game must match the last recorded outcome exactly.
Otherwise the game must be at MENU and unfinished results require an audited win.
Decision source is unchanged. Existing early-stop rules apply unless the caller
explicitly requests --finish-set to complete every remaining seed.
"""

import argparse
import gzip
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
import benchmark
from recorder import PROMPT_VERSION
from audit_wins import audit_run


def plan(root, version, name, attach_run=None):
    seeds = benchmark.SETS[name]
    rows, recoveries = {}, []
    for path in sorted(root.glob("*/summary.json")):
        row = json.loads(path.read_text(encoding="utf-8"))
        bench = row.get("benchmark") or {}
        if row.get("prompt_version") != version or bench.get("name") != name:
            continue
        i = bench.get("index")
        if not isinstance(i, int) or not 0 <= i < len(seeds) or row["seed"] != seeds[i].split("#")[0]:
            raise ValueError(f"Invalid seed/index: {path}")
        if row["result"] == "abandoned":
            continue
        if i in rows:
            raise ValueError(f"Duplicate benchmark index {i}: refusing to choose a result")
        if row["result"] not in ("won", "lost") and row["run_id"] != attach_run:
            with gzip.open(path.parent / "states.jsonl.gz", "rt", encoding="utf-8") as f:
                saved = [json.loads(line) for line in f]
            gs = saved[-1]["gs"] if saved else {}
            if (gs.get("won") is not True or gs.get("seed") != row["seed"]
                    or audit_run(path.parent)["verdict"] != "verified_win"):
                raise ValueError(f"Unfinished run has no confirmed win: {path}")
            # Only public outcome metadata is needed; never use draw-pile order.
            outcome = {k: gs.get(k) for k in ("seed", "won", "ante_num", "round_num", "money")}
            recoveries.append((path, row, outcome, saved[-1]["id"]))
        rows[i] = row
    if not rows:
        raise ValueError("No existing runs for this version and benchmark")
    if attach_run and not any(r["run_id"] == attach_run and r["result"] == "in_progress" for r in rows.values()):
        raise ValueError("Requested attachment is not an unfinished run in this benchmark")
    settings = {(r["deck"], r["stake"], r["model"]) for r in rows.values()}
    if len(settings) != 1:
        raise ValueError("Inconsistent run settings")
    queue = [(i, seed) for i, seed in enumerate(seeds) if i not in rows]
    return rows, recoveries, queue, settings.pop()


def validate_attachment(row, events, gs):
    """Refuse an autosave rollback or the wrong live game; compare public facts only."""
    last = next(e for e in reversed(events) if e["kind"] == "outcome")
    if gs.get("seed") != row["seed"] or gs.get("deck") != row["deck"] or gs.get("stake") != row["stake"]:
        raise ValueError("Loaded seed/deck/stake differs from the recorded run")
    for live_key, saved_key in (("state", "state"), ("ante_num", "ante"), ("round_num", "round"), ("money", "money")):
        if gs.get(live_key) != last.get(saved_key):
            raise ValueError(f"Loaded {live_key} differs from the last recorded outcome")
    if gs.get("state") != "SELECTING_HAND":
        raise ValueError("Only a paused hand can be attached by this launcher")
    for key in ("chips", "hands_left", "discards_left"):
        if (gs.get("round") or {}).get(key) != last.get(key):
            raise ValueError(f"Loaded {key} differs from the last recorded outcome")
    if [c.get("label") for c in gs["jokers"]["cards"]] != last.get("jokers"):
        raise ValueError("Loaded jokers differ from the last recorded outcome")


def attach(bot, folder, row, events, gs):
    from decisions import _remember
    from jev_bot import cards_of
    validate_attachment(row, events, gs)
    bot.recorder.run, bot.recorder.dir = row, folder
    bot.recorder._conf_sum = sum(e.get("confidence") or 0 for e in events if e["kind"] == "decision")
    bot._event_id = max(e["id"] for e in events)
    bot.events.extend(events[-400:])
    bot.bench_current = row["benchmark"]
    # Forced plays did not save full hands in v15. Rebuild only exact recorded
    # decision snapshots; never fabricate held cards or reconstruct future draws.
    with gzip.open(folder / "states.jsonl.gz", "rt", encoding="utf-8") as f:
        states = {s["id"]: s["gs"] for s in map(json.loads, f)}
    restored = 0
    for event in events:
        if (event["kind"] == "decision" and event.get("question") == "move"
                and event.get("action", [None])[0] == "play" and event["id"] in states):
            past = states[event["id"]]
            _remember(bot, past, cards_of(past, "hand"), event["action"][1]["cards"])
            restored += 1
    plays = sum(e["kind"] == "action" and e.get("method") == "play" for e in events)
    row.setdefault("resumptions", []).append({"at": time.time(), "after_event": events[-1]["id"],
        "recorded_plays_restored": restored, "prior_plays": plays,
        "idle_seconds": max(0, time.time() - events[-1]["t"])})
    bot.recorder._save()
    bot.emit("info", msg=f"Attached paused run at its last recorded outcome; restored {restored}/{plays} exact prior play snapshots. Missing forced-play history is documented.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--expected-version", required=True)
    parser.add_argument("--set", default=benchmark.NAME, choices=benchmark.SETS)
    parser.add_argument("--serve", action="store_true")
    parser.add_argument("--finish-set", action="store_true",
                        help="Explicitly finish the remaining fixed seeds without performance early stopping")
    parser.add_argument("--attach-run", help="Existing unfinished run after loading its exact autosave")
    args = parser.parse_args()
    if PROMPT_VERSION != args.expected_version:
        raise SystemExit(f"Source version {PROMPT_VERSION} differs from requested {args.expected_version}")
    rows, recoveries, queue, settings = plan(ROOT / "runs", PROMPT_VERSION, args.set, args.attach_run)
    print(json.dumps({"version": PROMPT_VERSION, "set": args.set, "recorded": len(rows),
                      "confirmed_wins_to_recover": [r[1]["run_id"] for r in recoveries],
                      "remaining": len(queue), "next": queue[0] if queue else None,
                      "settings": settings, "attach_run": args.attach_run,
                      "performance_early_stop": not args.finish_set}), flush=True)
    if not args.serve:
        return

    import panel
    from balatro import Balatro
    # Bind first: an existing panel must never be replaced or run in parallel.
    server = panel.ThreadingHTTPServer(("127.0.0.1", 8765), panel.Handler)
    gs = Balatro(timeout=8).state()
    attached = next((r for r in rows.values() if r["run_id"] == args.attach_run), None)
    attached_events = []
    if attached:
        folder = ROOT / "runs" / args.attach_run
        attached_events = [json.loads(l) for l in (folder / "decisions.jsonl").read_text(encoding="utf-8").splitlines()]
        validate_attachment(attached, attached_events, gs)
    elif gs.get("state") != "MENU":
        server.server_close()
        raise SystemExit("Game is not at MENU; preserve the active game and recover it separately")
    panel.ctl = ctl = panel.Controller()
    bot = ctl.bot
    bot.deck, bot.stake, bot.model = settings
    if bot.model != "jev-latest":
        raise SystemExit("This launcher requires the existing jev-latest model setting")
    bot.bench_name = args.set
    bot.auto_new_run = False  # Never start unrelated random games after the queue.
    bot.last_state = gs

    for path, row, outcome, state_id in recoveries:
        backup = path.with_name("summary.pre-resume.json")
        with backup.open("x", encoding="utf-8") as f:
            f.write(path.read_text(encoding="utf-8"))
        events = [json.loads(line) for line in (path.parent / "decisions.jsonl").read_text(encoding="utf-8").splitlines()]
        bot.recorder.run, bot.recorder.dir = row, path.parent
        bot.recorder._conf_sum = sum(e.get("confidence") or 0 for e in events if e["kind"] == "decision")
        bot._event_id = max(bot._event_id, max(e["id"] for e in events))
        bot.emit("info", msg=f"Recovered confirmed victory from saved state {state_id}; interrupted after the win. Original summary preserved.")
        bot.emit("run_end", won=True, ante=outcome["ante_num"], round=outcome["round_num"], recovered=True)
        bot.recorder.progress(outcome)
        row.update(result="won", ended=events[-1]["t"], recovery={"at": time.time(), "state_id": state_id,
                   "reason": "Saved gamestate explicitly won=True before post-victory timeout", "backup": backup.name})
        bot.recorder._save()
        bot.recorder.run = None

    for key in ("decisions", "input_tokens", "output_tokens", "jev_ms", "errors"):
        bot.stats[key] = sum(r.get(key, 0) for r in rows.values())
    done = sum(r["result"] in ("won", "lost") for r in rows.values())
    bot.stats.update(runs=done, wins=sum(r["result"] == "won" for r in rows.values()),
                     best_ante=max(r.get("ante") or 0 for r in rows.values()))
    bot.bench_queue = queue
    bot.bench_done_pending = not queue
    if args.finish_set:
        # Runtime-only evaluation setting; decision source and recorded results stay unchanged.
        bot.EARLY_STOP_AFTER = len(benchmark.SETS[args.set]) + 1
    if attached:
        attach(bot, ROOT / "runs" / args.attach_run, attached, attached_events, gs)
    stopping = "Performance early stopping disabled for this explicitly requested completion." if args.finish_set else "Existing early-stop rules apply."
    bot.emit("info", msg=f"Resumed {args.set}: {done} completed results retained; {len(queue)} later seeds queued. {stopping}")
    ctl.mode = "running"
    print("Resumed panel: http://127.0.0.1:8765", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
