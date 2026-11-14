#!/usr/bin/env python3
"""DAgger fine-tune: teach a PPO policy v77's late shop decisions (antes 5+) while keeping its own early game.

    /home/user/rlvenv/bin/python scripts/rl/train_dagger.py --out runs/_rl/dagger/v0 \
        --init-ckpt runs/_rl/ppo/rl-ppo-a/ckpt.pt --dagger runs/_rl/dagger/r0 [--dagger runs/_rl/dagger/r1 ...] \
        [--v77 runs/_rl/bc-v77-s2/games] [--weight-family replace_joker=3,sell_joker=3] [--keep 1.0]

Data (rlcore v2 logs, read_rlb.py):
  --dagger D   gen_dagger.sh output: D/kK/games/*.rlb.gz, where records with ante <= K are the network's and the rest
               v77's (JEVLAB_RLANTES=1-K). v77's records (all at ante K+1 >= 5) are labels; the network's records at
               antes 1-4 are the keep set; its records at antes 5..K are not used.
  --v77 G      v77's own games (bc-v77-s2): their records at ante >= 5 are labels too.
Loss: cross-entropy (masked to the legal ids, weighted by --weight-family on v77's chosen family) on the labels,
plus --keep x KL(reference || new) on the keep set, the reference being the starting policy (frozen), so the early
game stays what it is. Each step takes one label batch and one keep batch of the same size. Forced records (one legal
id) and unexpressible ones are left out. The value head is not trained (PPO's critic is separate).
Split: by game, sha1(seed) % 10 == 0 is held out (as train_bc.py). The kept epoch is the one with the lowest held-out
label cross-entropy + keep x held-out keep KL (early stop after --patience epochs without improvement).
Report (printed and OUT/report.json): held-out late top-1 agreement with v77 before (the starting policy) and after,
overall and by v77's chosen family, for the DAgger labels and v77's own games separately; the keep set's held-out KL
and argmax agreement with the reference.
Writes OUT/model.pt (train_bc.py's format, so train_rl.py --init can read it), OUT/weights.brlw, and OUT/ckpt.pt: a
train_rl.py run checkpoint (iteration 0, the new actor, the critic and its optimiser copied from --init-ckpt's run,
a fresh actor optimiser, the run's config with init = OUT and bin = --bin), which train_rl.py resumes from when it
is copied to runs/_rl/ppo/NAME/ckpt.pt.
"""
import argparse
import collections
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

UNEXP = R.UNEXPRESSIBLE


def merge(dss):
    """Concatenate load_dir datasets (same schema) into one."""
    d0 = dss[0]
    out = {k: d0[k] for k in ("obs_dim", "n_actions", "schema_version", "schema_hash", "schema")}
    for d in dss:
        assert d["schema_hash"] == d0["schema_hash"], "schema mismatch between sources"
    rec_cols = ["step", "ante", "phase", "blind_index", "round", "money", "blinds_cleared", "blinds_passed", "action", "atype", "aindex",
                "ncards", "flags"]
    for c in rec_cols:
        out[c] = np.concatenate([d[c] for d in dss])
    for ip, data in (("obs_indptr", ("obs_indices", "obs_values")), ("mask_indptr", ("mask_ids",))):
        parts, off = [np.zeros(1, np.int64)], 0
        for d in dss:
            parts.append(d[ip][1:] + off)
            off += d[ip][-1]
        out[ip] = np.concatenate(parts)
        for k in data:
            out[k] = np.concatenate([d[k] for d in dss])
    goff, games = 0, []
    for d in dss:
        games.append(d["game"] + goff)
        goff += len(d["game_seed"])
    out["game"] = np.concatenate(games)
    for k in ("game_seed", "game_file", "game_counters"):
        out[k] = [x for d in dss for x in d[k]]
    for k in ("game_won", "game_ante", "game_blinds", "game_round", "game_passed"):
        out[k] = np.concatenate([d[k] for d in dss])
    return out


def load_sources(dagger_dirs, v77_dirs):
    """Returns (ds, role per record: 0 unused, 1 DAgger label, 2 v77-own label, 3 keep)."""
    dss, roles = [], []
    for D in dagger_dirs:
        for kd in sorted(glob.glob(os.path.join(D, "k*"))):
            K = int(os.path.basename(kd)[1:])
            ds = R.load_dir(os.path.join(kd, "games"))
            r = np.zeros(len(ds["action"]), np.int8)
            r[ds["ante"] > K] = 1
            r[ds["ante"] <= 4] = 3
            dss.append(ds)
            roles.append(r)
            print(f"  {kd}: {len(ds['game_seed'])} games, {len(r)} records: v77 {int((r == 1).sum())}, keep {int((r == 3).sum())}, "
                  f"unused {int((r == 0).sum())}", flush=True)
    for G in v77_dirs:
        ds = R.load_dir(G)
        r = np.where(ds["ante"] >= 5, 2, 0).astype(np.int8)
        dss.append(ds)
        roles.append(r)
        print(f"  {G}: {len(ds['game_seed'])} games, {len(r)} records: v77 ante 5+ {int((r == 2).sum())}", flush=True)
    return merge(dss), np.concatenate(roles)


def load_actor(path):
    """An actor state dict from a train_rl.py ckpt.pt ("actor") or a train_bc.py model.pt ("state")."""
    ck = torch.load(path, map_location="cpu", weights_only=False)
    return ck, ck["actor"] if "actor" in ck else ck["state"]


def net_from(sd):
    n_layers = len([k for k in sd if k.startswith("layers.") and k.endswith(".weight")])
    hidden = [sd["emb.weight"].shape[1]] + [sd[f"layers.{i}.weight"].shape[0] for i in range(n_layers)]
    m = B.Net(sd["emb.weight"].shape[0], sd["pi.weight"].shape[0], hidden, n_value=sd["v.weight"].shape[0])
    m.load_state_dict(sd)
    return m


def logp(model, b):
    logits, _ = model(b[0], b[1], b[2])
    return F.log_softmax(logits.masked_fill(~b[3], -1e9), 1)


def predict(model, data, rows, ref=None, bs=4096):
    """argmax over legal ids, CE to the logged action, and (with ref) KL(ref || model) and ref's argmax, per record."""
    model.eval()
    pa, ce, kl, ra = [], [], [], []
    with torch.no_grad():
        for i in range(0, len(rows), bs):
            b = data.batch(rows[i:i + bs])
            lp = logp(model, b)
            pa.append(lp.argmax(1).numpy())
            ce.append((-lp.gather(1, b[4][:, None])[:, 0]).numpy())
            if ref is not None:
                lr = logp(ref, b)
                pr = lr.exp()
                kl.append((pr * (lr - lp)).masked_fill(~b[3], 0).sum(1).numpy())
                ra.append(lr.argmax(1).numpy())
    model.train()
    cat = lambda x: np.concatenate(x) if x else np.zeros(0)
    return cat(pa), cat(ce), cat(kl), cat(ra)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--init-ckpt", default="runs/_rl/ppo/rl-ppo-a/ckpt.pt", help="starting policy (and reference for the keep term)")
    ap.add_argument("--dagger", action="append", default=[], help="gen_dagger.sh output dir (repeatable)")
    ap.add_argument("--v77", action="append", default=[], help="v77's own logs dir (repeatable); ante 5+ records are labels")
    ap.add_argument("--weight-family", default="", help="label weights by v77's chosen family, e.g. replace_joker=3,sell_joker=3")
    ap.add_argument("--keep", type=float, default=1.0, help="weight of KL(reference || new) on the network's antes 1-4 records")
    ap.add_argument("--lr", type=float, default=3e-4)
    ap.add_argument("--bs", type=int, default=512)
    ap.add_argument("--epochs", type=int, default=12)
    ap.add_argument("--patience", type=int, default=2)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--bin", default="runs/_rl/bin/jevlab-rl11", help="jevlab binary written into ckpt.pt's run config")
    ap.add_argument("--threads", type=int, default=4)
    a = ap.parse_args()
    torch.set_num_threads(a.threads)
    torch.manual_seed(a.seed)
    rng = np.random.default_rng(a.seed)
    os.makedirs(a.out, exist_ok=True)
    t0 = time.time()
    ds, role = load_sources(a.dagger, a.v77)
    names = ds["schema"]["vocab"]["names"]
    fams = B.fam_of(names)
    data = B.Data(ds)
    ok = (ds["action"] != UNEXP) & (np.diff(ds["mask_indptr"]) > 1)
    afam = np.where(ds["action"] == UNEXP, "?", fams[np.minimum(ds["action"], len(names) - 1)])
    for kv in filter(None, a.weight_family.split(",")):
        f, x = kv.split("=")
        data.w[afam == f] = float(x)
    held_game = np.array([int(hashlib.sha1(s.encode()).hexdigest(), 16) % 10 == 0 for s in ds["game_seed"]])
    held = held_game[ds["game"]]
    lab = ok & ((role == 1) | (role == 2))
    keep = ok & (role == 3)
    tr_lab, va_lab = np.where(lab & ~held)[0], np.where(lab & held)[0]
    tr_keep, va_keep = np.where(keep & ~held)[0], np.where(keep & held)[0]
    print(f"loaded {len(ds['game_seed'])} games in {time.time() - t0:.0f}s; labels train {len(tr_lab)} held {len(va_lab)}; "
          f"keep train {len(tr_keep)} held {len(va_keep)}", flush=True)

    ck, sd = load_actor(a.init_ckpt)
    ref = net_from(sd)
    ref.eval()
    for p in ref.parameters():
        p.requires_grad_(False)
    model = net_from(sd)
    opt = torch.optim.Adam(model.parameters(), lr=a.lr)

    def held_score():
        _, ce, _, _ = predict(model, data, va_lab)
        _, _, kl, _ = predict(model, data, va_keep, ref)
        return float(ce.mean()), float(kl.mean())

    p_before, _, _, _ = predict(ref, data, va_lab)
    ce0, kl0 = held_score()
    print(f"before: held-out label ce {ce0:.4f}, keep kl {kl0:.4f}", flush=True)
    best, best_state, bad, hist = ce0 + a.keep * kl0, None, 0, []
    for ep in range(a.epochs):
        perm = rng.permutation(tr_lab)
        tot = collections.Counter()
        for i in range(0, len(perm), a.bs):
            b = data.batch(perm[i:i + a.bs])
            lp = logp(model, b)
            ce = (-lp.gather(1, b[4][:, None])[:, 0] * b[7]).sum() / b[7].sum()
            kb = data.batch(rng.choice(tr_keep, size=a.bs, replace=False))
            lpk = logp(model, kb)
            with torch.no_grad():
                lr_ = logp(ref, kb)
            kl = (lr_.exp() * (lr_ - lpk)).masked_fill(~kb[3], 0).sum(1).mean()
            loss = ce + a.keep * kl
            opt.zero_grad()
            loss.backward()
            opt.step()
            tot["ce"] += float(ce.detach())
            tot["kl"] += float(kl.detach())
            tot["n"] += 1
        vce, vkl = held_score()
        score = vce + a.keep * vkl
        hist.append(dict(epoch=ep + 1, train_ce=tot["ce"] / tot["n"], train_kl=tot["kl"] / tot["n"], held_ce=vce, held_kl=vkl))
        print(f"  epoch {ep + 1}: train ce {tot['ce'] / tot['n']:.4f} kl {tot['kl'] / tot['n']:.4f}  held ce {vce:.4f} kl {vkl:.4f}  "
              f"({time.time() - t0:.0f}s)", flush=True)
        if score < best - 1e-4:
            best, bad = score, 0
            best_state = {k: v.clone() for k, v in model.state_dict().items()}
        else:
            bad += 1
            if bad >= a.patience:
                break
    if best_state is not None:
        model.load_state_dict(best_state)

    p_after, _, _, _ = predict(model, data, va_lab)
    _, _, kl_k, ra_k = predict(model, data, va_keep, ref)
    pk_after, _, _, _ = predict(model, data, va_keep)
    act = data.act[va_lab]
    src = role[va_lab]
    vf = afam[va_lab]
    rep = {"args": vars(a), "history": hist, "rows": {}}
    print()
    print(f"HELD-OUT LATE TOP-1 AGREEMENT WITH V77 (unforced records; before = {a.init_ckpt}, after = this network)")
    print(f"  {'':36s} {'n':>6} {'before':>7} {'after':>7}")

    def row(label, m):
        if m.sum() == 0:
            return
        b_, a_ = float((p_before[m] == act[m]).mean()), float((p_after[m] == act[m]).mean())
        print(f"  {label:36s} {int(m.sum()):6d} {b_:7.3f} {a_:7.3f}")
        rep["rows"][label] = [int(m.sum()), b_, a_]

    for sname, sm in (("all", np.ones(len(act), bool)), ("dagger", src == 1), ("v77 own", src == 2)):
        row(f"{sname}", sm)
    for f, _ in collections.Counter(vf.tolist()).most_common():
        row(f"  v77 chose {f}", vf == f)
        if (src == 1).any():
            row(f"    dagger: {f}", (vf == f) & (src == 1))
    ante = ds["ante"][va_lab]
    for an in np.unique(ante):
        row(f"  ante {an}", ante == an)
    conf = collections.Counter((vf[k], fams[p_after[k]]) for k in np.where(p_after != act)[0])
    print("  top disagreements after (v77 chose -> network chose):")
    for (x, y), n in conf.most_common(12):
        print(f"    {x:22s} -> {y:22s} {n}")
    kagree = float((pk_after == ra_k).mean()) if len(va_keep) else float("nan")
    print(f"KEEP SET (network's antes 1-4, held out, {len(va_keep)} records): KL(reference || new) {kl_k.mean():.4f}, argmax agrees with the reference {kagree:.3f}")
    rep["keep"] = {"n": int(len(va_keep)), "kl": float(kl_k.mean()), "argmax_agree": kagree}
    rep["disagreements"] = [[x, y, n] for (x, y), n in conf.most_common(30)]

    hidden = [model.emb.weight.shape[1]] + [l.weight.shape[0] for l in model.layers]
    torch.save({"state": model.state_dict(), "hidden": hidden, "schema_hash": ds["schema_hash"]}, os.path.join(a.out, "model.pt"))
    wpath = os.path.join(a.out, "weights.brlw")
    B.export_weights(model, wpath, ds["schema_hash"], ds["obs_dim"], ds["n_actions"])
    if "critic" in ck:  # a train_rl.py run checkpoint at iteration 0 that resumes with this actor and that run's critic
        cfg = dict(ck["cfg"])
        cfg.update(init=os.path.abspath(a.out), bin=os.path.abspath(a.bin), init_ckpt=None)
        opt_a = torch.optim.Adam(model.parameters(), lr=cfg["lr"])
        torch.save({"cfg": cfg, "iter": 0, "curve": [], "actor": model.state_dict(), "critic": ck["critic"], "opt_a": opt_a.state_dict(),
                    "opt_c": ck["opt_c"]}, os.path.join(a.out, "ckpt.pt"))
    json.dump(rep, open(os.path.join(a.out, "report.json"), "w"), indent=1)
    print(f"wrote {wpath}, model.pt, ckpt.pt, report.json; {time.time() - t0:.0f}s total")


if __name__ == "__main__":
    main()
