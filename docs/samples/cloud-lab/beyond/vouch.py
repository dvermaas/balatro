import json, numpy as np, collections, sys
from sklearn.linear_model import LogisticRegression
A=int(sys.argv[1]); L='/home/user/balatro/runs/_policy_lab/'
JV=json.load(open('/home/user/balatro/native/bot/data/joker_values.json'))
rows=[]
for run in ['e16k-v61','c2k-v61','c2k-pp10','b2k-jp1','b2k-pp10']:
    for l in open(L+run+'/results.jsonl'):
        g=json.loads(l); first={}
        for s in g.get('snaps',[]): first.setdefault(s['ante'],s)
        s=first.get(A)
        if s and s.get('power'): rows.append((s,int(g['won'])))
vc=collections.Counter(v for s,_ in rows for v in s['vouchers']); ec=collections.Counter(j[1] for s,_ in rows for j in s['jokers'] if j[1])
V=[v for v,c in vc.most_common() if c>=100]; E=list(ec)
def f(s):
    p=np.array(s['power'])
    base=[np.median(p),np.percentile(p,90),min(s['money'],100)/16,len(s['jokers']),sum(JV.get(j[0],0) for j in s['jokers'])]
    return base+[v in s['vouchers'] for v in V]+[sum(j[1]==e for j in s['jokers']) for e in E]
X=np.array([f(s) for s,_ in rows],float); y=np.array([w for _,w in rows])
m=LogisticRegression(C=1e6,max_iter=5000).fit(X,y); P=m.predict_proba(X)[:,1]
Xa=np.hstack([np.ones((len(X),1)),X]); se=np.sqrt(np.diag(np.linalg.pinv(Xa.T@(Xa*(P*(1-P))[:,None]))))[1:]
names=['power_p50','power_p90','money/$16','jokers','jvalue']+['V '+v for v in V]+['E '+e for e in E]
cnt=[None]*5+[vc[v] for v in V]+[ec[e] for e in E]
print(f"ante {A}: {len(y)} runs; logit per unit, controlling for power, money, jokers, joker values")
for i in range(len(names)): print(f"  {names[i]:26s} n {str(cnt[i] or ''):>6s} {m.coef_[0][i]:+.3f} z {m.coef_[0][i]/se[i]:+5.1f}")
