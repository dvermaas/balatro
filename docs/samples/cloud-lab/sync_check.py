"""sync_check.py REAL_RUN_DIR SIM_RUN_DIR: does a seed played on the real game go as on rumbot?

Play the same seed twice with `jevbot play --lab --seed S`: once with `--backend balatrobot` (the real game; run dir
under runs/) and once with `--backend rumbot` (under runs/sim/). Same seed + same decisions should give the same run.
Prints:
  1. the first game state that differs (hand, shop, jokers, money, ...), with the decision just before it;
  2. the first action the bot chose differently;
  3. per run: blind skips (with their tag) and every free ($0) shop item, so tag payouts can be checked.
A state difference before any action difference is a simulator mismatch: that is the thing to report.
"""
import gzip
import json
import sys
from pathlib import Path


def load(p):
    return [json.loads(l) for l in open(p, encoding="utf-8")]


def card(c):
    v, m = c.get("value") or {}, c.get("modifier") or {}
    return (c.get("label"), v.get("rank"), v.get("suit"), m.get("enhancement"), m.get("edition"), m.get("seal"),
            (c.get("cost") or {}).get("buy"))


def area(g, k):
    a = g.get(k) or {}
    return [card(c) for c in (a.get("cards") or [])] if isinstance(a, dict) else []


def norm(g):
    """The parts of a gamestate both engines must agree on (no ids, no timings)."""
    rnd = g.get("round") or {}
    return {"state": g.get("state"), "ante": g.get("ante_num"), "round": g.get("round_num"), "money": g.get("money"),
            "hand": area(g, "hand"), "jokers": area(g, "jokers"), "consumables": area(g, "consumables"),
            "shop": area(g, "shop"), "vouchers": area(g, "vouchers"), "packs": area(g, "packs"),
            "chips": rnd.get("chips"), "hands_left": rnd.get("hands_left"), "discards_left": rnd.get("discards_left")}


def states(d):
    p = Path(d) / "states.jsonl.gz"
    return [json.loads(l).get("gs", {}) for l in gzip.open(p, "rt", encoding="utf-8")] if p.exists() else []


def decisions(d):
    out, ante = [], None
    for e in load(Path(d) / "decisions.jsonl"):
        ante = e.get("ante") or ante
        if e.get("kind") in ("decision", "auto", "forced", "action"):
            out.append({**e, "ante": e.get("ante") or ante})
    return out


def first_state_diff(a, b):
    for i, (x, y) in enumerate(zip(a, b)):
        nx, ny = norm(x), norm(y)
        if nx != ny:
            return i, {k: (nx[k], ny[k]) for k in nx if nx[k] != ny[k]}
    return None, None


def first_action_diff(a, b):
    acts = lambda ev: [(e.get("method"), str(e.get("params"))) for e in ev if e.get("kind") == "action"]
    for i, (x, y) in enumerate(zip(acts(a), acts(b))):
        if x != y:
            return i, x, y
    return None, None, None


def tag_report(sts, dec):
    out = [f"  skip at ante {e.get('ante') or '?'}: {e.get('desc')}" for e in dec
           if e.get("question") == "blind" and e.get("choice") == "skip"]
    seen = set()
    for g in sts:
        if g.get("state") != "SHOP":
            continue
        key = (g.get("ante_num"), g.get("round_num"))
        free = [(c[0], c[4]) for c in area(g, "shop") + area(g, "vouchers") + area(g, "packs") if c[6] == 0]
        if free and key not in seen:
            seen.add(key)
            out.append(f"  ante {key[0]} round {key[1]} shop, free: {free}")
    return out


def main():
    real, sim = sys.argv[1], sys.argv[2]
    sa, sb, da, db = states(real), states(sim), decisions(real), decisions(sim)
    i, diff = first_state_diff(sa, sb)
    if i is None:
        print(f"states: identical for {min(len(sa), len(sb))} states (real {len(sa)}, sim {len(sb)})")
    else:
        g = sa[i]
        print(f"states: first difference at state {i} (ante {g.get('ante_num')}, round {g.get('round_num')}, {g.get('state')}):")
        for k, (x, y) in diff.items():
            print(f"  {k}:\n    real {x}\n    sim  {y}")
    j, x, y = first_action_diff(da, db)
    print("actions: identical" if j is None else f"actions: first difference at action {j}: real {x}  sim {y}")
    for name, d, sts, dec in (("real", real, sa, da), ("sim", sim, sb, db)):
        end = next((e for e in reversed(load(Path(d) / "decisions.jsonl")) if e.get("kind") == "run_end"), {})
        print(f"\n{name} ({d}): ante {end.get('ante')} round {end.get('round')} won {end.get('won')}")
        print("\n".join(tag_report(sts, dec)) or "  no skips")


if __name__ == "__main__":
    main()
