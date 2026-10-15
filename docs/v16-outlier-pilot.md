# V16: investigate and raise the early-failure floor

The user asked to iterate first on persistently weak seeds rather than only
raising the overall average. V15 `6725b5bb` completed bench30-a with two verified
wins and 14.90 mean cleared blinds, but several runs still ended in antes 1–2.

## Selection before implementation

Rank seeds by mean cleared blinds across completed v9, v10, v11, v12, v13 and v15
bench30-a runs. Exclude incomplete v14 from that ranking. Use the four lowest
means, the largest recent regression, and two stronger controls:

| Seed | Reason | v9 | v11 | v12 | v13 | v15 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 75WVKF42 | Persistent worst, recent mean 4.50 | 4 | 5 | 4 | 4 | 5 |
| DHPB8Q1L | Persistent weak, recent mean 5.17 | 5 | 2 | 11 | 6 | 5 |
| PX8GK4E6 | Early failures, recent mean 7.83 | 5 | 5 | 5 | 13 | 5 |
| JV8CL6BQ | Weak history plus regression, recent mean 9.17 | 3 | 3 | 14 | 20 | 7 |
| 2RGEX6EN | V15 regression from earlier 17–23 | 20 | 23 | 18 | 17 | 2 |
| 2CM51JLD | Consistently strong control | 20 | 14 | 20 | 19 | 20 |
| BAPQJMY5 | V15 victory control | 11 | 13 | 19 | 14 | 24 |

The fixed order is the table order, registered as **weak7-a**. Original sets and
the default bench30-a remain unchanged. Full ranking and preselected seed list:
[selection evidence](samples/v16-outlier-selection.json). This is a development
set selected using past results, not a held-out evaluation. Comparisons use the
existing v15 runs on the same seeds; they are not fresh replicate baselines.

## Recorded failure evidence

- `75WVKF42` reached The Water with Crafty Joker and Baseball Card, no discards,
  and scored 1,324/1,600. It passed Scholar earlier and bought Baseball Card with
  no Uncommon support. Conditional scoring and spending remain hypotheses to
  investigate; this experiment does not claim to solve that boss's hand planning.
- `DHPB8Q1L` skipped its first paid Buffoon pack (event 3010), despite rating
  Burglar 1.37/4. After Gros Michel expired it lost the ante-2 boss with Odd Todd
  alone. Burglar's extra hands are not valued by recent-hand score calculations;
  losing discards is a real tradeoff, so its value must be tested.
- `2RGEX6EN` skipped Riff-raff (event 3569, 1.22/4) from a paid pack with an empty
  joker lineup, then died to The Hook with no jokers and $13.
- `JV8CL6BQ` skipped its first pack (event 5248, best Flash Card 1.47/4), then
  reached ante 2 with no jokers. V15 ultimately lost ante-3 Big Blind.
- `PX8GK4E6` passed Misprint at event 2346: its exported description is just
  `  +`, tagged as utility. Installed game source confirms a random integer
  between 0 and 23 Mult (`lovely/dump/game.lua`, j_misprint config;
  `card.lua`, Misprint scoring branch). No random state or future draws were read.

## Focused change

V16 **503b35ff** retains v15 hand play, build selection and shop spending
thresholds. It changes the information and cutoff used for early joker value:

1. Engine guidance lists missing basic Mult support before xMult and removes
   the unconditional claim that xMult is the biggest gap.
2. Pack questions explicitly evaluate a free selection versus skipping. A
   non-rental, non-eternal joker without a destructive effect can fill a spare
   slot if JEV rates it at least Marginal (1/4), rather than requiring 1.5/4.
   JEV still chooses among eligible cards. Low-rated cards can still be skipped;
   rentals, eternal and destructive cards retain the existing cutoff.
3. Utility is identified as outside the recent-hand scoring measurement rather
   than represented as a zero scoring gain with only a scaling caveat.
4. Misprint gets its missing effect range in the item question. Its random mean
   is not added to guaranteed-clear calculations.

No seed-specific policy or draw-order information is used. The exact v15 decision
files are preserved in `runs/_sources/6725b5bb/` before editing. Existing raw
recordings and unrelated UI/controller edits are retained.

## Validation and protocol

All **39 offline tests passed**: the existing 32 plus seven early-item tests,
including the recorded Riff-raff/Burglar skip scores, harmful/permanent/rental
guards, full slots, a genuinely low JEV rating, and Misprint's missing text.
Line endings of hashed sources are preserved.

Three recorded pack questions were rerun with real JEV and no game actions:
Burglar and Riff-raff are now taken; the third chooses Showman rather than the
previous skip. That is evidence of changed decisions, not evidence of improved
survival. See [pack replay](samples/v16-pack-replay.jsonl). Replays use the
existing replay helper's limited recent-play reconstruction and are smoke checks.
The [Misprint replay](samples/v16-misprint-replay.jsonl) still leaves the shop;
restoring the description alone did not change that recorded decision.

The live pilot uses Red Deck, White Stake, `jev-latest`, normal panel delay 0.3 s,
the fixed seven seeds, and the repaired terminal-win controller. Source stays
frozen during the pilot. Auto-starting unrelated random runs is disabled.
Report each seed against its v15 result, weakest-five and control results
separately. Do not claim a general win-rate improvement from this chosen subset.

## Result

The fixed **weak7-a** pilot completed all seven runs with v16 `503b35ff`, zero
wins and zero execution errors. These are cleared blinds, so a completed win is
24:

| Seed | Group | v13 | v15 | v16 | Change vs v15 |
| --- | --- | ---: | ---: | ---: | ---: |
| 75WVKF42 | Persistent weak | 4 | 5 | 5 | 0 |
| DHPB8Q1L | Persistent weak | 6 | 5 | 2 | -3 |
| PX8GK4E6 | Persistent weak | 13 | 5 | 8 | +3 |
| JV8CL6BQ | Persistent weak | 20 | 7 | 14 | +7 |
| 2RGEX6EN | Recent regression | 17 | 2 | 17 | +15 |
| 2CM51JLD | Strong control | 19 | 20 | 20 | 0 |
| BAPQJMY5 | Winning control | 14 | 24 | 12 | -12 |

Three of the five weak seeds improved against v15, one tied and one worsened.
Their mean rose from **4.8 to 9.2**, but remained below v13's **12.0**. Most of
the aggregate gain came from recovering the recent `2RGEX6EN` regression; its
17 cleared blinds only matched v13. The persistent bottom two together fell
from 5.0 to 3.5. The controls fell from 22.0 to 16.0, with the former winning
seed losing half its progress. Across all seven, the mean was 11.14 versus
v15's 9.71 and v13's 13.29.

The recorded mechanisms are mixed. `2RGEX6EN` took Riff-raff and gained an early
joker lineup, surviving to ante 6. `DHPB8Q1L` took Burglar, but its seven hands
with no discards scored only 486/600 against The Goad. `75WVKF42` again failed
The Water despite no longer buying unsupported Baseball Card. Better item
information alone did not solve its hand planning.

**Do not promote this as a general upgrade.** This is one run per seed on a
selected development subset, with stochastic JEV decisions and no fresh v15
replicate. The bundled changes do not isolate the cause of each outcome. The
paired standard error for the seven-seed difference against v15 is 3.17 blinds,
larger than the observed +1.43 difference. More importantly for the stated goal,
the persistently worst pair did not improve and a winning control regressed.

Completion was recorded as event 2504, `Benchmark weak7-a complete`, with the
panel paused, no current run and no queued seeds. Source remained `503b35ff`;
its exact decision files are also preserved in `runs/_sources/503b35ff/`.
V16 remains the experimental working source, with the panel paused. No full
bench30-a evaluation has been started for v16. Run identifiers, paired
statistics and completion evidence are in the
[result data](samples/v16-outlier-results.json).

## Follow-up diagnosis while the source remained frozen

The first two pilot losses were again states with no discards: The Water on
`75WVKF42`, and Burglar removing discards on `DHPB8Q1L`. The current candidate
ranking measures the immediate play and generally pads it with low cards; it
does not measure the next hand after a play. A separate read-only probe compared
existing plays with plays that retain a flush/straight draw, using 128 uniform
unordered draws and the existing scorer. It used no actual future draw order.

For `75WVKF42` event 169, keeping four Hearts and playing four off-suit cards
scores only 15 immediately, but its modeled next play averages 841. The chosen
Pair scores 36 now and its next-play average is 302. Their two-hand averages are
856 versus 338; the modeled 20th-percentile totals are 609 versus 96. This is a
concrete candidate-generation/planning gap, not proof that another action wins.
The probe freezes joker state and does not simulate a complete round, so it must
not be used as an exact policy without additional validation. All candidate
comparisons are saved in [no-discard diagnosis](samples/v16-no-discard-diagnosis.json).

Taking Burglar is not automatically beneficial: its extra hands do not compensate
for losing all discards if the hand policy cannot exploit them. Treat that pilot
regression as evidence against indiscriminately relaxing free-joker selection.
