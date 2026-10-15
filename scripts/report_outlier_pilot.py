"""Compare the fixed weak7-a development pilot with recorded v13/v15/v16 runs."""
import argparse
import json
import statistics
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
import analytics
import benchmark
from audit_wins import audit_run


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--version", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    versions = {"v13": ("ac9101d7", "bench30-a"), "v15": ("6725b5bb", "bench30-a"),
                "v16": ("503b35ff", "weak7-a"), "candidate": (args.version, "weak7-a")}
    runs = analytics.all_runs()
    pairs, audits = [], []
    for index, seed in enumerate(benchmark.SETS["weak7-a"]):
        row = {"seed": seed, "group": "weak" if index < 5 else "control"}
        for label, (version, set_name) in versions.items():
            matches = [r for r in runs if r.get("prompt_version") == version and r.get("seed") == seed
                       and (r.get("benchmark") or {}).get("name") == set_name and r["result"] in ("won", "lost")]
            if not matches:
                raise SystemExit(f"Missing terminal {label} run for {seed}; pilot is not complete")
            run = max(matches, key=lambda r: r["started"])
            row[label] = run["metrics"]["progress"]
            row[label + "_run"] = run["run_id"]
            if label == "candidate":
                row.update(result=run["result"], errors=run["errors"], ante=run["ante"])
                events = analytics.load_decisions(ROOT / "runs" / run["run_id"])
                offered, chosen = [], []
                for event in events:
                    options = (event.get("questions") or {}).get("best_play", {}).get("criteria", {})
                    if any("next_hand_outlook" in p for p in options.values()):
                        offered.append(event["id"])
                    if options.get(event.get("choice"), {}).get("setup"):
                        chosen.append(event["id"])
                row.update(forecast_decision_events=offered, setup_chosen_events=chosen)
                if run["result"] == "won":
                    audits.append(audit_run(ROOT / "runs" / run["run_id"]))
        pairs.append(row)
    comparisons = {}
    for group in ("weak", "control", "all"):
        rows = [r for r in pairs if group == "all" or r["group"] == group]
        comparisons[group] = {}
        for base in ("v13", "v15", "v16"):
            differences = [r["candidate"] - r[base] for r in rows]
            comparisons[group][base] = {
                "n": len(rows), "candidate_mean": statistics.mean(r["candidate"] for r in rows),
                "baseline_mean": statistics.mean(r[base] for r in rows),
                "difference": statistics.mean(differences),
                "paired_se": statistics.stdev(differences) / len(rows) ** .5,
                "better": sum(d > 0 for d in differences), "tied": differences.count(0),
                "worse": sum(d < 0 for d in differences),
            }
    gates = {
        "bottom_pair_improves_without_v15_regression": sum(r["candidate"] for r in pairs[:2]) > 10
            and all(r["candidate"] >= 5 for r in pairs[:2]),
        "weak_mean_at_least_v16": comparisons["weak"]["v16"]["candidate_mean"] >= 9.2,
        "controls_within_three_blinds_of_v15": all(r["candidate"] >= r["v15"] - 3 for r in pairs[5:]),
        "no_execution_errors": not any(r["errors"] for r in pairs),
        "all_recorded_wins_verified": all(a["verdict"] == "verified_win" for a in audits),
    }
    result = {"version": args.version, "set": "weak7-a", "done": len(pairs),
              "wins": sum(r["result"] == "won" for r in pairs), "errors": sum(r["errors"] for r in pairs),
              "pairs": pairs, "comparisons": comparisons, "win_audits": audits,
              "advancement_checks": gates, "advance_to_bench30": all(gates.values()),
              "limitations": "Selected development subset; one stochastic run per seed; no fresh baseline replicate; bundled threshold rollback and planning change."}
    Path(args.out).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
