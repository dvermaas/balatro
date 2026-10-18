"""Shops where a target joker was on offer: bought / forecast verdict / why not. python castle_why.py ctmp"""
import json, re, sys, glob, collections
TARGETS = ["Castle", "Mail-In Rebate", "Faceless Joker", "Hit the Road", "Burnt Joker", "Trading Card", "Yorick", "Burglar", "Green Joker"]
num = re.compile(r"\(skip ([\d.]+), ([+-][\d.]+) ± ([\d.]+)\)")
out = collections.defaultdict(list)
for f in glob.glob(sys.argv[1] + "/*/*/decisions.jsonl"):
    ev = [json.loads(l) for l in open(f)]
    seed = f.split("-")[-1].split("/")[0]
    for i, e in enumerate(ev):
        if e["kind"] != "info" or not e.get("msg", "").startswith("Shop ("):
            continue
        money = int(re.match(r"Shop \(\$(-?\d+)\)", e["msg"]).group(1))
        nxt = next((j for j in range(i + 1, len(ev)) if ev[j]["kind"] == "info" and ev[j].get("msg", "").startswith("Shop (")), len(ev))
        for t in TARGETS:
            m = re.search(r"\$(\d+) " + re.escape(t) + ":", e["msg"])
            if not m:
                continue
            cost = int(m.group(1))
            rec = {"seed": seed, "ante": e.get("ante"), "money": money, "cost": cost, "verdict": None, "outcome": "not offered as an option"}
            if cost > money:
                rec["outcome"] = "can't afford"
            for e2 in ev[i + 1:nxt]:
                if e2["kind"] == "info" and "hides" in e2.get("msg", "") and t in e2["msg"]:
                    rec["outcome"] = "hidden (forecast: clearly worse than skip)"
                if e2["kind"] == "auto" and e2.get("question") == "shop" and t in str(e2.get("desc")):
                    rec["outcome"] = "bought (" + e2.get("reason", "")[:40] + ")"
                if e2["kind"] == "decision" and e2.get("question") == "shop" and rec["verdict"] is None:
                    for q in e2.get("questions", {}).values():
                        ins = q.get("instructions") if isinstance(q, dict) else None; it = ins.get("item", {}) if isinstance(ins, dict) else {}
                        if t in str(it.get("what", "")):
                            mm = num.search(str(it.get("blinds_cleared_horizon", "")))
                            if mm:
                                rec["verdict"] = (float(mm.group(2)), float(mm.group(3)))
                            if rec["outcome"] == "not offered as an option":
                                rec["outcome"] = "offered, not bought (below the 1 SE bar)"
                            rec["swap"] = "sell" in str(it.get("what", "")) or it.get("kind") == "joker swap"
            out[t].append(rec)
for t in TARGETS:
    rs = out[t]
    if not rs:
        print(f"{t}: never in a shop"); continue
    c = collections.Counter(r["outcome"].split(" (")[0] for r in rs)
    v = [r["verdict"] for r in rs if r["verdict"]]
    mean = sum(d for d, s in v) / len(v) if v else float("nan")
    pos = sum(d > 0 for d, s in v)
    print(f"{t}: {len(rs)} shop offers | " + ", ".join(f"{k} {n}" for k, n in c.most_common()) +
          (f" | forecast vs skip: mean {mean:+.2f} blinds, positive in {pos}/{len(v)}" if v else ""))
json.dump(out, open(sys.argv[1] + "/../castle_why.json", "w"))
