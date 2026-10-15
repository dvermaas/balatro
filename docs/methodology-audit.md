# Methodology audit: why progress is slowing, and what to change

2026-10-15. Requested while v18 `d22adb59` was running `bench30-a`.
The current benchmark was left untouched. This audit made no game actions,
model requests, strategy changes, server restarts or draw-order inspections.
It examined completed v15/v17 public decision logs, source, earlier experiments,
and primary documentation. V18's first few games are not a finished result.

## Assessment

We have not established that JEV has reached its capability limit, nor that
Python is the principal constraint. We have established a methodological gap:
we can often show that a patch changes a decision, but usually cannot identify
which component caused a loss or demonstrate that the changed decision improves
survival reliably. Small selected pilots followed by the same development
benchmark are too coarse to answer those questions.

The v18 work is useful evidence of this distinction. It demonstrated cash
transactions and taking Riff-raff, but its six paired checkpoint repetitions
produced one improved outcome and five ties, capped at ante 4. Those checks
justified an experiment, not an expectation of a large win-rate increase. My
recommendation to benchmark it should be understood at that limited strength.

We should change how we diagnose and validate changes before producing another
broad strategy version. Keep JEV as the production decision model. Treat
alternative policies as diagnostic controls rather than silently replacing it.

## What the fresh audit measured

`scripts/audit_methodology.py` reads `decisions.jsonl` and terminal summaries
only. It does not load `states.jsonl.gz`, the game, or the JEV API. Score
predictions are matched with the subsequent recorded play outcome, including
forced clear predictions. Results: [machine-readable audit](samples/methodology-audit.json).
The manually inspected examples and duplicate checks are retained separately
in [case notes](samples/methodology-case-notes.json).

| Diagnostic | V15 | V17 |
| --- | ---: | ---: |
| Complete bench30-a games | 30 | 30 |
| JEV decisions | 3,091 | 2,932 |
| Plays with matched prediction and actual score | 923 | 902 |
| Exact score matches | 814 | 812 |
| Score predictions within 10% of actual | 853 (92.4%) | 840 (93.1%) |
| Score errors larger than 25% of actual | 27 | 31 |
| JEV-chosen plays giving up >10% of best immediate modeled score | 0 | 15 |
| Such choices without a two-play forecast | 0 | 0 |
| Code chose discard with JEV play-now probability at least 0.5 | 125 / 1,140 | 81 / 1,041 |

These diagnostics do not establish optimal play. The scorer can be wrong on
unplayed alternatives, and stochastic effects can cause prediction errors.
Choosing a weaker immediate play can be correct setup. A threshold overriding
0.5 can be appropriate. Nevertheless, the evidence does **not** support the
simple account that JEV usually ignores a plainly stronger immediate hand.

All 6,023 recorded v15/v17 model responses identify `jev-1.13.0`; model drift
does not explain the difference between these two completed panels. Neither
has duplicate terminal benchmark cells.

### Large errors remain, including gaps that are not reported as unsupported

- **Pareidolia with Smiley Face and Photograph:** v17 JV8CL6BQ, event 4157,
  predicted 352 and scored 1,540. The public hand contains two nines and two
  threes. Applying the face-card effects gives 44 chips times 35 Mult, matching
  the actual outcome. The scorer tests printed face ranks without applying
  Pareidolia. The request has no `jokers_not_in_estimates` warning.
- **Raised Fist under The Pillar:** WMAZ7HWJ, event 8112, predicted 2,730 and
  scored 630. The selected play leaves only debuffed cards held; the current
  Raised Fist rule adds rank-based Mult without testing that debuff. Confirm
  the exact mechanics against the game before implementing a correction.
- **One claimed forced clear did not immediately clear:** A7YBUVWD event 10467
  predicted 11,583; the actual play scored 9,993 and remained at hand selection.
  This is not itself a lost run, but shows why an approximate score plus a 10%
  margin is not a mathematical guarantee.

The existing unsupported detector only flags unparsed text containing certain
scoring words. Effects classified as utility can change other scoring rules,
so absence of a warning does not prove coverage. Mechanics tests should report
known coverage explicitly, including interactions and event timing.

## Where the architecture is actually limited

### Candidate generation is already partly exhaustive

`hands.evaluated_plays` scores every subset of one to five cards before pruning.
For an eight-card hand that is **218 subsets**. It then retains at most eight
play candidates, with at most two per hand type. It does not enumerate scoring
order permutations. `discard_candidates` offers a handful of flush/set/straight/
high-card plans plus a preservation plan, rather than every legal discard.

This can omit a useful action before JEV gets a choice. A capable selector
cannot recover an absent candidate. Count candidate omission separately from
selection error; expanding candidates without good ranking can also hurt.

### Forecasts optimize proxies over short horizons

`odds.forecast` samples 32 unordered draws, except when exact enumeration is
smaller. It measures the best immediate next play after one discard. The v17
setup extension sometimes adds a two-play forecast, only for supported effects
and when discards are exhausted. Neither is a full remaining-round policy.

We need to distinguish expected chips, chance of clearing the round, retained
resources and future growth. A high mean from rare large scores can be worse
for survival. At probability 0.5, a 32-sample frequency has about 8.8 percentage
points of standard error before accounting for model errors. Reusing a fixed
sampling seed improves reproducibility, not the accuracy of every comparison.

### Shop value is a particularly weak approximation

`jokers.gain` measures an item on old chosen hands, keeping their old actions
and round counters. It does not re-optimize play for the acquired joker. An
item that enables a different hand or play policy can therefore look weak.
The shop scales existing readiness by that historical gain instead of
simulating the changed deck and resources.

`readiness.power` assumes two hands left and one discard; `assess` normally
assumes four hands per round. This misses important resource-changing jokers
and boss conditions. Boss names appear in the text, but the numerical forecast
does not simulate each boss's rules. More precise-looking numbers can be worse
than admitting uncertainty when their assumptions are wrong.

Finally, item ratings are converted through buy/pack thresholds and special
rules. JEV does not directly choose every final action. The empty-slot Riff-raff
skip involved a 1.12/4 rating and a 1.5 cutoff; the missing value estimate, rubric
and cutoff all matter. We should not attribute their combined behavior solely
to the model.

## Change the experimental method

### 1. Create an independent mechanics reference suite

Use the real game as the reference after v18 finishes. Begin with deterministic
recorded mismatches and common boss/joker interactions, plus ordinary controls.
Include card order, held-card debuffs, copied effects, hand classification,
resource changes and after-play/discard transitions. For random effects, test
the distribution or valid bounds rather than demanding one expected score.

Required gate: deterministic covered cases match the game; unsupported cases
are explicit. Do not describe a speculative estimate as a guaranteed clear.
Keep unit tests, but tests of our own formulas alone are not independent proof
that the formulas reproduce Balatro.

### 2. Build a fixed decision diagnosis set

Start with roughly 40 positions spanning early survival, shopping, boss
adaptation, scaling and strong-run controls, selected across multiple seeds.
For each position retain public inputs, candidate actions, forecasts, model
answers and the final code-selected action. Label failure candidates as:

1. Wrong or incomplete mechanics/information.
2. Useful action missing from the candidates.
3. Bad numerical objective or insufficient lookahead.
4. Model selects poorly from accurate, adequate choices.
5. Code overrides or execution changes a reasonable model answer.

These are hypotheses until verified, not labels assigned from the fact that
the run eventually lost. Examine shops and resource use before the losing
blind, not just its final hand.

For selection tests, repeat exact requests and shuffle option order. Measure
action flip rates and rank/score changes. Never interpret a JEV Choice
probability or Score/4 as the probability of winning a round or game.

### 3. Run controlled continuations that isolate the bottleneck

At an identical checkpoint, change one decision or one component, then return
to the frozen policy. Start with several paired repetitions per position,
include all attempts, and predeclare the horizon. A short horizon is a tactical
test; purchases and scaling need a longer continuation, sometimes the full run.
Never stop retrying simply because a favorable outcome appeared.

| Diagnostic comparison | What it can establish |
| --- | --- |
| Current forecasts vs corrected mechanics, same decision policy | Whether wrong facts are changing useful actions |
| Existing candidates vs broader candidates, same ranking | Whether pruning hides materially stronger alternatives |
| JEV vs a numerical selector on the same validated candidates | Whether model selection adds or loses tactical value |
| Current shop choices vs a few reviewed shop interventions, same hand player | Whether purchasing/build construction is the main bottleneck |
| Current short forecast vs a remaining-round planner, same shop policy | Whether longer tactical planning improves survival |

The model-free selector is an experimental control. It has not been installed
as the production player. Expert or stronger-model advice can help propose
counterfactuals, but opinions alone are not ground truth; test the continuations.

### 4. Separate development from evaluation

The repeatedly inspected `bench30-a` is a **development panel**. Keep its seeds
unchanged for continuity. Create a separate fresh evaluation panel after an
implementation is frozen, and do not tune against its individual failures.
If its results inform another patch, it becomes development data for that patch.

Run candidate and baseline contemporaneously with a pinned JEV version and
the same environment. Retain v15 as a historical reference and v18 as the direct
baseline for changes built on v18. Use multiple repetitions for stochastic
decision policies; report paired seed-level differences and uncertainty, not
best-of-repeats. Respect the seed grouping when estimating uncertainty.

Predeclare the primary metric: **clearing ante 8** if that is the objective.
Keep mean blinds cleared, early-loss frequency, lower-tail performance and cost
as secondary diagnostics. Improving the mean without more wins can still be
progress, but it must not substitute for the actual objective. Thirty games
are coarse for small changes in a low win rate; report wide uncertainty rather
than inventing a universal significance cutoff or rerunning until favorable.

Record complete experiment identity: decision source, runner/evaluation settings,
requested and resolved model, SDK, game/mod versions, profile/unlocks, seed set,
attempt/repeat identity and interruption history. The current nine-file hash
does not identify all of those. The dashboard currently keeps the latest terminal
attempt per seed; use a repeat-aware report instead for future repeated panels.

## Search and Rust

**Prototype better search before committing to a full rewrite.** There are only
218 immediate subsets in a normal eight-card hand; we already enumerate them.
But drawing five from 44 unseen cards has 1,086,008 possible subsets, before the
next action and subsequent draws. Full-game exhaustive search is not a practical
next step merely because a faster language is available.

A useful next planner would search until the current blind is won or lost,
using sampled hidden futures, state-dependent hands/discards and supported joker
transitions. It should return clear probability estimates with uncertainty and
coverage, plus resource consequences, for JEV's bounded decision. Search must
sample unknown futures from public information; never use the actual seed/RNG,
draw order or future shops as an oracle. The private evaluation engine and the
policy's observations need a strict separation, including in counterfactuals.

First measure whether this planner wins the isolated tactical comparison. Then
profile it. If scoring or transition expansion dominates runtime, port that
small, tested core to Rust (or another compiled implementation) and require
identical fixtures. If the gain is mostly from better shop decisions, a full
tactical-engine rewrite would have targeted the wrong problem.

Before writing a complete simulator, evaluate existing work. Jackdaw advertises
a Python simulator, a BalatroBot validation bridge and 250+ validation scenarios.
Those are author claims, not validation against our installed game. Use recorded
deterministic fixtures to assess compatibility before adoption.
[Jackdaw source](https://github.com/TylerFlar/jackdaw-balatro).

Search alone is no guarantee. Optilatro reports a roughly 5% result for one
heuristic policy over 300 seeds and explicitly separates future-information
oracle results from fair evaluation. Another project's best heuristic panels
report 3/20 ante-8 clears, while its coached wins have an explicitly unmeasured
unattended win rate. These are different systems and settings, not a ranking
against ours; their disclosures are more useful here than their headline wins.
[Optilatro](https://github.com/dylanbailes/Optilatro),
[balatro-ai results](https://github.com/Attol8/balatro-ai/blob/main/docs/results.md).

## What the JEV documentation changes about this plan

TypeSafe documents limitations in numeric reasoning, multi-step indirection,
irrelevant context and option-order sensitivity for JEV 1.13. That supports
using code for quantitative comparisons and testing narrowly framed judgments,
rather than continually expanding general strategy instructions.
[Official limitations](https://docs.typesafe.ai/model-jaggedness/jev-1.13).

Its confidence documentation describes confidence as a statistic of the answer
distribution. That is not a calibrated Balatro outcome predictor. Validate
game-specific relationships before using such values as survival estimates.
[Official confidence documentation](https://docs.typesafe.ai/confidence).

As checked today, the official model page lists `jev-1.14.0` as a new preview
release, while `jev-latest` still points at `jev-1.13.0`. A pinned-version
comparison on the fixed decision set is a sensible separate experiment; do not
mix a model change with a strategy change or switch the active benchmark.
[Official model versions](https://docs.typesafe.ai/models).

## Recommended next milestone

Let v18 finish unchanged. Then produce a **diagnosis and mechanics milestone**,
not another broad prompt version: verify the large scoring mismatches, build
the fixed decision set, and run the tactical-versus-shop controlled comparisons.
Choose the next implementation from the component that demonstrates useful
counterfactual improvement. Our present evidence makes shop valuation and
round-level planning leading hypotheses, not proven winners.

We do not have a matched expert baseline that supports an exact near-100%
human win-rate claim. That uncertainty does not justify treating our low win
rate as unavoidable luck. The right test is whether strong reviewed decisions
or validated planning can repeatedly rescue positions our frozen policy loses.
