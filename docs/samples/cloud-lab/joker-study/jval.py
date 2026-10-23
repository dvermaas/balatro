"""jval.py ANTE RUN...: per-joker win association at the first shop of ANTE (raw and adjusted)."""
import json, sys, collections, numpy as np
from sklearn.linear_model import LogisticRegression
A=int(sys.argv[1]); runs=sys.argv[2:]
L='/home/user/balatro/runs/_policy_lab/'
rows=[]
for run in runs:
    for l in open(L+run+'/results.jsonl'):
        g=json.loads(l); first={}
        for s in g.get('snaps',[]): first.setdefault(s['ante'],s)
        s=first.get(A)
        if not s: continue
        lv=[v[0] for v in s['levels'].values()]
        rows.append(dict(won=int(g['won']), keys=set(j[0] for j in s['jokers']), money=min(s['money'],200), n_j=len(s['jokers']), lv=sum(x-1 for x in lv), ed=sum(bool(j[1]) for j in s['jokers'])))
y=np.array([r['won'] for r in rows]); base=y.mean()
cnt=collections.Counter(k for r in rows for k in r['keys'])
K=sorted(k for k,c in cnt.items() if c>=int(__import__("os").environ.get("JV_MIN","40")))
X=np.array([[r['money']/50,r['n_j'],r['lv']/5,r['ed']]+[k in r['keys'] for k in K] for r in rows],float)
m=LogisticRegression(C=1.0,max_iter=5000).fit(X,y)
coef=dict(zip(K,m.coef_[0][4:]))
print(f"ante {A}: {len(rows)} runs reach it, win {base:.3f}; jokers held >=40 times: {len(K)}")
out=[]
for k in K:
    h=np.array([k in r['keys'] for r in rows]); wh=y[h].mean(); wn=y[~h].mean(); se=np.sqrt(wh*(1-wh)/h.sum())
    out.append((coef[k],k,h.sum(),wh,wn,se))
out.sort()
fmt=lambda o: f"  {o[1][2:]:22s} n {o[2]:4d}  win held {o[3]:.2f} (+-{o[4+1]:.2f}) vs not {o[4]:.2f}  adj logit {o[0]:+.2f}"
print("worst (adjusted):"); [print(fmt(o)) for o in out[:15]]
print("best (adjusted):"); [print(fmt(o)) for o in out[-15:][::-1]]
json.dump([dict(key=o[1],n=int(o[2]),win_held=o[3],win_not=o[4],adj=o[0]) for o in out], open(f'/tmp/claude-0/-home-user-balatro/67ba2d27-f901-5d1c-93fc-3995ad80f262/scratchpad/jv/jval_a{A}.json','w'))
