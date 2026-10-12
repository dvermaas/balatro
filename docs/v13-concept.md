# v13 concept: readiness-driven shopping + plan memory

## Why

v5–v12 all plateau at ~12–13 blinds cleared on bench30-a, dying around ante 5 at ~50% of the
target. v12 (Codex: score-before-prune, Monte Carlo discard forecasts) fixed in-round play and is
tied with v9 so far (+0.25 ± 0.95 at 16/30). The losses are build strength, not hand play:
the shop doesn't know *how far behind* the next blinds it is, so it banks interest when it should
be buying power, and spends when it's already safe.

## Readiness (`src/readiness.py`, written, not wired)

- `power(gs)`: sample 24 random hands from the full deck (sorted by observable properties, never
  draw order), score the best play in each with `hands.best_score` + `jokers.score` using current
  jokers and hand levels. Returns median and 20th percentile. ~0.05 s.
- `upcoming(gs)`: next 3 blinds with targets: this ante's unbeaten blinds from `gs["blinds"]`,
  then the next ante's from Balatro's base amounts (×1 / ×1.5 / ×2).
- `assess(gs)`: per-blind `ratio = typical hand / (target / hands)` and `worst_ratio`.
- Calibration: a fresh starter deck scores 0.68 against the ante 1 boss (ignores discards and
  hand-play skill), so thresholds come from recorded runs, not 1.0. Step 1 of wiring is logging
  `assess()` at every shop and fitting `worst_ratio` against "died at the next boss".

## Shop modes (replace the fixed money rules)

| Mode | When (thresholds after calibration) | Policy |
|---|---|---|
| invest | worst_ratio below the danger line | spend down to ~$5 on the item with the biggest readiness gain; rerolls allowed from ~$10 |
| normal | in between | v9/v11 rules (reroll above $25, vouchers from surplus) |
| bank | well above the line | keep $25 interest; only strong vouchers and sure jokers |

Item gains are shown to JEV as readiness changes ("boss readiness 0.71 → 0.93") instead of raw
measured gain: same `jokers.gain` machinery, rerun `power()` with the item added.

## Plan memory (code-derived)

JEV only answers typed questions, so the "plan" lives in code and is restated each decision:
`priority: scoring power (boss readiness 0.7)` / `economy (ready, bank interest)` plus the build
(`build.describe`) and a short shopping list (e.g. "xMult joker", "level Flush"). Stored per seed
on the bot like `_recent`, updated each shop.

## Wiring (after v12 finishes and Codex commits)

1. Add `readiness.py` to `recorder.PROMPT_VERSION` hash list and CLAUDE.md/README.
2. `decisions.shop`: compute `assess()` once per shop visit, pick the mode, apply its money floor
   and reroll rule; add `readiness` + `plan` to the state JEV sees.
3. `prompts.py`: one line on what readiness means; VERSION_NOTE.
4. Benchmark on bench30-a against v9 and v12.
