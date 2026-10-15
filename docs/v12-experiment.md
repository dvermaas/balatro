# V12: measure the hand after the discard

Implementation: `d1c693b2`. Baseline: v11 `8cfd03bf`, commit `69a8d7b`.
Prepared on 2026-10-12 after the user confirmed v11 was finished. The panel independently
confirmed paused/MENU, no queued seeds, 30 finished runs, and the benchmark completion event.

## What v11 established

The fixed `bench30-a` results use the existing panel metric, **blinds cleared**. The current
analytics implementation does not actually include the fractional final-blind progress described
in its comment, so these figures should not be interpreted as fractional chip progress.

| Version | Finished | Wins | Mean blinds cleared | Standard error |
|---|---:|---:|---:|---:|
| v8 | 30 | 0 | 7.30 | 0.79 |
| v9 | 30 | 0 | 12.77 | 0.78 |
| v10 | 30 | 0 | 12.10 | 0.80 |
| v11 | 30 | 1 | 11.97 | 0.92 |

V11 minus v9 is **-0.80 blinds (paired standard error 0.675)** across 30 seeds: 9 better,
8 tied, 13 worse. V11 minus v10 is **-0.133 blinds (paired standard error 0.730)**:
9 better, 12 tied, 9 worse. These are descriptive comparisons from one run per seed,
not proof that either variant has a higher underlying win probability.

V11's first victory was `2RGEX6EN`, recorded in `runs/20261012-192347-2RGEX6EN`.
Its final observed lineup was Sly Joker, Negative Scholar, Raised Fist, Even Steven,
Ancient Joker, and Ramen. Twelve of the 29 losses occurred at a Big Blind; six runs
cleared fewer than six blinds. The average problem extends beyond boss handling.

## Lessons retained

- V3/v4's historical notes warn against letting early play frequency lock the build into Pair.
- V6's buy thresholds were too high for JEV's score distribution. V7 corrected that, but
  prompt changes alone did not produce the later improvement.
- V8 made score estimates useful. V9 added measured purchase value and draw information,
  improving average progress substantially.
- V10 spent early cash on rerolls/vouchers and sharply increased set-chasing. Its average
  tied v9 within the observed uncertainty. V11 restored the economy discipline but did not
  improve the average over v9.
- The coaching transcript emphasizes a reliable finish, useful draw objectives, and evaluating
  played versus held cards. Its eight-ante high-score context is qualified in
  [the distilled notes](coaching-tips-distilled.md).
- TypeSafe's [Jev 1.13 guidance](https://docs.typesafe.ai/model-jaggedness/jev-1.13)
  recommends doing arithmetic in code. Its [Score documentation](https://docs.typesafe.ai/primitives/score)
  also distinguishes a rating from a literal measured quantity.

## Hypothesis and implementation

V12 keeps v11's shop, reroll, voucher, tarot, and committed-build policies. It changes the
hand-decision evidence rather than retuning spending against these seeds.

1. **Score before pruning.** Candidate generation previously preferred playing extra junk before
   comparing score. It could eliminate a stronger short Half Joker play or discard a useful
   held Steel/Baron card as padding. Every subset is now scored first; extra junk breaks score
   ties. Psychic/Eye/Mouth restrictions are applied before pruning. A boss-forbidden fallback
   is valued at zero instead of being considered a sure clear.
2. **Evaluate the entire future hand.** Each discard plan samples 32 uniform draws without
   replacement within each draw, then scores every playable subset of the resulting hand.
   Small draw spaces (at most 32 combinations) are enumerated completely. The average includes
   misses and alternative hands; cards left held contribute to the score. It is not the old
   `chance to hit × score of a representative hit` proxy.
3. **Model the spent resource.** The next scoring snapshot has one fewer discard; Banner and
   Mystic Summit respond accordingly. Blue Joker, Ramen, and Green Joker values are adjusted.
   Serpent draws three, including after a one-card discard.
4. **Preserve a viable hand.** A new candidate keeps the current scoring core and individually
   useful held cards while cycling other cards. The complete combined discard is then evaluated
   by the same forecast, so interacting held effects can still make it unattractive.
5. **Give JEV the comparison.** Both the play-now question and discard question receive the
   modeled average, 20th-percentile outcome, and estimated chances of clearing or meeting pace.
   Code calculates the percentage change versus playing now. Draw plans are presented in measured
   order; the last hand prioritizes clear probability. JEV still makes the strategic choices,
   with build fit described as a tiebreaker. Existing pace gates remain, using the new average.

The draw pile is sorted by observable scoring properties before sampling. No card IDs, actual
draw positions, future draws, game mutation, or game RNG are used. The local fixed sampling seed
is independent of the game's seed. Identical physical cards retain their multiplicity.

## Offline validation

Run from the repository root:

```powershell
.venv/Scripts/python.exe -B -m unittest discover -s tests -v
.venv/Scripts/python.exe -B scripts/audit_hand_forecasts.py --limit 60
```

- **14 tests passed:** exact tiny-deck expectations including misses, duplicate-card probability,
  sample invariance under deck reordering, empty piles, held Steel, Half Joker, Psychic/Eye/Mouth,
  Serpent, spent-discard effects, snapshot immutability, and forecast delivery to the questions.
- Python compilation and whitespace checks passed.
- **60 recorded v11 states replayed without JEV or game calls.** No recorded state was modified.
  Among states that still required a question, no best-play estimate decreased; no increase was
  observed in this evenly spaced sample either. The earlier 549-state v9-v11 audit found four
  >10% improvements from exhaustive candidate scoring, so this fix is useful but relatively rare.
- The highest measured draw plan differed from v11's chosen discard in **9 of the 60 sampled
  states**. This measures disagreement, not counterfactual game success.
- Replay runtime: median **0.339 s**, 95th percentile **0.556 s** per decision on this machine.
  Certain larger Serpent hands may cost more.

Example: v11's first decision on `7X7WSDCX` kept a pair of aces. V12's forecast ranks the
four-spade flush chase first: modeled average next-play score 206, 20th percentile 80,
81% of sampled draws meeting pace. These are model estimates from unordered samples, not the
actual next draw or proof that changing this move improves the eventual run.

## Limits and live experiment

The scoring model remains incomplete (including some joker interactions and special hand
classifiers). Forecasts assume rank-sorted future hands with deterministic ties and predict only
the next play after one discard. They do not simulate the rest of the round, optimize card order,
or model every discard-triggered effect. Rare outs can be missed by 32 samples; zero sampled
successes is not proof of impossibility. The 20th percentile is not a guaranteed floor. JEV sees
that the numbers are estimates and still receives unmodeled-joker context.

Live validation uses the same Red Deck, White Stake, JEV model setting, panel delay, and unchanged
`bench30-a` seeds and early-stop rule. No edits to versioned source during that benchmark.
The automatic heartbeat remains paused at the user's request.

**Live result: completed, 30/30 terminal summaries**, checked on 2026-10-13 while
continuing the user's later v15 benchmark. V12 `d1c693b2` started at 20:04
Europe/Berlin on 2026-10-12; its first run is `20261012-200418-7X7WSDCX`.

| Version | Mean blinds cleared | Mean ante reached | Wins |
| --- | ---: | ---: | ---: |
| v9 | 12.77 | 4.77 | 0/30 |
| v11 | 11.97 | 4.50 | 0/30 |
| v12 | 13.90 | 5.17 | 0/30 |

Win counts above are audited. V11's originally reported victory was an ante-8
boss loss mislabeled by the game's terminal win flag: 14,497 / 100,000 chips on
`2RGEX6EN`. See `v15-benchmark-resume.md` for the evidence and preserved originals.
This correction does not change the cleared-blind comparisons below.

Using the existing panel's progress metric and pairing identical seeds, v12 minus
v11 is **+1.93 blinds, SE 0.93** (17 better, 5 tied, 8 worse). Against v9 it is
**+1.13 blinds, SE 0.79** (14 better, 8 tied, 8 worse). V12 improved the observed
average on this fixed set, but did not improve the observed win count. The v9
comparison is particularly uncertain; 30 seeds and repeated development against
the same set do not establish general superiority or isolate which v12 change
helped. These numbers describe the recorded runs, not a fresh held-out evaluation.
