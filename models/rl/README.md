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

- `it40.brlw`: the policy after 37 policy iterations (29,600 games). c2k 0-449 without the round search: 224 wins
  (`JEVLAB_RLORDER=dagger JEVLAB_RLPOLICY=models/rl/ppo-item/it40.brlw`, `JEVLAB_EXP=no-round-search`); iteration 26
  won 210 (in git history, `f75f451`).
- `init/`: the network the run started from (`runs/_rl/item/mix`, `scripts/rl/train_item.py --labels mix`).
- `ckpt.pt`: the run's resume point at iteration 55 (41,600 games; iteration 55 won 214, no better than 40).
- `curve.jsonl`: one row per iteration, as of iteration 55.

Started with: `python scripts/rl/train_rl.py rl-ppo-item --init runs/_rl/item/mix --bin runs/_rl/bin/jevlab-rl12
--games 800 --warmup 3 --eval-every 5 --eval-n 300 --lr 0.0002 --ent 0.003 --progress position --hours H`.

## ppo-early (runs `rl-ppo-early`, `rl-ppo-early2`: antes 1-4 only, v77 plays the rest)

Curves only (`curve-early.jsonl`, `curve-early2.jsonl`, one row per iteration). The trained policy won exactly as many
games as the one it started from (`ppo-item/ckpt.pt`, iteration 55): 530 against 530 on c2k 0-899 with v77 from
ante 5 (docs/lab-lessons.md, "PPO on antes 1-4"), so its weights are not kept.

Started with: `python scripts/rl/train_rl.py rl-ppo-early --init-ckpt runs/_rl/ppo/rl-ppo-item/ckpt.pt --init
runs/_rl/item/mix --bin runs/_rl/bin/jevlab-rl13 --antes 1-4 --games 500 --warmup 1 --eval-every 100 --lr 0.0001
--ent 0.003 --progress position --hours H`; `rl-ppo-early2` the same from `rl-ppo-early`'s checkpoint with `--lr 0.0003`.

## rl-adv-late, rl-adv-late2: the network may overrule v77 in antes 5-8 (`train_rl.py --advise`, `scripts/rl/advchunk.sh`)

v77 (with the round search) plays every game; in antes 5-8 each of its actions outside a round is probed and the
network (PPO it40's item network) picks among the legal actions with +6 on v77's logit (about 5 overrules a game at
first). Validation (200 e16k seeds, `seeds-val200.txt`): v77 wins 123 (61.5%); the untrained network at argmax 109.
- `rl-adv-late`: lr 1e-4 barely moved the policy (KL ~0.0005 an update); raised to 5e-4 from iteration 6, the
  overrules rose (5.5 -> 7.6 a game) and the sampled wins fell (56% -> 47% by iteration 9). Most likely the entropy
  bonus (0.01): over ~70 near-certain decisions a game it steadily rewards overruling v77, while the advantage signal
  is noisy. Stopped at iteration 10.
- `rl-adv-late2`: restarted from rl-adv-late's iteration 2 (warmed critic, untrained actor) with no entropy bonus and
  lr 3e-4.
- `rl-adv-late2` with `--anchor` (a -log p(v77's action) loss term; 0.5 from iteration 4, 0.2 from 8): the drift
  stopped (overrules 5.8 -> ~1.1 a game in sampled play, sampled wins 54-60%), and so did the learning (KL ~0.001 an
  update). Validation at iteration 12 (argmax): 123 of 200, the same as v77, 197 of 200 games with the same outcome,
  0.075 overrules a game. After ~5,500 games the network holds no preference strong enough to beat the +6 on v77's
  action: it plays v77. Stopped there: one win/loss bit a game over ~70 decisions, under v77's own 28% luck, is too
  little signal for this budget (log, "The network may overrule v77").
