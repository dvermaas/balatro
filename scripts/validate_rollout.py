"""Validate src/rollout.py against ground truth in the Jackdaw sim (evaluation only, never policy).

For shop states sampled from recorded sim runs (v18 JEV runs under runs/sim, seed set sim30-a, and
no-JEV random-policy runs), the run is replayed in the sim to recover the TRUE engine state at that
shop decision. Then, per shop option:

  forecast   rollout.forecast() built ONLY from the public gamestate (serialised like BalatroBot)
  truth      the true engine state branched M times with fresh RNG seeds for the hidden future
             (GTxxxxxx), same option, same frozen build, same code policy
  fidelity   the true state replayed with the forecast's own seeds (ROxxxxxx): per-rollout
             agreement isolates reconstruction error from sampling noise
  gain       the current shop measure, jokers.gain() on the run's last 10 plays (as v18 saw it)

  python scripts/validate_rollout.py --states 40 --jobs 4      # -> docs/samples/rollout-validation.json
  python scripts/validate_rollout.py --random-dir runs/_rollout_validation/random

No JEV/API calls. Reads recordings only; writes the JSON sample (and nothing under runs/sim).
"""
from __future__ import annotations

import argparse
import copy
import gzip
import json
import math
import multiprocessing as mp
import os
import random
import statistics
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "src"))

RECENT_PLAYS = 10  # decisions.RECENT_PLAYS


def _load(run_dir):
    run_dir = Path(run_dir)
    summ = json.loads((run_dir / "summary.json").read_text(encoding="utf-8"))
    events = sorted((json.loads(l) for l in open(run_dir / "decisions.jsonl", encoding="utf-8")), key=lambda e: e["id"])
    states = {}
    with gzip.open(run_dir / "states.jsonl.gz", "rt", encoding="utf-8") as f:
        for l in f:
            s = json.loads(l)
            states[s["id"]] = s["gs"]
    return summ, events, states


def shop_points(run_dir):
    """First shop decision of each shop visit that has a recorded state: [(event id, ante, round)]."""
    _, events, states = _load(run_dir)
    out, seen = [], set()
    for e in events:
        if e.get("kind") == "decision" and e.get("question") == "shop" and e["id"] in states:
            key = (e.get("ante"), e.get("round"))
            if key not in seen:
                seen.add(key)
                out.append((e["id"], e.get("ante"), e.get("round")))
    return out


def replay_to(run_dir, event_id):
    """Replay a recorded sim run up to (not including the actions of) shop decision `event_id`.
    Returns (true engine gs, recorded public state, decision event, recent plays)."""
    import sim_backend
    from balatro import RPCError
    from hands import card_info, _d
    summ, events, states = _load(run_dir)
    game = sim_backend.SimBalatro()
    game.call("start", {"seed": summ["seed"], "deck": summ.get("deck") or "RED", "stake": summ.get("stake") or "WHITE"})
    recent, last_state = [], None
    for e in events:
        if e["id"] in states:
            last_state = states[e["id"]]
        if e["id"] == event_id:
            return game.gs, states[event_id], e, recent[-RECENT_PLAYS:]
        if e.get("kind") != "action" or e.get("method") in ("start", "menu"):
            continue
        params = e.get("params") or {}
        if e["method"] == "play" and last_state and last_state.get("state") == "SELECTING_HAND":
            rnd = _d(last_state.get("round"))
            recent.append({"infos": [card_info(c) for c in _d(last_state.get("hand")).get("cards") or []],
                           "combo": list(params.get("cards") or []),
                           "rnd": {"hands_left": rnd.get("hands_left"), "discards_left": rnd.get("discards_left")}})
        try:
            game.call(e["method"], params)
        except RPCError:
            pass  # rejected in the recording too (sim runs replay exactly)
    raise KeyError(event_id)


def _surv(r, bosses):
    return 1.0 if r and r[-1]["cleared"] and sum(b["type"] == "Boss" for b in r) >= bosses else 0.0


def _gain(pub, recent, opt):
    import jokers
    from hands import _d
    shop = _d(pub.get("shop")).get("cards") or []
    buys = [i for k, i in opt["actions"] if k == "buy"]
    sells = [i for k, i in opt["actions"] if k == "sell"]
    if opt["key"] == "skip":
        return 0.0
    if not buys:
        return None  # vouchers: no gain measure
    c = shop[buys[0]]
    if c.get("set") == "JOKER":
        return jokers.gain(pub, recent, add=c, remove=sells[0] if sells else None)
    if c.get("set") == "PLANET":
        lv = jokers.planet_level(_d(c.get("value")).get("effect"))
        return jokers.gain(pub, recent, level_up=lv) if lv else None
    return None


def evaluate(task):
    """One shop state (errors are returned as text: engine exceptions don't unpickle in the parent)."""
    try:
        return _evaluate(task)
    except Exception as e:  # noqa: BLE001
        import traceback
        return {"run": Path(task[0]).name, "event": task[1], "source": task[2],
                "error": f"{type(e).__name__}: {e}", "trace": traceback.format_exc(limit=8)}


def _evaluate(task):
    """One shop state: forecast vs truth vs fidelity vs gain for every supported option."""
    run_dir, event_id, source, n, m, f, bosses = task
    import rollout
    import sim_backend
    from jackdaw.engine.rng import PseudoRandom
    t0 = time.time()
    true_gs, recorded, dec, recent = replay_to(run_dir, event_id)
    pub = sim_backend.serialize(true_gs)
    same = all(pub.get(k) == recorded.get(k) for k in ("money", "ante_num", "round_num")) and \
        [c["label"] for c in pub["jokers"]["cards"]] == [c["label"] for c in recorded["jokers"]["cards"]] and \
        [c["label"] for c in pub["shop"]["cards"]] == [c["label"] for c in recorded["shop"]["cards"]]
    replay_s = time.time() - t0
    fc = rollout.forecast(pub, n=n, bosses=bosses)
    opts = {o["key"]: o for o in rollout.options(pub) if o["key"] in fc["options"]}
    rows = {}
    for key, o in opts.items():
        pol = rollout.Policy(rollout.option_jokers(pub, o))
        truth, fid = [], []
        for tag, count, sink in (("GT", m, truth), ("RO", f, fid)):
            for k in range(count):
                gs = copy.deepcopy(true_gs)
                gs["rng"] = PseudoRandom(rollout._seed(k, tag))
                rollout.apply_option(gs, o)
                sink.append(rollout.play_horizon(gs, pol, bosses))
        fruns = fc["_runs"][key]
        same_outcome = sum(_surv(a, bosses) == _surv(b, bosses) for a, b in zip(fid, fruns)) / max(1, len(fid))
        same_path = sum([x["cleared"] for x in a] == [x["cleared"] for x in b] for a, b in zip(fid, fruns)) / max(1, len(fid))
        rows[key] = {"desc": o["desc"], "cost": o["cost"], "set": o.get("set"),
                     "forecast": {k2: v for k2, v in fc["options"][key].items() if k2 not in ("desc", "cost")},
                     "truth": rollout.summarize(truth, bosses),
                     "fidelity": {"n": len(fid), "same_survival": round(same_outcome, 3),
                                  "same_blind_path": round(same_path, 3)},
                     "gain": _gain(pub, recent, o),
                     "_f": [_surv(r, bosses) for r in fruns], "_t": [_surv(r, bosses) for r in truth],
                     "_fb": [sum(b["cleared"] for b in r) for r in fruns],
                     "_tb": [sum(b["cleared"] for b in r) for r in truth]}
    return {"run": Path(run_dir).name, "source": source, "event": event_id, "seed": pub.get("seed"),
            "ante": pub.get("ante_num"), "round": pub.get("round_num"), "money": pub.get("money"),
            "jokers": [c["label"] for c in pub["jokers"]["cards"]],
            "shop": [f"${c['cost']['buy']} {c['label']}" for c in pub["shop"]["cards"]],
            "vouchers": [c["label"] for c in pub["vouchers"]["cards"]] if isinstance(pub.get("vouchers"), dict) else [],
            "replay_matches_recording": same, "choice": dec.get("choice"), "recent_plays": len(recent),
            "forecast_seconds": fc["seconds"], "forecast_rollouts_per_option": fc["rollouts"],
            "n_options": len(opts), "unsupported": fc["unsupported"], "replay_seconds": round(replay_s, 2),
            "seconds": round(time.time() - t0, 1), "options": rows}


# ---------------------------------------------------------------------------
# Aggregation
# ---------------------------------------------------------------------------

def _rank(xs):
    order = sorted(range(len(xs)), key=lambda i: xs[i])
    r = [0.0] * len(xs)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and xs[order[j + 1]] == xs[order[i]]:
            j += 1
        for k in range(i, j + 1):
            r[order[k]] = (i + j) / 2
        i = j + 1
    return r


def spearman(a, b):
    if len(a) < 3:
        return None
    ra, rb = _rank(a), _rank(b)
    ma, mb = statistics.mean(ra), statistics.mean(rb)
    num = sum((x - ma) * (y - mb) for x, y in zip(ra, rb))
    den = math.sqrt(sum((x - ma) ** 2 for x in ra) * sum((y - mb) ** 2 for y in rb))
    return num / den if den else None


def _bin(pairs):
    out = {}
    for a, b in pairs:
        out.setdefault(min(4, int(a * 5)), []).append((a, b))
    return out


def value(summary):
    """Ranking value: P(survive the horizon), ties broken by expected blinds cleared."""
    return summary["p_survive"] + 0.01 * summary["exp_blinds_cleared"]


def choice_key(choice):
    if choice in ("leave", "reroll"):
        return "skip"  # neither buys anything now (reroll is not modelled)
    return choice


def aggregate(results, bosses):
    pairs, fid_s, fid_p, se_unpaired, se_paired = [], [], [], [], []
    boss_pairs, se_paired_b, se_unpaired_b = [], [], []
    rho_fc, rho_gain_same, rho_fc_same, top_fc, top_gain, top_fc_same = [], [], [], [], [], []
    v18 = {"states": 0, "differs": 0, "choice_unsupported": 0, "regret_v18": [], "regret_forecast": [], "cases": []}
    disagreements = []
    for s in results:
        rows = s["options"]
        keys = list(rows)
        for k in keys:
            r = rows[k]
            pairs.append((r["forecast"]["p_survive"], r["truth"]["p_survive"], r["forecast"]["exp_blinds_cleared"],
                          r["truth"]["exp_blinds_cleared"]))
            fid_s.append(r["fidelity"]["same_survival"])
            fid_p.append(r["fidelity"]["same_blind_path"])
            fb = next((i for i, b in enumerate(r["forecast"]["blinds"]) if b["blind"].endswith("boss")), None)
            tb = next((i for i, b in enumerate(r["truth"]["blinds"]) if b["blind"].endswith("boss")), None)
            if fb is not None and tb is not None:
                boss_pairs.append((r["forecast"]["blinds"][fb]["p_clear"], r["truth"]["blinds"][tb]["p_clear"]))
            if k != "skip" and "skip" in rows:
                for a_, b_, sink_p, sink_u in (("_f", "_f", se_paired, se_unpaired), ("_fb", "_fb", se_paired_b, se_unpaired_b)):
                    d = [x - y for x, y in zip(r[a_], rows["skip"][b_])]
                    if len(d) > 1:
                        sink_p.append(statistics.stdev(d) / math.sqrt(len(d)))
                        sink_u.append(math.sqrt(statistics.pvariance(r[a_]) / len(d) + statistics.pvariance(rows["skip"][b_]) / len(d)))
        if len(keys) < 2:
            continue
        fv = [value(rows[k]["forecast"]) for k in keys]
        tv = [value(rows[k]["truth"]) for k in keys]
        t_best = max(tv)
        # GT standard error of the best option's value (binomial, M branches)
        se_t = max(math.sqrt(max(rows[k]["truth"]["p_survive"] * (1 - rows[k]["truth"]["p_survive"]), 0.0)
                             / rows[k]["truth"]["n"]) for k in keys)
        f_pick = keys[fv.index(max(fv))]
        rho = spearman(fv, tv)
        if rho is not None:
            rho_fc.append(rho)
        top_fc.append(t_best - value(rows[f_pick]["truth"]) <= max(se_t, 1e-9) + 1e-9)
        gk = [k for k in keys if rows[k]["gain"] is not None]
        if len(gk) >= 2:
            gv = [rows[k]["gain"] for k in gk]
            tg = [value(rows[k]["truth"]) for k in gk]
            fg = [value(rows[k]["forecast"]) for k in gk]
            r1, r2 = spearman(gv, tg), spearman(fg, tg)
            if r1 is not None and r2 is not None:
                rho_gain_same.append(r1)
                rho_fc_same.append(r2)
            g_pick, f_pick2 = gk[gv.index(max(gv))], gk[fg.index(max(fg))]
            tb = max(tg)
            top_gain.append(tb - value(rows[g_pick]["truth"]) <= se_t + 1e-9)
            top_fc_same.append(tb - value(rows[f_pick2]["truth"]) <= se_t + 1e-9)
            if g_pick != f_pick2:
                disagreements.append({
                    "state": f"{s['run']}#{s['event']} ante {s['ante']} ${s['money']}", "jokers": s["jokers"],
                    "gain_pick": f"{g_pick} {rows[g_pick]['desc']} (gain {round(rows[g_pick]['gain'], 2)})",
                    "forecast_pick": f"{f_pick2} {rows[f_pick2]['desc']}",
                    "truth_p_survive": {g_pick: rows[g_pick]["truth"]["p_survive"],
                                        f_pick2: rows[f_pick2]["truth"]["p_survive"]},
                    "forecast_p_survive": {g_pick: rows[g_pick]["forecast"]["p_survive"],
                                           f_pick2: rows[f_pick2]["forecast"]["p_survive"]},
                    "truth_agrees_with": "forecast" if value(rows[f_pick2]["truth"]) > value(rows[g_pick]["truth"]) else
                    "gain" if value(rows[g_pick]["truth"]) > value(rows[f_pick2]["truth"]) else "tie"})
        if s["source"] == "v18":
            v18["states"] += 1
            ck = choice_key(s["choice"])
            if ck not in rows:
                v18["choice_unsupported"] += 1
                v18["cases"].append({"state": f"{s['run']}#{s['event']}", "ante": s["ante"], "v18": s["choice"],
                                     "forecast_pick": f_pick, "note": "v18 choice not simulated (pack/tarot)"})
                continue
            differs = ck != f_pick
            v18["differs"] += differs
            v18["regret_v18"].append(t_best - value(rows[ck]["truth"]))
            v18["regret_forecast"].append(t_best - value(rows[f_pick]["truth"]))
            v18["cases"].append({"state": f"{s['run']}#{s['event']}", "ante": s["ante"], "money": s["money"],
                                 "v18": f"{s['choice']} {rows[ck]['desc']}", "forecast_pick": f"{f_pick} {rows[f_pick]['desc']}",
                                 "truth_p_survive": {"v18": rows[ck]["truth"]["p_survive"],
                                                     "forecast": rows[f_pick]["truth"]["p_survive"],
                                                     "best": round(max(rows[k]["truth"]["p_survive"] for k in keys), 3)},
                                 "forecast_p_survive": {"v18": rows[ck]["forecast"]["p_survive"],
                                                        "forecast": rows[f_pick]["forecast"]["p_survive"]}})
    bins = {}
    for fp, tp, _, _ in pairs:
        b = min(4, int(fp * 5))
        bins.setdefault(b, []).append((fp, tp))
    calib = [{"forecast_bin": f"{b / 5:.1f}-{(b + 1) / 5:.1f}", "n": len(v),
              "mean_forecast": round(statistics.mean(x for x, _ in v), 3),
              "mean_truth": round(statistics.mean(y for _, y in v), 3)} for b, v in sorted(bins.items())]
    fps, tps = [p[0] for p in pairs], [p[1] for p in pairs]
    fbs, tbs = [p[2] for p in pairs], [p[3] for p in pairs]

    def mean(xs):
        return round(statistics.mean(xs), 3) if xs else None

    def pearson(a, b):
        if len(a) < 3 or statistics.pstdev(a) == 0 or statistics.pstdev(b) == 0:
            return None
        ma, mb = statistics.mean(a), statistics.mean(b)
        return sum((x - ma) * (y - mb) for x, y in zip(a, b)) / (len(a) * statistics.pstdev(a) * statistics.pstdev(b))

    secs = [s["forecast_seconds"] for s in results]
    per6 = [s["forecast_seconds"] / max(1, s["n_options"]) * 6 for s in results]
    return {
        "states": len(results), "options": len(pairs), "horizon_bosses": bosses,
        "speed": {"forecast_seconds_median": mean([statistics.median(secs)]), "forecast_seconds_max": max(secs),
                  "seconds_per_6_options_median": round(statistics.median(per6), 2),
                  "seconds_per_6_options_p90": round(sorted(per6)[int(0.9 * (len(per6) - 1))], 2),
                  "options_per_state_median": statistics.median(s["n_options"] for s in results)},
        "calibration_p_survive": {"mean_abs_error": mean([abs(a - b) for a, b in zip(fps, tps)]),
                                  "bias_forecast_minus_truth": mean([a - b for a, b in zip(fps, tps)]),
                                  "pearson": round(pearson(fps, tps), 3) if pearson(fps, tps) is not None else None,
                                  "bins": calib},
        "calibration_blinds_cleared": {"mean_abs_error": mean([abs(a - b) for a, b in zip(fbs, tbs)]),
                                       "bias": mean([a - b for a, b in zip(fbs, tbs)]),
                                       "pearson": round(pearson(fbs, tbs), 3) if pearson(fbs, tbs) is not None else None},
        "calibration_next_boss_clear": {
            "pairs": len(boss_pairs),
            "mean_abs_error": mean([abs(a - b) for a, b in boss_pairs]),
            "bias": mean([a - b for a, b in boss_pairs]),
            "pearson": round(pearson([a for a, _ in boss_pairs], [b for _, b in boss_pairs]), 3) if len(boss_pairs) > 2 else None,
            "bins": [{"forecast_bin": f"{b / 5:.1f}-{(b + 1) / 5:.1f}", "n": len(v),
                      "mean_forecast": round(statistics.mean(x for x, _ in v), 3),
                      "mean_truth": round(statistics.mean(y for _, y in v), 3)}
                     for b, v in sorted(_bin(boss_pairs).items())]},
        "reconstruction_fidelity": {"same_survival_same_seed": mean(fid_s), "same_blind_path_same_seed": mean(fid_p)},
        "variance": {"se_p_survive_n40_mean": mean([math.sqrt(a * (1 - a) / 40) for a in fps]),
                     "se_next_boss_clear_n40_mean": mean([math.sqrt(a * (1 - a) / 40) for a, _ in boss_pairs]),
                     "se_blinds_cleared_minus_skip_paired_median": round(statistics.median(se_paired_b), 3) if se_paired_b else None,
                     "se_blinds_cleared_minus_skip_unpaired_median": round(statistics.median(se_unpaired_b), 3) if se_unpaired_b else None,"se_option_minus_skip_paired_median": round(statistics.median(se_paired), 3) if se_paired else None,
                     "se_option_minus_skip_unpaired_median": round(statistics.median(se_unpaired), 3) if se_unpaired else None},
        "rank_agreement": {"forecast_vs_truth_spearman_mean": mean(rho_fc), "states_ranked": len(rho_fc),
                           "forecast_top1_within_truth_se": mean([float(x) for x in top_fc]),
                           "on_gain_measurable_options": {
                               "states": len(top_gain),
                               "gain_vs_truth_spearman_mean": mean(rho_gain_same),
                               "forecast_vs_truth_spearman_mean": mean(rho_fc_same),
                               "gain_top1_within_truth_se": mean([float(x) for x in top_gain]),
                               "forecast_top1_within_truth_se": mean([float(x) for x in top_fc_same])}},
        "gain_vs_forecast_disagreements": {
            "count": len(disagreements),
            "truth_sides_with": {k: sum(d["truth_agrees_with"] == k for d in disagreements) for k in ("forecast", "gain", "tie")},
            "cases": disagreements},
        "v18": {"states": v18["states"], "forecast_pick_differs": v18["differs"],
                "v18_choice_not_simulated": v18["choice_unsupported"],
                "mean_truth_regret_v18_choice": mean(v18["regret_v18"]),
                "mean_truth_regret_forecast_pick": mean(v18["regret_forecast"]), "cases": v18["cases"]},
    }


def pick_states(sources, count, rng):
    """Stratified sample of shop points (antes 2-7 weighted; a few ante 1)."""
    pool = []
    for run_dir, source in sources:
        for eid, ante, rnd in shop_points(run_dir):
            pool.append((str(run_dir), eid, source, int(ante or 1)))
    rng.shuffle(pool)
    by_src = {}
    for p in pool:
        by_src.setdefault(p[2], []).append(p)
    out = []
    quota = {"v18": int(count * 0.6 + 0.5)}
    quota["random"] = count - quota["v18"]
    for src, items in by_src.items():
        q = quota.get(src, 0)
        # one per run per ante band first, then fill; ante >= 2 preferred
        items.sort(key=lambda p: (p[3] < 2, rng.random()))
        seen, pick = set(), []
        for p in items:
            k = (p[0], p[3])
            if k not in seen and len(pick) < q:
                seen.add(k)
                pick.append(p)
        out += pick
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--states", type=int, default=40)
    ap.add_argument("--n", type=int, default=40, help="forecast rollouts per option")
    ap.add_argument("--m", type=int, default=40, help="ground-truth branches per option")
    ap.add_argument("--f", type=int, default=20, help="same-seed fidelity replays per option")
    ap.add_argument("--bosses", type=int, default=2)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--random-dir", default=str(ROOT / "runs" / "_rollout_validation" / "random"))
    ap.add_argument("--out", default=str(ROOT / "docs" / "samples" / "rollout-validation.json"))
    ap.add_argument("--seed", type=int, default=20261016)
    a = ap.parse_args()

    sources = []
    for d in sorted((ROOT / "runs" / "sim").iterdir()):
        try:
            s = json.loads((d / "summary.json").read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        if s.get("backend") == "jackdaw" and (s.get("benchmark") or {}).get("name") == "sim30-a" \
                and s.get("prompt_version") == "d22adb59" and s.get("result") in ("won", "lost"):
            sources.append((d, "v18"))
    rd = Path(a.random_dir)
    if rd.exists():
        for d in sorted(rd.iterdir()):
            if (d / "summary.json").exists() and (d / "states.jsonl.gz").exists():
                sources.append((d, "random"))
    tasks = [(p[0], p[1], p[2], a.n, a.m, a.f, a.bosses) for p in pick_states(sources, a.states, random.Random(a.seed))]
    print(f"{len(sources)} runs, {len(tasks)} states", flush=True)
    t0 = time.time()
    results, errors = [], []
    with mp.get_context("spawn").Pool(a.jobs) as pool:
        for r in pool.imap_unordered(evaluate, tasks):
            if r.get("error"):
                print(json.dumps(r), flush=True)
                errors.append(r)
                continue
            results.append(r)
            print(json.dumps({k: r[k] for k in ("run", "event", "source", "ante", "n_options", "forecast_seconds", "seconds",
                                                 "replay_matches_recording")}), flush=True)
    results.sort(key=lambda r: (r["source"], r["run"], r["event"]))
    agg = aggregate(results, a.bosses)
    agg["wall_seconds"] = round(time.time() - t0, 1)
    agg["errors"] = [{k: e[k] for k in ("run", "event", "error")} for e in errors]
    agg["params"] = {"n_forecast": a.n, "m_truth": a.m, "f_fidelity": a.f, "bosses": a.bosses,
                     "sources": {"v18": sum(1 for _, s in sources if s == "v18"), "random": sum(1 for _, s in sources if s == "random")}}
    for r in results:
        for row in r["options"].values():
            for k in ("_f", "_t", "_fb", "_tb"):
                row.pop(k, None)
    out = {"summary": agg, "states": results}
    Path(a.out).write_text(json.dumps(out, indent=1), encoding="utf-8")
    print(json.dumps({k: v for k, v in agg.items() if k not in ("gain_vs_forecast_disagreements", "v18")}, indent=1))
    print(json.dumps({"v18": {k: v for k, v in agg["v18"].items() if k != "cases"},
                      "disagreements": {k: v for k, v in agg["gain_vs_forecast_disagreements"].items() if k != "cases"}}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
