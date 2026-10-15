# Mechanics and decision diagnostics — first milestone

2026-10-15. This implements the first part of the methodology audit. Production
remains byte-identical to v18 `d22adb59`. JEV remains the decision model. There
has been no new full benchmark, full-game simulator adoption, or win-rate claim.

## Result and recommendation

Jackdaw is useful now as an **independent scoring reference**, with an explicit
adapter and coverage limits. It matches all 27 reconstructable deterministic
plays in our frozen bank, including two previously identified large errors.
An isolated correction to Raised Fist and Pareidolia matches those 27 actual
game outcomes and all 5,582 supported alternative subsets checked against
Jackdaw. The current scorer matches 24 of the same 27 actual outcomes.

The correction also changes useful choices in controlled JEV probes. This is
evidence of a mechanics/candidate problem, not merely a need for more coaching
in the prompt. It is **not** evidence that these two fixes will materially raise
the full-run win rate: they address relatively specific interactions, and we
have not measured their continuation outcomes yet.

Keep the next candidate narrow: the two verified mechanics corrections first.
Use Jackdaw to expand validation coverage and test a remaining-round planner
separately. A full Rust port is not justified by the current measurements.

## What is frozen

[40 diagnostic positions](samples/diagnostic-positions-v1.json), from 25 completed
v17 development runs:

| Stratum | Positions | Selection |
| --- | ---: | --- |
| Known scoring failures | 2 | Explicitly selected regressions |
| Early plays, ante 1–2 | 8 | Stable hash order; one position per run in this stratum |
| Later boss plays | 10 | Same hash rule |
| Other later plays | 10 | Same hash rule |
| Shops | 10 | Same hash rule |

Selection uses `SHA256(diagnostic-v1:run:event)` and the fixed quotas, not the
recorded outcome. This is still a deliberately stratified **development bank**,
not a representative random sample or a held-out evaluation. The known failures
must not be counted as an unbiased estimate of error frequency.

The public observation is constructed with an allowlist. It includes visible
hand order, joker order, descriptions, hand levels, round counters and current
shop/blind information. The extractor does not access the ordered draw pile.
The reference receives no real run seed, RNG state, future shops or save data.
Historical outcomes are labels used for validation, never inputs to scoring.

## Jackdaw evaluation

Reference: [TylerFlar/jackdaw-balatro](https://github.com/TylerFlar/jackdaw-balatro),
pinned at `e66de78855df84755d3af7d9a016129f3449f878`. Its MIT license remains in
the isolated checkout under `runs/_research/jackdaw/`. No Jackdaw code or extra
dependencies were installed into the production player. Core scoring imports
without the training stack. Upstream pytest was installed in a separate research
environment under `runs/_research/python/`.

- Selected upstream hand-evaluation/scoring suites: **192 passed, 2 skipped**.
  The two skipped tests require an additional live Lua oracle. These tests alone
  do not validate compatibility with our installation.
- Our actual recorded game outcomes: **27/27 exact matches** within the adapter's
  supported deterministic subset.
- Three of the 30 plays explicitly excluded: D03 requires The Hook's pre-score
  random discard transition; D14 and D18 score Lucky cards. No successful random
  draw was selected to manufacture agreement.
- The scorer reconstructs fresh mutable engine objects on every evaluation.
  Repeated evaluations do not accumulate hand counters or decay jokers.
- Dynamic values such as Ice Cream, Ramen, Fortune Teller and Ancient Joker are
  restored from public effect text; absent/unsupported state fails explicitly.

This validates a bounded **one-play adapter**, not Jackdaw's full deck generation,
shops, save compatibility, all 150 jokers, or complete run transitions. Unknown
jokers and certain pre-score bosses remain excluded. Random immediate scoring
needs distributional tests; deterministic Glass scoring does not validate its
later destruction outcome.

## What the scoring errors actually change

The installed patched game's `Card:is_face`, Photograph and Raised Fist code was
also inspected. Raised Fist is an individual held-card effect: the **last**
lowest-ranked non-Stone card triggers it, a debuffed selected card blocks it,
and held-card ordering/retriggers matter. Pareidolia changes face-card predicates,
including Photograph and Sock and Buskin; testing only printed ranks misses it.

| Case | Old estimate for recorded play | Actual score | Corrected estimate | Decision consequence |
| --- | ---: | ---: | ---: | --- |
| D01, WMAZ7HWJ, event 8112 | 2,730 | 630 | 630 | A different subset worth 2,730 under the reference was omitted from the old shortlist |
| D02, JV8CL6BQ, event 4157 | 352 | 1,540 | 1,540 | Pareidolia interaction was silently absent; this correction alone keeps the same strongest fixed-order subset |
| D12, U8AC5647, event 9135 | 16,065 | 9,639 | 9,639 | Corrected ranking favors a different offered play worth 15,660 under the reference |

Alternative scores in the last column are **reference predictions**, not observed
alternative game outcomes. D01 particularly distinguishes missing candidates
from model selection: JEV cannot choose a play it was never offered.

[Full comparison](samples/diagnostic-mechanics-v1.json) separates current scoring,
reference reranking of the existing shortlist, all 1–5 card subsets, and ordering
the best fixed-order subset. There are 26 positions with complete deterministic
subset comparisons; D02 has unsupported Lucky alternatives. On those 26:

- Reference reranking of the existing shortlist gains more than 10% in D12.
- Expanding the shortlist gains more than 10% in D01.
- Ordering the best subset gains more than 10% in none. This limited ordering
  test does not prove that order is unimportant elsewhere.

The whole comparison, including both scorers and extra permutations, took about
3.6 seconds locally. This is adequate for offline diagnosis. It is not a
performance measurement of full-round search, which expands many more states.

## Controlled JEV probes

[Protocol frozen before API calls](samples/diagnostic-selection-plan-v1.json).
Requested and resolved model: **`jev-1.13.0`**, TypeSafe SDK 0.7.4. All 88 calls
completed without errors; every response is retained in
[the request-result ledger](samples/diagnostic-selection-v1.jsonl).

Six tactical positions (D01/D12 and four controls), each with baseline versus
corrected facts/candidates, original versus reversed option insertion order,
and two repeats per order: **24 paired comparisons / 48 calls**. Instructions
are unchanged. These are bounded immediate best-play questions, without the
full play/discard policy or its two-play forecast. They are not exact replays
of the entire original request. Option IDs retain their names, so this tests
insertion-order sensitivity rather than fully counterbalancing label effects.

| Diagnostic | Baseline facts/candidates | Corrected facts/candidates |
| --- | ---: | ---: |
| Selects best offered immediate reference score | 20/24 | 24/24 |
| D01 selected reference score, all four repeats | 630 | 2,730 |
| D12 selected reference score, all four repeats | 9,639 | 15,660 |

Paired: **8 improved immediate scores, 16 tied, 0 worse**. The same JEV model
and instructions respond appropriately when the facts and available options
are corrected. The four repeats do not constitute four independent seeds.
The cases were selected after the scoring diagnosis; these are mechanism checks,
not evidence of generalization or win-rate improvement.

The remaining 40 calls repeat the ten original shop requests with both question
orders and two repeats. Eight shops retain the same highest-rated item set;
two vary among close alternatives. The largest within-item range is 0.25 on
the 0–4 rating scale. This checks stability only. Scores are not purchase actions
(code still applies thresholds), and stable choices can consistently be poor.
We have **not** established whether shop policy or round planning is the larger
survival bottleneck. [Detailed probe results](samples/diagnostic-selection-results-v1.json).

## Reviewable candidate and checks

[Exact proposed scorer diff](samples/mechanics-candidate.patch). It is deliberately
not applied to `src/` and does not register a new version during the unfinished
v18 benchmark. `scripts/diagnostics/mechanics_candidate.py` constructs the isolated
candidate, fails if its v18 baseline changes, and writes the review artifact.

Tests cover debuffed lowest held cards, rank versus chip value, equal-rank ties,
Stone exclusions, empty held hands, Steel ordering, Mime/Red Seal retriggers,
Pareidolia face effects and retriggers, debuffed Pareidolia, input immutability,
private-data exclusion, invalid actions, unknown mechanics and recorded scores.
**76 project tests pass**, including 16 new diagnostic regressions. The candidate
matches the reference on **5,582/5,582** supported subsets across this bank.
Existing unsupported mechanics outside these rules have not been fixed.

Reproduce from the repository root:

```powershell
# The pinned checkout is an optional research dependency, outside production.
git clone https://github.com/TylerFlar/jackdaw-balatro runs/_research/jackdaw
git -C runs/_research/jackdaw checkout e66de78855df84755d3af7d9a016129f3449f878
.venv/Scripts/python scripts/diagnostics/mechanics_candidate.py
.venv/Scripts/python scripts/diagnostics/compare_mechanics.py
.venv/Scripts/python -m unittest discover -s tests -q
.venv/Scripts/python scripts/diagnostics/probe_selection.py --summarize
```

Use the existing checkout if present. The position extractor intentionally refuses
to overwrite the frozen bank. `probe_selection.py` without flags validates the
frozen plan without API calls; `--execute` resumes only missing cells and never
replaces a completed or failed attempt silently. Do not rerun cells until a
favorable response appears.

## Benchmark status and next experimental gate

During this work, v18 was found paused after a JEV HTTP 499 error with 12 completed
games and game 13 incomplete. The game process had closed. Its opaque autosave
was backed up, the game reopened, and that save loaded for a public-state check.
It is at the next blind, ahead of the last logged pack action. No game decision
was issued and no benchmark seed was restarted. The user explicitly requested
**leaving the benchmark paused** when presented with that discrepancy. It remains
paused; no full-v18 comparison is reported as complete.

The following live work is therefore deferred, not claimed as done:

1. Resolve the interrupted v18 cell transparently before resuming its remaining
   seeds. Preserve the partial log and autosave; do not silently merge unlogged
   actions into a supposedly uninterrupted result.
2. Test the isolated mechanics candidate in paired saved-state continuations,
   with identical pinned JEV version, fixed horizon and all repeats retained.
3. Add a separate shop intervention arm and a separate remaining-round-planning
   arm, keeping the other component frozen. A tactical score improvement alone
   cannot rank those approaches by survival value.
4. Only after choosing and freezing an implementation, use a fresh evaluation
   panel and a contemporaneous baseline. Treat bench30-a and this bank as
   development data. Primary outcome: ante-8 clears; secondary outcomes: early
   losses, blinds cleared, cost and latency, with seed-paired uncertainty.

The next technical investment is a public-information transition/search harness
validated one mechanic at a time against Jackdaw and the game. Search should
sample independent hidden futures from observable information. Actual game seeds,
saved RNG and ordered draw piles must never be available to the policy. Profile
that prototype before deciding whether any small scoring/search core needs Rust.
