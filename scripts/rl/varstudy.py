#!/usr/bin/env python3
"""varstudy.py DIR CKPT: where the variance of the return to go is (milestone 5, part 1).

DIR holds K sampled rollouts of the same seeds by one policy: DIR/s<k>/results.jsonl (jevlab rows, JEVLAB_SNAPS=1 so
each row carries the power snapshots) and DIR/s<k>-games/*.rlb.gz (JEVLAB_RLLOG, schema v2). CKPT is the PPO run's
ckpt.pt whose critic is "the present critic".

Out-of-sample explained variance (EV) of the Monte Carlo return to go (train_rl.rewards), seeds split 80/20 (every
rollout of a seed on the same side), overall and by ante, for:
  a  the run's critic as it is; the same architecture refit here
  b  per-seed baseline: the mean final outcome of the seed's other rollouts, minus progress so far (b2: only the
     rollouts that got at least this far)
  c  a critic on the full observation (sparse, the clone's shape), with a learning curve over training games
  d  the present critic's blocks plus scoring power (the shop's power snapshot: log10 best-play score over 32 dealt
     hands, its quantiles, and their margins over this ante's blind targets)
  e  combinations (d with b as an input, c with power)
and the correlation of the advantages (GAE 0.95) the run's critic gives with those of the best estimator.
"""
import argparse
import collections
import glob
import hashlib
import json
import os
import sys

import numpy as np
import torch
import torch.nn.functional as F

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import read_rlb as R  # noqa: E402
import train_bc as B  # noqa: E402
import train_rl as T  # noqa: E402

BASE = [300, 800, 2000, 5000, 11000, 20000, 35000, 50000]  # white stake ante base chips (small x1, big x1.5, boss x2)


def load(d):
    """All rollouts as one dataset, plus per-record power features and per-game (seed, rollout)."""
    dirs = sorted(glob.glob(os.path.join(d, "s*-games")))
    dss, meta = [], []
    for k, gd in enumerate(dirs):
        name = os.path.basename(gd)[:-6]
        rows = {json.loads(l)["seed"]: json.loads(l) for l in open(os.path.join(d, name, "results.jsonl")) if l.strip()}
        ds = R.load_dir(gd)
        dss.append(ds)
        meta.append((k, rows))
    return dss, meta


def power_feats(ds, rows):
    """Per record: [has_power, p25, p50, p75, mean, p50 - log10 boss target, p25 - log10 boss target, p75 - log10 big
    target] from the latest power snapshot at or before the record's round (0s before the first shop)."""
    n = len(ds["action"])
    out = np.zeros((n, 8), np.float32)
    snaps = {}
    for gi, s in enumerate(ds["game_seed"]):
        sn = sorted(((x["round"], x) for x in rows[s].get("snaps", []) if x.get("power")), key=lambda t: t[0])
        snaps[gi] = sn
    g = ds["game"]
    rnd = ds["round"]
    ante = np.clip(ds["ante"], 1, 8)
    for i in range(n):
        sn = snaps[g[i]]
        best = None
        for r, x in sn:
            if r <= rnd[i]:
                best = x
            else:
                break
        if best is None:
            continue
        p = np.sort(np.asarray(best["power"], dtype=np.float64))
        m = len(p)
        boss = np.log10(2 * BASE[ante[i] - 1])
        big = np.log10(1.5 * BASE[ante[i] - 1])
        out[i] = [1, p[m // 4], p[m // 2], p[3 * m // 4], p.mean(), p[m // 2] - boss, p[m // 4] - boss, p[3 * m // 4] - big]
    return out


def ev(pred, y):
    return T.explained_variance(pred, y)


def by_ante(pred, y, ante):
    return {int(a): round(ev(pred[ante == a], y[ante == a]), 3) for a in range(1, 9) if (ante == a).sum() > 50}


class Dense(torch.nn.Module):
    def __init__(self, d, h=64):
        super().__init__()
        self.net = torch.nn.Sequential(torch.nn.Linear(d, h), torch.nn.ReLU(), torch.nn.Linear(h, h), torch.nn.ReLU(), torch.nn.Linear(h, 1))

    def forward(self, x):
        return self.net(x)[:, 0]


def fit_dense(X, y, tr, va, epochs=30, lr=1e-3, wd=1e-4, seed=0):
    """Dense MLP with early stopping on va (a held-out part of the training seeds); returns the model."""
    torch.manual_seed(seed)
    X = X.astype(np.float32)
    y = y.astype(np.float32)
    m = Dense(X.shape[1])
    opt = torch.optim.AdamW(m.parameters(), lr=lr, weight_decay=wd)
    rng = np.random.default_rng(seed)
    Xt = torch.from_numpy(X)
    yt = torch.from_numpy(y)
    best, state, bad = -1e9, None, 0
    for _ in range(epochs):
        perm = rng.permutation(tr)
        for i in range(0, len(perm), 1024):
            k = perm[i:i + 1024]
            loss = F.mse_loss(m(Xt[k]), yt[k])
            opt.zero_grad()
            loss.backward()
            opt.step()
        with torch.no_grad():
            e = ev(m(Xt[va]).numpy(), y[va])
        if e > best + 1e-4:
            best, state, bad = e, {k: v.clone() for k, v in m.state_dict().items()}, 0
        else:
            bad += 1
            if bad >= 3:
                break
    m.load_state_dict(state)
    return m


def predict_dense(m, X):
    with torch.no_grad():
        return m(torch.from_numpy(X.astype(np.float32))).numpy()


def fit_sparse(batch, rows_all, y, tr, va, extra=None, epochs=20, lr=1e-3, seed=0, hidden=(256, 256)):
    """The clone's network shape with one output on the full observation (+ optional dense extras added to the first
    hidden layer); early stopping on va."""
    ds = batch.ds
    torch.manual_seed(seed)
    net = B.Net(ds["obs_dim"], ds["n_actions"], list(hidden), n_value=1)
    ex = None
    if extra is not None:
        ex = torch.nn.Linear(extra.shape[1], hidden[0])
        Xe = torch.from_numpy(extra.astype(np.float32))
    params = list(net.parameters()) + (list(ex.parameters()) if ex is not None else [])
    opt = torch.optim.Adam(params, lr=lr)
    rng = np.random.default_rng(seed)

    def fwd(k):
        b = batch.data.batch(rows_all[k])
        h = net.emb(b[0], b[1], per_sample_weights=b[2]) + net.b0
        if ex is not None:
            h = h + ex(Xe[k])
        h = F.relu(h)
        for l in net.layers:
            h = F.relu(l(h))
        return net.v(h)[:, 0]

    def pred(idx):
        out = []
        with torch.no_grad():
            for i in range(0, len(idx), 8192):
                out.append(fwd(idx[i:i + 8192]).numpy())
        return np.concatenate(out) if out else np.zeros(0)

    yt = torch.from_numpy(y)
    best, state, bad = -1e9, None, 0
    for _ in range(epochs):
        perm = rng.permutation(tr)
        for i in range(0, len(perm), 2048):
            k = perm[i:i + 2048]
            loss = F.mse_loss(fwd(k), yt[k])
            opt.zero_grad()
            loss.backward()
            opt.step()
        e = ev(pred(va), y[va])
        if e > best + 1e-4:
            best, bad = e, 0
            state = ({k: v.clone() for k, v in net.state_dict().items()}, {k: v.clone() for k, v in ex.state_dict().items()} if ex else None)
        else:
            bad += 1
            if bad >= 2:
                break
    net.load_state_dict(state[0])
    if ex is not None:
        ex.load_state_dict(state[1])
    return pred


def gae(values, rewards_, game, lam):
    adv = np.zeros(len(values), np.float32)
    starts = np.flatnonzero(np.r_[True, game[1:] != game[:-1]])
    ends = np.r_[starts[1:], len(values)]
    for s, e in zip(starts, ends):
        v = values[s:e]
        r = rewards_[s:e]
        vn = np.append(v[1:], 0.0)
        delta = r + vn - v
        a = 0.0
        for t in range(e - s - 1, -1, -1):
            a = delta[t] + lam * a
            adv[s + t] = a
    return adv


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dir")
    ap.add_argument("ckpt")
    ap.add_argument("--out", help="write the summary json here")
    ap.add_argument("--skip-sparse", action="store_true")
    ap.add_argument("--no-curve", action="store_true", help="full observation: only the all-games fits")
    a = ap.parse_args()
    torch.set_num_threads(4)
    dss, meta = load(a.dir)
    K = len(dss)
    # one table of policy records over all rollouts
    recs = []
    for k, (ds, (_, rows)) in enumerate(zip(dss, meta)):
        batch = T.Batch(ds)
        r = batch.rows
        g = ds["game"][r]
        pw = power_feats(ds, rows)[r]
        rew = np.zeros(len(r), np.float32)
        ret = np.zeros(len(r), np.float32)
        starts = np.flatnonzero(np.r_[True, g[1:] != g[:-1]])
        ends = np.r_[starts[1:], len(r)]
        for s, e in zip(starts, ends):
            gi = g[s]
            rw = T.rewards(ds["blinds_passed"][r[s:e]], ds["game_passed"][gi], ds["game_won"][gi])
            rew[s:e] = rw
            ret[s:e] = np.cumsum(rw[::-1])[::-1]
        final = (ds["game_passed"] / 24.0 + ds["game_won"]).astype(np.float32)
        recs.append(dict(batch=batch, rows=r, game=g, seed=np.array(ds["game_seed"])[g], pw=pw, rew=rew, ret=ret,
                         phi=(ds["blinds_passed"][r] / 24.0).astype(np.float32), ante=np.clip(ds["ante"][r], 1, 8),
                         final_by_seed=dict(zip(ds["game_seed"], final)), passed_by_seed=dict(zip(ds["game_seed"], ds["game_passed"])),
                         nmask=batch.n_mask[r]))
    seeds = sorted(set(recs[0]["final_by_seed"]))
    is_test = {s: int(hashlib.sha1(("var:" + s).encode()).hexdigest(), 16) % 5 == 0 for s in seeds}
    is_val = {s: int(hashlib.sha1(("val:" + s).encode()).hexdigest(), 16) % 8 == 0 for s in seeds}
    y = np.concatenate([x["ret"] for x in recs])
    ante = np.concatenate([x["ante"] for x in recs])
    phi = np.concatenate([x["phi"] for x in recs])
    seed_arr = np.concatenate([x["seed"] for x in recs])
    roll = np.concatenate([np.full(len(x["ret"]), k) for k, x in enumerate(recs)])
    test = np.array([is_test[s] for s in seed_arr])
    val = np.array([is_val[s] for s in seed_arr]) & ~test
    train = ~test & ~val
    te, tr, va = np.nonzero(test)[0], np.nonzero(train)[0], np.nonzero(val)[0]
    n_games = {k: len(x["final_by_seed"]) for k, x in enumerate(recs)}
    print(f"{K} rollouts x {n_games[0]} seeds; records {len(y)}; test seeds {sum(is_test.values())}; return to go mean {y.mean():.3f} var {y.var():.4f}")
    wins = [np.mean([v >= 1 for v in x["final_by_seed"].values()]) for x in recs]
    print("sampled win rate per rollout:", [round(w, 3) for w in wins])
    res = {}

    def report(name, pred, idx=te):
        res[name] = dict(ev=round(ev(pred[idx], y[idx]), 4), by_ante=by_ante(pred[idx], y[idx], ante[idx]))
        print(f"  {name:48s} EV {res[name]['ev']:+.3f}   by ante {res[name]['by_ante']}", flush=True)

    # progress table (a floor)
    tab = collections.defaultdict(list)
    passed_all = np.round(phi * 24).astype(int)
    for p_, v_ in zip(passed_all[tr], y[tr]):
        tab[p_].append(v_)
    tabm = {k: np.mean(v) for k, v in tab.items()}
    report("table: blinds passed", np.array([tabm.get(p_, y[tr].mean()) for p_ in passed_all]))

    # a: the run's critic, as is and refit
    ck = torch.load(a.ckpt, map_location="cpu", weights_only=False)
    crit = T.Critic(ck["critic"]["cols"].tolist())
    crit.load_state_dict(ck["critic"])
    Xc = np.concatenate([crit.features(x["batch"].ds, x["rows"]).numpy() for x in recs])
    v_run = T.critic_values(crit, torch.from_numpy(Xc))
    report("a: run critic (iteration 74, as trained)", v_run)
    m_a = fit_dense(Xc, y, tr, va)
    v_a = predict_dense(m_a, Xc)
    report("a: run critic's blocks, refit here", v_a)

    # b: per-seed baselines
    finals = np.array([[x["final_by_seed"][s] for x in recs] for s in seeds])  # [seed, K]
    passed = np.array([[x["passed_by_seed"][s] for x in recs] for s in seeds])
    sidx = {s: i for i, s in enumerate(seeds)}
    si = np.array([sidx[s] for s in seed_arr])
    others = (finals[si].sum(1) - finals[si, roll]) / (K - 1)
    v_b = others - phi
    report("b: seed baseline (others' mean final - progress)", v_b)
    # b2: only the other rollouts that got this far
    reached = passed[si] >= np.round(phi * 24)[:, None]
    reached[np.arange(len(si)), roll] = False
    num = (finals[si] * reached).sum(1)
    den = reached.sum(1)
    v_b2 = np.where(den > 0, num / np.maximum(den, 1) - phi, v_b)
    report("b2: seed baseline, rollouts that got this far", v_b2)
    seed_var = np.var(finals.mean(1)) / np.var(finals)
    print(f"  share of the final outcome's variance between seeds (one-way ANOVA, raw): {seed_var:.3f}")

    # d: run critic blocks + power
    pw = np.concatenate([x["pw"] for x in recs])
    Xd = np.concatenate([Xc, pw], 1)
    m_d = fit_dense(Xd, y, tr, va)
    v_d = predict_dense(m_d, Xd)
    report("d: run critic's blocks + power", v_d)
    m_p = fit_dense(np.concatenate([Xc[:, :52], pw], 1), y, tr, va)  # scalars + power only
    report("d: scalars + power", predict_dense(m_p, np.concatenate([Xc[:, :52], pw], 1)))

    # e: d with the seed baseline as an input
    Xe = np.concatenate([Xd, v_b2[:, None], others[:, None]], 1)
    m_e = fit_dense(Xe, y, tr, va)
    v_e = predict_dense(m_e, Xe)
    report("e: blocks + power + seed baseline (b2, b)", v_e)
    Xe2 = np.concatenate([Xc, v_b2[:, None], others[:, None]], 1)
    report("e: blocks + seed baseline", predict_dense(fit_dense(Xe2, y, tr, va), Xe2))

    # c: full observation, learning curve over training games
    best_name, best_v = max([("d", v_d), ("e", v_e)], key=lambda t: ev(t[1][te], y[te]))
    if not a.skip_sparse:
        # pool all rollouts into one dataset view for the sparse batcher
        offs = np.cumsum([0] + [len(x["ret"]) for x in recs])

        class Pool:
            pass
        # concatenate CSR pieces
        allds = {"obs_dim": dss[0]["obs_dim"], "n_actions": dss[0]["n_actions"]}
        ip, ii, iv, mp, mi, act = [np.zeros(1, np.int64)], [], [], [np.zeros(1, np.int64)], [], []
        for x in recs:
            ds = x["batch"].ds
            for r in x["rows"]:
                a0, a1 = ds["obs_indptr"][r], ds["obs_indptr"][r + 1]
                ii.append(ds["obs_indices"][a0:a1])
                iv.append(ds["obs_values"][a0:a1])
                ip.append(np.array([a1 - a0]))
                b0, b1 = ds["mask_indptr"][r], ds["mask_indptr"][r + 1]
                mi.append(ds["mask_ids"][b0:b1])
                mp.append(np.array([b1 - b0]))
                act.append(ds["action"][r])
        allds.update(obs_indptr=np.cumsum(np.concatenate(ip)), obs_indices=np.concatenate(ii), obs_values=np.concatenate(iv),
                     mask_indptr=np.cumsum(np.concatenate(mp)), mask_ids=np.concatenate(mi), action=np.array(act),
                     game=np.zeros(len(act), np.int64), game_won=np.zeros(1), game_blinds=np.zeros(1))
        pool = Pool()
        pool.ds = allds
        pool.data = B.Data(allds)
        rows_all = np.arange(len(act))
        game_key = np.array([f"{s}:{k}" for s, k in zip(seed_arr, roll)])
        tr_games = sorted(set(game_key[tr]))
        rng = np.random.default_rng(0)
        rng.shuffle(tr_games)
        for n in ((len(tr_games),) if a.no_curve else (500, 1000, 2000, len(tr_games))):
            keep = set(tr_games[:n])
            trn = np.array([i for i in tr if game_key[i] in keep])
            p = fit_sparse(pool, rows_all, y, trn, va)
            report(f"c: full observation, {n} training games", p(np.arange(len(y))))
        p = fit_sparse(pool, rows_all, y, tr, va, extra=np.concatenate([pw, v_b2[:, None], others[:, None]], 1))
        v_c = p(np.arange(len(y)))
        report(f"e: full observation + power + seed baseline, {len(tr_games)} games", v_c)
        if ev(v_c[te], y[te]) > ev(best_v[te], y[te]):
            best_name, best_v = "e-full", v_c

    # advantages: the run's critic vs the best estimator (test seeds, unforced records)
    rew = np.concatenate([x["rew"] for x in recs])
    gkey = np.concatenate([x["game"] + 100000 * k for k, x in enumerate(recs)])
    adv_run = gae(v_run, rew, gkey, 0.95)
    adv_best = gae(best_v, rew, gkey, 0.95)
    unf = np.concatenate([x["nmask"] for x in recs]) > 1
    m = test & unf
    c = np.corrcoef(adv_run[m], adv_best[m])[0, 1]
    print(f"\nadvantages (GAE 0.95, test seeds, unforced records): corr(run critic, best = {best_name}) = {c:.3f}; "
          f"std run {adv_run[m].std():.3f}, best {adv_best[m].std():.3f}")
    for lam in (0.0, 0.5):
        c2 = np.corrcoef(gae(v_run, rew, gkey, lam)[m], gae(best_v, rew, gkey, lam)[m])[0, 1]
        print(f"   with lambda {lam}: corr {c2:.3f}")
    res["adv_corr"] = dict(best=best_name, corr=round(float(c), 4), std_run=float(adv_run[m].std()), std_best=float(adv_best[m].std()))
    if a.out:
        json.dump(res, open(a.out, "w"), indent=1)


if __name__ == "__main__":
    main()
