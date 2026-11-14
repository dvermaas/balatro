#!/usr/bin/env python3
"""policy_stats.py WEIGHTS_MODEL_CKPT GAMES_DIR [GAMES_DIR...]: how random a sampled policy is, on its own logs.

WEIGHTS_MODEL_CKPT: a train_rl ckpt.pt (its actor) or a train_bc model.pt. For every unforced record: the entropy of
the policy's distribution over the legal actions and the probability it gave the action that was sampled. Prints,
by phase and by the family of the sampled action: records, mean entropy (nats), mean probability of the argmax
action, share of sampled actions the policy gave under 5% (and under 1%), and share where the sample was not the argmax.
"""
import collections
import json
import os
import sys

import numpy as np
import torch
import torch.nn.functional as F

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import read_rlb as R  # noqa: E402
import train_bc as B  # noqa: E402

PHASES = {0: "blind_select", 2: "round_eval", 3: "shop", 4: "pack"}


def load_actor(path, ds):
    ck = torch.load(path, map_location="cpu", weights_only=False)
    sd = ck["actor"] if "actor" in ck else ck["state"]
    n_layers = len([k for k in sd if k.startswith("layers.") and k.endswith(".weight")])
    hidden = [sd["emb.weight"].shape[1]] + [sd[f"layers.{i}.weight"].shape[0] for i in range(n_layers)]
    net = B.Net(ds["obs_dim"], ds["n_actions"], hidden, n_value=sd["v.weight"].shape[0])
    net.load_state_dict(sd)
    net.eval()
    return net


def main():
    path, dirs = sys.argv[1], sys.argv[2:]
    torch.set_num_threads(4)
    stats = collections.defaultdict(lambda: np.zeros(7))
    names = None
    for d in dirs:
        ds = R.load_dir(d)
        names = names or ds["schema"]["vocab"]["names"]
        net = load_actor(path, ds)
        data = B.Data(ds)
        rows = np.nonzero((ds["action"] != R.UNEXPRESSIBLE) & (np.diff(ds["mask_indptr"]) > 1))[0]
        with torch.no_grad():
            for i in range(0, len(rows), 8192):
                r = rows[i:i + 8192]
                b = data.batch(r)
                logits, _ = net(b[0], b[1], b[2])
                lp = F.log_softmax(logits.masked_fill(~b[3], -1e9), 1)
                p = lp.exp()
                ent = -(p * lp).sum(1).numpy()
                pa = p.gather(1, b[4][:, None])[:, 0].numpy()
                pmax = p.max(1).values.numpy()
                amax = p.argmax(1).numpy()
                for j, rr in enumerate(r):
                    fam = names[ds["action"][rr]].split(":")[0]
                    for key in ("all", "phase " + PHASES.get(int(ds["phase"][rr]), "?"), "sampled " + fam):
                        s = stats[key]
                        s += [1, ent[j], pmax[j], pa[j] < 0.05, pa[j] < 0.01, amax[j] != ds["action"][rr], pa[j]]
    print(f"{'':32s} {'records':>8} {'entropy':>8} {'p(argmax)':>9} {'p<5%':>6} {'p<1%':>6} {'not argmax':>10} {'p(sampled)':>10}")
    for k in sorted(stats, key=lambda k: (not k == "all", not k.startswith("phase"), -stats[k][0])):
        s = stats[k]
        n = s[0]
        if n < 200:
            continue
        print(f"{k:32s} {int(n):8d} {s[1] / n:8.3f} {s[2] / n:9.3f} {s[3] / n:6.3f} {s[4] / n:6.3f} {s[5] / n:10.3f} {s[6] / n:10.3f}")


if __name__ == "__main__":
    main()
