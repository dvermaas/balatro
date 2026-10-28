"""beyond.py ANTE: what predicts winning at ANTE's first shop once the build's power is known (= what the forecast misses)."""
import json, sys, numpy as np, collections
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import roc_auc_score
A = int(sys.argv[1]); L = '/home/user/balatro/runs/_policy_lab/'
JV = json.load(open('/home/user/balatro/native/bot/data/joker_values.json'))
HANDS = ['High Card','Pair','Two Pair','Three of a Kind','Straight','Flush','Full House','Four of a Kind']
def feats(s):
    p = np.array(s['power']); lv = s['levels']
    lvl = {h: lv[h][0] for h in lv}; played = {h: lv[h][1] for h in lv}
    top = max(HANDS, key=lambda h: (played.get(h, 0), lvl.get(h, 1)))
    f = dict(power_p50=np.median(p), power_p10=np.percentile(p, 10), power_p90=np.percentile(p, 90),
             money=min(s['money'], 100), jokers=len(s['jokers']), jvalue=sum(JV.get(j[0], 0) for j in s['jokers']),
             editions=sum(bool(j[1]) for j in s['jokers']), deck=s['deck'], enh=s['enh'], hand_size=s['hand_size'],
             vouchers=len(s['vouchers']), levels_total=sum(v - 1 for v in lvl.values()), top_level=lvl[top] - 1,
             hands_played_types=sum(1 for h in HANDS if played.get(h, 0) > 0))
    for h in ['Pair', 'Two Pair', 'Three of a Kind', 'Straight', 'Flush', 'Full House']:
        f['main_' + h.replace(' ', '_')] = int(top == h)
    return f
def load(runs):
    X, y = [], []
    for run in runs:
        for l in open(L + run + '/results.jsonl'):
            g = json.loads(l); first = {}
            for s in g.get('snaps', []): first.setdefault(s['ante'], s)
            s = first.get(A)
            if s and s.get('power'): X.append(feats(s)); y.append(int(g['won']))
    return X, np.array(y)
tr, ytr = load(['e16k-v61', 'c2k-v61', 'c2k-pp10']); te, yte = load(['b2k-jp1', 'b2k-pp10'])
names = list(tr[0]); M = lambda D: np.array([[d[k] for k in names] for d in D], float)
Xtr, Xte = M(tr), M(te); mu, sd = Xtr.mean(0), Xtr.std(0) + 1e-9; Ztr, Zte = (Xtr - mu) / sd, (Xte - mu) / sd
print(f"ante {A}: train {len(ytr)} (win {ytr.mean():.3f}), test {len(yte)}")
pw = [names.index(k) for k in ['power_p10', 'power_p50', 'power_p90']]
m0 = LogisticRegression(C=1e6, max_iter=5000).fit(Ztr[:, pw], ytr)
m1 = LogisticRegression(C=1e6, max_iter=5000).fit(Ztr, ytr)
print(f"  held-out AUC: power only {roc_auc_score(yte, m0.predict_proba(Zte[:, pw])[:, 1]):.3f}, everything {roc_auc_score(yte, m1.predict_proba(Zte)[:, 1]):.3f}")
# standard errors from the Hessian
P = m1.predict_proba(Ztr)[:, 1]; Xa = np.hstack([np.ones((len(Ztr), 1)), Ztr])
H = Xa.T @ (Xa * (P * (1 - P))[:, None]); se = np.sqrt(np.diag(np.linalg.pinv(H)))[1:]
print("  per 1 SD, controlling for everything else (logit, z):")
for i in sorted(range(len(names)), key=lambda i: -abs(m1.coef_[0][i] / se[i])):
    print(f"    {names[i]:22s} {m1.coef_[0][i]:+.3f}  z {m1.coef_[0][i]/se[i]:+6.1f}   (1 SD = {sd[i]:.2f})")
