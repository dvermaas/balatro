"""Compare recorded game scores, current estimates and a strict Jackdaw reference.

Also isolate candidate pruning from scoring on the fixed public-position bank.
Alternative scores are simulator predictions, not observed continuation results.
No game calls, model calls, saved games, or hidden draws are used.
"""
import hashlib
import json
import sys
import time
from collections import Counter
from itertools import combinations, permutations
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT / "src"), str(ROOT / "scripts")]
import hands
import jokers
from diagnostics.jackdaw_reference import Unsupported, evaluate, verify_checkout
from diagnostics.mechanics_candidate import candidate_module

BANK = ROOT / "docs/samples/diagnostic-positions-v1.json"
OUT = ROOT / "docs/samples/diagnostic-mechanics-v1.json"
FILES = ("prompts.py", "hands.py", "strategy.py", "decisions.py", "jokers.py", "odds.py", "build.py", "tarots.py", "readiness.py")


def current_candidates(obs, scoring_module=jokers):
    # Same immediate scoring and boss filter as production; no draw forecasting.
    import decisions
    boss = next(b["name"] for b in obs["blinds"].values() if b["status"] == "CURRENT")
    levels = decisions._boss_levels(obs["hands"], boss)
    js = scoring_module.build(obs)
    scorer = lambda infos, combo, name, sc: scoring_module.score(infos, combo, name, sc, levels, js, obs["round"])
    allowed = lambda combo, name: decisions._boss_allows(combo, name, levels, boss)
    return hands.play_candidates(obs["hand"]["cards"], levels, scorer=scorer, allowed=allowed), scorer


def compare(position, candidate):
    obs = position["observation"]
    chosen = position["recorded"]["action"][1]["cards"]
    row = {"case_id": position["case_id"], "stratum": position["stratum"], "source": position["source"],
           "chosen": chosen, "actual": position["actual"]["chips_delta"]}
    cache, skipped = {}, Counter()

    def reference(indices):
        key = tuple(indices)
        if key not in cache:
            try:
                cache[key] = evaluate(obs, indices)
            except Unsupported as error:
                cache[key] = {"unsupported": str(error)}
        return cache[key]

    plays, scorer = current_candidates(obs)
    infos = [hands.card_info(c) for c in obs["hand"]["cards"]]
    name, sc = hands.classify([infos[i] for i in chosen])
    row["current_chosen_score"] = scorer(infos, chosen, name, sc)
    candidate_plays, candidate_scorer = current_candidates(obs, candidate)
    row["candidate_chosen_score"] = candidate_scorer(infos, chosen, name, sc)
    row["reference_chosen"] = reference(chosen)
    if "unsupported" in row["reference_chosen"]:
        return row
    scored = []
    for n in range(1, min(5, len(infos)) + 1):
        for combo in combinations(range(len(infos)), n):
            ref = reference(combo)
            if "unsupported" in ref:
                skipped[ref["unsupported"]] += 1
            else:
                scored.append({"cards": list(combo), **ref})
    # Compare all score ties when attributing a gain to pruning, not arbitrary tie order.
    def best(rows):
        return max(rows, key=lambda r: (r["total"], len(r["cards"]), [-i for i in r["cards"]]))
    offered = [{"cards": p["cards"], **reference(p["cards"])} for p in plays]
    supported_offered = [r for r in offered if "unsupported" not in r]
    row["current_top_candidate"] = {**plays[0], "reference": reference(plays[0]["cards"])}
    row["reference_best_existing_candidates"] = best(supported_offered) if supported_offered else None
    row["reference_best_all_subsets"] = best(scored)
    row["candidate_top"] = {**candidate_plays[0], "reference": reference(candidate_plays[0]["cards"])}
    differences = []
    for alt in scored:
        name, sc = hands.classify([infos[i] for i in alt["cards"]])
        estimate = candidate_scorer(infos, alt["cards"], name, sc)
        if estimate != alt["total"]:
            differences.append({"cards": alt["cards"], "candidate": estimate, "reference": alt["total"]})
    row["candidate_reference_disagreements"] = differences
    ordered = [{"cards": list(p), **reference(p)} for p in permutations(row["reference_best_all_subsets"]["cards"])]
    row["reference_best_order_of_best_subset"] = best([r for r in ordered if "unsupported" not in r])
    row["evaluated_subsets"] = len(scored)
    row["excluded_subsets"] = dict(skipped)
    row["complete_deterministic_subset_comparison"] = not skipped
    return row


def main():
    revision = verify_checkout()
    bank = json.loads(BANK.read_text(encoding="utf-8"))
    started = time.perf_counter()
    candidate = candidate_module()
    rows = [compare(p, candidate) for p in bank["positions"] if p["recorded"]["question"] == "move"]
    supported = [r for r in rows if "total" in r["reference_chosen"]]
    complete = [r for r in supported if r["complete_deterministic_subset_comparison"]]
    summary = {
        "recorded_plays": len(rows), "supported_chosen_plays": len(supported),
        "reference_exact_matches": sum(r["reference_chosen"]["total"] == r["actual"] for r in supported),
        "current_exact_matches_same_subset": sum(r["current_chosen_score"] == r["actual"] for r in supported),
        "candidate_exact_matches_same_subset": sum(r["candidate_chosen_score"] == r["actual"] for r in supported),
        "candidate_reference_comparisons": sum(r["evaluated_subsets"] for r in supported),
        "candidate_reference_disagreements": sum(len(r["candidate_reference_disagreements"]) for r in supported),
        "complete_subset_comparisons": len(complete),
        "scoring_rerank_gains_over_10_percent": [r["case_id"] for r in complete if r["reference_best_existing_candidates"]
            and r["reference_best_existing_candidates"]["total"] > 1.1 * r["current_top_candidate"]["reference"]["total"]],
        "candidate_expansion_gains_over_10_percent": [r["case_id"] for r in complete if r["reference_best_existing_candidates"]
            and r["reference_best_all_subsets"]["total"] > 1.1 * r["reference_best_existing_candidates"]["total"]],
        "ordering_gains_over_10_percent": [r["case_id"] for r in complete if
            r["reference_best_order_of_best_subset"]["total"] > 1.1 * r["reference_best_all_subsets"]["total"]],
        "seconds": round(time.perf_counter() - started, 3),
    }
    result = {"identity": {"bank_sha256": hashlib.sha256(BANK.read_bytes()).hexdigest(),
              "current_version": hashlib.sha1(b"".join((ROOT / "src" / f).read_bytes() for f in FILES)).hexdigest()[:8],
              "reference_commit": revision, "python": sys.version}, "summary": summary,
              "limitations": ["Fixed development cases, including two selected failures; no population accuracy or win-rate claim",
                  "Recorded plays have actual outcomes; unplayed alternatives have reference predictions only",
                  "Immediate score ignores future draws, resource value and scaling; no remaining-round search",
                  "Card-order search only permutes the best fixed-order subset, not every possible ordered subset",
                  "Adapter coverage is deliberately narrower than Jackdaw's full engine; randomized and unknown effects are excluded"],
              "positions": rows}
    OUT.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))
    for r in supported:
        if r["current_chosen_score"] != r["actual"] or r["case_id"] in sum((v for k, v in summary.items() if k.endswith("percent")), []):
            print(r["case_id"], "actual/current", r["actual"], r["current_chosen_score"], "reference best/existing/ordered",
                  r["reference_best_all_subsets"]["total"], r["reference_best_existing_candidates"]["total"],
                  r["reference_best_order_of_best_subset"]["total"])


if __name__ == "__main__":
    main()
