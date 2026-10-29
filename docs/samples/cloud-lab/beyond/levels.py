"""levels.py ANTE: per-hand-type levels, deck size and enhancements vs winning, with power, money, jokers and joker/edition values held fixed."""
import json, numpy as np, sys
from sklearn.linear_model import LogisticRegression
A=int(sys.argv[1]); L='/home/user/balatro/runs/_policy_lab/'
JV=json.load(open('/home/user/balatro/native/bot/data/joker_values.json'))
ED={'NEGATIVE':0.62,'POLYCHROME':0.41,'FOIL':0.21,'HOLO':0.08}
H=['High Card','Pair','Two Pair','Three of a Kind','Straight','Flush','Full House','Four of a Kind']
rows=[]
for run in ['e16k-v61','c2k-v61','c2k-pp10','b2k-jp1','b2k-pp10']:
    for l in open(L+run+'/results.jsonl'):
        g=json.loads(l); first={}
        for s in g.get('snaps',[]): first.setdefault(s['ante'],s)
        s=first.get(A)
        if s and s.get('power'): rows.append((s,int(g['won'])))
def f(s):
    p=np.array(s['power']); lv=s['levels']
    base=[np.median(p),np.percentile(p,90),min(s['money'],100)/16,len(s['jokers']),sum(JV.get(j[0],0)+ED.get(j[1] or '',0) for j in s['jokers']),
          s['deck']-52,s['enh'],s['hand_size']-8,len(s['vouchers'])]
    main=max(H,key=lambda h:lv[h][1])
    return base+[lv[h][0]-1 for h in H]+[lv[main][0]-1]
names=['power_p50','power_p90','money/$16','jokers','joker+edition value','deck size-52','enhanced cards','hand size-8','vouchers']+['level '+h for h in H]+['level of most-played hand']
X=np.array([f(s) for s,_ in rows],float); y=np.array([w for _,w in rows])
m=LogisticRegression(C=1e6,max_iter=8000).fit(X,y); P=m.predict_proba(X)[:,1]
Xa=np.hstack([np.ones((len(X),1)),X]); se=np.sqrt(np.diag(np.linalg.pinv(Xa.T@(Xa*(P*(1-P))[:,None]))))[1:]
print(f"ante {A}: {len(y)} runs; logit per unit")
for i in range(len(names)): print(f"  {names[i]:28s} mean {X[:,i].mean():6.2f} {m.coef_[0][i]:+.3f} z {m.coef_[0][i]/se[i]:+5.1f}")
