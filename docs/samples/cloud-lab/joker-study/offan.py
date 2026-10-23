import json, collections, numpy as np
SP='/tmp/claude-0/-home-user-balatro/67ba2d27-f901-5d1c-93fc-3995ad80f262/scratchpad/jv/'
done={json.loads(l)['seed']:json.loads(l)['won'] for l in open('/home/user/balatro/runs/_policy_lab/jvoff/results.jsonl')}
by=collections.defaultdict(list)
for l in open(SP+'offers800.jsonl'):
    r=json.loads(l)
    if r['seed'] in done: by[r['seed']].append(r)
val={}
for a in (3,5):
    for o in json.load(open(SP+f'jval_a{a}.json')): val.setdefault(o['key'],{})[a]=o['adj']
stat=collections.defaultdict(lambda: dict(n=0,taken=0,z=[],ante=[]))
for seed,L in by.items():
    for i,r in enumerate(L):
        cards={}
        for it in r['items']:
            k=it.get('card')
            if not k or not str(k).startswith('j_') or it.get('delta') is None: continue
            z=it['delta']/max(it['se'] or 1e-9,1e-9)
            cards[k]=max(cards.get(k,-99),z)
        nxt=L[i+1]['held'] if i+1<len(L) else None
        for k,z in cards.items():
            s=stat[k]; s['n']+=1; s['z'].append(z); s['ante'].append(r['ante'])
            if nxt is not None and nxt.count(k)>r['held'].count(k): s['taken']+=1
rows=[]
for k,s in stat.items():
    if s['n']<25: continue
    v=val.get(k,{})
    rows.append((v.get(5,np.nan),k,s['n'],s['taken']/s['n'],np.median(s['z']),np.mean(np.array(s['z'])>1),v.get(3,np.nan),np.median(s['ante'])))
rows.sort(key=lambda r:-(r[0] if r[0]==r[0] else -9))
print(f"{'joker':20s} {'val@a5':>6s} {'val@a3':>6s} {'offers':>6s} {'taken':>6s} {'med z':>6s} {'z>1':>5s} {'ante':>4s}")
for r in rows: print(f"{r[1][2:]:20s} {r[0]:+6.2f} {r[6]:+6.2f} {r[2]:6d} {r[3]:6.2f} {r[4]:+6.2f} {r[5]:5.2f} {r[7]:4.0f}")
json.dump([dict(key=r[1],val5=r[0],val3=r[6],offers=r[2],taken=r[3],medz=r[4]) for r in rows],open(SP+'offan.json','w'))
v=np.array([r[0] for r in rows]); t=np.array([r[3] for r in rows]); z=np.array([r[4] for r in rows]); m=~np.isnan(v)
print('corr(value, take rate)', np.corrcoef(v[m],t[m])[0,1].round(2), ' corr(value, median forecast z)', np.corrcoef(v[m],z[m])[0,1].round(2))
