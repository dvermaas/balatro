#!/usr/bin/env python3
"""agentq.py: the agent's side of jevlab's `agent-dir=D` hook (an LLM subagent takes the forecast-backed shop decisions).

  agentq.py next   D SEED                     wait for SEED's next decision (or its game's end) and print it
  agentq.py answer D SEED ID KEY "reason"     answer decision ID with option KEY, then wait for and print the next one
  agentq.py report D NAME BASE                agent vs code choices and the paired outcome (runs/_policy_lab/NAME vs BASE)

jevlab writes D/req/SEED.json and waits for D/ans/SEED-ID.txt; at the game's end it writes D/done/SEED.
Answers are logged to D/log.jsonl. A wait gives up after 100 s (run the same command again).
"""
import json, os, sys, time

R = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def fmt_req(d, r):
    g = r["state"]["game"]; x = r.get("extra", {})
    out = [f"DECISION {r['id']} | seed {r['seed']} | ante {r['ante']} round {r['round']} | money ${r['money']}"
           f" (interest +${g.get('interest_per_round', 0)}/round) | rerolls this shop {r['rerolls']}"]
    b = x.get("boss") or {}
    if b:
        out.append(f"Boss of this ante: {b.get('name')} ({b.get('effect', '')})")
    out.append(f"Jokers {g.get('joker_slots', '')} (left to right):")
    out += [f"  - {j}" for j in g.get("jokers", [])] or ["  (none)"]
    out.append(f"Consumables {g.get('consumable_slots', '')}: {', '.join(g.get('consumables', [])) or 'none'}")
    out.append(f"Vouchers: {', '.join(v.replace('v_', '') for v in x.get('vouchers', [])) or 'none'}")
    hs = x.get("hands", {})
    lv = {k: v for k, v in hs.items() if (v.get("played") or 0) > 0 or (v.get("level") or 1) > 1}
    lv = sorted(lv.items(), key=lambda kv: -(kv[1].get("played") or 0))
    out.append("Hands (level, times played): " + ", ".join(f"{k} L{v.get('level')} x{v.get('played') or 0}" for k, v in lv))
    s = x.get("suits", {}); rk = x.get("ranks", {}); en = x.get("enhancements", {})
    out.append(f"Deck: {x.get('deck_size')} cards, hand size {x.get('hand_size')}; suits "
               + " ".join(f"{k[:1]}{v}" for k, v in sorted(s.items()))
               + "; ranks " + " ".join(f"{k}:{v}" for k, v in sorted(rk.items(), key=lambda kv: -kv[1]))
               + ("; enhanced " + ", ".join(f"{k} {v}" for k, v in en.items()) if en else "; no enhanced cards"))
    eng = g.get("engine") or {}
    if eng:
        miss = eng.get("missing", [])
        miss = miss if isinstance(miss, str) else ", ".join(miss)
        out.append(f"Engine: has {eng.get('has')}; missing {miss or 'nothing'}")
    out.append("Options (forecast = extra blinds cleared over the next 3 bosses vs buying nothing, mean +- SE of 64 simulated futures):")
    for it in r["items"]:
        fc = f"forecast {it['fc_delta']:+.2f} +- {it['fc_se']:.2f}" if it.get("fc_delta") is not None else "no forecast"
        out.append(f"  {it['key']:<13} ${it['cost']:<3} [{it['kind']}] {it['desc']}  | {fc}")
    if r.get("can_reroll"):
        out.append("  reroll        reroll the shop")
    out.append("  leave         leave the shop (next blind)")
    path = r.get("code_path", "choice")
    note = {"forecast": " (confident: the forecast says it beats buying nothing by more than 1 SE)",
            "early slot fill": " (early-ante rule: fill joker slots)", "free joker": " (a free joker)",
            "planet rule": " (rule: level the top hand / open Celestial packs)"}.get(path, "")
    out.append(f"Code's choice: {r['code_choice']}{note}")
    out.append(f"Answer with: python3 scripts/lab/agentq.py answer {d} {r['seed']} {r['id']} <key> \"<one-line reason>\"")
    return "\n".join(out)


def options(r):
    k = [it["key"] for it in r["items"]] + ["leave"]
    return k + (["reroll"] if r.get("can_reroll") else [])


def nxt(d, seed, wait=100.0):
    t0 = time.time()
    while time.time() - t0 < wait:
        p = os.path.join(d, "req", f"{seed}.json")
        if os.path.exists(p):
            try:
                r = json.load(open(p))
            except (json.JSONDecodeError, OSError):
                time.sleep(0.2); continue
            if not os.path.exists(os.path.join(d, "ans", f"{seed}-{r['id']}.txt")):
                print(fmt_req(d, r)); return 0
        if os.path.exists(os.path.join(d, "done", seed)):
            print(f"GAME OVER for seed {seed}. Go on to your next seed; after your last seed, reply with one line per seed: the plan you followed.")
            return 0
        time.sleep(0.3)
    print(f"No decision yet (the bot is still playing). Run: python3 scripts/lab/agentq.py next {d} {seed}")
    return 0


def answer(d, seed, i, key, reason):
    p = os.path.join(d, "req", f"{seed}.json")
    r = json.load(open(p)) if os.path.exists(p) else None
    if r is None or str(r["id"]) != str(i):
        print(f"Decision {i} is not pending (already answered or timed out)."); return nxt(d, seed)
    if key not in options(r):
        print(f"'{key}' is not an option here; choose one of: {', '.join(options(r))}"); return 1
    a = os.path.join(d, "ans", f"{seed}-{i}.txt")
    open(a + ".tmp", "w").write(key + "\n" + reason + "\n"); os.replace(a + ".tmp", a)
    with open(os.path.join(d, "log.jsonl"), "a") as f:
        f.write(json.dumps({"seed": seed, "id": int(i), "ante": r["ante"], "money": r["money"], "key": key,
                            "code": r["code_choice"], "reason": reason, "t": time.time()}) + "\n")
    time.sleep(0.5)
    return nxt(d, seed)


def report(d, name, base):
    rows = lambda n: {j["seed"]: j for j in (json.loads(l) for l in open(f"{R}/runs/_policy_lab/{n}/results.jsonl") if l.strip()) if "won" in j}
    a, b = rows(name), rows(base)
    log = [json.loads(l) for l in open(os.path.join(d, "log.jsonl"))] if os.path.exists(os.path.join(d, "log.jsonl")) else []
    tot = dict(n=0, db=0, w=0, l=0)
    for s, r in sorted(a.items()):
        o = b.get(s)
        ag = r.get("agent", [])
        ov = [e for e in ag if e["agent"] != e["code"]]
        if o:
            db = r["blinds"] - o["blinds"]; tot["n"] += 1; tot["db"] += db
            tot["w"] += int(r["won"] and not o["won"]); tot["l"] += int(o["won"] and not r["won"])
        print(f"{s}: agent {'WON' if r['won'] else 'lost'} {r['blinds']} blinds | code {'WON' if o and o['won'] else 'lost'} "
              f"{o['blinds'] if o else '?'} | {len(ag)} decisions, {len(ov)} overrides, {sum(not e['answered'] for e in ag)} timeouts")
        for e in log:
            if e["seed"] == s and e["key"] != e["code"]:
                print(f"    a{e['ante']} #{e['id']} ${e['money']}: {e['code']} -> {e['key']}: {e['reason'][:150]}")
    if tot["n"]:
        print(f"paired {tot['n']}: blinds {tot['db'] / tot['n']:+.2f}/game, games won by only one side: agent {tot['w']} code {tot['l']}")


if __name__ == "__main__":
    c = sys.argv[1]
    if c == "next":
        sys.exit(nxt(sys.argv[2], sys.argv[3]))
    if c == "answer":
        sys.exit(answer(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], " ".join(sys.argv[6:])))
    if c == "report":
        sys.exit(report(sys.argv[2], sys.argv[3], sys.argv[4]))
    print(__doc__); sys.exit(2)
