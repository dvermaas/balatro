"""cons.py: consumables held across many shop visits without being used."""
import json, glob, collections, sys
held = collections.Counter(); seen = collections.Counter()
for f in glob.glob(sys.argv[1] + '/*/decisions.jsonl'):
    L = [json.loads(l) for l in open(f)]
    shops = [r for r in L if r.get('kind') == 'decision' and r.get('question') == 'shop' and isinstance(r.get('state_sent'), dict)]
    run = collections.Counter(); best = {}
    prev_round = None
    for r in shops:
        if r['round'] == prev_round:
            continue
        prev_round = r['round']
        cs = [c.split(':')[0] for c in r['state_sent'].get('consumables', [])]
        for c in set(cs):
            run[c] += 1
            best[c] = max(best.get(c, 0), run[c])
        for c in list(run):
            if c not in cs:
                run[c] = 0
    for c, n in best.items():
        seen[c] += 1
        if n >= 3:
            held[c] += 1
print('held through 3+ shops (games) / games seen:')
for c, n in held.most_common(25):
    print(n, seen[c], c)
