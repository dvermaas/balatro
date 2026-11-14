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
