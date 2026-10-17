"""Record reference traces of the Python bot (v27, then v31) for the Rust port (docs/rust-port-guide.md).

One gzipped JSONL per game: runs/_port/traces/<arm>/<seed>.jsonl.gz. Lines, in call order:
  {"k": "call", "method", "params", "ok": <result>} or {..., "err": {"code", "message", "name"}}
      every game call, including the "gamestate" reads (step() fetches state through call("gamestate")).
      A result identical to the previous call's result is stored as {"same": true} instead.
  {"k": "jev", "state", "questions": {qid: model_dump}, "answers": {qid: model_dump}}
  {"k": "event", "kind", "data"}   JevBot.emit (the decisions.jsonl events, without id and t)
  {"k": "oracle", "fn", "args": {...}, "ret": ...}
      rollout.forecast, rollout_items.forecast, rollout_items.Picker.values / .pick_pub: the random or
      expensive calls, so the decision port can be checked before the forecasts are ported.
  {"k": "end", "result": <policy_lab row>}

Usage: .venv/Scripts/python scripts/port/trace_game.py --set sim30-a --count 4 --arm bot-forecast-items --jobs 4
"""

import argparse
import gzip
import json
import sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT / "src"), str(ROOT / "scripts")]
OUT = ROOT / "runs" / "_port" / "traces"


def _jsonable(x):
    if isinstance(x, dict):
        return {str(k): _jsonable(v) for k, v in x.items()}
    if isinstance(x, (list, tuple)):
        return [_jsonable(v) for v in x]
    if isinstance(x, (set, frozenset)):
        return sorted(_jsonable(v) for v in x)
    if x is None or isinstance(x, (bool, int, float, str)):
        return x
    if hasattr(x, "model_dump"):
        return _jsonable(x.model_dump(mode="json"))
    return {"__repr__": repr(x)}


class Trace:
    def __init__(self, path):
        self.f = gzip.open(path, "wt", encoding="utf-8")
        self.last = None

    def write(self, rec):
        self.f.write(json.dumps(rec, ensure_ascii=False) + "\n")

    def close(self):
        self.f.close()


CURRENT = None  # the game's Trace; patches are installed once per worker process
_INSTALLED = []


def _install():
    """Wrap the game, JEV and the oracle functions; every wrapper records and returns the real result."""
    if _INSTALLED:
        return _INSTALLED[0]

    class Proxy:
        def __getattr__(self, name):
            return getattr(CURRENT, name)

        def __setattr__(self, name, value):
            setattr(CURRENT, name, value)

    trace = Proxy()
    import policy_lab
    import rollout
    import rollout_items
    import sim_backend
    from balatro import RPCError

    orig_call = sim_backend.SimBalatro.call

    def call(self, method, params=None):
        rec = {"k": "call", "method": method, "params": _jsonable(params)}
        try:
            res = orig_call(self, method, params)
        except RPCError as e:
            rec["err"] = {"code": e.code, "message": str(getattr(e, "message", e)), "name": getattr(e, "name", None)}
            trace.write(rec)
            trace.last = None
            raise
        j = _jsonable(res)
        if j == trace.last:
            rec["same"] = True
        else:
            rec["ok"] = j
            trace.last = j
        trace.write(rec)
        return res

    sim_backend.SimBalatro.call = call

    orig_code_jev = policy_lab._code_jev

    def code_jev(bot_ref, items=False, opts=None):
        jev = orig_code_jev(bot_ref, items, opts)
        orig = jev.system_one

        def system_one(state, questions):
            resp = orig(state=state, questions=questions)
            trace.write({"k": "jev", "state": _jsonable(state),
                         "questions": {k: _jsonable(q) for k, q in questions.items()},
                         "answers": {k: _jsonable(a) for k, a in resp.answers.items()}})
            return resp

        jev.system_one = system_one
        return jev

    policy_lab._code_jev = code_jev

    import jev_bot
    orig_emit = jev_bot.JevBot.emit

    def emit(self, kind, **data):
        trace.write({"k": "event", "kind": kind, "data": _jsonable(data)})
        return orig_emit(self, kind, **data)

    jev_bot.JevBot.emit = emit

    depth = [0]  # record top-level calls only: forecasts call the Picker inside their rollouts

    def oracle(name, fn, argnames):
        def wrapped(*a, **kw):
            depth[0] += 1
            try:
                ret = fn(*a, **kw)
            finally:
                depth[0] -= 1
            if depth[0]:
                return ret
            args = {n: _jsonable(v) for n, v in zip(argnames, a)}
            args.update({k: _jsonable(v) for k, v in kw.items()})
            args.pop("pub", None)  # the public state: always the bot's last state
            args.pop("self", None)  # a Picker built from that same state
            trace.write({"k": "oracle", "fn": name, "args": args, "ret": _jsonable(ret)})
            return ret
        return wrapped

    rollout.forecast = oracle("rollout.forecast", rollout.forecast, ["pub", "n", "bosses", "keys", "budget_s"])
    rollout_items.forecast = oracle("rollout_items.forecast", rollout_items.forecast,
                                    ["pub", "n", "bosses", "keys", "budget_s"])
    P = rollout_items.Picker
    P.values = oracle("Picker.values", P.values, ["self", "pack", "hand", "money", "jokers", "joker_limit"])
    P.pick_pub = oracle("Picker.pick_pub", P.pick_pub, ["self", "pub"])
    _INSTALLED.append(policy_lab)
    return policy_lab


def run_one(task):
    seed, arm, out_dir = task
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    global CURRENT
    CURRENT = Trace(out_dir / f"{seed}.jsonl.gz")
    trace = CURRENT
    try:
        policy_lab = _install()
        import time
        row = policy_lab._play_bot(seed, arm, time.time())
        row.pop("seconds", None)
        trace.write({"k": "end", "result": _jsonable(row)})
        return seed, row.get("ante"), row.get("won")
    finally:
        trace.close()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", default="sim30-a")
    ap.add_argument("--count", type=int, default=4)
    ap.add_argument("--seeds", nargs="*")
    ap.add_argument("--arm", default="bot-forecast-items")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--out", default=None)
    a = ap.parse_args()
    import benchmark
    seeds = a.seeds or [s.split("#")[0] for s in benchmark.SETS[a.set]][:a.count]
    out = Path(a.out) if a.out else OUT / a.arm
    tasks = [(s, a.arm, str(out)) for s in seeds]
    with ProcessPoolExecutor(a.jobs) as ex:
        for seed, ante, won in ex.map(run_one, tasks):
            print(seed, "won" if won else f"ante {ante}", flush=True)


if __name__ == "__main__":
    main()
