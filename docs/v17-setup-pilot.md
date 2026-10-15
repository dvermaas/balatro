# V17: preserve a useful next hand when discards run out

The user approved one more focused change and the same seven-seed pilot before
deciding whether to spend a full bench30-a evaluation on it. V16 improved three
weak seeds but failed the persistent worst pair and regressed a winning control.
See [v16 report](v16-outlier-pilot.md).

## Implementation

V17 **11845ce8** retains v16's item descriptions and basic-support guidance but
restores the original **1.5/4** free-joker selection threshold. It adds setup
plays only when no discards remain, at least two hands remain, the deck is not
empty, and the boss/joker transitions are supported.

- Keep every original play candidate. Add distinct legal plays that retain
  flush, straight, set or high-card draws from the existing draw-plan generator.
- Score the immediate play and forecast the best next play on 32 reproducible
  unordered draws (enumerate exactly when there are at most 32 possibilities).
  Include misses and held-card effects. No actual future draw order is used.
- Advance hands remaining, hand-type usage/history and Blue Joker's remaining
  deck count. Account for The Eye, The Mouth, The Psychic and The Serpent.
- Give JEV both immediate and combined two-play scores, the 20th percentile,
  modeled two-play clear frequency, retained cards and numeric average ranking.
  JEV still selects the action; an existing sure clear bypasses this forecast.
- Fall back to unchanged play candidates and the original play question for
  unsupported bosses, unknown/scaling/destructive/random/copy jokers, hidden
  cards, or large hands. This explicitly avoids pretending to simulate their
  transitions. The implemented whitelist is in `jokers.py`; it is deliberately
  narrower than the ordinary scorer's coverage.

These are approximate two-play scoring forecasts, not complete-round win
probabilities. They assume the best modeled next action and do not value shops,
future rounds or every card effect. No seed-specific rules were added.

## Validation before dispatch

**51 offline tests passed**, including the recorded Water failure, unchanged
input snapshots, shuffled-pile invariance, original candidate preservation,
sure-clear bypass, spent-hand effects (Acrobat), hand history (Supernova,
Eye/Mouth), Psychic legality, Serpent's three draws, Blue Joker deck shrinkage,
unsupported/hidden-state fallback and the restored pack threshold.

The regression fixture strips deck card IDs and sorts the deck by observable
properties, discarding the actual draw order:
[fixture](samples/v17-setup-state.json).

The [offline audit](samples/v17-setup-audit.json) reviewed all 25 recorded v16
no-discard states with at least two hands remaining. Twelve gained setup
choices; median processing time was 0.392 s, maximum 0.822 s. The audit changes
no game state and makes no JEV calls. Run it with
`python scripts/audit_setup_plays.py --out <report.json>`.

Three real JEV replays changed choices. In both Water failures it selected the
new play retaining four Hearts. The Burglar replay chose a different Pair with
different padding, rather than the highest modeled draw setup. These are
decision smoke checks, not counterfactual wins. See
[replay results](samples/v17-setup-replay.jsonl). The replay helper does not
reconstruct recent play history, so build advice can differ from the original
recording. The forecast uses hand history in the recorded game state.

## Pilot protocol and advancement decision

Run **weak7-a** once in its existing fixed order: `75WVKF42`, `DHPB8Q1L`,
`PX8GK4E6`, `JV8CL6BQ`, `2RGEX6EN`, `2CM51JLD`, `BAPQJMY5`. Red Deck, White
Stake, `jev-latest`, 0.3 s panel delay, audio/shaders enabled. Freeze source
through completion, preserve all recordings and disable automatic random runs.

Decide whether to run a full bench30-a before seeing these pilot results:

1. The persistent bottom pair must exceed their v15 mean of 5 cleared blinds,
   with neither seed below its v15 result of 5.
2. The five weak seeds must at least match v16's 9.2 mean cleared blinds.
3. Neither control may drop more than 3 cleared blinds below its v15 result
   (20 and 24). Any execution or terminal-recording failure blocks advancement.

This is an exploratory screening rule, not a significance test. The subset was
selected using past results; one stochastic run per seed cannot isolate causes.
Compare every seed against v15, v16 and v13, including regressions. If the gate
passes, run the unchanged full bench30-a set once. If it fails, report the
evidence and leave the panel paused rather than starting another version.

## Pilot results

All seven games finished with **one verified win and zero execution errors**.
These are cleared blinds (24 means the final boss was beaten):

| Seed | v13 | v15 | v16 | v17 |
| --- | ---: | ---: | ---: | ---: |
| 75WVKF42 | 4 | 5 | 5 | 5 |
| DHPB8Q1L | 6 | 5 | 2 | 23 |
| PX8GK4E6 | 13 | 5 | 8 | 5 |
| JV8CL6BQ | 20 | 7 | 14 | 14 |
| 2RGEX6EN | 17 | 2 | 17 | 2 |
| 2CM51JLD | 19 | 20 | 20 | 20 |
| BAPQJMY5 | 14 | 24 | 12 | 24 |

The weak-five mean was **9.8**, versus v15's 4.8, v16's 9.2 and v13's 12.0.
The two controls both matched v15, recovering v16's lost win. All seven averaged
13.29 versus 9.71 in v15 and 11.14 in v16; this only matched v13's seven-seed
mean. Against v15, two seeds improved and five tied. Against v16, two improved,
three tied and two worsened. The +3.57 mean paired gain versus v15 has a 2.60
blind standard error; this small selected sample is not proof of a general gain.

The persistent bottom pair averaged 14.0 versus 5.0 in v15. This result is driven
by `DHPB8Q1L`, which took Burglar again, saw 20 decisions with forecasts and
selected ten setup plays. It reached Crimson Heart and scored 94,925/100,000.
`75WVKF42` selected the four-Heart setup, made a 990-point flush on its next
play, but still lost The Water at 1,271/1,600. The intended planning behavior is
now available and used, but it has not solved that persistent failure.

Restoring the pack threshold also restored a bad skip on `2RGEX6EN`: event 1542
rated Riff-raff 1.08/4, below 1.5, and skipped the paid pack. It again died with
two blinds cleared. V17 therefore loses a meaningful v16 gain; this should
remain visible when considering a later targeted pack-selection change.

`BAPQJMY5` beat Verdant Leaf at 109,650/100,000 (round-end event 3133), verified
from scored-round and public terminal-state evidence. The panel emitted
`Benchmark weak7-a complete` at event 3137, paused with no current run or queued
seeds, and the game returned to MENU. See
[paired results and win audit](samples/v17-outlier-results.json) and
[completion proof](samples/v17-pilot-completion.json).

All predeclared advancement checks passed. Game and server were restarted after
the user's closure, with the tested source frozen at `11845ce8`. The full
**bench30-a** evaluation is now authorized by the agreed gate; run it once with
the same source and settings. Keep its fresh repeats of the seven pilot seeds
separate from the pilot rather than selecting the better result.

## Full benchmark results

**Complete: 30/30 fixed seeds, zero wins, zero execution errors. V17 did not improve on v15 in this evaluation.** The initial
evaluation used the existing automatic stopping rules: stop after at least 10 paired seeds if more than two paired
standard errors behind the best completed version, or after at least 15 if more
than one standard error behind. A rule-triggered stop is a result to report,
not a reason to restart or alter the experiment automatically.

### User-requested completion after the usage reset

The user subsequently requested resuming the benchmark and evaluating it only
after the whole set, without individual-seed monitoring. Both game and server
were offline. Exactly 21 terminal summaries were preserved, with no unfinished
run to recover. The existing futility check at that point would stop the queue:
v17 trails v15 by 0.95 +/- 0.94 blinds over 21 paired seeds.

For this explicit request to finish the set, the resume launcher gained an
opt-in `--finish-set` switch. It disables performance early stopping for the
remaining queue only, without changing the nine hashed decision files. Resume
at game 22, `8GWNFCSN`, retaining all 21 results and queuing the other nine once.
This is a disclosed change to the evaluation stopping policy after intermediate
results were known, not a fresh benchmark or a new decision version. Per-seed
monitoring and updates are discontinued at the user's request.

Startup was verified: running v17 `11845ce8`, game 22 on `8GWNFCSN`, eight later
seeds queued, with the preceding 21 terminal results retained. See
[resumption evidence](samples/v17-benchmark-resumption.json). The panel is set
to pause after the remaining fixed seeds; no per-seed monitor was restarted.


### Completed comparison

| Version | Mean cleared blinds | Wins |
| --- | ---: | ---: |
| v9 | 12.77 | 0/30 |
| v11 (audited) | 11.97 | 0/30 |
| v12 | 13.90 | 0/30 |
| v13 | 14.47 | 0/30 |
| v15 | 14.90 | 2/30 |
| v17 | 14.33 | 0/30 |

Against v15, v17 improved eight seeds, tied fourteen and worsened eight. The
paired mean difference is **-0.57 blinds, standard error 0.71**. That does not
establish that v17 is inherently worse, but provides no support for promoting
it as an improvement. Against v13 the difference is -0.13, SE 0.80. Excluding
v15's interrupted game 17 (`X62IZYJ4`) leaves -0.24 versus v15, SE 0.66; the
conclusion remains the same. This is an existing development benchmark with
one historical baseline run per seed, not an independent held-out evaluation.

The targeted weak five averaged **6.8 versus v15's 4.8**. `DHPB8Q1L` improved
5 -> 8 and `JV8CL6BQ` 7 -> 14; `75WVKF42` stayed at 5, `PX8GK4E6` stayed at 5,
and `2RGEX6EN` stayed at 2. Thus two weak seeds improved, but the persistent
lowest failures were not broadly solved. The two v15 wins, `BAPQJMY5` and
`X62IZYJ4`, both fell from 24 to 14; `W4YA1M4U` fell from 23 to 14.

The pilot overstated repeatability. Its 23-blind `DHPB8Q1L` result became 8 in
the fresh full-set run; its `BAPQJMY5` victory became 14. Five of the seven
pilot seeds repeated their cleared-blind result, but the two exceptional
results did not. For BAPQJMY5's first move, the recorded state and questions
were identical yet JEV chose discard in the pilot and play in the full run.
See [identical-input decision variation](samples/v17-repeat-decision.json).
Both DHPB8Q1L runs took Burglar and selected ten setup plays; the fresh run
lost The Wall at 5,008/8,000. See [repeat evidence](samples/v17-dh-repeat.json).

The planning feature was exercised in 11 of 30 games, on
44 decisions, with 20 setup choices. It demonstrably
exposes useful options and changes actions; that implementation evidence is
separate from a repeatable performance gain. The nine decision files still
match the frozen `11845ce8` source. No benchmark recording was overwritten or
selected for being the better of two attempts.

**Recommendation:** retain v15 as the performance reference. Do not promote
v17 on this result. Isolate one proposed improvement at a time and require
repeated weak-seed/control runs before another full evaluation. Early joker
valuation remains a concrete investigation target: the Riff-raff skip on
`2RGEX6EN` still recreates a known early failure. Avoid concluding from the
pilot alone that a planning fix caused a large survival gain.

Full paired results, run identifiers, sensitivity comparison and feature usage:
[result data](samples/v17-benchmark-results.json). Completion proof:
[panel completion](samples/v17-benchmark-completion.json).

| Seed | v15 | v17 | Change |
| --- | ---: | ---: | ---: |
| 7X7WSDCX | 14 | 14 | +0 |
| 6CAXN9I6 | 14 | 17 | +3 |
| BAPQJMY5 | 24 | 14 | -10 |
| TBIR2XMU | 14 | 14 | +0 |
| PX8GK4E6 | 5 | 5 | +0 |
| WWYTV5PP | 17 | 17 | +0 |
| DHPB8Q1L | 5 | 8 | +3 |
| XUTRVN1Y | 11 | 17 | +6 |
| 2RGEX6EN | 2 | 2 | +0 |
| V98SL5G2 | 17 | 17 | +0 |
| WMAZ7HWJ | 20 | 20 | +0 |
| W4YA1M4U | 23 | 14 | -9 |
| U8AC5647 | 15 | 17 | +2 |
| 8XNG984V | 13 | 16 | +3 |
| SDNUURBN | 11 | 11 | +0 |
| A7YBUVWD | 20 | 17 | -3 |
| X62IZYJ4 | 24 | 14 | -10 |
| JRE681Y8 | 19 | 19 | +0 |
| 75WVKF42 | 5 | 5 | +0 |
| I9Q96AKF | 16 | 17 | +1 |
| LRFULSLB | 20 | 14 | -6 |
| 8GWNFCSN | 13 | 11 | -2 |
| MW6PBTW4 | 16 | 14 | -2 |
| 5V1F9XQ2 | 18 | 18 | +0 |
| 2S38V5NR | 17 | 18 | +1 |
| 2CM51JLD | 20 | 20 | +0 |
| UR3ZTT24 | 14 | 14 | +0 |
| C3XPB74B | 16 | 15 | -1 |
| JV8CL6BQ | 7 | 14 | +7 |
| 57AXFNDP | 17 | 17 | +0 |
