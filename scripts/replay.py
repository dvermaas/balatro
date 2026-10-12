"""Replay recorded decisions through the current code + prompts, without the game.

Every Jev call saves the full gamestate (runs/<run>/states.jsonl.gz). This loads those states, optionally
edits them (forge a scenario), runs the decision handler from src/ with a stand-in bot that asks Jev for
real but never touches the game, and compares the new choice with the one recorded at the time.

    .venv/Scripts/python scripts/replay.py --version ac9101d7 --question shop --ante-min 3 --limit 10
    .venv/Scripts/python scripts/replay.py --run 20261012-212450-7X7WSDCX --question shop \\
        --edit "gs['money'] = 40"

Costs Jev credits (one call per decision). Limits: this run's recent plays are not reconstructed, so
measured gains use typical hands (as in a run's first shops); bot.ask (blind select) is not replayed.
"""

import argparse
import gzip
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HANDLERS = {"shop": "shop", "move": "selecting_hand", "pack": "booster"}


class _Done(Exception):
    pass


class ReplayBot:
    """The attributes decisions.py uses; act() stops the handler at its first game action."""

    def __init__(self, jev):
        self.jev = jev
        self._shop_key = self._recent_seed = None
        self._shop_actions = self._rerolls = self._last_pack_cost = 0
        self._failed, self._recent = set(), []
        self.events, self.decision, self.action = [], None, None

    def emit(self, kind, **kw):
        self.events.append({"kind": kind, **kw})

    def query(self, gs, state, questions):
        resp = self.jev.system_one(state=state, questions=questions)
        return resp.answers, {"state_sent": state, "answers": {k: a.model_dump() for k, a in resp.answers.items()}}

    def record_decision(self, question, gs, choice, action, options, probs, confidence, meta, display="prob"):
        self.decision = {"choice": choice, "desc": options.get(choice), "probs": probs, "options": options,
                         "state_sent": meta.get("state_sent")}

    def act(self, method, params=None):
        self.action = (method, params)
        raise _Done

    def ask(self, *a, **k):
        raise _Done


def load_states(run_dir):
    p = run_dir / "states.jsonl.gz"
    if not p.exists():
        return []
    with gzip.open(p, "rt", encoding="utf-8") as f:
        return [json.loads(line) for line in f]


def recorded(run_dir, question, ante_min):
    """(decision event, raw gamestate) pairs: the raw state is the last one saved at or before the decision."""
    states = load_states(run_dir)
    out = []
    for line in open(run_dir / "decisions.jsonl", encoding="utf-8"):
        e = json.loads(line)
        if e.get("kind") != "decision" or e.get("question") != question or (e.get("ante") or 0) < ante_min:
            continue
        raw = [s for s in states if s["id"] <= e["id"]]
        if raw:
            e["run_id"] = run_dir.name
            out.append((e, raw[-1]["gs"]))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", help="prompt version id: replay benchmark runs of this version")
    ap.add_argument("--run", action="append", help="run dir name under runs/ (repeatable)")
    ap.add_argument("--question", default="shop", choices=list(HANDLERS))
    ap.add_argument("--ante-min", type=int, default=1)
    ap.add_argument("--limit", type=int, default=10)
    ap.add_argument("--edit", action="append", default=[], help="python statement run on `gs` (forge a scenario)")
    ap.add_argument("--src", default=str(ROOT / "src"), help="code to test (default: working tree src/)")
    ap.add_argument("--show-state", action="store_true", help="print the state Jev was sent")
    ap.add_argument("--out", help="write results as JSON lines")
    args = ap.parse_args()

    sys.path.insert(0, args.src)
    import decisions
    from jev_bot import load_env
    from typesafe_sdk import TypeSafeClient
    import os

    load_env()
    jev = TypeSafeClient(api_key=os.environ.get("JEV_API_KEY") or os.environ.get("TYPESAFE_API_KEY"),
                         model="jev-latest")

    runs = [ROOT / "runs" / r for r in args.run or []]
    if args.version:
        for s in sorted((ROOT / "runs").glob("*/summary.json")):
            d = json.loads(s.read_text(encoding="utf-8"))
            if d.get("prompt_version") == args.version and d.get("benchmark"):
                runs.append(s.parent)
    pairs = [p for r in runs for p in recorded(r, args.question, args.ante_min)]
    # Spread the sample over runs instead of taking the first run's decisions.
    step = max(1, len(pairs) // args.limit) if args.limit else 1
    pairs = pairs[::step][:args.limit]
    print(f"{len(pairs)} {args.question} decisions from {len(runs)} runs; edits: {args.edit or 'none'}\n")

    handler = getattr(decisions, HANDLERS[args.question])
    same = 0
    out = open(args.out, "w", encoding="utf-8") if args.out else None
    for e, gs in pairs:
        for stmt in args.edit:
            exec(stmt, {"gs": gs})
        bot = ReplayBot(jev)
        try:
            handler(bot, gs)
        except _Done:
            pass
        except Exception as ex:  # a forged state can break a handler; report and continue
            print(f"ante {e.get('ante')} id {e['id']}: handler error {ex!r}")
            continue
        new = bot.decision or {"choice": "(no question)", "desc": str(bot.action)}
        old_desc = (e.get("options") or {}).get(e.get("choice"), e.get("choice"))
        match = new["choice"] == e.get("choice")
        same += match
        print(f"ante {e.get('ante')} round {e.get('round')} [{e.get('run_id', '')}]{'' if match else '  CHANGED'}")
        print(f"   was: {old_desc}")
        print(f"   now: {new.get('desc') or new['choice']}  (action {bot.action})")
        for ev in bot.events:
            if ev["kind"] in ("info", "forced") and "Shop (" not in str(ev.get("msg", "")):
                print(f"   . {ev.get('msg') or ev}")
        if args.show_state and new.get("state_sent"):
            print("   state:", json.dumps({k: v for k, v in new["state_sent"].items() if k != "game_rules"})[:1500])
        if out:
            out.write(json.dumps({"id": e["id"], "ante": e.get("ante"), "was": old_desc, "now": new.get("desc"),
                                  "action": bot.action, "probs": new.get("probs")}) + "\n")
    print(f"\nsame choice in {same}/{len(pairs)}")


if __name__ == "__main__":
    main()
