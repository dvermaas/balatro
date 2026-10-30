"""vanish.py: jokers that leave without a sell, per traced game (decisions.jsonl outcomes)."""
import json, glob, collections, sys
ways = collections.Counter(); names = collections.Counter(); ex = collections.defaultdict(list)
for f in glob.glob(sys.argv[1] + '/*/decisions.jsonl'):
    prev = None
    for l in open(f):
        r = json.loads(l)
        if r.get('kind') != 'outcome':
            continue
        j = [x if isinstance(x, str) else (x.get('label') or x.get('name') or str(x)) for x in r.get('jokers', [])]
        if prev is not None and r['method'] != 'sell':
            lost = collections.Counter(prev) - collections.Counter(j)
            for n in lost.elements():
                ways[(n, r['method'])] += 1; names[n] += 1
                if len(ex[n]) < 3:
                    ex[n].append((f.split('/')[2], r['ante'], r['method'], prev))
        prev = j
for k, v in ways.most_common(40):
    print(v, k)
print()
for n in list(names)[:15]:
    print(n, ex[n][:2])
