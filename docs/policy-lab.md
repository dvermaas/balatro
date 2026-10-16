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
