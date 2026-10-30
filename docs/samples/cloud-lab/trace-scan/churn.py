"""churn.py: jokers sold soon after they arrived (rounds held), from outcome joker lists and sell actions."""
import json, glob, collections, sys
quick = collections.Counter(); sold = collections.Counter(); ex = []
for f in glob.glob(sys.argv[1] + '/*/decisions.jsonl'):
    L = [json.loads(l) for l in open(f)]
    since = {}; prev = []
    for r in L:
        if r.get('kind') != 'outcome':
            continue
        j = [x if isinstance(x, str) else str(x) for x in r.get('jokers', [])]
        for n in set(j) - set(prev):
            since[n] = r['round']
        if r['method'] == 'sell':
            for n in set(prev) - set(j):
                held = r['round'] - since.get(n, r['round'])
                sold[n] += 1
                if held <= 1:
                    quick[n] += 1
                    if len(ex) < 12: ex.append((f.split('/')[2], r['ante'], n, held, j))
        prev = j
print('sold', sum(sold.values()), 'within 1 round of arriving', sum(quick.values()))
for n, c in quick.most_common(15): print(c, sold[n], n)
for e in ex: print(e)
