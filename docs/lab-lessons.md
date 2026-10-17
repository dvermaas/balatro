# What the lab taught us (v27 → v31, 2026-10-16/17)

A cloud session ran about 40 policy-lab experiments (code only, no JEV, Jackdaw sim) on the
mid-game scaling problem. This page keeps the conclusions; the numbers are in
[policy-lab.md](policy-lab.md) (from "Mid-game scaling experiments"), one row per experiment in
[samples/cloud-experiments.json](samples/cloud-experiments.json), and every game in
[samples/cloud-lab/](samples/cloud-lab/).

## Where it got to

| | v27 | v31 |
|---|---:|---:|
| sim60-a mean ante (wins of 60) | 5.90 (4) | 6.50 (10) |
| 60 fresh seeds | 6.15 (6) | 6.63 (10) |
| 200 fresh seeds (v28 → v31) | 6.46 (27) | 6.68 (40) |

About a third of v31 runs reach ante 8. Most losses are antes 5-7 at ~75% of the target (not enough
scoring), plus the ante-8 finisher (Violet Vessel kills nearly every run that meets it).

## What worked: fixing what the forecast measured wrong

The shop forecast (play the next 3 bosses with the build frozen, compare blinds cleared against
buying nothing) is a good judge of whatever it is asked about. Every gain came from a place where it
measured the wrong thing or was never asked:

1. **Planets (v28, +0.31 ante, wins doubled).** `jokers.gain` kept each sample hand's play fixed, so a
   planet for a hand we did not play yet was worth 0, and small sure gains never cleared the 2 SE bar.
   v27 had gained ~3 levels by ante 6; v28 ~9.
2. **Verdant Leaf (v30, 27 → 29 wins of 200).** The bot never sold a joker to lift it, and Jackdaw never
   modelled the lift.
3. **The auto-buy bar (v31, +0.23 ± 0.10 over 320 seeds).** 2 paired SEs at 16-24 rollouts rejected
   real gains of +0.3 to +0.9 blinds. 1 SE is the optimum (1.5 and 0.5 are worse).

## What did not work, and why

- **Heuristic nudges on top of the forecast** (xMult or scaling credit, engine credit and protection,
  discard commitment to a hand, suit tarots by measured value, Mouth first plays, tag skips): flat or
  worse every time. Where the forecast and a rule of thumb disagree, the forecast is usually right for
  this bot. Example: engine credit made engines form 4x as often, but they did not score, because the
  bot does not play to exploit them.
- **More forecast compute** (32/48 rollouts, 4-boss horizon, joker-aware discards inside rollouts,
  forecasts that keep shopping): flat or worse. Better forecasts move every option alike.
- **Money rules** (more rerolls, late spending, interest guard, future-value vouchers): flat or worse.
  Money is not the bottleneck; v31 spends hard early and that is fine.
- **A learned value of shop states**: from ~700 games, summary features explain 6-17% of the outcome
  within an ante and joker identities overfit. At this data size the future is mostly luck.
- **The 5k-games dataset as training data** (money, jokers and hand levels at each ante's first shop):
  a win model trained on its 5k teacher games ranks our games about as well as one trained on our 712
  (AUC 0.57-0.63 vs 0.53-0.65 at antes 3-7); training on both is no better. Our own model does improve
  with more of our games (ante 4-6 AUC: 142 games 0.55, 284 games 0.60, 569 games 0.63), so more of our own
  games would help a little per doubling; the teacher's games add nothing on top.

## What the stronger bot does differently

The [5k-balatro-games](https://huggingface.co/datasets/makemake/5k-balatro-games) dataset (a heuristic
with an RNG oracle and search shops, Blue Deck, 74.6% wins) builds much like us through ante 5, but
commits to **Pair**: its highest-level hand at ante 6 is Pair in 77% of its wins (ours: spread over
Straight, Full House, Two Pair, Pair), with Pair-repeat jokers (Card Sharp, Supernova, Green Joker,
Half Joker) and more xMult late. Our own data agrees: Pair decks win ~30%, Flush decks ~11% and drift
into other hands after ante 5. Simple planet nudges toward Pair did not change outcomes, and neither
did a full commitment (below).

**Pair commitment (Rust lab, v32 Rust, sim60-a, `JEVLAB_EXP` in `native/bot/src/experiment.rs`).** Every
shop planet and pack pick goes to Pair (Mercury buys 0.17 -> 0.85 a game); Pair jokers (Jolly, Sly, The Duo,
Half Joker, Supernova, Card Sharp, Spare Trousers, Mad, Clever) clear the auto-buy bar 1 SE earlier and are
never sold (0.5 -> 1.7 held at death); a Pair within 85% of the best play is played. Baseline 6.50 (10 wins):
planets -0.08 +- 0.13 (8 wins), + jokers -0.23 +- 0.14 (8), + plays -0.08 +- 0.15 (8). Not confirmed on 200
seeds: no arm was ahead. The commitment happens and does not pay. Probable reasons (not tested): the
teacher's Pair wins come from its oracle and shop search finding the right Pair jokers, and our Pair decks
win more because good seeds deal pairs, not because Pair is the better plan. Rows: `samples/cloud-lab/pair-sim60.jsonl.gz`.

## How to experiment (method lessons)

- Pair every comparison by seed and keep the lab deterministic; confirm on 200+ seeds before shipping.
  Several +0.2 ante or +9 win bumps on 60 seeds vanished at 200-320.
- Count wins with "seeds won by only one side" (z = (a - b) / sqrt(a + b)), not raw totals.
- Read the logs before building: the three wins above were found by looking at what the bot did
  (levels by ante, a boss it never beat, items it never bought), not by tuning knobs.
- Prototype new rules as `BOT_ARMS` in `scripts/policy_lab.py` (Python, monkeypatched, no version
  change); port to production and to the Rust bot once they win.
- In the Rust lab, add a flag to `native/bot/src/experiment.rs` (`JEVLAB_EXP`, unset = production) and run
  `jevlab --jobs 1` in separate processes on a fixed seed split: threads share `readiness::play_mix`'s cache,
  so `--jobs N` games can differ between identical runs. ~17 CPU-s a game; screen on 60 seeds, confirm the
  arms that lead on 200 more.

## Open ideas

- A learned value at the forecast horizon, trained on far more games (hundreds of thousands, which the
  Rust lab makes feasible) and checked out of sample before use.
- A JEV benchmark of v31 on the real game, to see whether the lab gains carry over.
