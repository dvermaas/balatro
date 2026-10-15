"""Read-only paired report for a completed or early-stopped fixed benchmark."""
import argparse
import json
import statistics
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
import analytics
from audit_wins import audit_run


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--version", required=True)
    ap.add_argument("--set", default="bench30-a")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    matrix = analytics.benchmark_matrix(args.set)
    candidate = next(v for v in matrix["versions"] if v["version"] == args.version)
    comparisons = {}
    for baseline in matrix["versions"]:
        if baseline["version"] == args.version or baseline["done"] != baseline["of"]:
            continue
        rows = [{"seed": seed, "candidate": candidate["cells"][seed]["progress"],
                 "baseline": baseline["cells"][seed]["progress"]}
                for seed in matrix["seeds"] if seed in candidate["cells"] and seed in baseline["cells"]]
        differences = [r["candidate"] - r["baseline"] for r in rows]
        if not differences:
            continue
        comparisons[baseline["label"]] = {
            "version": baseline["version"], "n": len(rows),
            "candidate_mean": statistics.mean(r["candidate"] for r in rows),
            "baseline_paired_mean": statistics.mean(r["baseline"] for r in rows),
            "mean_difference": statistics.mean(differences),
            "paired_se": statistics.stdev(differences) / len(rows) ** .5 if len(rows) > 1 else None,
            "better": sum(d > 0 for d in differences), "tied": differences.count(0),
            "worse": sum(d < 0 for d in differences), "pairs": rows,
        }
    audits = [audit_run(ROOT / "runs" / cell["run_id"])
              for cell in candidate["cells"].values() if cell["result"] == "won"]
    pilot = next((v for v in analytics.benchmark_matrix("weak7-a")["versions"] if v["version"] == args.version), None)
    repeats = [{"seed": seed, "pilot": cell["progress"], "full_benchmark": candidate["cells"][seed]["progress"],
                "pilot_run": cell["run_id"], "full_run": candidate["cells"][seed]["run_id"]}
               for seed, cell in (pilot or {}).get("cells", {}).items() if seed in candidate["cells"]]
    sensitivity = None
    v15_pairs = comparisons.get("v15", {}).get("pairs", [])
    if any(r["seed"] == "X62IZYJ4" for r in v15_pairs):
        differences = [r["candidate"] - r["baseline"] for r in v15_pairs if r["seed"] != "X62IZYJ4"]
        sensitivity = {"excluded_seed": "X62IZYJ4", "n": len(differences),
                       "mean_difference_vs_v15": statistics.mean(differences),
                       "paired_se": statistics.stdev(differences) / len(differences) ** .5,
                       "reason": "V15 game 17 resumed after an interruption with incomplete recent-play reconstruction."}
    result = {"version": args.version, "set": args.set, "done": candidate["done"], "of": candidate["of"],
              "recorded_wins": candidate["wins"], "verified_wins": sum(a["verdict"] == "verified_win" for a in audits),
              "mean_progress": candidate["avg_progress"], "mean_ante": candidate["avg_ante"],
              "cells": candidate["cells"], "comparisons": comparisons,
              "win_audits": audits, "pilot_repeats": repeats, "v15_resume_sensitivity": sensitivity,
              "limitations": "Existing development benchmark, not held out; no fresh old-version baseline. If incomplete, comparisons use only paired completed seeds. Pilot repeats remain separate, never best-of-two."}
    Path(args.out).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in result.items() if k not in ("cells", "comparisons")}, indent=2))
    print(json.dumps({label: {k: v for k, v in c.items() if k != "pairs"}
                      for label, c in comparisons.items() if label in ("v9", "v11", "v12", "v13", "v15")}, indent=2))


if __name__ == "__main__":
    main()
