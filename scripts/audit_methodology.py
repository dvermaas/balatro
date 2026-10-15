"""Read-only method diagnostics from completed v15/v17 public decision logs.

No game calls, model calls, raw states or ordered draw-pile data are used.
Disagreement and score errors are diagnostics, not counterfactual win rates.
"""
import json
import re
import statistics
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def audit(version):
    runs = []
    for p in sorted((ROOT / "runs").glob("*/summary.json")):
        s = json.loads(p.read_text(encoding="utf-8"))
        if (s.get("prompt_version") == version and s.get("result") in ("won", "lost")
                and (s.get("benchmark") or {}).get("name") == "bench30-a"):
            runs.append((p, s))
    models, counts, unknown, by_boss = Counter(), Counter(), Counter(), {}
    plays, selected_lower, gates = [], [], []
    for path, summary in runs:
        events = [json.loads(s) for s in path.with_name("decisions.jsonl").read_text(encoding="utf-8").splitlines()]
        pending = None
        for e in events:
            if e["kind"] == "decision":
                models[e.get("model", "not_logged")] += 1
                counts["decisions"] += 1
                q = e.get("question")
                counts[q] += 1
                pending = None
                if q == "move":
                    state = e.get("state_sent", {})
                    rnd = state.get("round", {})
                    not_modelled = state.get("jokers_not_in_estimates") or []
                    if not_modelled:
                        counts["moves_with_unmodelled_scoring"] += 1
                        unknown.update(str(x) for x in not_modelled)
                    answers = e.get("answers", {})
                    now = answers.get("play_now", {}).get("noul")
                    action = (e.get("action") or [None])[0]
                    if now is not None:
                        counts["moves_with_play_now"] += 1
                        if now >= .5 and action == "discard":
                            counts["code_discards_despite_play_now_at_least_half"] += 1
                            gates.append({"run": summary["run_id"], "id": e["id"], "play_now": now})
                    criteria = e.get("questions", {}).get("best_play", {}).get("criteria", {})
                    chosen = criteria.get(e.get("choice"), {})
                    if action == "play":
                        estimate = chosen.get("estimated_score")
                        if estimate is None:
                            m = re.search(r" ~([0-9]+)", e.get("options", {}).get(e.get("choice"), ""))
                            estimate = int(m[1]) if m else None
                        top = max((v.get("estimated_score", 0) for v in criteria.values()), default=estimate or 0)
                        has_outlook = any(v.get("next_hand_outlook") for v in criteria.values())
                        if top and estimate is not None and estimate < .9 * top:
                            selected_lower.append({"run": summary["run_id"], "id": e["id"],
                                "chosen_estimate": estimate, "best_immediate": top,
                                "two_play_forecast_present": has_outlook,
                                "boss": rnd.get("blind"), "unmodelled": not_modelled})
                        pending = {"run": summary["run_id"], "id": e["id"], "source": "jev_choice",
                                   "estimate": estimate, "boss": rnd.get("blind"), "unmodelled": not_modelled}
            elif e["kind"] == "forced" and e.get("question") == "move":
                counts["forced_moves"] += 1
                m = re.search(r"\(([0-9]+)\) clears the blind", e.get("desc", ""))
                pending = {"run": summary["run_id"], "id": e["id"], "source": "forced_clear",
                           "estimate": int(m[1]) if m else None, "boss": None, "unmodelled": None}
                if m:
                    counts["forced_clear_claims"] += 1
            elif e["kind"] == "outcome" and e.get("method") == "play":
                if pending and pending["estimate"] is not None and e.get("chips_delta") is not None:
                    actual = e["chips_delta"]
                    row = {**pending, "actual": actual, "absolute_relative_error":
                           abs(pending["estimate"] - actual) / max(1, abs(actual)),
                           "jokers": e.get("jokers"), "after_state": e.get("state")}
                    plays.append(row)
                    if pending["source"] == "forced_clear" and e.get("state") == "SELECTING_HAND":
                        counts["forced_clear_followed_by_selecting_hand"] += 1
                pending = None
    errors = [p["absolute_relative_error"] for p in plays]
    severe = sorted(plays, key=lambda p: p["absolute_relative_error"], reverse=True)
    for p in plays:
        if p["boss"]:
            by_boss.setdefault(p["boss"], []).append(p["absolute_relative_error"])
    return {"version": version, "runs": len(runs), "counts": dict(counts), "response_models": dict(models),
            "scoring": {"matched_plays": len(plays), "exact": sum(e == 0 for e in errors),
                        "within_10_percent": sum(e <= .1 for e in errors),
                        "over_25_percent": sum(e > .25 for e in errors),
                        "median_absolute_relative_error": statistics.median(errors) if errors else None,
                        "large_error_examples": severe[:12]},
            "jev_selected_below_90_percent_of_best_immediate": len(selected_lower),
            "lower_choices_without_two_play_forecast": sum(not p["two_play_forecast_present"] for p in selected_lower),
            "lower_choice_examples": selected_lower[:12],
            "unmodelled_scoring_descriptions": dict(unknown),
            "boss_error_counts": {k: {"matched_plays": len(v), "over_25_percent": sum(x > .25 for x in v)} for k, v in by_boss.items()},
            "gate_examples": gates[:8],
            "limitations": "Observed chosen plays only; stochastic effects and unsupported mechanics can explain score differences. Lower immediate score is not necessarily a worse long-term choice. Forced clear after-state flags need individual verification. No alternative continuation was tested."}


if __name__ == "__main__":
    index = json.loads((ROOT / "runs/_prompts/index.json").read_text(encoding="utf-8"))["order"]
    report = {"scope": "Completed historical development runs only; running v18 untouched", "versions": {
        label: audit(index[int(label[1:]) - 1]) for label in ("v15", "v17")}}
    (ROOT / "docs/samples/methodology-audit.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    for label, result in report["versions"].items():
        print(label, json.dumps({k: v for k, v in result.items() if k in ("version", "runs", "counts", "response_models",
            "jev_selected_below_90_percent_of_best_immediate", "lower_choices_without_two_play_forecast")}))
        print('scoring', json.dumps({k: v for k, v in result["scoring"].items() if k != "large_error_examples"}))
