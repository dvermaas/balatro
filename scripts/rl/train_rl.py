#!/usr/bin/env python3
"""PPO on the strategic decisions (blind select, shop, packs, round eval), starting from the behaviour clone.

    /home/user/rlvenv/bin/python scripts/rl/train_rl.py NAME --init runs/_rl/bc/v4 --bin runs/_rl/bin/jevlab-rl7 \
        [--hours H] [--games 800] [--warmup 3] [--eval-every 5] ...

Actors: a pinned jevlab in config (b) (v77 plays the rounds with JEVLAB_EXP=no-round-search; the network takes every
decision outside a round, JEVLAB_RLPOLICY, sampled at JEVLAB_RLTEMP=1 with a different JEVLAB_RLSEED per iteration)
logs every strategic decision (JEVLAB_RLLOG, rlcore schema v2) and the learner reads them back (read_rlb.py).

Seeds: fresh every iteration, a deterministic function of (NAME, iteration), never in b2k, c2k or e16k. Validation:
argmax play on e16k seeds 15000-15299 every --eval-every iterations (the loop never touches b2k or c2k).

Reward (`rewards`, the one place to change it): progress potential, blinds passed (beaten or skipped) / 24 as the
difference between consecutive decisions, the last decision collecting the run's final potential, plus WIN_BONUS
for a win; gamma 1, GAE(lambda). Room is left for more terms (late scoring power) there.

Critic: a separate small dense network on a few observation blocks (`Critic`: the full observation lets it memorise
games), predicting the return to go, never exported; the actor's gradients stay its own. It is warmed up first by --warmup critic-only iterations on the clone's own sampled games,
with the explained variance reported before any policy update (measured on each iteration's fresh games before
fitting on them, so out of sample). It fits Monte Carlo returns to go by default (--critic-target).

Guards: PPO clip, entropy bonus, a per-update KL stop (approximate KL past --target-kl ends the update's epochs), and an
optional KL-to-clone penalty (--clone-kl B, decaying by --clone-kl-decay per iteration; default 0, off: clip plus the KL
stop already bound each update, and an anchor to a clone that wins a quarter of its games caps what can be learned).

Operations: everything lives in runs/_rl/ppo/NAME/: ckpt.pt (config, iteration, actor, critic, optimisers and the
curve, one atomic file), curve.jsonl (rewritten from it), weights/itK.brlw (the clone it0, the current and every
evaluated policy), lab/ (jevlab's rows: itK sampled, evalK argmax). Resumable: rerun with the NAME alone; a killed
iteration is replayed from its start (same seeds, same sampling seed). Atomic writes (tmp + rename). --hours H stops cleanly between iterations when the next would not fit. Disk: the
trajectories of an iteration are deleted once it is learned from, and jevlab runs with JEVLAB_RMDIRS=1 (the loop also
removes that jevlab's /tmp/plab_*_<pid> dirs after it has exited). A binary under native/target is refused.
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time

import numpy as np
import torch
import torch.nn.functional as F

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import read_rlb as R  # noqa: E402
import train_bc as B  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SEED_DIR = os.path.join(ROOT, "docs/samples/cloud-lab")
CHARS = "123456789ABCDEFGHIJKLMNPQRSTUVWXYZ"
VAL_RANGE = (15000, 15300)
WIN_BONUS = 1.0


# ---------------------------------------------------------------- reward (swap here)

PROGRESS = "count"  # "count" (rl-ppo-a) or "position"; set from --progress


def game_rewards(ds, rows, gi):
    """The rewards of game gi's records `rows` (step order) under PROGRESS:
    count     blinds passed (beaten or skipped) / 24: what rl-ppo-a used. Hieroglyph / Petroglyph (ante -1) make
              more blinds to pass, so a run that buys them and still loses collects more than 24/24 (milestone 5).
    position  where the run stands, 3 (ante - 1) + blind index (capped 0..24) / 24: an ante shift moves it back,
              and a lost run ends where its last decision stood (the blind it died on); a win ends at 24."""
    if PROGRESS == "count":
        return rewards(ds["blinds_passed"][rows], ds["game_passed"][gi], ds["game_won"][gi])
    pos = np.clip(3 * (ds["ante"][rows].astype(np.int64) - 1) + ds["blind_index"][rows], 0, 24)
    won = bool(ds["game_won"][gi])
    return rewards(pos, 24 if won else pos[-1], won)


def rewards(passed, passed_final, won):
    """Per-decision rewards of one game. passed: blinds passed before each decision (records in step order);
    passed_final: blinds passed when the run ended (24 for a win). Return to go from decision t is then
    (passed_final - passed[t]) / 24 + WIN_BONUS * won. A late-scoring-power term would be added here."""
    phi = np.asarray(passed, dtype=np.float64) / 24.0
    nxt = np.append(phi[1:], passed_final / 24.0)
    r = nxt - phi
    r[-1] += WIN_BONUS * float(won)
    return r.astype(np.float32)


# ---------------------------------------------------------------- utilities

def atomic_write(path, data, mode="w"):
    tmp = f"{path}.tmp{os.getpid()}"
    with open(tmp, mode) as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def atomic_torch(obj, path):
    tmp = f"{path}.tmp{os.getpid()}"
    torch.save(obj, tmp)
    os.replace(tmp, path)


def reserved_seeds():
    s = set()
    for n in ("b2k", "c2k", "e16k"):
        s.update(open(os.path.join(SEED_DIR, f"seeds-{n}.txt")).read().split())
    return s


def fresh_seeds(name, it, n, reserved):
    out, k = [], 0
    while len(out) < n:
        h = hashlib.sha1(f"{name}:{it}:{k}".encode()).digest()
        k += 1
        s = "".join(CHARS[b % len(CHARS)] for b in h[:8])
        if s not in reserved and s not in out:
            out.append(s)
    return out


def val_seeds():
    return open(os.path.join(SEED_DIR, "seeds-e16k.txt")).read().split()[VAL_RANGE[0]:VAL_RANGE[1]]


def sampling_seed(name, it):
    return int.from_bytes(hashlib.sha1(f"{name}:rlseed:{it}".encode()).digest()[:4], "little")


def run_jevlab(a, out, name, seeds, weights, temp, rlseed, logdir):
    """One jevlab run (config (b)); returns (rows, wall seconds). Removes its /tmp/plab_*_<pid> after it exits."""
    env = dict(os.environ)
    env.update(JEVLAB_RLPOLICY=weights, JEVLAB_EXP=a.exp, JEVLAB_RMDIRS="1", JEVLAB_RLORDER=a.order)
    env.pop("JEVLAB_TRACE", None)
    env.pop("JEV_PERF", None)
    if temp is not None:
        env["JEVLAB_RLTEMP"] = str(temp)
        env["JEVLAB_RLSEED"] = str(rlseed)
    else:
        env.pop("JEVLAB_RLTEMP", None)
    if logdir:
        env["JEVLAB_RLLOG"] = logdir
    else:
        env.pop("JEVLAB_RLLOG", None)
    shutil.rmtree(os.path.join(out, name), ignore_errors=True)
    tmp = os.environ.get("TMPDIR", "/tmp")
    # run dirs a killed earlier attempt of this run left (its seeds, a jevlab that is no longer alive)
    want = set(seeds)
    for dname in os.listdir(tmp):
        parts = dname.split("_")
        if len(parts) == 3 and parts[0] == "plab" and parts[1] in want and parts[2].isdigit() and not os.path.exists(f"/proc/{parts[2]}"):
            shutil.rmtree(os.path.join(tmp, dname), ignore_errors=True)
    t0 = time.time()
    with open(os.path.join(out, f"{name}.stderr"), "w") as err:
        p = subprocess.Popen([a.bin, "--seeds", ",".join(seeds), "--arm", "F", "--jobs", str(a.jobs), "--out", out, "--name", name],
                             cwd=ROOT, env=env, stdout=subprocess.DEVNULL, stderr=err)
        rc = p.wait()
    wall = time.time() - t0
    suffix = f"_{p.pid}"
    for d in os.listdir(tmp):
        if d.startswith("plab_") and d.endswith(suffix):
            shutil.rmtree(os.path.join(tmp, d), ignore_errors=True)
    if rc != 0:
        raise SystemExit(f"jevlab exited {rc} (see {out}/{name}.stderr)")
    rows = [json.loads(l) for l in open(os.path.join(out, name, "results.jsonl")) if l.strip()]
    os.remove(os.path.join(out, f"{name}.stderr"))
    return rows, wall


def game_stats(rows):
    ok = [r for r in rows if "won" in r]
    n = max(1, len(ok))
    return dict(games=len(ok), errors=len(rows) - len(ok), win=sum(bool(r["won"]) for r in ok) / n,
                ante=sum(r["ante"] for r in ok) / n, blinds=sum(r["blinds"] for r in ok) / n,
                swaps=sum(r.get("rl_swaps", 0) for r in ok) / n)


# ---------------------------------------------------------------- learner

class Batch:
    """The records of one iteration, with old log-probs, values, advantages and returns."""

    def __init__(self, ds):
        self.ds = ds
        self.data = B.Data(ds)
        n = len(ds["action"])
        order = np.lexsort((ds["step"], ds["game"]))
        assert (order == np.arange(n)).all(), "records are in game, step order"
        self.ok = ds["action"] != R.UNEXPRESSIBLE  # joker rearranges (code's) carry no policy decision
        self.rows = np.nonzero(self.ok)[0]
        self.n_mask = np.diff(ds["mask_indptr"])

    def forward(self, model, rows, bs=8192):
        outs_l, outs_v = [], []
        with torch.no_grad():
            for i in range(0, len(rows), bs):
                b = self.data.batch(rows[i:i + bs])
                logits, v = model(b[0], b[1], b[2])
                ml = logits.masked_fill(~b[3], -1e9)
                outs_l.append(F.log_softmax(ml, 1).gather(1, b[4][:, None])[:, 0].numpy())
                outs_v.append(v[:, 0].numpy())
        return np.concatenate(outs_l), np.concatenate(outs_v)

    def targets(self, values, lam):
        """GAE(lambda), gamma 1, over each game's policy records; returns (advantages, lambda-returns, MC returns)."""
        ds = self.ds
        rows = self.rows
        g = ds["game"][rows]
        adv = np.zeros(len(rows), np.float32)
        ret_mc = np.zeros(len(rows), np.float32)
        starts = np.flatnonzero(np.r_[True, g[1:] != g[:-1]])
        ends = np.r_[starts[1:], len(rows)]
        for s, e in zip(starts, ends):
            k = g[s]
            r = game_rewards(ds, rows[s:e], k)
            v = values[s:e]
            vn = np.append(v[1:], 0.0)
            delta = r + vn - v
            a = 0.0
            for t in range(e - s - 1, -1, -1):
                a = delta[t] + lam * a
                adv[s + t] = a
            ret_mc[s:e] = np.cumsum(r[::-1])[::-1]
        return adv, adv + values, ret_mc


def explained_variance(pred, y):
    vy = float(np.var(y))
    return float("nan") if vy == 0 else 1.0 - float(np.var(y - pred)) / vy


CRITIC_BLOCKS = ("scalars", "hand_levels", "boss", "vouchers_owned", "deck")


class Critic(torch.nn.Module):
    """The value of a decision state from a few observation blocks (run scalars, hand levels, boss, vouchers, deck
    composition): a dense MLP. Measured on v77's 1,000 logged games, the full sparse observation lets a critic
    memorise games (validation explained variance below 0 while training EV passes 0.9); these blocks generalise
    (validation EV 0.14, about what blinds passed alone gives) and keep improving with the fresh games of each iteration."""

    def __init__(self, cols, hidden=64, n_extra=0):
        super().__init__()
        self.register_buffer("cols", torch.as_tensor(cols, dtype=torch.int64))
        self.n_extra = n_extra
        self.net = torch.nn.Sequential(torch.nn.Linear(len(cols) + n_extra, hidden), torch.nn.ReLU(), torch.nn.Linear(hidden, hidden), torch.nn.ReLU(),
                                       torch.nn.Linear(hidden, 1))

    def features(self, ds, rows, extra=None):
        import scipy.sparse as sp
        X = sp.csr_matrix((ds["obs_values"], ds["obs_indices"], ds["obs_indptr"]), shape=(len(ds["action"]), ds["obs_dim"]))
        X = X[rows][:, self.cols.numpy()].toarray().astype(np.float32)
        if self.n_extra:
            X = np.concatenate([X, extra.astype(np.float32)], 1)
        return torch.from_numpy(X)

    def forward(self, x):
        return self.net(x)[:, 0]


def critic_cols(schema):
    blocks = {b["name"]: b for b in schema["blocks"]}
    out = []
    for n in CRITIC_BLOCKS:
        b = blocks[n]
        out += range(b["offset"], b["offset"] + len(b["rows"]) * len(b["cols"]))
    return sorted(out)


def seed_features(ds, rows):
    """Per record, the per-seed baseline from the same iteration's other rollouts of its seed (milestone 5: the seed
    explains ~40% of the win variance): [has others, b = their mean total return - this rollout's reward so far,
    b2 = the same over the rollouts that passed at least as many blinds (else b)]. Generic in the reward
    (`game_rewards`); valid as a baseline: it does not depend on this rollout's actions."""
    g = ds["game"][rows]
    total = {}
    past = np.zeros(len(rows), np.float64)
    starts = np.flatnonzero(np.r_[True, g[1:] != g[:-1]])
    ends = np.r_[starts[1:], len(rows)]
    for s_, e in zip(starts, ends):
        r = game_rewards(ds, rows[s_:e], g[s_]).astype(np.float64)
        total[g[s_]] = r.sum()
        past[s_:e] = np.concatenate([[0.0], np.cumsum(r)[:-1]])
    by_seed = {}
    for gi in total:
        by_seed.setdefault(ds["game_seed"][gi], []).append(gi)
    p24 = ds["blinds_passed"][rows]
    out = np.zeros((len(rows), 3), np.float32)
    for i in range(len(rows)):
        oth = [h for h in by_seed[ds["game_seed"][g[i]]] if h != g[i]]
        if not oth:
            continue
        f = np.array([total[h] for h in oth])
        b = f.mean() - past[i]
        reach = ds["game_passed"][oth] >= p24[i]
        b2 = f[reach].mean() - past[i] if reach.any() else b
        out[i] = [1.0, b, b2]
    return out


def critic_values(critic, X):
    with torch.no_grad():
        return critic(X).numpy()


def fit_critic(critic, opt, X, targets, epochs, bs, rng):
    tl, nb = 0.0, 0
    y = torch.from_numpy(targets.astype(np.float32))
    for _ in range(epochs):
        perm = rng.permutation(len(y))
        for i in range(0, len(perm), bs):
            k = perm[i:i + bs]
            loss = F.mse_loss(critic(X[k]), y[k])
            opt.zero_grad()
            loss.backward()
            opt.step()
            tl += float(loss.detach())
            nb += 1
    return tl / max(1, nb)


def ppo_update(actor, clone, opt, batch, old_logp, adv, a, beta, rng):
    rows = batch.rows
    keep = batch.n_mask[rows] > 1  # forced records have no gradient
    idx = np.nonzero(keep)[0]
    advn = (adv - adv[idx].mean()) / (adv[idx].std() + 1e-8)
    stats = dict(pi_loss=0.0, entropy=0.0, kl=0.0, clipfrac=0.0, clone_kl=0.0, n=0)
    epochs_done, stopped = 0, False
    for ep in range(a.ppo_epochs):
        perm = rng.permutation(idx)
        kls = []
        for i in range(0, len(perm), a.bs):
            k = perm[i:i + a.bs]
            b = batch.data.batch(rows[k])
            logits, _ = actor(b[0], b[1], b[2])
            ml = logits.masked_fill(~b[3], -1e9)
            lp_all = F.log_softmax(ml, 1)
            lp = lp_all.gather(1, b[4][:, None])[:, 0]
            olp = torch.from_numpy(old_logp[k])
            ratio = torch.exp(lp - olp)
            A = torch.from_numpy(advn[k].astype(np.float32))
            pl = -torch.min(ratio * A, torch.clamp(ratio, 1 - a.clip, 1 + a.clip) * A).mean()
            p_all = lp_all.exp()
            ent = -(p_all * lp_all).sum(1).mean()
            loss = pl - a.ent * ent
            ck = torch.zeros(())
            if beta > 0:
                with torch.no_grad():
                    cl, _ = clone(b[0], b[1], b[2])
                    clp = F.log_softmax(cl.masked_fill(~b[3], -1e9), 1)
                ck = (p_all * (lp_all - clp)).sum(1).mean()
                loss = loss + beta * ck
            opt.zero_grad()
            loss.backward()
            torch.nn.utils.clip_grad_norm_(actor.parameters(), a.max_grad)
            opt.step()
            with torch.no_grad():
                kl = float((olp - lp).mean())
                kls.append(kl)
                stats["pi_loss"] += float(pl)
                stats["entropy"] += float(ent)
                stats["kl"] += kl
                stats["clipfrac"] += float(((ratio - 1).abs() > a.clip).float().mean())
                stats["clone_kl"] += float(ck)
                stats["n"] += 1
        epochs_done += 1
        if np.mean(kls) > a.target_kl:
            stopped = True
            break
    n = max(1, stats.pop("n"))
    out = {k: v / n for k, v in stats.items()}
    out.update(epochs=epochs_done, kl_stop=stopped)
    return out


def new_net(shape, hidden, n_value):
    return B.Net(shape["obs_dim"], shape["n_actions"], hidden, n_value=n_value)


CFG_KEYS = ("bin", "init", "games", "jobs", "warmup", "eval_every", "eval_n", "exp", "order", "lr", "critic_lr", "critic_epochs", "critic_target",
            "ppo_epochs", "bs", "clip", "ent", "target_kl", "lam", "max_grad", "clone_kl", "clone_kl_decay", "rollouts", "seed_baseline",
            "critic_buffer", "init_ckpt", "progress")
CFG_DEFAULTS = {"rollouts": 1, "seed_baseline": False, "critic_buffer": 1, "init_ckpt": None, "progress": "count"}  # runs from before milestone 5


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("name")
    ap.add_argument("--init", help="clone dir (model.pt, weights.brlw), for a new run")
    ap.add_argument("--bin", help="pinned jevlab binary (not under native/target), for a new run")
    ap.add_argument("--hours", type=float, default=1.5, help="stop between iterations before this much wall time")
    ap.add_argument("--iters", type=int, default=10 ** 9, help="stop once the run has this many iterations")
    ap.add_argument("--games", type=int, default=800, help="games per iteration")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--warmup", type=int, default=4, help="critic-only iterations first")
    ap.add_argument("--eval-every", type=int, default=5, help="argmax evaluation every K policy iterations (and after the warm-up)")
    ap.add_argument("--eval-n", type=int, default=300)
    ap.add_argument("--exp", default="no-round-search", help="JEVLAB_EXP of the actors")
    ap.add_argument("--order", default="dagger", help="JEVLAB_RLORDER of the actors")
    ap.add_argument("--lr", type=float, default=1e-4)
    ap.add_argument("--critic-lr", type=float, default=1e-3)
    ap.add_argument("--critic-epochs", type=int, default=1, help="passes over each iteration's (fresh) games")
    ap.add_argument("--critic-target", default="mc", help="mc (Monte Carlo return to go; gamma 1 makes it unbiased) or lambda (GAE returns)")
    ap.add_argument("--ppo-epochs", type=int, default=4)
    ap.add_argument("--bs", type=int, default=2048)
    ap.add_argument("--clip", type=float, default=0.2)
    ap.add_argument("--ent", type=float, default=0.01)
    ap.add_argument("--target-kl", type=float, default=0.02)
    ap.add_argument("--lam", type=float, default=0.95)
    ap.add_argument("--max-grad", type=float, default=1.0)
    ap.add_argument("--clone-kl", type=float, default=0.0)
    ap.add_argument("--clone-kl-decay", type=float, default=0.9)
    ap.add_argument("--rollouts", type=int, default=1, help="play each seed K times per iteration (games / K seeds), for the per-seed baseline")
    ap.add_argument("--seed-baseline", action="store_true", help="the critic also sees the per-seed baseline of the other rollouts (needs --rollouts > 1)")
    ap.add_argument("--critic-buffer", type=int, default=1, help="the critic trains on the last N iterations' games")
    ap.add_argument("--progress", default="count", help="reward potential: count (blinds passed, rl-ppo-a) or position (ante and blind)")
    ap.add_argument("--init-ckpt", help="a new run starts from this run's actor (ckpt.pt); fresh critic, warm-up as usual")
    a = ap.parse_args()

    d = os.path.join(ROOT, "runs/_rl/ppo", a.name)
    os.makedirs(os.path.join(d, "weights"), exist_ok=True)
    os.makedirs(os.path.join(d, "lab"), exist_ok=True)
    cp = os.path.join(d, "ckpt.pt")
    if os.path.exists(cp):
        ck = torch.load(cp, map_location="cpu", weights_only=False)
        cfg = ck["cfg"]
        for k in ("bin", "init"):
            if getattr(a, k) and os.path.abspath(getattr(a, k)) != cfg[k]:
                raise SystemExit(f"--{k} differs from the run's ({cfg[k]}); start a new NAME")
    else:
        if a.init_ckpt and not a.init:
            a.init = torch.load(a.init_ckpt, map_location="cpu", weights_only=False)["cfg"]["init"]
        if not (a.init and a.bin):
            raise SystemExit("a new run needs --init (or --init-ckpt) and --bin")
        if a.seed_baseline and a.rollouts < 2:
            raise SystemExit("--seed-baseline needs --rollouts 2 or more")
        cfg = {k: getattr(a, k) for k in CFG_KEYS}
        cfg["bin"] = os.path.abspath(a.bin)
        cfg["init"] = os.path.abspath(a.init)
        cfg["init_ckpt"] = os.path.abspath(a.init_ckpt) if a.init_ckpt else None
        ck = None
    cfg = {**CFG_DEFAULTS, **cfg}
    for k, v in cfg.items():
        setattr(a, k, v)
    global PROGRESS
    PROGRESS = a.progress
    if "/native/target/" in os.path.realpath(a.bin):
        raise SystemExit("copy the jevlab binary out of native/target first (runs/_rl/bin)")
    torch.set_num_threads(a.jobs)

    clone_sd = torch.load(os.path.join(a.init, "model.pt"), map_location="cpu")["state"]  # train_bc.py's checkpoint
    n_layers = len([k for k in clone_sd if k.startswith("layers.") and k.endswith(".weight")])
    hidden = [clone_sd["emb.weight"].shape[1]] + [clone_sd[f"layers.{i}.weight"].shape[0] for i in range(n_layers)]
    shape = {"obs_dim": clone_sd["emb.weight"].shape[0], "n_actions": clone_sd["pi.weight"].shape[0]}
    n_value = clone_sd["v.weight"].shape[0]
    with open(os.path.join(a.init, "weights.brlw"), "rb") as f:
        schema_hash = int.from_bytes(f.read(24)[8:16], "little")
    actor, clone = new_net(shape, hidden, n_value), new_net(shape, hidden, n_value)
    critic = None  # built from the first trajectories' schema (its blocks), or from the checkpoint
    clone.load_state_dict(clone_sd)
    opt_a = torch.optim.Adam(actor.parameters(), lr=a.lr)
    opt_c = None
    if ck:
        actor.load_state_dict(ck["actor"])
        critic = Critic(ck["critic"]["cols"].tolist(), n_extra=ck["critic"]["net.0.weight"].shape[1] - len(ck["critic"]["cols"]))
        critic.load_state_dict(ck["critic"])
        opt_c = torch.optim.AdamW(critic.parameters(), lr=a.critic_lr, weight_decay=1e-4)
        opt_a.load_state_dict(ck["opt_a"])
        opt_c.load_state_dict(ck["opt_c"])
        it0, rows_done = ck["iter"], ck["curve"]
    else:
        actor.load_state_dict(torch.load(a.init_ckpt, map_location="cpu", weights_only=False)["actor"] if a.init_ckpt else clone_sd)
        it0, rows_done = 0, []
    buf_dir = os.path.join(d, "buffer")
    os.makedirs(buf_dir, exist_ok=True)
    curve = os.path.join(d, "curve.jsonl")

    def export(name):
        p = os.path.join(d, "weights", name)
        B.export_weights(actor, p + ".tmp", schema_hash, shape["obs_dim"], shape["n_actions"])
        os.replace(p + ".tmp", p)
        return p

    reserved = reserved_seeds()
    t_start, last_s = time.time(), None
    it = it0
    while it < a.iters:
        if last_s is not None and time.time() - t_start + last_s > a.hours * 3600:
            print(f"stopping before iteration {it}: it would pass --hours {a.hours}", flush=True)
            break
        t_it = time.time()
        warm = it < a.warmup
        wpath = export(f"it{it}.brlw")
        traj = os.path.join(d, "traj")
        shutil.rmtree(traj, ignore_errors=True)
        K = a.rollouts
        seeds = fresh_seeds(a.name, it, a.games // K, reserved)
        rows, wall = [], 0.0
        for j in range(K):
            # K rollouts of the same seeds, each with its own sampling seed; logs renamed <seed>~r<j> into one dir
            sub = traj if K == 1 else os.path.join(d, f"traj-r{j}")
            shutil.rmtree(sub, ignore_errors=True) if K > 1 else None
            rr, w = run_jevlab(a, os.path.join(d, "lab"), f"it{it}" if K == 1 else f"it{it}r{j}", seeds, wpath, 1.0,
                               sampling_seed(a.name, it) + j, sub)
            rows += rr
            wall += w
            if K > 1:
                os.makedirs(traj, exist_ok=True)
                for fn in os.listdir(sub):
                    dst = fn.replace(".rlb.gz", f"~r{j}.rlb.gz") if fn.endswith(".rlb.gz") else fn
                    os.replace(os.path.join(sub, fn), os.path.join(traj, dst))
                shutil.rmtree(sub, ignore_errors=True)
        gs = game_stats(rows)
        ds = R.load_dir(traj)
        assert ds["schema_hash"] == schema_hash, "trajectories and weights disagree on the schema"
        batch = Batch(ds)
        if critic is None:
            torch.manual_seed(0)
            critic = Critic(critic_cols(ds["schema"]), n_extra=3 if a.seed_baseline else 0)
            opt_c = torch.optim.AdamW(critic.parameters(), lr=a.critic_lr, weight_decay=1e-4)
        X = critic.features(ds, batch.rows, seed_features(ds, batch.rows) if critic.n_extra else None)
        old_logp, _ = batch.forward(actor, batch.rows)
        v0 = critic_values(critic, X)
        adv, ret, ret_mc = batch.targets(v0, a.lam)
        ev = explained_variance(v0, ret_mc)  # out of sample: the critic has not seen these games
        not_in_mask = int((ds["flags"][batch.rows] & 1 == 0).sum())
        rng = np.random.default_rng(it)
        pst = {}
        if not warm:  # policy first (advantages from the critic before these games), then the critic
            beta = a.clone_kl * (a.clone_kl_decay ** (it - a.warmup))
            pst = ppo_update(actor, clone, opt_a, batch, old_logp, adv, a, beta, rng)
            pst["clone_kl_beta"] = beta
        tgt = ret_mc if a.critic_target == "mc" else ret
        # the critic's buffer: this iteration's games plus the last critic_buffer - 1 iterations' (buffer/itK.npz)
        np.savez(os.path.join(buf_dir, f"it{it}.npz.tmp.npz"), X=X.numpy().astype(np.float16), y=tgt.astype(np.float32))
        os.replace(os.path.join(buf_dir, f"it{it}.npz.tmp.npz"), os.path.join(buf_dir, f"it{it}.npz"))
        Xs, ys = [X], [tgt]
        for k in range(it - 1, it - a.critic_buffer, -1):
            p_ = os.path.join(buf_dir, f"it{k}.npz")
            if k >= 0 and os.path.exists(p_):
                z = np.load(p_)
                if z["X"].shape[1] == X.shape[1]:
                    Xs.append(torch.from_numpy(z["X"].astype(np.float32)))
                    ys.append(z["y"])
        vloss = fit_critic(critic, opt_c, torch.cat(Xs), np.concatenate(ys), a.critic_epochs, 1024, rng)
        v1 = critic_values(critic, X)
        row = dict(iter=it, phase="warmup" if warm else "ppo", **{f"sampled_{k}": v for k, v in gs.items()},
                   records=int(len(batch.rows)), not_in_mask=not_in_mask, ev=ev, ev_after_fit=explained_variance(v1, ret_mc),
                   value_loss=vloss, ret_mean=float(ret_mc.mean()), adv_std=float(adv.std()),
                   games_per_s=gs["games"] / wall, jevlab_s=wall, **pst)
        it += 1
        nxt = export(f"it{it}.brlw")
        if it == a.warmup or (it > a.warmup and (it - a.warmup) % a.eval_every == 0):
            erows, ewall = run_jevlab(a, os.path.join(d, "lab"), f"eval{it}", val_seeds()[:a.eval_n], nxt, None, 0, None)
            row.update({f"eval_{k}": v for k, v in game_stats(erows).items()}, eval_policy=f"it{it}", eval_s=ewall)
        row["seconds"] = time.time() - t_it
        rows_done = rows_done + [row]
        atomic_torch({"cfg": cfg, "iter": it, "curve": rows_done, "actor": actor.state_dict(), "critic": critic.state_dict(),
                      "opt_a": opt_a.state_dict(), "opt_c": opt_c.state_dict()}, cp)
        atomic_write(curve, "".join(json.dumps(r) + "\n" for r in rows_done))
        shutil.rmtree(traj, ignore_errors=True)
        for fn in os.listdir(buf_dir):  # bounded: only the last critic_buffer iterations
            k = fn[2:].split(".")[0]
            if k.isdigit() and int(k) < it - a.critic_buffer:  # it was advanced: keep it-1 .. it-N
                os.remove(os.path.join(buf_dir, fn))
        keep = {"it0.brlw", f"it{it}.brlw"} | {f"{r['eval_policy']}.brlw" for r in rows_done if "eval_policy" in r}
        for fn in os.listdir(os.path.join(d, "weights")):
            if fn.endswith(".brlw") and fn not in keep:
                os.remove(os.path.join(d, "weights", fn))
        last_s = row["seconds"]
        print(json.dumps({k: (round(v, 4) if isinstance(v, float) else v) for k, v in row.items()}), flush=True)


if __name__ == "__main__":
    main()
