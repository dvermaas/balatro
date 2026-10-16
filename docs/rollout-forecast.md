# Rollout forecast for shop decisions (phase 1, offline)

2026-10-16. `src/rollout.py` (new, not wired in, not hashed) forecasts each shop option by
simulating the next blinds in Jackdaw. `scripts/validate_rollout.py` checks it against ground truth
in the sim. Data: [samples/rollout-validation.json](samples/rollout-validation.json). No JEV calls.

## Why

The [loss audit](loss-audit.md) found that runs die at ante 5-7 bosses. In those rounds the best
hand is about 1/3 of the target, so the build does not scale. Today the shop values a purchase as
`jokers.gain`: the score ratio on recently played hands, computed with a text-parsed scorer. That
measure cannot see scaling jokers grow, cannot see economy or utility, and ignores the blind
curve.

## What it does

`rollout.forecast(pub, n=40, bosses=2)` takes the **public** BalatroBot shop gamestate and:

1. **Rebuilds a Jackdaw state** (`build_state`) from public information only:
   - the deck as an order-free multiset (canonically sorted, so the order of `gs["cards"]` is
     never read; a test enforces this);
   - Hiker chips, enhancements, editions and seals;
   - hand levels and play counts;
   - jokers, with their dynamic values parsed from the effect text (about 30 scaling jokers,
     checked by round-trip tests);
   - eternal, perishable and rental stickers, and sell values;
   - Idol, Mail, Ancient and Castle targets, skips (from Throwback), tarots used (from Fortune
     Teller) and planets used (from Satellite);
   - vouchers, applied with the engine's `apply_voucher`;
   - hand, joker and consumable limits, money, ante and round;
   - this ante's known boss and the blind statuses.
2. **Lists the options** the bot's shop offers: skip/save, buy a joker, planet or playing card,
   redeem a voucher, or sell joker j and buy joker i. Tarots, spectrals and packs are listed as
   `unsupported`. Rerolls are omitted.
3. **Runs rollouts.** For each option it runs n rollouts: deep-copy the state, reseed the RNG
   with `RO{k}`, apply the option through real engine actions (`BuyCard`, `SellCard`,
   `RedeemVoucher`), then play with the build frozen until `bosses` boss blinds are done or the
   run dies. In those rollouts:
   - planets are used as soon as they are held;
   - shops are left without buying;
   - scaling jokers, boss effects, money and interest all run in the engine;
   - future bosses come from the engine's own pool draw.
4. **Uses a fast code policy** (`Policy`) for play and discard decisions:
   - It builds a small set of structural candidates: rank groups, two pair, full house,
     flushes, a straight and high card.
   - It scores them with the bot's own text scorer (`jokers.score`; read-only import).
   - It handles The Psychic, The Eye, The Mouth and the Cerulean Bell forced card.
   - It discards toward a flush (4+ of a suit) or toward rank groups when the best play is
     below the pace the blind needs.
   - The engine computes the real score.
5. **Uses common random numbers.** Rollout k of every option uses the same seed. Balatro's RNG
   streams are keyed by name, so shuffles and boss draws line up across options.

The output for each option has these fields:
- `p_survive`: P(clear every blind through the horizon), with `se_survive`.
- For each upcoming blind: `p_clear` (unconditional) and `p_clear_given_reached`. Boss rows also
  have `median_margin` (score / target) and `bosses_seen`.
- `exp_blinds_cleared`.

`paired_delta()` gives the CRN difference between two options and its paired SE.

## Ground truth and how it was validated

`validate_rollout.py` collected 40 shop states:
- 24 from the v18 `d22adb59` JEV sim runs (`runs/sim`, seed set sim30-a, finished games only);
- 16 from 12 no-JEV `--policy random` games (`runs/_rollout_validation/random`, seeds
  `RVAAAAA1..RVAAAAB3`).

By ante: 1×6, 2×15, 3×5, 4×8, 5×2, 6×2, 7×2. The sample is early-heavy because random-shop games
die by ante 2-3.

For each state the recorded run is **replayed in the sim** to recover the true engine state. All
40 replays matched the recorded public state. Then, for each supported option (214 option rows in
total):

| | built from | seeds | count |
|---|---|---|---|
| forecast | public state only (`sim_backend.serialize` of the true state) | `RO0..RO39` | 40 |
| truth | the TRUE engine state (real deck, joker internals, boss history) | fresh `GT0..GT39` | 40 |
| fidelity | the TRUE engine state | the forecast's own `RO0..RO19` | 20 |

Both forecast and truth use the same frozen-build code policy. So truth answers this question:
under this policy, how often does this option survive, given the real hidden state and an unknown
future? The same-seed fidelity runs isolate reconstruction error from sampling noise.

## Results (`--states 40 --n 40 --m 40`, horizon 2 bosses)

**Speed** (one core, measured while a JEV sim benchmark ran in parallel):
- about 12 ms per rollout;
- 1.2 s median per shop (median 4 options);
- **1.75 s median and 3.1 s p90, normalised to 6 options**;
- 7.2 s maximum, for a full-slot shop with many sell-swap combinations.

That meets the ≤ ~5 s target for 5-8 options at 40 rollouts.

**Reconstruction fidelity** (same seeds, public versus true state): the survival outcome is
identical in 97.4% of paired rollouts and the full blind-by-blind path in 90.5%. The residue comes
from hidden information: boss-pool usage history (which boss is drawn), joker counters that are
not fully public, and tarots used without a Fortune Teller.

**Calibration against branch truth:**

| quantity | MAE | bias | Pearson |
|---|---:|---:|---:|
| P(survive 2 bosses) | 0.014 | -0.001 | 0.98 |
| P(clear the next boss) | 0.046 | +0.018 | 0.979 |
| expected blinds cleared | 0.106 blinds | +0.002 | 0.995 |

P(clear next boss) by forecast bin (forecast → truth):
- 0.03 → 0.03 (n=85)
- 0.29 → 0.23 (16)
- 0.52 → 0.54 (12)
- 0.68 → 0.62 (25)
- 0.93 → 0.91 (56)

There is a slight optimism of about 0.05 in the 0.2-0.8 range.

P(survive 2 bosses) is **degenerate in mid and late antes**: 189 of 214 option rows are below 0.2
in both forecast and truth, because a frozen build rarely survives two more bosses. For ranking,
the informative numbers are P(clear the next boss), expected blinds cleared and the boss margin.

**Variance:**
- At n=40 the mean binomial SE is 0.037 on P(next boss) and 0.014 on P(survive), which is mostly
  0 or 1.
- For "option minus skip" in blinds cleared, CRN gives a paired SE of 0.095 against 0.152
  unpaired, about 2.5x fewer rollouts for the same precision.

**Rank agreement with truth** (value = P(survive) + 0.01 × blinds cleared):
- The mean Spearman between forecast and truth option rankings is **0.945** (32 states with 3+
  options).
- The forecast's top pick is within one truth SE of the truth-best option in **95%** of states.

## Compared with `jokers.gain`

`jokers.gain` was computed exactly as v18 sees it: the run's last 10 plays, reconstructed from the
recording, with `remove=` for swaps and `level_up=` for planets. Skip counts as 0. Unparseable
jokers drop out.

On the options gain can measure (40 states):

| | Spearman vs truth | top pick within truth SE |
|---|---:|---:|
| `jokers.gain` | 0.43 | 70% |
| rollout forecast | 0.93 | 97.5% |

The two disagree on the top pick in 14 states. Truth sides with the forecast in 12 and ties in 2.
Gain is never right where they disagree. The patterns:
- **Gain over-rates hand-specific +Mult on recent hands.** Gluttonous Joker (gain +215%) and The
  Idol (gain +205%) score big on the few hands they were measured on. Over the next blinds they
  lose to Mystic Summit (0 → 7.5% survive) and Space Joker (0 → 35%).
- **Gain is 0 for non-text-scoring jokers that do score.** Misprint is 80% against 0% for skip
  at ante 1. Space Joker, Card Sharp, Hack and Pareidolia come out at 0 or are unmeasured.
- **Swap choice:** gain sells Even Steven for Raised Fist; the forecast sells Popcorn, which is
  decaying.
- **Planet choice:** gain prefers the levelled hand of recent plays (Pluto); the forecast prefers
  the hand the deck actually makes (Jupiter).

Caveat: truth uses the same frozen code policy, so it inherits that policy's biases. For example,
Mystic Summit is valued because the policy often ends rounds with 0 discards. "Agrees with truth"
means "agrees with what this policy achieves", not "agrees with optimal play". Gain is also
handicapped by design: it was never meant to see economy or scaling.

## Compared with v18's actual purchases

24 v18 states. In 4 of them v18 bought a pack or tarot, which phase 1 does not simulate. Of the
remaining 20, the forecast's best option **differs from v18's choice in 8 (40%)**. Reroll and
leave both map to "buy nothing now".

The mean truth regret (truth value of the best option minus truth value of the choice) is 0.041
for v18's choices and 0.009 for the forecast's picks.

Notable cases:
- XT9RAZDH ante 4, $49: v18 rerolled. Gluttonous Joker takes truth P(survive 2 bosses) from 0.23
  to 0.50.
- AEQ18EUN ante 2, $15: v18 bought The Idol (truth 0.00). Space Joker reaches 0.35; Hieroglyph
  reaches 0.23.
- R8ZXJN6L ante 2: v18 left. Uranus doubles a small P(survive).
- 665WDGX8 ante 3: v18 rerolled where Space Joker was marginally better.

Many late-ante v18 states are ties at P(survive) = 0, where every option loses within the
horizon.

Hieroglyph shows a horizon effect. Its ante -1 is a real benefit, but with a horizon counted in
bosses it also replays an easier ante. Read voucher forecasts against the blind labels.

## Known gaps

- **Not simulated:** packs, tarots, spectrals, and rerolls (the value of searching the shop).
  Rerolls appear as "skip".
- **Frozen build:** no future purchases, so absolute survival is pessimistic beyond the next boss.
  Use the forecast as a relative ranking plus near-term probabilities.
- **Hidden state:** past bosses (pool usage), skips and tarot or planet counts without their
  joker, Obelisk-style streaks.
- **Policy:** the policy sees face-down cards (The House, Wheel, Mark, Fish) in both forecast and
  truth. Discards are simple, and no blinds are skipped.
- **Cost:** sell-swap options multiply with full joker slots (up to 15 options, about 7 s). For
  production, prune swaps to the two weakest owned jokers.

## Proposal: wiring into the shop (v19)

1. **When to run it.** In `decisions.shop`, run `rollout.forecast(gs, n=32, bosses=2,
   budget_s=4)` once per shop visit. Cache it by (seed, ante, round, shop contents, money) so it
   is not recomputed after each action within the visit. Prune swaps to the 2 weakest owned
   jokers. Packs and tarots keep today's path.
2. **What JEV sees per item**, replacing `measured_gain` and `readiness_after`:
   - `next_boss_clear`: e.g. "61% (skip 44%, +17 ± 6 pts)", using the paired CRN delta against
     skip.
   - `survive_2_bosses`: e.g. "12% (skip 5%)".
   - `boss_margin`: e.g. "median 0.93x of ante 5 boss target (skip 0.81x)".
   - `blinds_cleared_next_6`: e.g. "4.1 (skip 3.6)".

   Plus one state-level line, `forecast_horizon`, naming the blinds and bosses simulated
   ("ante 4 boss The Water known; ante 5 boss sampled").
3. **Code-level guard rails** (strategy, not JEV):
   - A purchase whose paired delta against skip is more than 2 SE above zero counts as "sure"
     (this replaces `SURE_GAIN`).
   - An option more than 2 SE worse than skip is not offered unless its value is economic. Cash
     tarots keep the existing path.
   - Reroll when no option beats skip by 1 SE and money stays at or above the interest floor.
4. **Experiment.** Evaluate on sim30-a first (sim benchmark, cheap), then bench30-a. Watch ante
   5-7 boss deaths and The Water deaths, which the forecast sees through the known current boss.

## v19: wired into the shop (draft, prompt version 22d2445a)

`rollout.py` is now hashed (`recorder.PROMPT_VERSION`, CLAUDE.md updated). In `decisions.shop`:
- **Forecast call.** `_shop_forecast` runs `forecast(gs, n=32, bosses=2, budget_s=4)` and caches it
  by seed, ante, round, money, shop and voucher contents, and owned jokers. Swaps only sell the 2
  weakest owned jokers, meaning those with the smallest `jokers.gain(remove=j)` loss on recent
  hands. Packs, tarots and spectrals keep the v18 path. The v18 path also takes over entirely if
  the forecast raises, takes more than 10 s, or has no skip baseline.
- **Item fields.** Forecast items carry `next_boss_clear` (with skip and the paired difference
  ± SE in points), `survive_2_bosses`, `boss_margin` and `blinds_cleared_next_6` (with skip and
  the paired difference ± SE). These replace `measured_gain` and `readiness_after`. The state
  carries a `forecast` line saying what was simulated: the frozen build, the known current boss,
  and that later bosses are sampled.
- **Guard rails.** All use the paired blinds-cleared difference against skip:
  - an item more than 2 SE better is "sure" (it still needs a JEV score of at least 1);
  - an item more than 2 SE worse is hidden, unless it is economic (a voucher, a cash tarot, or a
    non-scoring joker);
  - the bot rerolls when nothing beats skip by 1 SE and money minus the reroll cost stays at or
    above $25.
- **No-discard play.** The forecast policy now plays spare low cards alongside a hand when no
  discards are left (The Water, or discards spent) and it is short of pace. The cards must not
  change the hand type or lower its text score. Bosses are still played by the engine.
- `scripts/diagnostics/mechanics_candidate.py` now pins only `jokers.py` to its v18 bytes (the
  candidate patches that file alone), so the diagnostic keeps working under v19.

Smoke checks (no JEV, `--policy greedy`, 2 seeds): every shop decision carried the forecast state
line and item fields, and forecasts took under 1 s. One JEV game on the sim (TU9KANLF,
`runs/_rollout_validation/jev`):
- **Result:** lost at ante 6. v18's sim30-a run of the same seed lost at ante 3. This is one game
  and is not evidence.
- **Cost:** 392k input and 14k output tokens over 151 decisions, 205 s wall, 33 s of JEV time.
- **Forecast latency:** 48 forecasts, median 2.0 s, max 4.1 s.
- **Shop decisions:** 45 of 48 carried forecast item fields. The other 3 offered only packs or
  tarots and carried the state line only.
- **Watch the reroll rule:** it fired 17 times in that game.

## v20: arm C in production (draft, prompt version 14f19901)

Following [policy-lab.md](policy-lab.md):
- **Horizon and rollouts.** `FORECAST_BOSSES = 3` and `FORECAST_N = 24`, with the same 4 s budget.
  With a stubbed JEV over 4 smoke games, 204 forecasts took median 1.8 s, p90 3.7 s, max 4.25 s.
- **Code buys clear winners.** A forecast option that beats buying nothing by more than 2 SE in
  blinds cleared is bought by code before JEV is asked; with several, the largest gain wins. It is
  recorded as an `auto` event with `question: "shop"`, the choice, the action, a reason ("beats
  buying nothing by +2.29 blinds (> 2 SE = 0.76)"), the forecast numbers, and the forecast time and
  rollout count. JEV still rates close calls, packs, tarots, spectrals and economy items, with the
  forecast fields in each item.
- **Unchanged from v19:** the reroll rule, swaps limited to the 2 weakest jokers, hiding items more
  than 2 SE worse, and the v18 fallback.
- **Renamed fields.** `survive_2_bosses` is now `survive_horizon` ("x% clear all 3 next bosses")
  and `blinds_cleared_next_6` is now `blinds_cleared_horizon`, because the horizon is no longer
  2 bosses and 6 blinds.
- **Empty deck guard.** `rollout.play_horizon` treats an empty deck and hand as a lost blind instead
  of crashing.

**Smoke run** (no JEV, `--policy greedy`, 4 seeds, `runs/_rollout_validation/smoke20`): the auto
path fired 44 times, and every one was recorded with its reason and numbers. All 158 JEV-path shop
decisions carried the forecast fields. Greedy games now reach ante 6-8; under v19 the same stub died
at ante 2 because it never buys.
