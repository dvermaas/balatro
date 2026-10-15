"""Predeclared bounded JEV diagnostics; never touches the game or production policy.

Six tactical positions x two mechanics conditions x two orders x two repeats.
Ten frozen shops x two question orders x two repeats (unchanged item questions).
Tactics are immediate best-play questions, deliberately not full-policy replays.
Shop scores measure repeat/order sensitivity, not purchase or win quality.
"""
import argparse
import copy
import hashlib
import json
import os
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT / "src"), str(ROOT / "scripts")]
from typesafe_sdk import Choice, Score, TypeSafeClient
from jev_bot import load_env
from diagnostics.compare_mechanics import current_candidates
from diagnostics.jackdaw_reference import evaluate, verify_checkout
from diagnostics.mechanics_candidate import candidate_module

MODEL = "jev-1.13.0"
TACTICAL = ("D01", "D12", "D04", "D10", "D15", "D26")
BANK = ROOT / "docs/samples/diagnostic-positions-v1.json"
OUT = ROOT / "docs/samples/diagnostic-selection-v1.jsonl"
PLAN = ROOT / "docs/samples/diagnostic-selection-plan-v1.json"


def make_request(position, condition, order):
    recorded = position["recorded"]
    state = copy.deepcopy(recorded["state_sent"])
    if recorded["question"] == "shop":
        qs = copy.deepcopy(recorded["questions"])
        if order == "reverse":
            qs = dict(reversed(list(qs.items())))
        return state, {k: Score.model_validate(v) for k, v in qs.items()}, None
    import decisions
    import jokers
    obs = position["observation"]
    plays, _ = current_candidates(obs, jokers if condition == "baseline" else candidate_module())
    info = decisions._round_info(obs)
    criteria = {f"play_{i}": decisions._play_obj(p, info, 1.1) for i, p in enumerate(plays, 1)}
    state.pop("best_draw_by_measurement", None)
    state["strongest_play"] = decisions._play_obj(plays[0], info, 1.1)
    state["hand_levels"] = decisions._levels(obs, {p["hand"] for p in plays})
    state.pop("jokers_not_in_estimates", None)
    # Existing best-play instructions only: no additional nudging or new model.
    instruction = decisions.INSTRUCTIONS["best_play"]
    if order == "reverse":
        criteria = dict(reversed(list(criteria.items())))
    truth = {f"play_{i}": {"cards": p["cards"], "score": evaluate(obs, p["cards"])["total"]}
             for i, p in enumerate(plays, 1)}
    return state, {"best_play": Choice(instructions=instruction, criteria=criteria)}, truth


def planned_calls(positions):
    calls = []
    chosen = [p for p in positions if p["case_id"] in TACTICAL or p["stratum"] == "shop"]
    for repeat in range(2):
        for order in ("original", "reverse"):
            for p in chosen:
                conditions = ["baseline", "candidate"] if p["case_id"] in TACTICAL else ["recorded_shop"]
                if (repeat + (order == "reverse")) % 2:
                    conditions.reverse()
                for condition in conditions:
                    calls.append({"case_id": p["case_id"], "condition": condition, "order": order, "repeat": repeat})
    return calls


def summarize(rows):
    tactics = [r for r in rows if r.get("truth")]
    summary = {"completed": len(rows), "errors": sum("error" in r for r in rows),
               "models": sorted({r["model"] for r in rows if "model" in r}), "tactics": {}, "shops": {}}
    for condition in ("baseline", "candidate"):
        subset = [r for r in tactics if r["condition"] == condition]
        summary["tactics"][condition] = {
            "calls": len(subset),
            "selected_reference_best_offered": sum(r["selected_score"] == r["best_offered_score"] for r in subset),
            "by_case": {case: [{k: r[k] for k in ("order", "repeat", "choice", "selected_score", "best_offered_score")}
                        for r in subset if r["case_id"] == case] for case in TACTICAL}}
    for case in sorted({r["case_id"] for r in rows if r["condition"] == "recorded_shop"}):
        subset = [r for r in rows if r["case_id"] == case and "answers" in r]
        values = defaultdict(list)
        top_sets = []
        for r in subset:
            scores = {k: v["score"] for k, v in r["answers"].items()}
            top_sets.append(sorted(k for k, v in scores.items() if v == max(scores.values())))
            for k, v in scores.items():
                values[k].append(v)
        summary["shops"][case] = {"calls": len(subset), "top_item_sets": top_sets,
            "item_score_ranges": {k: [min(v), max(v)] for k, v in values.items()}}
    (ROOT / "docs/samples/diagnostic-selection-results-v1.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in summary.items() if k not in ("tactics", "shops")}))
    for condition, result in summary["tactics"].items():
        print(condition, result["selected_reference_best_offered"], "/", result["calls"], "best immediate reference score")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--execute", action="store_true")
    ap.add_argument("--summarize", action="store_true")
    args = ap.parse_args()
    if args.summarize:
        summarize([json.loads(s) for s in OUT.read_text(encoding="utf-8").splitlines()])
        return
    verify_checkout()
    bank = json.loads(BANK.read_text(encoding="utf-8"))
    positions = {p["case_id"]: p for p in bank["positions"]}
    calls = planned_calls(bank["positions"])
    plan = {"model": MODEL, "bank_sha256": hashlib.sha256(BANK.read_bytes()).hexdigest(), "calls": calls,
            "primary_diagnostic": "Selected immediate reference score, with identical instruction and bounded candidates",
            "limits": "Neither full-policy replay nor continuation; order and stochastic sensitivity reported together, no win-rate inference"}
    if PLAN.exists():
        if json.loads(PLAN.read_text(encoding="utf-8")) != plan:
            raise RuntimeError("Frozen plan changed")
    else:
        PLAN.write_text(json.dumps(plan, indent=2) + "\n", encoding="utf-8")
    prepared = []
    for call in calls:
        prepared.append((call, make_request(positions[call["case_id"]], call["condition"], call["order"])))
    if not args.execute:
        print(f"Validated and froze {len(calls)} requests. No API calls made.")
        return
    # Resume only missing cells, never replace a completed or failed attempt silently.
    rows = [json.loads(s) for s in OUT.read_text(encoding="utf-8").splitlines()] if OUT.exists() else []
    def identity(row):
        return tuple(row[k] for k in ("case_id", "condition", "order", "repeat"))
    completed = {identity(r) for r in rows}
    load_env()
    client = TypeSafeClient(api_key=os.environ.get("JEV_API_KEY") or os.environ.get("TYPESAFE_API_KEY"), model=MODEL)
    with OUT.open("a", encoding="utf-8") as out:
        for call, (state, questions, truth) in prepared:
            if identity(call) in completed:
                continue
            row = dict(call)
            try:
                response = client.system_one(state=state, questions=questions)
                row.update(model=response.model, answers={k: a.model_dump() for k, a in response.answers.items()},
                           input_tokens=response.usage.input_tokens, output_tokens=response.usage.output_tokens,
                           request_sha256=hashlib.sha256(json.dumps({"state": state,
                               "questions": {k: q.model_dump() for k, q in questions.items()}}).encode()).hexdigest())
                if truth:
                    probabilities = dict(response.answers["best_play"].probabilities)
                    choice = max(probabilities, key=probabilities.get)
                    row.update(truth=truth, choice=choice, selected_score=truth[choice]["score"],
                               best_offered_score=max(v["score"] for v in truth.values()))
            except Exception as error:
                row["error"] = type(error).__name__
                out.write(json.dumps(row) + "\n")
                out.flush()
                raise
            out.write(json.dumps(row) + "\n")
            out.flush()
            rows.append(row)
    summarize(rows)


if __name__ == "__main__":
    main()
