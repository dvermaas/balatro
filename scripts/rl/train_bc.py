#!/usr/bin/env python3
"""Behaviour cloning of v77's strategic decisions (scripts/rl/read_rlb.py data) into the policy network the
fast environment runs (native/rlenv/src/net.rs), and the evaluation of the clone on held-out games.

    /home/user/rlvenv/bin/python scripts/rl/train_bc.py runs/_rl/bc-v77/games --out runs/_rl/bc/v1 \
        [--hidden 256,256] [--epochs 30] [--curve 100,200,400] [--check 1000]

Network: sparse observation -> Linear (EmbeddingBag, sum of value-weighted rows) -> ReLU -> hidden Linear+ReLU
... -> policy logits (n_actions, masked to the legal ids) and value (2: logit P(win), final blinds / 24).
Loss: cross-entropy over the legal mask + W * (BCE(win) + MSE(blinds / 24)), W = --value-weight (0.5); with
--value-detach the value head reads a detached trunk (the outcome is one label per game, and with a few hundred
games a value loss through the trunk memorises games). The checkpoint kept is the epoch with the best validation
cross-entropy (early stop after 3 epochs without improvement).
Split: by game (seed): a game is validation when sha1(seed) % 10 == 0 (~10%), so the split is stable as data grows.
Writes OUT/model.pt, OUT/weights.brlw (BRLW, carries the schema hash) and OUT/report.json; prints the tables.
"""
import argparse
import collections
import hashlib
import json
import os
import struct
import subprocess
import sys
import time

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import read_rlb as R  # noqa: E402

UNEXP = R.UNEXPRESSIBLE


class Net(nn.Module):
    def __init__(self, obs_dim, n_actions, hidden, n_value=2):
        super().__init__()
        self.emb = nn.EmbeddingBag(obs_dim, hidden[0], mode="sum")
        self.b0 = nn.Parameter(torch.zeros(hidden[0]))
        self.layers = nn.ModuleList(nn.Linear(hidden[i - 1], hidden[i]) for i in range(1, len(hidden)))
        self.pi = nn.Linear(hidden[-1], n_actions)
        self.v = nn.Linear(hidden[-1], n_value)
        nn.init.normal_(self.emb.weight, std=1.0 / np.sqrt(20.0))
        self.hidden = list(hidden)

    value_detach = False

    def forward(self, idx, offsets, vals):
        h = F.relu(self.emb(idx, offsets, per_sample_weights=vals) + self.b0)
        for l in self.layers:
            h = F.relu(l(h))
        return self.pi(h), self.v(h.detach() if self.value_detach else h)


def export_weights(model, path, schema_hash, obs_dim, n_actions):
    """BRLW format, see native/rlenv/src/net.rs."""
    with open(path, "wb") as f:
        f.write(b"BRLW")
        f.write(struct.pack("<IQII", 1, schema_hash, obs_dim, n_actions))
        f.write(struct.pack("<I", len(model.hidden)))
        f.write(struct.pack(f"<{len(model.hidden)}I", *model.hidden))
        f.write(struct.pack("<I", model.v.out_features))
        w = lambda t: f.write(t.detach().cpu().float().contiguous().numpy().astype("<f4").tobytes())
        w(model.emb.weight)  # [obs_dim][h0], input-major
        w(model.b0)
        for l in model.layers:
            w(l.weight)
            w(l.bias)
        w(model.pi.weight)
        w(model.pi.bias)
        w(model.v.weight)
        w(model.v.bias)


def gather(indptr, data, rows):
    starts = indptr[rows]
    lens = indptr[rows + 1] - starts
    tot = int(lens.sum())
    excl = np.concatenate([[0], np.cumsum(lens)[:-1]])
    pos = np.repeat(starts - excl, lens) + np.arange(tot)
    return data[pos], lens, excl


class Data:
    def __init__(self, ds):
        self.ds = ds
        self.n_actions = ds["n_actions"]
        self.act = ds["action"].astype(np.int64)
        g = ds["game"]
        self.won = ds["game_won"][g].astype(np.float32)
        self.blinds = (ds["game_blinds"][g] / 24.0).astype(np.float32)

    def batch(self, rows):
        ds = self.ds
        idx, lens, excl = gather(ds["obs_indptr"], ds["obs_indices"], rows)
        val, _, _ = gather(ds["obs_indptr"], ds["obs_values"], rows)
        mids, mlens, _ = gather(ds["mask_indptr"], ds["mask_ids"], rows)
        mask = torch.zeros((len(rows), self.n_actions), dtype=torch.bool)
        mask[torch.from_numpy(np.repeat(np.arange(len(rows)), mlens)), torch.from_numpy(mids.astype(np.int64))] = True
        return (torch.from_numpy(idx.astype(np.int64)), torch.from_numpy(excl.astype(np.int64)), torch.from_numpy(val),
                mask, torch.from_numpy(self.act[rows]), torch.from_numpy(self.won[rows]), torch.from_numpy(self.blinds[rows]))


def losses(model, b):
    idx, off, val, mask, act, won, blinds = b
    logits, v = model(idx, off, val)
    ml = logits.masked_fill(~mask, -1e9)
    ce = F.cross_entropy(ml, act)
    vl = F.binary_cross_entropy_with_logits(v[:, 0], won) + F.mse_loss(v[:, 1], blinds)
    return ce, vl, ml


def evaluate(model, data, rows, bs=4096):
    model.eval()
    preds, ces = [], 0.0
    with torch.no_grad():
        for i in range(0, len(rows), bs):
            b = data.batch(rows[i:i + bs])
            ce, _, ml = losses(model, b)
            ces += float(ce) * len(b[4])
            preds.append(ml.argmax(1).numpy())
    model.train()
    return (np.concatenate(preds) if preds else np.zeros(0, np.int64)), ces / max(1, len(rows))


VALUE_WEIGHT = 0.5
VALUE_DETACH = False


def train(data, tr, va, hidden, epochs, lr, bs, seed, log=True):
    torch.manual_seed(seed)
    rng = np.random.default_rng(seed)
    ds = data.ds
    model = Net(ds["obs_dim"], ds["n_actions"], hidden)
    model.value_detach = VALUE_DETACH
    opt = torch.optim.Adam(model.parameters(), lr=lr)
    best, best_state, bad = 1e9, None, 0
    t0 = time.time()
    for ep in range(epochs):
        perm = rng.permutation(tr)
        tot = [0.0, 0.0, 0]
        for i in range(0, len(perm), bs):
            b = data.batch(perm[i:i + bs])
            ce, vl, _ = losses(model, b)
            loss = ce + VALUE_WEIGHT * vl
            opt.zero_grad()
            loss.backward()
            opt.step()
            tot[0] += float(ce.detach()) * len(b[4])
            tot[1] += float(vl.detach()) * len(b[4])
            tot[2] += len(b[4])
        _, vce = evaluate(model, data, va)
        if log:
            print(f"  epoch {ep + 1}: train ce {tot[0] / tot[2]:.4f} value {tot[1] / tot[2]:.4f}  val ce {vce:.4f}  ({time.time() - t0:.0f}s)", flush=True)
        if vce < best - 1e-4:
            best, bad = vce, 0
            best_state = {k: v.clone() for k, v in model.state_dict().items()}
        else:
            bad += 1
            if bad >= 3:
                break
    model.load_state_dict(best_state)
    return model, best


def fam_of(names):
    return np.array([n.split(":")[0] for n in names])


def baselines(data, tr, va, fams, phases):
    """uniform over legal (expected accuracy), and the most frequent chosen family per phase (item: the most frequent id)."""
    ds = data.ds
    act = data.act
    ph = ds["phase"]
    mlen = np.diff(ds["mask_indptr"])
    uni = 1.0 / mlen[va]
    fam_rank = {}
    id_freq = collections.Counter(act[tr].tolist())
    for p in np.unique(ph[tr]):
        c = collections.Counter(fams[act[tr][ph[tr] == p]].tolist())
        fam_rank[p] = {f: i for i, (f, _) in enumerate(c.most_common())}
    pred = np.zeros(len(va), np.int64)
    for k, r in enumerate(va):
        legal = ds["mask_ids"][ds["mask_indptr"][r]:ds["mask_indptr"][r + 1]]
        rk = fam_rank.get(ph[r], {})
        pred[k] = min(legal, key=lambda a: (rk.get(fams[a], 99), -id_freq.get(int(a), 0), a))
    return uni, pred


def table(title, keys, groups):
    print(title)
    for k in keys:
        print("  " + k)
    for name, vals in groups:
        print(f"  {name:28s} " + "  ".join(f"{v:>8}" for v in vals))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("data")
    ap.add_argument("--out", default="runs/_rl/bc/v1")
    ap.add_argument("--hidden", default="256,256")
    ap.add_argument("--epochs", type=int, default=30)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--bs", type=int, default=512)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--curve", default="", help="training-game counts for a learning curve, e.g. 100,200,400")
    ap.add_argument("--check", type=int, default=1000, help="records for the Rust/PyTorch logit check (0 = skip)")
    ap.add_argument("--rlenv", default="native/target/release/rlenv")
    ap.add_argument("--limit", type=int)
    ap.add_argument("--value-weight", type=float, default=0.5)
    ap.add_argument("--value-detach", action="store_true", help="value head on a detached trunk (no gradient into the trunk)")
    a = ap.parse_args()
    global VALUE_WEIGHT, VALUE_DETACH
    VALUE_WEIGHT, VALUE_DETACH = a.value_weight, a.value_detach
    torch.set_num_threads(4)
    os.makedirs(a.out, exist_ok=True)
    t0 = time.time()
    ds = R.load_dir(a.data, a.limit)
    print(f"loaded {len(ds['game_seed'])} games, {len(ds['action'])} records in {time.time() - t0:.0f}s", flush=True)
    names = ds["schema"]["vocab"]["names"]
    fams = fam_of(names)
    phases = ds["schema"]["phases"]
    data = Data(ds)
    seeds = ds["game_seed"]
    is_val_game = np.array([int(hashlib.sha1(s.encode()).hexdigest(), 16) % 10 == 0 for s in seeds])
    ok = ds["action"] != UNEXP
    rec_val = is_val_game[ds["game"]]
    va = np.where(rec_val & ok)[0]
    tr_games = np.where(~is_val_game)[0]
    hidden = [int(x) for x in a.hidden.split(",")]
    print(f"train games {len(tr_games)}, val games {int(is_val_game.sum())}, val records {len(va)}", flush=True)

    def rows_of(games):
        gs = np.zeros(len(seeds), bool)
        gs[games] = True
        return np.where(gs[ds["game"]] & ok)[0]

    tr = rows_of(tr_games)
    model, vce = train(data, tr, va, hidden, a.epochs, a.lr, a.bs, a.seed)
    torch.save({"state": model.state_dict(), "hidden": hidden, "schema_hash": ds["schema_hash"]}, os.path.join(a.out, "model.pt"))
    wpath = os.path.join(a.out, "weights.brlw")
    export_weights(model, wpath, ds["schema_hash"], ds["obs_dim"], ds["n_actions"])
    pred, _ = evaluate(model, data, va)
    # value head on validation: P(win) log loss vs the base rate, final-blinds MAE vs the mean
    with torch.no_grad():
        model.eval()
        vs = []
        for i in range(0, len(va), 4096):
            b = data.batch(va[i:i + 4096])
            vs.append(model(b[0], b[1], b[2])[1].numpy())
        model.train()
    vs = np.concatenate(vs)
    pw = 1 / (1 + np.exp(-vs[:, 0]))
    yw, yb = data.won[va], data.blinds[va] * 24
    base = data.won[tr].mean()
    ll = lambda p: float(-np.mean(yw * np.log(np.clip(p, 1e-6, 1)) + (1 - yw) * np.log(np.clip(1 - p, 1e-6, 1))))
    vrep = {"win_logloss": ll(pw), "win_logloss_base": ll(np.full_like(pw, base)), "blinds_mae": float(np.abs(vs[:, 1] * 24 - yb).mean()),
            "blinds_mae_base": float(np.abs(data.blinds[tr].mean() * 24 - yb).mean())}
    print(f"value head (val records): P(win) log loss {vrep['win_logloss']:.3f} (base rate {vrep['win_logloss_base']:.3f}); "
          f"final blinds MAE {vrep['blinds_mae']:.2f} (mean {vrep['blinds_mae_base']:.2f})")
    act = data.act[va]
    hit = pred == act
    mlen = np.diff(ds["mask_indptr"])[va]
    unforced = mlen > 1
    uni, bpred = baselines(data, tr, va, fams, phases)
    bhit = bpred == act
    rep = {"games": len(seeds), "records": int(len(ds["action"])), "train_games": int(len(tr_games)), "val_games": int(is_val_game.sum()),
           "val_records": int(len(va)), "hidden": hidden, "val_ce": vce, "value": vrep}
    print()
    print(f"VALIDATION ({len(va)} records, {int(unforced.sum())} with >1 legal action)")
    print(f"  {'':34s} {'n':>7} {'clone':>7} {'uniform':>7} {'freq':>7}")
    def row(label, m):
        if m.sum() == 0:
            return
        print(f"  {label:34s} {int(m.sum()):7d} {hit[m].mean():7.3f} {uni[m].mean():7.3f} {bhit[m].mean():7.3f}")
        rep.setdefault("rows", {})[label] = [int(m.sum()), float(hit[m].mean()), float(uni[m].mean()), float(bhit[m].mean())]
    row("all", np.ones(len(va), bool))
    row("unforced", unforced)
    ph = ds["phase"][va]
    for p in np.unique(ph):
        row(f"phase {phases[p]} (unforced)", unforced & (ph == p))
    cf = fams[act]
    for f, _ in collections.Counter(cf[unforced].tolist()).most_common():
        row(f"chose {f} (unforced)", unforced & (cf == f))
    ante = ds["ante"][va]
    for an in np.unique(ante):
        row(f"ante {an} (unforced)", unforced & (ante == an))
    # disagreements: what v77 chose -> what the clone chose
    conf = collections.Counter((cf[k], fams[pred[k]]) for k in np.where(~hit)[0])
    print("  top disagreements (v77 chose -> clone chose):")
    for (x, y), n in conf.most_common(15):
        print(f"    {x:22s} -> {y:22s} {n}")
    rep["disagreements"] = [[x, y, n] for (x, y), n in conf.most_common(30)]

    if a.curve:
        print()
        print("LEARNING CURVE (val top-1, unforced records; same val set)")
        rng = np.random.default_rng(1)
        order = rng.permutation(tr_games)
        curve = []
        for n in [int(x) for x in a.curve.split(",")] + [len(tr_games)]:
            if n > len(tr_games):
                continue
            if n == len(tr_games):
                acc = float(hit[unforced].mean())
            else:
                m, _ = train(data, rows_of(order[:n]), va, hidden, a.epochs, a.lr, a.bs, a.seed, log=False)
                p2, _ = evaluate(m, data, va)
                acc = float((p2 == act)[unforced].mean())
            curve.append([n, acc])
            print(f"  {n:6d} training games: {acc:.3f}", flush=True)
        rep["curve"] = curve

    if a.check:
        rows = va[: a.check]
        with open(os.path.join(a.out, "check_obs.bin"), "wb") as f:
            f.write(struct.pack("<I", len(rows)))
            for r in rows:
                s, e = ds["obs_indptr"][r], ds["obs_indptr"][r + 1]
                f.write(struct.pack("<H", e - s))
                f.write(ds["obs_indices"][s:e].astype("<u2").tobytes())
                f.write(ds["obs_values"][s:e].astype("<f4").tobytes())
        outp = os.path.join(a.out, "check_out.bin")
        subprocess.run([a.rlenv, "netcheck", "--weights", wpath, "--obs", os.path.join(a.out, "check_obs.bin"), "--out", outp], check=True)
        raw = open(outp, "rb").read()
        n, na, nv = struct.unpack_from("<III", raw, 0)
        arr = np.frombuffer(raw, dtype="<f4", offset=12).reshape(n, na + nv)
        model.eval()
        with torch.no_grad():
            b = data.batch(rows)
            lg, v = model(b[0], b[1], b[2])
        dl = np.abs(arr[:, :na] - lg.numpy())
        dv = np.abs(arr[:, na:] - v.numpy())
        legal_d = dl[b[3].numpy()]
        print()
        print(f"RUST vs PYTORCH on {n} records: max |logit diff| {dl.max():.2e} (legal ids {legal_d.max():.2e}), max |value diff| {dv.max():.2e}; "
              f"argmax over legal agrees {(np.where(b[3].numpy(), arr[:, :na], -1e9).argmax(1) == lg.masked_fill(~b[3], -1e9).argmax(1).numpy()).mean():.4f}")
        rep["check"] = {"n": int(n), "max_logit_diff": float(dl.max()), "max_value_diff": float(dv.max())}
        os.remove(outp)
        os.remove(os.path.join(a.out, "check_obs.bin"))
    json.dump(rep, open(os.path.join(a.out, "report.json"), "w"), indent=1)
    print(f"\nwrote {wpath} ({os.path.getsize(wpath) / 1e6:.1f} MB), report.json; {time.time() - t0:.0f}s total")


if __name__ == "__main__":
    main()
