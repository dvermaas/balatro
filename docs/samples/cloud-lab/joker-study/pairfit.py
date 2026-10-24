"""pairfit.py ANTE TEST_RUN TRAIN_RUN... [--out OUT.json] [--s S]: win model at the first shop of ANTE with per-joker
values and regularized pair terms (pair columns scaled by S, so they shrink harder); held-out log loss / AUC on TEST
with and without the pairs. --out writes {"values": {key: v}, "pairs": {"a|b": v}} (pairs fitted on all runs)."""
import json, sys, collections, itertools, numpy as np
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import log_loss, roc_auc_score
args=sys.argv[1:]; out=None; S=0.5
if '--out' in args: i=args.index('--out'); out=args[i+1]; del args[i:i+2]
if '--s' in args: i=args.index('--s'); S=float(args[i+1]); del args[i:i+2]
A=int(args[0]); test=args[1]; train=args[2:]; L='/home/user/balatro/runs/_policy_lab/'
def load(runs):
    rows=[]
    for run in runs:
        for l in open(L+run+'/results.jsonl'):
            g=json.loads(l); first={}
            for s in g.get('snaps',[]): first.setdefault(s['ante'],s)
            s=first.get(A)
            if not s: continue
            lv=[v[0] for v in s['levels'].values()]
            rows.append(dict(won=int(g['won']), keys=set(j[0] for j in s['jokers']), money=min(s['money'],200)/50, n_j=len(s['jokers']), lv=sum(x-1 for x in lv)/5, ed=sum(bool(j[1]) for j in s['jokers'])))
    return rows
tr, te = load(train), load([test])
cnt=collections.Counter(k for r in tr for k in r['keys']); K=sorted(k for k,c in cnt.items() if c>=20)
pc=collections.Counter(p for r in tr for p in itertools.combinations(sorted(r['keys']),2)); P=sorted(p for p,c in pc.items() if c>=30 and p[0] in K and p[1] in K)
def X(rows, pairs):
    base=[[r['money'],r['n_j'],r['lv'],r['ed']]+[k in r['keys'] for k in K] for r in rows]
    if not pairs: return np.array(base,float)
    pr=[[S*(p[0] in r['keys'] and p[1] in r['keys']) for p in P] for r in rows]
    return np.hstack([np.array(base,float),np.array(pr,float)])
y=np.array([r['won'] for r in tr]); yt=np.array([r['won'] for r in te])
print(f"ante {A}: train {len(tr)} test {len(te)}; {len(K)} jokers, {len(P)} pairs")
for pairs in (False, True):
    m=LogisticRegression(C=1.0,max_iter=8000).fit(X(tr,pairs),y); p=m.predict_proba(X(te,pairs))[:,1]
    print(f"  {'with pairs' if pairs else 'jokers only':12s} held-out log loss {log_loss(yt,p):.4f} AUC {roc_auc_score(yt,p):.4f}")
if out:
    al=tr+te; ya=np.array([r['won'] for r in al])
    m=LogisticRegression(C=1.0,max_iter=8000).fit(X(al,True),ya); c=m.coef_[0]
    vals={k: round(float(c[4+i]),3) for i,k in enumerate(K)}
    pairs={f"{a}|{b}": round(float(c[4+len(K)+j]*S),3) for j,(a,b) in enumerate(P)}
    json.dump({"values":vals,"pairs":pairs},open(out,'w'),indent=0)
    top=sorted(pairs.items(), key=lambda kv: kv[1])
    print('most negative pairs', top[:8]); print('most positive pairs', top[-12:][::-1])
