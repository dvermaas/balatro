# Learned policy checkpoints

Copies of what lives under `runs/_rl` (not in git), kept here so a lost container does not lose the training.
Schema: rlcore v2. The lab binary that plays these is jevlab built from commit `a3f70c0` (`runs/_rl/bin/jevlab-rl7`).

## ppo-a (run `rl-ppo-a`, `scripts/rl/train_rl.py`)

- `it61.brlw`: the policy after 58 policy iterations (46,400 games). c2k 0-449 without the round search: 205 wins
  (`JEVLAB_RLORDER=dagger JEVLAB_RLPOLICY=models/rl/ppo-a/it61.brlw`, `JEVLAB_EXP=no-round-search`).
- `ckpt.pt`: the run's resume point at iteration 73 (actor, critic, optimisers, curve).
- `init/`: the clone the run started from (`runs/_rl/bc/v4w5`); the run's config points at it.
- `curve.jsonl`: one row per iteration.

To resume elsewhere: copy `ckpt.pt` to `runs/_rl/ppo/rl-ppo-a/ckpt.pt`, `init/` to `runs/_rl/bc/v4w5/`, build jevlab
and copy it to `runs/_rl/bin/jevlab-rl7`, then `python scripts/rl/train_rl.py rl-ppo-a --hours H`. The checkpoint
stores absolute paths (`/home/user/balatro/...`), so use the same location or edit `cfg` in the checkpoint.

## ppo-item (run `rl-ppo-item`, item-level network, weights format 2)

Plays with jevlab built from commit `7efcbe1` or later (`runs/_rl/bin/jevlab-rl12`); older binaries cannot read
format 2.

- `it26.brlw`: the policy after 23 policy iterations (18,400 games). c2k 0-449 without the round search: 210 wins
  (`JEVLAB_RLORDER=dagger JEVLAB_RLPOLICY=models/rl/ppo-item/it26.brlw`, `JEVLAB_EXP=no-round-search`).
- `init/`: the network the run started from (`runs/_rl/item/mix`, `scripts/rl/train_item.py --labels mix`).
- `curve.jsonl`: one row per iteration, as of `it26`.

Started with: `python scripts/rl/train_rl.py rl-ppo-item --init runs/_rl/item/mix --bin runs/_rl/bin/jevlab-rl12
--games 800 --warmup 3 --eval-every 5 --eval-n 300 --lr 0.0002 --ent 0.003 --progress position --hours H`.
