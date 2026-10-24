"""pairs.py ANTE RUN...: joker pairs held together at the first shop of ANTE: wins vs the main-effects model's prediction."""
import json, sys, collections, itertools, numpy as np
from sklearn.linear_model import LogisticRegression
A=int(sys.argv[1]); runs=sys.argv[2:]; L='/home/user/balatro/runs/_policy_lab/'
rows=[]
for run in runs:
    for l in open(L+run+'/results.jsonl'):
        g=json.loads(l); first={}
        for s in g.get('snaps',[]): first.setdefault(s['ante'],s)
        s=first.get(A)
        if not s: continue
        lv=[v[0] for v in s['levels'].values()]
        rows.append(dict(won=int(g['won']), keys=set(j[0][2:] for j in s['jokers']), money=min(s['money'],200), n_j=len(s['jokers']), lv=sum(x-1 for x in lv), ed=sum(bool(j[1]) for j in s['jokers'])))
y=np.array([r['won'] for r in rows])
cnt=collections.Counter(k for r in rows for k in r['keys'])
K=sorted(k for k,c in cnt.items() if c>=25)
X=np.array([[r['money']/50,r['n_j'],r['lv']/5,r['ed']]+[k in r['keys'] for k in K] for r in rows],float)
p=LogisticRegression(C=1.0,max_iter=5000).fit(X,y).predict_proba(X)[:,1]
pc=collections.Counter(pr for r in rows for pr in itertools.combinations(sorted(r['keys']),2))
res=[]
for pr,c in pc.items():
    if c<30: continue
    m=np.array([pr[0] in r['keys'] and pr[1] in r['keys'] for r in rows])
    obs=y[m].sum(); exp=p[m].sum(); var=(p[m]*(1-p[m])).sum()
    res.append(((obs-exp)/np.sqrt(var), pr, c, obs/c, exp/c))
res.sort()
import math
out={}
for z,pr,c,w,e in res:
    if abs(z)<3: continue
    m=np.array([pr[0] in r['keys'] and pr[1] in r['keys'] for r in rows])
    d=0.0
    for _ in range(20):
        q=1/(1+np.exp(-(np.log(p[m]/(1-p[m]))+d))); d+=(y[m]-q).sum()/(q*(1-q)).sum()
    d*=c/(c+100)
    out['j_'+pr[0]+'|j_'+pr[1]]=round(max(-1,min(1,d)),3)
json.dump(out,open('strong_pairs.json','w'),indent=0)
print('strong pairs',out)
print(f"ante {A}: {len(rows)} runs, {len(res)} pairs held together >= 30 times")
print("pairs that win LESS than their jokers predict:"); [print(f"  {r[1][0]:>18s} + {r[1][1]:<18s} n {r[2]:3d} win {r[3]:.2f} vs predicted {r[4]:.2f} z {r[0]:+.1f}") for r in res[:12]]
print("pairs that win MORE:"); [print(f"  {r[1][0]:>18s} + {r[1][1]:<18s} n {r[2]:3d} win {r[3]:.2f} vs predicted {r[4]:.2f} z {r[0]:+.1f}") for r in res[-15:][::-1]]
