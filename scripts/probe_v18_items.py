"""Ablate v18 item questions on recorded public JEV requests; never call the game.

Uses exact recorded questions (including recent-hand measurements), so it does
not introduce replay.py's missing-history approximation. Output includes every
repeat, not only successful changes. These are decision checks, not run results.
"""
import argparse
import copy
import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
from jev_bot import load_env
from typesafe_sdk import Choice, Score, TypeSafeClient

CASH_QUESTION = (
    "How valuable is taking this cash tarot in this situation? Use `cash_outcome`, which already "
    "subtracts the price before calculating the payout. In a shop, buying and using it can fund "
    "another purchase in the SAME shop; compare net profit, not just the temporary purchase cost. "
    "A substantial immediate net cash profit is a strong economy improvement even though it adds "
    "no chips directly and fits any hand type. A loss or zero profit is not an immediate cash upgrade. "
    "In a paid pack, the money competes with the other free picks: prefer immediate scoring if "
    "needed to survive, otherwise consider how the cash funds future upgrades."
)
PACK_QUESTION = (
    "Which free joker should we take from this already-paid pack, or should we skip? Compare each "
    "option with leaving the slot empty. Temporary scoring, money and generated jokers can help "
    "before a final build exists; recent-hand score gain does not measure all those effects. "
    "A sellable joker can be replaced later. Account for rental costs, eternal slots, destructive "
    "effects and lost discards. Skip if the options' downsides outweigh their benefits."
)
CASH_STEP_QUESTION = (
    "Which action should we take before continuing this shop? Each cash-tarot option buys and "
    "immediately uses that card, then returns to this SAME shop with the stated net cash profit. "
    "Other items remain available; no blind is played in between. Prefer useful immediate profit "
    "that funds our next purchases. Choose continue_shopping if there is a concrete reason to "
    "pass these cash transactions. The numeric cash outcomes already include the purchase price."
)


def cash_facts(item, state):
    what = item.get("what", "")
    money, cost = state.get("money", 0) or 0, item.get("cost", 0)
    if what.startswith("The Hermit:"):
        payout = max(0, min(money - cost, 20))
    elif what.startswith("Temperance:"):
        m = re.search(r"gives \+\$(\d+)", item.get("use", ""))
        if not m:
            return None
        payout = int(m.group(1))
    else:
        return None
    return {"cash_before": money, "purchase_cost": cost, "cash_after_purchase": money - cost,
            "payout_if_used_now": payout, "net_cash_gain": payout - cost,
            "cash_after_use": money - cost + payout,
            "interest_after_use": max(0, min(5, (money - cost + payout) // 5))}


def questions_for(event, variant):
    qs = copy.deepcopy(event["questions"])
    state = copy.deepcopy(event["state_sent"])
    joker_pack = event["question"] == "pack" and all(
        q["instructions"]["item"].get("kind") == "joker" for q in qs.values())
    if variant != "baseline":
        for q in qs.values():
            item = q["instructions"]["item"]
            facts = cash_facts(item, state)
            if facts:
                item["cash_outcome"] = facts
                item["use"] = f"gives +${facts['payout_if_used_now']} if used after taking this card"
                if variant in ("focused", "cash_step"):
                    q["instructions"]["question"] = CASH_QUESTION
            if item.get("what", "").startswith("Riff-raff:"):
                count, limit = map(int, state["joker_slots"].split("/"))
                free = max(0, limit - count - 1)
                item["next_blind_generation"] = {
                    "free_slots_after_taking": free, "common_jokers_created": min(2, free),
                    "resulting_joker_count": count + 1 + min(2, free),
                    "scoring": "random Common jokers; exact effects unknown"}
    if variant == "focused" and joker_pack:
        opts = {k: q["instructions"]["item"] for k, q in qs.items()}
        opts["skip"] = {"action": "Skip the pack; keep the existing lineup and empty slots"}
        return state, {"pack_pick": Choice(instructions=PACK_QUESTION, criteria=opts)}
    questions = {k: Score.model_validate(q) for k, q in qs.items()}
    if variant == "cash_step" and event["question"] == "shop":
        cash = {k: q["instructions"]["item"] for k, q in qs.items()
                if q["instructions"]["item"].get("cash_outcome", {}).get("net_cash_gain", 0) > 0}
        if cash:
            cash["continue_shopping"] = {"action": "Pass these cash transactions and consider the other shop actions"}
            questions["cash_step"] = Choice(instructions=CASH_STEP_QUESTION, criteria=cash)
    return state, questions


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--manifest", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--variants", nargs="+", default=["baseline", "facts", "focused"])
    ap.add_argument("--repeats", type=int, default=3)
    args = ap.parse_args()
    load_env()
    client = TypeSafeClient(api_key=os.environ.get("JEV_API_KEY") or os.environ.get("TYPESAFE_API_KEY"), model="jev-latest")
    cases = json.loads(Path(args.manifest).read_text(encoding="utf-8"))
    output = Path(args.out)
    if output.exists():
        raise SystemExit(f"Refusing to overwrite {output}")
    with output.open("w", encoding="utf-8") as f:
        for case in cases:
            events = map(json.loads, (ROOT / "runs" / case["run_id"] / "decisions.jsonl").read_text(encoding="utf-8").splitlines())
            event = next(e for e in events if e["id"] == case["event_id"])
            for repeat in range(args.repeats):
                # Alternate order to reduce any systematic time/order bias.
                variants = args.variants if repeat % 2 == 0 else list(reversed(args.variants))
                for variant in variants:
                    state, qs = questions_for(event, variant)
                    resp = client.system_one(state=state, questions=qs)
                    if "pack_pick" in resp.answers:
                        values = dict(resp.answers["pack_pick"].probabilities)
                        choice = max(values, key=values.get)
                    elif "cash_step" in resp.answers:
                        values = dict(resp.answers["cash_step"].probabilities)
                        choice = max(values, key=values.get)
                    else:
                        values = {k: a.score for k, a in resp.answers.items()}
                        choice = max(values, key=values.get)
                        if event["question"] == "pack" and values[choice] < 1.5:
                            choice = "below_pack_threshold"
                    row = {**case, "repeat": repeat, "variant": variant, "model": resp.model,
                           "old_choice": event["choice"], "top_choice": choice, "values": values,
                           "input_tokens": resp.usage.input_tokens, "output_tokens": resp.usage.output_tokens}
                    f.write(json.dumps(row) + "\n")
                    f.flush()
            print(case["name"], "done", flush=True)


if __name__ == "__main__":
    main()
