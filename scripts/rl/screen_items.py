#!/usr/bin/env python3
"""Milestone 9 screen (PyTorch only, offline): can a network that scores jokers with shared weights predict v77's
joker choice (buy_joker / replace_joker) better than the flat MLP, from the same v77 labels?

    /home/user/rlvenv/bin/python scripts/rl/screen_items.py --out runs/_rl/screen [--models flat,item,itematt] [--fracs 1]

Data: v77's records only: runs/_rl/bc-v77-s2/games (all antes) and the v77 side (ante > K) of runs/_rl/dagger/r0
and r1 (gen_dagger.sh). Split by game: sha1(seed) % 10 == 0 is the test set (the split of train_bc.py /
train_dagger.py, so v1a never trained on it), == 1 the validation set (early stopping on its cross-entropy), the
rest training (--fracs takes the first fraction of the training games in a fixed shuffled order: learning curve).
Models, all trained from scratch with cross-entropy over the legal mask on every v77 record:
  flat     train_bc.Net (sparse embedding, 256-256 MLP, one logit per vocabulary id)
  item     nets.ItemNet (attention=False): the same trunk gives a context vector and the logits of every other id; buy_joker:<key> and
           replace_joker:<sold>@slot<k> logits come from one shared scorer MLP over (context, the shop joker's
           embedding, the sold joker's embedding (zero for a buy), the mean of the owned jokers' embeddings) plus a
           family bias. A joker's embedding = a shared per-key embedding + a linear map of its observation row:
           shop jokers: count, editions, stickers, cost, buyable; owned jokers: count, editions, stickers,
           debuffed, sell value, counters. The shop slot's joker comes from the shop_slot_jokers block.
  itematt  item, with the owned summary an attention over the owned jokers, queried by the candidate (shop joker,
           sold joker, context), instead of the mean.
Metric (test games): on records where v77 acquired a joker (buy_joker or replace_joker) with at least two legal
joker-acquiring ids, the model's logits restricted to those ids: top-1 == v77's (by v77's family, with the record
count and the mean number of legal alternatives); also exact replace_joker agreement, swap family recall and
precision, and overall top-1 on unforced records. v1a (runs/_rl/dagger/v1a) is scored on the same metric.
Writes OUT/report.json.
"""
import argparse
import glob
import hashlib
import json
import os
import sys
import time

import numpy as np
import torch
import torch.nn.functional as F

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import read_rlb as R  # noqa: E402
import train_bc as B  # noqa: E402
import train_dagger as D  # noqa: E402
import nets as N  # noqa: E402

BUY0, NBUY = 7, 150
REP0, NREP = 723, 900


def load(dagger_dirs, v77_dirs):
    dss, keep = [], []
    for Dd in dagger_dirs:
        for kd in sorted(glob.glob(os.path.join(Dd, "k*"))):
            K = int(os.path.basename(kd)[1:])
            ds = R.load_dir(os.path.join(kd, "games"))
            dss.append(ds)
            keep.append(ds["ante"] > K)
    for G in v77_dirs:
        ds = R.load_dir(G)
        dss.append(ds)
        keep.append(np.ones(len(ds["action"]), bool))
    return D.merge(dss), np.concatenate(keep)


def fwd(model, b):
    lg, _ = model(b[0], b[1], b[2], b[3])
    return lg.masked_fill(~b[3], -1e9)


def predict(model, data, rows, bs=2048):
    model.eval()
    out, ces = [], []
    with torch.no_grad():
        for i in range(0, len(rows), bs):
            b = data.batch(rows[i:i + bs])
            ml = fwd(model, b)
            ces.append(F.cross_entropy(ml, b[4], reduction="none").numpy())
            out.append(ml.numpy().astype(np.float32))
    model.train()
    return np.concatenate(out), np.concatenate(ces)


def train(model, data, tr, va, lr, bs, epochs, patience, seed, log):
    torch.manual_seed(seed)
    rng = np.random.default_rng(seed)
    opt = torch.optim.Adam(model.parameters(), lr=lr)
    best, state, bad = 1e9, None, 0
    t0 = time.time()
    for ep in range(epochs):
        perm = rng.permutation(tr)
        tl, n = 0.0, 0
        for i in range(0, len(perm), bs):
            b = data.batch(perm[i:i + bs])
            loss = F.cross_entropy(fwd(model, b), b[4])
            opt.zero_grad()
            loss.backward()
            opt.step()
            tl += float(loss.detach())
            n += 1
        _, ce = predict(model, data, va)
        log(f"    epoch {ep + 1}: train ce {tl / n:.4f} val ce {ce.mean():.4f} ({time.time() - t0:.0f}s)")
        if ce.mean() < best - 1e-4:
            best, bad = float(ce.mean()), 0
            state = {k: v.clone() for k, v in model.state_dict().items()}
        else:
            bad += 1
            if bad >= patience:
                break
    model.load_state_dict(state)
    return best


def metrics(logits, ds, rows, fams):
    act = ds["action"][rows].astype(np.int64)
    ml = np.diff(ds["mask_indptr"])[rows]
    pred = logits.argmax(1)
    af = fams[act]
    pf = fams[pred]
    m = {}
    unf = ml > 1
    m["overall_top1_unforced"] = [int(unf.sum()), float((pred == act)[unf].mean())]
    sw = af == "replace_joker"
    m["replace_exact"] = [int(sw.sum()), float((pred == act)[sw].mean())]
    m["swap_recall"] = float((pf[sw] == "replace_joker").mean())
    m["swap_precision"] = float((af[pf == "replace_joker"] == "replace_joker").mean()) if (pf == "replace_joker").any() else float("nan")
    # joker choice among the legal joker-acquiring ids
    jl = logits.copy()
    jl[:, :BUY0] = -np.inf
    jl[:, BUY0 + NBUY:REP0] = -np.inf
    legal_j = (logits[:, BUY0:BUY0 + NBUY] > -1e8).sum(1) + (logits[:, REP0:REP0 + NREP] > -1e8).sum(1)
    jp = jl.argmax(1)
    for f in ("buy_joker", "replace_joker", "both"):
        sel = (legal_j >= 2) & ((af == f) if f != "both" else np.isin(af, ["buy_joker", "replace_joker"]))
        m[f"joker_choice_{f}"] = [int(sel.sum()), float((jp == act)[sel].mean()) if sel.any() else float("nan"),
                                  float(legal_j[sel].mean()) if sel.any() else float("nan"), float((1.0 / legal_j[sel]).mean()) if sel.any() else float("nan")]
    return m


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default="runs/_rl/screen")
    ap.add_argument("--dagger", default="runs/_rl/dagger/r0,runs/_rl/dagger/r1")
    ap.add_argument("--v77", default="runs/_rl/bc-v77-s2/games")
    ap.add_argument("--models", default="flat,item,itematt")
    ap.add_argument("--fracs", default="1")
    ap.add_argument("--epochs", type=int, default=15)
    ap.add_argument("--patience", type=int, default=2)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--bs", type=int, default=512)
    ap.add_argument("--v1a", default="runs/_rl/dagger/v1a/model.pt")
    a = ap.parse_args()
    torch.set_num_threads(4)
    os.makedirs(a.out, exist_ok=True)
    log = lambda s: print(s, flush=True)
    ds, keep = load(a.dagger.split(","), a.v77.split(","))
    names = ds["schema"]["vocab"]["names"]
    fams = B.fam_of(names)
    fams = np.append(fams, "?")
    data = B.Data(ds)
    h = np.array([int(hashlib.sha1(s.encode()).hexdigest(), 16) % 10 for s in ds["game_seed"]])
    ok = keep & (ds["action"] != R.UNEXPRESSIBLE)
    g = ds["game"]
    te = np.where(ok & (h[g] == 0))[0]
    va = np.where(ok & (h[g] == 1))[0]
    tr_games = np.random.default_rng(0).permutation(np.where(h >= 2)[0])
    log(f"{len(ds['game_seed'])} games; records train {int((ok & (h[g] >= 2)).sum())} val {len(va)} test {len(te)}")
    rep = {"rows": {}}
    path = os.path.join(a.out, "report.json")
    if os.path.exists(path):
        rep = json.load(open(path))

    def show(label, m):
        jb, js = m["joker_choice_buy_joker"], m["joker_choice_replace_joker"]
        log(f"  {label:20s} joker choice: buys {jb[1]:.3f} (n={jb[0]}, legal {jb[2]:.1f}, chance {jb[3]:.3f})  swaps {js[1]:.3f} (n={js[0]}, legal {js[2]:.1f}, "
            f"chance {js[3]:.3f})  | replace exact {m['replace_exact'][1]:.3f} swap recall {m['swap_recall']:.3f} precision {m['swap_precision']:.3f} "
            f"| overall {m['overall_top1_unforced'][1]:.3f}")

    if a.v1a and "v1a" not in rep["rows"]:
        net = D.net_from(D.load_actor(a.v1a)[1])
        lg, _ = predict(net, data, te)
        rep["rows"]["v1a"] = metrics(lg, ds, te, fams)
    if "v1a" in rep["rows"]:
        show("v1a (DAgger)", rep["rows"]["v1a"])
    for frac in [float(x) for x in a.fracs.split(",")]:
        gsel = np.zeros(len(ds["game_seed"]), bool)
        gsel[tr_games[:int(round(frac * len(tr_games)))]] = True
        tr = np.where(ok & gsel[g])[0]
        for mname in a.models.split(","):
            label = f"{mname}@{frac:g}"
            log(f"{label}: {int(gsel.sum())} training games, {len(tr)} records")
            if mname == "flat":
                model = B.Net(ds["obs_dim"], ds["n_actions"], [256, 256])
            else:
                model = N.ItemNet(ds["obs_dim"], ds["n_actions"], N.item_layout(ds["schema"]), attention=(mname == "itematt"))
            vce = train(model, data, tr, va, a.lr, a.bs, a.epochs, a.patience, 0, log)
            lg, ce = predict(model, data, te)
            m = metrics(lg, ds, te, fams)
            m["val_ce"], m["test_ce"], m["train_games"] = vce, float(ce.mean()), int(gsel.sum())
            rep["rows"][label] = m
            show(label, m)
            json.dump(rep, open(path, "w"), indent=1)
    json.dump(rep, open(path, "w"), indent=1)


if __name__ == "__main__":
    main()
