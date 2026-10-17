# scoring handoff (branch port/scoring)

## Done
- native/bot/src/hands.rs written in full (NOT yet compiled; depends on jokers.rs which is not written):
  hands.py (card_info, describe, classify, estimate, evaluated_plays, best_score, best_play,
  play_candidates, discard_candidates), native.py glue as `hands::native` (calls jev_kernel directly;
  `native::set_enabled(false)` = JEV_NATIVE=0 / the Python path), and decisions.py 200-231
  (_boss_levels, _boss_allows, BossAllow) -- the decisions agent imports these from hands.
- Design: `ScorerArg::{None, Jokers(&jokers::Scorer), Func(&dyn Fn)}`, `Allowed::{None, Boss(&BossAllow), Func}`.
  Kernel is used only for Jokers scorer + None/Boss allowed + encodable cards (unknown suit string, e.g. "X",
  is the unencodable case to put in fixtures); else the Python loop.
  Callers: decisions/readiness/odds/rollout use Jokers; round_planner (not ported) uses Func.

## Done (task jokers-core)
- native/bot/src/jokers.rs: jokers.py 1-221 + 286-392 (parse, entry, build, unmodelled, score, _hand, Scorer, Rnd, Pred/Cond/
  HandSpec/Rule/JokerEntry with `to_json()` for fixtures). Crate builds (`cargo build --release`), hands.rs needed no edits.
- scripts/port/fixtures/jokers_parse.py + native/bot/tests/jokers_parse.rs: 2085/2085 identical (parse 322, entry 323, build 1440).
  Python effect tuples hold ints and floats; the fixture encodes them all as floats (only added/multiplied into float totals).
  Sets are sorted lists. No lambdas in parse() output.
- `Scorer::new(Levels, Vec<JokerEntry>, Rnd)`, `Rnd::from_value(&round_dict)`, `build(gs, Option<&[Value]>)`.

## Left
1. jokers.rs `// TODO(jokers-rest)`: jokers.py 190-285 (play_forecast_supported, PLAY_STABLE/PLAY_UTILITY, after_discard,
   after_play) and 395-441 (sample_plays, gain, planet_level).
2. (compile of hands.rs: done)
3. scripts/port/fixtures/scoring.py + native/bot/tests/scoring.rs (one test per function; also Python-path rows with
   native disabled and unknown-suit rows). Not started; no results yet (0 tests).
Notes: scores are i64 (Python int; >9e18 not handled); `gain` ratio uses f64 division of i64 sums.
