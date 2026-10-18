"""Discards used per round, and how rounds end (one-hand clear vs needing discards)."""
import json, glob, sys, collections, statistics as st
rounds = []
for f in glob.glob(sys.argv[1] + "/*/*/decisions.jsonl"):
    ev = [json.loads(l) for l in open(f)]
    cur = None
    for e in ev:
        if e["kind"] == "action" and e.get("method") in ("play", "discard"):
            if cur is None:
                cur = {"plays": 0, "discards": 0, "forced_first": None}
            cur["plays" if e["method"] == "play" else "discards"] += 1
        if e["kind"] == "forced" and e.get("question") == "move" and "clears the blind" in str(e.get("desc")) and cur is not None and cur["forced_first"] is None:
            cur["forced_first"] = cur["plays"] == 0 and cur["discards"] == 0
        if e["kind"] == "forced" and e.get("question") == "move" and "clears the blind" in str(e.get("desc")) and cur is None:
            cur = {"plays": 0, "discards": 0, "forced_first": True}
        if e["kind"] == "round_end":
            if cur:
                cur["ante"] = e.get("ante"); cur["won"] = e.get("scored", 0) >= e.get("target", 1)
                rounds.append(cur)
            cur = None
print(len(rounds), "rounds")
print("discards used per round: mean %.2f of 4 (Red Deck: 4 per round)" % st.mean(r["discards"] for r in rounds))
print("rounds with 0 discards used: %.0f%%" % (100 * sum(r["discards"] == 0 for r in rounds) / len(rounds)))
print("hands played per round: mean %.2f" % st.mean(r["plays"] for r in rounds))
print("rounds won by the very first play (no discard before it): %.0f%%" % (100 * sum(bool(r["forced_first"]) for r in rounds) / len(rounds)))
for a in range(1, 9):
    rs = [r for r in rounds if r.get("ante") == a]
    if rs:
        print(f"  ante {a}: {len(rs)} rounds, discards {st.mean(r['discards'] for r in rs):.2f}, plays {st.mean(r['plays'] for r in rs):.2f}, zero-discard {100*sum(r['discards']==0 for r in rs)/len(rs):.0f}%")
