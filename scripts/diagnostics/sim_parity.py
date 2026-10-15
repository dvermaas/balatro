"""Sim-backend parity: replay recorded real-game runs through src/sim_backend.SimBalatro.

For every recorded run (runs/<id>/decisions.jsonl actions + states.jsonl.gz BalatroBot gamestates)
this replays the actions through the sim and, before each recorded decision, compares:

  * the gameplay snapshot of jackdaw_fidelity.py (state, money, hands, deck, shop, packs, ...),
  * every effect text the policy reads (jokers, consumables, shop, vouchers, packs, pack cards,
    hand and deck cards, boss blind effect, tag effects) for the same object in the same state,
  * the JSON shape (keys and value types of every field, card by card).

Historical runs used the real profile of the time, so this applies that profile's unlocks (as the
audit does); the sim's default for new games is the all-unlocked profile.

  .venv/Scripts/python scripts/diagnostics/sim_parity.py [--set bench30-a] [--runs N] [--out FILE]
"""
from __future__ import annotations

import argparse
import collections
import glob
import gzip
import json
import os
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "src"))
sys.path.insert(0, os.path.join(ROOT, "scripts", "diagnostics"))

import sim_backend  # noqa: E402  (puts third_party/jackdaw on sys.path first)
from balatro import RPCError  # noqa: E402
import jackdaw_fidelity as jf  # noqa: E402

# The real profile at recording time (it has since been switched to all-unlocked): the 58 locked
# pool keys the audit read from meta.jkr, and the undiscovered centers that gate Tag pools (never
# offered in any recorded run: Rare/Polychrome/Negative Tags).
HISTORICAL_UNDISCOVERED = {"j_blueprint", "e_polychrome", "e_negative"}


def historical_profile():
    from jackdaw.engine.data.prototypes import _load_json
    audit = json.load(open(os.path.join(ROOT, "docs", "samples", "jackdaw-fidelity.json"), encoding="utf8"))
    return audit["profile_locked_keys"], set(_load_json("centers.json")) - HISTORICAL_UNDISCOVERED


TEXT_AREAS = ("jokers", "consumables", "shop", "vouchers", "packs", "pack", "hand", "cards")


def _d(x):
    return x if isinstance(x, dict) else {}


def shape(v):
    if isinstance(v, dict):
        return "dict" if v else "empty"
    if isinstance(v, list):
        return "list" if v else "empty"
    if isinstance(v, bool):
        return "bool"
    if isinstance(v, (int, float)):
        return "num"
    return type(v).__name__


def shape_diff(rec, sim, path, out):
    """Paths whose key set or value type differs (empty {} and [] count as the same Lua table)."""
    if isinstance(rec, dict) and isinstance(sim, dict):
        for k in set(rec) | set(sim):
            if k not in sim:
                out[f"{path}.{k}: missing in sim"] += 1
            elif k not in rec:
                out[f"{path}.{k}: extra in sim"] += 1
            else:
                shape_diff(rec[k], sim[k], f"{path}.{k}", out)
    elif isinstance(rec, list) and isinstance(sim, list):
        for a, b in zip(rec, sim):
            shape_diff(a, b, path + "[]", out)
    elif shape(rec) != shape(sim) and not (rec is None and sim is None):
        out[f"{path}: {shape(rec)} vs {shape(sim)}"] += 1


def card_fields(c):
    m = _d(c.get("modifier"))
    return (c.get("key"), c.get("set"), c.get("label"), json.dumps(m, sort_keys=True))


def compare_texts(rec, sim, stats, samples, where):
    """Effect text of the same object (same area, same position, same key/modifiers) in both states."""
    for a in TEXT_AREAS:
        rc, sc = _d(rec.get(a)).get("cards") or [], _d(sim.get(a)).get("cards") or []
        if len(rc) != len(sc):
            continue
        if a in ("hand", "cards"):  # playing-card order can differ harmlessly: pair cards by identity+text
            pool = list(sc)
            pairs = []
            for r in rc:
                j = next((j for j, s in enumerate(pool) if card_fields(s) == card_fields(r)
                          and _d(s.get("value")).get("effect") == _d(r.get("value")).get("effect")), None)
                if j is None:
                    j = next((j for j, s in enumerate(pool) if card_fields(s) == card_fields(r)), None)
                if j is not None:
                    pairs.append((r, pool.pop(j)))
        else:
            pairs = list(zip(rc, sc))
        for r, s in pairs:
            if card_fields(r) != card_fields(s) or bool(_d(r.get("state")).get("debuff")) != bool(_d(s.get("state")).get("debuff")):
                continue
            rt, st = _d(r.get("value")).get("effect"), _d(s.get("value")).get("effect")
            cat = r.get("set")
            if a == "cards":
                cat = "DECK_" + cat
            _tally(stats, samples, cat, r.get("key") if r.get("set") not in ("DEFAULT", "ENHANCED") else r.get("label"), rt, st, where)
            if r.get("label") != s.get("label"):
                _tally(stats, samples, "LABEL", r.get("key"), r.get("label"), s.get("label"), where)
    rb, sb = _d(rec.get("blinds")), _d(sim.get("blinds"))
    for k in ("small", "big", "boss"):
        r, s = _d(rb.get(k)), _d(sb.get(k))
        if r.get("name") == s.get("name"):
            _tally(stats, samples, "BLIND", r.get("name"), r.get("effect"), s.get("effect"), where)
        if r.get("tag_name") == s.get("tag_name") and r.get("tag_name"):
            _tally(stats, samples, "TAG", r.get("tag_name"), r.get("tag_effect"), s.get("tag_effect"), where)


def _tally(stats, samples, cat, key, rt, st, where):
    ok = rt == st
    s = stats[cat]
    s["n"] += 1
    s["ok"] += ok
    d = s["distinct"].setdefault(f"{key}|{rt}", [0, 0])
    d[0] += 1
    d[1] += ok
    if not ok:
        k = (cat, key, rt)
        if k not in samples:
            samples[k] = {"cat": cat, "key": key, "rec": rt, "sim": st, "where": where}


def replay(run_dir, stats, samples, shapes):
    summ = json.load(open(os.path.join(run_dir, "summary.json")))
    events = sorted((json.loads(l) for l in open(os.path.join(run_dir, "decisions.jsonl"))), key=lambda e: e["id"])
    states = {}
    with gzip.open(os.path.join(run_dir, "states.jsonl.gz"), "rt", encoding="utf8") as f:
        for l in f:
            s = json.loads(l)
            states[s["id"]] = s["gs"]
    real_ok = {}
    for i, e in enumerate(events):
        if e.get("kind") == "action":
            ok = False
            for f in events[i + 1:]:
                if f.get("kind") in ("action", "decision", "forced"):
                    break
                if f.get("kind") == "outcome" and f.get("method") == e["method"]:
                    ok = True
                    break
            real_ok[e["id"]] = ok
    base_locked = jf._PATCHED["locked"]
    if base_locked is not None:
        jf._PATCHED["locked"] = base_locked | {k for k, t in jf.PROFILE_HISTORY.items() if summ["run_id"] < t}
    try:
        return _replay(summ, events, states, real_ok, stats, samples, shapes)
    finally:
        jf._PATCHED["locked"] = base_locked


def _replay(summ, events, states, real_ok, stats, samples, shapes):
    game = sim_backend.SimBalatro()
    game.call("start", {"seed": summ["seed"], "deck": summ.get("deck", "RED"), "stake": summ.get("stake", "WHITE")})
    res = {"run_id": summ["run_id"], "seed": summ["seed"], "result": summ.get("result"), "rec_ante": summ.get("ante"),
           "states_compared": 0, "states_exact": 0, "first_critical": None, "action_error": None,
           "actions_applied": 0, "rejections_matched": 0, "sim_accepted_real_rejected": [], "texts_compared_states": 0}
    last_rec = None
    for e in events:
        if e["id"] in states:
            rec_gs = states[e["id"]]
            rec = jf.snapshot(rec_gs)
            last_rec = rec
            sim_gs = game.state()
            d = jf.diff(rec, jf.snapshot(sim_gs))
            res["states_compared"] += 1
            res["states_exact"] += not d
            crit = [k for k in d if k in jf.CRITICAL or k.endswith("_multiset")]
            if crit and res["first_critical"] is None:
                res["first_critical"] = {"id": e["id"], "ante": rec["ante"], "round": rec["round_num"], "state": rec["state"],
                                         "fields": sorted(crit), "detail": {k: str(d[k])[:300] for k in crit}}
            if res["first_critical"] is None:  # objects still the same: compare texts and shape
                res["texts_compared_states"] += 1
                compare_texts(rec_gs, sim_gs, stats, samples, f"{summ['run_id']}#{e['id']}")
                shape_diff(rec_gs, sim_gs, "gs", shapes)
        if e.get("kind") != "action":
            continue
        method, params = e["method"], dict(e.get("params") or {})
        if method in ("start", "menu"):
            continue
        try:
            sim_now = jf.snapshot(game.state())
            fresh = last_rec and last_rec["state"] == sim_now["state"] and last_rec["round_num"] == sim_now["round_num"]
            for field in ("cards", "targets"):
                if fresh and field in params and last_rec.get("hand") and (field == "targets" or method in ("play", "discard", "use")):
                    m = jf.remap_cards(params[field], last_rec["hand"], sim_now["hand"])
                    if m is not None:
                        params[field] = m
            if not real_ok.get(e["id"], True):
                try:
                    game.call(method, params)
                    res["sim_accepted_real_rejected"].append({"id": e["id"], "method": method, "params": e.get("params")})
                except RPCError:
                    res["rejections_matched"] += 1
                continue
            game.call(method, params)
            res["actions_applied"] += 1
        except RPCError as exc:
            res["action_error"] = {"id": e["id"], "method": method, "params": e.get("params"), "error": str(exc)[:200],
                                   "ante": last_rec and last_rec["ante"], "round": last_rec and last_rec["round_num"]}
            break
    res["engine_errors"] = game.engine_errors
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", default="bench30-a", help="benchmark set to replay ('all' = every recorded run)")
    ap.add_argument("--runs", type=int, default=0)
    ap.add_argument("--run", action="append")
    ap.add_argument("--out")
    ap.add_argument("--no-profile", action="store_true", help="replay with the all-unlocked profile")
    ap.add_argument("-q", action="store_true")
    a = ap.parse_args()
    locked, disc = ([], None) if a.no_profile else historical_profile()
    jf.patch_profile(locked, disc)
    if a.run:
        dirs = a.run
    else:
        dirs = []
        for f in sorted(glob.glob(os.path.join(ROOT, "runs", "2026*", "summary.json"))):
            s = json.load(open(f))
            if (a.set == "all" or _d(s.get("benchmark")).get("name") == a.set) and s.get("backend") in (None, "balatrobot") \
                    and os.path.exists(os.path.join(os.path.dirname(f), "states.jsonl.gz")):
                dirs.append(os.path.dirname(f))
        if a.runs:
            dirs = dirs[: a.runs]
    stats = collections.defaultdict(lambda: {"n": 0, "ok": 0, "distinct": {}})
    samples, shapes, reps = {}, collections.Counter(), []
    t0 = time.time()
    for d in dirs:
        r = replay(d, stats, samples, shapes)
        reps.append(r)
        if not a.q and (r["first_critical"] or r["action_error"]):
            fc, ae = r["first_critical"], r["action_error"]
            print(f"{r['run_id']}: first critical {fc and (fc['ante'], fc['round'], fc['state'], fc['fields'])}"
                  f" | action error {ae and (ae['method'], ae['params'], ae['error'][:100])}")
    text = {}
    for cat, s in sorted(stats.items()):
        dd = s["distinct"]
        text[cat] = {"instances": s["n"], "instances_exact": s["ok"], "distinct": len(dd),
                     "distinct_exact": sum(1 for n, ok in dd.values() if n == ok)}
    tot = {k: sum(v[k] for c, v in text.items() if c != "LABEL") for k in ("instances", "instances_exact", "distinct", "distinct_exact")}
    summary = {
        "runs": len(reps), "seconds": round(time.time() - t0, 1),
        "runs_replayed_to_end_cleanly": sum(r["first_critical"] is None and r["action_error"] is None for r in reps),
        "runs_without_critical_divergence": sum(r["first_critical"] is None for r in reps),
        "states_compared": sum(r["states_compared"] for r in reps), "states_exact": sum(r["states_exact"] for r in reps),
        "actions_applied": sum(r["actions_applied"] for r in reps),
        "real_rejections_matched": sum(r["rejections_matched"] for r in reps),
        "sim_accepted_real_rejected": sum(len(r["sim_accepted_real_rejected"]) for r in reps),
        "engine_errors": sum(len(r["engine_errors"]) for r in reps),
        "first_critical_fields": dict(collections.Counter(str(r["first_critical"]["fields"]) for r in reps if r["first_critical"])),
        "action_errors": dict(collections.Counter(f"{r['action_error']['method']}: {r['action_error']['error'][:70]}" for r in reps if r["action_error"])),
        "text_match": {"total": tot, **text},
        "shape_mismatches": dict(shapes.most_common(40)),
    }
    print(json.dumps(summary, indent=1))
    bad = sorted(samples.values(), key=lambda x: (x["cat"], str(x["key"])))
    print(f"{len(bad)} distinct text mismatches")
    for b in bad[:60]:
        print(f"  [{b['cat']}] {b['key']}\n     rec: {b['rec']!r}\n     sim: {b['sim']!r}")
    if a.out:
        with open(a.out, "w", newline="\n", encoding="utf8") as f:
            json.dump({"jackdaw_commit": sim_backend.JACKDAW_COMMIT, "set": a.set, "summary": summary,
                       "text_mismatches": bad, "replays": reps}, f, indent=1, default=str)


if __name__ == "__main__":
    main()
