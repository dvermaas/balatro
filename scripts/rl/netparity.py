#!/usr/bin/env python3
"""Rust/PyTorch parity of a policy network (flat or item, scripts/rl/nets.py) on logged records.

    /home/user/rlvenv/bin/python scripts/rl/netparity.py MODEL WEIGHTS LOGDIR [--n 2000] [--rlenv native/target/release/rlenv]

MODEL: a model.pt ("state") or train_rl.py ckpt.pt ("actor"); WEIGHTS: its exported .brlw; LOGDIR: *.rlb.gz logs.
Takes the first N records with a joker-acquiring legal id first (the item scorer's), then others, runs
`rlenv netcheck --masked 1` and compares the logits on the legal ids (max abs difference) and the argmax over them.
Exits 1 unless max |diff| < 1e-4 and every argmax agrees.
"""
import argparse
import os
import struct
import subprocess
import sys
import tempfile

import numpy as np
import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import nets as N  # noqa: E402
import read_rlb as R  # noqa: E402
import train_bc as B  # noqa: E402

ap = argparse.ArgumentParser()
ap.add_argument("model")
ap.add_argument("weights")
ap.add_argument("logdir")
ap.add_argument("--n", type=int, default=2000)
ap.add_argument("--limit", type=int, default=300, help="games to read")
ap.add_argument("--rlenv", default="native/target/release/rlenv")
a = ap.parse_args()
ck = torch.load(a.model, map_location="cpu", weights_only=False)
model = N.net_from_state(ck.get("actor", ck.get("state")))
model.eval()
ds = R.load_dir(a.logdir, a.limit)
data = B.Data(ds)
fam = ds["schema"]["vocab"]["families"]
item_ids = set()
for f in fam:
    if f["name"] in ("buy_joker", "replace_joker"):
        item_ids.update(range(f["start"], f["start"] + f["len"]))
n_all = len(ds["action"])
has_item = np.array([bool(set(ds["mask_ids"][ds["mask_indptr"][r]:ds["mask_indptr"][r + 1]].tolist()) & item_ids) for r in range(n_all)])
rows = np.concatenate([np.where(has_item)[0], np.where(~has_item)[0]])[: a.n]
rows = np.sort(rows)
with tempfile.TemporaryDirectory() as td:
    ip, op = os.path.join(td, "obs.bin"), os.path.join(td, "out.bin")
    with open(ip, "wb") as f:
        f.write(struct.pack("<I", len(rows)))
        for r in rows:
            s, e = ds["obs_indptr"][r], ds["obs_indptr"][r + 1]
            f.write(struct.pack("<H", e - s))
            f.write(ds["obs_indices"][s:e].astype("<u2").tobytes())
            f.write(ds["obs_values"][s:e].astype("<f4").tobytes())
            m = ds["mask_ids"][ds["mask_indptr"][r]:ds["mask_indptr"][r + 1]]
            f.write(struct.pack("<H", len(m)))
            f.write(m.astype("<u2").tobytes())
    subprocess.run([a.rlenv, "netcheck", "--weights", a.weights, "--obs", ip, "--out", op, "--masked", "1"], check=True)
    raw = open(op, "rb").read()
n, na, nv = struct.unpack_from("<III", raw, 0)
arr = np.frombuffer(raw, dtype="<f4", offset=12).reshape(n, na + nv)[:, :na]
with torch.no_grad():
    b = data.batch(rows)
    lg, _ = model(b[0], b[1], b[2], b[3])
mask = b[3].numpy()
lg = lg.numpy()
d = np.abs(arr - lg)[mask]
am_r = np.where(mask, arr, -np.inf).argmax(1)
am_p = np.where(mask, lg, -np.inf).argmax(1)
agree = (am_r == am_p).mean()
print(f"parity: {n} records ({int(has_item[rows].sum())} with a joker-acquiring legal id), max |logit diff| on legal ids {d.max():.2e}, "
      f"argmax agrees {agree:.4f} ({int((am_r != am_p).sum())} differ)")
sys.exit(0 if d.max() < 1e-4 and agree == 1.0 else 1)
