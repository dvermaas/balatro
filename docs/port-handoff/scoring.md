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

## Left
1. native/bot/src/jokers.rs: port jokers.py. hands.rs expects these items from `crate::jokers`:
   `Scorer` (new(levels, Vec<JokerEntry>, Rnd); `call(&[Info], combo, name, sc)->i64`; `native()->Option<&jev_kernel::Scorer>`
   cached in a OnceCell via hands::native::scorer), `Pred {FirstFace, First, Final, Suit(char), Ranks(Vec<char>), All}`,
   `HandSpec {Const((f64,f64,f64)), Cond{cond,yes,no}, TimesPlayed, PerDiscard(f64), LowestHeld}`,
   `Cond {Names(Vec<String>), Le3, Final, NoDiscards, HeldSuits(Vec<Option<char>>), FourSuits, PlayedThisRound}`,
   `Rule {card: Vec<(Pred,f64,f64,f64)>, retrigger: Vec<(Pred,i64)>, held: Vec<(Pred,f64,f64)>, held_retrigger: Option<i64>, hand: Option<HandSpec>}`,
   `JokerEntry {label: Option<String>, rule: Option<Rule>, edition: Option<String>, scoring: bool}`,
   `Rnd {hands_left: f64, discards_left: f64}` (from dict: `or 1` / `or 0`).
   Also parse, _card_pred, _parts, entry, build, unmodelled, after_discard, after_play (pycompat fixed/pyfloat: pending),
   play_forecast_supported, score, _hand, sample_plays, gain, planet_level. Needs HAND_NAMES / CONTAINED_IN
   (strategy.py 11-23) as private copies with `// SHARED?`.
2. Compile (`cargo build` in native/bot), fix errors in hands.rs.
3. scripts/port/fixtures/scoring.py + native/bot/tests/scoring.rs (one test per function; also Python-path rows with
   native disabled and unknown-suit rows). Not started; no results yet (0 tests).
Notes: scores are i64 (Python int; >9e18 not handled); `gain` ratio uses f64 division of i64 sums.
