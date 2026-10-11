"""Derived metrics per run and per prompt version, computed from runs/*/decisions.jsonl (cached by mtime)."""

import json
from pathlib import Path

from recorder import prompt_versions

RUNS = Path(__file__).parent / "runs"
_cache = {}


def load_decisions(run_dir):
    path = run_dir / "decisions.jsonl"
    if not path.exists():
        return []
    with open(path, encoding="utf-8") as f:
        return [json.loads(line) for line in f if line.strip()]


def run_metrics(run_dir):
    path = run_dir / "decisions.jsonl"
    key = (str(run_dir), path.stat().st_mtime if path.exists() else 0)
    if key in _cache:
        return _cache[key]
    conf = {}
    m = {"plays": 0, "discards": 0, "pack_picks": 0, "pack_takes": 0, "packs_bought": 0,
         "cards_bought": 0, "wasted_discards": None, "jokers_at_end": 0}
    last_move = None
    for e in load_decisions(run_dir):
        if e["kind"] != "decision":
            continue
        q, c = e["question"], e["choice"]
        conf.setdefault(q, []).append(e.get("confidence") or 0)
        g = (e.get("state_sent") or {}).get("game", {})
        m["jokers_at_end"] = len(g.get("jokers") or [])
        if q == "move":
            last_move = g
            m["plays" if c.startswith("play") else "discards"] += 1
        elif q == "pack":
            m["pack_picks"] += 1
            m["pack_takes"] += c.startswith("take")
        elif q == "shop":
            m["packs_bought"] += c.startswith("buy_pack")
            m["cards_bought"] += c.startswith("buy_card")
    if last_move is not None:
        m["wasted_discards"] = last_move.get("discards_left")
    m["confidence"] = {q: round(sum(v) / len(v), 3) for q, v in conf.items()}
    _cache[key] = m
    return m


def all_runs():
    out = []
    for p in sorted(RUNS.glob("*/summary.json"), reverse=True):
        try:
            s = json.loads(p.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        out.append({**s, "metrics": run_metrics(p.parent)})
    return out


def _avg(xs):
    xs = [x for x in xs if x is not None]
    return round(sum(xs) / len(xs), 2) if xs else None


def version_stats():
    runs = all_runs()
    out = []
    for v in prompt_versions():
        rs = [r for r in runs if r.get("prompt_version") == v["version"]]
        done = [r for r in rs if r["result"] in ("won", "lost")]
        lost = [r for r in done if r["result"] == "lost"]
        ms = [r["metrics"] for r in rs]
        qs = sorted({q for m in ms for q in m["confidence"]})
        picks = sum(m["pack_picks"] for m in ms)
        out.append({
            **v,
            "runs": len(rs),
            "finished": len(done),
            "wins": sum(r["result"] == "won" for r in done),
            "avg_ante": _avg([r["ante"] for r in done]),
            "best_ante": max((r["ante"] or 0 for r in rs), default=None),
            "avg_round": _avg([r["round"] for r in done]),
            "avg_decisions": _avg([r["decisions"] for r in rs]),
            "avg_tokens": _avg([r["input_tokens"] + r["output_tokens"] for r in rs]),
            "avg_confidence": _avg([r["avg_confidence"] for r in rs]),
            "confidence_by_question": {q: _avg([m["confidence"].get(q) for m in ms]) for q in qs},
            "pack_take_rate": round(sum(m["pack_takes"] for m in ms) / picks, 2) if picks else None,
            "discard_share": _avg([m["discards"] / (m["plays"] + m["discards"]) for m in ms
                                   if m["plays"] + m["discards"]]),
            "avg_wasted_discards_on_loss": _avg([r["metrics"]["wasted_discards"] for r in lost]),
            "avg_jokers_at_end": _avg([m["jokers_at_end"] for m in ms]),
        })
    return out


def benchmark_matrix():
    """Seed x version grid for the fixed benchmark, plus a summary row per version."""
    import benchmark

    runs = [r for r in all_runs() if (r.get("benchmark") or {}).get("name") == benchmark.NAME
            and r["result"] in ("won", "lost")]
    versions = []
    for v in prompt_versions():
        rs = {}
        for r in sorted(runs, key=lambda r: r["started"]):  # latest attempt per seed wins
            if r["prompt_version"] == v["version"]:
                rs[r["seed"]] = r
        if not rs and not v["current"]:
            continue
        done = list(rs.values())
        antes = sorted(r["ante"] or 0 for r in done)
        versions.append({
            "version": v["version"], "label": v["label"], "current": v["current"], "note": v["note"],
            "done": len(done), "of": len(benchmark.SEEDS),
            "wins": sum(r["result"] == "won" for r in done),
            "avg_ante": _avg([r["ante"] for r in done]),
            "median_ante": antes[len(antes) // 2] if antes else None,
            "avg_round": _avg([r["round"] for r in done]),
            "avg_confidence": _avg([r["avg_confidence"] for r in done]),
            "avg_tokens": _avg([r["input_tokens"] + r["output_tokens"] for r in done]),
            "avg_seconds": _avg([r["ended"] - r["started"] for r in done if r.get("ended")]),
            "cells": {s: {"result": r["result"], "ante": r["ante"], "round": r["round"], "run_id": r["run_id"]}
                      for s, r in rs.items()},
        })
    return {"name": benchmark.NAME, "seeds": benchmark.SEEDS, "versions": versions}
