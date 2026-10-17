# Policy lab: shop valuation at scale, no JEV

2026-10-16. `scripts/policy_lab.py` plays full games (ante 8 or death) on the Jackdaw sim with
code-only shop policies. Data: [samples/policy-lab.json](samples/policy-lab.json). Raw results:
`runs/_policy_lab/main/results.jsonl` (gitignored).

## Why

The JEV sim benchmark of v19 against v18 came out at -0.03 ± 0.44 ante. Repeats of one seed ranged
from ante 3 to ante 8, so 30 JEV games cannot detect a realistic difference. The lab removes JEV:
every arm uses the same fixed play policy on the same 600 fresh seeds (not sim30-a), and only the
joker and voucher valuation differs.

## Setup

- **Play/discard policy (all arms):** `rollout.Policy`, the forecast's fast code policy. It plays
  spare low cards alongside a hand when no discards are left (The Water). Blinds are never skipped.
- **Shared rule (all arms):** buy the shop planet for the most played hand when affordable (keeping
  $4) and use planets immediately. Skip packs, tarots, spectrals and playing cards.
- **Arms:**

  | arm | rule |
  |---|---|
  | A `gain` | v18's code thresholds on `jokers.gain` over the last 10 plays. Gain ≥ `SURE_GAIN` (0.5) buys. Gain ≥ 0.15 buys if $25 remains after (`INTEREST_BUFFER`). Any positive gain buys in the early joker phase (ante ≤ 3, fewer than 4 jokers). Swaps need gain ≥ 0.5. v18's "strong" vouchers are bought from surplus above $25. Rerolls (max 2 per shop) come from surplus above $25. |
  | B `forecast` | v19's guard rails: `rollout.forecast(n=16, bosses=2)` over joker buys, sell-swaps of the 2 weakest jokers, and vouchers. It buys the option with the best paired blinds-cleared gain over skip, if that gain is more than 2 SE. It rerolls (max 2) when nothing beats skip by 1 SE and $25 remains. |
  | C `forecast-long` | B with `bosses=3` |
  | D `none` | never buys jokers or vouchers (floor) |

  JEV's item scores are not reproducible without JEV, so A approximates v18's money discipline and
  gain thresholds.
- **Seeds:** a pilot of 50 seeds gave a paired SD of about 2.0 ante for B−A. Detecting 0.25 ante
  (about 2.8 SE) then needs about 500 seeds, so the main run used 600.

**Speed** (15 worker processes):
- A and D take about 0.1 s per game.
- B takes 26 s per game (30 forecasts at 0.87 s each, n=16).
- C takes 35 s per game (1.1 s per forecast).
- The main run (2,400 games) took about 50 minutes of wall time. A further 300-game diagnostic took
  about 12 minutes.

## Results (600 seeds, all arms on every seed)

| arm | mean ante (SE) | ante-8 clear | mean blinds cleared | rerolls / $ on rerolls | buys (swaps, vouchers) |
|---|---:|---:|---:|---:|---:|
| A gain | 4.00 (0.08) | 1.2% | 10.4 | 8.2 / $44 | 5.4 (0.8, 0.7) |
| B forecast | 4.70 (0.09) | 4.0% | 13.3 | 11.5 / $62 | 6.6 (2.0, 0.6) |
| C forecast-long | **4.90** (0.09) | **6.0%** | **14.1** | 12.3 / $66 | 7.2 (2.3, 0.7) |
| D none | 1.74 (0.02) | 0% | 3.2 | 0 | 0 |

Paired differences (same seeds):

| | ante | blinds cleared | ante-8 clear |
|---|---:|---:|---:|
| B − A | **+0.70 ± 0.08** | +2.86 ± 0.27 | +2.8 ± 0.8 pts |
| C − A | **+0.91 ± 0.08** | +3.63 ± 0.26 | +4.8 ± 1.0 pts |
| B − D | +2.96 ± 0.09 | +10.1 ± 0.27 | +4.0 ± 0.8 pts |
| C − B | **+0.21 ± 0.05** | +0.77 ± 0.17 | +2.0 ± 0.9 pts |
| A − D | +2.26 ± 0.08 | +7.2 ± 0.24 | +1.2 ± 0.4 pts |

Death ante, 0 to 8 then won (ante 0 is a Hieroglyph run that died at ante 0):

| arm | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | won |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| A | | 81 | 106 | 51 | 79 | 139 | 87 | 37 | 13 | 7 |
| B | 1 | 61 | 78 | 41 | 50 | 119 | 123 | 71 | 32 | 24 |
| C | 1 | 59 | 68 | 41 | 41 | 112 | 120 | 84 | 38 | 36 |

Hieroglyph lowers the ante counter, and the forecast arms bought it in about 20% of games. This
makes mean ante understate B's and C's progress slightly. Blinds cleared and the clear rate are
unaffected, and they agree.

## Reading

- **The forecast beats `jokers.gain` clearly**, at about 8 SE. It wins at every depth: fewer
  ante 0-2 deaths (140 against 187 for A) and many more runs reaching ante 7-8 or winning (127
  against 57). It buys earlier-paying flat Mult (Half Joker, Mystic Summit, Misprint, Abstract,
  Smiley), Grabber and Hieroglyph, and swaps more. Gain buys more suit/hand-conditional jokers and
  Blue Joker.
- **A longer horizon helps** (C−B +0.21 ± 0.05 ante, +2 pts clear rate). Two bosses is somewhat
  biased toward immediate power; three bosses gives scaling jokers and economy more time to pay off.
- **Rerolls are worth their cost.** This was the 300-seed diagnostic, C without rerolls:
  `forecast-long-noreroll − forecast-long` = **-0.62 ± 0.07 ante** (-2.0 ± 0.24 blinds). It is
  still +0.34 ± 0.11 ante over A. The extra reroll spending in B and C, about $20 a game over A,
  buys better shops rather than wasting money. My earlier concern about the v19 game's 17 rerolls
  does not hold in the lab.
- B was better than A, so the "diagnose and fix" step was not needed. The reroll and horizon
  diagnostics above show which parts of B carry the gain.

## Caveats

- **Home advantage:** the forecast simulates exactly the lab's play policy, so B and C get an
  advantage. JEV plays differently, so its value may shrink with JEV at the table.
- **Arm A is an approximation:** v18's real shop decisions came from JEV item scores plus these
  thresholds.
- **The lab policy is weak** (only 1-6% clear ante 8), so absolute numbers are low. The comparison
  is the point.
- The JEV v19 benchmark result (≈ v18) is not contradicted: 30 JEV games had an SE of 0.44 ante,
  wider than the whole effect measured here.

## Recommendation

Give JEV, and the code around it, the **forecast with a 3-boss horizon**. Let the code act on it,
with JEV judging only what the forecast can't see.
1. Set `FORECAST_BOSSES = 3` (C−B +0.21 ± 0.05). n=16 to 24 within the 4 s budget is enough (1.1 s
   per call at n=16 here).
2. Keep the rule to reroll when nothing beats skip by 1 SE and money stays at or above $25: worth
   +0.62 ante.
3. Buy the best option that beats skip by more than 2 SE **in code**, without requiring a JEV score
   of at least 1. This is exactly arm C. Keep JEV for packs, tarots, spectrals, economy items and
   close calls, with the forecast numbers in the item fields as v19 does.
4. Verify with JEV only after the code-level change. A JEV sim benchmark needs at least 150-200
   seeds to see effects of about 0.5 ante, given the run-to-run variance seen.

## Arm E: the bot's real play code, JEV replaced by its top-ranked option

The question: how much of JEV's apparent edge over the lab play policy comes from our code's play
forecasts, and how much from JEV's judgement? Arm E runs the real `JevBot` on `SimBalatro` with
`decisions.py` (v20 14f19901). Each question gets a code answer:
- every Choice takes the first option, which is the bot's top-ranked one (best play, best discard,
  play the blind; cash-tarot steps continue shopping);
- `play_now` plays when the strongest play clears or matches the best measured discard;
- shop and pack item scores are 0, except the shared planet rule (4 for the most played hand's
  planet, keeping $4).

The shop is therefore v20's code path only: forecast auto-buys at more than 2 SE (3 bosses, n=16 as
in C) and the reroll rule. v18's strong-voucher rule was switched off so the shop matches arm C.
Joker rearranging and planet auto-use come from `strategy.auto_action` as in the bot. Same 600
seeds. About 109 s per game (odds forecasts on every hand), so 600 games took about 80 minutes on
15 workers.

| arm | mean ante (SE) | ante-8 clear | blinds cleared |
|---|---:|---:|---:|
| A gain (lab play) | 4.00 (0.08) | 1.2% | 10.4 |
| C forecast-long (lab play) | 4.90 (0.09) | 6.0% | 14.1 |
| **E forecast-long + bot play code** | **5.47** (0.08) | 5.8% | 15.9 |
| D none | 1.74 (0.02) | 0% | 3.2 |

Paired differences:

| | ante | blinds cleared | ante-8 clear |
|---|---:|---:|---:|
| E − C | +0.56 ± 0.09 | +1.83 ± 0.29 | -0.2 ± 1.1 pts |
| E − A | +1.47 ± 0.10 | +5.46 ± 0.30 | +4.7 ± 1.0 pts |
| E − D | +3.73 ± 0.08 | +12.7 ± 0.27 | +5.8 ± 1.0 pts |

Death ante for E: 1: 44, 2: 34, 3: 15, 4: 44, 5: 116, 6: 153, 7: 104, 8: 55, won 35. That is far
fewer early deaths than C (93 against 168 at ante 1-3), but no more wins.

**Reading.** The bot's play code (hands.py candidates, odds and setup forecasts, discard plans)
is worth about +0.56 ante over the lab's simple policy. Most of that comes from surviving ante 1-3;
it adds nothing at ante 8. The code-only bot reaches a mean ante of **5.47**, the same as JEV's sim
benchmarks: v18 5.50 and v19 5.47 on sim30-a, v20 pending. The seed sets and shop versions differ,
so this is not a paired comparison. Still, the gap the coordinator called "JEV's +1.5 over the lab"
looks like it is almost all our code's play forecasts plus the forecast shop: E − A = +1.47 ± 0.10.
On these numbers JEV's own judgement adds roughly nothing on top of taking its top-ranked options.

The direct test is cheap: run arm E on sim30-a or sim60-a (no JEV) and compare it paired with the
JEV runs of the same version.

## Death tables, `--set` and sequential stopping (lab tooling)

- **Death stats.** Every lost game now records its fatal blind: name, type, boss effect, target,
  final score, best single hand in that round, hands played and jokers at death. For the bot arms
  these come from the run's `decisions.jsonl` (`outcome` plays after the last `round_end`) and
  `JevBot._round_blind`. `--report` prints the [loss-audit](loss-audit.md) tables per arm (death
  blind, final score / target, fatal bosses) after the JSON.
- **`--set NAME`** plays a fixed seed set from `src/benchmark.py` instead of generated seeds.
- **Sequential stopping** (on by default when 2+ arms run): every `--check-every 100` completed
  pairs, the paired ante difference *first arm minus second arm* is checked. The run stops when
  |diff| > `--stop-at-se 3` SE, or when the 95% CI upper bound is below `--min-gain 0.15` ante
  (no meaningful gain). `--stop-at-se 0` turns it off. List the treatment arm first.

## Arm E on sim60-a against the JEV v20 benchmark

`--set sim60-a --arms bot-forecast-long --name sim60-e`, paired with v20's (14f19901) JEV sim
benchmark on the same 60 seeds (`runs/sim`):

| | mean ante (SE) | wins |
|---|---:|---:|
| arm E (code, JEV's top option) | 5.70 (0.23) | 6 |
| JEV v20 | 6.08 (0.20) | 8 |
| **E − JEV, paired** | **-0.38 ± 0.21** (SD 1.61) | -3.3 ± 4.7 pts |

E did better on 10 seeds, the same on 30 and worse on 20. On the sim30-a half the difference is
-0.30 ± 0.29. Most of the gap comes from four seeds where E dies at ante 1-4 and JEV goes on
(XYK41G5M -6, 68RZS4XR -5, R8ZXJN6L -4, 54ATPSF9 -4). So JEV's judgement looks worth about
+0.4 ante over its top-ranked option (1.8 SE). That is suggestive, not conclusive, and it
corrects the arm E reading above that JEV adds "roughly nothing". Arm E is deterministic: its
rerun on 305 of the main seeds reproduced every ante and blind count.

## Arm F: forecast packs, shop tarots and spectrals (`src/rollout_items.py`)

`rollout.forecast` lists packs, tarots and spectrals as unsupported, so arm E scores them 0 and
never buys them. Arm F (`bot-forecast-items`) is arm E plus:

- **`rollout_items.forecast`** (not hashed, lab only). It takes the public shop state and lists
  these options: skip, each affordable pack, and each affordable shop tarot or spectral that the
  bot will actually use (`tarots.TARGETED` or `SAFE_UNTARGETED`). Each option gets the same
  frozen-build rollouts as `rollout.forecast` (`build_state`, `Policy`, common random numbers RO{k},
  3 bosses, n=16), with two additions:
  - **Packs** are opened in the engine with the rollout's fresh RNG. Contents are sampled from the
    engine's pools and never from the real seed. The card to take comes from `Picker` (below).
  - **Held consumables** are used as `tarots.auto_use` uses them: safe untargeted ones at once,
    targeted ones on the dealt hand with `tarots.plan`'s targets. Enhancements go on the build's
    best cards, Steel and Gold on low cards. Death, Strength and Hanged Man shape the deck, and
    suit tarots convert toward the flush suit. The play policy is rebuilt when jokers change.
- **`Picker`**, a cheap pick rule used both in the rollouts and for real pack openings in arm F.
  Values are on one scale (fractional score gain per hand, $1 = 0.01):
  - jokers: `jokers.gain` on 12 synthetic hands dealt from the deck multiset with a fixed RNG
    (never the draw order), +0.01; only with a free slot;
  - planets: the gain of their level-up, always taken;
  - Black Hole: 0.3;
  - enhancement tarots: `tarots.measured`;
  - other usable tarots and spectrals: a flat table, targeted ones only with a plan;
  - Hermit and Temperance: their cash;
  - Standard-pack cards: edition, enhancement and seal value, scaled down when off the flush suit.
- **Shop rule.** The option that beats skip by more than 2 SE in blinds cleared scores 3, which
  is bought (the shared planet rule's 4 comes first). A cash tarot is taken at the cash step. In an
  open pack, `Picker`'s card scores 4, so `decisions.booster` takes it with `tarots.plan` targets.

Run: `--seeds 600 --arms bot-forecast-long,bot-forecast-items --name ef` (the main seeds). E was
rerun to get death stats; it matched main exactly. The run was stopped by the sequential rule at
the 300-pair check: 303 complete pairs, |+0.44| > 3 SE.

| arm | mean ante (SE) | ante-8 clear | blinds cleared | s/game |
|---|---:|---:|---:|---:|
| E bot-forecast-long | 5.48 (0.12) | 7.3% | 16.0 | 108 |
| **F bot-forecast-items** | **5.92** (0.10) | 7.3% | 17.4 | 155 |
| **F − E, paired (303 seeds)** | **+0.44 ± 0.12** | +0.0 ± 1.6 pts | +1.38 ± 0.38 | |

Death ante:

| arm | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | won |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| E | 26 | 19 | 7 | 17 | 51 | 78 | 55 | 28 | 22 |
| F | 9 | 11 | 10 | 20 | 58 | 65 | 67 | 41 | 22 |

| deaths (281 losses each) | E | F |
|---|---:|---:|
| died at a boss / big / small blind | 202 / 59 / 20 | 194 / 70 / 17 |
| died at ante 5-7 | 184 | 190 |
| median final score / target | 0.755 | 0.764 |
| within 75-100% of target (close calls) | 142 | 149 |
| final < 25% / 25-50% / 50-75% of target | 11 / 44 / 84 | 9 / 35 / 88 |
| median best hand in fatal round / target | 0.297 | 0.298 |
| deaths to The Water | 13 | 11 |
| median jokers at death | 5 | 5 |

Top fatal bosses (E / F): The Wall 17 / 22, The Water 13 / 11, The Eye 11 / 9, The Needle 11 / 7,
The Window 12 / 5, The Head 9 / 12, The Plant 9 / 11, The Mouth 8 / 11.

**What F buys** (per game): 2.9 item buys and 2.8 pack picks. Over the 303 games that is 337
Buffoon, 230 Celestial, 123 Arcana, 96 Standard and 6 Spectral packs, plus about 60 shop tarots
(mostly Judgement, Hermit, Emperor, Temperance). Picks were 335 jokers, 257 planets, 145 tarots,
104 playing cards and 9 spectrals.

**Speed.** 36 item forecasts per game at 0.99 s each (n=16, 3 bosses, up to 5 options, one core
while 15 workers ran). That is on par with the joker forecast (1.17 s per call). A game takes 155 s
against 108 s for E. 25 of about 10,800 item forecasts raised an exception and bought nothing.

**Reading.** The gain is real (3.8 SE) but sits almost entirely in **early survival**: ante 1-2
deaths fall from 45 to 20, mostly through Buffoon packs filling joker slots in ante 1-2. Wins do not
move (22 each), and the loss audit's shape is unchanged. Deaths are still ante 5-7 boss scaling
shortfalls with the best fatal hand at 0.30x of the target. Packs and tarots do not fix late
scaling; The Wall rises with the extra runs that now reach it.

**Recommendation: ship as v21.** Wire `rollout_items.forecast` into `decisions.shop` beside the
joker forecast. Code buys a pack, tarot or spectral that beats buying nothing by > 2 SE (after the
joker auto-buy, before JEV). In `decisions.booster`, use `Picker`'s card (or give JEV its value as a
field). Keep the 4 s budget: run both forecasts in one budget, or cut item n to 12 when the joker
forecast took over 2 s. The home-advantage caveat applies (the rollouts use the lab play policy).
The sim60-a result above suggests checking that JEV does not override these buys.

## v21 in production: confirmation against arm F

`--name v21`: arm F's 303 results were copied in from `ef`, and arm E was run on the current
production code (v21 e98e336d, JEV replaced by its top option) on the same seeds
(`--only-done bot-forecast-items`). The sequential rule stopped at the 100-pair check, because the
95% CI upper bound +0.07 was below +0.15. Production minus arm F = **-0.04 ± 0.06 ante**
(-0.11 ± 0.21 blinds), so production behaves like arm F. Arm F has no further role: from v21
its lab item forecast duplicates production's.

## Arm G: remaining-round planner (`src/round_planner.py`, lab only)

Half of the code bot's losses are close calls (149 of 281 in arm F end at 75-100% of the target).
Arm G (`bot-planner`) is v21 production plus a planner in front of `decisions.selecting_hand`.

- **When it runs.** Only when the round is at risk: no candidate play clears outright, and best
  estimate x hands left < 1.5x the chips still needed.
- **Candidates.** The bot's top 4 plays (`hands.play_candidates`), its discard plans
  (`hands.discard_candidates`, up to 6) and the fast policy's discard.
- **Evaluation.** Each candidate gets 32 rollouts of the rest of the round. A rollout applies the
  candidate, then plays on with `rollout.Policy` and records whether the blind is cleared.
- **Public information only.** Every copy re-sorts the draw pile canonically, shuffles it with a
  fresh RNG and reseeds the engine. A test checks that the real draw order does not change the plan.
  Known gap: face-down cards are visible to it, as in `rollout.Policy`.
- **Common random numbers.** Copy k of every candidate has the same deck order and RNG.
- **Override rule.** The planner overrides the bot only when its best candidate beats the bot's
  strongest play by at least 0.10 in P(clear) and by more than 2 paired SE. A first version
  without this gate overrode on 1-2 rollout differences, which were noise.
- **Cost.** 0.75 s per evaluation, 23 evaluations and 2.8 overrides per game (81% of overrides
  are discards), +10 s per game.

Result (`--arms bot-planner,bot-forecast-long --name v21`): the rule stopped at 200 pairs, with
95% CI upper bound +0.06 < +0.15.

| arm | mean ante (SE) | ante-8 clear | blinds cleared |
|---|---:|---:|---:|
| v21 production (E) | 5.84 (0.13) | 10.5% | 17.2 |
| G planner | 5.79 (0.13) | 9.5% | 17.1 |
| **G − v21, paired (200)** | **-0.06 ± 0.06** | -1.0 ± 1.0 pts | -0.17 ± 0.19 |

| deaths (200 seeds) | v21 | G |
|---|---:|---:|
| losses | 179 | 181 |
| died at a boss | 130 | 132 |
| within 75-100% of target (close calls) | 90 | 94 |
| final < 25% / 25-50% / 50-75% | 6 / 30 / 53 | 6 / 25 / 56 |
| median final score / target | 0.753 | 0.764 |
| median best hand in fatal round / target | 0.302 | 0.308 |
| deaths to The Water | 7 | 9 |

**Reading: no gain, not shipped.** Close-call deaths do not fall (90 against 94). The rollouts
judge each first move by how the weaker fast policy would continue. Where the planner disagrees
with the bot's own play code, it is not more often right. Most close calls are also not decided
by one move: the best fatal hand is 0.30x of the target. A planner would need the bot's own play
code inside the rollouts (about 100x slower) or a stronger rollout policy.

## Compute scaling (2026-10-16): more forecast compute does not help

Arm `bot-heavy` (forecasts n=128, 5-boss horizon, no time budget, Picker synth 96) against the v22
production path (`bot-forecast-long`), paired on the main seeds. Interrupted once by a PC restart and
resumed from `runs/_policy_lab/scale/results.jsonl`; stopped by hand at 128 pairs because the
question (does it gain >= +0.5 ante?) was settled.

| | mean ante | wins |
| --- | ---: | ---: |
| v22 production forecast | 5.95 | 12 |
| heavy forecast (~4x compute, 609 s/game) | 5.79 | 10 |
| heavy - production | -0.16 +- 0.19 | |

Upper 95% bound about +0.22 ante. Forecast compute is not the bottleneck for decision quality, so a
Rust/native port is not justified by play strength. It would still speed experiments (development
throughput), which is a separate cost/benefit question. Next gains have to come from what the
forecasts model (frozen build, the play policy inside rollouts) and from decision logic.

## Shopping rollouts (2026-10-16, lab only): `bot-forecast-shop`

**Problem.** Both shop forecasts play the horizon with a frozen build. Shops inside a rollout are not
even populated (`sim_patches.rollout_skips_shop_population`). Money left over is therefore worth
nothing to the forecast, interest has no value, and jokers that pay off through later buys are
undervalued.

**`src/rollout_shop.py`** (not hashed). `forecast` and `forecast_items` are drop-in versions of
`rollout.forecast` and `rollout_items.forecast`. Their rollouts shop at every shop they reach after
a cash-out:
- **Shop contents.** The engine populates shops with the rollout's fresh RNG (`RO{k}`), never the
  real seed's future. Only these states clear the no-shop flag.
- **`Shopper`** works without nested forecasts. Each joker gets a static value, cached per key and
  edition for the whole forecast call: `Picker.joker_value` (jokers.gain on 12 synthetic hands from
  the deck multiset; rental, eternal and joker-destroying jokers are worth 0) plus 0.10 for scaling
  jokers. Owned jokers of the real build are valued by the loss from removing them.
- **Buy rules.**
  - With a free slot, it buys the best joker worth at least 0.03, up to 2 buys per shop.
  - With full slots, it sells the weakest joker when the new one is worth more than 1.5x it plus
    0.05.
  - It keeps $25 for interest from ante 3.
  - It buys the planet for the most played hand when the money stays above that buffer (and $4).
  - It never rerolls and never buys packs or vouchers.
- **Lab arm.** `bot-forecast-shop` is arm E (v23 production path) with both forecasts swapped in via
  `rollout_shop.Patched()`.
- **Tests:** `tests/test_rollout_shop.py`.

**Result.** sim30-a, code only, both arms deterministic, `--name shop30`:

| arm | mean ante (SE) | wins | blinds cleared | joker / item forecast s per call (n=16) | s/game |
|---|---:|---:|---:|---:|---:|
| bot-forecast-long (v23) | 5.17 (0.32) | 1 | 15.4 | 0.85 / 0.76 | 112 |
| bot-forecast-shop | 5.80 (0.36) | 2 | 17.2 | 1.27 / 1.02 | 149 |
| **shop − long, paired (30)** | **+0.63 ± 0.45** | +3.3 ± 3.3 pts | +1.83 ± 1.45 | | |

- **Per seed (ante):** 12 better, 12 the same, 6 worse. By blinds cleared: 13 better, 6 the same,
  11 worse.
- **Where the mean comes from:** four early-death rescues (TU9KANLF +7, HXGTTFCH +6, F1PE5WEY +5,
  6L3CIXAF +4) against two early collapses (WKPGXL14 −5, 11BLBC1T −3). These swings come from games
  diverging at ante 1-3, which is chaotic. They are not evidence about late scaling.
- **Deaths (shop against long):** 2 at ante 1-2 against 4. Plain-blind deaths: 8 against 9 (no small-blind deaths
  against 3). Close calls (75-100% of target): 14 against 20. Deaths at ante 7-8: 11 against 4.
- **Inside the rollouts:** 0.37 joker buys per shop visited and 0.24 swaps. The swap rate is high:
  the static values churn jokers.

**Latency.** At n=16 with no budget, 12 workers at idle priority: the joker forecast costs +49% per
call and the item forecast +34%. The extra cost comes from shop population and from longer rollouts
(they survive more blinds). At production's n=24 that is about 1.9 s plus 1.5 s, so late shops
would hit the 4 s budget more often and drop toward the 8-rollout floor. The paired SE between
options also grows: in first-shop smoke tests the SE of a buy against skip rose from about 0.35 to
0.55 blinds. Fewer options would clear the 2-SE auto-buy bar.

**Reading.** The direction is positive (1.4 SE), but 30 seeds can't confirm it. Blinds cleared is
nearly a coin flip seed by seed (13 better, 11 worse). A 300-seed lab run is needed before this
ships.

## Mid-game scaling experiments (2026-10-16, cloud box, 4 workers): v28

All arms are arm F (`bot-forecast-items`) plus one change, paired on the same seeds, code only. v27
baseline: sim60-a 5.90 ante (4 wins); 60 fresh seeds (`--seeds 60`) 6.15 (6 wins).
`PLAB_KEEP_LOGS=<dir>` keeps each game's `decisions.jsonl` and `states.jsonl.gz` for these analyses.

**Diagnosis (v27, sim60-a).**
- Hand levels barely move: the median best hand level is 2 at antes 5-7 and about 3 levels are
  gained in total by ante 6. 18 of 60 games never used a planet from a consumable slot.
- Joker slots are full from ante 3. Buys fall from about 2 per ante to 1 per ante after ante 4, with
  about $30 held.
- Jokers held at death are mostly flat +Mult/+Chips (Abstract, Blue Joker, Banner, Odd Todd). xMult
  jokers are offered about 3 times a game and rarely bought: their forecast gain, re-run at n=64 on
  saved states, is mostly negative for swaps (The Order -0.62 +- 0.17, The Tribe -0.44 +- 0.20).
- The forecast is not saturated: from ante 3 the skip option already survives all 3 bosses in ~0%
  of rollouts. Forecast skip blinds sit below what the game then clears (the game keeps shopping).

**Planets (shipped as v28).** `jokers.gain` keeps each sample's play fixed, so a planet for a hand we
don't play yet was worth 0: pack picks tied at +0.001 and levels spread over Pair, High Card and
Straight Flush. v28 values a pack's planet with the best play re-chosen after the level-up on deals of
hand size + 3 (`rollout_items.planet_gain`, Rust `best_score`), and the shop buys the planet for the
`play_mix` top hand and the biggest Celestial pack while $4 stays (`decisions._planet_rule`).

| change (lab arm) | sim60-a | fresh 60 |
|---|---:|---:|
| shop planet for the `play_mix` top hand (`bot-pl-mix`) | +0.00 +- 0.08 | |
| + Celestial packs, keep $10 (`bot-pl-mix-cel10`) | +0.18 +- 0.15 | |
| + Celestial packs, keep $4 (`bot-pl-mix-cel4`) | +0.20 +- 0.20 | |
| + re-chosen planet picks (`bot-pl-reopt-cel4`) | +0.25 +- 0.19 | +0.13 +- 0.16 |
| **v28 production code** (cb57eae7) | **+0.27 +- 0.20** (10 wins vs 4) | **+0.35 +- 0.14** (10 vs 6) |

Pooled over 120 seeds v28 is about +0.31 +- 0.12 ante, wins 20 against 10. Levels gained by ante 6:
2.8 -> 8.8. Lab time per game 16 -> 22 s.

**Did not help.**
- Rerolls: 4 per shop +0.12 +- 0.07; 4 down to $15 -0.03 +- 0.12; 6 down to $10 -0.12 +- 0.14.
- An xMult bonus (+0.3 / +0.6 blinds added to xMult joker forecasts, `bot-xm-03/06`): 24 -> 39 xMult
  buys, -0.05 / -0.08 +- 0.14 ante. Bought blindly, they are mostly conditional effects the build
  doesn't meet (Baseball Card without Uncommons, Vampire without enhanced cards).
- A joker-aware rollout discard (Rust only, `docs/samples/smart-discard.patch`, env
  `JEV_SMART_DISCARD=1`): holds by suit (3+), rank groups, the best straight window and the best play,
  each scored over 8 redraws with the text scorer. It is a stronger rollout player (skip clears 3.90
  vs 3.61 blinds over the 3-boss horizon on 12 games' shops) but the shop decisions did not improve:
  -0.02 +- 0.24 on sim60-a against v28, 2x slower. Every option gets the same better player, so
  paired option differences barely move.

**Reading.** The forecast judges each item alone against a frozen build. What it cannot see is the
build changing around an item (later buys, planets and tarots aimed at a joker's condition), which is
how strong runs find a working xMult. The shopping rollouts above were the one experiment that
models later buys (+0.63 +- 0.45 on 30 seeds, Python only and slow).

**After v28 (same day): wins vs losses, and four more arms, none shipped.**

Win/loss comparison on 200 new v28 games with full logs (`--seeds 200 --seed-offset 60`, 27 wins),
at the first shop of each ante, winners against games that died later:
- Winners are already stronger at ante 3: a typical hand x 4 is 3.2x that ante's boss target, against 2.2x.
- xMult jokers separate the mid-game. Winners hold 0.56 -> 0.89 -> 1.07 at antes 4 / 6 / 7; later losers
  stay at about 0.55 and keep more flat +Mult jokers. Both see 2-3 xMult offers per game at antes 4-6;
  winners end up holding 0.67 new ones by ante 5-7, losers 0.25.
- A scaling-joker signal on the first 120 games (14 wins) did not hold on these 200.
- Swap blind spot: with full slots, only the 2 jokers with the smallest measured loss on recent hands
  may be sold. In one loss the forecast (n=16) had three Ramen swaps over the 2 SE bar (sell Abstract
  Joker +1.25 +- 0.28) but only Fibonacci and Seltzer were eligible.

| arm (vs v28, paired) | sim60-a | fresh 60 |
|---|---:|---:|
| late spending: from ante 5 keep $5, 6 rerolls (`bot-late5`) | -0.08 +- 0.09 | |
| late spending from ante 6 (`bot-late6`) | -0.10 +- 0.05 | |
| swaps may sell any of 3 jokers (`bot-swap3`) | -0.03 +- 0.09 | +0.02 +- 0.07 |
| swaps may sell any joker (`bot-swap5`; 3.6 -> 3.8 swaps a game) | -0.05 +- 0.09 | +0.03 +- 0.09 |
| shopping rollouts on v28 (`bot-items-shop`, 7x slower) | -0.27 +- 0.23 | -0.68 +- 0.28 |

The shopping rollouts lose about half an ante over 120 seeds (-0.48 +- 0.18; 24 seeds better, 50
worse), so porting `rollout_shop` to Rust is not worth it. Note: the lab's per-arm `mean_swaps` counts
"replace" in a buy description, which reads "sell X for Y"; count "sell " instead.

**v29, v30 and tag skipping (2026-10-17).**
- Early game (antes 1-3, 200 v28 games): winners are stronger from the first shop (a typical hand x 4
  is 6.2x the ante-1 boss target against 3.6x) with the same joker count and buys, which looks like
  luck of the early shops. Edition jokers in ante 1-3 shops were taken 0 of 20 times, mostly because
  slots were full.
- **v29** (`a0adcc12`): `decisions.shop` dropped every joker when slots were full, Negative ones too
  (`rollout.options` and the game allow them). sim60-a +0.05 +- 0.04, fresh 60 +0.00 (rare).
- Where v28 runs end (200 games): 69 reach ante 8 and 27 win. The ante-8 finisher kills most of the
  rest: Verdant Leaf 13, Violet Vessel 8 (8 of 8 met), Crimson Heart 6, Amber Acorn 5, Cerulean Bell 4.
- **v30** (`9ecfe423`): under Verdant Leaf the bot never sold a joker, and Jackdaw never lifted the
  boss on a sale (rumbot-sim did). `decisions._leaf_sale` sells the weakest joker first;
  `sim_patches.verdant_leaf_lifts_on_joker_sale` lifts it. On the 200 seeds: wins 27 -> 29, Verdant
  Leaf deaths 13 -> 10 (4 of the 13 now win; one v28 win, TIXQA7PH, now sells Troubadour and falls 2%
  short: the sale should skip hand-size jokers). sim60-a and fresh 60: identical antes. **To check on
  the real game:** that BalatroBot allows `sell` during a round (the bot falls back to playing if
  not), and the sim patch against the Lua dump plus `sim_parity.py`.
- Skipping small/big blinds for tags when readiness for the next blind is >= 1.3 / 1.5
  (`bot-skip-strong`: Negative, Polychrome, Rare, Holographic, Investment, Orbital, Meteor, Buffoon,
  Uncommon, Double; `bot-skip-wide` adds 9 more): strong -0.03 +- 0.19 / +0.02 +- 0.16, wide
  -0.22 +- 0.21 / -0.38 +- 0.19 (sim60-a / fresh 60). A lost shop visit costs more than the tag.
- Director's Cut / Retcon would reroll Violet Vessel, but neither Jackdaw nor BalatroBot (as used
  here) has a boss-reroll action.
