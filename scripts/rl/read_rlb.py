#!/usr/bin/env python3
"""Load behaviour-cloning logs (`<seed>.rlb.gz`, written by jevlab under JEVLAB_RLLOG; format in
native/rlcore/src/log.rs) into numpy arrays, and print dataset stats.

    /home/user/rlvenv/bin/python scripts/rl/read_rlb.py runs/_rl/bc-v77/games [--limit N]

Also reads fast-env trajectories (`<seed>.brt.gz`, native/rlenv/src/traj.rs): then ds also has logp,
value_win, value_blinds, reward (per record) and game_power (per game: [(ante, log10 power samples)]).

As a module: `ds = load_dir(dir)` returns a dict of arrays (one row per record):
  obs_indptr, obs_indices, obs_values   CSR observation (dim = ds["obs_dim"]); obs_csr(ds) gives scipy CSR
  mask_indptr, mask_ids                 CSR legal vocabulary ids;  mask_dense(ds, rows) gives booleans
  action                                chosen vocabulary id (65535 = unexpressible)
  step, ante, phase, blind_index, round, money, blinds_cleared, blinds_passed (v2; -1 in v1), flags, atype, aindex, game
  game_seed, game_won, game_ante, game_blinds, game_round, game_counters, game_passed (v2)   (one entry per game)
v2 (BRL2) flags bit 128: a joker swap (replace_joker id; the record's action fields are the sale).
plus "schema" (the dir's schema.json).
"""
import argparse
import collections
import glob
import gzip
import json
import os
import struct
import sys

import numpy as np

MAGIC = b"BRL1"
MAGIC2 = b"BRL2"  # schema v2: + blinds_passed per record, passed_final in the footer
MAGIC_TRAJ = b"BRT1"  # fast-env trajectories (native/rlenv/src/traj.rs): + logp, value_win, value_blinds, reward
UNEXPRESSIBLE = 0xFFFF
REC_HEAD = struct.Struct("<IhBBHiHHBhB")  # step ante phase blind round money blinds_cleared action atype aindex ncards
REC_HEAD2 = struct.Struct("<IhBBHiHHHBhB")  # v2: blinds_passed after blinds_cleared


def read_game(path):
    raw = gzip.open(path, "rb").read()
    if raw[:4] not in (MAGIC, MAGIC2, MAGIC_TRAJ):
        raise ValueError(f"{path}: bad magic")
    traj = raw[:4] == MAGIC_TRAJ
    v2 = raw[:4] == MAGIC2
    rh = REC_HEAD2 if v2 else REC_HEAD
    version, shash, obs_dim, n_actions = struct.unpack_from("<IQII", raw, 4)
    p = 4 + 20
    (slen,) = struct.unpack_from("<H", raw, p)
    p += 2
    seed = raw[p:p + slen].decode()
    p += slen
    (n,) = struct.unpack_from("<I", raw, p)
    p += 4
    head = np.zeros((n, 12), dtype=np.int64)
    flags = np.zeros(n, dtype=np.uint8)
    cards, oi, ov, mk = [], [], [], []
    rl = np.zeros((n, 4), dtype=np.float32)
    for r in range(n):
        h = rh.unpack_from(raw, p)
        p += rh.size
        if not v2:
            h = h[:7] + (-1,) + h[7:]
        head[r] = h
        nc = h[11]
        cards.append(tuple(raw[p:p + nc]))
        p += nc
        flags[r] = raw[p]
        p += 1
        (nnz,) = struct.unpack_from("<H", raw, p)
        p += 2
        oi.append(np.frombuffer(raw, dtype="<u2", count=nnz, offset=p))
        p += 2 * nnz
        ov.append(np.frombuffer(raw, dtype="<f4", count=nnz, offset=p))
        p += 4 * nnz
        (nm,) = struct.unpack_from("<H", raw, p)
        p += 2
        mk.append(np.frombuffer(raw, dtype="<u2", count=nm, offset=p))
        p += 2 * nm
        if traj:
            rl[r] = struct.unpack_from("<4f", raw, p)
            p += 16
    won, ante, blinds, rnd, nctr = struct.unpack_from("<BhHHI", raw, p)
    p += struct.calcsize("<BhHHI")
    passed_final = -1
    if v2:  # passed_final sits before n_counters
        p -= 4
        passed_final, nctr = struct.unpack_from("<HI", raw, p)
        p += 6
    counters = {}
    for _ in range(nctr):
        (kl,) = struct.unpack_from("<H", raw, p)
        p += 2
        k = raw[p:p + kl].decode()
        p += kl
        (v,) = struct.unpack_from("<Q", raw, p)
        p += 8
        counters[k] = v
    power = []
    if traj:
        (npw,) = struct.unpack_from("<H", raw, p)
        p += 2
        for _ in range(npw):
            a, k = struct.unpack_from("<hH", raw, p)
            p += 4
            power.append((a, np.frombuffer(raw, dtype="<f4", count=k, offset=p).copy()))
            p += 4 * k
    if p != len(raw):
        raise ValueError(f"{path}: {len(raw) - p} trailing bytes")
    return dict(version=version, schema_hash=shash, obs_dim=obs_dim, n_actions=n_actions, seed=seed, head=head,
                flags=flags, cards=cards, obs_idx=oi, obs_val=ov, mask=mk, won=bool(won), ante=ante, blinds=blinds,
                round=rnd, counters=counters, traj=traj, rl=rl, power=power, passed_final=passed_final)


def load_dir(d, limit=None):
    files = sorted(glob.glob(os.path.join(d, "*.rlb.gz"))) or sorted(glob.glob(os.path.join(d, "*.brt.gz")))
    if limit:
        files = files[:limit]
    if not files:
        raise SystemExit(f"no *.rlb.gz / *.brt.gz under {d}")
    games = [read_game(f) for f in files]
    for g, f in zip(games, files):
        g["file"] = os.path.basename(f)
    g0 = games[0]
    for g in games:
        if (g["version"], g["schema_hash"], g["obs_dim"], g["n_actions"]) != (g0["version"], g0["schema_hash"], g0["obs_dim"], g0["n_actions"]):
            raise SystemExit(f"schema mismatch: {g['seed']} vs {g0['seed']}")
    schema = None
    sp = os.path.join(d, "schema.json")
    if os.path.exists(sp):
        schema = json.load(open(sp))
        assert schema["obs_dim"] == g0["obs_dim"] and schema["n_actions"] == g0["n_actions"], "schema.json disagrees with the files"
    head = np.concatenate([g["head"] for g in games]) if games else np.zeros((0, 12), np.int64)
    oi = [a for g in games for a in g["obs_idx"]]
    ov = [a for g in games for a in g["obs_val"]]
    mk = [a for g in games for a in g["mask"]]
    lens = np.array([len(a) for a in oi], dtype=np.int64)
    mlens = np.array([len(a) for a in mk], dtype=np.int64)
    cols = ["step", "ante", "phase", "blind_index", "round", "money", "blinds_cleared", "blinds_passed", "action", "atype", "aindex", "ncards"]
    ds = {c: head[:, i] for i, c in enumerate(cols)}
    ds.update(
        obs_dim=g0["obs_dim"], n_actions=g0["n_actions"], schema_version=g0["version"], schema_hash=g0["schema_hash"], schema=schema,
        obs_indptr=np.concatenate([[0], np.cumsum(lens)]),
        obs_indices=np.concatenate(oi).astype(np.int32) if oi else np.zeros(0, np.int32),
        obs_values=np.concatenate(ov).astype(np.float32) if ov else np.zeros(0, np.float32),
        mask_indptr=np.concatenate([[0], np.cumsum(mlens)]),
        mask_ids=np.concatenate(mk).astype(np.int32) if mk else np.zeros(0, np.int32),
        flags=np.concatenate([g["flags"] for g in games]),
        cards=[c for g in games for c in g["cards"]],
        game=np.concatenate([np.full(len(g["head"]), k) for k, g in enumerate(games)]),
        game_seed=[g["seed"] for g in games], game_file=[g["file"] for g in games], game_won=np.array([g["won"] for g in games]),
        game_ante=np.array([g["ante"] for g in games]), game_blinds=np.array([g["blinds"] for g in games]),
        game_round=np.array([g["round"] for g in games]), game_counters=[g["counters"] for g in games],
        game_passed=np.array([g["passed_final"] for g in games]),
    )
    if g0["traj"]:
        rl = np.concatenate([g["rl"] for g in games])
        ds.update(logp=rl[:, 0], value_win=rl[:, 1], value_blinds=rl[:, 2], reward=rl[:, 3], game_power=[g["power"] for g in games])
    return ds


def obs_csr(ds):
    import scipy.sparse as sp
    n = len(ds["obs_indptr"]) - 1
    return sp.csr_matrix((ds["obs_values"], ds["obs_indices"], ds["obs_indptr"]), shape=(n, ds["obs_dim"]))


def mask_dense(ds, rows=None):
    rows = range(len(ds["mask_indptr"]) - 1) if rows is None else rows
    rows = list(rows)
    m = np.zeros((len(rows), ds["n_actions"]), dtype=bool)
    for k, r in enumerate(rows):
        m[k, ds["mask_ids"][ds["mask_indptr"][r]:ds["mask_indptr"][r + 1]]] = True
    return m


def stats(ds):
    sch = ds["schema"] or {}
    names = (sch.get("vocab") or {}).get("names")
    fams = (sch.get("vocab") or {}).get("families") or []
    phases = sch.get("phases") or [str(i) for i in range(7)]
    ng, n = len(ds["game_seed"]), len(ds["action"])
    print(f"schema v{ds['schema_version']} hash {ds['schema_hash']:016x}  obs_dim {ds['obs_dim']}  n_actions {ds['n_actions']}")
    won = ds["game_won"]
    print(f"games {ng}  won {won.sum()} ({100 * won.mean():.1f}%)  mean ante {ds['game_ante'].mean():.2f}  mean blinds {ds['game_blinds'].mean():.2f}")
    per = np.bincount(ds["game"], minlength=ng)
    print(f"records {n}  per game mean {per.mean():.1f} min {per.min()} max {per.max()}")
    nnz = np.diff(ds["obs_indptr"])
    print(f"obs nnz per record mean {nnz.mean():.1f} max {nnz.max()}")
    ml = np.diff(ds["mask_indptr"])
    print(f"legal-mask size mean {ml.mean():.2f} median {np.median(ml):.0f} max {ml.max()}  (records with 1 legal action: {(ml == 1).sum()})")
    ph = collections.Counter(phases[p] for p in ds["phase"])
    print("records by phase:", dict(ph.most_common()))

    def fam(a):
        if a == UNEXPRESSIBLE:
            return "UNEXPRESSIBLE"
        if names:
            return names[a].split(":")[0]
        return str(a)
    h = collections.Counter(fam(a) for a in ds["action"])
    print("chosen action type histogram:")
    for k, v in h.most_common():
        print(f"  {k:22s} {v:8d}  {100 * v / n:5.1f}%")
    unexp = int((ds["action"] == UNEXPRESSIBLE).sum())
    f = ds["flags"]
    exp = ds["action"] != UNEXPRESSIBLE
    print(f"unexpressible records {unexp}")
    print(f"round trip: chosen id not in mask {int((exp & ((f & 1) == 0)).sum())}, identity mismatch {int((exp & ((f & 2) == 0)).sum())}, "
          f"resolved to another slot of the same item {int((exp & ((f & 2) != 0) & ((f & 4) == 0)).sum())}, rumbot-illegal actions {int(((f & 64) == 0).sum())}")
    tg = (f & 8) != 0
    print(f"actions with card targets {int(tg.sum())}: hint kept {int((tg & ((f & 16) != 0)).sum())}, default targets equal the bot's {int((tg & ((f & 32) != 0)).sum())}")
    tot = collections.Counter()
    for c in ds["game_counters"]:
        tot.update(c)
    print("counters (summed over games):")
    for k in sorted(tot):
        if k.startswith(("unexpressible", "in_round", "roundtrip", "not_in", "action_illegal", "steps", "log_us")):
            print(f"  {k:40s} {tot[k]}")
    if names and n:
        top = collections.Counter(names[a] for a in ds["action"] if a != UNEXPRESSIBLE).most_common(15)
        print("top chosen actions:", ", ".join(f"{k} {v}" for k, v in top))
    if fams:
        print("vocabulary families:", ", ".join(f"{x['name']}[{x['len']}]" for x in fams))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dir")
    ap.add_argument("--limit", type=int)
    a = ap.parse_args()
    ds = load_dir(a.dir, a.limit)
    stats(ds)
    return 0


if __name__ == "__main__":
    sys.exit(main())
