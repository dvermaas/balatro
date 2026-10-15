"""Where and by how much do benchmark runs die? Reads summary.json + decisions.jsonl only.

Usage: .venv/Scripts/python scripts/audit_losses.py [version ...]   (default: v15 v17 v18)
Writes docs/samples/loss-audit.json.
"""
import glob, json, os, statistics as st, sys
from collections import Counter, defaultdict

VERSIONS = sys.argv[1:] or ["6725b5bb", "11845ce8", "d22adb59"]
BENCH = "bench30-a"


def load(run):
    s = json.load(open(os.path.join(run, "summary.json")))
    rows = [json.loads(l) for l in open(os.path.join(run, "decisions.jsonl"), encoding="utf-8")]
    return s, rows


def analyse(s, rows):
    rounds = [r for r in rows if r["kind"] == "round_end"]
    plays = [r for r in rows if r["kind"] == "outcome" and r.get("method") == "play"]
    decs = [r for r in rows if r["kind"] == "decision" and isinstance(r.get("state_sent"), dict)
            and isinstance(r["state_sent"].get("round"), dict)]
    out = {"seed": s["seed"], "result": s["result"], "ante": s["ante"], "round": s["round"],
           "jokers": (plays[-1].get("jokers") if plays else []) or [],
           # margin = scored/target for every cleared blind, in order
           "cleared": [(r["ante"], r["blind"], r["target"], r["scored"], r.get("hands_left")) for r in rounds]}
    if s["result"] == "lost" and decs:
        rd = decs[-1]["state_sent"]["round"]
        target = rd.get("chip_target") or 0
        final = plays[-1]["chips"] if plays else rd.get("chips_scored", 0)
        # best single hand in the fatal round = largest chips_delta among plays after the last round_end
        last_end = rounds[-1]["id"] if rounds else -1
        fatal = [p.get("chips_delta", 0) for p in plays if p["id"] > last_end]
        out.update(fatal_blind=rd.get("blind"), boss_effect=rd.get("boss_effect"), target=target,
                   final=final, ratio=round(final / target, 3) if target else None,
                   best_hand=max(fatal) if fatal else 0, hands_played=len(fatal),
                   best_hand_share=round(max(fatal) / target, 3) if fatal and target else None)
    return out


def main():
    res = {}
    for v in VERSIONS:
        cells = {}
        for sp in sorted(glob.glob("runs/2026*/summary.json")):
            run = os.path.dirname(sp)
            try:
                s = json.load(open(sp))
            except Exception:
                continue
            b = s.get("benchmark") or {}
            if s.get("prompt_version") != v or b.get("name") != BENCH or s.get("result") not in ("won", "lost"):
                continue
            cells[s["seed"]] = run  # latest terminal cell per seed
        runs = [analyse(*load(r)) for r in cells.values()]
        losses = [r for r in runs if r["result"] == "lost" and r.get("ratio") is not None]
        ratios = [r["ratio"] for r in losses]
        buckets = Counter("<25%" if x < .25 else "25-50%" if x < .5 else "50-75%" if x < .75 else "75-100%" for x in ratios)
        by_ante = Counter(r["ante"] for r in losses)
        boss = Counter((r["fatal_blind"] or "?") for r in losses if r["fatal_blind"] not in ("Small Blind", "Big Blind"))
        kind = Counter("boss" if r["fatal_blind"] not in ("Small Blind", "Big Blind") else r["fatal_blind"] for r in losses)
        # power trend: median scored/target of cleared blinds per ante, across all runs
        trend = defaultdict(list)
        for r in runs:
            for a, _, t, sc, _ in r["cleared"]:
                if t:
                    trend[a].append(sc / t)
        res[v] = {
            "cells": len(runs), "wins": sum(r["result"] == "won" for r in runs), "losses": len(losses),
            "death_ante": dict(sorted(by_ante.items())), "death_blind_kind": dict(kind),
            "final_over_target": dict(buckets), "median_ratio": round(st.median(ratios), 3) if ratios else None,
            "median_best_hand_share": round(st.median([r["best_hand_share"] for r in losses if r["best_hand_share"] is not None]), 3) if losses else None,
            "fatal_bosses": dict(boss.most_common()),
            "cleared_margin_median_by_ante": {a: round(st.median(x), 2) for a, x in sorted(trend.items())},
            "median_jokers_at_death": st.median([len(r["jokers"]) for r in losses]) if losses else None,
            "losses_detail": [{k: r[k] for k in ("seed", "ante", "fatal_blind", "boss_effect", "target", "final", "ratio",
                                                 "best_hand_share", "hands_played", "jokers")} for r in losses],
        }
    json.dump(res, open("docs/samples/loss-audit.json", "w"), indent=1)
    for v, r in res.items():
        print(v, {k: r[k] for k in r if k != "losses_detail"})


if __name__ == "__main__":
    main()
